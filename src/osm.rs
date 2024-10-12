use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io;

use crate::osmpbf;
use crate::osmxml;

/// Node
#[derive(Debug, PartialEq)]
pub struct Node {
    /// Node id
    pub id: u64,
    /// Latitude in decimicro degrees (10⁻⁷ degrees).
    pub decimicro_lat: i32,
    /// Longitude in decimicro degrees (10⁻⁷ degrees).
    pub decimicro_lon: i32,
    /// Tags
    pub tags: Option<HashMap<String, String>>,
}
impl Node {
    /// Returns the latitude of the node in degrees.
    pub fn lat(&self) -> f64 {
        self.decimicro_lat as f64 * 1e-7
    }
    /// Returns the longitude of the node in degrees.
    pub fn lon(&self) -> f64 {
        self.decimicro_lon as f64 * 1e-7
    }
    pub fn coord_to_decimicro(coord: f64) -> i32 {
        (coord * 1e7).round() as i32
    }
}

/// Way
#[derive(Debug, PartialEq)]
pub struct Way {
    /// Way id
    pub id: u64,
    /// List of ordered nodes
    pub nodes: Vec<u64>,
    /// Tags
    pub tags: Option<HashMap<String, String>>,
}

/// Relation member
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct Member {
    /// node/way/relation id
    #[serde(rename = "ref")]
    pub ref_: u64,
    /// Role in relation
    pub role: String,
    /// Type: node/way/relation
    #[serde(rename = "type")]
    pub type_: String,
}

/// Relation
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct Relation {
    /// Relation id
    pub id: u64,
    /// List of ordered members
    #[serde(rename = "member")]
    pub members: Vec<Member>,
    /// Tags
    #[serde(rename = "tag")]
    pub tags: Option<HashMap<String, String>>,
}

#[derive(Clone, PartialEq)]
pub enum Action {
    Create(),
    Modify(),
    Delete(),
    None,
}

pub trait OsmReader {
    fn read_node(&mut self, id: u64) -> Option<Node>;
    fn read_way(&mut self, id: u64) -> Option<Way>;
    fn read_relation(&mut self, id: u64) -> Option<Relation>;
}

pub trait OsmWriter {
    fn write_node(&mut self, node: &Node) -> Result<(), io::Error>;
    fn write_way(&mut self, way: &Way) -> Result<(), io::Error>;
    fn write_relation(&mut self, relation: &Relation) -> Result<(), io::Error>;

    fn write_start(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn write_end(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn import(&mut self, filename: &str) -> Result<(), Box<dyn Error>> where Self: Sized {
        if filename.ends_with(".pbf") {
            let mut reader = osmpbf::OsmPbf::new(filename).unwrap();
            reader.copy_to(self)
        } else if filename.ends_with(".osm.gz") || filename.ends_with(".osm") {
            let mut reader = osmxml::OsmXml::new(filename).unwrap();
            reader.copy_to(self)
        } else {
            Err(NotSupportedFileType{filename: filename.to_string()}.into())
        }
    }
}

pub trait OsmUpdate: OsmWriter {
    fn update_node(&mut self, node: &Node, action: &Action) -> Result<(), io::Error>;
    fn update_way(&mut self, way: &Way, action: &Action) -> Result<(), io::Error>;
    fn update_relation(&mut self, relation: &Relation, action: &Action) -> Result<(), io::Error>;

    fn update(&mut self, filename: &str) -> Result<(), Box<dyn Error>> where Self: Sized {
        if filename.ends_with(".pbf") {
            let mut reader = osmpbf::OsmPbf::new(filename).unwrap();
            reader.copy_to(self)
        } else if filename.ends_with(".osm.gz") || filename.ends_with(".osm") {
            let mut reader = osmxml::OsmXml::new(filename).unwrap();
            reader.copy_to(self)
        } else if filename.ends_with(".osc.gz") || filename.ends_with(".osc") {
            let mut reader = osmxml::OsmXml::new(filename).unwrap();
            reader.update_to(self)
        } else {
            Err(NotSupportedFileType{filename: filename.to_string()}.into())
        }
    }
}

pub trait OsmCopyTo {
    fn copy_to(&mut self, target: &mut impl OsmWriter) -> Result<(), Box<dyn Error>>;
}
pub trait OsmUpdateTo {
    fn update_to(&mut self, target: &mut impl OsmUpdate) -> Result<(), Box<dyn Error>>;
}

#[derive(Debug)]
pub struct NotSupportedFileType {
    pub filename: String,
}
impl Error for NotSupportedFileType {}
impl fmt::Display for NotSupportedFileType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "File {} is not supported", self.filename)
    }
}
