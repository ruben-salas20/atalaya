//! Prepara una página HTML de arXiv como el servicio (#38) y cuenta lo que
//! sale: fórmulas, tablas, párrafos por traducir. No usa la GPU.
//!
//! Uso: `cargo run -q -p atalaya-server --example preparar_paper -- pagina.html`

use atalaya_server::{
    content::{Part, split},
    papers::full::{prep, render},
};

fn main() {
    let path = std::env::args().nth(1).expect("falta la página HTML");
    let page = std::fs::read_to_string(&path).expect("no se pudo leer la página");
    let p = prep::prepare(&page, "https://arxiv.org/html/");
    let inline = p.formulas.iter().filter(|f| !f.display).count();
    let (html, tables) = render::extract_tables(&p.html);
    let cells: usize = tables.iter().map(|t| prep::table_cells(t).1.len()).sum();
    let parts = split(&html);
    let paragraphs: Vec<&String> = parts
        .iter()
        .filter_map(|p| match p {
            Part::Translate(t) => Some(t),
            Part::Keep(_) => None,
        })
        .collect();
    let to_model = paragraphs
        .iter()
        .filter(|t| prep::has_text_outside_markers(t))
        .count();
    println!(
        "{path}: {} fórmulas ({inline} en línea, {} aparte) · {} tablas, {cells} celdas con palabras · \
         {} párrafos, {to_model} al modelo · quedan <math: {} · bibliografía: {} · imágenes: {:?}",
        p.formulas.len(),
        p.formulas.len() - inline,
        tables.len(),
        paragraphs.len(),
        p.html.matches("<math").count(),
        p.html.contains("ltx_bibliography"),
        p.html
            .match_indices("<img src=\"")
            .map(|(i, m)| p.html[i + m.len()..].split('"').next().unwrap_or(""))
            .take(2)
            .collect::<Vec<_>>()
    );
}
