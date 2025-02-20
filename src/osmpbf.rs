//! Reader for OpenStreetMap pbf files

use chrono;
use osmpbfreader;
use std::error::Error;
use std::fs::File;
use std::path::Path;

use crate::osm::{Member, Node, Relation, Way};
use crate::osm::{OsmCopyTo, OsmWriter};

/// Reader for OpenStreetMap pbf files
///
/// Only a few fields are kept from pbf file, as we don’t need all fields for OsmBin database.
///   - nodes: only latitude and longitude
///   - ways: only list of nodes
///   - relations: all fields
pub struct OsmPbf {
    filename: String,
}

impl OsmPbf {
    /// Read a pbf file
    pub fn new(filename: &str) -> Result<OsmPbf, Box<dyn Error>> {
        Ok(OsmPbf {
            filename: filename.to_string(),
        })
    }
}

macro_rules! printlnt {
    ($($arg:tt)*) => {
        println!("{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), format_args!($($arg)*));
    };
}

impl<T> OsmCopyTo<T> for OsmPbf
where
    T: OsmWriter,
{
    #[allow(clippy::cast_sign_loss)]
    fn copy_to(&mut self, target: &mut T) -> Result<(), Box<dyn Error>> {
        let r = File::open(Path::new(&self.filename)).unwrap();
        let mut pbf = osmpbfreader::OsmPbfReader::new(r);

        target.write_start(false).unwrap();
        let mut start_way = false;
        let mut start_relation = false;

        printlnt!("Starting pbf read");

        for obj in pbf.par_iter() {
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
                    if !start_way {
                        printlnt!("Starting ways");
                        start_way = true;
                    }
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
                    if !start_relation {
                        printlnt!("Starting relations");
                        start_relation = true;
                    }
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
                        });
                    }
                    let mut tags: Vec<(String, String)> = Vec::new();
                    for (k, v) in relation.tags.into_inner() {
                        tags.push((k.to_string(), v.to_string()));
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
        printlnt!("Finished pbf read");

        target.write_end(false).unwrap();

        Ok(())
    }
}
