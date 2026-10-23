//! Rutas de papers de arXiv (ADR 0005, P1–P5). Siempre contra el servicio:
//! sin red no hay arXiv.

use atalaya_api::{EntrySummary, Paper, PaperPage, PaperQuery, QueryTranslation};
use reqwest::Method;

use super::{ApiClient, Result};

impl ApiClient {
    /// `GET /api/papers/translate`: cómo se buscará en inglés (P1).
    pub fn translate_paper_query(&self, q: &str) -> Result<QueryTranslation> {
        self.get_query("/api/papers/translate", &[("q", q)])
    }

    /// `GET /api/papers/search`.
    pub fn search_papers(&self, query: &PaperQuery) -> Result<PaperPage> {
        self.get_query("/api/papers/search", query)
    }

    /// `GET /api/papers/{arxiv_id}`. Pone en marcha la traducción del resumen.
    pub fn paper(&self, arxiv_id: &str) -> Result<Paper> {
        self.get(&format!("/api/papers/{}", path_id(arxiv_id)))
    }

    /// `POST /api/papers/{arxiv_id}/save`: lo lleva a Guardados (P5).
    pub fn save_paper(&self, arxiv_id: &str) -> Result<EntrySummary> {
        let path = format!("/api/papers/{}/save", path_id(arxiv_id));
        Ok(super::read_json(self.request(Method::POST, &path).send())?)
    }
}

/// Los identificadores antiguos llevan una barra (`hep-th/9901001`), que en
/// la ruta va codificada.
fn path_id(arxiv_id: &str) -> String {
    arxiv_id.replace('/', "%2F")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_ids_escape_the_slash() {
        assert_eq!(path_id("hep-th/9901001"), "hep-th%2F9901001");
        assert_eq!(path_id("2610.08812"), "2610.08812");
    }
}
