//! Traducción del resumen de un paper (paso 4.2, P4).
//!
//! Como el texto de un artículo (ADR 0003): abrir el paper la pone en marcha
//! y responde enseguida con `Pending`; la app vuelve a preguntar. Cada
//! párrafo pasa por [`content::translate_segment`], con sus comprobaciones.

use std::time::{Duration, Instant};

use atalaya_api::TranslationState;

use super::{
    Papers, abstract_math,
    arxiv::ArxivPaper,
    db::{self, FIELD_ABSTRACT},
    full,
};
use crate::{
    content, lang,
    store::{self, STATE_DONE, STATE_FAILED},
};

/// Tras un fallo, abrir el paper otra vez no reintenta hasta pasado este tiempo.
const RETRY_FAILED_AFTER_SECS: i64 = 600;

/// Cuánto espera [`Papers::wait_abstract`] como mucho.
const WAIT_LIMIT: Duration = Duration::from_secs(600);

/// Saca el paper de los que están en curso al terminar, también si falla.
struct Running {
    papers: Papers,
    arxiv_id: String,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.papers.abstracts_running().remove(&self.arxiv_id);
    }
}

/// Si el paper ya está en español y no hay que traducir nada.
pub fn is_spanish(paper: &ArxivPaper) -> bool {
    lang::detect(&paper.title, &paper.abstract_text).as_deref() == Some("es")
}

impl Papers {
    /// Estado de la traducción del resumen y, si está hecha, el texto. Si
    /// hace falta y no ha empezado, la pone en marcha sin esperarla.
    pub async fn abstract_state(&self, paper: &ArxivPaper) -> (TranslationState, Option<String>) {
        if is_spanish(paper) {
            return (TranslationState::NotNeeded, None);
        }
        if self.abstracts_running().contains(&paper.arxiv_id) {
            return (TranslationState::Pending, None);
        }
        let hash = store::source_hash(&paper.abstract_text);
        let id = paper.arxiv_id.clone();
        let row = self
            .store()
            .call(move |c| db::translation(c, &id, FIELD_ABSTRACT))
            .await;
        match row {
            Ok(Some(row)) if row.source_hash == hash => match row.state.as_str() {
                STATE_DONE => return (TranslationState::Done, row.text),
                STATE_FAILED if row.age_secs < RETRY_FAILED_AFTER_SECS => {
                    return (TranslationState::Failed, None);
                }
                // `pending` sin trabajo en marcha: el servicio se reinició.
                _ => {}
            },
            Ok(_) => {}
            Err(e) => {
                tracing::error!(paper = paper.arxiv_id, "estado del resumen: {e}");
                return (TranslationState::Failed, None);
            }
        }

        if self.abstracts_running().insert(paper.arxiv_id.clone()) {
            let running = Running {
                papers: self.clone(),
                arxiv_id: paper.arxiv_id.clone(),
            };
            tokio::spawn(
                self.clone()
                    .translate_abstract(paper.clone(), hash, running),
            );
        }
        (TranslationState::Pending, None)
    }

    /// Espera a que el resumen esté traducido (o falle), y devuelve el texto.
    /// La pone en marcha si hace falta. `None` si está en español, si falla o
    /// si tarda demasiado.
    pub async fn wait_abstract(&self, paper: &ArxivPaper) -> Option<String> {
        let started = Instant::now();
        loop {
            match self.abstract_state(paper).await {
                (TranslationState::Done, text) => return text,
                (TranslationState::Pending, _) if started.elapsed() < WAIT_LIMIT => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                _ => return None,
            }
        }
    }

    async fn translate_abstract(self, paper: ArxivPaper, hash: String, _running: Running) {
        let id = paper.arxiv_id.clone();
        let model = self.llama().model().to_owned();
        let (pid, phash, pmodel) = (id.clone(), hash.clone(), model.clone());
        if let Err(e) = self
            .store()
            .call(move |c| db::save_translation(c, &pid, FIELD_ABSTRACT, &phash, &pmodel, None))
            .await
        {
            tracing::error!(
                paper = id,
                "no se pudo guardar el comienzo del resumen: {e}"
            );
        }

        let started = Instant::now();
        let result = self.translate_paragraphs(&paper.abstract_text).await;
        let secs = started.elapsed().as_secs_f32();
        match &result {
            Ok(_) => tracing::info!(paper = id, secs, "resumen traducido"),
            Err(e) => tracing::warn!(paper = id, secs, "no se pudo traducir el resumen: {e}"),
        }
        let sid = id.clone();
        if let Err(e) = self
            .store()
            .call(move |c| {
                db::save_translation(c, &sid, FIELD_ABSTRACT, &hash, &model, Some(&result))
            })
            .await
        {
            tracing::error!(paper = id, "no se pudo guardar el resumen traducido: {e}");
        }
    }

    /// Traduce el texto párrafo a párrafo. Un párrafo que sale mal se queda
    /// en inglés; si salen mal todos, es un error.
    async fn translate_paragraphs(&self, text: &str) -> Result<String, String> {
        let paragraphs: Vec<&str> = text
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .collect();
        let mut out = Vec::with_capacity(paragraphs.len());
        let mut kept = 0;
        for p in &paragraphs {
            let segment = if abstract_math::formulas(p).is_empty() {
                let _permit = self.gpu_permit().await;
                content::translate_segment(self.llama(), p).await?
            } else {
                // Con fórmulas, cada una va como marca y el modelo no toca su
                // LaTeX (#80, ADR 0010). Vuelve como texto con `$…$`.
                let marked = abstract_math::marked(p);
                let mut segment = {
                    let _permit = self.gpu_permit().await;
                    full::translate_paragraph(self.llama(), &marked).await?
                };
                segment.html = if segment.translated {
                    abstract_math::unmarked(&segment.html, p)
                } else {
                    (*p).to_owned()
                };
                segment
            };
            if !segment.translated {
                kept += 1;
            }
            out.push(segment.html);
        }
        if !paragraphs.is_empty() && kept == paragraphs.len() {
            return Err(format!("ninguno de los {kept} párrafos salió bien"));
        }
        Ok(out.join("\n\n"))
    }
}
