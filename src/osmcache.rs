use std::collections::HashMap;
use std::sync::Arc;

use crate::osm::OsmReader;
use crate::osm::{Node, Relation, Way};

#[derive(Clone)]
pub struct OsmCache {
    nodes_cache: Arc<HashMap<u64, Option<(i32, i32)>>>,
    ways_cache: Arc<HashMap<u64, Option<Vec<u64>>>>,
    relations_cache: Arc<HashMap<u64, Option<Relation>>>,
}

impl OsmCache {
    pub fn new(
        nodes_cache: Arc<HashMap<u64, Option<(i32, i32)>>>,
        ways_cache: Arc<HashMap<u64, Option<Vec<u64>>>>,
        relations_cache: Arc<HashMap<u64, Option<Relation>>>,
    ) -> OsmCache {
        OsmCache {
            nodes_cache,
            ways_cache,
            relations_cache,
        }
    }
}

impl OsmReader for OsmCache {
    fn read_node(&mut self, id: u64) -> Option<Node> {
        if let Some(node) = self.nodes_cache.get(&id) {
            if let Some((decimicro_lat, decimicro_lon)) = node {
                return Some(Node {
                    id,
                    decimicro_lat: *decimicro_lat,
                    decimicro_lon: *decimicro_lon,
                    tags: None,
                    ..Default::default()
                });
            }
            return None;
        }
        panic!("Node {id} not found ");
    }
    fn read_way(&mut self, id: u64) -> Option<Way> {
        if let Some(nodes) = self.ways_cache.get(&id) {
            if let Some(nodes) = nodes {
                return Some(Way {
                    id,
                    nodes: nodes.clone(),
                    tags: None,
                    ..Default::default()
                });
            }
            return None;
        }
        panic!("Way {id} not found ");
    }
    fn read_relation(&mut self, id: u64) -> Option<Relation> {
        if let Some(relation) = self.relations_cache.get(&id) {
            return relation.clone();
        }
        panic!("Relation {id} not found ");
    }
}
