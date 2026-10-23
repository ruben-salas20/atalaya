//! Cliente de llama-swap, por su API compatible con OpenAI (ADR 0004, 0009).

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

/// Tiempo máximo de una petición. Incluye la carga del modelo en frío, que
/// tarda unos 30 s (ADR 0009).
const TIMEOUT: Duration = Duration::from_secs(180);

/// Instrucción para un título. Parte de la plantilla `gemma` de
/// `bench/traduccion/plantillas.json`, con lo que pide el ADR 0009: español
/// de Latinoamérica y mayúsculas solo al principio y en nombres propios.
const TITLE_PROMPT: &str = "Translate the following headline from English into Latin American Spanish. \
Use Spanish capitalization: capitalize only the first word and proper nouns, even if the English headline capitalizes every word. \
Reply with the Spanish translation only, without notes, explanations or quotes.\n\n";

#[derive(Debug, Clone)]
pub struct Llama {
    http: reqwest::Client,
    url: String,
    model: String,
}

impl Llama {
    pub fn new(http: reqwest::Client, url: &str, model: &str) -> Self {
        Self {
            http,
            url: url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Manda una instrucción y devuelve la respuesta del modelo, sin tocar.
    pub async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Response {
            choices: Vec<Choice>,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: Message,
            finish_reason: Option<String>,
        }
        #[derive(Deserialize)]
        struct Message {
            content: Option<String>,
        }

        let body = json!({
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false,
            "max_tokens": max_tokens,
            "temperature": 0.2,
        });
        let resp = self
            .http
            .post(format!("{}/v1/chat/completions", self.url))
            .json(&body)
            .timeout(TIMEOUT)
            .send()
            .await
            .map_err(|e| format!("no se pudo hablar con llama-swap: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let text: String = text.chars().take(300).collect();
            return Err(format!("llama-swap respondió {status}: {text}"));
        }
        let resp: Response = resp
            .json()
            .await
            .map_err(|e| format!("respuesta de llama-swap no válida: {e}"))?;
        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or("llama-swap respondió sin texto")?;
        if choice.finish_reason.as_deref() == Some("length") {
            return Err("la respuesta se cortó por larga".into());
        }
        Ok(choice.message.content.unwrap_or_default())
    }

    /// Traduce un título al español. Si la respuesta no parece un título, lo
    /// intenta otra vez.
    pub async fn translate_title(&self, title: &str) -> Result<String, String> {
        let prompt = format!("{TITLE_PROMPT}{title}");
        let mut last_error = String::new();
        for _ in 0..2 {
            match self.complete(&prompt, 256).await {
                Ok(raw) => match clean_title(title, &raw) {
                    Some(t) => return Ok(t),
                    None => last_error = format!("respuesta no válida: {raw:?}"),
                },
                // Si llama-swap no responde, reintentar enseguida no sirve.
                Err(e) => return Err(e),
            }
        }
        Err(last_error)
    }
}

/// Limpia la respuesta del modelo: primera línea con texto, sin comillas
/// alrededor que el original no tuviera. `None` si está vacía o es demasiado
/// larga para ser la traducción de ese título.
pub fn clean_title(original: &str, raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut t = line;
    for (open, close) in [("\"", "\""), ("“", "”"), ("«", "»"), ("'", "'"), ("*", "*")] {
        let orig = original.trim();
        if t.len() > open.len() + close.len()
            && t.starts_with(open)
            && t.ends_with(close)
            && !(orig.starts_with(open) && orig.ends_with(close))
        {
            t = t[open.len()..t.len() - close.len()].trim();
        }
    }
    let original_len = original.chars().count();
    if t.is_empty() || t.chars().count() > original_len * 3 + 40 {
        return None;
    }
    Some(t.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_quotes_and_extra_lines() {
        assert_eq!(
            clean_title("Hello world", "\"Hola mundo\"\n\nNota: traducido.").as_deref(),
            Some("Hola mundo")
        );
        assert_eq!(clean_title("Hello", "  «Hola»  ").as_deref(), Some("Hola"));
        // Si el original ya va entre comillas, se respetan.
        assert_eq!(
            clean_title("\"Hello\"", "\"Hola\"").as_deref(),
            Some("\"Hola\"")
        );
    }

    #[test]
    fn rejects_empty_or_rambling_answers() {
        assert_eq!(clean_title("Hi", "   \n  "), None);
        assert_eq!(clean_title("Hi", &"palabra ".repeat(40)), None);
    }
}
