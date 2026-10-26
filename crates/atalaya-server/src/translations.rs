//! Punto de encuentro entre la lectura (paso 2.2) y la traducción (2.3 y 2.4).
//!
//! La lectura llama a estos métodos para completar lo que devuelve. Los
//! títulos se traducen en `titles` (2.3); el texto completo, en `content` (2.4).

use std::{collections::HashMap, sync::Arc};

use atalaya_api::{Id, TranslationState};

use crate::{
    content::{ContentState, InProgress},
    llama::Llama,
    miniflux::Miniflux,
    store,
    store::Store,
    titles::Queue,
};

/// Lo que se sabe del título de un artículo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TitleInfo {
    /// Título en español, si ya está traducido.
    pub title: Option<String>,
    /// Idioma original en ISO 639-1, si ya se detectó.
    pub lang: Option<String>,
}

#[derive(Clone)]
pub struct Translations {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    llama: Llama,
    miniflux: Miniflux,
    queue: Queue,
    in_progress: InProgress,
}

impl Translations {
    pub fn new(store: Store, llama: Llama, miniflux: Miniflux) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                llama,
                miniflux,
                queue: Queue::new(),
                in_progress: InProgress::default(),
            }),
        }
    }

    pub fn store(&self) -> &Store {
        &self.inner.store
    }

    pub fn llama(&self) -> &Llama {
        &self.inner.llama
    }

    pub fn miniflux(&self) -> &Miniflux {
        &self.inner.miniflux
    }

    pub(crate) fn queue(&self) -> &Queue {
        &self.inner.queue
    }

    pub(crate) fn in_progress(&self) -> &InProgress {
        &self.inner.in_progress
    }

    /// Títulos ya traducidos de esos artículos. Los que faltan en el mapa,
    /// aún no se han visto. Paso 2.3.
    pub async fn titles(&self, entry_ids: &[Id]) -> HashMap<Id, TitleInfo> {
        let ids = entry_ids.to_vec();
        match self.store().call(move |c| store::titles(c, &ids)).await {
            Ok(map) => map,
            Err(e) => {
                tracing::error!("no se pudieron leer los títulos traducidos: {e}");
                HashMap::new()
            }
        }
    }

    /// Estado de la traducción del texto completo de un artículo y, si está
    /// hecha, el HTML en español. Si hace falta y no ha empezado, la pone en
    /// marcha sin esperarla. Se llama al abrir y al guardar. Paso 2.4.
    ///
    /// Si no se puede saber (Miniflux o la base fallan), responde `Failed`.
    pub async fn content(&self, entry_id: Id) -> ContentState {
        match self.content_state(entry_id).await {
            Ok(state) => state,
            Err(e) => {
                tracing::error!(entry = entry_id, "estado de la traducción del texto: {e}");
                ContentState {
                    translation: TranslationState::Failed,
                    html: None,
                    progress: None,
                }
            }
        }
    }
}
