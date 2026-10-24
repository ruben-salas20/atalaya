//! Cliente de llama-swap, por su API compatible con OpenAI (ADR 0004, 0009).

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

/// Tiempo máximo de una petición. Incluye la carga del modelo en frío, que
/// tarda unos 30 s (ADR 0009).
const TIMEOUT: Duration = Duration::from_secs(180);

/// Espera antes de repetir una petición que llama-swap rechazó con 502 o 503.
/// Suele ser un cambio de modelo en curso, que termina en pocos segundos.
const RETRY_WAIT: Duration = Duration::from_secs(3);

/// Instrucción para un título. Parte de la plantilla `gemma` de
/// `bench/traduccion/plantillas.json`, con lo que pide el ADR 0009: español
/// de Latinoamérica y mayúsculas solo al principio y en nombres propios.
const TITLE_PROMPT: &str = "Translate the following headline from English into Latin American Spanish. \
Use Spanish capitalization: capitalize only the first word and proper nouns, even if the English headline capitalizes every word. \
Reply with the Spanish translation only, without notes, explanations or quotes.\n\n";

/// Error de [`Llama::complete`] cuando la respuesta no cupo en `max_tokens`.
/// No es un fallo de llama-swap: el mismo texto puede salir bien otra vez.
pub const CUT_OFF: &str = "la respuesta se cortó por larga";

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
    /// Si llama-swap responde 502 o 503, espera un poco y lo intenta una vez más.
    pub async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, String> {
        self.complete_after(prompt, max_tokens, RETRY_WAIT).await
    }

    async fn complete_after(
        &self,
        prompt: &str,
        max_tokens: u32,
        wait: Duration,
    ) -> Result<String, String> {
        let mut attempt = 1;
        loop {
            match self.send_once(prompt, max_tokens).await {
                Err(Failure::Busy(e)) if attempt == 1 => {
                    tracing::warn!("llama-swap ocupado, se reintenta: {e}");
                    attempt += 1;
                    tokio::time::sleep(wait).await;
                }
                Err(e) => return Err(e.into_message()),
                Ok(text) => return Ok(text),
            }
        }
    }

    async fn send_once(&self, prompt: &str, max_tokens: u32) -> Result<String, Failure> {
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
            .map_err(|e| Failure::Other(format!("no se pudo hablar con llama-swap: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let text: String = text.chars().take(300).collect();
            let message = format!("llama-swap respondió {status}: {text}");
            return Err(if matches!(status.as_u16(), 502 | 503) {
                Failure::Busy(message)
            } else {
                Failure::Other(message)
            });
        }
        let resp: Response = resp
            .json()
            .await
            .map_err(|e| Failure::Other(format!("respuesta de llama-swap no válida: {e}")))?;
        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| Failure::Other("llama-swap respondió sin texto".into()))?;
        if choice.finish_reason.as_deref() == Some("length") {
            return Err(Failure::Other(CUT_OFF.into()));
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

/// Por qué falló una petición. Solo `Busy` se reintenta.
enum Failure {
    /// llama-swap respondió 502 o 503.
    Busy(String),
    Other(String),
}

impl Failure {
    fn into_message(self) -> String {
        match self {
            Failure::Busy(m) | Failure::Other(m) => m,
        }
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

    /// Servidor de pruebas: responde en orden con los `(estado, cuerpo)` dados
    /// y devuelve la dirección.
    fn fake_server(replies: Vec<(&'static str, &'static str)>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for (status, body) in replies {
                let Ok((mut conn, _)) = listener.accept() else {
                    return;
                };
                // Se lee la petición entera antes de contestar.
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    let n = conn.read(&mut chunk).unwrap_or(0);
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(head_end) = text.find("\r\n\r\n") {
                        let len = text[..head_end]
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length: ")
                                    .or_else(|| l.strip_prefix("Content-Length: "))
                            })
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if buf.len() >= head_end + 4 + len {
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                let resp = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = conn.write_all(resp.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    const ANSWER: &str = r#"{"choices":[{"message":{"content":"Hola"},"finish_reason":"stop"}]}"#;

    #[tokio::test]
    async fn retries_once_when_busy() {
        let url = fake_server(vec![("503 Service Unavailable", "{}"), ("200 OK", ANSWER)]);
        let llama = Llama::new(reqwest::Client::new(), &url, "m");
        let text = llama
            .complete_after("hola", 16, Duration::from_millis(10))
            .await;
        assert_eq!(text.as_deref(), Ok("Hola"));
    }

    #[tokio::test]
    async fn gives_up_after_second_busy_answer() {
        let url = fake_server(vec![
            ("502 Bad Gateway", "{}"),
            ("503 Service Unavailable", "{}"),
        ]);
        let llama = Llama::new(reqwest::Client::new(), &url, "m");
        let err = llama
            .complete_after("hola", 16, Duration::from_millis(10))
            .await
            .unwrap_err();
        assert!(err.starts_with("llama-swap respondió 503"), "{err}");
    }

    #[tokio::test]
    async fn does_not_retry_other_errors() {
        let url = fake_server(vec![("500 Internal Server Error", "{}")]);
        let llama = Llama::new(reqwest::Client::new(), &url, "m");
        let err = llama
            .complete_after("hola", 16, Duration::from_millis(10))
            .await
            .unwrap_err();
        assert!(err.starts_with("llama-swap respondió 500"), "{err}");
    }
}
