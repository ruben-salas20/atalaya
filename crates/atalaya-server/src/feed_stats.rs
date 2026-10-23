//! Idioma y artículos al día de un feed (F2, ADR 0007).
//!
//! Miniflux no da ninguno de los dos. Se calculan con los últimos artículos:
//! los del feed descargado, si aún no está suscrito, o los que guarda
//! Miniflux, si ya lo está. El idioma se detecta aquí, con `whatlang`, sin
//! llama-swap.

use chrono::{DateTime, TimeDelta, Utc};

/// Cuántos artículos recientes se usan para calcular.
pub const SAMPLE: usize = 50;

/// Texto de cada artículo que entra en la detección del idioma.
const CHARS_PER_ENTRY: usize = 300;
const CHARS_TOTAL: usize = 6000;

/// Lo que se usa de un artículo para calcular.
#[derive(Debug, Clone, Default)]
pub struct EntrySample {
    pub title: String,
    /// HTML o texto del artículo; puede estar vacío.
    pub content: String,
    pub published_at: Option<DateTime<Utc>>,
}

/// Idioma y artículos al día de un feed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FeedStats {
    pub lang: Option<String>,
    pub articles_per_day: Option<f64>,
}

impl FeedStats {
    /// `declared` es el idioma que dice el propio feed, si lo dice. Solo se
    /// usa cuando la detección no es fiable.
    ///
    /// Usa los [`SAMPLE`] artículos más nuevos: un feed no siempre los trae
    /// en orden.
    pub fn compute(entries: &[EntrySample], declared: Option<&str>, now: DateTime<Utc>) -> Self {
        let mut entries: Vec<&EntrySample> = entries.iter().collect();
        // Del más nuevo al más viejo; los que no tienen fecha, al final.
        entries.sort_by_key(|e| std::cmp::Reverse(e.published_at));
        entries.truncate(SAMPLE);
        let dates: Vec<_> = entries.iter().filter_map(|e| e.published_at).collect();
        Self {
            lang: detect_lang(entries.iter().copied())
                .or_else(|| declared.and_then(normalize_lang)),
            articles_per_day: articles_per_day(&dates, now),
        }
    }
}

/// Media de artículos al día, redondeada a una décima.
///
/// Cuenta los intervalos entre artículos (`n - 1`) y los reparte entre el más
/// viejo y `now`. Medir hasta `now`, y no hasta el más nuevo, hace que un
/// feed que dejó de publicar baje en vez de quedarse con su ritmo de antes.
///
/// Devuelve `None` si hay menos de dos fechas, si abarcan menos de una hora
/// (no da para estimar) o si caben todas en un minuto: es lo que pasa cuando
/// el feed no trae fechas y Miniflux les pone la hora a la que lo descargó.
pub fn articles_per_day(dates: &[DateTime<Utc>], now: DateTime<Utc>) -> Option<f64> {
    if dates.len() < 2 {
        return None;
    }
    // Una fecha en el futuro cuenta como ahora.
    let dates: Vec<_> = dates.iter().map(|d| (*d).min(now)).collect();
    let oldest = *dates.iter().min()?;
    let newest = *dates.iter().max()?;
    if newest - oldest < TimeDelta::minutes(1) {
        return None;
    }
    let span = now - oldest;
    if span < TimeDelta::hours(1) {
        return None;
    }
    let days = span.num_seconds() as f64 / 86_400.0;
    let rate = (dates.len() - 1) as f64 / days;
    Some((rate * 10.0).round() / 10.0)
}

/// Idioma de los artículos en ISO 639-1, si la detección es fiable.
pub fn detect_lang<'a>(entries: impl IntoIterator<Item = &'a EntrySample>) -> Option<String> {
    let mut text = String::new();
    for e in entries {
        if text.len() >= CHARS_TOTAL {
            break;
        }
        text.push_str(&e.title);
        text.push_str(". ");
        let body = strip_html(&e.content);
        text.extend(body.chars().take(CHARS_PER_ENTRY));
        text.push('\n');
    }
    let info = whatlang::detect(&text)?;
    if !info.is_reliable() {
        return None;
    }
    iso_639_1(info.lang())
}

/// Pasa el código de 3 letras de `whatlang` al de 2 letras de la API.
pub fn iso_639_1(lang: whatlang::Lang) -> Option<String> {
    // `whatlang` da la variante concreta; ISO 639-1 solo tiene la general.
    let code = match lang.code() {
        "cmn" => return Some("zh".into()),
        "pes" => return Some("fa".into()),
        other => other,
    };
    isolang::Language::from_639_3(code)
        .and_then(|l| l.to_639_1())
        .map(str::to_owned)
}

/// `"en-US"` → `"en"`. Lo que no parece un código de idioma, `None`.
pub fn normalize_lang(declared: &str) -> Option<String> {
    let primary = declared
        .trim()
        .split(['-', '_'])
        .next()?
        .to_ascii_lowercase();
    (primary.len() == 2 && primary.chars().all(|c| c.is_ascii_lowercase())).then_some(primary)
}

/// Quita las etiquetas HTML y deja el texto, con espacios donde estaban.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn now() -> DateTime<Utc> {
        at("2026-10-23T12:00:00Z")
    }

    #[test]
    fn one_article_per_day() {
        // Diez artículos, uno cada día, el último hoy.
        let dates: Vec<_> = (0..10).map(|d| now() - TimeDelta::days(d)).collect();
        assert_eq!(articles_per_day(&dates, now()), Some(1.0));
    }

    #[test]
    fn many_per_day() {
        // Uno cada hora durante un día: unos 24.
        let dates: Vec<_> = (1..=24).map(|h| now() - TimeDelta::hours(h)).collect();
        assert_eq!(articles_per_day(&dates, now()), Some(23.0));
    }

    #[test]
    fn stale_feed_drops() {
        // Publicaba uno al día, pero el último fue hace 10 días.
        let dates: Vec<_> = (10..20).map(|d| now() - TimeDelta::days(d)).collect();
        assert_eq!(articles_per_day(&dates, now()), Some(0.5));
    }

    #[test]
    fn not_enough_dates() {
        assert_eq!(articles_per_day(&[], now()), None);
        assert_eq!(articles_per_day(&[now()], now()), None);
        // Dos artículos de hace minutos: no da para estimar.
        let dates = [
            now() - TimeDelta::minutes(5),
            now() - TimeDelta::minutes(30),
        ];
        assert_eq!(articles_per_day(&dates, now()), None);
    }

    #[test]
    fn dates_filled_by_miniflux_are_ignored() {
        // Como el feed de Paul Graham: sin fechas, todas a la misma hora.
        let base = now() - TimeDelta::hours(3);
        let dates: Vec<_> = (0..200)
            .map(|i| base + TimeDelta::microseconds(i))
            .collect();
        assert_eq!(articles_per_day(&dates, now()), None);
    }

    #[test]
    fn future_dates_count_as_now() {
        let dates = [now() + TimeDelta::days(3), now() - TimeDelta::days(1)];
        assert_eq!(articles_per_day(&dates, now()), Some(1.0));
    }

    fn sample(title: &str, content: &str) -> EntrySample {
        EntrySample {
            title: title.into(),
            content: content.into(),
            published_at: None,
        }
    }

    #[test]
    fn detects_english_and_spanish() {
        let en = [
            sample(
                "Apple announces new MacBook Pro",
                "<p>The company said on Tuesday that the new laptops will ship next week with faster chips.</p>",
            ),
            sample(
                "Why the internet is getting worse",
                "<p>Search results are full of spam and nobody seems to know how to fix it.</p>",
            ),
        ];
        assert_eq!(detect_lang(&en).as_deref(), Some("en"));

        let es = [sample(
            "El Gobierno aprueba la nueva ley de vivienda",
            "<p>La norma limita el precio del alquiler en las zonas tensionadas y obliga a los grandes propietarios a declarar sus pisos vacíos.</p>",
        )];
        assert_eq!(detect_lang(&es).as_deref(), Some("es"));
    }

    #[test]
    fn uses_the_newest_articles_whatever_the_order() {
        // 60 artículos, uno por hora, desordenados: solo cuentan los 50 últimos.
        let mut entries: Vec<_> = (0..60)
            .map(|h| EntrySample {
                published_at: Some(now() - TimeDelta::hours(h)),
                ..Default::default()
            })
            .collect();
        entries.reverse();
        let stats = FeedStats::compute(&entries, None, now());
        // 49 intervalos en 49 horas: uno por hora.
        assert_eq!(stats.articles_per_day, Some(24.0));
    }

    #[test]
    fn falls_back_to_declared_language() {
        let stats = FeedStats::compute(&[], Some("en-US"), now());
        assert_eq!(stats.lang.as_deref(), Some("en"));
        assert_eq!(FeedStats::compute(&[], None, now()).lang, None);
    }

    #[test]
    fn normalizes_declared_language() {
        assert_eq!(normalize_lang("en-us").as_deref(), Some("en"));
        assert_eq!(normalize_lang("ES_es").as_deref(), Some("es"));
        assert_eq!(normalize_lang(""), None);
        assert_eq!(normalize_lang("english"), None);
    }

    #[test]
    fn every_whatlang_language_has_a_code() {
        for lang in whatlang::Lang::all() {
            assert!(iso_639_1(*lang).is_some(), "{lang:?} sin código ISO 639-1");
        }
    }

    #[test]
    fn strips_tags() {
        assert_eq!(
            strip_html("<p>Hola <a href=\"x\">mundo</a></p><p>adiós</p>"),
            "Hola mundo adiós"
        );
    }
}
