//! Punto de encuentro entre la lectura (paso 2.2) y la traducción (2.3 y 2.4).
//!
//! La lectura llama a estos métodos para completar lo que devuelve. Mientras
//! la traducción no existe, devuelven «sin traducir»; los pasos 2.3 y 2.4
//! los rellenan sin cambiar su firma.

use std::collections::HashMap;

use atalaya_api::{Id, TranslationState};

/// Lo que se sabe del título de un artículo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TitleInfo {
    /// Título en español, si ya está traducido.
    pub title: Option<String>,
    /// Idioma original en ISO 639-1, si ya se detectó.
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Translations {}

impl Translations {
    pub fn new() -> Self {
        Self {}
    }

    /// Títulos ya traducidos de esos artículos. Los que faltan en el mapa,
    /// aún no. Paso 2.3.
    pub async fn titles(&self, _entry_ids: &[Id]) -> HashMap<Id, TitleInfo> {
        HashMap::new()
    }

    /// Estado de la traducción del texto completo de un artículo y, si está
    /// hecha, el HTML en español. Si hace falta y no ha empezado, la pone en
    /// marcha sin esperarla. Se llama al abrir y al guardar. Paso 2.4.
    pub async fn content(&self, _entry_id: Id) -> (TranslationState, Option<String>) {
        (TranslationState::NotNeeded, None)
    }
}
