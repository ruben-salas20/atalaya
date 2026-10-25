//! Actualización de la app de PC desde GitHub Releases (ADR 0012).
//!
//! Este archivo no sabe nada de la interfaz. Lee la última versión publicada,
//! compara versiones, baja el programa, comprueba su huella SHA-256 y lo pone
//! en lugar del actual. La pantalla lo llama desde `ui::updates`.

use std::fmt;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cache::Cache;

/// La última Release del repo público. `ATALAYA_RELEASES_API` la cambia para
/// probar con un servidor propio.
const DEFAULT_API: &str = "https://api.github.com/repos/ruben-salas20/atalaya/releases/latest";
const API_ENV: &str = "ATALAYA_RELEASES_API";
/// Nombre del programa de PC dentro de la Release.
pub const BINARY_NAME: &str = "atalaya-desktop-linux-x86_64";
const SUMS_NAME: &str = "SHA256SUMS";
/// Archivo provisional, en la misma carpeta que el programa actual.
const NEW_NAME: &str = ".atalaya-desktop.nuevo";
/// Archivo de prueba para ver si la carpeta se puede escribir.
const PROBE_NAME: &str = ".atalaya-desktop.prueba";
/// Clave de la caché donde se guarda la última comprobación.
const CACHE_KEY: &str = "actualizacion";
/// Como mucho una comprobación al abrir la app en este tiempo.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
const API_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const NOT_REPLACEABLE: &str =
    "Esta copia no se puede actualizar sola; usa scripts/instalar-pc.sh --release";

/// La versión con la que se compiló esta copia.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Una versión `X.Y.Z`. Se ordenan como números, así que 0.10.0 es mayor que 0.9.9.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// Lee `X.Y.Z`, con una `v` delante si la hay. Lo que sigue a un `-`
    /// (como `-rc1`) se quita. Cualquier otra cosa no es una versión.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let core = text.split('-').next()?;
        let mut parts = core.split('.');
        let version = Self {
            major: number(parts.next()?)?,
            minor: number(parts.next()?)?,
            patch: number(parts.next()?)?,
        };
        parts.next().is_none().then_some(version)
    }
}

/// Un número de solo dígitos. `u64::from_str` acepta un `+` delante, y aquí no.
fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// La versión instalada en esta copia.
pub fn current() -> Version {
    Version::parse(CURRENT).unwrap_or_default()
}

/// Lo que la app necesita de la última Release.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub version: Version,
    /// Las novedades, tal como las escribió Rubén en la Release.
    pub notes: String,
    pub binary_url: Option<String>,
    pub sums_url: Option<String>,
}

impl Release {
    /// Hay algo que instalar: una versión mayor que la instalada y con los
    /// dos archivos. Una Release sin archivos (los está subiendo Actions) no
    /// ofrece nada.
    pub fn offers_update(&self, installed: Version) -> bool {
        self.version > installed && self.binary_url.is_some() && self.sums_url.is_some()
    }
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    body: Option<String>,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// Lee el JSON de `GET /releases/latest` de GitHub.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let api: ApiRelease = serde_json::from_str(json)
        .map_err(|_| "La respuesta de GitHub no se entiende".to_owned())?;
    let version = Version::parse(&api.tag_name)
        .ok_or_else(|| format!("La etiqueta «{}» no es una versión X.Y.Z", api.tag_name))?;
    let url = |name: &str| {
        api.assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
    };
    Ok(Release {
        version,
        notes: api.body.unwrap_or_default(),
        binary_url: url(BINARY_NAME),
        sums_url: url(SUMS_NAME),
    })
}

/// La huella de `file` en un `SHA256SUMS`, en el formato de `sha256sum`:
/// `<hex>  <nombre>`, una línea por archivo. Devuelve el hex en minúsculas.
pub fn expected_digest(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hex, name) = line.trim().split_once(char::is_whitespace)?;
        // En modo binario, `sha256sum` escribe el nombre con un `*` delante.
        let name = name.trim_start().trim_start_matches('*');
        let hex_ok = hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit());
        (name == file && hex_ok).then(|| hex.to_ascii_lowercase())
    })
}

/// La huella SHA-256 de `bytes`, en hex y minúsculas.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Falla si la huella de `bytes` no es `expected`.
pub fn verify(bytes: &[u8], expected: &str) -> Result<(), String> {
    if sha256_hex(bytes) == expected.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(
            "La huella del programa descargado no coincide con SHA256SUMS; no se ha instalado nada"
                .to_owned(),
        )
    }
}

/// Segundos desde 1970, para guardar cuándo fue la última comprobación.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Toca mirar otra vez: nunca se miró, o hace `CHECK_EVERY` o más. Una hora
/// en el futuro (el reloj cambió) también cuenta como vieja.
pub fn check_due(checked_at: u64, now: u64) -> bool {
    checked_at > now || now - checked_at >= CHECK_EVERY.as_secs()
}

/// Lo que se guarda en la caché al comprobar: cuándo y qué se encontró.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub checked_at: u64,
    pub release: Release,
}

/// La última comprobación guardada, si la hay.
pub fn load_check() -> Option<Check> {
    Cache::open()?.load(CACHE_KEY)
}

pub fn save_check(check: &Check) {
    if let Some(cache) = Cache::open() {
        cache.store(CACHE_KEY, check);
    }
}

fn client(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        // GitHub rechaza las peticiones sin User-Agent.
        .user_agent(format!("atalaya-desktop/{CURRENT}"))
        .timeout(timeout)
        .build()
        .map_err(|e| format!("No se pudo preparar la conexión: {e}"))
}

fn get(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client.get(url).send().map_err(|e| network_error(&e))?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub respondió con el error {}",
            response.status()
        ));
    }
    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|e| network_error(&e))
}

fn network_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "GitHub tarda demasiado en responder".to_owned()
    } else {
        format!("No se pudo conectar con GitHub: {error}")
    }
}

/// Lee la última Release de la API, sin token.
pub fn fetch_latest() -> Result<Release, String> {
    let api = std::env::var(API_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_API.to_owned());
    let json = get(&client(API_TIMEOUT)?, api.trim())?;
    parse_release(&String::from_utf8_lossy(&json))
}

/// Baja la versión de `release`, comprueba su huella y la pone en lugar de
/// `exe`. Antes de bajar nada mira que `exe` se pueda cambiar.
pub fn install(release: &Release, exe: &Path) -> Result<(), String> {
    check_replaceable(exe)?;
    let (Some(binary_url), Some(sums_url)) = (&release.binary_url, &release.sums_url) else {
        return Err(format!(
            "La versión {} todavía no tiene el programa de PC",
            release.version
        ));
    };
    let client = client(DOWNLOAD_TIMEOUT)?;
    let sums = String::from_utf8_lossy(&get(&client, sums_url)?).into_owned();
    let expected = expected_digest(&sums, BINARY_NAME)
        .ok_or_else(|| "SHA256SUMS no trae la huella del programa de PC".to_owned())?;
    let bytes = get(&client, binary_url)?;
    verify(&bytes, &expected)?;
    replace(exe, &bytes)
}

/// Dice si se puede cambiar el programa `exe`. No se puede si está dentro de
/// una carpeta `target/` (es una compilación de desarrollo) o si su carpeta
/// no se puede escribir.
pub fn check_replaceable(exe: &Path) -> Result<(), String> {
    let dir = exe.parent().ok_or(NOT_REPLACEABLE)?;
    if dir.components().any(|part| part.as_os_str() == "target") {
        return Err(NOT_REPLACEABLE.to_owned());
    }
    let probe = dir.join(PROBE_NAME);
    let _ = fs::remove_file(&probe);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|_| NOT_REPLACEABLE.to_owned())?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

/// Escribe `bytes` junto a `exe` y lo pone en su lugar con un `rename`. Ese
/// cambio es atómico: el programa actual sigue hasta el último instante y no
/// queda ningún archivo a medias.
pub fn replace(exe: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = exe.parent().ok_or(NOT_REPLACEABLE)?;
    let tmp = dir.join(NEW_NAME);
    let result = write_executable(&tmp, bytes).and_then(|()| {
        fs::rename(&tmp, exe)
            .map_err(|e| format!("No se pudo poner la versión nueva en su sitio: {e}"))
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_executable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let write_error = |e: std::io::Error| format!("No se pudo escribir la actualización: {e}");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o755)
        .open(path)
        .map_err(write_error)?;
    file.write_all(bytes).map_err(write_error)?;
    file.sync_all().map_err(write_error)?;
    // `mode` solo cuenta al crear, y la máscara del usuario puede quitar bits.
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(write_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE_JSON: &str = r#"{
        "tag_name": "v0.2.1",
        "body": "Novedades\n- Arreglos",
        "assets": [
            {"name": "notas.txt", "browser_download_url": "https://x.dev/notas.txt"},
            {"name": "atalaya-desktop-linux-x86_64", "browser_download_url": "https://x.dev/bin"},
            {"name": "SHA256SUMS", "browser_download_url": "https://x.dev/sums"}
        ]
    }"#;

    #[test]
    fn compares_versions_as_numbers() {
        let v = |text| Version::parse(text).unwrap();
        assert!(v("0.2.1") > v("0.2.0"));
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert!(!(v("0.2.0") > v("0.2.0")));
    }

    #[test]
    fn reads_v_prefix_and_drops_suffix() {
        assert_eq!(Version::parse("v0.2.1"), Version::parse("0.2.1"));
        assert_eq!(Version::parse("0.2.1-rc.1"), Version::parse("0.2.1"));
        assert_eq!(
            Version::parse("v1.2.3"),
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3
            })
        );
    }

    #[test]
    fn rejects_what_is_not_x_y_z() {
        for text in [
            "", "v", "0.2", "a.b.c", "0.2.1.4", "+0.2.1", "0.-2.1", "latest",
        ] {
            assert_eq!(Version::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn reads_release_json() {
        let release = parse_release(RELEASE_JSON).unwrap();
        assert_eq!(release.version, Version::parse("0.2.1").unwrap());
        assert_eq!(release.notes, "Novedades\n- Arreglos");
        assert_eq!(release.binary_url.as_deref(), Some("https://x.dev/bin"));
        assert_eq!(release.sums_url.as_deref(), Some("https://x.dev/sums"));
        assert!(release.offers_update(Version::parse("0.2.0").unwrap()));
        assert!(!release.offers_update(Version::parse("0.2.1").unwrap()));
    }

    #[test]
    fn release_without_files_offers_nothing() {
        let json = r#"{"tag_name": "v0.3.0", "body": null, "assets": []}"#;
        let release = parse_release(json).unwrap();
        assert_eq!(release.notes, "");
        assert!(!release.offers_update(Version::parse("0.2.0").unwrap()));
    }

    #[test]
    fn rejects_bad_json_and_bad_tag() {
        assert!(parse_release("no es json").is_err());
        let tag = r#"{"tag_name": "nightly", "assets": []}"#;
        assert!(parse_release(tag).unwrap_err().contains("nightly"));
    }

    #[test]
    fn reads_sha256sums_line_for_the_binary() {
        let hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let sums = format!(
            "{}  otro-archivo\n{}  {}\n",
            "0".repeat(64),
            hex.to_uppercase(),
            BINARY_NAME
        );
        assert_eq!(expected_digest(&sums, BINARY_NAME).as_deref(), Some(hex));
        assert_eq!(expected_digest(&sums, "no-esta"), None);
        // Modo binario de sha256sum: el nombre lleva `*` delante.
        let star = format!("{hex} *{BINARY_NAME}");
        assert_eq!(expected_digest(&star, BINARY_NAME).as_deref(), Some(hex));
        // Una huella corta no vale.
        assert_eq!(
            expected_digest(&format!("abc  {BINARY_NAME}"), BINARY_NAME),
            None
        );
    }

    #[test]
    fn accepts_the_right_bytes_and_rejects_others() {
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_hex(b"abc"), abc);
        assert!(verify(b"abc", abc).is_ok());
        let err = verify(b"abd", abc).unwrap_err();
        assert!(err.contains("no coincide"));
    }

    #[test]
    fn checks_only_when_six_hours_passed() {
        let now = 1_000_000;
        assert!(!check_due(now - 3600, now));
        assert!(!check_due(now - (6 * 3600 - 1), now));
        assert!(check_due(now - 6 * 3600, now));
        // Una hora en el futuro: el reloj cambió, se vuelve a mirar.
        assert!(check_due(now + 3600, now));
    }

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("atalaya-actualizar-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replaces_the_binary_in_place() {
        let dir = test_dir("reemplazo");
        let exe = dir.join("atalaya-desktop");
        fs::write(&exe, b"vieja").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o600)).unwrap();

        check_replaceable(&exe).unwrap();
        replace(&exe, b"nueva").unwrap();

        let mode = fs::metadata(&exe).unwrap().permissions().mode() & 0o777;
        let content = fs::read(&exe).unwrap();
        let leftovers = fs::read_dir(&dir).unwrap().count();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(content, b"nueva");
        assert_eq!(mode, 0o755);
        // Solo queda el programa: no sobra el provisional ni la prueba.
        assert_eq!(leftovers, 1);
    }

    /// Un servidor de pruebas: contesta `requests` peticiones. Las rutas que
    /// no están en `routes` dan 404. Devuelve su dirección.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>, requests: usize) -> String {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().take(requests) {
                let Ok(mut stream) = stream else { continue };
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&request);
                let path = text.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let response = match routes.iter().find(|(route, _)| *route == path) {
                    Some((_, body)) => {
                        let mut out = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        out.extend_from_slice(body);
                        out
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = stream.write_all(&response);
            }
        });
        address
    }

    fn release_at(server: &str) -> Release {
        Release {
            version: Version::parse("0.2.1").unwrap(),
            notes: String::new(),
            binary_url: Some(format!("{server}/bin")),
            sums_url: Some(format!("{server}/sums")),
        }
    }

    #[test]
    fn install_replaces_the_binary_when_the_digest_matches() {
        let dir = test_dir("instalar");
        let exe = dir.join("atalaya-desktop");
        fs::write(&exe, b"vieja").unwrap();
        let sums = format!("{}  {BINARY_NAME}\n", sha256_hex(b"nueva"));
        let server = serve(
            vec![("/sums", sums.into_bytes()), ("/bin", b"nueva".to_vec())],
            2,
        );

        let result = install(&release_at(&server), &exe);

        let content = fs::read(&exe).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        result.unwrap();
        assert_eq!(content, b"nueva");
    }

    #[test]
    fn install_rejects_a_binary_with_another_digest() {
        let dir = test_dir("huella");
        let exe = dir.join("atalaya-desktop");
        fs::write(&exe, b"vieja").unwrap();
        let sums = format!("{}  {BINARY_NAME}\n", "0".repeat(64));
        let server = serve(
            vec![("/sums", sums.into_bytes()), ("/bin", b"nueva".to_vec())],
            2,
        );

        let result = install(&release_at(&server), &exe);

        let content = fs::read(&exe).unwrap();
        let leftovers = fs::read_dir(&dir).unwrap().count();
        fs::remove_dir_all(&dir).unwrap();
        assert!(result.unwrap_err().contains("no coincide"));
        // El programa actual sigue igual y no queda ningún provisional.
        assert_eq!(content, b"vieja");
        assert_eq!(leftovers, 1);
    }

    #[test]
    fn refuses_a_build_inside_target() {
        let exe = Path::new("/algun/sitio/target/debug/atalaya-desktop");
        let err = check_replaceable(exe).unwrap_err();
        assert!(err.contains("scripts/instalar-pc.sh --release"));
    }
}
