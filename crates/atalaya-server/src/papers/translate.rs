//! Traducción de búsquedas y títulos de papers (P1, P3).
//!
//! Aquí Rubén está esperando la respuesta, así que no se usa la cola de
//! `titles`, que puede estar ocupada con la revisión al arrancar. Se habla
//! con llama-swap directamente, con el mismo modelo. llama-server atiende
//! varias peticiones a la vez del mismo modelo, así que no hay cambio de
//! modelo ni choque en la GPU.
//!
//! Medido con `gemma-4-e4b-rapido` y 20 títulos de arXiv, ya cargado:
//!
//! | Cómo | Tiempo |
//! |---|---|
//! | Uno a uno, en fila | 10,1 s |
//! | Uno a uno, 4 a la vez | 5,6 s |
//! | Los 20 en una petición | 7,2 s |
//! | De 5 en 5, 4 peticiones a la vez | 3,6 s |
//!
//! Por eso los títulos van de 5 en 5 y en paralelo. Si una respuesta no trae
//! todas las líneas, los que faltan se traducen uno a uno. En lote, el modelo
//! copia las mayúsculas del inglés («Modelos Del Mundo») si la instrucción no
//! lleva un ejemplo.

use std::collections::HashMap;

use tokio::task::JoinSet;

use super::{
    Papers,
    db::{self, FIELD_TITLE},
};
use crate::{
    error::{AppError, AppResult},
    lang,
    llama::clean_title,
    store,
};

/// Títulos por petición a llama-swap.
pub const TITLES_PER_REQUEST: usize = 5;

/// Peticiones a llama-swap a la vez desde la búsqueda de papers.
pub const PARALLEL_REQUESTS: usize = 4;

const TITLES_PROMPT: &str = "Translate each of the following numbered titles of scientific papers from English into Latin American Spanish. \
Use Spanish capitalization: capitalize only the first word and proper nouns, even if the English title capitalizes every word. \
Keep technical terms, acronyms and model names that are usually left in English. \
Reply with exactly one line per title, in the same order, each starting with its number, a period and a space, and nothing else.\n\n\
For example, \"Efficient Training of Large Language Models on Edge Devices\" becomes \
\"Entrenamiento eficiente de modelos de lenguaje grandes en dispositivos de borde\".\n\n";

const QUERY_PROMPT: &str = "Translate the following search query for scientific papers from Spanish into English. \
If it is already in English, repeat it unchanged. Keep acronyms, technical terms and names as they are. \
Reply with the English query only, without quotes, notes or explanations.\n\n";

/// Un título por traducir.
#[derive(Debug, Clone)]
pub struct TitleToTranslate {
    pub arxiv_id: String,
    pub title: String,
    /// Para detectar si el paper ya está en español.
    pub abstract_text: String,
}

/// La clave de la caché de búsquedas: minúsculas y espacios normalizados.
pub fn query_key(q: &str) -> String {
    q.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

impl Papers {
    /// La búsqueda en inglés. Usa la caché si ya se tradujo antes.
    pub async fn english_query(&self, q: &str) -> AppResult<String> {
        let q = q.split_whitespace().collect::<Vec<_>>().join(" ");
        if q.is_empty() {
            return Err(AppError::bad_request("la búsqueda está vacía"));
        }
        let key = query_key(&q);
        let lookup = key.clone();
        let cached = self
            .store()
            .call(move |c| db::query_translation(c, &lookup))
            .await
            .map_err(|e| {
                AppError::new(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("no se pudo leer la caché de búsquedas: {e}"),
                )
            })?;
        if let Some(english) = cached {
            return Ok(english);
        }

        let llama = self.llama();
        let raw = {
            let _permit = self.gpu_permit().await;
            llama.complete(&format!("{QUERY_PROMPT}{q}"), 128).await
        }
        .map_err(|e| AppError::bad_gateway(format!("no se pudo traducir la búsqueda: {e}")))?;
        let english = clean_title(&q, &raw)
            .ok_or_else(|| AppError::bad_gateway("la traducción de la búsqueda salió vacía"))?;

        let model = llama.model().to_owned();
        let save = english.clone();
        if let Err(e) = self
            .store()
            .call(move |c| db::save_query_translation(c, &key, &save, &model))
            .await
        {
            tracing::error!("no se pudo guardar la búsqueda traducida: {e}");
        }
        Ok(english)
    }

    /// Títulos en español de esos papers, por identificador. Usa los ya
    /// traducidos y traduce el resto. Los que están en español, o cuya
    /// traducción falla, no salen en el mapa.
    pub async fn titles(&self, papers: Vec<TitleToTranslate>) -> HashMap<String, String> {
        let papers: Vec<TitleToTranslate> = papers
            .into_iter()
            .filter(|p| {
                !p.title.trim().is_empty()
                    && lang::detect(&p.title, &p.abstract_text).as_deref() != Some("es")
            })
            .collect();
        let wanted: Vec<(String, String)> = papers
            .iter()
            .map(|p| (p.arxiv_id.clone(), store::source_hash(&p.title)))
            .collect();
        let mut done = match self
            .store()
            .call(move |c| db::done_titles(c, &wanted))
            .await
        {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("no se pudieron leer los títulos de papers: {e}");
                HashMap::new()
            }
        };
        let missing: Vec<TitleToTranslate> = papers
            .into_iter()
            .filter(|p| !done.contains_key(&p.arxiv_id))
            .collect();
        if missing.is_empty() {
            return done;
        }

        let started = std::time::Instant::now();
        let n = missing.len();
        let mut tasks = JoinSet::new();
        for chunk in missing.chunks(TITLES_PER_REQUEST) {
            let this = self.clone();
            let chunk = chunk.to_vec();
            tasks.spawn(async move { this.translate_chunk(chunk).await });
        }
        let mut results = Vec::new();
        while let Some(r) = tasks.join_next().await {
            match r {
                Ok(r) => results.extend(r),
                Err(e) => tracing::error!("la traducción de títulos se interrumpió: {e}"),
            }
        }
        let ok = results.iter().filter(|(_, _, r)| r.is_ok()).count();
        tracing::info!(
            secs = started.elapsed().as_secs_f32(),
            "papers: {ok} de {n} títulos traducidos"
        );

        let model = self.llama().model().to_owned();
        let to_save = results.clone();
        if let Err(e) = self
            .store()
            .call(move |c| -> rusqlite::Result<()> {
                for (id, title, result) in &to_save {
                    let hash = store::source_hash(title);
                    db::save_translation(c, id, FIELD_TITLE, &hash, &model, Some(result))?;
                }
                Ok(())
            })
            .await
        {
            tracing::error!("no se pudieron guardar los títulos de papers: {e}");
        }
        for (id, _, result) in results {
            if let Ok(t) = result {
                done.insert(id, t);
            }
        }
        done
    }

    /// Traduce unos pocos títulos en una sola petición. Devuelve, por cada
    /// uno, su identificador, el original y el resultado.
    async fn translate_chunk(
        &self,
        chunk: Vec<TitleToTranslate>,
    ) -> Vec<(String, String, Result<String, String>)> {
        let llama = self.llama();
        let list: Vec<String> = chunk
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{}. {}", i + 1, p.title))
            .collect();
        let prompt = format!("{TITLES_PROMPT}{}", list.join("\n"));
        let max_tokens = 120 * chunk.len() as u32;
        let raw = {
            let _permit = self.gpu_permit().await;
            llama.complete(&prompt, max_tokens).await
        };
        let lines = match &raw {
            Ok(raw) => numbered_lines(raw, chunk.len()),
            Err(e) => {
                tracing::warn!("títulos de papers en lote: {e}");
                vec![None; chunk.len()]
            }
        };

        let mut out = Vec::with_capacity(chunk.len());
        for (p, line) in chunk.into_iter().zip(lines) {
            let result = match line.and_then(|l| clean_title(&p.title, &l)) {
                Some(t) => Ok(t),
                // Si llama-swap no respondió, uno a uno tampoco: no insistir.
                None if raw.is_err() => Err(raw.as_ref().unwrap_err().clone()),
                None => {
                    let _permit = self.gpu_permit().await;
                    llama.translate_title(&p.title).await
                }
            };
            out.push((p.arxiv_id, p.title, result));
        }
        out
    }
}

/// Las líneas `1. …` a `n. …` de la respuesta, en orden. Las que faltan o
/// se repiten quedan en `None`.
pub fn numbered_lines(raw: &str, n: usize) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = vec![None; n];
    let mut seen = vec![false; n];
    for line in raw.lines() {
        let line = line.trim().trim_start_matches(['*', '-', ' ']);
        let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
        let Ok(num) = digits.parse::<usize>() else {
            continue;
        };
        let rest = line[digits.len()..].trim_start();
        let Some(rest) = rest.strip_prefix(['.', ')', ':']) else {
            continue;
        };
        if num == 0 || num > n {
            continue;
        }
        let text = rest.trim().trim_matches('*').trim();
        if seen[num - 1] {
            out[num - 1] = None;
        } else if !text.is_empty() {
            out[num - 1] = Some(text.to_owned());
        }
        seen[num - 1] = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numbered_lines() {
        let raw = "Aquí están:\n1. Uno\n2) Dos\n\n**3.** Tres\n5. Cinco de más";
        assert_eq!(
            numbered_lines(raw, 4),
            [
                Some("Uno".into()),
                Some("Dos".into()),
                Some("Tres".into()),
                None
            ]
        );
        // Un número repetido no se puede atribuir.
        assert_eq!(
            numbered_lines("1. A\n1. B\n2. C", 2),
            [None, Some("C".into())]
        );
    }

    #[test]
    fn query_keys_ignore_case_and_spaces() {
        assert_eq!(query_key("  Modelos   de Lenguaje "), "modelos de lenguaje");
    }
}
