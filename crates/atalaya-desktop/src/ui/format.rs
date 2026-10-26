//! Textos derivados de los datos: cuánto hace y etiqueta de idioma.

use atalaya_api::Progress;
use chrono::{DateTime, Local, Utc};

/// La marca que pone el servicio alrededor de cada párrafo que aún no está
/// traducido (ADR 0015).
const PENDING_OPEN: &str = "<span class=\"atalaya-pendiente\">";
const PENDING_CLOSE: &str = "</span>";

/// Un trozo del HTML en español: o son párrafos traducidos, o son párrafos
/// que siguen en inglés mientras se traduce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub html: String,
    pub pending: bool,
}

/// Parte el HTML parcial en trozos, para pintar los pendientes en gris.
///
/// El visor HTML no lee clases ni estilos de un `span`, así que cada grupo
/// de párrafos pendientes sale aparte, sin la marca, y la app lo pinta en
/// gris. Dos pendientes seguidos (solo con etiquetas de bloque entre ellos)
/// forman un solo trozo.
pub fn split_pending(html: &str) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(PENDING_OPEN) {
        let (before, from_open) = rest.split_at(start);
        let body = &from_open[PENDING_OPEN.len()..];
        let (inner, after) = match body.find(PENDING_CLOSE) {
            Some(end) => (&body[..end], &body[end + PENDING_CLOSE.len()..]),
            // Una marca sin cerrar: lo que queda es pendiente.
            None => (body, ""),
        };
        match pieces.last_mut() {
            Some(last) if last.pending && !has_text(before) => {
                last.html.push_str(before);
                last.html.push_str(inner);
            }
            _ => {
                push_translated(&mut pieces, before);
                pieces.push(Piece {
                    html: inner.to_owned(),
                    pending: true,
                });
            }
        }
        rest = after;
    }
    push_translated(&mut pieces, rest);
    pieces
}

/// Añade un trozo traducido, salvo que no tenga nada que mostrar.
fn push_translated(pieces: &mut Vec<Piece>, html: &str) {
    if has_text(html) || html.contains("<img") {
        pieces.push(Piece {
            html: html.to_owned(),
            pending: false,
        });
    }
}

/// Si hay letras o cifras fuera de las etiquetas.
fn has_text(html: &str) -> bool {
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag && !c.is_whitespace() => return true,
            _ => {}
        }
    }
    false
}

/// El texto de la barra de avance: «Traduciendo… 12 de 52».
pub fn progress_text(progress: Progress) -> String {
    format!("Traduciendo… {} de {}", progress.done, progress.total)
}

/// Qué parte de la barra de avance va llena, de 0 a 1.
pub fn progress_fraction(progress: Progress) -> f32 {
    if progress.total == 0 {
        return 0.0;
    }
    (progress.done as f32 / progress.total as f32).clamp(0.0, 1.0)
}

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

    fn piece(html: &str, pending: bool) -> Piece {
        Piece {
            html: html.into(),
            pending,
        }
    }

    #[test]
    fn partial_without_marks_is_all_translated() {
        assert_eq!(
            split_pending("<p>Uno.</p><p>Dos.</p>"),
            [piece("<p>Uno.</p><p>Dos.</p>", false)]
        );
        assert!(split_pending("").is_empty());
    }

    #[test]
    fn partial_splits_the_pending_paragraphs_out() {
        let html = "<p>Uno.</p><p><span class=\"atalaya-pendiente\">Two.</span></p>\
                    <p><span class=\"atalaya-pendiente\">Three.</span></p>";
        // Dos pendientes seguidos son un solo trozo, sin la marca.
        assert_eq!(
            split_pending(html),
            [
                piece("<p>Uno.</p><p>", false),
                piece("Two.</p><p>Three.", true),
            ]
        );
    }

    #[test]
    fn partial_keeps_links_inside_a_pending_paragraph() {
        let html = "<p>Uno.</p><p><span class=\"atalaya-pendiente\">See <a href=\"x\">this</a>.</span></p>";
        let pieces = split_pending(html);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[1], piece("See <a href=\"x\">this</a>.", true));
        assert!(pieces.iter().all(|p| !p.html.contains("atalaya-pendiente")));
    }

    #[test]
    fn partial_with_pending_in_the_middle() {
        let html = "<p><span class=\"atalaya-pendiente\">A.</span></p><p>B.</p>\
                    <p><span class=\"atalaya-pendiente\">C.</span></p>";
        assert_eq!(
            split_pending(html),
            [
                piece("A.", true),
                piece("</p><p>B.</p><p>", false),
                piece("C.", true),
            ]
        );
    }

    #[test]
    fn partial_unclosed_mark_is_pending_to_the_end() {
        assert_eq!(
            split_pending("<p>Uno.</p><span class=\"atalaya-pendiente\">Dos."),
            [piece("<p>Uno.</p>", false), piece("Dos.", true)]
        );
    }

    #[test]
    fn progress_reads_as_done_of_total() {
        let progress = Progress {
            done: 12,
            total: 52,
        };
        assert_eq!(progress_text(progress), "Traduciendo… 12 de 52");
        assert!((progress_fraction(progress) - 12.0 / 52.0).abs() < 1e-6);
        assert_eq!(progress_fraction(Progress { done: 0, total: 0 }), 0.0);
        assert_eq!(progress_fraction(Progress { done: 9, total: 3 }), 1.0);
    }

    #[test]
    fn badge_only_for_foreign() {
        assert_eq!(lang_badge(Some("en")).as_deref(), Some("EN"));
        assert_eq!(lang_badge(Some("es")), None);
        assert_eq!(lang_badge(None), None);
    }
}
