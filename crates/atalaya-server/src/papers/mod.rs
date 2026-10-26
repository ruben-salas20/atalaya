//! Papers de arXiv: búsqueda (paso 4.1; P1–P3), paper abierto y guardado
//! (paso 4.2; P4, P5). ADR 0005.
//!
//! 1. `GET /api/papers/translate` traduce la búsqueda al inglés, para que la
//!    app muestre «Se buscará en inglés: …». Las traducciones se guardan en la
//!    base: la app la pide cada vez que Rubén deja de escribir, y repetir una
//!    búsqueda no gasta GPU.
//! 2. `GET /api/papers/search` traduce la búsqueda (de la caché, casi siempre),
//!    pregunta a arXiv ([`arxiv`]) y responde con los títulos ya en español
//!    ([`translate`]). Los datos de cada paper se guardan en la base, para
//!    abrirlo después sin volver a preguntar a arXiv.
//! 3. `GET /api/papers/{arxiv_id}` responde con el resumen y pone en marcha
//!    su traducción ([`abstracts`]).
//! 4. `POST /api/papers/{arxiv_id}/save` lo guarda en Miniflux ([`saved`]).

pub mod abstract_math;
pub mod abstracts;
pub mod arxiv;
pub mod db;
pub mod full;
pub mod saved;
pub mod translate;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex, MutexGuard},
};

use atalaya_api::{
    EntrySummary, Id, Paper, PaperField, PaperPage, PaperQuery, PaperSummary, QueryTranslation,
    QueryTranslationRequest,
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    http::StatusCode,
    routing::{get, post},
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
        .route("/papers/{arxiv_id}", get(open))
        .route("/papers/{arxiv_id}/save", post(save))
        .route(
            "/papers/{arxiv_id}/full",
            get(full_state).post(full_request),
        )
}

#[derive(Clone)]
pub struct Papers {
    inner: Arc<Inner>,
}

struct Inner {
    arxiv: Arxiv,
    translations: Translations,
    /// Papers completos (P7, ADR 0010).
    full: full::FullPapers,
    /// Limita las peticiones a llama-swap a la vez desde los papers.
    gpu: Semaphore,
    /// Papers cuyo resumen se está traduciendo ahora mismo.
    abstracts_running: Mutex<HashSet<String>>,
    /// Id del feed «Papers guardados», cuando ya se encontró.
    saved_feed: tokio::sync::Mutex<Option<Id>>,
}

impl Papers {
    /// Si el resumen o el texto completo de ese paper se está traduciendo (#91).
    pub fn is_translating(&self, arxiv_id: &str) -> bool {
        self.abstracts_running().contains(arxiv_id) || self.full().is_running(arxiv_id)
    }

    /// Olvida lo que se guarda en memoria de ese paper, tras borrar su
    /// traducción (#91).
    pub fn forget_cached(&self, arxiv_id: &str) {
        self.full().forget_abstract_svgs(arxiv_id);
    }

    pub fn full(&self) -> &full::FullPapers {
        &self.inner.full
    }

    pub fn new(http: reqwest::Client, translations: Translations) -> Self {
        Self::with_arxiv(Arxiv::new(http), translations)
    }

    pub fn with_arxiv(arxiv: Arxiv, translations: Translations) -> Self {
        Self {
            inner: Arc::new(Inner {
                arxiv,
                full: full::FullPapers::new(
                    reqwest::Client::builder()
                        .user_agent(concat!("Atalaya/", env!("CARGO_PKG_VERSION")))
                        .build()
                        .expect("no se pudo crear el cliente HTTP"),
                    translations.clone(),
                ),
                translations,
                gpu: Semaphore::new(translate::PARALLEL_REQUESTS),
                abstracts_running: Mutex::new(HashSet::new()),
                saved_feed: tokio::sync::Mutex::new(None),
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

    fn abstracts_running(&self) -> MutexGuard<'_, HashSet<String>> {
        self.inner
            .abstracts_running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn saved_feed_cache(&self) -> &tokio::sync::Mutex<Option<Id>> {
        &self.inner.saved_feed
    }

    /// Un paper por su identificador: de la base si ya se vio; si no, de arXiv.
    async fn paper(&self, arxiv_id: &str) -> AppResult<ArxivPaper> {
        let id = arxiv_id.to_owned();
        let known = self.store().call(move |c| db::paper(c, &id)).await;
        match known {
            Ok(Some(p)) => return Ok(p),
            Ok(None) => {}
            Err(e) => tracing::error!(paper = arxiv_id, "no se pudo leer el paper: {e}"),
        }
        let paper =
            self.arxiv().paper(arxiv_id).await?.ok_or_else(|| {
                AppError::not_found(format!("arXiv no tiene el paper {arxiv_id}"))
            })?;
        self.remember(std::slice::from_ref(&paper)).await;
        Ok(paper)
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
    let Query(req) = query.map_err(|e| invalid(crate::error::detail_es(&e.body_text())))?;
    let english = state.papers.english_query(&req.q).await?;
    Ok(Json(QueryTranslation { q: req.q, english }))
}

async fn search(
    State(state): State<AppState>,
    query: Result<Query<PaperQuery>, QueryRejection>,
) -> AppResult<Json<PaperPage>> {
    let Query(query) = query.map_err(|e| invalid(crate::error::detail_es(&e.body_text())))?;
    let papers = &state.papers;
    if query.field != PaperField::Author
        && let Some(id) = arxiv::id_in_query(&query.q)
    {
        return search_by_id(papers, id, query.offset.unwrap_or(0)).await;
    }
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

/// Una búsqueda que es un identificador de arXiv: ese paper o ninguno.
async fn search_by_id(papers: &Papers, id: String, offset: u32) -> AppResult<Json<PaperPage>> {
    let found = if offset == 0 {
        papers.arxiv().paper(&id).await?
    } else {
        None
    };
    let found: Vec<ArxivPaper> = found.into_iter().collect();
    papers.remember(&found).await;
    let titles = papers
        .titles(found.iter().map(to_translate).collect())
        .await;
    let saved = papers.saved_entries().await;
    Ok(Json(PaperPage {
        english_query: id,
        total: u64::from(offset == 0 && !found.is_empty()),
        papers: found
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

/// El identificador de la ruta, sin versión. La barra de los antiguos llega
/// ya decodificada (`hep-th%2F9901001` → `hep-th/9901001`).
fn arxiv_id(path: Result<Path<String>, PathRejection>) -> AppResult<String> {
    let Path(raw) = path.map_err(|e| invalid(crate::error::detail_es(&e.body_text())))?;
    let id = arxiv::strip_version(raw.trim()).to_owned();
    if !arxiv::is_valid_id(&id) {
        return Err(AppError::bad_request(format!(
            "«{raw}» no es un identificador de arXiv"
        )));
    }
    Ok(id)
}

async fn open(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> AppResult<Json<Paper>> {
    let id = arxiv_id(path)?;
    let papers = &state.papers;
    let paper = papers.paper(&id).await?;
    let title = papers
        .titles(vec![to_translate(&paper)])
        .await
        .remove(&paper.arxiv_id);
    let (translation, translated_abstract) = papers.abstract_state(&paper).await;
    let saved = papers.saved_entries().await.get(&paper.arxiv_id).copied();
    let (abstract_html, translated_abstract_html) = papers
        .full()
        .abstract_html(
            &paper.arxiv_id,
            &paper.abstract_text,
            translated_abstract.as_deref(),
        )
        .await;
    Ok(Json(Paper {
        summary: summary(&paper, title.as_ref(), saved),
        abstract_text: abstract_math::tex_dashes(&paper.abstract_text),
        translation,
        translated_abstract: translated_abstract
            .as_deref()
            .map(abstract_math::tex_dashes),
        abstract_html,
        translated_abstract_html,
        abs_url: arxiv::abs_url(&paper.arxiv_id),
        pdf_url: arxiv::pdf_url(&paper.arxiv_id),
    }))
}

/// `GET /api/papers/{arxiv_id}/full`: cómo va el paper completo (P7).
async fn full_state(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> AppResult<Json<atalaya_api::FullPaper>> {
    let id = arxiv_id(path)?;
    state
        .papers
        .full()
        .state(&id)
        .await
        .map(Json)
        .map_err(AppError::bad_gateway)
}

/// `POST /api/papers/{arxiv_id}/full`: lo pide (P7). Responde como el `GET`.
async fn full_request(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> AppResult<Json<atalaya_api::FullPaper>> {
    let id = arxiv_id(path)?;
    state
        .papers
        .full()
        .request(&id)
        .await
        .map(Json)
        .map_err(AppError::bad_gateway)
}

/// `GET /formulas/{arxiv_id}/{n}.svg`: una fórmula de un paper completo. Va
/// sin clave, como una imagen cualquiera: solo es una fórmula de un paper
/// público, y el servicio solo se ve dentro de Tailscale (ADR 0008).
pub async fn formula_svg(
    State(state): State<AppState>,
    Path((raw_id, file)): Path<(String, String)>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let id = arxiv::strip_version(raw_id.trim()).to_owned();
    // `rK.svg` es la fórmula K del resumen (#80); se guarda con número negativo.
    let n = file
        .strip_suffix(".svg")
        .and_then(|n| match n.strip_prefix('r') {
            Some(k) => k.parse::<i64>().ok().map(|k| -(k + 1)),
            None => n.parse::<i64>().ok(),
        });
    let (true, Some(n)) = (arxiv::is_valid_id(&id), n) else {
        return AppError::not_found("Esa fórmula no existe").into_response();
    };
    match state.papers.full().formula(&id, n).await {
        Ok(Some(svg)) => (
            [
                (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
                (
                    axum::http::header::CACHE_CONTROL,
                    "public, max-age=31536000, immutable",
                ),
            ],
            svg,
        )
            .into_response(),
        Ok(None) => AppError::not_found("Esa fórmula no existe").into_response(),
        Err(e) => AppError::bad_gateway(e).into_response(),
    }
}

async fn save(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> AppResult<(StatusCode, Json<EntrySummary>)> {
    let id = arxiv_id(path)?;
    let paper = state.papers.paper(&id).await?;
    let saved = state.papers.save(&paper).await?;
    let entry = state.miniflux.entry(saved.entry_id).await?;
    let title = state
        .translations
        .titles(&[saved.entry_id])
        .await
        .remove(&saved.entry_id);
    let status = if saved.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(crate::entries::summary(&entry, title.as_ref())),
    ))
}
