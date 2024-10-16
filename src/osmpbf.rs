use osmpbfreader;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::path::Path;

use crate::osm::{Member, Node, Relation, Way};
use crate::osm::{OsmCopyTo, OsmWriter};

pub struct OsmPbf {
    filename: String,
}

impl OsmPbf {
    pub fn new(filename: &str) -> Result<OsmPbf, ()> {
        Ok(OsmPbf {
            filename: filename.to_string(),
        })
    }
}

impl OsmCopyTo for OsmPbf {
    fn copy_to(&mut self, target: &mut impl OsmWriter) -> Result<(), Box<dyn Error>> {
        let r = File::open(&Path::new(&self.filename)).unwrap();
        let mut pbf = osmpbfreader::OsmPbfReader::new(r);

        target.write_start().unwrap();

        for obj in pbf.iter() {
            let obj = obj?;
            match obj {
                osmpbfreader::OsmObj::Node(node) => {
                    target
                        .write_node(&mut Node {
                            id: node.id.0 as u64,
                            decimicro_lat: node.decimicro_lat,
                            decimicro_lon: node.decimicro_lon,
                            tags: None,
                            ..Default::default()
                        })
                        .unwrap();
                }
                osmpbfreader::OsmObj::Way(way) => {
                    let nodes: Vec<u64> = way.nodes.iter().map(|x| x.0 as u64).collect();
                    target
                        .write_way(&mut Way {
                            id: way.id.0 as u64,
                            nodes,
                            tags: None,
                            ..Default::default()
                        })
                        .unwrap();
                }
                osmpbfreader::OsmObj::Relation(relation) => {
                    let mut members: Vec<Member> = Vec::new();
                    for r in relation.refs {
                        let ref_: u64;
                        let type_: String;
                        match r.member {
                            osmpbfreader::objects::OsmId::Node(id) => {
                                ref_ = id.0 as u64;
                                type_ = String::from("node");
                            }
                            osmpbfreader::objects::OsmId::Way(id) => {
                                ref_ = id.0 as u64;
                                type_ = String::from("way");
                            }
                            osmpbfreader::objects::OsmId::Relation(id) => {
                                ref_ = id.0 as u64;
                                type_ = String::from("relation");
                            }
                        }
                        members.push(Member {
                            ref_,
                            type_,
                            role: r.role.to_string(),
                        })
                    }
                    let mut tags: HashMap<String, String> = HashMap::new();
                    for (k, v) in relation.tags.into_inner() {
                        tags.insert(k.to_string(), v.to_string());
                    }
                    target
                        .write_relation(&mut Relation {
                            id: relation.id.0 as u64,
                            members,
                            tags: Some(tags),
                            ..Default::default()
                        })
                        .unwrap();
                }
            }
        }

        target.write_end().unwrap();

        Ok(())
    }
}
