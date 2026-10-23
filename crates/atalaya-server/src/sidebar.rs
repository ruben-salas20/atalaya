//! Menú lateral: totales de las vistas fijas y carpetas con sus feeds (paso 2.2).

use std::collections::HashMap;

use atalaya_api::{Id, Sidebar, SidebarCategory, SidebarFeed, View};
use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;

use crate::{AppState, error::AppResult, miniflux::Miniflux};

pub fn routes() -> Router<AppState> {
    Router::new().route("/sidebar", get(sidebar))
}

async fn sidebar(State(state): State<AppState>) -> AppResult<Json<Sidebar>> {
    let mf = &state.miniflux;
    let (total, unread, starred, categories, feeds, counters) = tokio::try_join!(
        mf.count(View::All),
        mf.count(View::Unread),
        mf.count(View::Starred),
        mf.sidebar_categories(),
        mf.sidebar_feeds(),
        mf.feed_counters(),
    )?;
    Ok(Json(Sidebar {
        total,
        unread,
        starred,
        categories: build_categories(categories, feeds, &counters),
    }))
}

/// Carpetas en orden alfabético, con sus feeds también en orden. Incluye las
/// carpetas vacías, para que se puedan elegir al añadir un feed.
fn build_categories(
    categories: Vec<MinifluxCategory>,
    feeds: Vec<MinifluxFeed>,
    counters: &MinifluxCounters,
) -> Vec<SidebarCategory> {
    let mut by_category: HashMap<Id, Vec<SidebarFeed>> = HashMap::new();
    for feed in feeds {
        by_category
            .entry(feed.category.id)
            .or_default()
            .push(SidebarFeed {
                id: feed.id,
                title: feed.title,
                unread: counters.unreads.get(&feed.id).copied().unwrap_or(0),
            });
    }
    let mut out: Vec<SidebarCategory> = categories
        .into_iter()
        .map(|c| {
            let mut feeds = by_category.remove(&c.id).unwrap_or_default();
            feeds.sort_by_key(|f| f.title.to_lowercase());
            SidebarCategory {
                id: c.id,
                unread: feeds.iter().map(|f| f.unread).sum(),
                title: c.title,
                feeds,
            }
        })
        .collect();
    out.sort_by_key(|c| c.title.to_lowercase());
    out
}

#[derive(Debug, Deserialize)]
struct MinifluxCategory {
    id: Id,
    title: String,
}

#[derive(Debug, Deserialize)]
struct MinifluxFeed {
    id: Id,
    title: String,
    category: MinifluxCategoryRef,
}

#[derive(Debug, Deserialize)]
struct MinifluxCategoryRef {
    id: Id,
}

/// `GET /v1/feeds/counters`: artículos leídos y sin leer por feed.
#[derive(Debug, Default, Deserialize)]
struct MinifluxCounters {
    #[serde(default)]
    unreads: HashMap<Id, u64>,
}

impl Miniflux {
    async fn sidebar_categories(&self) -> AppResult<Vec<MinifluxCategory>> {
        self.get("/v1/categories").await
    }

    async fn sidebar_feeds(&self) -> AppResult<Vec<MinifluxFeed>> {
        self.get("/v1/feeds").await
    }

    async fn feed_counters(&self) -> AppResult<MinifluxCounters> {
        self.get("/v1/feeds/counters").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_sorts_and_sums() {
        let categories: Vec<MinifluxCategory> = serde_json::from_str(
            r#"[{"id":2,"title":"tecnología","user_id":1,"hide_globally":false},
                {"id":1,"title":"All","user_id":1,"hide_globally":false},
                {"id":3,"title":"Vacía","user_id":1,"hide_globally":false}]"#,
        )
        .unwrap();
        let feeds: Vec<MinifluxFeed> = serde_json::from_str(
            r#"[{"id":1,"title":"Paul Graham: Essays","feed_url":"x","category":{"id":1,"title":"All"}},
                {"id":5,"title":"The Verge","category":{"id":2,"title":"tecnología"}},
                {"id":4,"title":"ars technica","category":{"id":2,"title":"tecnología"}}]"#,
        )
        .unwrap();
        let counters: MinifluxCounters =
            serde_json::from_str(r#"{"reads":{"1":10},"unreads":{"1":209,"5":3,"4":2}}"#).unwrap();

        let out = build_categories(categories, feeds, &counters);

        let titles: Vec<&str> = out.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["All", "tecnología", "Vacía"]);
        assert_eq!(out[0].unread, 209);
        assert_eq!(out[1].unread, 5);
        let feeds: Vec<&str> = out[1].feeds.iter().map(|f| f.title.as_str()).collect();
        assert_eq!(feeds, ["ars technica", "The Verge"]);
        assert!(out[2].feeds.is_empty());
        assert_eq!(out[2].unread, 0);
    }

    #[test]
    fn feed_without_counter_has_zero_unread() {
        let out = build_categories(
            vec![MinifluxCategory {
                id: 1,
                title: "All".into(),
            }],
            vec![MinifluxFeed {
                id: 9,
                title: "Nuevo".into(),
                category: MinifluxCategoryRef { id: 1 },
            }],
            &MinifluxCounters::default(),
        );
        assert_eq!(out[0].feeds[0].unread, 0);
    }
}
