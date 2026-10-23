//! Búsqueda de papers en arXiv (paso 4.1; P1–P3, ADR 0005).
//!
//! 1. `GET /api/papers/translate` traduce la búsqueda al inglés, para que la
//!    app muestre «Se buscará en inglés: …». Las traducciones se guardan en la
//!    base: la app la pide cada vez que Rubén deja de escribir, y repetir una
//!    búsqueda no gasta GPU.
//! 2. `GET /api/papers/search` traduce la búsqueda (de la caché, casi siempre),
//!    pregunta a arXiv ([`arxiv`]) y responde con los títulos ya en español
//!    ([`translate`]). Los datos de cada paper se guardan en la base, para
//!    abrirlo después sin volver a preguntar a arXiv.

pub mod arxiv;
pub mod db;
pub mod translate;

use std::sync::Arc;

use atalaya_api::{
    PaperField, PaperPage, PaperQuery, PaperSummary, QueryTranslation, QueryTranslationRequest,
};
use axum::{
    Json, Router,
    extract::{Query, State, rejection::QueryRejection},
    routing::get,
};
use tokio::sync::{Semaphore, SemaphorePermit};

use crate::{
    AppState,
    error::{AppError, AppResult},
    llama::Llama,
    store::Store,
    translations::Translations,
};
use arxiv::{Arxiv, ArxivPaper};
use translate::TitleToTranslate;

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 50;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/papers/translate", get(translate_query))
        .route("/papers/search", get(search))
}

#[derive(Clone)]
pub struct Papers {
    inner: Arc<Inner>,
}

struct Inner {
    arxiv: Arxiv,
    translations: Translations,
    /// Limita las peticiones a llama-swap a la vez desde los papers.
    gpu: Semaphore,
}

impl Papers {
    pub fn new(http: reqwest::Client, translations: Translations) -> Self {
        Self::with_arxiv(Arxiv::new(http), translations)
    }

    pub fn with_arxiv(arxiv: Arxiv, translations: Translations) -> Self {
        Self {
            inner: Arc::new(Inner {
                arxiv,
                translations,
                gpu: Semaphore::new(translate::PARALLEL_REQUESTS),
            }),
        }
    }

    pub fn arxiv(&self) -> &Arxiv {
        &self.inner.arxiv
    }

    pub fn translations(&self) -> &Translations {
        &self.inner.translations
    }

    pub fn store(&self) -> &Store {
        self.inner.translations.store()
    }

    pub fn llama(&self) -> &Llama {
        self.inner.translations.llama()
    }

    async fn gpu_permit(&self) -> SemaphorePermit<'_> {
        self.inner
            .gpu
            .acquire()
            .await
            .expect("el semáforo no se cierra nunca")
    }

    /// Guarda los datos de los papers en la base. Un fallo no impide responder.
    async fn remember(&self, papers: &[ArxivPaper]) {
        let papers = papers.to_vec();
        if let Err(e) = self
            .store()
            .call(move |c| db::save_papers(c, &papers))
            .await
        {
            tracing::error!("no se pudieron guardar los papers: {e}");
        }
    }
}

async fn translate_query(
    State(state): State<AppState>,
    query: Result<Query<QueryTranslationRequest>, QueryRejection>,
) -> AppResult<Json<QueryTranslation>> {
    let Query(req) = query.map_err(|e| invalid(e.body_text()))?;
    let english = state.papers.english_query(&req.q).await?;
    Ok(Json(QueryTranslation { q: req.q, english }))
}

async fn search(
    State(state): State<AppState>,
    query: Result<Query<PaperQuery>, QueryRejection>,
) -> AppResult<Json<PaperPage>> {
    let Query(query) = query.map_err(|e| invalid(e.body_text()))?;
    let papers = &state.papers;
    let english = match query.field {
        // Los nombres van tal cual.
        PaperField::Author => query.q.split_whitespace().collect::<Vec<_>>().join(" "),
        _ => papers.english_query(&query.q).await?,
    };
    if english.is_empty() {
        return Err(AppError::bad_request("la búsqueda está vacía"));
    }
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let page = papers
        .arxiv()
        .search(
            &english,
            query.field,
            query.sort,
            query.offset.unwrap_or(0),
            limit,
        )
        .await?;
    papers.remember(&page.papers).await;

    let titles = papers
        .titles(page.papers.iter().map(to_translate).collect())
        .await;
    let saved = papers.saved_entries().await;
    Ok(Json(PaperPage {
        english_query: english,
        total: page.total,
        papers: page
            .papers
            .iter()
            .map(|p| summary(p, titles.get(&p.arxiv_id), saved.get(&p.arxiv_id).copied()))
            .collect(),
    }))
}

fn to_translate(p: &ArxivPaper) -> TitleToTranslate {
    TitleToTranslate {
        arxiv_id: p.arxiv_id.clone(),
        title: p.title.clone(),
        abstract_text: p.abstract_text.clone(),
    }
}

/// Une un paper de arXiv con su título traducido, si lo hay.
fn summary(p: &ArxivPaper, title: Option<&String>, saved: Option<atalaya_api::Id>) -> PaperSummary {
    let translated = title.filter(|t| !t.trim().is_empty());
    PaperSummary {
        arxiv_id: p.arxiv_id.clone(),
        title: translated.unwrap_or(&p.title).clone(),
        original_title: p.title.clone(),
        title_translated: translated.is_some(),
        authors: p.authors.clone(),
        published_at: p.published_at,
        category: p.category.clone(),
        saved_entry_id: saved,
    }
}

fn invalid(detail: String) -> AppError {
    AppError::bad_request(format!("parámetros no válidos: {detail}"))
}

impl Papers {
    /// Papers guardados, por identificador de arXiv. Paso 4.2.
    async fn saved_entries(&self) -> std::collections::HashMap<String, atalaya_api::Id> {
        std::collections::HashMap::new()
    }
}
