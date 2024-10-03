use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const NODE_CRD: &str = "node.crd";
const WAY_IDX: &str = "way.idx";
const WAY_DATA: &str = "way.data";
const WAY_FREE: &str = "way.free";

/// Size of a node-id stored in node.crd or way.data
pub const NODE_ID_SIZE: usize = 5;
/// Size of a way pointer in way.idx to way.data
pub const WAY_PTR_SIZE: usize = 5;

/// Simplified OpenStreetMap database
///
/// Database used by `OsmBin` is stored in few files:
/// - `node.crd`: stores latitude/longitude of node, as 2*4 bytes. File is directly indexed by node
/// id. Not allocated nodes are not written to file, so its size is smaller than `max(node_id) *
/// 8`, thanks to sparse files.
/// - `way.idx`: stores a pointer into `way.data`, as [`WAY_PTR_SIZE`] bytes. File is directly
/// indexed by way id.
/// - `way.data`: stores a list of nodes id, as `number of nodes` (2-bytes), followed by N node-id
/// (each using [`NODE_ID_SIZE`] bytes). File is indexed by pointer given by `way.idx`.
pub struct OsmBin {
    dir: String,
    node_crd: File,
    way_idx: File,
    way_data: File,
    way_data_size: u64,
    way_free: File,
}

impl OsmBin {
    pub fn new(dir: &str) -> Result<OsmBin, io::Error> {
        let mut file_options = OpenOptions::new();
        file_options.read(true).write(true);
        let node_crd = file_options.open(Path::new(dir).join(NODE_CRD))?;
        let way_idx = file_options.open(Path::new(dir).join(WAY_IDX))?;

        let way_data = file_options.open(Path::new(dir).join(WAY_DATA))?;
        let way_data_size = way_data.metadata()?.len();

        let way_free = file_options.open(Path::new(dir).join(WAY_FREE))?;

        Ok(OsmBin {
            dir: dir.to_string(),
            node_crd,
            way_idx,
            way_data,
            way_data_size,
            way_free,
        })
    }

    pub fn init(dir: &str) {
        match fs::create_dir(dir) {
            Ok(_) => (),
            Err(error) => match error.kind() {
                ErrorKind::AlreadyExists => (),
                _ => panic!("Error with directory {dir}: {error}"),
            },
        };

        for filename in vec![NODE_CRD, WAY_IDX, WAY_DATA, WAY_FREE] {
            let full_filename = Path::new(dir).join(filename);
            let f = File::create_new(full_filename);
            match f {
                Ok(_) => (),
                Err(error) => match error.kind() {
                    ErrorKind::AlreadyExists => (),
                    _ => panic!("Error with file {filename}: {error}"),
                },
            };
        }
        match fs::create_dir(Path::new(dir).join("relation")) {
            Ok(_) => (),
            Err(error) => match error.kind() {
                ErrorKind::AlreadyExists => (),
                _ => panic!("Error with directory {dir}: {error}"),
            },
        };
    }

    pub fn read_node(&mut self, id: u64) -> Option<Node> {
        self.node_crd
            .seek(SeekFrom::Start(id * 8))
            .expect("Could not seek");
        let mut lat_buffer = [0u8; 4];
        let mut lon_buffer = [0u8; 4];
        let lat_read_count = self.node_crd.read(&mut lat_buffer).expect("Could not read");
        let lon_read_count = self.node_crd.read(&mut lon_buffer).expect("Could not read");

        if lat_read_count == 0
            || lon_read_count == 0
            || lat_buffer == [0u8; 4]
            || lon_buffer == [0u8; 4]
        {
            return None;
        }
        let lat = Self::bytes4_to_coord(&lat_buffer);
        let lon = Self::bytes4_to_coord(&lon_buffer);

        Some(Node {
            id,
            lat,
            lon,
            tags: None,
        })
    }

    pub fn write_node(&mut self, node: &Node) -> Result<(), io::Error> {
        let lat = Self::coord_to_bytes4(node.lat);
        let lon = Self::coord_to_bytes4(node.lon);
        self.node_crd.seek(SeekFrom::Start(node.id * 8)).unwrap();
        self.node_crd.write(&lat).unwrap();
        self.node_crd.write(&lon).unwrap();

        Ok(())
    }

    pub fn delete_node(&mut self, node: Node) -> Result<(), io::Error> {
        let empty: Vec<u8> = vec![0; 8];
        self.node_crd.seek(SeekFrom::Start(node.id * 8))?;
        self.node_crd.write(&empty)?;

        Ok(())
    }

    pub fn read_way(&mut self, id: u64) -> Option<Way> {
        self.way_idx
            .seek(SeekFrom::Start(id * (WAY_PTR_SIZE as u64)))
            .expect("Could not seek");
        let mut buffer = [0u8; WAY_PTR_SIZE];
        let read_count = self.way_idx.read(&mut buffer).expect("Could not read");

        if read_count == 0 || buffer == [0u8; WAY_PTR_SIZE] {
            return None;
        }
        let way_data_addr = Self::bytes5_to_int(&buffer);

        self.way_data
            .seek(SeekFrom::Start(way_data_addr))
            .expect("Could not seek");
        let mut buffer = [0u8; 2];
        let read_count = self.way_data.read(&mut buffer).expect("Could not read");
        if read_count == 0 || buffer == [0u8; 2] {
            return None;
        }
        let num_nodes = Self::bytes2_to_int(&buffer);

        let mut buffer = [0u8; NODE_ID_SIZE];

        let mut nodes: Vec<u64> = Vec::new();
        for _ in 0..num_nodes {
            let read_count = self.way_data.read(&mut buffer).expect("Could not read");
            if read_count == 0 || buffer == [0u8; NODE_ID_SIZE] {
                return None;
            }
            nodes.push(Self::bytes5_to_int(&buffer));
        }

        Some(Way {
            id,
            nodes,
            tags: None,
        })
    }
    pub fn write_way(&mut self, way: &Way) -> Result<(), io::Error> {
        self.delete_way(&way)?;
        // TODO: implement way_free
        let way_data_addr = self.way_data_size;

        self.way_data.seek(SeekFrom::Start(way_data_addr))?;
        let num_nodes = Self::int_to_bytes2(way.nodes.len() as u16);
        self.way_data.write(&num_nodes)?;
        for n in &way.nodes {
            let node = Self::int_to_bytes5(*n);
            self.way_data.write(&node)?;
        }

        self.way_idx
            .seek(SeekFrom::Start(way.id * (WAY_PTR_SIZE as u64)))?;
        let buffer = Self::int_to_bytes5(way_data_addr);
        self.way_idx.write(&buffer)?;

        self.way_data_size += 2 + (NODE_ID_SIZE * way.nodes.len()) as u64;

        Ok(())
    }
    pub fn delete_way(&mut self, way: &Way) -> Result<(), io::Error> {
        let way_idx_addr = way.id * (WAY_PTR_SIZE as u64);
        self.way_idx.seek(SeekFrom::Start(way_idx_addr))?;
        let mut buffer = [0u8; WAY_PTR_SIZE];
        let read_count = self.way_idx.read(&mut buffer)?;

        if read_count == 0 || buffer == [0u8; WAY_PTR_SIZE] {
            return Ok(());
        }
        let way_data_addr = Self::bytes5_to_int(&buffer);
        // TODO: implement way_free
        //
        let buffer = vec![0; WAY_PTR_SIZE];
        self.way_idx.seek(SeekFrom::Start(way_idx_addr))?;
        self.way_idx.write(&buffer)?;

        Ok(())
    }

    pub fn read_relation(&self, id: u64) -> Option<Relation> {
        let relid_digits = Self::to_digits(id);
        let relid_part0 = Self::join_nums(&relid_digits[0..3]);
        let relid_part1 = Self::join_nums(&relid_digits[3..6]);
        let relid_part2 = Self::join_nums(&relid_digits[6..9]);
        let rel_path = Path::new(&self.dir)
            .join("relation")
            .join(relid_part0)
            .join(relid_part1)
            .join(relid_part2);
        let rel_data = fs::read_to_string(&rel_path);
        let rel_data = match rel_data {
            Ok(d) => d,
            Err(error) => match error.kind() {
                ErrorKind::NotFound => return None,
                _ => panic!("Error with file {rel_path:?}: {error}"),
            },
        };
        let u: Relation = serde_json::from_str(rel_data.as_str()).unwrap();

        Some(u)
    }
    pub fn write_relation(&self, relation: &Relation) -> Result<(), io::Error> {
        let relid_digits = Self::to_digits(relation.id);
        let relid_part0 = Self::join_nums(&relid_digits[0..3]);
        let relid_part1 = Self::join_nums(&relid_digits[3..6]);
        let relid_part2 = Self::join_nums(&relid_digits[6..9]);
        let rel_path = Path::new(&self.dir)
            .join("relation")
            .join(relid_part0)
            .join(relid_part1)
            .join(relid_part2);
        match fs::create_dir_all(rel_path.parent().unwrap()) {
            Ok(_) => (),
            Err(error) => match error.kind() {
                ErrorKind::AlreadyExists => (),
                _ => panic!("Error with directory: {error}"),
            },
        };

        let json_data = serde_json::to_string(relation)?;
        fs::write(&rel_path, json_data)?;

        Ok(())
    }

    fn bytes5_to_int(d: &[u8; 5]) -> u64 {
        let mut arr: Vec<u8> = Vec::with_capacity(8);
        arr.extend([0; 3]);
        arr.extend(d);
        u64::from_be_bytes(arr.as_slice().try_into().unwrap())
    }
    fn int_to_bytes5(d: u64) -> [u8; 5] {
        if d > 2_u64.pow(5 * 8) {
            panic!("Integer {d:#x} do not fit on 5 bytes");
        }
        let v = d.to_be_bytes();
        let arr: [u8; 5] = v[3..8].try_into().unwrap();
        arr
    }

    fn bytes4_to_int(d: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*d)
    }
    fn int_to_bytes4(d: u32) -> [u8; 4] {
        d.to_be_bytes()
    }

    fn bytes4_to_coord(d: &[u8; 4]) -> f64 {
        (((Self::bytes4_to_int(d) as i64) - 1800000000) as f64) / 10000000.0
    }
    fn coord_to_bytes4(d: f64) -> [u8; 4] {
        Self::int_to_bytes4((((d * 10000000.0) as i64) + 1800000000) as u32)
    }

    fn bytes2_to_int(d: &[u8; 2]) -> u16 {
        u16::from_be_bytes(*d)
    }
    fn int_to_bytes2(d: u16) -> [u8; 2] {
        d.to_be_bytes()
    }

    fn to_digits(v: u64) -> Vec<u8> {
        let mut v = v.clone();
        let mut digits: Vec<u8> = Vec::with_capacity(10);
        while v > 0 {
            let n = (v % 10) as u8;
            v /= 10;
            digits.push(n);
        }
        if digits.len() < 9 {
            let missing = 9 - digits.len();
            for _ in 0..missing {
                digits.push(0);
            }
        }
        digits.reverse();
        digits
    }

    fn join_nums(nums: &[u8]) -> String {
        let str_nums: Vec<String> = nums.iter().map(|n| n.to_string()).collect();
        str_nums.join("")
    }
}

/// Node
#[derive(Debug, PartialEq)]
pub struct Node {
    /// Node id
    pub id: u64,
    /// Latitude in degree
    pub lat: f64,
    /// Longitude in degree
    pub lon: f64,
    /// Tags
    pub tags: Option<HashMap<String, String>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_node() {
        let mut osmbin = OsmBin::new("/home/jocelyn/.cache/osmbin-bdd/").unwrap();

        let node = osmbin.read_node(266053077);
        assert_eq!(
            Node {
                id: 266053077,
                lat: 17.9031745,
                lon: -62.8363074,
                tags: None
            },
            node.unwrap()
        );

        let node = osmbin.read_node(2619283352);
        assert_eq!(
            Node {
                id: 2619283352,
                lat: 17.9005419,
                lon: -62.8327042,
                tags: None
            },
            node.unwrap()
        );

        let node = osmbin.read_node(1);
        assert_eq!(true, node.is_none());

        let node = osmbin.read_node(266053076);
        assert_eq!(true, node.is_none());

        let node = osmbin.read_node(2619283353);
        assert_eq!(true, node.is_none());
    }

    #[test]
    fn read_way() {
        let mut osmbin = OsmBin::new("/home/jocelyn/.cache/osmbin-bdd/").unwrap();

        let way = osmbin.read_way(24473155);
        assert_eq!(true, way.is_some());
        assert_eq!(1665, way.unwrap().nodes.len());

        let way = osmbin.read_way(255316725);
        assert_eq!(
            Way {
                id: 255316725,
                nodes: vec![2610107905, 2610107903, 2610107901, 2610107902, 2610107904, 2610107905],
                tags: None
            },
            way.unwrap()
        );

        let way = osmbin.read_way(1);
        assert_eq!(true, way.is_none());

        let way = osmbin.read_way(24473154);
        assert_eq!(true, way.is_none());

        let way = osmbin.read_way(255316726);
        assert_eq!(true, way.is_none());
    }

    #[test]
    fn read_relation() {
        let osmbin = OsmBin::new("/home/jocelyn/.cache/osmbin-bdd/").unwrap();

        let rel = osmbin.read_relation(47796);
        assert_eq!(true, rel.is_some());

        let rel = osmbin.read_relation(529891);
        let exp_rel = Relation { id: 529891,
        members: vec![Member{ref_: 670634766, role: String::from(""), type_: String::from("node")},
                      Member{ref_: 670634768, role: String::from(""), type_: String::from("node")}],
            tags: Some(HashMap::from([
                                (String::from("name"), String::from("Saint-Barthélemy III")),
                                (String::from("note"), String::from("la Barriere des Quatre Vents")),
                                (String::from("ref"), String::from("9712303")),
                                (String::from("site"), String::from("geodesic")),
                                (String::from("source"), String::from("©IGN 2010 dans le cadre de la cartographie réglementaire")),
                                (String::from("type"), String::from("site")),
                                (String::from("url"), String::from("http://ancien-geodesie.ign.fr/fiche_geodesie_OM.asp?num_site=9712303&X=519509&Y=1980304"))]))
        };
        assert_eq!(exp_rel, rel.unwrap());

        let rel = osmbin.read_relation(2324452);
        let exp_rel = Relation {
            id: 2324452,
            members: vec![
                Member {
                    type_: String::from("node"),
                    ref_: 279149652,
                    role: String::from("admin_centre"),
                },
                Member {
                    type_: String::from("way"),
                    ref_: 174027472,
                    role: String::from("outer"),
                },
                Member {
                    type_: String::from("way"),
                    ref_: 53561037,
                    role: String::from("outer"),
                },
                Member {
                    type_: String::from("way"),
                    ref_: 53561045,
                    role: String::from("outer"),
                },
                Member {
                    type_: String::from("way"),
                    ref_: 53656098,
                    role: String::from("outer"),
                },
                Member {
                    type_: String::from("way"),
                    ref_: 174027473,
                    role: String::from("outer"),
                },
                Member {
                    type_: String::from("way"),
                    ref_: 174023902,
                    role: String::from("outer"),
                },
            ],
            tags: Some(HashMap::from([
                (String::from("admin_level"), String::from("8")),
                (String::from("boundary"), String::from("administrative")),
                (String::from("local_name"), String::from("Statia")),
                (String::from("name"), String::from("Sint Eustatius")),
                (String::from("name:el"), String::from("Άγιος Ευστάθιος")),
                (String::from("name:fr"), String::from("Saint-Eustache")),
                (String::from("name:nl"), String::from("Sint Eustatius")),
                (String::from("type"), String::from("boundary")),
            ])),
        };
        assert_eq!(exp_rel, rel.unwrap());

        let rel = osmbin.read_relation(1);
        assert_eq!(true, rel.is_none());

        let rel = osmbin.read_relation(47795);
        assert_eq!(true, rel.is_none());

        let rel = osmbin.read_relation(2707694);
        assert_eq!(true, rel.is_none());
    }

    #[test]
    fn bytes5_to_int() {
        assert_eq!(0x00_00_00_00_00, OsmBin::bytes5_to_int(&[0, 0, 0, 0, 0]));
        assert_eq!(
            0x12_23_45_67_89,
            OsmBin::bytes5_to_int(&[0x12, 0x23, 0x45, 0x67, 0x89])
        );
    }
    #[test]
    fn int_to_bytes5() {
        assert_eq!([0, 0, 0, 0, 0], OsmBin::int_to_bytes5(0));
        assert_eq!(
            [0x12, 0x23, 0x45, 0x67, 0x89],
            OsmBin::int_to_bytes5(0x12_23_45_67_89)
        );
    }
    #[test]
    fn bytes5() {
        for n in 0_u64..100000_u64 {
            assert_eq!(n, OsmBin::bytes5_to_int(&OsmBin::int_to_bytes5(n)));
            assert_eq!(
                14 * n,
                OsmBin::bytes5_to_int(&OsmBin::int_to_bytes5(14 * n))
            );
            assert_eq!(
                1098 * n,
                OsmBin::bytes5_to_int(&OsmBin::int_to_bytes5(1098 * n))
            );
            assert_eq!(
                4898481 * n,
                OsmBin::bytes5_to_int(&OsmBin::int_to_bytes5(4898481 * n))
            );
        }
    }
    #[test]
    #[should_panic]
    fn int_to_bytes5_too_big() {
        OsmBin::int_to_bytes5(0x99_12_23_45_67_89);
    }
}
