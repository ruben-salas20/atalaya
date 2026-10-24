//! Textos derivados de los datos: cuánto hace y etiqueta de idioma.

use chrono::{DateTime, Local, Utc};

/// Cuánto hace, en corto, como en la lista de las maquetas: «2 h», «1 d».
pub fn ago(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - when).num_seconds().max(0);
    match secs {
        0..60 => "ahora".into(),
        60..3_600 => format!("{} min", secs / 60),
        3_600..86_400 => format!("{} h", secs / 3_600),
        86_400..604_800 => format!("{} d", secs / 86_400),
        _ => when.with_timezone(&Local).format("%d/%m/%Y").to_string(),
    }
}

/// Cuánto hace, para una frase: «hace 2 h», «ahora» o la fecha si es viejo.
pub fn ago_phrase(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    match (now - when).num_seconds() {
        60..604_800 => format!("hace {}", ago(when, now)),
        _ => ago(when, now),
    }
}

/// Marca como código el texto de un `<pre>` que no trae `<code>` dentro. El
/// visor HTML solo pone letra de ancho fijo a lo que va en `<code>`.
pub fn code_in_pre(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = find_tag(rest, "<pre") {
        let Some(open_end) = rest[start..].find('>').map(|i| start + i + 1) else {
            break;
        };
        let Some(close) = rest[open_end..].find("</pre>").map(|i| open_end + i) else {
            break;
        };
        out.push_str(&rest[..open_end]);
        let inner = &rest[open_end..close];
        if inner.trim_start().starts_with("<code") {
            out.push_str(inner);
        } else {
            out.push_str("<code>");
            out.push_str(inner);
            out.push_str("</code>");
        }
        out.push_str("</pre>");
        rest = &rest[close + "</pre>".len()..];
    }
    out.push_str(rest);
    out
}

/// Posición de la etiqueta `tag` (`<pre`), no de otra que empiece igual
/// (`<preview`).
fn find_tag(html: &str, tag: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = html[from..].find(tag).map(|i| from + i) {
        match html[i + tag.len()..].chars().next() {
            Some('>' | ' ' | '\t' | '\n' | '/') => return Some(i),
            _ => from = i + tag.len(),
        }
    }
    None
}

/// Etiqueta del idioma original (L2): `EN`. Nada si es español o no se sabe.
pub fn lang_badge(lang: Option<&str>) -> Option<String> {
    lang.filter(|l| !l.is_empty() && *l != "es")
        .map(str::to_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn short_ages() {
        let now = DateTime::UNIX_EPOCH + Duration::days(30);
        assert_eq!(ago(now - Duration::seconds(5), now), "ahora");
        assert_eq!(ago(now - Duration::minutes(12), now), "12 min");
        assert_eq!(ago(now - Duration::minutes(130), now), "2 h");
        assert_eq!(ago(now - Duration::hours(30), now), "1 d");
        assert_eq!(ago(now + Duration::minutes(3), now), "ahora");
    }

    #[test]
    fn phrase_without_hace_for_dates() {
        let now = DateTime::UNIX_EPOCH + Duration::days(30);
        assert_eq!(ago_phrase(now - Duration::hours(3), now), "hace 3 h");
        assert_eq!(ago_phrase(now - Duration::seconds(5), now), "ahora");
        let old = ago_phrase(now - Duration::days(20), now);
        assert!(!old.starts_with("hace"), "{old}");
    }

    #[test]
    fn pre_gets_code() {
        assert_eq!(
            code_in_pre("<p>a</p><pre>npx x</pre><p>b</p>"),
            "<p>a</p><pre><code>npx x</code></pre><p>b</p>"
        );
        let with_code = "<pre class=\"x\"><code>y</code></pre>";
        assert_eq!(code_in_pre(with_code), with_code);
        assert_eq!(code_in_pre("<preview>z</preview>"), "<preview>z</preview>");
        assert_eq!(code_in_pre("<pre>sin cierre"), "<pre>sin cierre");
    }

    #[test]
    fn badge_only_for_foreign() {
        assert_eq!(lang_badge(Some("en")).as_deref(), Some("EN"));
        assert_eq!(lang_badge(Some("es")), None);
        assert_eq!(lang_badge(None), None);
    }
}
