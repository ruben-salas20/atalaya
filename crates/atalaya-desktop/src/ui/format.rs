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
    fn badge_only_for_foreign() {
        assert_eq!(lang_badge(Some("en")).as_deref(), Some("EN"));
        assert_eq!(lang_badge(Some("es")), None);
        assert_eq!(lang_badge(None), None);
    }
}
