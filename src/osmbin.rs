use serde_json;
use std::cmp;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind};
use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::io::{BufReader, BufWriter};
use std::path::Path;

use crate::bufreaderwriter;
use crate::osm::{Action, Node, Relation, Way};
use crate::osm::{OsmReader, OsmUpdate, OsmWriter};

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
    node_crd: bufreaderwriter::BufReaderWriterRand<File>,
    way_idx: bufreaderwriter::BufReaderWriterRand<File>,
    way_data: bufreaderwriter::BufReaderWriterRand<File>,
    way_free_data: HashMap<u16, Vec<u64>>,

    way_idx_size: u64,
    way_data_size: u64,
}

enum OpenMode {
    Read,
    Write,
}

impl OsmBin {
    pub fn new(dir: &str) -> Result<OsmBin, io::Error> {
        Self::new_any(dir, OpenMode::Read)
    }
    pub fn new_writer(dir: &str) -> Result<OsmBin, io::Error> {
        Self::new_any(dir, OpenMode::Write)
    }
    fn new_any(dir: &str, mode: OpenMode) -> Result<OsmBin, io::Error> {
        let mut file_options = OpenOptions::new();
        file_options.read(true);
        if let OpenMode::Write = mode {
            file_options.write(true);
        }
        let node_crd = file_options.open(Path::new(dir).join(NODE_CRD))?;
        let node_crd = bufreaderwriter::BufReaderWriterRand::new_reader(node_crd);
        let way_idx = file_options.open(Path::new(dir).join(WAY_IDX))?;
        let way_idx_size = way_idx.metadata()?.len();
        let way_idx = bufreaderwriter::BufReaderWriterRand::new_reader(way_idx);

        let way_data = file_options.open(Path::new(dir).join(WAY_DATA))?;
        let way_data_size = way_data.metadata()?.len();
        let way_data = bufreaderwriter::BufReaderWriterRand::new_reader(way_data);

        let way_free = file_options.open(Path::new(dir).join(WAY_FREE))?;
        let way_free = BufReader::new(way_free);
        let mut way_free_data: HashMap<u16, Vec<u64>> = HashMap::new();

        if let OpenMode::Write = mode {
            for line in way_free.lines() {
                let line = line.unwrap();
                let mut s = line.split(";");
                let pos: u64 = s.next().unwrap().parse().unwrap();
                let num_nodes: u16 = s.next().unwrap().parse().unwrap();
                way_free_data
                    .entry(num_nodes)
                    .or_insert_with(|| Vec::new())
                    .push(pos);
            }
        }

        Ok(OsmBin {
            dir: dir.to_string(),
            node_crd,
            way_idx,
            way_data,
            way_free_data,
            way_idx_size,
            way_data_size,
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
                Ok(mut file) => {
                    if filename == WAY_DATA {
                        file.write(b"--").expect("Could not write to {filename}");
                    }
                }
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

    fn bytes4_to_coord(d: &[u8; 4]) -> i32 {
        // TODO: Store directly i32 instead of converting to a positive number
        (Self::bytes4_to_int(d) as i32) - 1800000000
    }
    fn coord_to_bytes4(d: i32) -> [u8; 4] {
        // TODO: Store directly i32 instead of converting to a positive number
        Self::int_to_bytes4(((d as i64) + 1800000000) as u32)
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

impl Drop for OsmBin {
    fn drop(&mut self) {
        let way_free = File::create(Path::new(&self.dir).join(WAY_FREE)).unwrap();
        let mut way_free = BufWriter::new(way_free);

        for (num_nodes, v) in &self.way_free_data {
            for pos in v {
                write!(way_free, "{};{}\n", pos, num_nodes).unwrap();
            }
        }
    }
}

impl OsmReader for OsmBin {
    fn read_node(&mut self, id: u64) -> Option<Node> {
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
        let decimicro_lat = Self::bytes4_to_coord(&lat_buffer);
        let decimicro_lon = Self::bytes4_to_coord(&lon_buffer);

        Some(Node {
            id,
            decimicro_lat,
            decimicro_lon,
            tags: None,
        })
    }
    fn read_way(&mut self, id: u64) -> Option<Way> {
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
    fn read_relation(&mut self, id: u64) -> Option<Relation> {
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
}

impl OsmWriter for OsmBin {
    fn write_node(&mut self, node: &Node) -> Result<(), io::Error> {
        let lat = Self::coord_to_bytes4(node.decimicro_lat);
        let lon = Self::coord_to_bytes4(node.decimicro_lon);
        let node_crd_addr = node.id * 8;

        // Try not to seek if not necessary, as seeking flushes write buffer
        if self.node_crd.stream_position().unwrap() != node_crd_addr {
            self.node_crd.seek(SeekFrom::Start(node_crd_addr)).unwrap();
        }
        self.node_crd.write(&lat).unwrap();
        self.node_crd.write(&lon).unwrap();

        Ok(())
    }
    fn write_way(&mut self, way: &Way) -> Result<(), io::Error> {
        let way_idx_addr = way.id * (WAY_PTR_SIZE as u64);

        // Only need to delete way if it could be inside file
        if way_idx_addr < self.way_idx_size {
            self.update_way(&way, &Action::Delete())?;
        }
        let num_nodes = way.nodes.len() as u16;
        let way_data_addr = self
            .way_free_data
            .get_mut(&num_nodes)
            .unwrap_or(&mut Vec::new())
            .pop()
            .unwrap_or(self.way_data_size);

        // Try not to seek if not necessary, as seeking flushes write buffer
        if self.way_data.stream_position().unwrap() != way_data_addr {
            self.way_data.seek(SeekFrom::Start(way_data_addr))?;
        }
        let num_nodes = Self::int_to_bytes2(num_nodes);
        self.way_data.write(&num_nodes)?;
        for n in &way.nodes {
            let node = Self::int_to_bytes5(*n);
            self.way_data.write(&node)?;
        }

        // Try not to seek if not necessary, as seeking flushes write buffer
        if self.way_idx.stream_position().unwrap() != way_idx_addr {
            self.way_idx.seek(SeekFrom::Start(way_idx_addr))?;
        }
        let buffer = Self::int_to_bytes5(way_data_addr);
        self.way_idx.write(&buffer)?;

        self.way_idx_size = cmp::max(self.way_idx_size, self.way_idx.stream_position().unwrap());
        self.way_data_size = cmp::max(self.way_data_size, self.way_data.stream_position().unwrap());

        Ok(())
    }
    fn write_relation(&mut self, relation: &Relation) -> Result<(), io::Error> {
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
}

impl OsmUpdate for OsmBin {
    fn update_node(&mut self, node: &Node, action: &Action) -> Result<(), io::Error> {
        if *action == Action::Delete() {
            let empty: Vec<u8> = vec![0; 8];
            self.node_crd.seek(SeekFrom::Start(node.id * 8))?;
            self.node_crd.write(&empty)?;
        } else {
            self.write_node(node)?;
        }

        Ok(())
    }
    fn update_way(&mut self, way: &Way, action: &Action) -> Result<(), io::Error> {
        if *action == Action::Delete() {
            let way_idx_addr = way.id * (WAY_PTR_SIZE as u64);
            self.way_idx.seek(SeekFrom::Start(way_idx_addr))?;
            let mut buffer = [0u8; WAY_PTR_SIZE];
            let read_count = self.way_idx.read(&mut buffer)?;

            if read_count == 0 || buffer == [0u8; WAY_PTR_SIZE] {
                return Ok(());
            }
            let way_data_addr = Self::bytes5_to_int(&buffer);

            self.way_data
                .seek(SeekFrom::Start(way_data_addr))
                .expect("Could not seek");
            let mut buffer = [0u8; 2];
            let read_count = self.way_data.read(&mut buffer).expect("Could not read");
            if read_count == 0 || buffer == [0u8; 2] {
                panic!("Should have gotten way data for way_id={}", way.id);
            }
            let num_nodes = Self::bytes2_to_int(&buffer);

            self.way_free_data
                .entry(num_nodes)
                .or_insert_with(|| Vec::new())
                .push(way_data_addr);

            self.way_data
                .seek(SeekFrom::Start(way_data_addr))
                .expect("Could not seek");
            let empty = vec![0; 2];
            self.way_data.write(&empty)?;

            let buffer = vec![0; WAY_PTR_SIZE];
            self.way_idx.seek(SeekFrom::Start(way_idx_addr))?;
            self.way_idx.write(&buffer)?;
        } else {
            self.write_way(way)?;
        }
        Ok(())
    }
    fn update_relation(&mut self, relation: &Relation, action: &Action) -> Result<(), io::Error> {
        if *action == Action::Delete() {
            let relid_digits = Self::to_digits(relation.id);
            let relid_part0 = Self::join_nums(&relid_digits[0..3]);
            let relid_part1 = Self::join_nums(&relid_digits[3..6]);
            let relid_part2 = Self::join_nums(&relid_digits[6..9]);
            let rel_path = Path::new(&self.dir)
                .join("relation")
                .join(relid_part0)
                .join(relid_part1)
                .join(relid_part2);
            match fs::remove_file(&rel_path) {
                Ok(o) => Ok(o),
                Err(error) => match error.kind() {
                    ErrorKind::NotFound => Ok(()),
                    _ => panic!(
                        "Couldn’t delete relation {} ({:?}): {error}",
                        relation.id, rel_path
                    ),
                },
            }
        } else {
            self.write_relation(relation)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    use crate::osm::Member;

    const PBF_SAINT_BARTHELEMY: &str = "tests/resources/saint_barthelemy.osm.pbf";

    #[test]
    fn read_node() {
        let tmpdir_path = tempfile::tempdir().unwrap();
        let tmpdir = tmpdir_path.path().to_str().unwrap();
        OsmBin::init(&tmpdir);
        let mut osmbin = OsmBin::new_writer(&tmpdir).unwrap();
        osmbin.import(PBF_SAINT_BARTHELEMY).unwrap();

        let node = osmbin.read_node(266053077);
        assert_eq!(
            Node {
                id: 266053077,
                decimicro_lat: (17.9031745 * 1e7) as i32,
                decimicro_lon: (-62.8363074 * 1e7) as i32,
                tags: None
            },
            node.unwrap()
        );

        let node = osmbin.read_node(2619283352);
        assert_eq!(
            Node {
                id: 2619283352,
                decimicro_lat: (17.9005419 * 1e7) as i32,
                decimicro_lon: (-62.8327042 * 1e7) as i32,
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
        let tmpdir_path = tempfile::tempdir().unwrap();
        let tmpdir = tmpdir_path.path().to_str().unwrap();
        OsmBin::init(&tmpdir);
        let mut osmbin = OsmBin::new_writer(&tmpdir).unwrap();
        osmbin.import(PBF_SAINT_BARTHELEMY).unwrap();

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
        let tmpdir_path = tempfile::tempdir().unwrap();
        let tmpdir = tmpdir_path.path().to_str().unwrap();
        OsmBin::init(&tmpdir);
        let mut osmbin = OsmBin::new_writer(&tmpdir).unwrap();
        osmbin.import(PBF_SAINT_BARTHELEMY).unwrap();

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
