//! Las fórmulas del resumen de un paper (#80). arXiv da el resumen en texto
//! con el LaTeX entre `$…$` o `$$…$$`. Para traducirlo, cada fórmula va como
//! marca `<m id="N">` (ADR 0010), y el modelo no la toca. Para pintarlo, el
//! servicio da además el resumen en HTML, con cada fórmula como imagen SVG.

use super::full::mathjax::Svg;

/// Una fórmula del texto, en orden.
#[derive(Debug, Clone, PartialEq)]
pub struct Math {
    pub latex: String,
    /// `$$…$$`: va aparte.
    pub display: bool,
}

/// Un trozo del texto: texto normal o una fórmula.
#[derive(Debug, Clone, PartialEq)]
enum Piece<'a> {
    Text(&'a str),
    Math { latex: &'a str, display: bool },
}

/// Parte el texto en texto y fórmulas. Sigue la regla de pandoc para no
/// confundir un precio con una fórmula: tras el `$` que abre no puede ir un
/// espacio, antes del que cierra tampoco, y tras el que cierra no puede ir
/// una cifra. `\$` es un dólar, no una fórmula.
fn pieces(text: &str) -> Vec<Piece<'_>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut text_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' || (i > 0 && bytes[i - 1] == b'\\') {
            i += 1;
            continue;
        }
        let display = bytes.get(i + 1) == Some(&b'$');
        let open = if display { 2 } else { 1 };
        let start = i + open;
        if bytes.get(start).is_none_or(|b| b.is_ascii_whitespace()) {
            i += open;
            continue;
        }
        let Some(end) = closing(bytes, start, display) else {
            i += open;
            continue;
        };
        if text_start < i {
            out.push(Piece::Text(&text[text_start..i]));
        }
        out.push(Piece::Math {
            latex: &text[start..end],
            display,
        });
        i = end + open;
        text_start = i;
    }
    if text_start < text.len() {
        out.push(Piece::Text(&text[text_start..]));
    }
    out
}

/// Dónde acaba la fórmula que empieza en `start`, antes del `$` que la
/// cierra. Sin salir del párrafo.
fn closing(bytes: &[u8], start: usize, display: bool) -> Option<usize> {
    let mut j = start;
    while j < bytes.len() {
        match bytes[j] {
            b'\n' if bytes.get(j + 1) == Some(&b'\n') => return None,
            b'\\' => j += 2,
            b'$' if display => {
                return (bytes.get(j + 1) == Some(&b'$')).then_some(j);
            }
            b'$' => {
                let before = bytes[j - 1];
                let after = bytes.get(j + 1);
                if before.is_ascii_whitespace() || after.is_some_and(u8::is_ascii_digit) {
                    j += 1;
                    continue;
                }
                return Some(j);
            }
            _ => j += 1,
        }
    }
    None
}

/// Las fórmulas del texto, en orden.
pub fn formulas(text: &str) -> Vec<Math> {
    pieces(text)
        .into_iter()
        .filter_map(|p| match p {
            Piece::Math { latex, display } => Some(Math {
                latex: latex.to_owned(),
                display,
            }),
            Piece::Text(_) => None,
        })
        .collect()
}

/// Un párrafo como HTML para traducir: el texto escapado y cada fórmula como
/// `<m id="N">LaTeX</m>`, numeradas desde 0 en el párrafo.
pub fn marked(paragraph: &str) -> String {
    let mut out = String::with_capacity(paragraph.len() + 32);
    let mut n = 0;
    for piece in pieces(paragraph) {
        match piece {
            Piece::Text(t) => out.push_str(&escape(t)),
            Piece::Math { latex, display } => {
                // La marca no dice si iba aparte: se recupera por su número.
                let _ = display;
                out.push_str(&format!("<m id=\"{n}\">{}</m>", escape(latex)));
                n += 1;
            }
        }
    }
    out
}

/// Deshace [`marked`] sobre la traducción: el texto sin escapar y cada marca
/// otra vez entre `$…$` (o `$$…$$` si así iba en `original`).
pub fn unmarked(translated: &str, original: &str) -> String {
    let displays: Vec<bool> = formulas(original).iter().map(|m| m.display).collect();
    let mut out = String::with_capacity(translated.len());
    let mut rest = translated;
    while let Some(start) = rest.find("<m id=\"") {
        out.push_str(&unescape(&rest[..start]));
        let after = &rest[start + "<m id=\"".len()..];
        let n: usize = after
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or(usize::MAX);
        let (Some(gt), Some(end)) = (after.find('>'), after.find("</m>")) else {
            out.push_str(&unescape(&rest[start..]));
            return out;
        };
        let latex = unescape(&after[gt + 1..end]);
        let delimiter = if displays.get(n).copied().unwrap_or(false) {
            "$$"
        } else {
            "$"
        };
        out.push_str(&format!("{delimiter}{latex}{delimiter}"));
        rest = &after[end + "</m>".len()..];
    }
    out.push_str(&unescape(rest));
    out
}

/// El texto como HTML para pintar: un `<p>` por párrafo y cada fórmula como
/// en el paper completo (contrato de [`atalaya_api::FullPaper`]). `svgs` son
/// las del resumen original, en orden; `url(k)` da la dirección de la k-ésima.
///
/// La fórmula k-ésima del texto se pinta con la k-ésima del original si
/// cuadran en número: una traducción antigua pudo cambiar algún LaTeX. Si no,
/// se busca por su LaTeX, y sin SVG va como código.
pub fn html(
    text: &str,
    original: &[Math],
    svgs: &[Option<Svg>],
    url: impl Fn(usize) -> String,
) -> String {
    let same_count = formulas(text).len() == original.len();
    let mut k = 0;
    let mut out = String::new();
    for paragraph in text.split("\n\n").filter(|p| !p.trim().is_empty()) {
        out.push_str("<p>");
        for piece in pieces(paragraph.trim()) {
            match piece {
                Piece::Text(t) => out.push_str(&escape(&t.replace("---", "—").replace("--", "–"))),
                Piece::Math { latex, display } => {
                    let index = if same_count {
                        Some(k)
                    } else {
                        original.iter().position(|m| m.latex == latex)
                    };
                    k += 1;
                    let svg = index.and_then(|i| svgs.get(i).cloned().flatten().map(|s| (i, s)));
                    match svg {
                        Some((i, svg)) => {
                            let class = if display || original[i].display {
                                "atalaya-formula atalaya-formula-bloque"
                            } else {
                                "atalaya-formula"
                            };
                            out.push_str(&format!(
                                "<img class=\"{class}\" src=\"{}\" alt=\"{}\" \
                                 style=\"vertical-align: {}; width: {}; height: {}\">",
                                url(i),
                                escape(latex),
                                escape(&svg.vertical_align),
                                escape(&svg.width),
                                escape(&svg.height),
                            ));
                        }
                        None => out.push_str(&format!(
                            "<code class=\"atalaya-latex\">{}</code>",
                            escape(latex)
                        )),
                    }
                }
            }
        }
        out.push_str("</p>");
    }
    out
}

/// El tamaño y la alineación de un SVG de MathJax, sacados de su raíz:
/// `width="1.176ex" height="1.593ex" style="vertical-align: -0.023ex;"`. Así
/// basta con guardar el SVG.
pub fn svg_from_stored(svg: String) -> Svg {
    let root = svg.split('>').next().unwrap_or_default();
    let attr = |name: &str| {
        let needle = format!(" {name}=\"");
        root.find(&needle)
            .and_then(|i| root[i + needle.len()..].split('"').next())
            .unwrap_or_default()
            .to_owned()
    };
    let vertical_align = attr("style")
        .split(';')
        .find_map(|d| {
            d.trim()
                .strip_prefix("vertical-align:")
                .map(|v| v.trim().to_owned())
        })
        .unwrap_or_else(|| "0ex".into());
    Svg {
        width: attr("width"),
        height: attr("height"),
        vertical_align,
        svg,
    }
}

/// Los guiones de TeX fuera de las fórmulas: `---` es una raya (—) y `--`
/// un guion medio (–). arXiv deja el resumen como lo escribió el autor.
pub fn tex_dashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for piece in pieces(text) {
        match piece {
            Piece::Text(t) => out.push_str(&t.replace("---", "—").replace("--", "–")),
            Piece::Math { latex, display } => {
                let d = if display { "$$" } else { "$" };
                out.push_str(&format!("{d}{latex}{d}"));
            }
        }
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", "\u{a0}")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encuentra_formulas_y_respeta_los_precios() {
        let f = formulas(
            "the identity $\\bar I^{\\,2}\\bar Q/\\bar\\lambda\\equiv1$ holds; $$E=mc^2$$ too",
        );
        assert_eq!(
            f,
            vec![
                Math {
                    latex: "\\bar I^{\\,2}\\bar Q/\\bar\\lambda\\equiv1".into(),
                    display: false
                },
                Math {
                    latex: "E=mc^2".into(),
                    display: true
                },
            ]
        );
        assert!(formulas("costs $5 and $10 per hour").is_empty());
        assert!(formulas("a \\$ sign and $ x$").is_empty());
        assert!(formulas("open $x\n\ny$ across paragraphs").is_empty());
        assert_eq!(formulas("from $a$ to $b$").len(), 2);
    }

    #[test]
    fn marca_y_desmarca() {
        let p = "if $a<b$ & $$c$$ then";
        let m = marked(p);
        assert_eq!(m, "if <m id=\"0\">a&lt;b</m> &amp; <m id=\"1\">c</m> then");
        let es = "si <m id=\"0\">a&lt;b</m> y <m id=\"1\">c</m> entonces";
        assert_eq!(unmarked(es, p), "si $a<b$ y $$c$$ entonces");
    }

    #[test]
    fn html_con_imagenes_codigo_y_parrafos() {
        let original = formulas("x $a$ y $\\bad$");
        let svg = Svg {
            svg: "<svg/>".into(),
            vertical_align: "-0.1ex".into(),
            width: "1ex".into(),
            height: "2ex".into(),
        };
        let svgs = vec![Some(svg), None];
        let out = html("x $a$ y\n\n<z> $\\bad$", &original, &svgs, |k| {
            format!("/formulas/1/r{k}.svg")
        });
        assert_eq!(
            out,
            "<p>x <img class=\"atalaya-formula\" src=\"/formulas/1/r0.svg\" alt=\"a\" \
             style=\"vertical-align: -0.1ex; width: 1ex; height: 2ex\"> y</p>\
             <p>&lt;z&gt; <code class=\"atalaya-latex\">\\bad</code></p>"
        );
        // Una traducción antigua con el LaTeX cambiado usa la fórmula del original.
        let changed = html("x $a2$ y $\\bad$", &original, &svgs, |k| format!("r{k}"));
        assert!(changed.contains("src=\"r0\""), "{changed}");
    }

    #[test]
    fn guiones_de_tex_fuera_de_las_formulas() {
        assert_eq!(
            tex_dashes("bar---independent, 1--2 and $a--b$"),
            "bar—independent, 1–2 and $a--b$"
        );
    }

    #[test]
    fn saca_tamano_del_svg_guardado() {
        let s = svg_from_stored(
            "<svg style=\"vertical-align: -0.023ex;\" xmlns=\"x\" width=\"1.176ex\" height=\"1.593ex\" viewBox=\"0 0 1 1\"><g/></svg>"
                .into(),
        );
        assert_eq!(
            (
                s.width.as_str(),
                s.height.as_str(),
                s.vertical_align.as_str()
            ),
            ("1.176ex", "1.593ex", "-0.023ex")
        );
    }
}
