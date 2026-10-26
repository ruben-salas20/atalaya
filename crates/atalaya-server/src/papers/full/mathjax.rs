//! Fórmulas a SVG con MathJax (ADR 0010). El servicio lanza `node` una vez
//! por paper con todas sus fórmulas: `mathjax/svg.mjs`, que la imagen instala
//! en `/opt/mathjax`.

use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command};

/// Dónde está el script, salvo que `ATALAYA_MATHJAX` diga otra cosa.
pub const DEFAULT_SCRIPT: &str = "/opt/mathjax/svg.mjs";

/// Una fórmula ya convertida. El tamaño y la alineación van en `ex`, la
/// altura de la x de la letra: así la app la ajusta a su tamaño de letra.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Svg {
    pub svg: String,
    pub vertical_align: String,
    pub width: String,
    pub height: String,
}

#[derive(Serialize)]
struct Input<'a> {
    latex: &'a str,
    display: bool,
}

/// Convierte cada `(latex, aparte)`. `None` en las que MathJax no entiende.
pub async fn render(script: &str, formulas: &[(String, bool)]) -> Result<Vec<Option<Svg>>, String> {
    if formulas.is_empty() {
        return Ok(Vec::new());
    }
    let input: Vec<Input> = formulas
        .iter()
        .map(|(latex, display)| Input {
            latex,
            display: *display,
        })
        .collect();
    let input = serde_json::to_vec(&input).map_err(|e| e.to_string())?;

    let mut child = Command::new("node")
        .arg(script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("no se pudo lanzar node para MathJax: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("MathJax sin entrada estándar")?;
    // Se escribe en otra tarea: si la salida llena su búfer antes de leer toda
    // la entrada, escribir y esperar en la misma tarea se bloquearía.
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(&input).await;
    });
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("MathJax no terminó: {e}"))?;
    let _ = writer.await;
    if !output.status.success() {
        return Err(format!(
            "MathJax falló ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).chars().take(300).collect::<String>()
        ));
    }
    let svgs: Vec<Option<Svg>> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("MathJax respondió algo inesperado: {e}"))?;
    if svgs.len() != formulas.len() {
        return Err(format!(
            "MathJax devolvió {} fórmulas de {}",
            svgs.len(),
            formulas.len()
        ));
    }
    Ok(svgs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Usa el MathJax del repo si está instalado (`npm install` en
    /// `crates/atalaya-server/mathjax`); si no, no comprueba nada.
    #[tokio::test]
    async fn renders_with_the_repo_script() {
        let script = concat!(env!("CARGO_MANIFEST_DIR"), "/mathjax/svg.mjs");
        let modules = concat!(env!("CARGO_MANIFEST_DIR"), "/mathjax/node_modules");
        if !std::path::Path::new(modules).exists() {
            eprintln!("sin node_modules de MathJax: prueba omitida");
            return;
        }
        let out = render(
            script,
            &[
                ("\\frac{1}{2}".into(), false),
                ("\\lx@scalerel@obj".into(), false),
                ("E=mc^2".into(), true),
            ],
        )
        .await
        .unwrap();
        assert_eq!(out.len(), 3);
        let first = out[0].as_ref().unwrap();
        assert!(first.svg.starts_with("<svg"));
        assert!(first.vertical_align.ends_with("ex"));
        assert!(out[1].is_none());
        assert!(out[2].is_some());
    }
}
