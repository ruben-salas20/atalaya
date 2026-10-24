//! Traducción de títulos (paso 2.3, T1–T4).
//!
//! Los artículos llegan por dos caminos: el webhook de Miniflux, con lo nuevo,
//! y la revisión periódica, que busca en Miniflux lo que aún no tiene el
//! título traducido (cubre también un webhook perdido). Los dos pasan por
//! [`Translations::submit_titles`], que guarda el idioma y pone en cola lo que
//! hay que traducir. Un único trabajador vacía la cola de uno en uno, porque
//! la GPU es una sola.
//!
//! La misma cola lleva los párrafos del texto completo (`content`, paso 2.4).
//! El trabajador elige en este orden:
//!
//! 1. Títulos nuevos, del webhook: son cortos (menos de un segundo) y Rubén
//!    los ve en la lista.
//! 2. Párrafos de un artículo abierto o guardado: Rubén está esperando.
//! 3. Títulos de la revisión periódica: pueden ser cientos y no corren prisa.
//!
//! Como cada párrafo es un trabajo aparte, un título nuevo espera como mucho
//! a que termine el párrafo en curso, no el artículo entero.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use atalaya_api::Id;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::{
    content::{self, SegmentJob},
    lang,
    miniflux::Miniflux,
    store::{self, FIELD_TITLE, SeenEntry},
    translations::Translations,
};

/// Idiomas que se traducen al español (T1). El chino llega más adelante.
pub(crate) const SOURCE_LANGS: &[&str] = &["en"];

/// Artículos por página al pedir la lista a Miniflux.
const PAGE_SIZE: usize = 100;

/// Variable de entorno que limita cuántos artículos, de los más nuevos,
/// mira la revisión periódica. Sin ella, los mira todos. Sirve para probar.
const BACKFILL_LIMIT_VAR: &str = "ATALAYA_BACKFILL_LIMIT";

/// Cada cuánto se repasan los títulos pendientes de traducir.
const REVIEW_EVERY: Duration = Duration::from_secs(60 * 60);

/// Un artículo visto, ya con el idioma detectado.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEntry {
    pub entry_id: Id,
    pub feed_id: Id,
    pub title: String,
    pub detected_lang: Option<String>,
}

impl NewEntry {
    /// Detecta el idioma ya aquí: así no hay que guardar el contenido.
    pub fn new(entry_id: Id, feed_id: Id, title: String, content_html: &str) -> Self {
        let detected_lang = lang::detect(&title, content_html);
        Self {
            entry_id,
            feed_id,
            title,
            detected_lang,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Lo nuevo, del webhook.
    Urgent,
    /// Lo pendiente, de la revisión periódica.
    Backlog,
}

#[derive(Debug)]
pub struct TitleJob {
    entry_id: Id,
    title: String,
}

struct Receivers {
    urgent: mpsc::UnboundedReceiver<TitleJob>,
    content: mpsc::UnboundedReceiver<SegmentJob>,
    backlog: mpsc::UnboundedReceiver<TitleJob>,
}

/// Las colas de lo que hay que traducir: dos de títulos y una de párrafos.
pub struct Queue {
    urgent: mpsc::UnboundedSender<TitleJob>,
    content: mpsc::UnboundedSender<SegmentJob>,
    backlog: mpsc::UnboundedSender<TitleJob>,
    /// Los extremos de lectura, hasta que [`Translations::start`] los toma.
    receivers: Mutex<Option<Receivers>>,
}

impl Queue {
    pub fn new() -> Self {
        let (urgent, urgent_rx) = mpsc::unbounded_channel();
        let (content, content_rx) = mpsc::unbounded_channel();
        let (backlog, backlog_rx) = mpsc::unbounded_channel();
        Self {
            urgent,
            content,
            backlog,
            receivers: Mutex::new(Some(Receivers {
                urgent: urgent_rx,
                content: content_rx,
                backlog: backlog_rx,
            })),
        }
    }
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

impl Translations {
    /// Pone en marcha el trabajador y la revisión periódica. Se llama una
    /// vez, desde `main`.
    pub fn start(&self) {
        let Some(receivers) = self.queue().receivers.lock().unwrap().take() else {
            tracing::warn!("la traducción de títulos ya estaba en marcha");
            return;
        };
        tokio::spawn(self.clone().run_worker(receivers));
        tokio::spawn(self.clone().review_forever());
    }

    /// Revisa los títulos al arrancar y luego cada hora. Así un título que se
    /// quedó sin traducir (llama-swap caído, un fallo de red) se recupera solo.
    async fn review_forever(self) {
        let mut ticker = tokio::time::interval(REVIEW_EVERY);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            // La primera vez, `tick` responde enseguida: así la de arranque es la primera.
            ticker.tick().await;
            self.review().await;
        }
    }

    /// Manda a traducir los títulos de un feed recién creado. Miniflux guarda
    /// sus artículos al crearlo, pero el webhook solo avisa cuando actualiza un
    /// feed: sin esto, esos títulos esperarían a la revisión periódica.
    pub fn submit_new_feed(&self, feed_id: Id) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut entries = Vec::new();
            loop {
                let offset = entries.len();
                let page = match this
                    .miniflux()
                    .feed_entries_page(feed_id, PAGE_SIZE, offset)
                    .await
                {
                    Ok(page) => page,
                    Err(e) => {
                        tracing::warn!(
                            feed = feed_id,
                            "feed nuevo: no se pudieron leer sus artículos: {}",
                            e.message
                        );
                        break;
                    }
                };
                let got = page.entries.len();
                entries.extend(page.entries.into_iter().map(|e| {
                    let content = e.content.unwrap_or_default();
                    NewEntry::new(e.id, e.feed_id, e.title, &content)
                }));
                if got < PAGE_SIZE {
                    break;
                }
            }
            // Si una página falla, lo leído hasta ahí se manda igual: el resto
            // lo recoge la revisión periódica.
            let seen = entries.len();
            let queued = this.submit_titles(entries, Priority::Urgent).await;
            tracing::info!(
                feed = feed_id,
                "feed nuevo: {seen} artículos vistos, {queued} títulos en cola"
            );
        });
    }

    /// Pone un párrafo del texto completo en cola. La respuesta llega por
    /// `job.reply`.
    pub(crate) fn queue_segment(&self, job: SegmentJob) {
        // Solo falla si el trabajador terminó, y no termina nunca. Si pasara,
        // `reply` se suelta y quien espera recibe un error.
        let _ = self.queue().content.send(job);
    }

    /// Guarda el idioma de esos artículos y pone en cola los títulos que hay
    /// que traducir: los que no están en español (T3) ni traducidos ya (T4).
    /// Devuelve cuántos puso en cola.
    pub async fn submit_titles(&self, entries: Vec<NewEntry>, priority: Priority) -> usize {
        if entries.is_empty() {
            return 0;
        }
        let jobs = self
            .store()
            .call(move |c| -> rusqlite::Result<Vec<TitleJob>> {
                let seen: Vec<SeenEntry> = entries
                    .iter()
                    .map(|e| SeenEntry {
                        entry_id: e.entry_id,
                        feed_id: e.feed_id,
                        detected_lang: e.detected_lang.clone(),
                    })
                    .collect();
                let langs = store::record_langs(c, &seen)?;
                let mut jobs = Vec::new();
                for e in entries {
                    let lang = langs.get(&e.entry_id).cloned().flatten();
                    let wanted = lang.is_some_and(|l| SOURCE_LANGS.contains(&l.as_str()));
                    if !wanted || e.title.trim().is_empty() {
                        continue;
                    }
                    let hash = store::source_hash(&e.title);
                    if !store::is_translated(c, e.entry_id, FIELD_TITLE, &hash)? {
                        jobs.push(TitleJob {
                            entry_id: e.entry_id,
                            title: e.title,
                        });
                    }
                }
                Ok(jobs)
            })
            .await;
        let jobs = match jobs {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("no se pudieron guardar los idiomas: {e}");
                return 0;
            }
        };
        let queue = match priority {
            Priority::Urgent => &self.queue().urgent,
            Priority::Backlog => &self.queue().backlog,
        };
        let n = jobs.len();
        for job in jobs {
            // Solo falla si el trabajador terminó, y no termina nunca.
            let _ = queue.send(job);
        }
        n
    }

    async fn run_worker(self, mut rx: Receivers) {
        loop {
            tokio::select! {
                biased;
                Some(job) = rx.urgent.recv() => self.translate_title(job).await,
                Some(job) = rx.content.recv() => self.translate_segment(job).await,
                Some(job) = rx.backlog.recv() => self.translate_title(job).await,
                else => break,
            }
        }
    }

    async fn translate_segment(&self, job: SegmentJob) {
        // El artículo ya falló o se abandonó: no gastar GPU en él.
        if job.reply.is_closed() {
            return;
        }
        let result = content::translate_segment(self.llama(), &job.html).await;
        let failed = result.is_err();
        let _ = job.reply.send(result);
        if failed {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn translate_title(&self, job: TitleJob) {
        let id = job.entry_id;
        let hash = store::source_hash(&job.title);
        // Pudo entrar dos veces en cola, por el webhook y por la revisión.
        let check = hash.clone();
        let done = self
            .store()
            .call(move |c| store::is_translated(c, id, FIELD_TITLE, &check))
            .await;
        if matches!(done, Ok(true)) {
            return;
        }

        let started = Instant::now();
        let result = self.llama().translate_title(&job.title).await;
        let secs = started.elapsed().as_secs_f32();
        match &result {
            Ok(t) => tracing::debug!(entry = id, secs, "título traducido: {t}"),
            Err(e) => tracing::warn!(entry = id, secs, "no se pudo traducir el título: {e}"),
        }
        let failed = result.is_err();
        let model = self.llama().model().to_owned();
        let saved = self
            .store()
            .call(move |c| store::save_translation(c, id, FIELD_TITLE, &hash, &model, &result))
            .await;
        if let Err(e) = saved {
            tracing::error!(entry = id, "no se pudo guardar la traducción: {e}");
        }
        if failed {
            // Si llama-swap está caído, no vaciar la cola a fallos seguidos.
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Busca en Miniflux los artículos, del más nuevo al más viejo, y manda
    /// a traducir los títulos que falten.
    async fn review(&self) {
        let miniflux = self.miniflux();
        let limit = std::env::var(BACKFILL_LIMIT_VAR)
            .ok()
            .and_then(|v| v.parse::<usize>().ok());
        let mut entries = Vec::new();
        let mut offset = 0;
        loop {
            let page_size = limit.map_or(PAGE_SIZE, |l| PAGE_SIZE.min(l - entries.len()));
            if page_size == 0 {
                break;
            }
            let Some(page) = miniflux.entries_page_with_retry(page_size, offset).await else {
                return;
            };
            let got = page.entries.len();
            entries.extend(page.entries.into_iter().map(|e| {
                let content = e.content.unwrap_or_default();
                NewEntry::new(e.id, e.feed_id, e.title, &content)
            }));
            if got < page_size {
                break;
            }
            offset += got;
        }
        let seen = entries.len();
        let queued = self.submit_titles(entries, Priority::Backlog).await;
        tracing::info!("revisión de títulos: {seen} artículos vistos, {queued} títulos en cola");
    }
}

#[derive(Debug, Deserialize)]
pub struct MinifluxEntryPage {
    pub entries: Vec<MinifluxEntry>,
}

/// Lo que se usa de un artículo de Miniflux, en su API y en su webhook.
#[derive(Debug, Deserialize)]
pub struct MinifluxEntry {
    pub id: Id,
    pub feed_id: Id,
    pub title: String,
    pub content: Option<String>,
}

impl Miniflux {
    /// Una página de artículos, de todos los estados, del más nuevo al más viejo.
    async fn entries_page(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<MinifluxEntryPage, crate::error::AppError> {
        self.get(&format!(
            "/v1/entries?order=id&direction=desc&limit={limit}&offset={offset}"
        ))
        .await
    }

    /// Una página de artículos de un feed, del más nuevo al más viejo.
    async fn feed_entries_page(
        &self,
        feed_id: Id,
        limit: usize,
        offset: usize,
    ) -> Result<MinifluxEntryPage, crate::error::AppError> {
        self.get(&format!(
            "/v1/feeds/{feed_id}/entries?order=id&direction=desc&limit={limit}&offset={offset}"
        ))
        .await
    }

    /// Como [`Self::entries_page`], pero espera a Miniflux si aún arranca.
    async fn entries_page_with_retry(
        &self,
        limit: usize,
        offset: usize,
    ) -> Option<MinifluxEntryPage> {
        const ATTEMPTS: u32 = 5;
        for attempt in 1..=ATTEMPTS {
            match self.entries_page(limit, offset).await {
                Ok(page) => return Some(page),
                Err(e) if attempt < ATTEMPTS => {
                    tracing::warn!("revisión de títulos: {}. Reintento en 15 s", e.message);
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
                Err(e) => {
                    tracing::error!("revisión de títulos abandonada: {}", e.message);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{llama::Llama, store::Store};

    fn translations() -> Translations {
        let http = reqwest::Client::new();
        let llama = Llama::new(http.clone(), "http://127.0.0.1:9", "x");
        let miniflux = Miniflux::new(http, "http://127.0.0.1:9", "x");
        Translations::new(Store::open_in_memory(), llama, miniflux)
    }

    fn entry(id: Id, feed: Id, title: &str, lang: Option<&str>) -> NewEntry {
        NewEntry {
            entry_id: id,
            feed_id: feed,
            title: title.into(),
            detected_lang: lang.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn queues_only_english_titles_not_yet_translated() {
        let tr = translations();
        let queued = tr
            .submit_titles(
                vec![
                    entry(1, 1, "Hello", Some("en")),
                    entry(2, 3, "Hola", Some("es")),
                    entry(3, 1, "Founders", None), // toma el inglés del feed
                    entry(4, 2, "???", None),      // sin idioma: no se traduce
                ],
                Priority::Urgent,
            )
            .await;
        assert_eq!(queued, 2);

        let hash = store::source_hash("Hello");
        tr.store()
            .call(move |c| {
                store::save_translation(c, 1, FIELD_TITLE, &hash, "m", &Ok("Hola".into()))
            })
            .await
            .unwrap();
        let queued = tr
            .submit_titles(vec![entry(1, 1, "Hello", Some("en"))], Priority::Backlog)
            .await;
        assert_eq!(queued, 0, "nada se traduce dos veces");

        let infos = tr.titles(&[1, 2, 3, 4, 5]).await;
        assert_eq!(infos[&1].title.as_deref(), Some("Hola"));
        assert_eq!(infos[&2].lang.as_deref(), Some("es"));
        assert_eq!(infos[&2].title, None);
        assert_eq!(infos[&3].lang.as_deref(), Some("en"));
        assert_eq!(infos[&4].lang, None);
        assert!(!infos.contains_key(&5));
    }
}
