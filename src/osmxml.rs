use flate2::Compression;
use flate2::bufread::GzDecoder;
use flate2::write::GzEncoder;
use quick_xml;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::num::NonZeroU64;
use std::str;

use crate::osm::{self, Action, BoundingBox, Member, Node, Relation, Way};
use crate::osm::{OsmCopyTo, OsmUpdate, OsmUpdateTo, OsmWriter};

pub mod bbox;
pub mod filter;

enum CurObj {
    Empty(),
    Node(Node),
    Way(Way),
    Relation(Relation),
}

pub struct OsmXml {
    filename: String,
    xmlwriter: Option<Writer<Box<dyn Write>>>,
    actionwriter: Action,
}

impl OsmXml {
    pub fn new(filename: &str) -> Result<OsmXml, Box<dyn Error>> {
        Ok(OsmXml {
            filename: filename.to_string(),
            xmlwriter: None,
            actionwriter: Action::None,
        })
    }

    fn xmlreader(&self, filename: &str) -> Result<Reader<Box<dyn BufRead>>, Box<dyn Error>> {
        let freader = Box::new(File::open(filename)?);
        let reader: Box<dyn BufRead> = if self.filename.ends_with(".gz") {
            let breader = BufReader::new(freader);
            let gzreader = GzDecoder::new(breader);
            Box::new(BufReader::new(gzreader))
        } else {
            Box::new(BufReader::new(freader))
        };
        Ok(Reader::from_reader(reader))
    }
    fn xmlwriter(&self, filename: &str) -> Result<Writer<Box<dyn Write>>, Box<dyn Error>> {
        let fwriter = Box::new(File::create(filename)?);
        let writer: Box<dyn Write> = if self.filename.ends_with(".gz") {
            let gzwriter = GzEncoder::new(fwriter, Compression::default());
            Box::new(BufWriter::new(gzwriter))
        } else {
            Box::new(BufWriter::new(fwriter))
        };
        Ok(Writer::new_with_indent(writer, b' ', 0))
    }
    fn write_action_start(&mut self, action: &Action) {
        if *action != Action::None && *action != self.actionwriter {
            if self.actionwriter != Action::None {
                let action_str = match self.actionwriter {
                    Action::Create() => "create",
                    Action::Modify() => "modify",
                    Action::Delete() => "delete",
                    Action::None => "",
                };
                self.xmlwriter
                    .as_mut()
                    .unwrap()
                    .write_event(Event::End(BytesEnd::new(action_str)))
                    .unwrap();
            }

            let action_str = match action {
                Action::Create() => "create",
                Action::Modify() => "modify",
                Action::Delete() => "delete",
                Action::None => "",
            };
            self.xmlwriter
                .as_mut()
                .unwrap()
                .write_event(Event::Start(BytesStart::new(action_str)))
                .unwrap();
            self.actionwriter = action.clone();
        }
    }
}

impl<T> OsmCopyTo<T> for OsmXml
where
    T: OsmWriter,
{
    #[allow(clippy::too_many_lines)]
    fn copy_to(&mut self, target: &mut T) -> Result<(), Box<dyn Error>> {
        let mut reader = self.xmlreader(&self.filename).unwrap();

        let mut buf = Vec::new();

        let mut tags: Vec<(String, String)> = Vec::new();
        let mut nodes: Vec<u64> = Vec::new();
        let mut members: Vec<Member> = Vec::new();

        let mut curobj = CurObj::Empty();

        loop {
            match reader.read_event_into(&mut buf) {
                Err(e) => panic!("Error at position {}: {:?}", reader.error_position(), e),
                Ok(Event::Eof) => break, // end of file

                Ok(Event::Start(e)) => match e.name().as_ref() {
                    b"osm" => target.write_start(false)?,
                    b"node" => {
                        let mut id: u64 = 0;
                        let mut decimicro_lat: i32 = 0;
                        let mut decimicro_lon: i32 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"lat" => {
                                    decimicro_lat =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"lon" => {
                                    decimicro_lon =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        tags = Vec::new();
                        curobj = CurObj::Node(Node {
                            id,
                            decimicro_lat,
                            decimicro_lon,
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                        });
                    }
                    b"way" => {
                        let mut id: u64 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        tags = Vec::new();
                        nodes = Vec::new();
                        curobj = CurObj::Way(Way {
                            id,
                            nodes: Vec::new(),
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                            ..Default::default()
                        });
                    }
                    b"relation" => {
                        let mut id: u64 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        tags = Vec::new();
                        members = Vec::new();
                        curobj = CurObj::Relation(Relation {
                            id,
                            members: Vec::new(),
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                            ..Default::default()
                        });
                    }
                    k => panic!("Unsupported start element: {}", str::from_utf8(k)?),
                },
                Ok(Event::End(e)) => match e.name().as_ref() {
                    b"osm" => target.write_end(false)?,
                    b"node" => {
                        if let CurObj::Node(ref mut node) = curobj {
                            node.tags = Some(tags);
                            tags = Vec::new();
                            target.write_node(node)?;
                        } else {
                            panic!("Expected an initialized node");
                        }
                    }
                    b"way" => {
                        if let CurObj::Way(ref mut way) = curobj {
                            way.nodes = nodes;
                            way.tags = Some(tags);
                            nodes = Vec::new();
                            tags = Vec::new();
                            target.write_way(way)?;
                        } else {
                            panic!("Expected an initialized way");
                        }
                    }
                    b"relation" => {
                        if let CurObj::Relation(ref mut relation) = curobj {
                            relation.members = members;
                            relation.tags = Some(tags);
                            members = Vec::new();
                            tags = Vec::new();
                            target.write_relation(relation)?;
                        } else {
                            panic!("Expected an initialized relation");
                        }
                    }
                    k => panic!("Unsupported end element: {}", str::from_utf8(k)?),
                },
                Ok(Event::Empty(e)) => match e.name().as_ref() {
                    b"bounds" => (),
                    b"node" => {
                        let mut id: u64 = 0;
                        let mut decimicro_lat: i32 = 0;
                        let mut decimicro_lon: i32 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"lat" => {
                                    decimicro_lat =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"lon" => {
                                    decimicro_lon =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        target.write_node(&mut Node {
                            id,
                            decimicro_lat,
                            decimicro_lon,
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                        })?;
                    }
                    b"nd" => {
                        let nd = e
                            .attributes()
                            .find(|x| x.as_ref().unwrap().key.as_ref() == b"ref")
                            .unwrap()
                            .unwrap();
                        let nd: u64 = str::from_utf8(&nd.value)?.parse()?;
                        nodes.push(nd);
                    }
                    b"member" => {
                        let mut ref_: u64 = 0;
                        let mut role: String = String::new();
                        let mut type_: String = String::new();
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"ref" => ref_ = v.parse().unwrap(),
                                b"type" => type_ = String::from(v),
                                b"role" => role = String::from(v),
                                _ => (),
                            }
                        }
                        members.push(Member { ref_, role, type_ });
                    }
                    b"tag" => {
                        let mut key: String = String::new();
                        let mut val: String = String::new();
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"k" => key = String::from(v),
                                b"v" => val = String::from(v),
                                _ => (),
                            }
                        }
                        tags.push((key, val));
                    }
                    k => panic!("Unsupported empty element: {}", str::from_utf8(k)?),
                },
                Ok(Event::Text(_)) => (),
                Ok(Event::Decl(_)) => (),
                e => panic!("Unsupported entry: {:?}", e?),
            }
        }

        Ok(())
    }
}

impl<T> OsmUpdateTo<T> for OsmXml
where
    T: OsmUpdate,
{
    #[allow(clippy::too_many_lines)]
    fn update_to(&mut self, target: &mut T) -> Result<(), Box<dyn Error>> {
        let mut reader = self.xmlreader(&self.filename).unwrap();

        let mut buf = Vec::new();

        let mut tags: Vec<(String, String)> = Vec::new();
        let mut nodes: Vec<u64> = Vec::new();
        let mut members: Vec<Member> = Vec::new();
        let mut bbox: Option<BoundingBox> = None;

        let mut curaction = Action::None;
        let mut curobj = CurObj::Empty();

        loop {
            match reader.read_event_into(&mut buf) {
                Err(e) => panic!("Error at position {}: {:?}", reader.error_position(), e),
                Ok(Event::Eof) => break, // end of file

                Ok(Event::Start(e)) => match e.name().as_ref() {
                    b"osm" => target.write_start(false)?,
                    b"osmChange" => target.write_start(true)?,
                    b"node" => {
                        let mut id: u64 = 0;
                        let mut decimicro_lat: i32 = 0;
                        let mut decimicro_lon: i32 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"lat" => {
                                    decimicro_lat =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"lon" => {
                                    decimicro_lon =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        tags = Vec::new();
                        curobj = CurObj::Node(Node {
                            id,
                            decimicro_lat,
                            decimicro_lon,
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                        });
                    }
                    b"way" => {
                        let mut id: u64 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        tags = Vec::new();
                        nodes = Vec::new();
                        curobj = CurObj::Way(Way {
                            id,
                            nodes: Vec::new(),
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                            ..Default::default()
                        });
                    }
                    b"relation" => {
                        let mut id: u64 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        tags = Vec::new();
                        members = Vec::new();
                        curobj = CurObj::Relation(Relation {
                            id,
                            members: Vec::new(),
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                            ..Default::default()
                        });
                    }
                    b"create" => curaction = Action::Create(),
                    b"modify" => curaction = Action::Modify(),
                    b"delete" => curaction = Action::Delete(),
                    k => panic!("Unsupported start element: {}", str::from_utf8(k)?),
                },
                Ok(Event::End(e)) => match e.name().as_ref() {
                    b"osm" => target.write_end(false)?,
                    b"osmChange" => target.write_end(true)?,
                    b"node" => {
                        if let CurObj::Node(ref mut node) = curobj {
                            node.tags = Some(tags);
                            tags = Vec::new();
                            bbox = None;
                            target.update_node(node, &curaction)?;
                        } else {
                            panic!("Expected an initialized node");
                        }
                    }
                    b"way" => {
                        if let CurObj::Way(ref mut way) = curobj {
                            way.nodes = nodes;
                            way.tags = Some(tags);
                            way.bbox = bbox;
                            nodes = Vec::new();
                            tags = Vec::new();
                            bbox = None;
                            target.update_way(way, &curaction)?;
                        } else {
                            panic!("Expected an initialized way");
                        }
                    }
                    b"relation" => {
                        if let CurObj::Relation(ref mut relation) = curobj {
                            relation.members = members;
                            relation.tags = Some(tags);
                            relation.bbox = bbox;
                            members = Vec::new();
                            tags = Vec::new();
                            bbox = None;
                            target.update_relation(relation, &curaction)?;
                        } else {
                            panic!("Expected an initialized relation");
                        }
                    }
                    b"create" => (),
                    b"modify" => (),
                    b"delete" => (),
                    k => panic!("Unsupported end element: {}", str::from_utf8(k)?),
                },
                Ok(Event::Empty(e)) => match e.name().as_ref() {
                    b"bounds" => (),
                    b"node" => {
                        let mut id: u64 = 0;
                        let mut decimicro_lat: i32 = 0;
                        let mut decimicro_lon: i32 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"lat" => {
                                    decimicro_lat =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"lon" => {
                                    decimicro_lon =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        let mut node = Node {
                            id,
                            decimicro_lat,
                            decimicro_lon,
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                        };
                        target.update_node(&mut node, &curaction)?;
                    }
                    b"way" => {
                        let mut id: u64 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        tags = Vec::new();
                        nodes = Vec::new();
                        let mut way = Way {
                            id,
                            nodes: Vec::new(),
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                            ..Default::default()
                        };
                        target.update_way(&mut way, &curaction)?;
                    }
                    b"relation" => {
                        let mut id: u64 = 0;
                        let mut version: Option<NonZeroU64> = None;
                        let mut timestamp: Option<String> = None;
                        let mut uid: Option<NonZeroU64> = None;
                        let mut user: Option<String> = None;
                        let mut changeset: Option<NonZeroU64> = None;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"version" => version = Some(v.parse().unwrap()),
                                b"timestamp" => timestamp = Some(v.parse().unwrap()),
                                b"uid" => uid = Some(v.parse().unwrap()),
                                b"user" => user = Some(v.parse().unwrap()),
                                b"changeset" => changeset = Some(v.parse().unwrap()),
                                _ => (),
                            }
                        }
                        tags = Vec::new();
                        members = Vec::new();
                        let mut relation = Relation {
                            id,
                            members: Vec::new(),
                            tags: None,
                            version,
                            timestamp,
                            uid,
                            user,
                            changeset,
                            ..Default::default()
                        };
                        target.update_relation(&mut relation, &curaction)?;
                    }
                    b"nd" => {
                        let nd = e
                            .attributes()
                            .find(|x| x.as_ref().unwrap().key.as_ref() == b"ref")
                            .unwrap()
                            .unwrap();
                        let nd: u64 = str::from_utf8(&nd.value)?.parse()?;
                        nodes.push(nd);
                    }
                    b"member" => {
                        let mut ref_: u64 = 0;
                        let mut role: String = String::new();
                        let mut type_: String = String::new();
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"ref" => ref_ = v.parse().unwrap(),
                                b"type" => type_ = String::from(v),
                                b"role" => role = String::from(v),
                                _ => (),
                            }
                        }
                        members.push(Member { ref_, role, type_ });
                    }
                    b"tag" => {
                        let mut key: String = String::new();
                        let mut val: String = String::new();
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"k" => key = String::from(v),
                                b"v" => val = String::from(v),
                                _ => (),
                            }
                        }
                        tags.push((key, val));
                    }
                    b"bbox" => {
                        let mut decimicro_minlat: i32 = 0;
                        let mut decimicro_maxlat: i32 = 0;
                        let mut decimicro_minlon: i32 = 0;
                        let mut decimicro_maxlon: i32 = 0;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"minlat" => {
                                    decimicro_minlat =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"maxlat" => {
                                    decimicro_maxlat =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"minlon" => {
                                    decimicro_minlon =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                b"maxlon" => {
                                    decimicro_maxlon =
                                        osm::coord_to_decimicro(v.parse::<f64>().unwrap());
                                }
                                _ => (),
                            }
                        }
                        bbox = Some(BoundingBox {
                            decimicro_minlat,
                            decimicro_maxlat,
                            decimicro_minlon,
                            decimicro_maxlon,
                        });
                    }
                    k => panic!("Unsupported empty element: {}", str::from_utf8(k)?),
                },
                Ok(Event::Text(_)) => (),
                Ok(Event::Decl(_)) => (),
                e => panic!("Unsupported entry: {:?}", e?),
            }
        }

        Ok(())
    }
}

impl OsmWriter for OsmXml {
    fn write_node(&mut self, node: &mut Node) -> Result<(), io::Error> {
        let mut elem = self
            .xmlwriter
            .as_mut()
            .unwrap()
            .create_element("node")
            .with_attribute(("id", node.id.to_string().as_str()));

        if let Some(version) = &node.version {
            elem = elem.with_attribute(("version", version.to_string().as_str()));
        }
        if let Some(timestamp) = &node.timestamp {
            elem = elem.with_attribute(("timestamp", timestamp.to_string().as_str()));
        }
        if let Some(uid) = &node.uid {
            elem = elem.with_attribute(("uid", uid.to_string().as_str()));
        }
        if let Some(user) = &node.user {
            elem = elem.with_attribute(("user".as_bytes(), user.to_string().as_bytes()));
        }
        if let Some(changeset) = &node.changeset {
            elem = elem.with_attribute(("changeset", changeset.to_string().as_str()));
        }
        elem = elem
            .with_attribute(("lat", node.lat().to_string().as_str()))
            .with_attribute(("lon", node.lon().to_string().as_str()));

        if node.tags.is_none() {
            elem.write_empty().unwrap();
        } else {
            elem.write_inner_content(|writer| {
                if node.tags.is_some() {
                    for (k, v) in node.tags.as_ref().unwrap() {
                        writer
                            .create_element("tag")
                            .with_attribute(("k".as_bytes(), k.as_bytes()))
                            .with_attribute(("v".as_bytes(), v.as_bytes()))
                            .write_empty()
                            .unwrap();
                    }
                }
                Ok(())
            })
            .unwrap();
        }

        Ok(())
    }
    fn write_way(&mut self, way: &mut Way) -> Result<(), io::Error> {
        let mut elem = self
            .xmlwriter
            .as_mut()
            .unwrap()
            .create_element("way")
            .with_attribute(("id", way.id.to_string().as_str()));

        if let Some(version) = &way.version {
            elem = elem.with_attribute(("version", version.to_string().as_str()));
        }
        if let Some(timestamp) = &way.timestamp {
            elem = elem.with_attribute(("timestamp", timestamp.to_string().as_str()));
        }
        if let Some(uid) = &way.uid {
            elem = elem.with_attribute(("uid", uid.to_string().as_str()));
        }
        if let Some(user) = &way.user {
            elem = elem.with_attribute(("user".as_bytes(), user.to_string().as_bytes()));
        }
        if let Some(changeset) = &way.changeset {
            elem = elem.with_attribute(("changeset", changeset.to_string().as_str()));
        }

        elem.write_inner_content(|writer| {
            for n in &way.nodes {
                let n: u64 = *n;
                writer
                    .create_element("nd")
                    .with_attribute(("ref", n.to_string().as_str()))
                    .write_empty()
                    .unwrap();
            }
            if way.tags.is_some() {
                for (k, v) in way.tags.as_ref().unwrap() {
                    writer
                        .create_element("tag")
                        .with_attribute(("k".as_bytes(), k.as_bytes()))
                        .with_attribute(("v".as_bytes(), v.as_bytes()))
                        .write_empty()
                        .unwrap();
                }
            }
            if let Some(bb) = &way.bbox {
                writer
                    .create_element("bbox")
                    .with_attribute(("minlat", bb.minlat().to_string().as_str()))
                    .with_attribute(("maxlat", bb.maxlat().to_string().as_str()))
                    .with_attribute(("minlon", bb.minlon().to_string().as_str()))
                    .with_attribute(("maxlon", bb.maxlon().to_string().as_str()))
                    .write_empty()
                    .unwrap();
            }
            Ok(())
        })
        .unwrap();

        Ok(())
    }
    fn write_relation(&mut self, relation: &mut Relation) -> Result<(), io::Error> {
        let mut elem = self
            .xmlwriter
            .as_mut()
            .unwrap()
            .create_element("relation")
            .with_attribute(("id", relation.id.to_string().as_str()));

        if let Some(version) = &relation.version {
            elem = elem.with_attribute(("version", version.to_string().as_str()));
        }
        if let Some(timestamp) = &relation.timestamp {
            elem = elem.with_attribute(("timestamp", timestamp.to_string().as_str()));
        }
        if let Some(uid) = &relation.uid {
            elem = elem.with_attribute(("uid", uid.to_string().as_str()));
        }
        if let Some(user) = &relation.user {
            elem = elem.with_attribute(("user".as_bytes(), user.to_string().as_bytes()));
        }
        if let Some(changeset) = &relation.changeset {
            elem = elem.with_attribute(("changeset", changeset.to_string().as_str()));
        }

        elem.write_inner_content(|writer| {
            for m in &relation.members {
                writer
                    .create_element("member")
                    .with_attribute(("type", m.type_.as_str()))
                    .with_attribute(("ref", m.ref_.to_string().as_str()))
                    .with_attribute(("role", m.role.as_str()))
                    .write_empty()
                    .unwrap();
            }
            if relation.tags.is_some() {
                for (k, v) in relation.tags.as_ref().unwrap() {
                    writer
                        .create_element("tag")
                        .with_attribute(("k".as_bytes(), k.as_bytes()))
                        .with_attribute(("v".as_bytes(), v.as_bytes()))
                        .write_empty()
                        .unwrap();
                }
            }
            if let Some(bb) = &relation.bbox {
                writer
                    .create_element("bbox")
                    .with_attribute(("minlat", bb.minlat().to_string().as_str()))
                    .with_attribute(("maxlat", bb.maxlat().to_string().as_str()))
                    .with_attribute(("minlon", bb.minlon().to_string().as_str()))
                    .with_attribute(("maxlon", bb.maxlon().to_string().as_str()))
                    .write_empty()
                    .unwrap();
            }
            Ok(())
        })
        .unwrap();

        Ok(())
    }

    fn write_start(&mut self, change: bool) -> Result<(), Box<dyn Error>> {
        self.xmlwriter = Some(self.xmlwriter(&self.filename).unwrap());

        let mut elem = if change {
            BytesStart::new("osmChange")
        } else {
            BytesStart::new("osm")
        };
        elem.push_attribute(("version", "0.6"));
        elem.push_attribute(("generator", "xmlwriter"));

        self.xmlwriter
            .as_mut()
            .unwrap()
            .write_event(Event::Start(elem))?;

        Ok(())
    }
    fn write_end(&mut self, change: bool) -> Result<(), Box<dyn Error>> {
        if self.actionwriter != Action::None {
            let action_str = match self.actionwriter {
                Action::Create() => "create",
                Action::Modify() => "modify",
                Action::Delete() => "delete",
                Action::None => "",
            };
            self.xmlwriter
                .as_mut()
                .unwrap()
                .write_event(Event::End(BytesEnd::new(action_str)))
                .unwrap();
        }

        let elem = if change {
            BytesEnd::new("osmChange")
        } else {
            BytesEnd::new("osm")
        };
        self.xmlwriter
            .as_mut()
            .unwrap()
            .write_event(Event::End(elem))?;

        self.xmlwriter = None;

        Ok(())
    }
}

impl OsmUpdate for OsmXml {
    fn update_node(&mut self, node: &mut Node, action: &Action) -> Result<(), io::Error> {
        self.write_action_start(action);
        self.write_node(node)?;
        Ok(())
    }
    fn update_way(&mut self, way: &mut Way, action: &Action) -> Result<(), io::Error> {
        self.write_action_start(action);
        self.write_way(way)?;
        Ok(())
    }
    fn update_relation(
        &mut self,
        relation: &mut Relation,
        action: &Action,
    ) -> Result<(), io::Error> {
        self.write_action_start(action);
        self.write_relation(relation)?;
        Ok(())
    }
}
