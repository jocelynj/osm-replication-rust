//! Basic handling of OpenStreetMap data

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::cmp::{max, min};
use std::error::Error;
use std::fmt;
use std::io;
use std::num::NonZeroU64;

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
    pub tags: Option<Vec<(String, String)>>,
    /// Version
    pub version: Option<NonZeroU64>,
    /// Timestamp
    pub timestamp: Option<String>,
    /// User id
    pub uid: Option<NonZeroU64>,
    /// User name
    pub user: Option<String>,
    /// Changeset id
    pub changeset: Option<NonZeroU64>,
}
impl Node {
    /// Returns the latitude of the node in degrees.
    pub fn lat(&self) -> f64 {
        decimicro_to_coord(self.decimicro_lat)
    }
    /// Returns the longitude of the node in degrees.
    pub fn lon(&self) -> f64 {
        decimicro_to_coord(self.decimicro_lon)
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
    pub tags: Option<Vec<(String, String)>>,
    /// Version
    pub version: Option<NonZeroU64>,
    /// Timestamp
    pub timestamp: Option<String>,
    /// User id
    pub uid: Option<NonZeroU64>,
    /// User name
    pub user: Option<String>,
    /// Changeset id
    pub changeset: Option<NonZeroU64>,
    /// Bounding-box
    pub bbox: Option<BoundingBox>,
}

/// Relation member
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Relation {
    /// Relation id
    pub id: u64,
    /// List of ordered members
    #[serde(rename = "member")]
    pub members: Vec<Member>,
    /// Tags
    #[serde(rename = "tag")]
    #[serde_as(as = "Option<serde_with::Map<_, _>>")]
    pub tags: Option<Vec<(String, String)>>,
    /// Version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<NonZeroU64>,
    /// Timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// User id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<NonZeroU64>,
    /// User name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Changeset id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changeset: Option<NonZeroU64>,
    /// Bounding-box
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BoundingBox>,
}

/// Way, with its nodes
pub struct WayFull {
    pub way: Way,
    pub nodes: Vec<Option<Node>>,
}

/// Relation, with its members
pub struct RelationFull {
    pub relation: Relation,
    pub members: Vec<ElementFull>,
}

/// Full Element, used for members of RelationFull
pub enum ElementFull {
    Node(Option<Node>),
    Way(Option<WayFull>),
    Relation(Option<RelationFull>),
}

#[allow(clippy::cast_possible_truncation)]
/// Convert a floating-point latitude/longitude to the decimicro format
pub fn coord_to_decimicro(coord: f64) -> i32 {
    (coord * 1e7).round() as i32
}
/// Convert a decimicro latitude/longitude to floating-point
pub fn decimicro_to_coord(decimicro: i32) -> f64 {
    f64::from(decimicro) * 1e-7
}

/// Bounding-box of latitude/longitude
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoundingBox {
    pub decimicro_minlat: i32,
    pub decimicro_maxlat: i32,
    pub decimicro_minlon: i32,
    pub decimicro_maxlon: i32,
}
impl BoundingBox {
    /// Expand a bounding-box by another, to get a box containing both
    pub fn expand_bbox(&mut self, bbox2: &BoundingBox) {
        self.decimicro_minlat = min(self.decimicro_minlat, bbox2.decimicro_minlat);
        self.decimicro_maxlat = max(self.decimicro_maxlat, bbox2.decimicro_maxlat);
        self.decimicro_minlon = min(self.decimicro_minlon, bbox2.decimicro_minlon);
        self.decimicro_maxlon = max(self.decimicro_maxlon, bbox2.decimicro_maxlon);
        assert!(self.decimicro_minlat <= self.decimicro_maxlat);
        assert!(self.decimicro_minlon <= self.decimicro_maxlon);
    }
    /// Expand a bounding-box by a node, to get a box containing both
    pub fn expand_node(&mut self, node: &Node) {
        self.decimicro_minlat = min(self.decimicro_minlat, node.decimicro_lat);
        self.decimicro_maxlat = max(self.decimicro_maxlat, node.decimicro_lat);
        self.decimicro_minlon = min(self.decimicro_minlon, node.decimicro_lon);
        self.decimicro_maxlon = max(self.decimicro_maxlon, node.decimicro_lon);
        assert!(self.decimicro_minlat <= self.decimicro_maxlat);
        assert!(self.decimicro_minlon <= self.decimicro_maxlon);
    }

    pub fn minlat(&self) -> f64 {
        decimicro_to_coord(self.decimicro_minlat)
    }
    pub fn maxlat(&self) -> f64 {
        decimicro_to_coord(self.decimicro_maxlat)
    }
    pub fn minlon(&self) -> f64 {
        decimicro_to_coord(self.decimicro_minlon)
    }
    pub fn maxlon(&self) -> f64 {
        decimicro_to_coord(self.decimicro_maxlon)
    }
}

/// Action to apply to an Element
#[derive(Clone, PartialEq)]
pub enum Action {
    Create(),
    Modify(),
    Delete(),
    None,
}

/// Reader returning a node/way/relation from an osm id
pub trait OsmReader {
    fn read_node(&mut self, id: u64) -> Option<Node>;
    fn read_way(&mut self, id: u64) -> Option<Way>;
    fn read_relation(&mut self, id: u64) -> Option<Relation>;

    /// Get a way including all its nodes from an osm id
    fn read_way_full(&mut self, id: u64) -> Option<WayFull> {
        let way = self.read_way(id);
        if let Some(way) = way {
            let mut nodes: Vec<Option<Node>> = Vec::with_capacity(way.nodes.len());
            for n in &way.nodes {
                nodes.push(self.read_node(*n));
            }
            Some(WayFull { way, nodes })
        } else {
            None
        }
    }

    /// Get a relation including all its member from an osm id
    ///
    /// Note that a relation including itself is handled by dropping the inclusion.
    fn read_relation_full(&mut self, id: u64, prev_relations: &[u64]) -> Option<RelationFull> {
        if prev_relations.contains(&id) {
            println!("Detected relation recursion on id={id} - {prev_relations:?}",);
            return None;
        }
        let relation = self.read_relation(id);
        if let Some(relation) = relation {
            let mut members: Vec<ElementFull> = Vec::with_capacity(relation.members.len());

            for m in &relation.members {
                match m.type_.as_str() {
                    "node" => members.push(ElementFull::Node(self.read_node(m.ref_))),
                    "way" => members.push(ElementFull::Way(self.read_way_full(m.ref_))),
                    "relation" => {
                        let mut prev_relations = prev_relations.to_owned();
                        prev_relations.push(id);
                        members.push(ElementFull::Relation(
                            self.read_relation_full(m.ref_, &prev_relations),
                        ));
                    }
                    t => panic!("{t} not expected"),
                };
            }
            Some(RelationFull { relation, members })
        } else {
            None
        }
    }
}

/// Writer writing a new node/way/relation
pub trait OsmWriter {
    fn write_node(&mut self, node: &mut Node) -> Result<(), io::Error>;
    fn write_way(&mut self, way: &mut Way) -> Result<(), io::Error>;
    fn write_relation(&mut self, relation: &mut Relation) -> Result<(), io::Error>;

    fn write_start(&mut self, _change: bool) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn write_end(&mut self, _change: bool) -> Result<(), Box<dyn Error>> {
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

/// Trait to update or write a new node/way/relation
pub trait OsmUpdate: OsmWriter {
    fn update_node(&mut self, node: &mut Node, action: &Action) -> Result<(), io::Error>;
    fn update_way(&mut self, way: &mut Way, action: &Action) -> Result<(), io::Error>;
    fn update_relation(
        &mut self,
        relation: &mut Relation,
        action: &Action,
    ) -> Result<(), io::Error>;

    /// Write all nodes/ways/relations found in a given osm/osc[.gz] file
    fn update(&mut self, filename: &str) -> Result<(), Box<dyn Error>>
    where
        Self: Sized,
    {
        if filename.ends_with(".osm.gz")
            || filename.ends_with(".osm")
            || filename.ends_with(".osc.gz")
            || filename.ends_with(".osc")
        {
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

pub trait OsmCopyTo<T: OsmWriter> {
    fn copy_to(&mut self, target: &mut T) -> Result<(), Box<dyn Error>>;
}
pub trait OsmUpdateTo<T: OsmUpdate> {
    fn update_to(&mut self, target: &mut T) -> Result<(), Box<dyn Error>>;
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

impl ElementFull {
    fn fmt_inners(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        match self {
            ElementFull::Node(n) => {
                if let Some(n) = n {
                    writeln!(f, "{} node {:?}", " ".repeat(indent), &n.id)?;
                } else {
                    writeln!(f, "{} node missing", " ".repeat(indent))?;
                }
            }
            ElementFull::Way(w) => {
                if let Some(w) = w {
                    writeln!(f, "{} way  {:?}", " ".repeat(indent), &w.way.id)?;
                    for n in &w.nodes {
                        if let Some(n) = n {
                            writeln!(f, "{} node {:?}", " ".repeat(indent + 2), &n.id)?;
                        } else {
                            writeln!(f, "{} node missing", " ".repeat(indent + 2))?;
                        }
                    }
                } else {
                    writeln!(f, "{} way  missing", " ".repeat(indent))?;
                }
            }
            ElementFull::Relation(r) => {
                if let Some(r) = r {
                    r.fmt_inners(f, indent)?;
                } else {
                    writeln!(f, "{} relation missing", " ".repeat(indent))?;
                }
            }
        }
        Ok(())
    }
}

impl RelationFull {
    fn fmt_inners(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        writeln!(f, "{} relation {:?}", " ".repeat(indent), &self.relation.id)?;
        for i in &self.members {
            i.fmt_inners(f, indent + 2)?;
        }
        Ok(())
    }
}

impl fmt::Debug for RelationFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_inners(f, 0)?;
        Ok(())
    }
}
