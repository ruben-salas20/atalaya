//! Montaje del HTML del paper completo: tablas fuera del texto mientras se
//! traduce y fórmulas como imágenes al entregarlo (ADR 0010).

use super::mathjax::Svg;

/// Saca cada tabla de datos (`table.ltx_tabular` más externa) y deja en su
/// sitio `<img data-tabla="K">`, que `content::split` copia tal cual. Así sus
/// celdas no pasan por el modelo una a una: van juntas ([`super::prep`]).
pub fn extract_tables(html: &str) -> (String, Vec<String>) {
    let mut out = String::with_capacity(html.len());
    let mut tables = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<table class=\"ltx_tabular") {
        let Some(end) = table_end(&rest[start..]) else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push_str(&format!("<img data-tabla=\"{}\">", tables.len()));
        tables.push(rest[start..start + end].to_owned());
        rest = &rest[start + end..];
    }
    out.push_str(rest);
    (out, tables)
}

/// Longitud de la tabla que empieza en `html`, contando las anidadas.
fn table_end(html: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = 0;
    while let Some(off) = html[i..].find("<table").into_iter().chain(html[i..].find("</table>")).min() {
        let at = i + off;
        if html[at..].starts_with("</table>") {
            depth = depth.checked_sub(1)?;
            i = at + "</table>".len();
            if depth == 0 {
                return Some(i);
            }
        } else {
            depth += 1;
            i = at + "<table".len();
        }
    }
    None
}

/// Vuelve a poner las tablas en su sitio.
pub fn put_tables(html: &str, tables: &[String]) -> String {
    let mut out = html.to_owned();
    for (k, table) in tables.iter().enumerate() {
        out = out.replacen(&format!("<img data-tabla=\"{k}\">"), table, 1);
    }
    out
}

/// Una fórmula para entregar a la app: su LaTeX, si va aparte y su SVG, si
/// MathJax pudo con ella.
pub struct FormulaOut<'a> {
    pub latex: &'a str,
    pub display: bool,
    pub svg: Option<&'a Svg>,
}

/// Cambia cada `<m id="N">…</m>` y cada `<img … data-formula="N">` por la
/// fórmula `N`: una imagen del servicio (`/formulas/<id>/<N>.svg`) o, sin SVG,
/// su LaTeX como código.
pub fn put_formulas(html: &str, arxiv_id: &str, formulas: &[FormulaOut]) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let inline = rest.find("<m id=\"");
        let block = rest.find("data-formula=\"").and_then(|at| rest[..at].rfind("<img").map(|s| (s, at)));
        let next = match (inline, block) {
            (Some(i), Some((b, _))) if b < i => Next::Block(b),
            (Some(i), _) => Next::Inline(i),
            (None, Some((b, _))) => Next::Block(b),
            (None, None) => break,
        };
        match next {
            Next::Inline(start) => {
                let after = &rest[start + "<m id=\"".len()..];
                let n = number(after);
                let Some(end) = rest[start..].find("</m>") else {
                    break;
                };
                out.push_str(&rest[..start]);
                out.push_str(&formula(arxiv_id, n, formulas, false));
                rest = &rest[start + end + "</m>".len()..];
            }
            Next::Block(start) => {
                let Some(close) = rest[start..].find('>') else {
                    break;
                };
                let tag = &rest[start..start + close + 1];
                let n = tag
                    .find("data-formula=\"")
                    .and_then(|at| number(&tag[at + "data-formula=\"".len()..]));
                out.push_str(&rest[..start]);
                out.push_str(&formula(arxiv_id, n, formulas, true));
                rest = &rest[start + close + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

enum Next {
    Inline(usize),
    Block(usize),
}

fn number(s: &str) -> Option<usize> {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn formula(arxiv_id: &str, n: Option<usize>, formulas: &[FormulaOut], block: bool) -> String {
    let Some((n, f)) = n.and_then(|n| formulas.get(n).map(|f| (n, f))) else {
        return String::new();
    };
    let class = if block || f.display {
        "atalaya-formula atalaya-formula-bloque"
    } else {
        "atalaya-formula"
    };
    match f.svg {
        Some(svg) => format!(
            "<img class=\"{class}\" src=\"/formulas/{arxiv_id}/{n}.svg\" alt=\"{}\" \
             style=\"vertical-align: {}; width: {}; height: {}\">",
            escape(f.latex),
            escape(&svg.vertical_align),
            escape(&svg.width),
            escape(&svg.height),
        ),
        None => format!("<code class=\"atalaya-latex\">{}</code>", escape(f.latex)),
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_go_out_and_back() {
        let html = "<p>a</p><table class=\"ltx_tabular x\"><tr><td><table class=\"ltx_tabular\">\
                    <tr><td>in</td></tr></table></td></tr></table><p>b</p>";
        let (out, tables) = extract_tables(html);
        assert_eq!(out, "<p>a</p><img data-tabla=\"0\"><p>b</p>");
        assert_eq!(tables.len(), 1);
        assert!(tables[0].ends_with("</table></td></tr></table>"));
        assert_eq!(put_tables(&out, &tables), html);
    }

    #[test]
    fn formulas_become_images_or_latex() {
        let svg = Svg {
            svg: "<svg/>".into(),
            vertical_align: "-0.5ex".into(),
            width: "3ex".into(),
            height: "2ex".into(),
        };
        let formulas = [
            FormulaOut { latex: "x<y", display: false, svg: Some(&svg) },
            FormulaOut { latex: "\\lx@a", display: false, svg: None },
            FormulaOut { latex: "E=mc^2", display: true, svg: Some(&svg) },
        ];
        let html = "<p>Si <m id=\"0\">x&lt;y</m> y <m id=\"1\">\\lx@a</m>:</p>\
                    <img class=\"atalaya-formula-bloque\" data-formula=\"2\"><p>fin</p>";
        let out = put_formulas(html, "2610.1v1", &formulas);
        assert_eq!(
            out,
            "<p>Si <img class=\"atalaya-formula\" src=\"/formulas/2610.1v1/0.svg\" alt=\"x&lt;y\" \
             style=\"vertical-align: -0.5ex; width: 3ex; height: 2ex\"> y \
             <code class=\"atalaya-latex\">\\lx@a</code>:</p>\
             <img class=\"atalaya-formula atalaya-formula-bloque\" src=\"/formulas/2610.1v1/2.svg\" \
             alt=\"E=mc^2\" style=\"vertical-align: -0.5ex; width: 3ex; height: 2ex\"><p>fin</p>"
        );
    }
}
