//! Tablas del paper completo (`paper_full` y `paper_formulas`). El esquema
//! está en `store.rs`.

use rusqlite::{Connection, OptionalExtension, params};

use super::mathjax::Svg;

pub const STATE_PENDING: &str = "pending";
pub const STATE_DONE: &str = "done";
pub const STATE_FAILED: &str = "failed";
pub const STATE_NOT_AVAILABLE: &str = "not_available";

/// Lo guardado de un paper completo.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub state: String,
    pub html: Option<String>,
}

pub fn row(conn: &Connection, arxiv_id: &str) -> rusqlite::Result<Option<Row>> {
    conn.query_row(
        "SELECT state, html FROM paper_full WHERE arxiv_id = ?1",
        params![arxiv_id],
        |r| {
            Ok(Row {
                state: r.get(0)?,
                html: r.get(1)?,
            })
        },
    )
    .optional()
}

pub fn set_state(
    conn: &Connection,
    arxiv_id: &str,
    state: &str,
    html: Option<&str>,
    error: Option<&str>,
    model: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO paper_full (arxiv_id, state, html, error, model) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (arxiv_id) DO UPDATE SET
            state = excluded.state, html = excluded.html, error = excluded.error,
            model = excluded.model, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        params![arxiv_id, state, html, error, model],
    )?;
    Ok(())
}

/// Guarda las fórmulas ya convertidas, por su número. Borra las de antes.
pub fn save_formulas(
    conn: &mut Connection,
    arxiv_id: &str,
    svgs: &[Option<Svg>],
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM paper_formulas WHERE arxiv_id = ?1",
        params![arxiv_id],
    )?;
    {
        let mut insert =
            tx.prepare_cached("INSERT INTO paper_formulas (arxiv_id, n, svg) VALUES (?1, ?2, ?3)")?;
        for (n, svg) in svgs.iter().enumerate() {
            if let Some(svg) = svg {
                insert.execute(params![arxiv_id, n as i64, svg.svg])?;
            }
        }
    }
    tx.commit()
}

pub fn formula(conn: &Connection, arxiv_id: &str, n: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT svg FROM paper_formulas WHERE arxiv_id = ?1 AND n = ?2",
        params![arxiv_id, n],
        |r| r.get(0),
    )
    .optional()
}
