use serde::{Deserialize, Serialize};
use std::cmp::{max, min};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io;

use crate::osmpbf;
use crate::osmxml;

/// Node
#[derive(Debug, Default, PartialEq)]
pub struct Node {
    /// Node id
    pub id: u64,
    /// Latitude in decimicro degrees (10⁻⁷ degrees).
    pub decimicro_lat: i32,
    /// Longitude in decimicro degrees (10⁻⁷ degrees).
    pub decimicro_lon: i32,
    /// Tags
    pub tags: Option<HashMap<String, String>>,
    /// Bounding-box
    pub bbox: Option<BoundingBox>,
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
#[derive(Debug, Default, PartialEq)]
pub struct Way {
    /// Way id
    pub id: u64,
    /// List of ordered nodes
    pub nodes: Vec<u64>,
    /// Tags
    pub tags: Option<HashMap<String, String>>,
    /// Bounding-box
    pub bbox: Option<BoundingBox>,
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
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Relation {
    /// Relation id
    pub id: u64,
    /// List of ordered members
    #[serde(rename = "member")]
    pub members: Vec<Member>,
    /// Tags
    #[serde(rename = "tag")]
    pub tags: Option<HashMap<String, String>>,
    /// Bounding-box
    pub bbox: Option<BoundingBox>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoundingBox {
    pub decimicro_minlat: i32,
    pub decimicro_maxlat: i32,
    pub decimicro_minlon: i32,
    pub decimicro_maxlon: i32,
}
impl BoundingBox {
    pub fn expand_bbox(&mut self, bbox2: &BoundingBox) {
        self.decimicro_minlat = min(self.decimicro_minlat, bbox2.decimicro_minlat);
        self.decimicro_maxlat = max(self.decimicro_maxlat, bbox2.decimicro_maxlat);
        self.decimicro_minlon = min(self.decimicro_minlon, bbox2.decimicro_minlon);
        self.decimicro_maxlon = max(self.decimicro_maxlon, bbox2.decimicro_maxlon);
        assert!(self.decimicro_minlat <= self.decimicro_maxlat);
        assert!(self.decimicro_minlon <= self.decimicro_maxlon);
    }
    pub fn expand_node(&mut self, node: &Node) {
        self.decimicro_minlat = min(self.decimicro_minlat, node.decimicro_lat);
        self.decimicro_maxlat = max(self.decimicro_maxlat, node.decimicro_lat);
        self.decimicro_minlon = min(self.decimicro_minlon, node.decimicro_lon);
        self.decimicro_maxlon = max(self.decimicro_maxlon, node.decimicro_lon);
        assert!(self.decimicro_minlat <= self.decimicro_maxlat);
        assert!(self.decimicro_minlon <= self.decimicro_maxlon);
    }

    pub fn minlat(&self) -> f64 {
        self.decimicro_minlat as f64 * 1e-7
    }
    pub fn maxlat(&self) -> f64 {
        self.decimicro_maxlat as f64 * 1e-7
    }
    pub fn minlon(&self) -> f64 {
        self.decimicro_minlon as f64 * 1e-7
    }
    pub fn maxlon(&self) -> f64 {
        self.decimicro_maxlon as f64 * 1e-7
    }
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
    fn write_node(&mut self, node: &mut Node) -> Result<(), io::Error>;
    fn write_way(&mut self, way: &mut Way) -> Result<(), io::Error>;
    fn write_relation(&mut self, relation: &mut Relation) -> Result<(), io::Error>;

    fn write_start(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn write_end(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn import(&mut self, filename: &str) -> Result<(), Box<dyn Error>>
    where
        Self: Sized,
    {
        if filename.ends_with(".pbf") {
            let mut reader = osmpbf::OsmPbf::new(filename).unwrap();
            reader.copy_to(self)
        } else if filename.ends_with(".osm.gz") || filename.ends_with(".osm") {
            let mut reader = osmxml::OsmXml::new(filename).unwrap();
            reader.copy_to(self)
        } else {
            Err(NotSupportedFileType {
                filename: filename.to_string(),
            }
            .into())
        }
    }
}

pub trait OsmUpdate: OsmWriter {
    fn update_node(&mut self, node: &mut Node, action: &Action) -> Result<(), io::Error>;
    fn update_way(&mut self, way: &mut Way, action: &Action) -> Result<(), io::Error>;
    fn update_relation(
        &mut self,
        relation: &mut Relation,
        action: &Action,
    ) -> Result<(), io::Error>;

    fn update(&mut self, filename: &str) -> Result<(), Box<dyn Error>>
    where
        Self: Sized,
    {
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
            Err(NotSupportedFileType {
                filename: filename.to_string(),
            }
            .into())
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
