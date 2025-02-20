//! Split OpenstreetMap diffs by polygons
//!
//! osm-replication-rust is a tool to download OpenStreetMap diffs from planet, and split them by
//! polygons. The generated diffs can then be used to update a smaller OpenStreetMap database.

mod bufreaderwriter;
pub mod diffs;
pub mod osm;
pub mod osmbin;
pub mod osmcache;
pub mod osmgeom;
pub mod osmpbf;
pub mod osmxml;
pub mod update;
