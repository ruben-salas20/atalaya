//! Caché de las estadísticas de cada feed (idioma y artículos por día).
//!
//! Calcularlas lee los artículos más nuevos de cada feed en Miniflux. Con
//! cientos de feeds, cada carga de la lista tardaría mucho. Por eso se guardan
//! unos minutos. Al crear, editar o borrar un feed se vacía todo, para que
//! el cambio se vea enseguida.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use atalaya_api::Id;

use crate::feed_stats::FeedStats;

/// Cuánto vive una estadística antes de volver a calcularse.
const TTL: Duration = Duration::from_secs(5 * 60);

/// Caché compartida: clonarla no copia los datos, solo la referencia.
#[derive(Clone)]
pub struct StatsCache {
    ttl: Duration,
    entries: Arc<Mutex<HashMap<Id, (Instant, FeedStats)>>>,
}

impl Default for StatsCache {
    fn default() -> Self {
        Self::with_ttl(TTL)
    }
}

impl StatsCache {
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Las estadísticas guardadas de un feed, si no han caducado.
    pub fn get(&self, feed_id: Id) -> Option<FeedStats> {
        let entries = self.entries.lock().unwrap();
        let (saved_at, stats) = entries.get(&feed_id)?;
        (saved_at.elapsed() < self.ttl).then(|| stats.clone())
    }

    pub fn put(&self, feed_id: Id, stats: FeedStats) {
        self.entries
            .lock()
            .unwrap()
            .insert(feed_id, (Instant::now(), stats));
    }

    /// Vacía la caché entera. Se llama tras crear, editar o borrar un feed.
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(lang: &str) -> FeedStats {
        FeedStats {
            lang: Some(lang.into()),
            articles_per_day: Some(1.0),
        }
    }

    #[test]
    fn returns_what_was_put() {
        let cache = StatsCache::default();
        assert_eq!(cache.get(1), None);
        cache.put(1, stats("es"));
        assert_eq!(cache.get(1), Some(stats("es")));
        assert_eq!(cache.get(2), None);
    }

    #[test]
    fn forgets_after_the_ttl() {
        let cache = StatsCache::with_ttl(Duration::ZERO);
        cache.put(1, stats("es"));
        assert_eq!(cache.get(1), None);
    }

    #[test]
    fn clear_empties_everything_and_clones_share_it() {
        let cache = StatsCache::default();
        let other = cache.clone();
        cache.put(1, stats("es"));
        other.put(2, stats("en"));
        other.clear();
        assert_eq!(cache.get(1), None);
        assert_eq!(cache.get(2), None);
    }
}
