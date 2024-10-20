use geo::{point, Intersects, MultiPolygon};
use std::collections::HashSet;
use std::error::Error;
use std::io;

use crate::osm::{Action, Node, Relation, Way};
use crate::osm::{OsmReader, OsmUpdate, OsmWriter};
use crate::osmbin;
use crate::osmgeom;
use crate::osmxml::OsmXml;

pub struct OsmXmlFilter {
    xmlwriter: OsmXml,
    reader: Box<dyn OsmReader>,
    poly: MultiPolygon<i64>,
    poly_buffered: MultiPolygon<i64>,
    nodes_seen_in_poly: HashSet<u64>,
    ways_seen_in_poly: HashSet<u64>,
    relations_seen_in_poly: HashSet<u64>,
}

impl OsmXmlFilter {
    pub fn new_osmbin(
        filename: &str,
        dir_osmbin: &str,
        poly_file: &str,
    ) -> Result<OsmXmlFilter, ()> {
        let poly: MultiPolygon<i64> = osmgeom::read_multipolygon_from_wkt(poly_file).unwrap().1;
        let poly_buffered = poly.clone(); // TODO
        Ok(OsmXmlFilter {
            xmlwriter: OsmXml::new(filename).unwrap(),
            reader: Box::new(osmbin::OsmBin::new(dir_osmbin).unwrap()),
            poly,
            poly_buffered,
            nodes_seen_in_poly: HashSet::new(),
            ways_seen_in_poly: HashSet::new(),
            relations_seen_in_poly: HashSet::new(),
        })
    }

    fn node_in_poly(&mut self, id: u64) -> bool {
        if self.nodes_seen_in_poly.contains(&id) {
            return true;
        }
        let node = self.reader.read_node(id);
        if let Some(node) = node {
            let point = point!(x: node.decimicro_lon as i64, y: node.decimicro_lat as i64);
            if point.intersects(&self.poly) {
                self.nodes_seen_in_poly.insert(id);
                return true;
            }
        }
        false
    }
    fn way_in_poly(&mut self, id: u64) -> bool {
        if self.ways_seen_in_poly.contains(&id) {
            return true;
        }
        let way = self.reader.read_way(id);
        if let Some(way) = way {
            for n in way.nodes {
                if self.node_in_poly(n) {
                    return true;
                }
            }
        }
        false
    }
    fn relation_in_poly(&mut self, id: u64) -> bool {
        if self.relations_seen_in_poly.contains(&id) {
            return true;
        }
        let relation = self.reader.read_relation(id);
        if let Some(relation) = relation {
            for m in &relation.members {
                let is_inside = match m.type_.as_str() {
                    "node" => self.node_in_poly(m.ref_),
                    "way" => self.way_in_poly(m.ref_),
                    "relation" => self.relation_in_poly(m.ref_),
                    _ => panic!("Unsupported relation member: {:?}", m),
                };
                if is_inside {
                    return true;
                }
            }
        }
        false
    }
}

impl OsmWriter for OsmXmlFilter {
    fn write_node(&mut self, node: &mut Node) -> Result<(), io::Error> {
        self.xmlwriter.write_node(node)
    }
    fn write_way(&mut self, way: &mut Way) -> Result<(), io::Error> {
        self.xmlwriter.write_way(way)
    }
    fn write_relation(&mut self, relation: &mut Relation) -> Result<(), io::Error> {
        self.xmlwriter.write_relation(relation)
    }
    fn write_start(&mut self) -> Result<(), Box<dyn Error>> {
        self.xmlwriter.write_start()
    }
    fn write_end(&mut self) -> Result<(), Box<dyn Error>> {
        self.xmlwriter.write_end()
    }
}
impl OsmUpdate for OsmXmlFilter {
    fn update_node(&mut self, node: &mut Node, action: &Action) -> Result<(), io::Error> {
        let bbox = osmgeom::bounding_box_to_polygon(
            &node
                .bbox
                .expect("Input OSC XML file must contain bbox tags"),
        );
        if bbox.intersects(&self.poly_buffered) {
            let point = point!(x: node.decimicro_lon as i64, y: node.decimicro_lat as i64);
            if point.intersects(&self.poly) {
                self.nodes_seen_in_poly.insert(node.id);
                self.xmlwriter.write_action_start(action);
            } else {
                self.xmlwriter.write_action_start(&Action::Delete());
            }
            self.write_node(node)?;
        }
        Ok(())
    }
    fn update_way(&mut self, way: &mut Way, action: &Action) -> Result<(), io::Error> {
        let bbox = osmgeom::bounding_box_to_polygon(
            &way.bbox.expect("Input OSC XML file must contain bbox tags"),
        );
        if bbox.intersects(&self.poly_buffered) {
            let mut is_inside = false;
            for nd in &way.nodes {
                if self.node_in_poly(*nd) {
                    is_inside = true;
                    break;
                }
            }
            if is_inside {
                self.ways_seen_in_poly.insert(way.id);
                self.xmlwriter.write_action_start(action);
            } else {
                self.xmlwriter.write_action_start(&Action::Delete());
            }
            self.write_way(way)?;
        }
        Ok(())
    }
    fn update_relation(
        &mut self,
        relation: &mut Relation,
        action: &Action,
    ) -> Result<(), io::Error> {
        let mut inside_bbox;

        if let Some(bbox) = &relation.bbox {
            inside_bbox = false;
            let bbox = osmgeom::bounding_box_to_polygon(bbox);
            if bbox.intersects(&self.poly_buffered) {
                inside_bbox = true;
            }
        } else {
            inside_bbox = true;
        }
        if inside_bbox {
            let mut is_inside = false;
            for m in &relation.members {
                is_inside = match m.type_.as_str() {
                    "node" => self.node_in_poly(m.ref_),
                    "way" => self.way_in_poly(m.ref_),
                    "relation" => self.relation_in_poly(m.ref_),
                    _ => panic!("Unsupported relation member: {:?}", m),
                };
                if is_inside {
                    break;
                }
            }
            if is_inside {
                self.relations_seen_in_poly.insert(relation.id);
                self.xmlwriter.write_action_start(action);
            } else {
                self.xmlwriter.write_action_start(&Action::Delete());
            }
            self.write_relation(relation)?;
        }
        Ok(())
    }
}
