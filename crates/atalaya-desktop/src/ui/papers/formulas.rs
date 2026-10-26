//! Las imágenes de las fórmulas del paper completo (ADR 0010). Se bajan del
//! servicio en segundo plano; GPUI las pide por dirección mientras pinta el
//! texto, así que aquí se guardan ya bajadas, con el color del texto del
//! tema. Lo que todavía no ha llegado sale en blanco.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gpui::{Image, ImageFormat};

use super::full::recolor_svg;

/// Lo que se sabe de cada fórmula. Compartido entre el hilo de la interfaz y
/// las descargas.
pub struct Formulas {
    store: Mutex<Store>,
    blank: Arc<Image>,
}

#[derive(Default)]
struct Store {
    /// El SVG tal cual lo da el servicio, por dirección completa.
    svg: HashMap<String, Arc<str>>,
    /// Bajando ahora, para no pedirla dos veces.
    loading: HashSet<String>,
    /// Falló la descarga: no se reintenta hasta reabrir el paper.
    failed: HashSet<String>,
    /// La última imagen pintada de cada fórmula, con su color.
    painted: HashMap<String, (String, Arc<Image>)>,
}

impl Formulas {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(Store::default()),
            blank: Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec(),
            )),
        }
    }

    /// Las direcciones que hay que bajar ahora. Las marca como en curso.
    pub fn take_missing(&self, urls: Vec<String>) -> Vec<String> {
        let mut store = self.lock();
        let mut missing = Vec::new();
        for url in urls {
            if store.svg.contains_key(&url)
                || store.failed.contains(&url)
                || store.loading.contains(&url)
            {
                continue;
            }
            store.loading.insert(url.clone());
            missing.push(url);
        }
        missing
    }

    pub fn loaded(&self, url: &str, svg: String) {
        let mut store = self.lock();
        store.loading.remove(url);
        store.svg.insert(url.to_owned(), svg.into());
    }

    /// Al abrir otro paper, las que fallaron se vuelven a pedir.
    pub fn forget_failed(&self) {
        self.lock().failed.clear();
    }

    pub fn failed(&self, url: &str) {
        let mut store = self.lock();
        store.loading.remove(url);
        store.failed.insert(url.to_owned());
    }

    /// La imagen de una fórmula, con `color` como `currentColor`. Sin bajar,
    /// una imagen en blanco.
    pub fn image(&self, url: &str, color: &str) -> Arc<Image> {
        let mut store = self.lock();
        if let Some((painted_color, image)) = store.painted.get(url)
            && painted_color == color
        {
            return image.clone();
        }
        let Some(svg) = store.svg.get(url).cloned() else {
            return self.blank.clone();
        };
        let image = Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            recolor_svg(&svg, color).into_bytes(),
        ));
        store
            .painted
            .insert(url.to_owned(), (color.to_owned(), image.clone()));
        image
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }
}
