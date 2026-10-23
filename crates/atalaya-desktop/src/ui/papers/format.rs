//! Textos de la búsqueda de papers, iguales que en Android
//! (`ui/papers/PaperFormat.kt`).

use atalaya_api::{PaperField, PaperSort};
use chrono::{DateTime, Datelike, Utc};

use super::recent::RecentSearch;

/// Autores de una fila (P3): «Chen», «Nakamura y Raman», «Chen y otros 4».
/// Va el apellido, que se toma como la última palabra del nombre.
pub fn short_authors(authors: &[String]) -> String {
    let last: Vec<&str> = authors
        .iter()
        .filter_map(|a| a.split_whitespace().last())
        .collect();
    match last.as_slice() {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [a, b] => format!("{a} y {b}"),
        [first, rest @ ..] => format!("{first} y otros {}", rest.len()),
    }
}

const MONTHS: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic",
];

/// «21 oct 2026». arXiv fecha en UTC.
pub fn paper_date(at: DateTime<Utc>) -> String {
    format!(
        "{} {} {}",
        at.day(),
        MONTHS[at.month0() as usize],
        at.year()
    )
}

/// Número con punto de miles, como en español: «12.644». Con cuatro cifras
/// va sin punto: «1234».
fn spanish_number(n: u64) -> String {
    let digits = n.to_string();
    if digits.len() < 5 {
        return digits;
    }
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// «38 resultados · buscado como «small language models»».
pub fn results_header(total: u64, english_query: &str) -> String {
    let word = if total == 1 {
        "resultado"
    } else {
        "resultados"
    };
    format!(
        "{} {word} · buscado como «{english_query}»",
        spanish_number(total)
    )
}

pub fn field_label(field: PaperField) -> &'static str {
    match field {
        PaperField::All => "Todo",
        PaperField::Title => "Título",
        PaperField::Author => "Autor",
        PaperField::Abstract => "Resumen",
    }
}

pub fn sort_label(sort: PaperSort) -> &'static str {
    match sort {
        PaperSort::Recent => "Más recientes",
        PaperSort::Relevance => "Relevancia",
    }
}

/// Cómo se ve en la lista de recientes: «destilación», «autor: Ortega».
pub fn recent_label(recent: &RecentSearch) -> String {
    match recent.field {
        PaperField::All => recent.q.clone(),
        field => format!("{}: {}", field_label(field).to_lowercase(), recent.q),
    }
}

/// Los párrafos de un resumen: van separados por una línea en blanco. Los
/// saltos de línea sueltos dentro de un párrafo se vuelven espacios.
pub fn abstract_paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join(" "));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }
    paragraphs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn authors_by_surname() {
        assert_eq!(short_authors(&[]), "");
        assert_eq!(short_authors(&names(&["Wei Chen"])), "Chen");
        assert_eq!(
            short_authors(&names(&["Kenji Nakamura", "Priya Raman"])),
            "Nakamura y Raman"
        );
        assert_eq!(
            short_authors(&names(&["Wei Chen", "A B", "C D", "E F", "G H"])),
            "Chen y otros 4"
        );
    }

    #[test]
    fn dates_in_spanish() {
        let at = DateTime::parse_from_rfc3339("2026-10-21T23:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(paper_date(at), "21 oct 2026");
    }

    #[test]
    fn header_counts() {
        assert_eq!(
            results_header(38, "small language models"),
            "38 resultados · buscado como «small language models»"
        );
        assert_eq!(results_header(1, "x"), "1 resultado · buscado como «x»");
        assert_eq!(
            results_header(12644, "x"),
            "12.644 resultados · buscado como «x»"
        );
        assert_eq!(
            results_header(1234567, "x"),
            "1.234.567 resultados · buscado como «x»"
        );
        assert_eq!(
            results_header(1234, "x"),
            "1234 resultados · buscado como «x»"
        );
    }

    #[test]
    fn recent_labels() {
        let all = RecentSearch {
            q: "destilación".into(),
            field: PaperField::All,
        };
        let author = RecentSearch {
            q: "Ortega".into(),
            field: PaperField::Author,
        };
        assert_eq!(recent_label(&all), "destilación");
        assert_eq!(recent_label(&author), "autor: Ortega");
    }

    #[test]
    fn paragraphs_split_on_blank_lines() {
        let text = "Uno\ndos.\n\n  Tres.  \n\n\n";
        assert_eq!(abstract_paragraphs(text), vec!["Uno dos.", "Tres."]);
    }
}
