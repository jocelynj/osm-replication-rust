use flate2::bufread::GzDecoder;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::str;

use crate::osm::{Member, Node, Relation, Way};
use crate::osm::{OsmCopyTo, OsmUpdate, OsmUpdateTo, OsmWriter};

enum CurObj {
    Empty(),
    Node(Node),
    Way(Way),
    Relation(Relation),
}

enum Action {
    Create(),
    Modify(),
    Delete(),
}

pub struct OsmXml {
    filename: String,
}

impl OsmXml {
    pub fn new(filename: &str) -> Result<OsmXml, ()> {
        Ok(OsmXml {
            filename: filename.to_string(),
        })
    }
    pub fn xmlreader(&self, filename: &str) -> Result<Reader<Box<dyn BufRead>>, Box<dyn Error>> {
        let freader = Box::new(File::open(&filename)?);
        let reader: Box<dyn BufRead>;
        if self.filename.ends_with(".gz") {
            let breader = BufReader::new(freader);
            let gzreader = GzDecoder::new(breader);
            reader = Box::new(BufReader::new(gzreader));
        } else {
            reader = Box::new(BufReader::new(freader));
        }
        Ok(Reader::from_reader(reader))
    }
}

impl OsmCopyTo for OsmXml {
    fn copy_to(&mut self, target: &mut impl OsmWriter) -> Result<(), Box<dyn Error>> {
        let mut reader = self.xmlreader(&self.filename).unwrap();

        let mut buf = Vec::new();

        let mut tags: HashMap<String, String> = HashMap::new();
        let mut nodes: Vec<u64> = Vec::new();
        let mut members: Vec<Member> = Vec::new();

        let mut curobj = CurObj::Empty();

        loop {
            match reader.read_event_into(&mut buf) {
                Err(e) => panic!("Error at position {}: {:?}", reader.error_position(), e),
                Ok(Event::Eof) => break, // end of file

                Ok(Event::Start(e)) => match e.name().as_ref() {
                    b"osm" => target.write_start()?,
                    b"node" => {
                        let mut id: u64 = 0;
                        let mut decimicro_lat: i32 = 0;
                        let mut decimicro_lon: i32 = 0;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"lat" => {
                                    decimicro_lat =
                                        Node::coord_to_decimicro(v.parse::<f64>().unwrap())
                                }
                                b"lon" => {
                                    decimicro_lon =
                                        Node::coord_to_decimicro(v.parse::<f64>().unwrap())
                                }
                                _ => (),
                            }
                        }
                        tags = HashMap::new();
                        curobj = CurObj::Node(Node {
                            id,
                            decimicro_lat,
                            decimicro_lon,
                            tags: None,
                        });
                    }
                    b"way" => {
                        let id = e
                            .attributes()
                            .find(|x| x.as_ref().unwrap().key.as_ref() == b"id")
                            .unwrap()
                            .unwrap();
                        let id: u64 = str::from_utf8(&id.value)?.parse()?;
                        tags = HashMap::new();
                        nodes = Vec::new();
                        curobj = CurObj::Way(Way {
                            id,
                            nodes: Vec::new(),
                            tags: None,
                        });
                    }
                    b"relation" => {
                        let id = e
                            .attributes()
                            .find(|x| x.as_ref().unwrap().key.as_ref() == b"id")
                            .unwrap()
                            .unwrap();
                        let id: u64 = str::from_utf8(&id.value)?.parse()?;
                        tags = HashMap::new();
                        members = Vec::new();
                        curobj = CurObj::Relation(Relation {
                            id,
                            members: Vec::new(),
                            tags: None,
                        });
                    }
                    k => println!("Unsupported start element: {}", str::from_utf8(&k)?),
                },
                Ok(Event::End(e)) => match e.name().as_ref() {
                    b"osm" => target.write_end()?,
                    b"node" => {
                        if let CurObj::Node(ref mut node) = curobj {
                            node.tags = Some(tags);
                            tags = HashMap::new();
                            target.write_node(&node)?;
                        } else {
                            panic!("Expected an initialized node");
                        }
                    }
                    b"way" => {
                        if let CurObj::Way(ref mut way) = curobj {
                            way.nodes = nodes;
                            way.tags = Some(tags);
                            nodes = Vec::new();
                            tags = HashMap::new();
                            target.write_way(&way)?;
                        } else {
                            panic!("Expected an initialized way");
                        }
                    }
                    b"relation" => {
                        if let CurObj::Relation(ref mut relation) = curobj {
                            relation.members = members;
                            relation.tags = Some(tags);
                            members = Vec::new();
                            tags = HashMap::new();
                            target.write_relation(&relation)?;
                        } else {
                            panic!("Expected an initialized relation");
                        }
                    }
                    k => println!("Unsupported end element: {}", str::from_utf8(&k)?),
                },
                Ok(Event::Empty(e)) => match e.name().as_ref() {
                    b"bounds" => (),
                    b"node" => {
                        let mut id: u64 = 0;
                        let mut decimicro_lat: i32 = 0;
                        let mut decimicro_lon: i32 = 0;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"lat" => {
                                    decimicro_lat =
                                        Node::coord_to_decimicro(v.parse::<f64>().unwrap())
                                }
                                b"lon" => {
                                    decimicro_lon =
                                        Node::coord_to_decimicro(v.parse::<f64>().unwrap())
                                }
                                _ => (),
                            }
                        }
                        target.write_node(&Node {
                            id,
                            decimicro_lat,
                            decimicro_lon,
                            tags: None,
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
                        tags.insert(key, val);
                    }
                    k => println!("Unsupported empty element: {}", str::from_utf8(&k)?),
                },
                Ok(Event::Text(_)) => (),
                Ok(Event::Decl(_)) => (),
                e => println!("Unsupported entry: {:?}", e?),
            }
        }

        Ok(())
    }
}

impl OsmUpdateTo for OsmXml {
    fn update_to(&mut self, target: &mut impl OsmUpdate) -> Result<(), Box<dyn Error>> {
        let mut reader = self.xmlreader(&self.filename).unwrap();

        let mut buf = Vec::new();

        let mut tags: HashMap<String, String> = HashMap::new();
        let mut nodes: Vec<u64> = Vec::new();
        let mut members: Vec<Member> = Vec::new();

        let mut curaction = Action::Create();
        let mut curobj = CurObj::Empty();

        loop {
            match reader.read_event_into(&mut buf) {
                Err(e) => panic!("Error at position {}: {:?}", reader.error_position(), e),
                Ok(Event::Eof) => break, // end of file

                Ok(Event::Start(e)) => match e.name().as_ref() {
                    b"osm" => target.write_start()?,
                    b"osmChange" => target.write_start()?,
                    b"node" => {
                        let mut id: u64 = 0;
                        let mut decimicro_lat: i32 = 0;
                        let mut decimicro_lon: i32 = 0;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"lat" => {
                                    decimicro_lat =
                                        Node::coord_to_decimicro(v.parse::<f64>().unwrap())
                                }
                                b"lon" => {
                                    decimicro_lon =
                                        Node::coord_to_decimicro(v.parse::<f64>().unwrap())
                                }
                                _ => (),
                            }
                        }
                        tags = HashMap::new();
                        curobj = CurObj::Node(Node {
                            id,
                            decimicro_lat,
                            decimicro_lon,
                            tags: None,
                        });
                    }
                    b"way" => {
                        let id = e
                            .attributes()
                            .find(|x| x.as_ref().unwrap().key.as_ref() == b"id")
                            .unwrap()
                            .unwrap();
                        let id: u64 = str::from_utf8(&id.value)?.parse()?;
                        tags = HashMap::new();
                        nodes = Vec::new();
                        curobj = CurObj::Way(Way {
                            id,
                            nodes: Vec::new(),
                            tags: None,
                        });
                    }
                    b"relation" => {
                        let id = e
                            .attributes()
                            .find(|x| x.as_ref().unwrap().key.as_ref() == b"id")
                            .unwrap()
                            .unwrap();
                        let id: u64 = str::from_utf8(&id.value)?.parse()?;
                        tags = HashMap::new();
                        members = Vec::new();
                        curobj = CurObj::Relation(Relation {
                            id,
                            members: Vec::new(),
                            tags: None,
                        });
                    }
                    b"create" => curaction = Action::Create(),
                    b"modify" => curaction = Action::Modify(),
                    b"delete" => curaction = Action::Delete(),
                    k => println!("Unsupported start element: {}", str::from_utf8(&k)?),
                },
                Ok(Event::End(e)) => match e.name().as_ref() {
                    b"osm" => target.write_end()?,
                    b"osmChange" => target.write_end()?,
                    b"node" => {
                        if let CurObj::Node(ref mut node) = curobj {
                            node.tags = Some(tags);
                            tags = HashMap::new();
                            if let Action::Delete() = curaction {
                                target.delete_node(&node)?;
                            } else {
                                target.write_node(&node)?;
                            }
                        } else {
                            panic!("Expected an initialized node");
                        }
                    }
                    b"way" => {
                        if let CurObj::Way(ref mut way) = curobj {
                            way.nodes = nodes;
                            way.tags = Some(tags);
                            nodes = Vec::new();
                            tags = HashMap::new();
                            if let Action::Delete() = curaction {
                                target.delete_way(&way)?;
                            } else {
                                target.write_way(&way)?;
                            }
                        } else {
                            panic!("Expected an initialized way");
                        }
                    }
                    b"relation" => {
                        if let CurObj::Relation(ref mut relation) = curobj {
                            relation.members = members;
                            relation.tags = Some(tags);
                            members = Vec::new();
                            tags = HashMap::new();
                            if let Action::Delete() = curaction {
                                target.delete_relation(&relation)?;
                            } else {
                                target.write_relation(&relation)?;
                            }
                        } else {
                            panic!("Expected an initialized relation");
                        }
                    }
                    b"create" => (),
                    b"modify" => (),
                    b"delete" => (),
                    k => println!("Unsupported end element: {}", str::from_utf8(&k)?),
                },
                Ok(Event::Empty(e)) => match e.name().as_ref() {
                    b"bounds" => (),
                    b"node" => {
                        let mut id: u64 = 0;
                        let mut decimicro_lat: i32 = 0;
                        let mut decimicro_lon: i32 = 0;
                        for a in e.attributes() {
                            let a = a.unwrap();
                            let k = a.key.as_ref();
                            let v = str::from_utf8(&a.value).unwrap();

                            match k {
                                b"id" => id = v.parse().unwrap(),
                                b"lat" => {
                                    decimicro_lat =
                                        Node::coord_to_decimicro(v.parse::<f64>().unwrap())
                                }
                                b"lon" => {
                                    decimicro_lon =
                                        Node::coord_to_decimicro(v.parse::<f64>().unwrap())
                                }
                                _ => (),
                            }
                        }
                        let node = Node {
                            id,
                            decimicro_lat,
                            decimicro_lon,
                            tags: None,
                        };
                        if let Action::Delete() = curaction {
                            target.delete_node(&node)?;
                        } else {
                            target.write_node(&node)?;
                        }
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
                        tags.insert(key, val);
                    }
                    k => println!("Unsupported empty element: {}", str::from_utf8(&k)?),
                },
                Ok(Event::Text(_)) => (),
                Ok(Event::Decl(_)) => (),
                e => println!("Unsupported entry: {:?}", e?),
            }
        }

        Ok(())
    }
}
