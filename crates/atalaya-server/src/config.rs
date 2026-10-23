//! Configuración, leída de variables de entorno al arrancar.
//!
//! | Variable | Por defecto | Qué es |
//! |---|---|---|
//! | `ATALAYA_API_KEY` | obligatoria | Clave que deben mandar las apps |
//! | `ATALAYA_BIND` | `0.0.0.0:8080` | Dirección en la que escucha |
//! | `ATALAYA_DATA_DIR` | `/data` | Carpeta de la base SQLite |
//! | `MINIFLUX_URL` | `http://miniflux:8080` | Miniflux, por la red interna de Docker |
//! | `MINIFLUX_API_KEY` | obligatoria | Clave de la API de Miniflux |
//! | `MINIFLUX_WEBHOOK_SECRET` | vacía | Secreto con el que Miniflux firma el webhook |
//! | `LLAMA_URL` | `http://llama-swap:8080` | llama-swap, por la red interna |
//! | `LLAMA_MODEL` | `gemma-4-e4b-rapido` | Modelo que traduce (ADR 0009) |

use std::{env, net::SocketAddr, path::PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub miniflux_url: String,
    pub miniflux_api_key: String,
    pub miniflux_webhook_secret: String,
    pub llama_url: String,
    pub llama_model: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            api_key: required("ATALAYA_API_KEY")?,
            bind: optional("ATALAYA_BIND", "0.0.0.0:8080")
                .parse()
                .map_err(|e| format!("ATALAYA_BIND no es una dirección válida: {e}"))?,
            data_dir: optional("ATALAYA_DATA_DIR", "/data").into(),
            miniflux_url: optional("MINIFLUX_URL", "http://miniflux:8080"),
            miniflux_api_key: required("MINIFLUX_API_KEY")?,
            miniflux_webhook_secret: optional("MINIFLUX_WEBHOOK_SECRET", ""),
            llama_url: optional("LLAMA_URL", "http://llama-swap:8080"),
            llama_model: optional("LLAMA_MODEL", "gemma-4-e4b-rapido"),
        })
    }
}

fn required(name: &str) -> Result<String, String> {
    match env::var(name) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err(format!("falta la variable de entorno {name}")),
    }
}

fn optional(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_owned())
}
