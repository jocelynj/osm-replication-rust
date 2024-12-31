use std::collections::HashMap;
use std::sync::Arc;

use crate::osm::OsmReader;
use crate::osm::{Node, Relation, Way};

#[derive(Clone, Default)]
pub struct OsmCache {
    pub(crate) nodes: HashMap<u64, Option<(i32, i32)>>,
    pub(crate) ways: HashMap<u64, Option<Vec<u64>>>,
    pub(crate) relations: HashMap<u64, Option<Relation>>,
}

impl OsmCache {
    pub fn new(
        nodes: HashMap<u64, Option<(i32, i32)>>,
        ways: HashMap<u64, Option<Vec<u64>>>,
        relations: HashMap<u64, Option<Relation>>,
    ) -> OsmCache {
        OsmCache {
            nodes,
            ways,
            relations,
        }
    }

    fn read_node(&self, id: u64) -> Option<Node> {
        if let Some(node) = self.nodes.get(&id) {
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
    fn read_way(&self, id: u64) -> Option<Way> {
        if let Some(nodes) = self.ways.get(&id) {
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
    fn read_relation(&self, id: u64) -> Option<Relation> {
        if let Some(relation) = self.relations.get(&id) {
            return relation.clone();
        }
        panic!("Relation {id} not found ");
    }
}

impl OsmReader for OsmCache {
    fn read_node(&mut self, id: u64) -> Option<Node> {
        OsmCache::read_node(self, id)
    }
    fn read_way(&mut self, id: u64) -> Option<Way> {
        OsmCache::read_way(self, id)
    }
    fn read_relation(&mut self, id: u64) -> Option<Relation> {
        OsmCache::read_relation(self, id)
    }
}

impl OsmReader for Arc<OsmCache> {
    fn read_node(&mut self, id: u64) -> Option<Node> {
        OsmCache::read_node(self.as_ref(), id)
    }
    fn read_way(&mut self, id: u64) -> Option<Way> {
        OsmCache::read_way(self.as_ref(), id)
    }
    fn read_relation(&mut self, id: u64) -> Option<Relation> {
        OsmCache::read_relation(self.as_ref(), id)
    }
}
