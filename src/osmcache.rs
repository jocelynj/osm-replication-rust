//! Cache for nodes/ways/relations

use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::osm::OsmReader;
use crate::osm::{Node, Relation, Way};

type OsmCacheHashMap<K, V> = FxHashMap<K, V>;

/// Cache for nodes/ways/relations
///
/// This cache is filled when reading a diff file the first time by
/// [`OsmXmlBBox`](crate::osmxml::bbox::OsmXmlBBox) from an [`OsmBin`](crate::osmbin::OsmBin)
/// database, and reused when generating sub-diffs by
/// [`OsmXmlFilter`](crate::osmxml::filter::OsmXmlFilter). It only contains enough data to compute
/// latitude/longitude for nodes, ways, and relations.
#[derive(Clone, Default)]
pub struct OsmCache {
    pub(crate) nodes: OsmCacheHashMap<u64, Option<(i32, i32)>>,
    pub(crate) ways: OsmCacheHashMap<u64, Option<Vec<u64>>>,
    pub(crate) relations: OsmCacheHashMap<u64, Option<Relation>>,
}

impl OsmCache {
    pub fn new(
        nodes: OsmCacheHashMap<u64, Option<(i32, i32)>>,
        ways: OsmCacheHashMap<u64, Option<Vec<u64>>>,
        relations: OsmCacheHashMap<u64, Option<Relation>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rel_23() -> Relation {
        Relation {
            id: 23,
            tags: Some(vec![
                (String::from("a"), String::from("1")),
                (String::from("b"), String::from("2")),
            ]),
            ..Default::default()
        }
    }

    fn init_osmcache() -> OsmCache {
        let nodes = OsmCacheHashMap::from_iter([(1, None), (2, Some((4, 5))), (3, Some((-4, -5)))]);
        let ways = OsmCacheHashMap::from_iter([
            (11, None),
            (12, Some(vec![1, 2, 3])),
            (13, Some(vec![4, 5, 4])),
        ]);

        let relations = OsmCacheHashMap::from_iter([
            (21, None),
            (
                22,
                Some(Relation {
                    id: 22,
                    ..Default::default()
                }),
            ),
            (23, Some(rel_23())),
        ]);

        OsmCache::new(nodes, ways, relations)
    }

    #[test]
    fn read_node() {
        let osmcache = init_osmcache();

        let node = osmcache.read_node(1);
        assert_eq!(None, node);

        let node = osmcache.read_node(2);
        assert_eq!(
            Some(Node {
                id: 2,
                decimicro_lat: 4,
                decimicro_lon: 5,
                ..Default::default()
            }),
            node
        );

        let node = osmcache.read_node(3);
        assert_eq!(
            Some(Node {
                id: 3,
                decimicro_lat: -4,
                decimicro_lon: -5,
                ..Default::default()
            }),
            node
        );
    }

    #[test]
    #[should_panic]
    fn read_node_panic() {
        let osmcache = init_osmcache();
        osmcache.read_node(4);
    }

    #[test]
    fn read_way() {
        let osmcache = init_osmcache();

        let way = osmcache.read_way(11);
        assert_eq!(None, way);

        let way = osmcache.read_way(12);
        assert_eq!(
            Some(Way {
                id: 12,
                nodes: vec![1, 2, 3],
                ..Default::default()
            }),
            way
        );

        let way = osmcache.read_way(13);
        assert_eq!(
            Some(Way {
                id: 13,
                nodes: vec![4, 5, 4],
                ..Default::default()
            }),
            way
        );
    }

    #[test]
    #[should_panic]
    fn read_way_panic() {
        let osmcache = init_osmcache();
        osmcache.read_way(14);
    }

    #[test]
    fn read_relation() {
        let osmcache = init_osmcache();

        let relation = osmcache.read_relation(21);
        assert_eq!(None, relation);

        let relation = osmcache.read_relation(22);
        assert_eq!(
            Some(Relation {
                id: 22,
                ..Default::default()
            }),
            relation
        );

        let relation = osmcache.read_relation(23);
        assert_eq!(Some(rel_23()), relation);
        assert_eq!(23, relation.unwrap().id);
    }

    #[test]
    #[should_panic]
    fn read_relation_panic() {
        let osmcache = init_osmcache();
        osmcache.read_relation(24);
    }
}
