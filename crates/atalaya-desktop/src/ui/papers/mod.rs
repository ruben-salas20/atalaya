//! Pantalla 4 de las maquetas: buscar papers de arXiv (ADR 0005, P1–P6).
//!
//! - `search.rs`: columna 2, buscador, filtros, resultados y recientes.
//! - `reader.rs`: columna 3, el paper abierto y el botón de guardar.
//! - `recent.rs`: las búsquedas recientes, en un archivo de este PC.
//! - `format.rs`: autores, fechas y etiquetas, como en Android.

mod chunks;
mod format;
pub mod formulas;
pub mod full;
pub mod reader;
mod recent;
pub mod search;
