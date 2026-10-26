//! El paper completo en el lector (P7, ADR 0010): el HTML que da el servicio,
//! partido en lo que el visor de GPUI sabe pintar. Son funciones sin
//! interfaz, para probarlas sin abrir la app.
//!
//! El visor no entiende las medidas en `ex` ni `vertical-align`. Cada fórmula
//! se reescribe con su tamaño en píxeles, y las que van aparte salen del
//! texto para centrarlas en su propio bloque.

use crate::ui::format::has_text;

/// Cuánto mide un `ex` (la altura de la «x») respecto de la letra del texto.
/// MathJax se mide en `ex` de la fuente del texto; 0,5 es lo habitual.
pub(crate) const EX_PER_FONT: f32 = 0.5;

/// Un trozo del paper, en orden: HTML para el visor, o una fórmula aparte.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Block(Formula),
}

/// Una fórmula aparte, centrada en su línea.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    /// Dirección completa del SVG en el servicio.
    pub url: String,
    /// Tamaño en píxeles.
    pub width: f32,
    pub height: f32,
    /// Número de la ecuación, como «(1)», si lo tiene.
    pub number: Option<String>,
}

/// Parte un trozo de HTML en texto y fórmulas aparte. Las fórmulas en línea
/// se quedan en el texto, con su tamaño en píxeles.
pub fn parts(html: &str, base: &str, font_px: f32) -> Vec<Part> {
    let ex = font_px * EX_PER_FONT;
    let mut parts = Vec::new();
    let mut text = String::new();
    // El visor recorta el espacio final de un `span`: «<span>1 </span>Introducción»
    // salía «1Introducción». Un espacio duro no se recorta.
    let html = html.replace(" </span>", "\u{a0}</span>");
    let mut rest = html.as_str();
    while let Some(start) = next_img(rest) {
        let Some(len) = rest[start..].find('>') else {
            break;
        };
        let end = start + len + 1;
        let tag = &rest[start..end];
        let attrs = attributes(tag);
        let class = attr(&attrs, "class").unwrap_or_default();
        let Some(src) = attr(&attrs, "src").filter(|_| class.contains("atalaya-formula")) else {
            text.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        };
        let url = absolute(base, src);
        let style = attr(&attrs, "style").unwrap_or_default();
        let width = style_ex(style, "width", ex).unwrap_or(ex);
        let height = style_ex(style, "height", ex).unwrap_or(ex);
        text.push_str(&rest[..start]);
        if class.contains("atalaya-formula-bloque") {
            flush(&mut parts, &mut text);
            let after = &rest[end..];
            let (number, after) = equation_number(after);
            parts.push(Part::Block(Formula {
                url,
                width,
                height,
                number,
            }));
            rest = after;
        } else {
            let alt = attr(&attrs, "alt").unwrap_or_default();
            // El `<img>` va dentro de un `<span>`: el visor de gpui-base corta
            // el párrafo en cada `<img>` que es hijo directo de un bloque
            // (un `<p>`), pero deja en línea el que va dentro de un elemento
            // en línea.
            text.push_str(&format!(
                "<span><img class=\"atalaya-formula\" src=\"{url}\" alt=\"{alt}\" \
                 style=\"width: {width:.1}px; height: {height:.1}px\"></span>"
            ));
            rest = &rest[end..];
            // El espacio que sigue a la fórmula también se recorta: «w en» salía
            // «wen». Va como espacio duro.
            if let Some(after) = rest.strip_prefix(' ') {
                text.push('\u{a0}');
                rest = after;
            }
        }
    }
    text.push_str(rest);
    flush(&mut parts, &mut text);
    parts
}

/// Las direcciones de las fórmulas que trae el HTML, completas.
pub fn formula_urls(html: &str, base: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = html;
    while let Some(start) = next_img(rest) {
        let Some(len) = rest[start..].find('>') else {
            break;
        };
        let attrs = attributes(&rest[start..start + len + 1]);
        if let Some(src) = attr(&attrs, "src").filter(|s| s.starts_with("/formulas/")) {
            urls.push(absolute(base, src));
        }
        rest = &rest[start + len + 1..];
    }
    urls
}

/// Cambia el color `currentColor` del SVG por el del texto del tema, y pasa
/// el tamaño de la raíz de `ex` a píxeles, al doble para que se vea nítido.
/// Sin tamaño, el SVG se rasteriza al de su `viewBox` (miles de píxeles por
/// fórmula) y la app llegaba a gastar varios GB.
pub fn recolor_svg(svg: &str, color: &str, ex_px: f32) -> String {
    let svg = svg.replace("currentColor", color);
    let Some(start) = svg.find("<svg") else {
        return svg;
    };
    let Some(len) = svg[start..].find('>') else {
        return svg;
    };
    let root = &svg[start..start + len];
    let root = ex_attr_to_px(&ex_attr_to_px(root, "width", ex_px), "height", ex_px);
    format!("{}{}{}", &svg[..start], root, &svg[start + len..])
}

/// `name="5.4ex"` de la etiqueta pasa a `name="{5.4 × ex_px × 2}"`. Sin
/// valor en `ex`, se quita.
fn ex_attr_to_px(tag: &str, name: &str, ex_px: f32) -> String {
    let needle = format!(" {name}=\"");
    let Some(start) = tag.find(&needle) else {
        return tag.to_owned();
    };
    let value_start = start + needle.len();
    let Some(close) = tag[value_start..].find('"') else {
        return tag.to_owned();
    };
    let end = value_start + close + 1;
    let value = &tag[value_start..end - 1];
    match value
        .strip_suffix("ex")
        .and_then(|v| v.trim().parse::<f32>().ok())
    {
        Some(ex) => format!(
            "{} {name}=\"{:.1}\"{}",
            &tag[..start],
            ex * ex_px * 2.0,
            &tag[end..]
        ),
        None => format!("{}{}", &tag[..start], &tag[end..]),
    }
}

/// Un `<img>` que empieza en `from`, no `<imgfoo>`.
fn next_img(html: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = html[from..].find("<img").map(|i| from + i) {
        match html[i + 4..].chars().next() {
            Some(c) if c.is_whitespace() || c == '/' || c == '>' => return Some(i),
            _ => from = i + 4,
        }
    }
    None
}

/// Pares `nombre="valor"` de una etiqueta, con los valores tal cual vienen.
fn attributes(tag: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = tag.trim_start_matches("<img").trim_end_matches('>');
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        let name_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        if name.is_empty() {
            break;
        }
        rest = rest[name_end..].trim_start();
        let Some(after_eq) = rest.strip_prefix('=') else {
            rest = rest.trim_start();
            continue;
        };
        let after_eq = after_eq.trim_start();
        let Some(quote) = after_eq.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let body = &after_eq[1..];
        let Some(close) = body.find(quote) else {
            break;
        };
        out.push((name, &body[..close]));
        rest = &body[close + 1..];
    }
    out
}

fn attr<'a>(attrs: &[(&str, &'a str)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

/// La dirección completa de una ruta del servicio: `/formulas/ID/N.svg` contra
/// `http://servicio:puerto`, sin dos barras seguidas.
fn absolute(base: &str, src: &str) -> String {
    if src.starts_with("http://") || src.starts_with("https://") {
        src.to_owned()
    } else {
        format!("{}{}", base.trim_end_matches('/'), src)
    }
}

/// Un valor `width` o `height` de un `style` en `ex`, en píxeles.
fn style_ex(style: &str, prop: &str, ex: f32) -> Option<f32> {
    let value = style.split(';').find_map(|decl| {
        let (name, value) = decl.split_once(':')?;
        (name.trim() == prop).then(|| value.trim())
    })?;
    let number = value.strip_suffix("ex")?.trim().parse::<f32>().ok()?;
    (number > 0.).then_some(number * ex)
}

/// Si lo que sigue a una fórmula aparte es su número, `<span
/// class="atalaya-numero">(1)</span>`, lo quita del texto y lo devuelve.
fn equation_number(after: &str) -> (Option<String>, &str) {
    const OPEN: &str = "<span class=\"atalaya-numero\">";
    let trimmed = after.trim_start();
    let Some(inner) = trimmed.strip_prefix(OPEN) else {
        return (None, after);
    };
    let Some(close) = inner.find("</span>") else {
        return (None, after);
    };
    let number = inner[..close].trim().to_owned();
    (Some(number), &inner[close + "</span>".len()..])
}

/// Guarda el texto acumulado, si tiene algo que mostrar.
fn flush(parts: &mut Vec<Part>, text: &mut String) {
    let html = std::mem::take(text);
    if has_text(&html) || html.contains("<img") {
        parts.push(Part::Text(html));
    }
}

/// Quita un atributo de una etiqueta de apertura, si lo tiene.
#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://ai-lab:8080/";

    fn inline(src: &str, style: &str) -> String {
        format!("<img class=\"atalaya-formula\" src=\"{src}\" alt=\"x&lt;y\" style=\"{style}\">")
    }

    #[test]
    fn space_after_an_inline_formula_survives() {
        let html = format!(
            "<p>a {} en b</p>",
            inline("/formulas/a/1.svg", "width: 2ex; height: 2ex")
        );
        let Part::Text(text) = &parts(&html, BASE, 16.0)[0] else {
            panic!()
        };
        assert!(text.contains("</span>\u{a0}en b"), "{text}");
    }

    #[test]
    fn trailing_space_of_a_span_survives() {
        let parts = parts(
            "<h2><span class=\"ltx_tag\">1 </span>Introducción</h2>",
            "http://s",
            16.0,
        );
        assert_eq!(
            parts,
            vec![Part::Text(
                "<h2><span class=\"ltx_tag\">1\u{a0}</span>Introducción</h2>".into()
            )]
        );
    }

    #[test]
    fn ex_sizes_become_pixels() {
        // Con letra de 16 px, 1 ex son 8 px: 5,429 ex son 43,4 px.
        let html = format!(
            "<p>Si {}</p>",
            inline(
                "/formulas/2610.31646v1/3.svg",
                "vertical-align: -0.439ex; width: 5.429ex; height: 2ex"
            )
        );
        let parts = parts(&html, BASE, 16.);
        assert_eq!(
            parts,
            [Part::Text(
                "<p>Si <span><img class=\"atalaya-formula\" \
                 src=\"http://ai-lab:8080/formulas/2610.31646v1/3.svg\" alt=\"x&lt;y\" \
                 style=\"width: 43.4px; height: 16.0px\"></span></p>"
                    .to_owned()
            )]
        );
    }

    #[test]
    fn vertical_align_is_dropped_and_missing_size_falls_back_to_one_ex() {
        let html = inline("/formulas/a/1.svg", "vertical-align: -0.5ex");
        let parts = parts(&html, BASE, 16.);
        assert_eq!(parts.len(), 1);
        let Part::Text(text) = &parts[0] else {
            panic!("se esperaba texto: {parts:?}");
        };
        assert!(!text.contains("vertical-align"), "{text}");
        assert!(text.contains("width: 8.0px; height: 8.0px"), "{text}");
    }

    #[test]
    fn relative_urls_join_the_service_address_once() {
        assert_eq!(
            absolute("http://ai-lab:8080", "/formulas/x/0.svg"),
            "http://ai-lab:8080/formulas/x/0.svg"
        );
        assert_eq!(
            absolute("http://ai-lab:8080/", "/formulas/x/0.svg"),
            "http://ai-lab:8080/formulas/x/0.svg"
        );
        assert_eq!(
            absolute(BASE, "https://otro.example/a.svg"),
            "https://otro.example/a.svg"
        );
        assert_eq!(
            formula_urls(&inline("/formulas/x/0.svg", ""), BASE),
            ["http://ai-lab:8080/formulas/x/0.svg"]
        );
    }

    #[test]
    fn current_color_takes_the_theme_color_and_the_root_size_goes_to_pixels() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"5.429ex\" \
                   height=\"2ex\" viewBox=\"0 -750 2400 900\" role=\"img\">\
                   <g fill=\"currentColor\" stroke-width=\"40\"><path d=\"M0 0\"/></g></svg>";
        let out = recolor_svg(svg, "#f1f1f1", 8.0);
        assert!(!out.contains("currentColor"), "{out}");
        assert!(out.contains("fill=\"#f1f1f1\""), "{out}");
        assert!(out.contains(" width=\"86.9\""), "{out}");
        assert!(out.contains(" height=\"32.0\""), "{out}");
        assert!(out.contains("viewBox=\"0 -750 2400 900\""), "{out}");
        assert!(out.contains("stroke-width=\"40\""), "{out}");
    }

    #[test]
    fn block_formula_leaves_the_text_and_keeps_its_number() {
        let html = "<p>Antes.</p><div class=\"atalaya-ecuacion\">\
                    <img class=\"atalaya-formula atalaya-formula-bloque\" \
                    src=\"/formulas/2610.31646v1/2.svg\" alt=\"E\" \
                    style=\"vertical-align: -0.5ex; width: 3ex; height: 2ex\">\
                    <span class=\"atalaya-numero\">(1)</span></div><p>Después.</p>";
        let parts = parts(html, BASE, 16.);
        assert_eq!(
            parts,
            [
                Part::Text("<p>Antes.</p><div class=\"atalaya-ecuacion\">".to_owned()),
                Part::Block(Formula {
                    url: "http://ai-lab:8080/formulas/2610.31646v1/2.svg".to_owned(),
                    width: 24.,
                    height: 16.,
                    number: Some("(1)".to_owned()),
                }),
                Part::Text("</div><p>Después.</p>".to_owned()),
            ]
        );
    }

    #[test]
    fn block_formula_without_number_and_without_wrapper() {
        let html = "<img class=\"atalaya-formula-bloque atalaya-formula\" \
                    src=\"/formulas/a/0.svg\" style=\"width: 10ex; height: 4ex\"><p>fin</p>";
        let parts = parts(html, BASE, 10.);
        assert_eq!(parts.len(), 2);
        assert!(matches!(
            &parts[0],
            Part::Block(Formula { number: None, width, height, .. })
                if (*width - 50.).abs() < 1e-3 && (*height - 20.).abs() < 1e-3
        ));
        assert_eq!(parts[1], Part::Text("<p>fin</p>".to_owned()));
    }

    #[test]
    fn latex_code_and_tables_pass_through_untouched() {
        let html = "<table><tr><td>Backbone learning rate</td><td>3e-4</td></tr></table>\
                    <p>Con <code class=\"atalaya-latex\">\\lx@a</code> y \
                    <img class=\"atalaya-formula\" src=\"/formulas/a/1.svg\" alt=\"b\" \
                    style=\"width: 1ex; height: 1ex\"> dentro.</p>";
        let parts = parts(html, BASE, 16.);
        let [Part::Text(text)] = parts.as_slice() else {
            panic!("una sola parte de texto: {parts:?}");
        };
        assert!(
            text.starts_with("<table><tr><td>Backbone learning rate</td>"),
            "{text}"
        );
        assert!(
            text.contains("<code class=\"atalaya-latex\">\\lx@a</code>"),
            "{text}"
        );
        assert!(text.contains("width: 8.0px; height: 8.0px"), "{text}");
    }

    #[test]
    fn inline_formula_is_wrapped_so_the_paragraph_is_not_cut() {
        // Un `<img>` suelto dentro de un `<p>` lo parte en tres bloques en
        // gpui-base; dentro de un `<span>`, sigue en la línea.
        let html = format!(
            "<p>mediante un índice de {} bits</p>",
            inline("/formulas/a/1.svg", "")
        );
        let parts = parts(&html, BASE, 16.);
        let [Part::Text(text)] = parts.as_slice() else {
            panic!("una sola parte: {parts:?}");
        };
        assert!(
            text.contains("<span><img class=\"atalaya-formula\""),
            "{text}"
        );
        assert!(text.contains("</span>\u{a0}bits</p>"), "{text}");
    }

    #[test]
    fn other_images_are_not_formulas() {
        let html = "<img src=\"https://arxiv.org/html/x/fig.png\" alt=\"fig\">";
        assert_eq!(parts(html, BASE, 16.), [Part::Text(html.to_owned())]);
        assert!(formula_urls(html, BASE).is_empty());
    }
}
