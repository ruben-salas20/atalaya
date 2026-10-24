//! `POST /webhook/miniflux`: Miniflux avisa de los artículos nuevos.
//!
//! Va fuera de `/api` porque Miniflux no conoce la clave de las apps. En su
//! lugar, Miniflux 2.3.4 firma cada aviso: la cabecera `X-Miniflux-Signature`
//! lleva el HMAC-SHA256 del cuerpo, en hexadecimal, con el secreto que muestra
//! su página de integraciones (`MINIFLUX_WEBHOOK_SECRET`). La cabecera
//! `X-Miniflux-Event-Type` dice el evento: `new_entries` o `save_entry`.
//!
//! Responde enseguida. La traducción sigue en segundo plano.

use atalaya_api::Id;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::{
    AppState,
    error::{AppError, AppResult},
    titles::{MinifluxEntry, NewEntry, Priority},
};

/// Un feed nuevo con el crawler activo puede mandar muchos artículos
/// completos de una vez: el límite de 2 MB de axum se queda corto.
const MAX_BODY: usize = 64 * 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/webhook/miniflux", post(miniflux))
        .layer(DefaultBodyLimit::max(MAX_BODY))
}

#[derive(Debug, Deserialize)]
struct Event {
    event_type: String,
    #[serde(default)]
    entries: Vec<MinifluxEntry>,
    /// Con `save_entry`, Miniflux manda el artículo guardado en este campo.
    #[serde(default)]
    entry: Option<MinifluxEntry>,
}

async fn miniflux(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<StatusCode> {
    let secret = &state.config.miniflux_webhook_secret;
    if secret.is_empty() {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "el webhook está desactivado: falta MINIFLUX_WEBHOOK_SECRET",
        ));
    }
    let signature = headers
        .get("X-Miniflux-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !verify_signature(secret, &body, signature) {
        return Err(AppError::new(
            StatusCode::UNAUTHORIZED,
            "la firma del webhook no es correcta",
        ));
    }

    let event: Event = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(format!("aviso de Miniflux no válido: {e}")))?;
    if event.event_type == "save_entry" {
        // Guardar en la web de Miniflux traduce el texto igual que hacen las
        // apps al guardar. No toca títulos.
        let saved: Vec<Id> = event.entry.iter().map(|e| e.id).collect();
        let translations = state.translations.clone();
        tokio::spawn(async move {
            for id in saved {
                let (translation, _) = translations.content(id).await;
                tracing::debug!(entry = id, ?translation, "webhook save_entry");
            }
        });
        return Ok(StatusCode::NO_CONTENT);
    }
    if event.event_type != "new_entries" {
        tracing::debug!("webhook {} ignorado", event.event_type);
        return Ok(StatusCode::NO_CONTENT);
    }

    tracing::info!("webhook: {} artículos nuevos", event.entries.len());
    let translations = state.translations.clone();
    tokio::spawn(async move {
        let entries = event
            .entries
            .into_iter()
            .map(|e| {
                let content = e.content.unwrap_or_default();
                NewEntry::new(e.id, e.feed_id, e.title, &content)
            })
            .collect();
        let queued = translations.submit_titles(entries, Priority::Urgent).await;
        tracing::debug!("webhook: {queued} títulos en cola");
    });
    Ok(StatusCode::NO_CONTENT)
}

/// Compara la firma en tiempo constante.
pub fn verify_signature(secret: &str, body: &[u8], signature_hex: &str) -> bool {
    let Ok(signature) = hex::decode(signature_hex.trim()) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Firma como Miniflux: `crypto.GenerateSHA256Hmac`.
    pub fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn accepts_only_the_right_signature() {
        let body = br#"{"event_type":"new_entries"}"#;
        let good = sign("secreto", body);
        assert!(verify_signature("secreto", body, &good));
        assert!(verify_signature("secreto", body, &good.to_uppercase()));
        assert!(!verify_signature("otro", body, &good));
        assert!(!verify_signature("secreto", b"{}", &good));
        assert!(!verify_signature("secreto", body, ""));
        assert!(!verify_signature("secreto", body, "no es hex"));
    }

    #[test]
    fn known_vector() {
        // RFC 4231, caso 2.
        assert!(verify_signature(
            "Jefe",
            b"what do ya want for nothing?",
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
        ));
    }

    #[test]
    fn save_entry_carries_the_article() {
        let event: Event = serde_json::from_str(
            r#"{"event_type":"save_entry","entry":{"id":42,"feed_id":3,"title":"Hola","content":"<p>x</p>"}}"#,
        )
        .unwrap();
        assert_eq!(event.event_type, "save_entry");
        assert_eq!(event.entry.map(|e| e.id), Some(42));
        assert!(event.entries.is_empty());
    }
}
