//! Detección del idioma de un artículo (T3).
//!
//! Un título solo da poco texto y la detección falla a menudo. Por eso se
//! mira el título junto con el principio del contenido, sin etiquetas HTML,
//! y solo entre los idiomas que pueden llegar. Si aun así no hay seguridad,
//! `store` usa el idioma mayoritario del feed.

use whatlang::{Detector, Lang};

/// Cuánto texto se mira como mucho. Más no mejora la detección.
const SAMPLE_CHARS: usize = 1500;

/// Confianza mínima cuando whatlang no da la detección por fiable. Con 0,5,
/// ningún título de prueba sale con un idioma equivocado.
const MIN_CONFIDENCE: f64 = 0.5;

/// Idiomas entre los que se elige. Con menos candidatos, un título corto se
/// confunde menos: sin esta lista, «Just Eat apologises…» salía en catalán.
const CANDIDATES: [Lang; 8] = [
    Lang::Eng,
    Lang::Spa,
    Lang::Por,
    Lang::Fra,
    Lang::Deu,
    Lang::Ita,
    Lang::Cmn,
    Lang::Jpn,
];

/// Idioma del artículo en ISO 639-1 (`"en"`), o `None` si no hay seguridad.
pub fn detect(title: &str, content_html: &str) -> Option<String> {
    let mut sample = String::with_capacity(SAMPLE_CHARS);
    sample.push_str(title);
    sample.push('\n');
    sample.push_str(&html_to_text(content_html));
    let sample: String = sample.chars().take(SAMPLE_CHARS).collect();
    detect_text(&sample)
}

/// Idioma de un texto ya sin HTML, en ISO 639-1, o `None` si no hay seguridad.
/// Lo usan tanto `detect` como las estadísticas de un feed: así las dos partes
/// detectan igual. Quien llama decide cuánto texto pasa.
pub fn detect_text(text: &str) -> Option<String> {
    let info = Detector::with_allowlist(CANDIDATES.to_vec()).detect(text)?;
    if !info.is_reliable() && info.confidence() < MIN_CONFIDENCE {
        return None;
    }
    Some(iso_639_1(info.lang()).to_owned())
}

/// whatlang usa ISO 639-3; la API, ISO 639-1.
fn iso_639_1(lang: Lang) -> &'static str {
    match lang {
        Lang::Eng => "en",
        Lang::Spa => "es",
        Lang::Por => "pt",
        Lang::Fra => "fr",
        Lang::Deu => "de",
        Lang::Ita => "it",
        Lang::Cmn => "zh",
        Lang::Jpn => "ja",
        other => other.code(),
    }
}

/// Texto visible de un fragmento HTML: quita etiquetas, `<script>` y
/// `<style>`, y traduce las entidades más comunes. Se queda con el principio.
pub fn html_to_text(html: &str) -> String {
    let limit = SAMPLE_CHARS * 2;
    let mut out = String::with_capacity(html.len().min(limit));
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let head = rest.as_bytes()[..rest.len().min(8)].to_ascii_lowercase();
        let close = if head.starts_with(b"<script") {
            Some("</script>")
        } else if head.starts_with(b"<style") {
            Some("</style>")
        } else {
            None
        };
        // `to_ascii_lowercase` no cambia la longitud: los índices valen en `rest`.
        let end = match close {
            Some(close) => rest
                .to_ascii_lowercase()
                .find(close)
                .map_or(rest.len(), |i| i + close.len()),
            None => rest.find('>').map_or(rest.len(), |i| i + 1),
        };
        rest = &rest[end..];
        out.push(' ');
        if out.len() > limit {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    decode_entities(&out)
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_english_with_content() {
        let lang = detect(
            "Meta delays new business terms",
            "<p>Meta has postponed the rollout of its new terms after publishers \
             pushed back against <a href=\"https://x.com\">AI crawling</a> of their sites.</p>",
        );
        assert_eq!(lang.as_deref(), Some("en"));
    }

    #[test]
    fn detects_spanish_with_content() {
        let lang = detect(
            "El Gobierno aprueba el presupuesto",
            "<p>El Consejo de Ministros aprobó este martes el proyecto de presupuestos \
             para el próximo año, que ahora deberá pasar por el Congreso.</p>",
        );
        assert_eq!(lang.as_deref(), Some("es"));
    }

    #[test]
    fn detects_english_title_alone() {
        let lang = detect(
            "Switch 2 security takes a hit as hackers unlock homebrew access",
            "",
        );
        assert_eq!(lang.as_deref(), Some("en"));
    }

    #[test]
    fn short_text_is_unsure() {
        assert_eq!(detect("Hackers", ""), None);
        assert_eq!(detect("", ""), None);
        assert_eq!(detect("42", "<img src=\"a.png\">"), None);
    }

    #[test]
    fn strips_tags_scripts_and_entities() {
        let text = html_to_text(
            "<p>Tom &amp; Jerry</p><script>var x = 1;</script><STYLE>p{}</STYLE><b>ok</b>",
        );
        let words: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(words, ["Tom", "&", "Jerry", "ok"]);
    }
}
