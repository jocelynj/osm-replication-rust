use std::collections::HashMap;
use std::error::Error;
use std::io;

use crate::osm::{Action, BoundingBox, Node, Relation, Way};
use crate::osm::{OsmReader, OsmUpdate, OsmWriter};
use crate::osmbin;
use crate::osmxml::OsmXml;

pub struct OsmXmlBBox<T>
where
    T: OsmReader,
{
    xmlwriter: OsmXml,
    reader: T,
    nodes_modified: HashMap<u64, BoundingBox>,
    ways_modified: HashMap<u64, BoundingBox>,
    relations_modified: HashMap<u64, BoundingBox>,
}
fn expand_bbox(bbox: &mut Option<BoundingBox>, bbox2: &BoundingBox) {
    if let Some(bb) = bbox.as_mut() {
        bb.expand_bbox(bbox2);
    } else {
        *bbox = Some(*bbox2);
    }
}

impl OsmXmlBBox<osmbin::OsmBin> {
    pub fn new_osmbin(
        filename: &str,
        dir_osmbin: &str,
    ) -> Result<OsmXmlBBox<osmbin::OsmBin>, Box<dyn Error>> {
        let reader = osmbin::OsmBin::new(dir_osmbin).unwrap();
        Ok(OsmXmlBBox {
            xmlwriter: OsmXml::new(filename).unwrap(),
            reader,
            nodes_modified: HashMap::new(),
            ways_modified: HashMap::new(),
            relations_modified: HashMap::new(),
        })
    }
}

impl<T> OsmXmlBBox<T>
where
    T: OsmReader,
{
    fn expand_bbox_node_only(&mut self, bbox: &mut Option<BoundingBox>, node: &Node) {
        if let Some(bb) = bbox.as_mut() {
            bb.expand_node(node);
        } else {
            *bbox = Some(BoundingBox {
                decimicro_minlat: node.decimicro_lat,
                decimicro_maxlat: node.decimicro_lat,
                decimicro_minlon: node.decimicro_lon,
                decimicro_maxlon: node.decimicro_lon,
            })
        }
    }
    fn expand_bbox_node_id(&mut self, bbox: &mut Option<BoundingBox>, id: u64) {
        if let Some(bb) = self.nodes_modified.get(&id) {
            expand_bbox(bbox, bb);
        }
        if let Some(node) = self.reader.read_node(id) {
            self.expand_bbox_node_only(bbox, &node);
        }
    }
    fn expand_bbox_node(&mut self, bbox: &mut Option<BoundingBox>, node: &Node) {
        self.expand_bbox_node_id(bbox, node.id);
        self.expand_bbox_node_only(bbox, node);
    }

    fn expand_bbox_way_only(&mut self, bbox: &mut Option<BoundingBox>, way: &Way) {
        for n in &way.nodes {
            self.expand_bbox_node_id(bbox, *n);
        }
    }
    fn expand_bbox_way_id(&mut self, bbox: &mut Option<BoundingBox>, id: u64) {
        if let Some(bb) = self.ways_modified.get(&id) {
            expand_bbox(bbox, bb);
        }
        if let Some(way) = self.reader.read_way(id) {
            self.expand_bbox_way_only(bbox, &way);
        }
    }
    fn expand_bbox_way(&mut self, bbox: &mut Option<BoundingBox>, way: &Way) {
        self.expand_bbox_way_id(bbox, way.id);
        self.expand_bbox_way_only(bbox, way);
    }

    fn expand_bbox_relation_only(
        &mut self,
        bbox: &mut Option<BoundingBox>,
        relation: &Relation,
        prev_relations: Vec<u64>,
    ) {
        for m in &relation.members {
            match m.type_.as_str() {
                "node" => self.expand_bbox_node_id(bbox, m.ref_),
                "way" => self.expand_bbox_way_id(bbox, m.ref_),
                "relation" => self.expand_bbox_relation_id(bbox, m.ref_, prev_relations.clone()),
                _ => panic!("Unsupported relation member: {:?}", m),
            }
        }
    }
    fn expand_bbox_relation_id(
        &mut self,
        bbox: &mut Option<BoundingBox>,
        id: u64,
        mut prev_relations: Vec<u64>,
    ) {
        if prev_relations.contains(&id) {
            println!(
                "Detected relation recursion on id={} - {:?}",
                id, prev_relations
            );
            return;
        }
        if let Some(bb) = self.relations_modified.get(&id) {
            expand_bbox(bbox, bb);
        }
        if let Some(relation) = self.reader.read_relation(id) {
            prev_relations.push(id);
            self.expand_bbox_relation_only(bbox, &relation, prev_relations);
        }
    }
    fn expand_bbox_relation(&mut self, bbox: &mut Option<BoundingBox>, relation: &Relation) {
        self.expand_bbox_relation_id(bbox, relation.id, vec![]);
        self.expand_bbox_relation_only(bbox, relation, vec![relation.id]);
    }
}

impl<T> OsmWriter for OsmXmlBBox<T>
where
    T: OsmReader,
{
    fn write_node(&mut self, node: &mut Node) -> Result<(), io::Error> {
        let mut bbox: Option<BoundingBox> = None;
        self.expand_bbox_node(&mut bbox, node);
        self.nodes_modified.insert(node.id, bbox.unwrap());

        self.xmlwriter.write_node(node)
    }
    fn write_way(&mut self, way: &mut Way) -> Result<(), io::Error> {
        let mut bbox: Option<BoundingBox> = None;
        self.expand_bbox_way(&mut bbox, way);
        way.bbox = bbox;
        if let Some(bb) = bbox {
            self.ways_modified.insert(way.id, bb);
        }

        self.xmlwriter.write_way(way)
    }
    fn write_relation(&mut self, relation: &mut Relation) -> Result<(), io::Error> {
        let mut bbox: Option<BoundingBox> = None;
        self.expand_bbox_relation(&mut bbox, relation);
        relation.bbox = bbox;
        if let Some(bb) = bbox {
            self.relations_modified.insert(relation.id, bb);
        }

        self.xmlwriter.write_relation(relation)
    }
    fn write_start(&mut self, change: bool) -> Result<(), Box<dyn Error>> {
        self.xmlwriter.write_start(change)
    }
    fn write_end(&mut self, change: bool) -> Result<(), Box<dyn Error>> {
        self.xmlwriter.write_end(change)
    }
}
impl<T> OsmUpdate for OsmXmlBBox<T>
where
    T: OsmReader,
{
    fn update_node(&mut self, node: &mut Node, action: &Action) -> Result<(), io::Error> {
        self.xmlwriter.write_action_start(action);
        self.write_node(node)?;
        Ok(())
    }
    fn update_way(&mut self, way: &mut Way, action: &Action) -> Result<(), io::Error> {
        self.xmlwriter.write_action_start(action);
        self.write_way(way)?;
        Ok(())
    }
    fn update_relation(
        &mut self,
        relation: &mut Relation,
        action: &Action,
    ) -> Result<(), io::Error> {
        self.xmlwriter.write_action_start(action);
        self.write_relation(relation)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    use crate::osm::Member;

    #[derive(Debug, Default)]
    struct MockReader {
        num_read_nodes: usize,
        num_read_ways: usize,
        num_read_relations: usize,
    }
    impl OsmReader for MockReader {
        fn read_node(&mut self, _id: u64) -> Option<Node> {
            self.num_read_nodes += 1;
            None
        }
        fn read_way(&mut self, _id: u64) -> Option<Way> {
            self.num_read_ways += 1;
            None
        }
        fn read_relation(&mut self, id: u64) -> Option<Relation> {
            self.num_read_relations += 1;

            // Recursion between relations 7801 and 7802
            if id == 7802 {
                Some(Relation {
                    id,
                    members: vec![Member {
                        ref_: 7801,
                        role: String::from("subarea"),
                        type_: String::from("relation"),
                    }],
                    ..Default::default()
                })
            } else if id == 7801 {
                Some(Relation {
                    id,
                    members: vec![Member {
                        ref_: 7802,
                        role: String::from("subarea"),
                        type_: String::from("relation"),
                    }],
                    ..Default::default()
                })
            } else {
                None
            }
        }
    }

    fn new_mockreader(filename: &str, reader: MockReader) -> OsmXmlBBox<MockReader> {
        OsmXmlBBox {
            xmlwriter: OsmXml::new(filename).unwrap(),
            reader: reader,
            nodes_modified: HashMap::new(),
            ways_modified: HashMap::new(),
            relations_modified: HashMap::new(),
        }
    }

    #[test]
    fn saint_barthelemy() {
        let reader = MockReader {
            ..Default::default()
        };
        let src = String::from("tests/resources/saint_barthelemy.osc.gz");
        let dest = tempfile::NamedTempFile::new().unwrap();
        let mut osmxmlbbox = new_mockreader(dest.path().to_str().unwrap(), reader);
        osmxmlbbox.update(&src).unwrap();

        assert_eq!(33, osmxmlbbox.reader.num_read_nodes);
        assert_eq!(7, osmxmlbbox.reader.num_read_ways);
        assert_eq!(9, osmxmlbbox.reader.num_read_relations);
    }
}
