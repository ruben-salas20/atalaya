//! Preparación del HTML de un paper de arXiv para traducirlo (issue #38,
//! ADR 0010). Son funciones puras: ni red ni base de datos.
//!
//! El flujo es: [`prepare`] limpia la página y saca las fórmulas; el
//! traductor manda cada trozo de texto con sus marcas `<m id="N">`; [`check_markers`]
//! comprueba la respuesta; las tablas se traducen celda a celda con
//! [`table_cells`], [`cells_prompt`], [`parse_cells`] y [`fill_table`].

use ego_tree::NodeRef;
use scraper::{ElementRef, Html, Node, Selector};

/// Una fórmula del paper, tal como la dejó arXiv (LaTeX sin escapar).
pub struct Formula {
    pub latex: String,
    /// `true` si es una fórmula aparte (`display="block"`).
    pub display: bool,
}

/// Resultado de [`prepare`]: el HTML limpio y sus fórmulas, en el orden de
/// aparición. La fórmula `N` es `formulas[N]` y es la marca `<m id="N">` o la
/// imagen `data-formula="N"` del HTML.
pub struct Prepared {
    pub html: String,
    pub formulas: Vec<Formula>,
}

/// Atributos que se conservan; el resto se quita.
const KEPT_ATTRIBUTES: [&str; 7] = [
    "href",
    "src",
    "alt",
    "class",
    "colspan",
    "rowspan",
    "data-formula",
];

/// Elementos que se quitan enteros.
const DROPPED: [&str; 5] = ["script", "style", "nav", "header", "footer"];

/// Etiquetas sin cierre.
const VOID: [&str; 6] = ["img", "br", "hr", "col", "wbr", "source"];

/// Limpia la página de un paper y saca sus fórmulas.
///
/// `page` es el HTML de `https://arxiv.org/html/<id>/`. `base` es
/// `https://arxiv.org/html/` (el directorio padre, con barra final), no la
/// URL del paper: las rutas relativas de las imágenes (`2610.31646v1/fig.png`)
/// se resuelven contra él, y así sale `https://arxiv.org/html/2610.31646v1/fig.png`
/// (200). Contra la URL del paper saldría la ruta duplicada (404).
///
/// Devuelve solo el contenido de `<article class="ltx_document…">`. Si la
/// página no tiene ese artículo, devuelve `html` vacío.
pub fn prepare(page: &str, base: &str) -> Prepared {
    let base = if base.ends_with('/') {
        base.to_string()
    } else {
        format!("{base}/")
    };
    let doc = Html::parse_document(page);
    let article = Selector::parse("article.ltx_document").expect("selector válido");
    let mut out = Prepared {
        html: String::new(),
        formulas: Vec::new(),
    };
    if let Some(article) = doc.select(&article).next() {
        let mut w = Writer {
            base: &base,
            out: &mut out,
        };
        for child in article.children() {
            w.node(child);
        }
    }
    out
}

struct Writer<'a> {
    base: &'a str,
    out: &'a mut Prepared,
}

impl Writer<'_> {
    fn node(&mut self, node: NodeRef<'_, Node>) {
        match node.value() {
            Node::Text(t) => self.out.html.push_str(&escapar(t)),
            Node::Element(_) => {
                if let Some(el) = ElementRef::wrap(node) {
                    self.element(el);
                }
            }
            _ => {}
        }
    }

    fn element(&mut self, el: ElementRef<'_>) {
        let v = el.value();
        let name = v.name();
        let has_class = |c: &str| v.classes().any(|x| x == c);
        if DROPPED.contains(&name)
            || (name == "section" && has_class("ltx_bibliography"))
            || has_class("ltx_report_issue")
            || v.attr("href").is_some_and(|h| h.contains("html_feedback"))
        {
            return;
        }
        if name == "math" {
            let Some(latex) = v.attr("alttext") else {
                return;
            };
            let display = v.attr("display") == Some("block");
            let n = self.push_formula(latex, display);
            if display {
                self.out.html.push_str(&format!(
                    "<img class=\"atalaya-formula-bloque\" data-formula=\"{n}\">"
                ));
            } else {
                self.out
                    .html
                    .push_str(&format!("<m id=\"{n}\">{}</m>", escapar(latex)));
            }
            return;
        }
        if (name == "table" && has_class("ltx_equation")) || has_class("ltx_equationgroup") {
            self.equation(el);
            return;
        }
        // Las figuras SVG de arXiv van en `<object data="…svg">`, que las apps
        // no pintan: se pasan a `<img>`.
        if name == "object"
            && let Some(data) = v.attr("data")
            && v.attr("type").is_none_or(|t| t.starts_with("image/"))
        {
            let src = resolver(self.base, data);
            let class = v.attr("class").unwrap_or_default();
            self.out.html.push_str(&format!(
                "<img src=\"{}\" alt=\"Figura\" class=\"{}\">",
                escapar(&src).replace('"', "&quot;"),
                escapar(class).replace('"', "&quot;")
            ));
            return;
        }
        // Etiqueta de apertura con los atributos que sirven.
        self.out.html.push('<');
        self.out.html.push_str(name);
        // En un orden fijo: el analizador no garantiza el del original.
        for attr in KEPT_ATTRIBUTES {
            let Some(value) = v.attr(attr) else {
                continue;
            };
            let value = if name == "img" && attr == "src" {
                resolver(self.base, value)
            } else {
                value.to_string()
            };
            self.out.html.push_str(&format!(
                " {attr}=\"{}\"",
                escapar(&value).replace('"', "&quot;")
            ));
        }
        self.out.html.push('>');
        if VOID.contains(&name) {
            return;
        }
        for child in el.children() {
            self.node(child);
        }
        self.out.html.push_str(&format!("</{name}>"));
    }

    /// Una ecuación aparte (o un grupo): por cada fila, sus fórmulas como
    /// imágenes sin texto, para que no pasen por el modelo, y su número detrás.
    fn equation(&mut self, el: ElementRef<'_>) {
        let rows = Selector::parse("tr").expect("selector válido");
        let math = Selector::parse("math").expect("selector válido");
        let tag = Selector::parse("span.ltx_tag_equation").expect("selector válido");
        let mut rows: Vec<ElementRef> = el.select(&rows).collect();
        if rows.is_empty() {
            rows.push(el);
        }
        self.out.html.push_str("<div class=\"atalaya-ecuacion\">");
        for row in rows {
            for m in row.select(&math) {
                if let Some(latex) = m.value().attr("alttext") {
                    let n = self.push_formula(latex, true);
                    self.out.html.push_str(&format!(
                        "<img class=\"atalaya-formula-bloque\" data-formula=\"{n}\">"
                    ));
                }
            }
            if let Some(number) = row.select(&tag).next() {
                let number: String = number.text().collect();
                let number = number.trim();
                if !number.is_empty() {
                    self.out.html.push_str(&format!(
                        "<span class=\"atalaya-numero\">{}</span>",
                        escapar(number)
                    ));
                }
            }
        }
        self.out.html.push_str("</div>");
    }

    fn push_formula(&mut self, latex: &str, display: bool) -> usize {
        self.out.formulas.push(Formula {
            latex: latex.to_string(),
            display,
        });
        self.out.formulas.len() - 1
    }
}

/// Escapa el texto para HTML (`&`, `<`, `>`).
fn escapar(texto: &str) -> String {
    texto
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Resuelve una ruta de imagen contra `base` (`https://arxiv.org/html/`).
/// Las `data:`, las absolutas (`https://…`, `//…`) se dejan igual.
fn resolver(base: &str, src: &str) -> String {
    if src.starts_with("data:") || src.contains("://") || src.starts_with("//") {
        return src.to_string();
    }
    if let Some(ruta) = src.strip_prefix('/') {
        // Ruta absoluta: se cuelga del origen de `base`.
        let origen: Vec<&str> = base.splitn(4, '/').take(3).collect();
        return format!("{}/{ruta}", origen.join("/"));
    }
    format!("{base}{}", src.trim_start_matches("./"))
}

/// Trozos de texto que no son marcas: el texto fuera de `<m id="N">…</m>`.
fn fuera_de_marcas(texto: &str) -> String {
    let mut fuera = String::new();
    let mut resto = texto;
    while let Some(i) = resto.find("<m id=\"") {
        fuera.push_str(&resto[..i]);
        match resto[i..].find("</m>") {
            Some(f) => resto = &resto[i + f + "</m>".len()..],
            None => {
                resto = "";
            }
        }
    }
    fuera.push_str(resto);
    fuera
}

/// Quita las etiquetas y las entidades (`&nbsp;`) de un trozo de HTML.
fn texto_visible(html: &str) -> String {
    let mut out = String::new();
    let mut dentro_etiqueta = false;
    let mut resto = html;
    while let Some(c) = resto.chars().next() {
        if dentro_etiqueta {
            if c == '>' {
                dentro_etiqueta = false;
            }
        } else if c == '<' {
            dentro_etiqueta = true;
        } else if c == '&' {
            if let Some(f) = resto.find(';').filter(|f| *f <= 10) {
                resto = &resto[f + 1..];
                continue;
            }
            out.push(c);
        } else {
            out.push(c);
        }
        resto = &resto[c.len_utf8()..];
    }
    out
}

/// `true` si el trozo tiene letras fuera de las marcas `<m>`. Si no, no se
/// manda al modelo.
pub fn has_text_outside_markers(fragment: &str) -> bool {
    texto_visible(&fuera_de_marcas(fragment))
        .chars()
        .any(char::is_alphabetic)
}

/// Lee las marcas `<m id="N">LATEX</m>` de un trozo, en orden. Error si una
/// marca no se cierra.
fn marcas(texto: &str) -> Result<Vec<(String, String)>, String> {
    let mut marcas = Vec::new();
    let mut resto = texto;
    while let Some(i) = resto.find("<m id=\"") {
        let tras_id = &resto[i + "<m id=\"".len()..];
        let Some(q) = tras_id.find('"') else {
            return Err("marca sin cerrar la comilla del id".into());
        };
        let id = &tras_id[..q];
        let tras_etiqueta = tras_id[q..]
            .strip_prefix("\">")
            .ok_or_else(|| format!("marca {id} mal escrita"))?;
        let Some(f) = tras_etiqueta.find("</m>") else {
            return Err(format!("marca {id} sin cerrar"));
        };
        marcas.push((id.to_string(), tras_etiqueta[..f].to_string()));
        resto = &tras_etiqueta[f + "</m>".len()..];
    }
    Ok(marcas)
}

/// Comprueba la traducción de un trozo: las mismas marcas `<m id="N">…</m>`,
/// con el mismo LaTeX, en el mismo orden, y sin Markdown (`*`, `` ` ``) que
/// no estuviera en el original.
pub fn check_markers(original: &str, translated: &str) -> Result<(), String> {
    let orig = marcas(original)?;
    let trad = marcas(translated)?;
    if orig != trad {
        for (id, _) in &orig {
            if !trad.iter().any(|(i, _)| i == id) {
                return Err(format!("falta la marca {id}"));
            }
        }
        for (id, _) in &trad {
            if !orig.iter().any(|(i, _)| i == id) {
                return Err(format!("sobra la marca {id}"));
            }
        }
        for (id, latex) in &trad {
            if let Some((_, original_latex)) = orig.iter().find(|(i, _)| i == id)
                && original_latex != latex
            {
                return Err(format!("cambió el LaTeX de la marca {id}"));
            }
        }
        if orig.len() != trad.len() {
            return Err("una marca aparece repetida".into());
        }
        return Err("cambió el orden de las marcas".into());
    }
    let fuera_orig = fuera_de_marcas(original);
    let fuera_trad = fuera_de_marcas(translated);
    for marca_md in ["*", "`"] {
        if fuera_trad.matches(marca_md).count() > fuera_orig.matches(marca_md).count() {
            return Err(format!("se añadió Markdown («{marca_md}»)"));
        }
    }
    Ok(())
}

/// Nombre de la marca que sustituye al texto de una celda en
/// [`table_cells`]: `<atalaya-celda n="K"></atalaya-celda>`.
const CELDA_ABRE: &str = "<atalaya-celda n=\"";
const CELDA_CIERRA: &str = "\"></atalaya-celda>";

/// Celdas (`td`, `th`) de una tabla ya preparada que tienen letras. Devuelve
/// la tabla con cada una de esas celdas sustituida por
/// `<atalaya-celda n="K"></atalaya-celda>`, y el texto de cada celda en orden.
/// Los números y las celdas sin letras quedan igual.
pub fn table_cells(table_html: &str) -> (String, Vec<String>) {
    let mut salida = String::new();
    let mut textos = Vec::new();
    let mut pos = 0;
    while let Some((inicio, nombre)) = siguiente_celda(table_html, pos) {
        let Some(fin_abre) = table_html[inicio..].find('>').map(|f| inicio + f) else {
            break;
        };
        let Some(inicio_cierre) = cierre_celda(table_html, fin_abre + 1, nombre) else {
            break;
        };
        let cierre_fin = inicio_cierre + format!("</{nombre}>").len();
        let interior = &table_html[fin_abre + 1..inicio_cierre];
        salida.push_str(&table_html[pos..=fin_abre]);
        if has_text_outside_markers(interior) {
            salida.push_str(&format!("{CELDA_ABRE}{}{CELDA_CIERRA}", textos.len()));
            textos.push(interior.to_string());
        } else {
            salida.push_str(interior);
        }
        salida.push_str(&format!("</{nombre}>"));
        pos = cierre_fin;
    }
    salida.push_str(&table_html[pos..]);
    (salida, textos)
}

/// Busca desde `desde` la siguiente apertura de `td` o `th`.
fn siguiente_celda(html: &str, desde: usize) -> Option<(usize, &'static str)> {
    let mut i = desde;
    while let Some(f) = html[i..].find('<') {
        let p = i + f;
        for nombre in ["td", "th"] {
            let resto = &html[p + 1..];
            if let Some(tras) = resto.strip_prefix(nombre)
                && tras.starts_with([' ', '>', '\n', '\t'])
            {
                return Some((p, nombre));
            }
        }
        i = p + 1;
    }
    None
}

/// Busca el cierre `</nombre>` que casa con una apertura cuyo interior empieza
/// en `desde`, contando las celdas anidadas.
fn cierre_celda(html: &str, desde: usize, nombre: &str) -> Option<usize> {
    let abre = format!("<{nombre}");
    let cierra = format!("</{nombre}>");
    let mut profundidad = 1;
    let mut i = desde;
    loop {
        let c = html[i..].find('<').map(|f| i + f)?;
        if html[c..].starts_with(&cierra) {
            profundidad -= 1;
            if profundidad == 0 {
                return Some(c);
            }
            i = c + cierra.len();
        } else if html[c..].starts_with(&abre)
            && html[c + abre.len()..].starts_with([' ', '>', '\n', '\t'])
        {
            profundidad += 1;
            i = c + abre.len();
        } else {
            i = c + 1;
        }
    }
}

/// Pone los textos traducidos en la tabla que dio [`table_cells`]. La celda
/// `K` se sustituye por `cells[K]`; las que no existan quedan con su marca.
pub fn fill_table(template: &str, cells: &[String]) -> String {
    let mut salida = String::new();
    let mut resto = template;
    while let Some(i) = resto.find(CELDA_ABRE) {
        salida.push_str(&resto[..i]);
        let tras = &resto[i + CELDA_ABRE.len()..];
        let numero = tras
            .find('"')
            .and_then(|q| tras[..q].parse::<usize>().ok().map(|k| (k, q)));
        match numero {
            Some((k, q)) if tras[q..].starts_with(CELDA_CIERRA) && k < cells.len() => {
                salida.push_str(&cells[k]);
                resto = &tras[q + CELDA_CIERRA.len()..];
            }
            _ => {
                salida.push_str(CELDA_ABRE);
                resto = tras;
            }
        }
    }
    salida.push_str(resto);
    salida
}

/// Aviso para traducir las celdas de una tabla en una sola petición.
/// Cada celda va en una línea numerada «N. texto».
pub fn cells_prompt(cells: &[String]) -> String {
    let mut prompt = String::from(
        "Traduce al español cada línea numerada de la lista siguiente. \
         Es una tabla de un paper científico: cada texto es una celda.\n\
         Reglas:\n\
         - Responde solo con las mismas líneas numeradas, en el mismo orden, «N. traducción».\n\
         - Una línea por celda. No añadas explicaciones ni líneas en blanco.\n\
         - Deja igual las marcas <m id=\"N\">…</m> y su contenido, sin cambiarlas.\n\
         - No traduzcas nombres propios, siglas ni nombres de métodos (AdamW, PartCrafter).\n\
         - No uses Markdown (asteriscos ni comillas invertidas).\n\n",
    );
    for (i, celda) in cells.iter().enumerate() {
        prompt.push_str(&format!("{}. {}\n", i + 1, celda.replace('\n', " ")));
    }
    prompt
}

/// Lee la respuesta de [`cells_prompt`]. Devuelve los textos si son exactamente
/// `expected` líneas numeradas «1. …», «2. …» en orden. Si no, `None`.
pub fn parse_cells(reply: &str, expected: usize) -> Option<Vec<String>> {
    let mut celdas = Vec::new();
    for linea in reply.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let numero = celdas.len() + 1;
        let texto = linea.strip_prefix(&format!("{numero}. "))?;
        if texto.trim().is_empty() {
            return None;
        }
        celdas.push(texto.trim().to_string());
    }
    (celdas.len() == expected).then_some(celdas)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://arxiv.org/html/";

    /// Página mínima con la forma de la de arXiv: cabecera, menú, artículo y bibliografía.
    fn pagina(articulo: &str) -> String {
        format!(
            "<html><head><script>var x = 1;</script><style>p{{}}</style></head><body>\
             <nav class=\"ltx_page_navbar\">MENU</nav>\
             <header>CABECERA</header>\
             <article class=\"ltx_document ltx_authors_1line\">{articulo}\
             <section id=\"bib\" class=\"ltx_bibliography\"><ul><li>REFERENCIA</li></ul></section>\
             </article>\
             <footer>PIE</footer></body></html>"
        )
    }

    #[test]
    fn solo_contenido_del_articulo() {
        let p = prepare(&pagina("<p class=\"ltx_p\">Hola</p>"), BASE);
        assert!(p.html.contains("Hola"));
        for fuera in ["MENU", "CABECERA", "PIE", "var x", "REFERENCIA"] {
            assert!(!p.html.contains(fuera), "queda «{fuera}»: {}", p.html);
        }
        assert!(!p.html.contains("<article"));
    }

    #[test]
    fn sin_articulo_no_hay_html() {
        let p = prepare("<html><body><p>nada</p></body></html>", BASE);
        assert!(p.html.is_empty());
        assert!(p.formulas.is_empty());
    }

    #[test]
    fn formula_en_linea_pasa_a_marca_con_latex_escapado() {
        let p = prepare(
            &pagina(
                "<p class=\"ltx_p\">a <math id=\"S1.m1\" class=\"ltx_Math\" alttext=\"x_{t}&lt;y&amp;z\" display=\"inline\"><semantics><mi>x</mi></semantics></math> b</p>",
            ),
            BASE,
        );
        assert!(
            p.html.contains("a <m id=\"0\">x_{t}&lt;y&amp;z</m> b"),
            "{}",
            p.html
        );
        assert!(!p.html.contains("<math"));
        assert_eq!(p.formulas.len(), 1);
        assert_eq!(p.formulas[0].latex, "x_{t}<y&z");
        assert!(!p.formulas[0].display);
    }

    #[test]
    fn numeracion_en_orden_de_aparicion() {
        let p = prepare(
            &pagina(
                "<p class=\"ltx_p\"><math alttext=\"a\" display=\"inline\"></math> <math alttext=\"b\" display=\"inline\"></math></p>\
                 <p class=\"ltx_p\"><math alttext=\"c\" display=\"inline\"></math></p>",
            ),
            BASE,
        );
        assert!(p.html.contains("<m id=\"0\">a</m>"));
        assert!(p.html.contains("<m id=\"1\">b</m>"));
        assert!(p.html.contains("<m id=\"2\">c</m>"));
        let latex: Vec<&str> = p.formulas.iter().map(|f| f.latex.as_str()).collect();
        assert_eq!(latex, ["a", "b", "c"]);
    }

    #[test]
    fn ecuacion_aparte_pasa_a_imagen_con_numero() {
        let p = prepare(
            &pagina(
                "<table class=\"ltx_equation ltx_eqn_table\"><tbody><tr class=\"ltx_equation ltx_eqn_row\">\
                 <td class=\"ltx_eqn_cell\"><math alttext=\"u_{t}=E[x]\" display=\"block\"><semantics><mi>u</mi></semantics></math></td>\
                 <td class=\"ltx_eqn_cell ltx_eqn_eqno\"><span class=\"ltx_tag ltx_tag_equation\">(1)</span></td>\
                 </tr></tbody></table><p class=\"ltx_p\">donde <math alttext=\"n\" display=\"inline\"></math></p>",
            ),
            BASE,
        );
        assert!(!p.html.contains("<table"), "{}", p.html);
        assert!(
            !p.html.contains("u_{t}"),
            "la fórmula de bloque no debe llegar como texto"
        );
        assert!(
            p.html.contains(
                "<img class=\"atalaya-formula-bloque\" data-formula=\"0\"><span class=\"atalaya-numero\">(1)</span>"
            ),
            "{}",
            p.html
        );
        assert!(p.html.contains("donde <m id=\"1\">n</m>"), "{}", p.html);
        assert_eq!(p.formulas.len(), 2);
        assert!(p.formulas[0].display);
        assert_eq!(p.formulas[0].latex, "u_{t}=E[x]");
        assert!(!p.formulas[1].display);
    }

    #[test]
    fn ecuacion_aparte_sin_numero_y_bloque_suelto() {
        let p = prepare(
            &pagina(
                "<table class=\"ltx_equation\"><tr><td><math alttext=\"q\" display=\"block\"></math></td></tr></table>\
                 <p><math alttext=\"r\" display=\"block\"></math></p>",
            ),
            BASE,
        );
        assert!(
            p.html
                .contains("<img class=\"atalaya-formula-bloque\" data-formula=\"0\">")
        );
        assert!(!p.html.contains("atalaya-numero"));
        assert!(
            p.html
                .contains("<img class=\"atalaya-formula-bloque\" data-formula=\"1\">")
        );
        assert!(p.formulas.iter().all(|f| f.display));
    }

    #[test]
    fn imagenes_relativas_se_resuelven_contra_la_base() {
        let p = prepare(
            &pagina(
                "<img src=\"2610.31646v1/fig_design.png\" alt=\"Refer to caption\">\
                 <img src=\"data:image/png;base64,AAAA\">\
                 <img src=\"https://static.arxiv.org/logo.svg\">",
            ),
            BASE,
        );
        assert!(
            p.html
                .contains("src=\"https://arxiv.org/html/2610.31646v1/fig_design.png\""),
            "{}",
            p.html
        );
        assert!(p.html.contains("src=\"data:image/png;base64,AAAA\""));
        assert!(p.html.contains("src=\"https://static.arxiv.org/logo.svg\""));
        assert!(p.html.contains("alt=\"Refer to caption\""));
    }

    #[test]
    fn quita_atributos_que_no_sirven() {
        let p = prepare(
            &pagina(
                "<p id=\"S1.p1\" class=\"ltx_p\" style=\"color:red\" intent=\"x\" aria-hidden=\"true\">\
                 <a href=\"#S1\" title=\"t\" class=\"ltx_ref\">enlace</a></p>\
                 <table class=\"ltx_tabular\"><tr><td id=\"c1\" colspan=\"2\" rowspan=\"1\" align=\"left\">x</td></tr></table>",
            ),
            BASE,
        );
        assert!(p.html.contains("<p class=\"ltx_p\">"), "{}", p.html);
        assert!(
            p.html
                .contains("<a href=\"#S1\" class=\"ltx_ref\">enlace</a>"),
            "{}",
            p.html
        );
        assert!(
            p.html.contains("<td colspan=\"2\" rowspan=\"1\">x</td>"),
            "{}",
            p.html
        );
        for quitado in ["id=", "style=", "intent=", "aria-", "title=", "align="] {
            assert!(!p.html.contains(quitado), "queda {quitado}: {}", p.html);
        }
    }

    #[test]
    fn quita_botones_de_informar_problema() {
        let p = prepare(
            &pagina(
                "<p class=\"ltx_p\">texto</p><a href=\"https://github.com/arXiv/html_feedback/issues/new\">Report issue</a>",
            ),
            BASE,
        );
        assert!(!p.html.contains("Report issue"), "{}", p.html);
        assert!(p.html.contains("texto"));
    }

    #[test]
    fn figuras_svg_en_object_pasan_a_img() {
        let page = "<article class=\"ltx_document\"><figure>\
            <object type=\"image/svg+xml\" data=\"2610.31646v1/fig1.svg\" class=\"ltx_graphics\" width=\"476\"></object>\
            </figure></article>";
        let p = prepare(page, "https://arxiv.org/html/");
        assert!(
            p.html.contains("<img src=\"https://arxiv.org/html/2610.31646v1/fig1.svg\" alt=\"Figura\" class=\"ltx_graphics\">"),
            "{}",
            p.html
        );
        assert!(!p.html.contains("<object"), "{}", p.html);
    }

    #[test]
    fn marcas_bien_hechas_se_aceptan() {
        assert!(
            check_markers(
                "<m id=\"0\">\\to</m> comprimir",
                "comprimir <m id=\"0\">\\to</m>"
            )
            .is_ok()
        );
        assert!(check_markers("a <m id=\"0\">x</m> *b*", "a <m id=\"0\">x</m> *b*").is_ok());
        assert!(check_markers("sin marcas", "sin marcas traducidas").is_ok());
    }

    #[test]
    fn check_markers_rechaza_marca_perdida() {
        let e = check_markers(
            "<m id=\"0\">x</m> y <m id=\"1\">z</m>",
            "<m id=\"0\">x</m> y",
        )
        .unwrap_err();
        assert!(e.contains("falta la marca 1"), "{e}");
    }

    #[test]
    fn check_markers_rechaza_latex_cambiado() {
        let e = check_markers(
            "<m id=\"0\">\\rightarrow</m>",
            "<m id=\"0\">\\leftarrow</m>",
        )
        .unwrap_err();
        assert!(e.contains("LaTeX de la marca 0"), "{e}");
    }

    #[test]
    fn check_markers_rechaza_orden_cambiado() {
        let e = check_markers(
            "<m id=\"0\">a</m> y <m id=\"1\">b</m>",
            "<m id=\"1\">b</m> y <m id=\"0\">a</m>",
        )
        .unwrap_err();
        assert!(e.contains("orden"), "{e}");
    }

    #[test]
    fn check_markers_rechaza_markdown_anadido() {
        let e = check_markers(
            "el <m id=\"0\">x</m> embeddings",
            "el <m id=\"0\">x</m> *embeddings*",
        )
        .unwrap_err();
        assert!(e.contains("Markdown"), "{e}");
        assert!(check_markers("`code` y <m id=\"0\">x</m>", "`code` y <m id=\"0\">x</m>").is_ok());
        assert!(check_markers("hecho", "hecho `así`").is_err());
    }

    #[test]
    fn letras_fuera_de_las_marcas() {
        assert!(!has_text_outside_markers("<m id=\"0\">\\alpha</m>"));
        assert!(!has_text_outside_markers(
            "<m id=\"0\">\\alpha</m> 1.25 &nbsp; <span>(2)</span>"
        ));
        assert!(has_text_outside_markers("<m id=\"0\">x</m> palabra"));
        assert!(has_text_outside_markers(
            "<span class=\"ltx_text\">Box</span>"
        ));
        assert!(!has_text_outside_markers(
            "<img src=\"a.png\" alt=\"Refer\">"
        ));
    }

    #[test]
    fn tabla_ida_y_vuelta() {
        let tabla = "<table class=\"ltx_tabular\"><tbody>\
            <tr><th><span class=\"ltx_text\">Backbone learning rate</span></th><td>0.0003</td></tr>\
            <tr><td>12</td><td colspan=\"2\"><m id=\"0\">\\alpha</m> escala</td></tr>\
            <tr><td>Box</td><td>1.5</td></tr>\
            </tbody></table>";
        let (plantilla, textos) = table_cells(tabla);
        // Solo las celdas con letras fuera de marcas: la cabecera, «escala» y «Box».
        assert_eq!(
            textos,
            vec![
                "<span class=\"ltx_text\">Backbone learning rate</span>".to_string(),
                "<m id=\"0\">\\alpha</m> escala".to_string(),
                "Box".to_string(),
            ]
        );
        assert!(plantilla.contains("<td>0.0003</td>"), "{plantilla}");
        assert!(plantilla.contains("<td>12</td>"), "{plantilla}");
        assert!(
            plantilla.contains("<atalaya-celda n=\"1\"></atalaya-celda>"),
            "{plantilla}"
        );
        // Con los mismos textos, la tabla vuelve idéntica.
        assert_eq!(fill_table(&plantilla, &textos), tabla);
        // Con textos traducidos, cada celda cambia y las demás no.
        let traducido = vec![
            "Tasa».".to_string(),
            "<m id=\"0\">\\alpha</m> escala".to_string(),
            "Caja".to_string(),
        ];
        let salida = fill_table(&plantilla, &traducido);
        assert!(salida.contains("<td>Caja</td>"), "{salida}");
        assert!(salida.contains("<td>1.5</td>"), "{salida}");
        assert!(!salida.contains("atalaya-celda"), "{salida}");
    }

    #[test]
    fn prompt_y_lectura_de_celdas() {
        let celdas = vec!["Box".to_string(), "Cond. <m id=\"0\">x</m>".to_string()];
        let prompt = cells_prompt(&celdas);
        assert!(
            prompt.contains("1. Box\n2. Cond. <m id=\"0\">x</m>\n"),
            "{prompt}"
        );
        assert_eq!(
            parse_cells("1. Caja\n2. Cond. <m id=\"0\">x</m>\n", 2),
            Some(vec![
                "Caja".to_string(),
                "Cond. <m id=\"0\">x</m>".to_string()
            ])
        );
        // Número de líneas distinto, orden distinto o texto sin numerar: no se acepta.
        assert_eq!(parse_cells("1. Caja\n", 2), None);
        assert_eq!(parse_cells("2. Caja\n1. Otra\n", 2), None);
        assert_eq!(parse_cells("Caja\nOtra\n", 2), None);
        assert_eq!(parse_cells("1. Caja\n2. Otra\n3. Extra\n", 2), None);
        assert_eq!(parse_cells("1. Caja\n2.\n", 2), None);
    }
}
