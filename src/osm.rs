use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;

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

pub trait OsmReader {
    fn read_node(&mut self, id: u64) -> Option<Node>;
    fn read_way(&mut self, id: u64) -> Option<Way>;
    fn read_relation(&mut self, id: u64) -> Option<Relation>;
}

pub trait OsmWriter {
    fn write_node(&mut self, node: &Node) -> Result<(), io::Error>;
    fn write_way(&mut self, way: &Way) -> Result<(), io::Error>;
    fn write_relation(&mut self, relation: &Relation) -> Result<(), io::Error>;
}

pub trait OsmUpdate: OsmWriter {
    fn delete_node(&mut self, node: &Node) -> Result<(), io::Error>;
    fn delete_way(&mut self, way: &Way) -> Result<(), io::Error>;
    fn delete_relation(&mut self, relation: &Relation) -> Result<(), io::Error>;
}
