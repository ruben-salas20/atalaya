//! Base SQLite de traducciones, en `<ATALAYA_DATA_DIR>/atalaya.db`.
//!
//! Dos tablas:
//!
//! - `entries`: el idioma de cada artículo de Miniflux que ya se vio.
//! - `translations`: una fila por artículo y por campo traducido (`title` o
//!   `content`). `source_hash` es el SHA-256 del original tal como lo da
//!   Miniflux: si cambia el texto, la traducción vieja deja de valer. Con
//!   `content`, `original` guarda el artículo completo pedido a Miniflux
//!   cuando el feed solo trae un resumen (L7).
//!
//! Los papers de arXiv (fase 4) tienen sus propias tablas, porque no son
//! artículos de Miniflux hasta que se guardan: ver [`crate::papers`].
//!
//! La conexión es una sola, tras un `Mutex`. Las consultas son cortas; para
//! no bloquear el runtime de Tokio, se llaman con [`Store::call`].

use std::{
    collections::{HashMap, hash_map::Entry},
    path::Path,
    sync::{Arc, Mutex},
};

use atalaya_api::Id;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::translations::TitleInfo;

/// Versión del esquema, en `PRAGMA user_version`. Cada cambio de tablas
/// añade un paso a [`migrate`] y sube este número.
const SCHEMA_VERSION: i32 = 3;

/// Campo traducido de un artículo.
pub const FIELD_TITLE: &str = "title";
pub const FIELD_CONTENT: &str = "content";

/// Estado de una fila de `translations`.
pub const STATE_DONE: &str = "done";
pub const STATE_FAILED: &str = "failed";
/// Solo `content`: la traducción empezó y no ha terminado.
pub const STATE_PENDING: &str = "pending";

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

/// Lo que se sabe de un artículo antes de traducirlo.
#[derive(Debug, Clone, PartialEq)]
pub struct SeenEntry {
    pub entry_id: Id,
    pub feed_id: Id,
    /// Idioma si la detección fue segura.
    pub detected_lang: Option<String>,
}

impl Store {
    /// Abre (o crea) la base en `dir/atalaya.db`, en modo WAL.
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("no se pudo crear {}: {e}", dir.display()))?;
        let path = dir.join("atalaya.db");
        let conn = Connection::open(&path)
            .map_err(|e| format!("no se pudo abrir {}: {e}", path.display()))?;
        Self::init(conn).map_err(|e| format!("no se pudo preparar {}: {e}", path.display()))
    }

    /// Base en memoria, para las pruebas.
    pub fn open_in_memory() -> Self {
        Self::init(Connection::open_in_memory().expect("SQLite en memoria"))
            .expect("esquema en memoria")
    }

    fn init(mut conn: Connection) -> rusqlite::Result<Self> {
        // WAL deja leer mientras se escribe y es lo que espera `.backup`.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Ejecuta `f` con la conexión en un hilo aparte, fuera del runtime.
    pub async fn call<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Connection) -> R + Send + 'static,
        R: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut conn)
        })
        .await
        .expect("la consulta a SQLite se interrumpió")
    }
}

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(
            "CREATE TABLE entries (
                entry_id      INTEGER PRIMARY KEY,  -- id de Miniflux
                feed_id       INTEGER NOT NULL,
                detected_lang TEXT,                 -- ISO 639-1 si la detección fue segura
                lang          TEXT,                 -- el que vale: el detectado o el del feed
                updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );
            CREATE INDEX entries_feed ON entries (feed_id, detected_lang);

            CREATE TABLE translations (
                entry_id    INTEGER NOT NULL REFERENCES entries (entry_id) ON DELETE CASCADE,
                field       TEXT NOT NULL,          -- 'title' o 'content'
                state       TEXT NOT NULL,          -- 'done', 'failed' o, con 'content', 'pending'
                source_hash TEXT NOT NULL,          -- SHA-256 en hexadecimal del original
                text        TEXT,                   -- la traducción, con state = 'done'
                model       TEXT,                   -- modelo de llama-swap que la hizo
                error       TEXT,                   -- por qué falló, con state = 'failed'
                attempts    INTEGER NOT NULL DEFAULT 0,
                updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                PRIMARY KEY (entry_id, field)
            );",
        )?;
        tx.pragma_update(None, "user_version", 1)?;
        tx.commit()?;
    }
    if version < 2 {
        let tx = conn.transaction()?;
        tx.execute_batch(
            "ALTER TABLE translations ADD COLUMN original TEXT;  -- artículo completo pedido a Miniflux (L7)",
        )?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
    }
    if version < 3 {
        let tx = conn.transaction()?;
        tx.execute_batch(
            "CREATE TABLE papers (
                arxiv_id      TEXT PRIMARY KEY,     -- sin versión: '2610.08812'
                title         TEXT NOT NULL,
                abstract_text TEXT NOT NULL,
                authors       TEXT NOT NULL,        -- JSON: lista de nombres
                published_at  TEXT NOT NULL,        -- RFC 3339
                category      TEXT NOT NULL,
                updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );

            CREATE TABLE paper_translations (
                arxiv_id    TEXT NOT NULL,
                field       TEXT NOT NULL,          -- 'title' o 'abstract'
                state       TEXT NOT NULL,          -- 'done', 'failed' o 'pending'
                source_hash TEXT NOT NULL,
                text        TEXT,
                model       TEXT,
                error       TEXT,
                updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                PRIMARY KEY (arxiv_id, field)
            );

            CREATE TABLE query_translations (
                query      TEXT PRIMARY KEY,        -- en minúsculas y con los espacios normalizados
                english    TEXT NOT NULL,
                model      TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );",
        )?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
    }
    Ok(())
}

/// SHA-256 en hexadecimal, para `translations.source_hash`.
pub fn source_hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// Guarda el idioma detectado de cada artículo y devuelve el que vale para
/// cada uno. Si la detección no fue segura, vale el idioma más frecuente
/// entre los artículos de su feed; si el feed tampoco lo da, `None`.
pub fn record_langs(
    conn: &mut Connection,
    entries: &[SeenEntry],
) -> rusqlite::Result<HashMap<Id, Option<String>>> {
    let tx = conn.transaction()?;
    {
        let mut upsert = tx.prepare_cached(
            "INSERT INTO entries (entry_id, feed_id, detected_lang, lang)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT (entry_id) DO UPDATE SET
                feed_id = excluded.feed_id,
                detected_lang = excluded.detected_lang,
                lang = excluded.lang,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        )?;
        for e in entries {
            upsert.execute(params![e.entry_id, e.feed_id, e.detected_lang])?;
        }
    }
    // Primero se guardan todos: así un lote cuenta para su propio feed.
    let mut langs = HashMap::with_capacity(entries.len());
    let mut feed_langs: HashMap<Id, Option<String>> = HashMap::new();
    {
        let mut majority = tx.prepare_cached(
            "SELECT detected_lang FROM entries
             WHERE feed_id = ?1 AND detected_lang IS NOT NULL
             GROUP BY detected_lang ORDER BY COUNT(*) DESC LIMIT 1",
        )?;
        let mut set_lang = tx.prepare_cached("UPDATE entries SET lang = ?2 WHERE entry_id = ?1")?;
        for e in entries {
            let lang = match &e.detected_lang {
                Some(l) => Some(l.clone()),
                None => {
                    let l = match feed_langs.entry(e.feed_id) {
                        Entry::Occupied(o) => o.get().clone(),
                        Entry::Vacant(v) => v
                            .insert(majority.query_row([e.feed_id], |r| r.get(0)).optional()?)
                            .clone(),
                    };
                    if l.is_some() {
                        set_lang.execute(params![e.entry_id, l])?;
                    }
                    l
                }
            };
            langs.insert(e.entry_id, lang);
        }
    }
    tx.commit()?;
    Ok(langs)
}

/// Si ese campo ya está traducido a partir de ese mismo original (T4).
pub fn is_translated(
    conn: &Connection,
    entry_id: Id,
    field: &str,
    hash: &str,
) -> rusqlite::Result<bool> {
    conn.prepare_cached(
        "SELECT 1 FROM translations
         WHERE entry_id = ?1 AND field = ?2 AND source_hash = ?3 AND state = 'done'",
    )?
    .exists(params![entry_id, field, hash])
}

/// Guarda el resultado de traducir un campo: el texto o el error.
pub fn save_translation(
    conn: &Connection,
    entry_id: Id,
    field: &str,
    hash: &str,
    model: &str,
    result: &Result<String, String>,
) -> rusqlite::Result<()> {
    let (state, text, error) = match result {
        Ok(t) => (STATE_DONE, Some(t.as_str()), None),
        Err(e) => (STATE_FAILED, None, Some(e.as_str())),
    };
    conn.prepare_cached(
        "INSERT INTO translations (entry_id, field, state, source_hash, text, model, error, attempts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)
         ON CONFLICT (entry_id, field) DO UPDATE SET
            state = excluded.state,
            source_hash = excluded.source_hash,
            text = excluded.text,
            model = excluded.model,
            error = excluded.error,
            attempts = CASE WHEN translations.source_hash = excluded.source_hash
                            THEN translations.attempts + 1 ELSE 1 END,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )?
    .execute(params![entry_id, field, state, hash, text, model, error])?;
    Ok(())
}

/// Idioma de un artículo: `None` si aún no se ha visto; `Some(None)` si se
/// vio pero no se sabe.
pub fn entry_lang(conn: &Connection, entry_id: Id) -> rusqlite::Result<Option<Option<String>>> {
    conn.prepare_cached("SELECT lang FROM entries WHERE entry_id = ?1")?
        .query_row([entry_id], |r| r.get(0))
        .optional()
}

/// Lo guardado del texto completo de un artículo.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentRow {
    pub state: String,
    pub source_hash: String,
    pub text: Option<String>,
    pub original: Option<String>,
    /// Segundos desde el último cambio.
    pub age_secs: i64,
}

pub fn content_row(conn: &Connection, entry_id: Id) -> rusqlite::Result<Option<ContentRow>> {
    conn.prepare_cached(
        "SELECT state, source_hash, text, original,
                CAST(strftime('%s', 'now') AS INTEGER) - CAST(strftime('%s', updated_at) AS INTEGER)
         FROM translations WHERE entry_id = ?1 AND field = 'content'",
    )?
    .query_row([entry_id], |r| {
        Ok(ContentRow {
            state: r.get(0)?,
            source_hash: r.get(1)?,
            text: r.get(2)?,
            original: r.get(3)?,
            age_secs: r.get(4)?,
        })
    })
    .optional()
}

/// Marca el texto completo como en curso y guarda el artículo completo, si
/// se pidió a Miniflux. Borra una traducción anterior de otro original.
pub fn start_content(
    conn: &Connection,
    entry_id: Id,
    hash: &str,
    original: Option<&str>,
) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "INSERT INTO translations (entry_id, field, state, source_hash, original)
         VALUES (?1, 'content', 'pending', ?2, ?3)
         ON CONFLICT (entry_id, field) DO UPDATE SET
            state = 'pending',
            text = NULL,
            error = NULL,
            original = excluded.original,
            attempts = CASE WHEN translations.source_hash = excluded.source_hash
                            THEN translations.attempts ELSE 0 END,
            source_hash = excluded.source_hash,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )?
    .execute(params![entry_id, hash, original])?;
    Ok(())
}

/// Idioma y título traducido de los artículos que ya se vieron.
pub fn titles(conn: &Connection, entry_ids: &[Id]) -> rusqlite::Result<HashMap<Id, TitleInfo>> {
    let mut stmt = conn.prepare_cached(
        "SELECT e.lang, t.text FROM entries e
         LEFT JOIN translations t
           ON t.entry_id = e.entry_id AND t.field = 'title' AND t.state = 'done'
         WHERE e.entry_id = ?1",
    )?;
    let mut out = HashMap::with_capacity(entry_ids.len());
    for &id in entry_ids {
        let row = stmt
            .query_row([id], |r| {
                Ok(TitleInfo {
                    lang: r.get(0)?,
                    title: r.get(1)?,
                })
            })
            .optional()?;
        if let Some(info) = row {
            out.insert(id, info);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(entry_id: Id, feed_id: Id, lang: Option<&str>) -> SeenEntry {
        SeenEntry {
            entry_id,
            feed_id,
            detected_lang: lang.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn opens_file_in_wal_mode_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mode: String = store
            .call(|c| c.pragma_query_value(None, "journal_mode", |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(mode, "wal");
        store
            .call(|c| record_langs(c, &[seen(1, 1, Some("en"))]))
            .await
            .unwrap();
        drop(store);

        let store = Store::open(dir.path()).unwrap();
        let got = store.call(|c| titles(c, &[1])).await.unwrap();
        assert_eq!(got[&1].lang.as_deref(), Some("en"));
    }

    #[test]
    fn unsure_entries_take_the_feed_majority() {
        let store = Store::open_in_memory();
        let mut c = store.conn.lock().unwrap();
        let langs = record_langs(
            &mut c,
            &[
                seen(1, 10, None), // antes que los seguros, en el mismo lote
                seen(2, 10, Some("en")),
                seen(3, 10, Some("en")),
                seen(4, 10, Some("es")),
                seen(5, 20, None), // feed sin ningún idioma seguro
            ],
        )
        .unwrap();
        assert_eq!(langs[&1].as_deref(), Some("en"));
        assert_eq!(langs[&4].as_deref(), Some("es"));
        assert_eq!(langs[&5], None);

        let got = titles(&c, &[1, 5, 99]).unwrap();
        assert_eq!(got[&1].lang.as_deref(), Some("en"));
        assert_eq!(got[&5].lang, None);
        assert!(!got.contains_key(&99));
    }

    #[test]
    fn translation_counts_only_for_the_same_original() {
        let store = Store::open_in_memory();
        let mut c = store.conn.lock().unwrap();
        record_langs(&mut c, &[seen(1, 10, Some("en"))]).unwrap();
        let old = source_hash("Hello");
        assert!(!is_translated(&c, 1, FIELD_TITLE, &old).unwrap());

        save_translation(&c, 1, FIELD_TITLE, &old, "m", &Err("caído".into())).unwrap();
        assert!(!is_translated(&c, 1, FIELD_TITLE, &old).unwrap());
        assert_eq!(titles(&c, &[1]).unwrap()[&1].title, None);

        save_translation(&c, 1, FIELD_TITLE, &old, "m", &Ok("Hola".into())).unwrap();
        assert!(is_translated(&c, 1, FIELD_TITLE, &old).unwrap());
        assert_eq!(titles(&c, &[1]).unwrap()[&1].title.as_deref(), Some("Hola"));
        let attempts: i64 = c
            .query_row("SELECT attempts FROM translations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(attempts, 2);

        // Si Miniflux cambia el título, la traducción vieja ya no vale.
        assert!(!is_translated(&c, 1, FIELD_TITLE, &source_hash("Hello!")).unwrap());
    }

    #[test]
    fn content_keeps_the_fetched_original_until_the_source_changes() {
        let store = Store::open_in_memory();
        let mut c = store.conn.lock().unwrap();
        record_langs(&mut c, &[seen(1, 10, Some("en"))]).unwrap();
        assert_eq!(entry_lang(&c, 1).unwrap(), Some(Some("en".into())));
        assert_eq!(entry_lang(&c, 2).unwrap(), None);
        assert_eq!(content_row(&c, 1).unwrap(), None);

        let hash = source_hash("<p>Resumen</p>");
        start_content(&c, 1, &hash, Some("<p>Completo</p>")).unwrap();
        let row = content_row(&c, 1).unwrap().unwrap();
        assert_eq!(row.state, STATE_PENDING);
        assert_eq!(row.original.as_deref(), Some("<p>Completo</p>"));
        assert!(row.age_secs <= 1);

        save_translation(&c, 1, FIELD_CONTENT, &hash, "m", &Ok("<p>Hola</p>".into())).unwrap();
        let row = content_row(&c, 1).unwrap().unwrap();
        assert_eq!(row.state, STATE_DONE);
        assert_eq!(row.text.as_deref(), Some("<p>Hola</p>"));
        assert_eq!(row.original.as_deref(), Some("<p>Completo</p>"));
        // El título no se mezcla con el texto.
        assert_eq!(titles(&c, &[1]).unwrap()[&1].title, None);

        start_content(&c, 1, &source_hash("<p>Otro</p>"), None).unwrap();
        let row = content_row(&c, 1).unwrap().unwrap();
        assert_eq!(row.state, STATE_PENDING);
        assert_eq!(row.text, None);
        assert_eq!(row.original, None);
    }

    #[test]
    fn migrates_a_version_1_database() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("atalaya.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE entries (entry_id INTEGER PRIMARY KEY, feed_id INTEGER NOT NULL,
                detected_lang TEXT, lang TEXT, updated_at TEXT NOT NULL DEFAULT '');
             CREATE TABLE translations (entry_id INTEGER NOT NULL, field TEXT NOT NULL,
                state TEXT NOT NULL, source_hash TEXT NOT NULL, text TEXT, model TEXT,
                error TEXT, attempts INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT '', PRIMARY KEY (entry_id, field));
             PRAGMA user_version = 1;",
        )
        .unwrap();
        drop(conn);
        let store = Store::open(dir.path()).unwrap();
        let c = store.conn.lock().unwrap();
        let version: i32 = c
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        c.execute(
            "INSERT INTO translations (entry_id, field, state, source_hash, original)
             VALUES (1, 'content', 'pending', 'h', 'x')",
            [],
        )
        .unwrap();
    }
}
