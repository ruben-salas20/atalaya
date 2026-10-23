//! Consultas a las tablas de papers (`papers`, `paper_translations` y
//! `query_translations`). El esquema está en `store.rs`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use super::arxiv::ArxivPaper;

pub const FIELD_TITLE: &str = "title";
pub const FIELD_ABSTRACT: &str = "abstract";

/// Guarda o actualiza los datos de los papers, tal como los da arXiv.
pub fn save_papers(conn: &mut Connection, papers: &[ArxivPaper]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut upsert = tx.prepare_cached(
            "INSERT INTO papers (arxiv_id, title, abstract_text, authors, published_at, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (arxiv_id) DO UPDATE SET
                title = excluded.title,
                abstract_text = excluded.abstract_text,
                authors = excluded.authors,
                published_at = excluded.published_at,
                category = excluded.category,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        )?;
        for p in papers {
            let authors = serde_json::to_string(&p.authors).unwrap_or_else(|_| "[]".into());
            upsert.execute(params![
                p.arxiv_id,
                p.title,
                p.abstract_text,
                authors,
                p.published_at.to_rfc3339(),
                p.category
            ])?;
        }
    }
    tx.commit()
}

/// Un paper ya visto, o `None`.
pub fn paper(conn: &Connection, arxiv_id: &str) -> rusqlite::Result<Option<ArxivPaper>> {
    conn.prepare_cached(
        "SELECT title, abstract_text, authors, published_at, category
         FROM papers WHERE arxiv_id = ?1",
    )?
    .query_row([arxiv_id], |r| {
        let authors: String = r.get(2)?;
        let published: String = r.get(3)?;
        Ok(ArxivPaper {
            arxiv_id: arxiv_id.to_owned(),
            title: r.get(0)?,
            abstract_text: r.get(1)?,
            authors: serde_json::from_str(&authors).unwrap_or_default(),
            published_at: DateTime::parse_from_rfc3339(&published)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or(DateTime::UNIX_EPOCH),
            category: r.get(4)?,
        })
    })
    .optional()
}

/// Lo guardado de la traducción de un campo de un paper.
#[derive(Debug, Clone, PartialEq)]
pub struct PaperTranslation {
    pub state: String,
    pub source_hash: String,
    pub text: Option<String>,
    /// Segundos desde el último cambio.
    pub age_secs: i64,
}

pub fn translation(
    conn: &Connection,
    arxiv_id: &str,
    field: &str,
) -> rusqlite::Result<Option<PaperTranslation>> {
    conn.prepare_cached(
        "SELECT state, source_hash, text,
                CAST(strftime('%s', 'now') AS INTEGER) - CAST(strftime('%s', updated_at) AS INTEGER)
         FROM paper_translations WHERE arxiv_id = ?1 AND field = ?2",
    )?
    .query_row(params![arxiv_id, field], |r| {
        Ok(PaperTranslation {
            state: r.get(0)?,
            source_hash: r.get(1)?,
            text: r.get(2)?,
            age_secs: r.get(3)?,
        })
    })
    .optional()
}

/// Títulos ya traducidos de esos papers, por identificador y hash del
/// original. Los que faltan no están traducidos o cambiaron.
pub fn done_titles(
    conn: &Connection,
    wanted: &[(String, String)],
) -> rusqlite::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT text FROM paper_translations
         WHERE arxiv_id = ?1 AND field = 'title' AND source_hash = ?2 AND state = 'done'",
    )?;
    let mut out = HashMap::new();
    for (id, hash) in wanted {
        if let Some(text) = stmt
            .query_row(params![id, hash], |r| r.get::<_, Option<String>>(0))
            .optional()?
            .flatten()
        {
            out.insert(id.clone(), text);
        }
    }
    Ok(out)
}

/// Guarda el estado de la traducción de un campo: `pending`, o el resultado.
pub fn save_translation(
    conn: &Connection,
    arxiv_id: &str,
    field: &str,
    hash: &str,
    model: &str,
    result: Option<&Result<String, String>>,
) -> rusqlite::Result<()> {
    let (state, text, error) = match result {
        None => (crate::store::STATE_PENDING, None, None),
        Some(Ok(t)) => (crate::store::STATE_DONE, Some(t.as_str()), None),
        Some(Err(e)) => (crate::store::STATE_FAILED, None, Some(e.as_str())),
    };
    conn.prepare_cached(
        "INSERT INTO paper_translations (arxiv_id, field, state, source_hash, text, model, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (arxiv_id, field) DO UPDATE SET
            state = excluded.state,
            source_hash = excluded.source_hash,
            text = excluded.text,
            model = excluded.model,
            error = excluded.error,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )?
    .execute(params![arxiv_id, field, state, hash, text, model, error])?;
    Ok(())
}

pub fn query_translation(conn: &Connection, query: &str) -> rusqlite::Result<Option<String>> {
    conn.prepare_cached("SELECT english FROM query_translations WHERE query = ?1")?
        .query_row([query], |r| r.get(0))
        .optional()
}

pub fn save_query_translation(
    conn: &Connection,
    query: &str,
    english: &str,
    model: &str,
) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "INSERT INTO query_translations (query, english, model) VALUES (?1, ?2, ?3)
         ON CONFLICT (query) DO UPDATE SET
            english = excluded.english,
            model = excluded.model,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )?
    .execute(params![query, english, model])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Store, source_hash};

    fn paper(id: &str) -> ArxivPaper {
        ArxivPaper {
            arxiv_id: id.into(),
            title: "Small models".into(),
            abstract_text: "One.\n\nTwo.".into(),
            authors: vec!["Wei Chen".into()],
            published_at: DateTime::parse_from_rfc3339("2026-10-21T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            category: "cs.CL".into(),
        }
    }

    #[tokio::test]
    async fn stores_papers_and_translations() {
        let store = Store::open_in_memory();
        store
            .call(|c| {
                save_papers(c, &[paper("2610.08812")])?;
                assert_eq!(super::paper(c, "2610.08812")?, Some(paper("2610.08812")));
                assert_eq!(super::paper(c, "1")?, None);

                let h = source_hash("Small models");
                let wanted = vec![("2610.08812".to_owned(), h.clone())];
                assert!(done_titles(c, &wanted)?.is_empty());
                save_translation(c, "2610.08812", FIELD_TITLE, &h, "m", None)?;
                assert!(done_titles(c, &wanted)?.is_empty());
                let ok = Ok("Modelos pequeños".to_owned());
                save_translation(c, "2610.08812", FIELD_TITLE, &h, "m", Some(&ok))?;
                assert_eq!(done_titles(c, &wanted)?["2610.08812"], "Modelos pequeños");
                let other = vec![("2610.08812".to_owned(), source_hash("Otro"))];
                assert!(done_titles(c, &other)?.is_empty(), "el original cambió");

                let row = translation(c, "2610.08812", FIELD_TITLE)?.unwrap();
                assert_eq!(row.state, "done");
                assert_eq!(translation(c, "2610.08812", FIELD_ABSTRACT)?, None);

                save_query_translation(c, "modelos pequeños", "small models", "m")?;
                assert_eq!(
                    query_translation(c, "modelos pequeños")?.as_deref(),
                    Some("small models")
                );
                Ok::<(), rusqlite::Error>(())
            })
            .await
            .unwrap();
    }
}
