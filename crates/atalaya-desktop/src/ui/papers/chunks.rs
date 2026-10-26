//! Los trozos de un HTML del servicio (el paper completo o el resumen), ya
//! partidos y guardados (#89). El lector pinta en cada render, y partir
//! 240 KB cuesta unos 4 ms: aquí se parte solo cuando cambia el HTML o el
//! tamaño de letra. Sin interfaz, para probarlo sin abrir la app.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::SharedString;

use super::full::{self, Formula, Part};
use crate::ui::format::{code_in_pre, split_pending};

/// Un trozo listo para pintar.
pub enum Chunk {
    /// Un `TextView`: su id (estable mientras el HTML no cambie de forma) y su
    /// HTML, ya sin los `<pre>` de código partidos. `pending` lo pinta en gris.
    Text {
        id: SharedString,
        html: SharedString,
        pending: bool,
    },
    /// Una fórmula aparte, centrada.
    Block(Formula),
}

/// Lo que se partió de cada texto, por id. Cambiar de paper lo vacía.
#[derive(Default)]
pub struct ChunkCache {
    entries: HashMap<String, Entry>,
    /// Cuántas veces se ha partido un HTML. Lo usan las pruebas.
    splits: usize,
}

struct Entry {
    html: String,
    base: String,
    font_px: f32,
    chunks: Rc<[Chunk]>,
}

impl ChunkCache {
    /// Los trozos del `html` con su `id`. Solo parte el HTML si no está ya
    /// partido con el mismo texto, la misma dirección y el mismo tamaño.
    pub fn get(&mut self, id: &str, html: &str, base: &str, font_px: f32) -> Rc<[Chunk]> {
        if let Some(entry) = self.entries.get(id)
            && entry.font_px == font_px
            && entry.base == base
            && entry.html == html
        {
            return entry.chunks.clone();
        }
        self.splits += 1;
        let chunks: Rc<[Chunk]> = split(id, html, base, font_px).into();
        self.entries.insert(
            id.to_owned(),
            Entry {
                html: html.to_owned(),
                base: base.to_owned(),
                font_px,
                chunks: chunks.clone(),
            },
        );
        chunks
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[cfg(test)]
    fn splits(&self) -> usize {
        self.splits
    }
}

/// Parte el HTML como lo hacía el lector en cada render: primero por
/// pendientes, después en texto y fórmulas aparte. Los ids van en orden, con
/// el mismo número que antes (`{id}-{n}`).
fn split(id: &str, html: &str, base: &str, font_px: f32) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut n = 0;
    for piece in split_pending(html) {
        for part in full::parts(&piece.html, base, font_px) {
            n += 1;
            chunks.push(match part {
                Part::Text(text) => Chunk::Text {
                    id: SharedString::from(format!("{id}-{n}")),
                    html: SharedString::from(code_in_pre(&text)),
                    pending: piece.pending,
                },
                Part::Block(formula) => Chunk::Block(formula),
            });
        }
    }
    chunks
}

#[cfg(test)]
pub(crate) fn fixture_paper_html() -> String {
    const BASE: &str = "/formulas/2610.31646v1";
    let mut html = String::with_capacity(260_000);
    let mut formula = 0;
    let mut table = 0;
    let mut block = 0;
    for paragraph in 0..160 {
        if paragraph % 16 == 0 {
            html.push_str(&format!(
                "<h2><span class=\"ltx_tag\">{} </span>Sección {paragraph}</h2>",
                paragraph / 16 + 1
            ));
        }
        html.push_str("<p>");
        for sentence in 0..8 {
            html.push_str(&format!(
                "Esta frase del párrafo {paragraph} describe el método con detalle suficiente \
                 para ocupar espacio real en el artículo, con cifras como 3,4 % y 1,2 ms. "
            ));
            if sentence % 4 == 1 && formula < 334 - 8 {
                formula += 1;
                html.push_str(&format!(
                    "<img class=\"atalaya-formula\" src=\"{BASE}/{formula}.svg\" alt=\"x_{formula}\" \
                     style=\"vertical-align: -0.439ex; width: 5.429ex; height: 2ex\"> "
                ));
            }
        }
        html.push_str("</p>");
        if block < 8 && paragraph % 20 == 10 {
            block += 1;
            html.push_str(&format!(
                "<div class=\"atalaya-ecuacion\"><img class=\"atalaya-formula atalaya-formula-bloque\" \
                 src=\"{BASE}/b{block}.svg\" alt=\"E\" style=\"vertical-align: -0.5ex; \
                 width: 12ex; height: 3ex\"><span class=\"atalaya-numero\">({block})</span></div>"
            ));
        }
        if paragraph % 16 == 12 && table < 10 {
            table += 1;
            html.push_str("<table>");
            for row in 0..12 {
                html.push_str(&format!(
                    "<tr><td>modelo {table}-{row}</td><td>{row}.25</td><td>{}</td></tr>",
                    row * 3
                ));
            }
            html.push_str("</table>");
        }
    }
    while formula < 334 - 8 {
        formula += 1;
        html.push_str(&format!(
            "<p><img class=\"atalaya-formula\" src=\"{BASE}/{formula}.svg\" alt=\"z\" \
             style=\"width: 2ex; height: 2ex\"></p>"
        ));
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://ai-lab:8080";
    const FONT: f32 = 16.0;

    fn text_ids(chunks: &[Chunk]) -> Vec<String> {
        chunks
            .iter()
            .filter_map(|c| match c {
                Chunk::Text { id, .. } => Some(id.to_string()),
                Chunk::Block(_) => None,
            })
            .collect()
    }

    #[test]
    fn no_vuelve_a_partir_si_el_html_no_cambia() {
        let html = fixture_paper_html();
        let mut cache = ChunkCache::default();
        let first = cache.get("full-2610.31646", &html, BASE, FONT);
        for _ in 0..50 {
            // Lo que hace el sondeo: el mismo HTML, en otra copia.
            let again = cache.get("full-2610.31646", &html.clone(), BASE, FONT);
            assert!(Rc::ptr_eq(&first, &again));
        }
        assert_eq!(cache.splits(), 1);
    }

    #[test]
    fn parte_otra_vez_si_cambian_el_html_el_tamano_o_la_direccion() {
        let html = fixture_paper_html();
        let mut cache = ChunkCache::default();
        cache.get("full-x", &html, BASE, FONT);
        cache.get("full-x", &format!("{html}<p>nuevo</p>"), BASE, FONT);
        assert_eq!(cache.splits(), 2);
        cache.get("full-x", &format!("{html}<p>nuevo</p>"), BASE, 20.0);
        assert_eq!(cache.splits(), 3);
        cache.get(
            "full-x",
            &format!("{html}<p>nuevo</p>"),
            "http://otro",
            20.0,
        );
        assert_eq!(cache.splits(), 4);
    }

    #[test]
    fn textos_de_otro_id_no_se_echan_fuera() {
        let html = fixture_paper_html();
        let mut cache = ChunkCache::default();
        cache.get("abstract-x-true", "<p>resumen</p>", BASE, FONT);
        cache.get("full-x", &html, BASE, FONT);
        cache.get("abstract-x-true", "<p>resumen</p>", BASE, FONT);
        assert_eq!(cache.splits(), 2);
    }

    #[test]
    fn los_ids_de_los_trozos_no_cambian_entre_sondeos() {
        let html = fixture_paper_html();
        let mut cache = ChunkCache::default();
        let before = text_ids(&cache.get("full-x", &html, BASE, FONT));
        cache.clear();
        let after = text_ids(&cache.get("full-x", &html, BASE, FONT));
        assert_eq!(before, after);
        assert_eq!(before.first().map(String::as_str), Some("full-x-1"));
        assert!(before.iter().all(|id| id.starts_with("full-x-")));
    }

    #[test]
    fn el_html_partido_es_el_mismo_que_daba_el_lector() {
        let html = fixture_paper_html();
        let mut cache = ChunkCache::default();
        let chunks = cache.get("full-x", &html, BASE, FONT);
        let mut expected = Vec::new();
        for piece in split_pending(&html) {
            for part in full::parts(&piece.html, BASE, FONT) {
                expected.push(part);
            }
        }
        assert_eq!(chunks.len(), expected.len());
        for (chunk, part) in chunks.iter().zip(&expected) {
            match (chunk, part) {
                (Chunk::Text { html, .. }, Part::Text(text)) => {
                    assert_eq!(html.as_ref(), code_in_pre(text));
                }
                (Chunk::Block(a), Part::Block(b)) => assert_eq!(a, b),
                _ => panic!("trozo distinto: {part:?}"),
            }
        }
    }

    #[test]
    fn los_pendientes_salen_marcados() {
        let html = "<p>Listo.</p><span class=\"atalaya-pendiente\"><p>Falta.</p></span>";
        let mut cache = ChunkCache::default();
        let chunks = cache.get("abstract-x-false", html, BASE, FONT);
        let flags: Vec<bool> = chunks
            .iter()
            .filter_map(|c| match c {
                Chunk::Text { pending, .. } => Some(*pending),
                Chunk::Block(_) => None,
            })
            .collect();
        assert_eq!(flags, [false, true]);
    }

    /// `cargo test -p atalaya-desktop medir_trabajo_por_fotograma -- --ignored --nocapture`
    #[test]
    #[ignore = "medida, no prueba: imprime cuánto cuesta un fotograma"]
    fn medir_trabajo_por_fotograma() {
        use std::time::Instant;

        let html = fixture_paper_html();
        println!("HTML: {} bytes", html.len());
        println!(
            "fórmulas: {} · trozos de texto: {} · fórmulas aparte: {}",
            html.matches("class=\"atalaya-formula").count(),
            text_ids(&split("full-x", &html, BASE, FONT)).len(),
            split("full-x", &html, BASE, FONT)
                .iter()
                .filter(|c| matches!(c, Chunk::Block(_)))
                .count(),
        );
        let frames = 200;

        let start = Instant::now();
        for _ in 0..frames {
            std::hint::black_box(split("full-x", std::hint::black_box(&html), BASE, FONT));
        }
        println!(
            "antes, partir el HTML en cada render: {:?}",
            start.elapsed() / frames
        );

        let mut cache = ChunkCache::default();
        cache.get("full-x", &html, BASE, FONT);
        let start = Instant::now();
        for _ in 0..frames {
            std::hint::black_box(cache.get("full-x", std::hint::black_box(&html), BASE, FONT));
        }
        println!("ahora, consultar la caché: {:?}", start.elapsed() / frames);
        assert_eq!(cache.splits(), 1);
    }
}
