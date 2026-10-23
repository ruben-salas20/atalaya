//! Cliente de la API de arXiv (ADR 0005).
//!
//! arXiv pide no pasar de una petición cada 3 segundos. Todas las peticiones
//! del servicio pasan por un mismo candado que guarda la hora de la última:
//! si llegan varias a la vez, esperan en fila. Las respuestas se guardan en
//! memoria un rato, para que repetir una búsqueda o pasar de página atrás y
//! adelante no espere otra vez.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use atalaya_api::{PaperField, PaperSort};
use chrono::{DateTime, Utc};
use quick_xml::{Reader, events::Event};

use crate::error::AppError;

const API_URL: &str = "https://export.arxiv.org/api/query";

/// Espera mínima entre dos peticiones a arXiv.
const MIN_INTERVAL: Duration = Duration::from_secs(3);

/// Cuánto vale una respuesta guardada.
const CACHE_TTL: Duration = Duration::from_secs(15 * 60);

/// Respuestas guardadas como mucho. Al pasar, se borran las caducadas y, si
/// no basta, todas.
const CACHE_MAX: usize = 200;

const TIMEOUT: Duration = Duration::from_secs(30);

/// Palabras que no se mandan a arXiv: no ayudan a buscar y `and`, `or` y
/// `not` son operadores de su API.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "of", "for", "in", "on", "and", "or", "not", "to", "with", "by", "at",
    "from", "andnot",
];

/// Un paper tal como lo da arXiv.
#[derive(Debug, Clone, PartialEq)]
pub struct ArxivPaper {
    /// Sin versión: `2610.08812` o `hep-th/9901001`.
    pub arxiv_id: String,
    /// Con los espacios normalizados: arXiv parte los títulos largos en líneas.
    pub title: String,
    /// Párrafos separados por una línea en blanco.
    pub abstract_text: String,
    pub authors: Vec<String>,
    pub published_at: DateTime<Utc>,
    pub category: String,
}

/// Una página de resultados.
#[derive(Debug, Clone, PartialEq)]
pub struct ArxivPage {
    pub total: u64,
    pub papers: Vec<ArxivPaper>,
}

pub struct Arxiv {
    http: reqwest::Client,
    base_url: String,
    /// Hora de la última petición. El candado es de Tokio porque se tiene
    /// mientras se espera y mientras dura la petición.
    last: tokio::sync::Mutex<Option<Instant>>,
    cache: Mutex<HashMap<String, (Instant, String)>>,
}

impl Arxiv {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_url(http, API_URL)
    }

    pub fn with_url(http: reqwest::Client, base_url: &str) -> Self {
        Self {
            http,
            base_url: base_url.to_owned(),
            last: tokio::sync::Mutex::new(None),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub async fn search(
        &self,
        english: &str,
        field: PaperField,
        sort: PaperSort,
        start: u32,
        max_results: u32,
    ) -> Result<ArxivPage, AppError> {
        let query = search_query(english, field)
            .ok_or_else(|| AppError::bad_request("la búsqueda no tiene palabras que buscar"))?;
        let (sort_by, order) = match sort {
            PaperSort::Recent => ("submittedDate", "descending"),
            PaperSort::Relevance => ("relevance", "descending"),
        };
        let params = [
            ("search_query", query),
            ("sortBy", sort_by.into()),
            ("sortOrder", order.into()),
            ("start", start.to_string()),
            ("max_results", max_results.to_string()),
        ];
        let xml = self.get(&params).await?;
        parse(&xml).map_err(|e| AppError::bad_gateway(format!("respuesta de arXiv no válida: {e}")))
    }

    /// Un paper por su identificador, o `None` si arXiv no lo conoce.
    pub async fn paper(&self, arxiv_id: &str) -> Result<Option<ArxivPaper>, AppError> {
        let params = [
            ("id_list", arxiv_id.to_owned()),
            ("max_results", "1".into()),
        ];
        let xml = self.get(&params).await?;
        let page = parse(&xml)
            .map_err(|e| AppError::bad_gateway(format!("respuesta de arXiv no válida: {e}")))?;
        Ok(page.papers.into_iter().find(|p| p.arxiv_id == arxiv_id))
    }

    async fn get(&self, params: &[(&str, String)]) -> Result<String, AppError> {
        let url = reqwest::Url::parse_with_params(&self.base_url, params)
            .map_err(|e| AppError::bad_request(format!("búsqueda no válida: {e}")))?
            .to_string();
        if let Some(xml) = self.cached(&url) {
            return Ok(xml);
        }

        let mut last = self.last.lock().await;
        // Otra petición igual pudo llenar la caché mientras se esperaba.
        if let Some(xml) = self.cached(&url) {
            return Ok(xml);
        }
        if let Some(at) = *last {
            tokio::time::sleep_until((at + MIN_INTERVAL).into()).await;
        }
        let result = self.fetch(&url).await;
        *last = Some(Instant::now());
        drop(last);

        let xml = result?;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= CACHE_MAX {
            cache.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
            if cache.len() >= CACHE_MAX {
                cache.clear();
            }
        }
        cache.insert(url, (Instant::now(), xml.clone()));
        Ok(xml)
    }

    fn cached(&self, url: &str) -> Option<String> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .get(url)
            .filter(|(at, _)| at.elapsed() < CACHE_TTL)
            .map(|(_, xml)| xml.clone())
    }

    async fn fetch(&self, url: &str) -> Result<String, AppError> {
        let started = Instant::now();
        let resp = self
            .http
            .get(url)
            .timeout(TIMEOUT)
            .send()
            .await
            .map_err(|e| AppError::bad_gateway(format!("no se pudo hablar con arXiv: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(AppError::bad_gateway(format!("arXiv respondió {status}")));
        }
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::bad_gateway(format!("respuesta de arXiv cortada: {e}")))?;
        tracing::debug!(secs = started.elapsed().as_secs_f32(), "arXiv: {url}");
        Ok(text)
    }
}

/// La búsqueda en la sintaxis de arXiv: cada palabra con el prefijo del
/// campo, unidas con `AND`. Lo que va entre comillas se busca como frase. Con
/// `Author`, el nombre entero va como frase. `None` si no queda nada.
pub fn search_query(english: &str, field: PaperField) -> Option<String> {
    let prefix = match field {
        PaperField::All => "all",
        PaperField::Title => "ti",
        PaperField::Author => "au",
        PaperField::Abstract => "abs",
    };
    let mut terms = Vec::new();
    if field == PaperField::Author {
        let name = words(english).join(" ");
        if !name.is_empty() {
            terms.push(phrase(prefix, &name));
        }
    } else {
        for (i, chunk) in english.split('"').enumerate() {
            // Los trozos impares estaban entre comillas.
            if i % 2 == 1 {
                let ws = words(chunk);
                if ws.len() > 1 {
                    terms.push(phrase(prefix, &ws.join(" ")));
                    continue;
                }
                terms.extend(ws.iter().map(|w| format!("{prefix}:{w}")));
                continue;
            }
            for w in words(chunk) {
                if !STOPWORDS.contains(&w.to_lowercase().as_str()) {
                    terms.push(format!("{prefix}:{w}"));
                }
            }
        }
    }
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

fn phrase(prefix: &str, text: &str) -> String {
    if text.contains(' ') {
        format!("{prefix}:\"{text}\"")
    } else {
        format!("{prefix}:{text}")
    }
}

/// Palabras sueltas, sin los signos que la API de arXiv interpreta.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || "():,;\"!?¿¡[]{}".contains(c))
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Identificador sin la versión del final: `2610.08812v2` → `2610.08812`.
pub fn strip_version(id: &str) -> &str {
    match id.rfind('v') {
        Some(i) if i > 0 && id[i + 1..].chars().all(|c| c.is_ascii_digit()) && i + 1 < id.len() => {
            &id[..i]
        }
        _ => id,
    }
}

/// Si parece un identificador de arXiv, nuevo (`2610.08812`) o antiguo
/// (`hep-th/9901001`, `math.GT/0309136`), sin versión.
pub fn is_valid_id(id: &str) -> bool {
    let new = |s: &str| {
        let Some((a, b)) = s.split_once('.') else {
            return false;
        };
        a.len() == 4
            && (4..=5).contains(&b.len())
            && a.chars().chain(b.chars()).all(|c| c.is_ascii_digit())
    };
    let old = |s: &str| {
        let Some((archive, num)) = s.split_once('/') else {
            return false;
        };
        !archive.is_empty()
            && archive
                .chars()
                .all(|c| c.is_ascii_alphabetic() || c == '-' || c == '.')
            && num.len() == 7
            && num.chars().all(|c| c.is_ascii_digit())
    };
    new(id) || old(id)
}

/// Página del paper.
pub fn abs_url(arxiv_id: &str) -> String {
    format!("https://arxiv.org/abs/{arxiv_id}")
}

pub fn pdf_url(arxiv_id: &str) -> String {
    format!("https://arxiv.org/pdf/{arxiv_id}")
}

/// Lee la respuesta Atom de arXiv. Si arXiv responde con un error (por
/// ejemplo, un `id_list` mal formado), lo hace con una sola entrada cuyo
/// título es «Error»: se devuelve como error.
pub fn parse(xml: &str) -> Result<ArxivPage, String> {
    let mut reader = Reader::from_str(xml);
    let mut total = None;
    let mut papers = Vec::new();
    let mut current: Option<Draft> = None;
    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();

    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) => {
                let name = e.name().as_ref().to_owned();
                if name == "entry" {
                    current = Some(Draft::default());
                }
                if let Some(d) = current.as_mut() {
                    d.read_attrs(&name, &e)?;
                }
                path.push(name);
                text.clear();
            }
            Event::Empty(e) => {
                let name = e.name().as_ref().to_owned();
                if let Some(d) = current.as_mut() {
                    d.read_attrs(&name, &e)?;
                }
            }
            Event::Text(t) => {
                // Desde quick-xml 0.38 el texto llega sin entidades: van aparte.
                text.push_str(&t.xml10_content());
            }
            Event::GeneralRef(r) => {
                let entity = format!("&{};", r.xml10_content());
                text.push_str(&quick_xml::escape::unescape(&entity).map_err(|e| e.to_string())?);
            }
            Event::CData(c) => text.push_str(&c.xml10_content()),
            Event::End(_) => {
                let name = path.pop().unwrap_or_default();
                let parent = path.last().map(String::as_str);
                match (current.as_mut(), name.as_str(), parent) {
                    (_, "opensearch:totalResults", Some("feed")) => {
                        total = text.trim().parse().ok();
                    }
                    (Some(d), "id", Some("entry")) => d.id = text.trim().to_owned(),
                    (Some(d), "title", Some("entry")) => d.title = collapse(&text),
                    (Some(d), "summary", Some("entry")) => d.summary = text.clone(),
                    (Some(d), "published", Some("entry")) => d.published = text.trim().to_owned(),
                    (Some(d), "name", Some("author")) => d.authors.push(collapse(&text)),
                    (Some(_), "entry", _) => {
                        let draft = current.take().expect("hay entrada");
                        papers.push(draft.finish()?);
                    }
                    _ => {}
                }
                text.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if let [only] = papers.as_slice()
        && only.title == "Error"
    {
        return Err(format!("arXiv: {}", collapse(&only.abstract_text)));
    }
    Ok(ArxivPage {
        total: total.unwrap_or(papers.len() as u64),
        papers,
    })
}

#[derive(Default)]
struct Draft {
    id: String,
    title: String,
    summary: String,
    published: String,
    authors: Vec<String>,
    primary_category: Option<String>,
    first_category: Option<String>,
}

impl Draft {
    fn read_attrs(&mut self, name: &str, e: &quick_xml::events::BytesStart) -> Result<(), String> {
        if name != "arxiv:primary_category" && name != "category" {
            return Ok(());
        }
        for attr in e.attributes() {
            let attr = attr.map_err(|e| e.to_string())?;
            if attr.key.as_ref() == "term" {
                let term = attr
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map_err(|e| e.to_string())?
                    .into_owned();
                if name == "category" {
                    self.first_category.get_or_insert(term);
                } else {
                    self.primary_category = Some(term);
                }
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<ArxivPaper, String> {
        // `http://arxiv.org/abs/2610.08812v1` → `2610.08812`.
        let id = self
            .id
            .split_once("/abs/")
            .map_or(self.id.as_str(), |(_, id)| id);
        let published_at = DateTime::parse_from_rfc3339(&self.published)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or(DateTime::UNIX_EPOCH);
        Ok(ArxivPaper {
            arxiv_id: strip_version(id).to_owned(),
            title: self.title,
            abstract_text: paragraphs(&self.summary),
            authors: self.authors,
            published_at,
            category: self
                .primary_category
                .or(self.first_category)
                .unwrap_or_default(),
        })
    }
}

/// Todos los espacios y saltos seguidos, en un solo espacio.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// El resumen en párrafos separados por una línea en blanco. arXiv parte a
/// veces las líneas largas: un salto suelto no separa párrafos; una línea en
/// blanco o una línea con sangría, sí.
pub fn paragraphs(summary: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in summary.trim().lines() {
        let starts_paragraph = line.trim().is_empty() || line.starts_with("  ");
        if starts_paragraph && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        let line = collapse(line);
        if !line.is_empty() {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&line);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version='1.0' encoding='UTF-8'?>
<feed xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/" xmlns:arxiv="http://arxiv.org/schemas/atom" xmlns="http://www.w3.org/2005/Atom">
  <id>https://arxiv.org/api/x</id>
  <title>arXiv Query: search_query=all:small</title>
  <opensearch:itemsPerPage>2</opensearch:itemsPerPage>
  <opensearch:totalResults>12644</opensearch:totalResults>
  <entry>
    <id>http://arxiv.org/abs/2610.31483v2</id>
    <title>Training Small Language Models
  for Transferable Tool Selection</title>
    <updated>2026-10-22T16:07:49Z</updated>
    <link href="https://arxiv.org/abs/2610.31483v1" rel="alternate" type="text/html"/>
    <summary>As LLM agents operate &amp; select tools &lt;fast&gt;.
  A second paragraph.</summary>
    <category term="cs.AI" scheme="http://arxiv.org/schemas/atom"/>
    <category term="cs.CL" scheme="http://arxiv.org/schemas/atom"/>
    <published>2026-10-22T16:07:49Z</published>
    <arxiv:primary_category term="cs.CL"/>
    <author><name>Wei Chen</name></author>
    <author><name>Lucía Ortega</name></author>
  </entry>
  <entry>
    <id>http://arxiv.org/abs/hep-th/9901001v1</id>
    <title>Old one</title>
    <summary>Text.</summary>
    <category term="hep-th" scheme="http://arxiv.org/schemas/atom"/>
    <published>1999-01-01T00:00:00Z</published>
    <author><name>A. Author</name></author>
  </entry>
</feed>"#;

    #[test]
    fn parses_entries() {
        let page = parse(SAMPLE).unwrap();
        assert_eq!(page.total, 12644);
        assert_eq!(page.papers.len(), 2);
        let p = &page.papers[0];
        assert_eq!(p.arxiv_id, "2610.31483");
        assert_eq!(
            p.title,
            "Training Small Language Models for Transferable Tool Selection"
        );
        assert_eq!(
            p.abstract_text,
            "As LLM agents operate & select tools <fast>.\n\nA second paragraph."
        );
        assert_eq!(p.authors, ["Wei Chen", "Lucía Ortega"]);
        assert_eq!(p.category, "cs.CL", "manda la categoría principal");
        assert_eq!(p.published_at.to_rfc3339(), "2026-10-22T16:07:49+00:00");
        let old = &page.papers[1];
        assert_eq!(old.arxiv_id, "hep-th/9901001");
        assert_eq!(old.category, "hep-th");
    }

    #[test]
    fn arxiv_error_is_an_error() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><id>x</id>
            <title>Error</title><summary>incorrect id format for 1234</summary>
            <published>2020-01-01T00:00:00Z</published></entry></feed>"#;
        assert_eq!(
            parse(xml).unwrap_err(),
            "arXiv: incorrect id format for 1234"
        );
    }

    #[test]
    fn builds_queries() {
        assert_eq!(
            search_query("small language models for agents", PaperField::All).as_deref(),
            Some("all:small AND all:language AND all:models AND all:agents")
        );
        assert_eq!(
            search_query("\"small language models\" edge", PaperField::Title).as_deref(),
            Some("ti:\"small language models\" AND ti:edge")
        );
        assert_eq!(
            search_query("Wei Chen", PaperField::Author).as_deref(),
            Some("au:\"Wei Chen\"")
        );
        assert_eq!(
            search_query("LLM: (quantization)?", PaperField::Abstract).as_deref(),
            Some("abs:LLM AND abs:quantization")
        );
        assert_eq!(search_query(" the of ", PaperField::All), None);
    }

    #[test]
    fn ids() {
        assert_eq!(strip_version("2610.08812v12"), "2610.08812");
        assert_eq!(strip_version("2610.08812"), "2610.08812");
        assert_eq!(strip_version("hep-th/9901001v1"), "hep-th/9901001");
        assert!(is_valid_id("2610.08812"));
        assert!(is_valid_id("0704.0001"));
        assert!(is_valid_id("hep-th/9901001"));
        assert!(is_valid_id("math.GT/0309136"));
        assert!(!is_valid_id("2610.08812v1"));
        assert!(!is_valid_id("hola"));
        assert!(!is_valid_id("../etc"));
    }

    #[test]
    fn keeps_paragraphs() {
        assert_eq!(paragraphs("  One\nline.\n\nTwo.  "), "One line.\n\nTwo.");
    }
}
