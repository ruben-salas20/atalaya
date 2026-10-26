//! Borrar traducciones guardadas (#91): la de un artículo o un paper, para
//! que se traduzca otra vez, y en bloque las de lo ya leído, para liberar
//! espacio. Solo se toca la base de Atalaya, nunca Miniflux.

use std::collections::HashSet;

use atalaya_api::{Cleanup, Id};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get},
};
use rusqlite::{Connection, params};
use serde::Deserialize;

use crate::{
    AppState,
    error::{ApiPath, AppError, AppResult},
    papers::arxiv,
    store::{FIELD_CONTENT, FIELD_TITLE},
    titles::{NewEntry, Priority},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/entries/{id}/translation", delete(forget_entry))
        .route("/papers/{arxiv_id}/translation", delete(forget_paper))
        .route("/translations/cleanup", get(preview).post(clean))
}

/// `DELETE /api/entries/{id}/translation`: borra el título y el texto
/// traducidos. El título vuelve a la cola enseguida; el texto se traduce al
/// abrir el artículo otra vez.
async fn forget_entry(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Id>,
) -> AppResult<StatusCode> {
    if state.translations.is_translating(id) {
        return Err(busy());
    }
    let entry = state.miniflux.entry(id).await?;
    state
        .translations
        .store()
        .call(move |c| delete_entry_rows(c, &[id], &[FIELD_TITLE, FIELD_CONTENT]))
        .await
        .map_err(db_error)?;
    state.stats.clear();
    let new = NewEntry::new(entry.id, entry.feed_id, entry.title, &entry.content);
    state
        .translations
        .submit_titles(vec![new], Priority::Urgent)
        .await;
    tracing::info!(entry = id, "traducción borrada para volver a traducir");
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/papers/{arxiv_id}/translation`: borra el título, el resumen,
/// el texto completo y las fórmulas del paper. Se traducen otra vez al abrirlo.
async fn forget_paper(
    State(state): State<AppState>,
    ApiPath(raw): ApiPath<String>,
) -> AppResult<StatusCode> {
    let id = arxiv::strip_version(raw.trim()).to_owned();
    if !arxiv::is_valid_id(&id) {
        return Err(AppError::bad_request(
            "Ese identificador de arXiv no es válido",
        ));
    }
    if state.papers.is_translating(&id) {
        return Err(busy());
    }
    let ids = vec![id.clone()];
    state
        .translations
        .store()
        .call(move |c| delete_paper_rows(c, &ids, true))
        .await
        .map_err(db_error)?;
    state.papers.forget_cached(&id);
    tracing::info!(paper = id, "traducción borrada para volver a traducir");
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/translations/cleanup`: cuánto se borraría.
async fn preview(State(state): State<AppState>) -> AppResult<Json<Cleanup>> {
    let targets = targets(&state).await?;
    let summary = state
        .translations
        .store()
        .call(move |c| measure(c, &targets))
        .await
        .map_err(db_error)?;
    Ok(Json(summary))
}

/// `POST /api/translations/cleanup`: lo borra y compacta la base.
async fn clean(State(state): State<AppState>) -> AppResult<Json<Cleanup>> {
    let targets = targets(&state).await?;
    let papers = targets.papers.clone();
    let summary = state
        .translations
        .store()
        .call(move |c| -> rusqlite::Result<Cleanup> {
            let summary = measure(c, &targets)?;
            delete_entry_rows(c, &targets.entries, &[FIELD_CONTENT])?;
            delete_paper_rows(c, &targets.papers, false)?;
            c.execute_batch("VACUUM")?;
            Ok(summary)
        })
        .await
        .map_err(db_error)?;
    for id in &papers {
        state.papers.forget_cached(id);
    }
    tracing::info!(
        entries = summary.entries,
        papers = summary.papers,
        bytes = summary.bytes,
        "traducciones de lo leído borradas"
    );
    Ok(Json(summary))
}

/// Lo que borra la limpieza: los artículos leídos y no guardados con texto
/// traducido, y los papers con algo guardado que no están en Guardados. Nada
/// que se esté traduciendo ahora.
#[derive(Debug, Default, Clone, PartialEq)]
struct Targets {
    entries: Vec<Id>,
    papers: Vec<String>,
}

async fn targets(state: &AppState) -> AppResult<Targets> {
    let read = read_unstarred(state).await?;
    let saved: HashSet<String> = state.papers.saved_entries().await.into_keys().collect();
    let (entries, papers) = state
        .translations
        .store()
        .call(move |c| -> rusqlite::Result<(Vec<Id>, Vec<String>)> {
            Ok((translated_entries(c)?, stored_papers(c)?))
        })
        .await
        .map_err(db_error)?;
    Ok(Targets {
        entries: entries
            .into_iter()
            .filter(|id| read.contains(id) && !state.translations.is_translating(*id))
            .collect(),
        papers: papers
            .into_iter()
            .filter(|id| !saved.contains(id) && !state.papers.is_translating(id))
            .collect(),
    })
}

#[derive(Deserialize)]
struct Page {
    entries: Vec<PageEntry>,
}

#[derive(Deserialize)]
struct PageEntry {
    id: Id,
}

/// Los ids de los artículos leídos y no guardados, de Miniflux.
async fn read_unstarred(state: &AppState) -> AppResult<HashSet<Id>> {
    const LIMIT: usize = 500;
    let mut ids = HashSet::new();
    for page in 0.. {
        let path = format!(
            "/v1/entries?status=read&starred=false&order=id&direction=desc&limit={LIMIT}&offset={}",
            page * LIMIT
        );
        let got: Page = state.miniflux.get(&path).await?;
        let n = got.entries.len();
        ids.extend(got.entries.into_iter().map(|e| e.id));
        if n < LIMIT {
            break;
        }
    }
    Ok(ids)
}

fn translated_entries(c: &Connection) -> rusqlite::Result<Vec<Id>> {
    let mut q = c.prepare("SELECT DISTINCT entry_id FROM translations WHERE field = ?1")?;
    q.query_map(params![FIELD_CONTENT], |r| r.get(0))?.collect()
}

/// Los papers con resumen, texto completo o fórmulas guardados. Los títulos
/// solos no cuentan: son cortos y se ven en la búsqueda.
fn stored_papers(c: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut q = c.prepare(
        "SELECT arxiv_id FROM paper_translations WHERE field = 'abstract'
         UNION SELECT arxiv_id FROM paper_full
         UNION SELECT arxiv_id FROM paper_formulas",
    )?;
    q.query_map([], |r| r.get(0))?.collect()
}

fn measure(c: &Connection, t: &Targets) -> rusqlite::Result<Cleanup> {
    let mut bytes = 0u64;
    let mut entries = 0u64;
    for id in &t.entries {
        let b: Option<i64> = c.query_row(
            "SELECT SUM(LENGTH(COALESCE(text, ''))) FROM translations
             WHERE entry_id = ?1 AND field = ?2",
            params![id, FIELD_CONTENT],
            |r| r.get(0),
        )?;
        if let Some(b) = b {
            entries += 1;
            bytes += b as u64;
        }
    }
    for id in &t.papers {
        let b: i64 = c.query_row(
            "SELECT
               (SELECT COALESCE(SUM(LENGTH(COALESCE(text, ''))), 0) FROM paper_translations
                 WHERE arxiv_id = ?1 AND field = 'abstract')
             + (SELECT COALESCE(SUM(LENGTH(COALESCE(html, ''))), 0) FROM paper_full WHERE arxiv_id = ?1)
             + (SELECT COALESCE(SUM(LENGTH(svg)), 0) FROM paper_formulas WHERE arxiv_id = ?1)",
            params![id],
            |r| r.get(0),
        )?;
        bytes += b as u64;
    }
    Ok(Cleanup {
        entries,
        papers: t.papers.len() as u64,
        bytes,
    })
}

fn delete_entry_rows(c: &Connection, ids: &[Id], fields: &[&str]) -> rusqlite::Result<()> {
    let mut q = c.prepare_cached("DELETE FROM translations WHERE entry_id = ?1 AND field = ?2")?;
    for id in ids {
        for field in fields {
            q.execute(params![id, field])?;
        }
    }
    Ok(())
}

/// Borra lo guardado de esos papers. `with_title`: también el título.
fn delete_paper_rows(c: &Connection, ids: &[String], with_title: bool) -> rusqlite::Result<()> {
    let fields = if with_title {
        "('abstract', 'title')"
    } else {
        "('abstract')"
    };
    let translations =
        format!("DELETE FROM paper_translations WHERE arxiv_id = ?1 AND field IN {fields}");
    for id in ids {
        c.execute(&translations, params![id])?;
        c.execute("DELETE FROM paper_full WHERE arxiv_id = ?1", params![id])?;
        c.execute(
            "DELETE FROM paper_formulas WHERE arxiv_id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

fn busy() -> AppError {
    AppError::new(
        StatusCode::CONFLICT,
        "Se está traduciendo ahora mismo; vuelve a intentarlo cuando termine",
    )
}

fn db_error(e: rusqlite::Error) -> AppError {
    AppError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("No se pudo borrar la traducción: {e}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn seed(c: &Connection) {
        c.execute_batch(
            "INSERT INTO entries (entry_id, feed_id) VALUES (1, 9), (2, 9);
             INSERT INTO translations (entry_id, field, state, source_hash, text)
               VALUES (1, 'title', 'done', 'h', 'Título'), (1, 'content', 'done', 'h', 'Texto largo'),
                      (2, 'content', 'done', 'h', 'Otro');
             INSERT INTO paper_translations (arxiv_id, field, state, source_hash, text)
               VALUES ('2610.1', 'title', 'done', 'h', 'T'), ('2610.1', 'abstract', 'done', 'h', 'Resumen'),
                      ('2610.2', 'title', 'done', 'h', 'Solo título');
             INSERT INTO paper_full (arxiv_id, state, html) VALUES ('2610.1', 'done', '<p>x</p>');
             INSERT INTO paper_formulas (arxiv_id, n, svg) VALUES ('2610.1', 0, '<svg/>'), ('2610.1', -1, '<svg/>');",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn mide_y_borra_lo_leido_sin_tocar_titulos() {
        let store = Store::open_in_memory();
        let targets = Targets {
            entries: vec![1],
            papers: vec!["2610.1".into()],
        };
        let (summary, left) = store
            .call(move |c| {
                seed(c);
                assert_eq!(translated_entries(c).unwrap().len(), 2);
                assert_eq!(stored_papers(c).unwrap(), vec!["2610.1".to_string()]);
                let summary = measure(c, &targets).unwrap();
                delete_entry_rows(c, &targets.entries, &[FIELD_CONTENT]).unwrap();
                delete_paper_rows(c, &targets.papers, false).unwrap();
                let left: Vec<(Id, String)> = c
                    .prepare("SELECT entry_id, field FROM translations ORDER BY entry_id, field")
                    .unwrap()
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                let papers: i64 = c
                    .query_row(
                        "SELECT (SELECT COUNT(*) FROM paper_full) + (SELECT COUNT(*) FROM paper_formulas)
                         + (SELECT COUNT(*) FROM paper_translations WHERE field = 'abstract')",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(papers, 0);
                (summary, left)
            })
            .await;
        assert_eq!(
            summary,
            Cleanup {
                entries: 1,
                papers: 1,
                bytes: ("Texto largo".len()
                    + "Resumen".len()
                    + "<p>x</p>".len()
                    + 2 * "<svg/>".len()) as u64
            }
        );
        // El título del 1 y el texto del 2 (no leído) se quedan.
        assert_eq!(left, vec![(1, "title".into()), (2, "content".into())]);
    }

    #[tokio::test]
    async fn volver_a_traducir_un_paper_borra_tambien_su_titulo() {
        let store = Store::open_in_memory();
        let left: i64 = store
            .call(|c| {
                seed(c);
                delete_paper_rows(c, &["2610.1".into()], true).unwrap();
                c.query_row(
                    "SELECT COUNT(*) FROM paper_translations WHERE arxiv_id = '2610.1'",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
            })
            .await;
        assert_eq!(left, 0);
    }
}
