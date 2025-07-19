//! Simplified OpenStreetMap database

use chrono;
use serde_json;
use std::borrow::Cow;
use std::cmp;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind};
use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::io::{BufReader, BufWriter};
use std::mem;
use std::path::Path;

use crate::bufreaderwriter;
use crate::osm::{Action, Node, Relation, Way};
use crate::osm::{OsmReader, OsmUpdate, OsmWriter};
use crate::osmcache::OsmCache;

const NODE_CRD: &str = "node.crd";
const WAY_IDX: &str = "way.idx";
const WAY_DATA: &str = "way.data";
const WAY_FREE: &str = "way.free";

/// Size of a node-id stored in `node.crd` or `way.data`
pub const NODE_ID_SIZE: usize = 5;
/// Size of a way pointer in `way.idx` to `way.data`
pub const WAY_PTR_SIZE: usize = 5;

/// Simplified OpenStreetMap database
///
/// Database used by `OsmBin` is stored in few files:
/// - `node.crd`: stores latitude/longitude of node, as 2*4 bytes. File is directly indexed by node
///   id. Not allocated nodes are not written to file, so its size is smaller than `max(node_id) *
///   8`, thanks to sparse files.
/// - `way.idx`: stores a pointer into `way.data`, as [`WAY_PTR_SIZE`] bytes. File is directly
///   indexed by way id.
/// - `way.data`: stores a list of nodes id, as `number of nodes` (2-bytes, as OSM limit is 2000),
///   followed by N node-id (each using [`NODE_ID_SIZE`] bytes). File is indexed by pointer given
///   by `way.idx`.
/// - `way.free`: stores pointer to `way.data` of free space, used to update or allocate a new way
///   without needing to allocate at the end of file. It is filled from ways that are deleted from
///   database
pub struct OsmBin {
    dir: String,
    node_crd: bufreaderwriter::BufReaderWriterRand<File>,
    way_idx: bufreaderwriter::BufReaderWriterRand<File>,
    way_data: bufreaderwriter::BufReaderWriterRand<File>,
    way_free_data: HashMap<u16, Vec<u64>>,

    node_crd_init_size: u64,
    way_idx_init_size: u64,
    way_data_size: u64,

    prev_node_id: u64,
    prev_way_id: u64,

    cache: OsmCache,

    stats: OsmBinStats,
}

#[allow(clippy::struct_field_names)]
#[derive(Default)]
struct OsmBinStats {
    num_nodes: u64,
    num_ways: u64,
    num_relations: u64,
    num_seek_node_crd: u64,
    num_seek_way_idx: u64,
    num_seek_way_data: u64,
    num_hit_nodes: u64,
    num_hit_ways: u64,
    num_hit_relations: u64,
}

enum OpenMode {
    Read,
    Write,
}

macro_rules! printlnt {
    ($($arg:tt)*) => {
        println!("{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), format_args!($($arg)*));
    };
}

impl OsmBin {
    /// Access an OsmBin database in read-only mode
    pub fn new(dir: &str) -> Result<OsmBin, io::Error> {
        Self::new_any(dir, &OpenMode::Read)
    }
    /// Access an OsmBin database in read-write mode
    pub fn new_writer(dir: &str) -> Result<OsmBin, io::Error> {
        Self::new_any(dir, &OpenMode::Write)
    }
    fn new_any(dir: &str, mode: &OpenMode) -> Result<OsmBin, io::Error> {
        let mut file_options = OpenOptions::new();
        file_options.read(true);
        if let OpenMode::Write = mode {
            file_options.write(true);
        }
        let node_crd = file_options.open(Path::new(dir).join(NODE_CRD))?;
        let node_crd_init_size = node_crd.metadata()?.len();
        let node_crd = bufreaderwriter::BufReaderWriterRand::new_reader(node_crd);
        let way_idx = file_options.open(Path::new(dir).join(WAY_IDX))?;
        let way_idx_init_size = way_idx.metadata()?.len();
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
                let mut s = line.split(';');
                let pos: u64 = s.next().unwrap().parse().unwrap();
                let num_nodes: u16 = s.next().unwrap().parse().unwrap();
                way_free_data.entry(num_nodes).or_default().push(pos);
            }
        }

        Ok(OsmBin {
            dir: dir.to_string(),
            node_crd,
            way_idx,
            way_data,
            way_free_data,
            node_crd_init_size,
            way_idx_init_size,
            way_data_size,
            prev_node_id: 0,
            prev_way_id: 0,
            cache: OsmCache::default(),
            stats: OsmBinStats {
                ..Default::default()
            },
        })
    }

    /// Initialize an OsmBin database with all required files
    pub fn init(dir: &str) {
        match fs::create_dir_all(dir) {
            Ok(()) => (),
            Err(error) => match error.kind() {
                ErrorKind::AlreadyExists => (),
                _ => panic!("Error with directory {dir}: {error}"),
            },
        };

        for filename in [NODE_CRD, WAY_IDX, WAY_DATA, WAY_FREE] {
            let full_filename = Path::new(dir).join(filename);
            let f = File::create_new(full_filename);
            match f {
                Ok(mut file) => {
                    if filename == WAY_DATA && file.write_all(b"--").is_err() {
                        panic!("Could not write to {filename}");
                    }
                }
                Err(error) => match error.kind() {
                    ErrorKind::AlreadyExists => (),
                    _ => panic!("Error with file {filename}: {error}"),
                },
            };
        }
        match fs::create_dir_all(Path::new(dir).join("relation")) {
            Ok(()) => (),
            Err(error) => match error.kind() {
                ErrorKind::AlreadyExists => (),
                _ => panic!("Error with directory {dir}: {error}"),
            },
        };
    }

    pub fn print_stats(&mut self) {
        self.stats.print_stats();
    }

    fn bytes5_to_int(d: [u8; 5]) -> u64 {
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

    fn bytes4_to_int(d: [u8; 4]) -> u32 {
        u32::from_be_bytes(d)
    }
    fn int_to_bytes4(d: u32) -> [u8; 4] {
        d.to_be_bytes()
    }

    #[allow(clippy::cast_possible_truncation)]
    fn bytes4_to_coord(d: [u8; 4]) -> i32 {
        // TODO: Store directly i32 instead of converting to a positive number
        (i64::from(Self::bytes4_to_int(d)) - 1_800_000_000) as i32
    }
    fn coord_to_bytes4(d: i32) -> [u8; 4] {
        // TODO: Store directly i32 instead of converting to a positive number
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        Self::int_to_bytes4((i64::from(d) + 1_800_000_000) as u32)
    }

    fn bytes2_to_int(d: [u8; 2]) -> u16 {
        u16::from_be_bytes(d)
    }
    fn int_to_bytes2(d: u16) -> [u8; 2] {
        d.to_be_bytes()
    }

    fn to_digits(v: u64) -> Vec<u8> {
        let mut v = v;
        let mut digits: Vec<u8> = Vec::with_capacity(10);
        while v > 0 {
            let n = (v % 10) as u8;
            v /= 10;
            digits.push(n);
        }
        if digits.len() < 9 {
            digits.resize(9, 0);
        }
        digits.reverse();
        digits
    }

    fn join_nums(nums: &[u8]) -> String {
        let str_nums: Vec<String> = nums.iter().map(std::string::ToString::to_string).collect();
        str_nums.join("")
    }

    pub fn get_cache(&mut self) -> OsmCache {
        mem::take(&mut self.cache)
    }

    fn check_node(&mut self, id: u64) -> Result<(), ElementNotFound> {
        if self.read_node(id).is_none() {
            return Err(ElementNotFound {
                type_: String::from("node"),
                id,
                inner: None,
            });
        }
        Ok(())
    }
    fn check_way(&mut self, id: u64) -> Result<(), ElementNotFound> {
        if self.cache.ways.contains_key(&id) {
            return Ok(());
        }
        let way = self.read_way(id);
        if let Some(way) = way {
            for n in &way.nodes {
                self.check_node(*n).map_err(|e| ElementNotFound {
                    type_: String::from("way"),
                    id,
                    inner: Some(Box::new(e)),
                })?;
            }
            Ok(())
        } else {
            Err(ElementNotFound {
                type_: String::from("way"),
                id,
                inner: None,
            })
        }
    }
    fn check_relation(&mut self, id: u64, prev_relations: &[u64]) -> Result<(), ElementNotFound> {
        if self.cache.relations.contains_key(&id) {
            return Ok(());
        }
        if prev_relations.contains(&id) {
            println!("Detected relation recursion on id={id} - {prev_relations:?}",);
            return Ok(());
        }
        let relation = self.read_relation(id);
        if let Some(relation) = relation {
            for m in &relation.members {
                match m.type_.as_str() {
                    "node" => self.check_node(m.ref_).map_err(|e| ElementNotFound {
                        type_: String::from("relation"),
                        id,
                        inner: Some(Box::new(e)),
                    })?,
                    "way" => self.check_way(m.ref_).map_err(|e| ElementNotFound {
                        type_: String::from("relation"),
                        id,
                        inner: Some(Box::new(e)),
                    })?,
                    "relation" => {
                        let mut prev_relations = prev_relations.to_owned();
                        prev_relations.push(id);
                        self.check_relation(m.ref_, &prev_relations).map_err(|e| {
                            ElementNotFound {
                                type_: String::from("relation"),
                                id,
                                inner: Some(Box::new(e)),
                            }
                        })?;
                    }
                    t => panic!("{t} not expected"),
                };
            }
            Ok(())
        } else {
            Err(ElementNotFound {
                type_: String::from("relation"),
                id,
                inner: None,
            })
        }
    }
    pub fn check_database(&mut self, start: u64) -> Result<(), Box<dyn Error>> {
        let s0: Cow<str> = format!("{:03}", start / 1_000_000).into();
        let s1: Cow<str> = format!("{:03}", start / 1_000).into();

        let relation_dir = Path::new(&self.dir).join("relation");
        let mut dirs = fs::read_dir(relation_dir)?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()?;
        dirs.sort();
        for dir in dirs {
            let part0 = dir.file_name().expect("Incorrect string").to_string_lossy();
            if part0 < s0 {
                continue;
            }
            let mut dirs = fs::read_dir(dir.as_path())?
                .map(|res| res.map(|e| e.path()))
                .collect::<Result<Vec<_>, io::Error>>()?;
            dirs.sort();
            for dir in dirs {
                let part1 = dir.file_name().expect("Incorrect string").to_string_lossy();
                if part1 < s1 {
                    continue;
                }
                printlnt!("{part0}{part1}");
                for f in fs::read_dir(dir.as_path())? {
                    let filename = f?.file_name();
                    let part2 = filename.to_string_lossy();
                    let id_str = format!("{part0}{part1}{part2}");
                    let id: u64 = id_str.parse()?;
                    self.check_relation(id, &[])?;
                }
            }
        }
        Ok(())
    }
}

impl OsmBinStats {
    pub fn print_stats(&mut self) {
        println!(
            "nodes:     {} ({} seeks) ({} hits)",
            self.num_nodes, self.num_seek_node_crd, self.num_hit_nodes,
        );
        println!(
            "ways:      {} ({} + {} seeks) ({} hits)",
            self.num_ways, self.num_seek_way_idx, self.num_seek_way_data, self.num_hit_ways,
        );
        println!(
            "relations: {} ({} hits)",
            self.num_relations, self.num_hit_relations
        );
    }
}

impl Drop for OsmBin {
    fn drop(&mut self) {
        let way_free = File::create(Path::new(&self.dir).join(WAY_FREE)).unwrap();
        let mut way_free = BufWriter::new(way_free);

        for (num_nodes, v) in &self.way_free_data {
            for pos in v {
                writeln!(way_free, "{pos};{num_nodes}").unwrap();
            }
        }
    }
}

impl OsmReader for OsmBin {
    fn read_node(&mut self, id: u64) -> Option<Node> {
        self.stats.num_nodes += 1;

        if self.cache.nodes.contains_key(&id) {
            self.stats.num_hit_nodes += 1;
            return self.cache.read_node(id);
        }

        let node_crd_addr = id * 8;

        let cur_position = self.node_crd.stream_position().unwrap();
        if cur_position != node_crd_addr {
            let diff: i64 =
                i64::try_from(node_crd_addr).unwrap() - i64::try_from(cur_position).unwrap();
            if diff > 0 && diff < 4096 {
                let mut vec: Vec<u8> = vec![0; usize::try_from(diff).unwrap()];
                if self.node_crd.read_exact(&mut vec).is_err() {
                    self.node_crd.seek_relative(diff).unwrap();
                    self.stats.num_seek_node_crd += 1;
                }
            } else {
                self.node_crd.seek_relative(diff).unwrap();
                self.stats.num_seek_node_crd += 1;
            }
        }
        let mut lat_buffer = [0u8; 4];
        let mut lon_buffer = [0u8; 4];
        self.node_crd.read_exact_allow_eof(&mut lat_buffer).unwrap();
        self.node_crd.read_exact_allow_eof(&mut lon_buffer).unwrap();

        if lat_buffer == [0u8; 4] && lon_buffer == [0u8; 4] {
            self.cache.nodes.insert(id, None);
            return None;
        }
        let decimicro_lat = Self::bytes4_to_coord(lat_buffer);
        let decimicro_lon = Self::bytes4_to_coord(lon_buffer);

        self.cache
            .nodes
            .insert(id, Some((decimicro_lat, decimicro_lon)));

        Some(Node {
            id,
            decimicro_lat,
            decimicro_lon,
            tags: None,
            ..Default::default()
        })
    }
    fn read_way(&mut self, id: u64) -> Option<Way> {
        self.stats.num_ways += 1;

        if self.cache.ways.contains_key(&id) {
            self.stats.num_hit_ways += 1;
            return self.cache.read_way(id);
        }

        let way_idx_addr = id * (WAY_PTR_SIZE as u64);

        let cur_position = self.way_idx.stream_position().unwrap();
        if cur_position != way_idx_addr {
            let diff: i64 =
                i64::try_from(way_idx_addr).unwrap() - i64::try_from(cur_position).unwrap();
            self.way_idx.seek_relative(diff).unwrap();
            self.stats.num_seek_way_idx += 1;
        }
        let mut buffer = [0u8; WAY_PTR_SIZE];
        self.way_idx.read_exact_allow_eof(&mut buffer).unwrap();

        if buffer == [0u8; WAY_PTR_SIZE] {
            self.cache.ways.insert(id, None);
            return None;
        }
        let way_data_addr = Self::bytes5_to_int(buffer);

        let cur_position = self.way_data.stream_position().unwrap();
        if cur_position != way_data_addr {
            let diff: i64 =
                i64::try_from(way_data_addr).unwrap() - i64::try_from(cur_position).unwrap();
            self.way_data.seek_relative(diff).unwrap();
            self.stats.num_seek_way_data += 1;
        }
        let mut buffer = [0u8; 2];
        self.way_data.read_exact(&mut buffer).unwrap();
        if buffer == [0u8; 2] {
            panic!("Should have gotten way num_nodes for way_id={id}");
        }
        let num_nodes = Self::bytes2_to_int(buffer);

        let mut buffer = [0u8; NODE_ID_SIZE];

        let mut nodes: Vec<u64> = Vec::new();
        for _ in 0..num_nodes {
            self.way_data.read_exact(&mut buffer).unwrap();
            if buffer == [0u8; NODE_ID_SIZE] {
                panic!("Should have gotten way node id for way_id={id}");
            }
            nodes.push(Self::bytes5_to_int(buffer));
        }

        self.cache.ways.insert(id, Some(nodes.clone()));

        Some(Way {
            id,
            nodes,
            tags: None,
            ..Default::default()
        })
    }
    fn read_relation(&mut self, id: u64) -> Option<Relation> {
        self.stats.num_relations += 1;

        if self.cache.relations.contains_key(&id) {
            self.stats.num_hit_relations += 1;
            return self.cache.read_relation(id);
        }

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
                ErrorKind::NotFound => {
                    self.cache.relations.insert(id, None);
                    return None;
                }
                _ => panic!("Error with file {rel_path:?}: {error}"),
            },
        };
        let u: Relation = serde_json::from_str(rel_data.as_str()).unwrap();

        self.cache.relations.insert(id, Some(u.clone()));

        Some(u)
    }
}

impl OsmWriter for OsmBin {
    fn write_node(&mut self, node: &mut Node) -> Result<(), io::Error> {
        debug_assert!(node.id >= self.prev_node_id);
        self.prev_node_id = node.id;

        let lat = Self::coord_to_bytes4(node.decimicro_lat);
        let lon = Self::coord_to_bytes4(node.decimicro_lon);
        let node_crd_addr = node.id * 8;

        // Try not to seek if not necessary, as seeking flushes write buffer
        let cur_position = self.node_crd.stream_position().unwrap();
        if cur_position != node_crd_addr {
            let diff: i64 =
                i64::try_from(node_crd_addr).unwrap() - i64::try_from(cur_position).unwrap();
            if self.node_crd_init_size < cur_position
                && self.node_crd_init_size < node_crd_addr
                && diff > 0
                && diff < 4096
            {
                let vec: Vec<u8> = vec![0; usize::try_from(diff).unwrap()];
                self.node_crd.write_all(&vec).unwrap();
            } else {
                self.node_crd.seek(SeekFrom::Start(node_crd_addr)).unwrap();
                self.stats.num_seek_node_crd += 1;
            }
            debug_assert_eq!(self.node_crd.stream_position().unwrap(), node_crd_addr);
        }
        self.node_crd.write_all(&lat).unwrap();
        self.node_crd.write_all(&lon).unwrap();

        self.stats.num_nodes += 1;

        Ok(())
    }
    fn write_way(&mut self, way: &mut Way) -> Result<(), io::Error> {
        debug_assert!(way.id >= self.prev_way_id);
        self.prev_way_id = way.id;

        let way_idx_addr = way.id * (WAY_PTR_SIZE as u64);

        // Only need to delete way if it could be inside file
        if way_idx_addr < self.way_idx_init_size {
            self.update_way(way, &Action::Delete())?;
        }
        #[allow(clippy::cast_possible_truncation)]
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
            self.stats.num_seek_way_data += 1;
        }
        let num_nodes = Self::int_to_bytes2(num_nodes);
        self.way_data.write_all(&num_nodes).unwrap();
        for n in &way.nodes {
            let node = Self::int_to_bytes5(*n);
            self.way_data.write_all(&node).unwrap();
        }

        // Try not to seek if not necessary, as seeking flushes write buffer
        let cur_position = self.way_idx.stream_position().unwrap();
        if cur_position != way_idx_addr {
            let diff: i64 =
                i64::try_from(way_idx_addr).unwrap() - i64::try_from(cur_position).unwrap();
            if self.way_idx_init_size < cur_position
                && self.way_idx_init_size < way_idx_addr
                && diff > 0
                && diff < 4096
            {
                let vec: Vec<u8> = vec![0; usize::try_from(diff).unwrap()];
                self.way_idx.write_all(&vec).unwrap();
            } else {
                self.way_idx.seek(SeekFrom::Start(way_idx_addr)).unwrap();
                self.stats.num_seek_way_idx += 1;
            }
            debug_assert_eq!(self.way_idx.stream_position().unwrap(), way_idx_addr);
        }
        let buffer = Self::int_to_bytes5(way_data_addr);
        self.way_idx.write_all(&buffer).unwrap();

        self.way_data_size = cmp::max(self.way_data_size, self.way_data.stream_position().unwrap());
        self.stats.num_ways += 1;

        Ok(())
    }
    fn write_relation(&mut self, relation: &mut Relation) -> Result<(), io::Error> {
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
            Ok(()) => (),
            Err(error) => match error.kind() {
                ErrorKind::AlreadyExists => (),
                _ => panic!("Error with directory: {error}"),
            },
        };

        let json_data = serde_json::to_string(relation)?;
        fs::write(&rel_path, json_data)?;

        self.stats.num_relations += 1;

        Ok(())
    }
    fn write_end(&mut self, _change: bool) -> Result<(), Box<dyn Error>> {
        println!("Osmbin import finished");
        self.stats.print_stats();
        Ok(())
    }
}

impl OsmUpdate for OsmBin {
    fn update_node(&mut self, node: &mut Node, action: &Action) -> Result<(), io::Error> {
        if *action == Action::Delete() {
            let empty: Vec<u8> = vec![0; 8];
            self.node_crd.seek(SeekFrom::Start(node.id * 8))?;
            self.node_crd.write_all(&empty).unwrap();
        } else {
            self.write_node(node)?;
        }

        Ok(())
    }
    fn update_way(&mut self, way: &mut Way, action: &Action) -> Result<(), io::Error> {
        if *action == Action::Delete() {
            let way_idx_addr = way.id * (WAY_PTR_SIZE as u64);
            self.way_idx.seek(SeekFrom::Start(way_idx_addr))?;
            let mut buffer = [0u8; WAY_PTR_SIZE];
            self.way_idx.read_exact_allow_eof(&mut buffer).unwrap();

            if buffer == [0u8; WAY_PTR_SIZE] {
                return Ok(());
            }
            let way_data_addr = Self::bytes5_to_int(buffer);

            self.way_data
                .seek(SeekFrom::Start(way_data_addr))
                .expect("Could not seek");
            let mut buffer = [0u8; 2];
            self.way_data.read_exact(&mut buffer).unwrap();
            if buffer == [0u8; 2] {
                panic!("Should have gotten way num_nodes for way_id={}", way.id);
            }
            let num_nodes = Self::bytes2_to_int(buffer);

            self.way_free_data
                .entry(num_nodes)
                .or_default()
                .push(way_data_addr);

            self.way_data
                .seek(SeekFrom::Start(way_data_addr))
                .expect("Could not seek");
            let empty = vec![0; 2];
            self.way_data.write_all(&empty).unwrap();

            let buffer = vec![0; WAY_PTR_SIZE];
            self.way_idx.seek(SeekFrom::Start(way_idx_addr))?;
            self.way_idx.write_all(&buffer).unwrap();
        } else {
            self.write_way(way)?;
        }
        Ok(())
    }
    fn update_relation(
        &mut self,
        relation: &mut Relation,
        action: &Action,
    ) -> Result<(), io::Error> {
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

#[derive(Debug)]
pub struct ElementNotFound {
    type_: String,
    id: u64,
    inner: Option<Box<ElementNotFound>>,
}
impl ElementNotFound {
    fn fmt_inner(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        write!(f, "\n{}{} {}", " ".repeat(indent), &self.type_, &self.id)?;
        if let Some(inner) = &self.inner {
            inner.fmt_inner(f, indent + 2)?;
        }
        Ok(())
    }
}
impl Error for ElementNotFound {}
impl fmt::Display for ElementNotFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Element was not found")?;
        self.fmt_inner(f, 2)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    use crate::osm::Member;

    const PBF_SAINT_BARTHELEMY: &str = "tests/resources/saint_barthelemy.osm.pbf";
    const OSM_WAY_666412102: &str = "tests/resources/way-666412102.osm.gz";
    const OSM_RELATION_17170852: &str = "tests/resources/relation-17170852.osm.gz";
    const OSM_BOUNDARY_UPDATE: &str = "tests/resources/saint_barthelemy-boundary.osc.gz";

    #[macro_export]
    macro_rules! assert_approx_eq {
        ($a:expr, $b:expr) => {{
            let eps = 1.0e-13;
            let (a, b) = (&$a, &$b);
            assert!(
                (*a - *b).abs() < eps,
                "assertion failed: `(left !== right)` \
                 (left: `{:?}`, right: `{:?}`, expect diff: `{:?}`, real diff: `{:?}`)",
                *a,
                *b,
                eps,
                (*a - *b).abs()
            );
        }};
        ($a:expr, $b:expr, $eps:expr) => {{
            let (a, b) = (&$a, &$b);
            let eps = $eps;
            assert!(
                (*a - *b).abs() < eps,
                "assertion failed: `(left !== right)` \
                 (left: `{:?}`, right: `{:?}`, expect diff: `{:?}`, real diff: `{:?}`)",
                *a,
                *b,
                eps,
                (*a - *b).abs()
            );
        }};
    }

    #[test]
    fn read_node() {
        let tmpdir_path = tempfile::tempdir().unwrap();
        let tmpdir = tmpdir_path.path().to_str().unwrap();
        OsmBin::init(&tmpdir);
        let mut osmbin = OsmBin::new_writer(&tmpdir).unwrap();
        osmbin.import(PBF_SAINT_BARTHELEMY).unwrap();
        drop(osmbin);
        let mut osmbin = OsmBin::new_writer(&tmpdir).unwrap();
        osmbin.update(OSM_WAY_666412102).unwrap();

        for i in 0..5 {
            // read several times to check cache
            if i == 3 {
                drop(osmbin);
                osmbin = OsmBin::new_writer(&tmpdir).unwrap();
            }

            let node = osmbin.read_node(266053077);
            assert_eq!(
                Node {
                    id: 266053077,
                    decimicro_lat: (17.9031745 * 1e7) as i32,
                    decimicro_lon: (-62.8363074 * 1e7) as i32,
                    tags: None,
                    ..Default::default()
                },
                node.unwrap()
            );

            let node = osmbin.read_node(2619283352);
            assert_eq!(
                Node {
                    id: 2619283352,
                    decimicro_lat: (17.9005419 * 1e7) as i32,
                    decimicro_lon: (-62.8327042 * 1e7) as i32,
                    tags: None,
                    ..Default::default()
                },
                node.unwrap()
            );

            let node = osmbin.read_node(1);
            assert_eq!(true, node.is_none());

            let node = osmbin.read_node(266053076);
            assert_eq!(true, node.is_none());

            let node = osmbin.read_node(2619283353);
            assert_eq!(true, node.is_none());

            let node = osmbin.read_node(120470298).unwrap();
            assert_approx_eq!(-47.9975933, node.lat());
            assert_approx_eq!(-74.2525578, node.lon());

            let node = osmbin.read_node(6239222548).unwrap();
            assert_approx_eq!(-48.0692340, node.lat());
            assert_approx_eq!(-74.2305121, node.lon());

            let node = osmbin.read_node(6239224513).unwrap();
            assert_approx_eq!(-48.0231575, node.lat());
            assert_approx_eq!(-74.2551240, node.lon());
        }
    }

    #[test]
    fn read_way() {
        let tmpdir_path = tempfile::tempdir().unwrap();
        let tmpdir = tmpdir_path.path().to_str().unwrap();
        OsmBin::init(&tmpdir);
        let mut osmbin = OsmBin::new_writer(&tmpdir).unwrap();
        osmbin.import(PBF_SAINT_BARTHELEMY).unwrap();
        drop(osmbin);
        let mut osmbin = OsmBin::new_writer(&tmpdir).unwrap();
        osmbin.update(OSM_RELATION_17170852).unwrap();

        for i in 0..5 {
            // read several times to check cache
            if i == 3 {
                drop(osmbin);
                osmbin = OsmBin::new_writer(&tmpdir).unwrap();
            }
            let way = osmbin.read_way(24473155);
            assert_eq!(true, way.is_some());
            assert_eq!(1665, way.unwrap().nodes.len());

            let way = osmbin.read_way(255316725);
            assert_eq!(
                Way {
                    id: 255316725,
                    nodes: vec![
                        2610107905, 2610107903, 2610107901, 2610107902, 2610107904, 2610107905
                    ],
                    tags: None,
                    ..Default::default()
                },
                way.unwrap()
            );

            let way = osmbin.read_way(1);
            assert_eq!(true, way.is_none());

            let way = osmbin.read_way(24473154);
            assert_eq!(true, way.is_none());

            let way = osmbin.read_way(255316726);
            assert_eq!(true, way.is_none());

            osmbin.read_way(13059911).unwrap();
            osmbin.read_way(666414264).unwrap();
            osmbin.read_way(666412101).unwrap();

            let way = osmbin.read_way(666412102);
            assert_eq!(true, way.is_some());
            let way = way.unwrap();
            assert_eq!(1060, way.nodes.len());
            assert_eq!(
                vec![
                    120470298, 6239222783, 6239222782, 6239222781, 6239222780, 6239222779,
                    6239222778, 6239222777, 6239222776, 6239222775, 6239222774, 6239222773,
                    6239222772, 6239222771, 6239222770, 6239222769, 6239222768, 6239222767,
                    6239222766, 6239222765, 6239222764, 6239222763, 6239222762, 6239222761,
                    6239222760, 6239222759, 6239222758, 6239222757, 6239222756, 6239222755,
                    6239222754, 6239222753, 6239222752, 6239222751, 6239222750, 6239222749,
                    6239222748, 6239222747, 6239222746, 6239222745, 6239222744, 6239222743,
                    6239222742, 6239222741, 6239222740, 6239222739, 6239222738, 6239222737,
                    6239222736, 6239222735, 6239222734, 6239222733, 6239222732, 6239222731,
                    6239222730, 6239222729, 6239222728, 6239222727, 6239222726, 6239222725,
                    6239222724, 6239222723, 6239222722, 6239222721, 6239222720, 6239222719,
                    6239222718, 6239222717, 6239222716, 6239222715, 6239222714, 6239222713,
                    6239222712, 6239222711, 6239222710, 6239222709, 6239222708, 6239222707,
                    6239222706, 6239222705, 6239222704, 6239222703, 6239222702, 6239222701,
                    6239222700, 6239222699, 6239222698, 6239222697, 6239222696, 6239222695,
                    6239222694, 6239222693, 6239222692, 6239222691, 6239222690, 6239222689,
                    6239222688, 6239224710, 6239224709, 6239224708, 6239224707, 6239224706,
                    6239224705, 6239224704, 6239224703, 6239224702, 6239224701, 6239224700,
                    6239224699, 6239224698, 6239224697, 6239224696, 6239224695, 6239224694,
                    6239224693, 6239224692, 6239224691, 6239224690, 6239224689, 6239224688,
                    6239224687, 6239224686, 6239224685, 6239224684, 6239224683, 6239224682,
                    6239224681, 6239224680, 6239224679, 6239224678, 6239224677, 6239224676,
                    6239224675, 6239224674, 6239224673, 6239224672, 6239224671, 6239224670,
                    6239224669, 6239224668, 6239224667, 6239224666, 6239224665, 6239224664,
                    6239224663, 6239224662, 6239224661, 6239224660, 6239224659, 6239224658,
                    6239224657, 6239224656, 6239224655, 6239224654, 6239224653, 6239224652,
                    6239224651, 6239224650, 6239224649, 6239224648, 6239224647, 6239224646,
                    6239224645, 6239224644, 6239224643, 6239224642, 6239224641, 6239224640,
                    6239224639, 6239224638, 6239224637, 6239224636, 6239224635, 6239224634,
                    6239224633, 6239224632, 6239224631, 6239224630, 6239224629, 6239224628,
                    6239224627, 6239224626, 6239224625, 6239224624, 6239224623, 6239224622,
                    6239224621, 6239224620, 6239224619, 6239224618, 6239224617, 6239224616,
                    6239224615, 6239224614, 6239224613, 6239224612, 6239224611, 6239224610,
                    6239224609, 6239224608, 6239224607, 6239224606, 6239224605, 6239224604,
                    6239224603, 6239224602, 6239224601, 6239224600, 6239224599, 6239224598,
                    6239224597, 6239224596, 6239224595, 6239224594, 6239224593, 6239224592,
                    6239224591, 6239224590, 6239224589, 6239224588, 6239224587, 6239224586,
                    6239224585, 6239224584, 6239224583, 6239224582, 6239224581, 6239224580,
                    6239224579, 6239224578, 6239224577, 6239224576, 6239224575, 6239224574,
                    6239224573, 6239224572, 6239224571, 6239224570, 6239224569, 6239224568,
                    6239224567, 6239224566, 6239224565, 6239224564, 6239224563, 6239224562,
                    6239224561, 6239224560, 6239224559, 6239224558, 6239224557, 6239224556,
                    6239224555, 6239224554, 6239224553, 6239224552, 6239224551, 6239224550,
                    6239224549, 6239224548, 6239224547, 6239224546, 6239224545, 6239224544,
                    6239224543, 6239224542, 6239224541, 6239224540, 6239224539, 6239224538,
                    6239224537, 6239224536, 6239224535, 6239224534, 6239224533, 6239224532,
                    6239224531, 6239224530, 6239224529, 6239224528, 6239224527, 6239224526,
                    6239224525, 6239224524, 6239224523, 6239224522, 6239224521, 6239224520,
                    6239224519, 6239224518, 6239224517, 6239224516, 6239224515, 6239224514,
                    6239224513, 6239224512, 6239224511, 6239224510, 6239224509, 6239224508,
                    6239224507, 6239224506, 6239224505, 6239224504, 6239224503, 6239224502,
                    6239224501, 6239224500, 6239224499, 6239224498, 6239224497, 6239224496,
                    6239224495, 6239224494, 6239224493, 6239224492, 6239224491, 6239224490,
                    6239224489, 6239224488, 6239224487, 6239224486, 6239224485, 6239224484,
                    6239224483, 6239224482, 6239224481, 6239224480, 6239224479, 6239224478,
                    6239224477, 6239224476, 6239224475, 6239224474, 6239224473, 6239224472,
                    6239224471, 6239224470, 6239224469, 6239224468, 6239224467, 6239224466,
                    6239224465, 6239224464, 6239224463, 6239224462, 6239224461, 6239224460,
                    6239224459, 6239224458, 6239224457, 6239224456, 6239224455, 6239224454,
                    6239224453, 6239224452, 6239224451, 6239224450, 6239224449, 6239224448,
                    6239224447, 6239224446, 6239224445, 6239224444, 6239224443, 6239224442,
                    6239224441, 6239224440, 6239224439, 6239224438, 6239224437, 6239224436,
                    6239224435, 6239224434, 6239224433, 6239224432, 6239224431, 6239224430,
                    6239224429, 6239224428, 6239224427, 6239224426, 6239224425, 6239224424,
                    6239224423, 6239224422, 6239224421, 6239224420, 6239224419, 6239224418,
                    6239224417, 6239224416, 6239224415, 6239224414, 6239224413, 6239224412,
                    6239224411, 6239224410, 6239224409, 6239224408, 6239224407, 6239224406,
                    6239224405, 6239224404, 6239224403, 6239224402, 6239224401, 6239224400,
                    6239224399, 6239224398, 6239224397, 6239224396, 6239224395, 6239224394,
                    6239224393, 6239224392, 6239224391, 6239224390, 6239224389, 6239224388,
                    6239224387, 6239224386, 6239224385, 6239224384, 6239224383, 6239224382,
                    6239224381, 6239224380, 6239224379, 6239224378, 6239224377, 6239224376,
                    6239224375, 6239224374, 6239224373, 6239224372, 6239224371, 6239224370,
                    6239224369, 6239224368, 6239224367, 6239224366, 6239224365, 6239224364,
                    6239224363, 6239224362, 6239224361, 6239224360, 6239224359, 6239224358,
                    6239224357, 6239224356, 6239224355, 6239224354, 6239224353, 6239224352,
                    6239224351, 6239224350, 6239224349, 6239224348, 6239224347, 6239224346,
                    6239224345, 6239224344, 6239224343, 6239224342, 6239224341, 6239224340,
                    6239224339, 6239224338, 6239224337, 6239224336, 6239224335, 6239224334,
                    6239224333, 6239224332, 6239224331, 6239224330, 6239224329, 6239224328,
                    6239224327, 6239224326, 6239224325, 6239224324, 6239224323, 6239224322,
                    6239224321, 6239224320, 6239224319, 6239224318, 6239224317, 6239224316,
                    6239224315, 6239224314, 6239224313, 6239224312, 6239224311, 6239224310,
                    6239224309, 6239224308, 6239224307, 6239224306, 6239224305, 6239224304,
                    6239224303, 6239224302, 6239224301, 6239224300, 6239224299, 6239224298,
                    6239224297, 6239224296, 6239224295, 6239224294, 6239224293, 6239224292,
                    6239224291, 6239224290, 6239224289, 6239224288, 6239224287, 6239224286,
                    6239224285, 6239224284, 6239224283, 6239224282, 6239224281, 6239224280,
                    6239224279, 6239224278, 6239224277, 6239224276, 6239224275, 6239224274,
                    6239224273, 6239224272, 6239224271, 6239224270, 6239224269, 6239224268,
                    6239224267, 6239224266, 6239224265, 6239224264, 6239224263, 6239224262,
                    6239224261, 6239224260, 6239224259, 6239224258, 6239224257, 6239224256,
                    6239224255, 6239224254, 6239224253, 6239224252, 6239224251, 6239224250,
                    6239224249, 6239224248, 6239224247, 6239224246, 6239224245, 6239224244,
                    6239224243, 6239224242, 6239224241, 6239224240, 6239224239, 6239224238,
                    6239224237, 6239224236, 6239224235, 6239224234, 6239224233, 6239224232,
                    6239224231, 6239224230, 6239224229, 6239224228, 6239224227, 6239224226,
                    6239224225, 6239224224, 6239224223, 6239224222, 6239224221, 6239224220,
                    6239224219, 6239224218, 6239224217, 6239224216, 6239224215, 6239224214,
                    6239224213, 6239224212, 6239224211, 6239224210, 6239224209, 6239224208,
                    6239224207, 6239224206, 6239224205, 6239224204, 6239224203, 6239224202,
                    6239224201, 6239224200, 6239224199, 6239224198, 6239224197, 6239224196,
                    6239224195, 6239224194, 6239224193, 6239224192, 6239224191, 6239224190,
                    6239224189, 6239224188, 6239224187, 6239224186, 6239224185, 6239224184,
                    6239224183, 6239224182, 6239224181, 6239224180, 6239224179, 6239224178,
                    6239224177, 6239224176, 6239224175, 6239224174, 6239224173, 6239224172,
                    6239224171, 6239224170, 6239224169, 6239224168, 6239224167, 6239224166,
                    6239224165, 6239224164, 6239224163, 6239224162, 6239224161, 6239224160,
                    6239224159, 6239224158, 6239224157, 6239224156, 6239224155, 6239224154,
                    6239224153, 6239224152, 6239224151, 6239224150, 6239224149, 6239224148,
                    6239224147, 6239224146, 6239224145, 6239224144, 6239224143, 6239224142,
                    6239224141, 6239224140, 6239224139, 6239224138, 6239224137, 6239224136,
                    6239224135, 6239224134, 6239224133, 6239224132, 6239224131, 6239224130,
                    6239224129, 6239224128, 6239224127, 6239224126, 6239224125, 6239224124,
                    6239224123, 6239224122, 6239224121, 6239224120, 6239224119, 6239224118,
                    6239224117, 6239224116, 6239224115, 6239224114, 6239224113, 6239224112,
                    6239224111, 6239224110, 6239224109, 6239224108, 6239224107, 6239224106,
                    6239224105, 6239224104, 6239224103, 6239224102, 6239224101, 6239224100,
                    6239224099, 6239224098, 6239224097, 6239224096, 6239224095, 6239224094,
                    6239224093, 6239224092, 6239224091, 6239224090, 6239224089, 6239224088,
                    6239224087, 6239224086, 6239224085, 6239224084, 6239224083, 6239224082,
                    6239224081, 6239224080, 6239224079, 6239224078, 6239224077, 6239224076,
                    6239224075, 6239224074, 6239224073, 6239224072, 6239224071, 6239224070,
                    6239224069, 6239224068, 6239224067, 6239224066, 6239224065, 6239224064,
                    6239224063, 6239224062, 6239224061, 6239224060, 6239224059, 6239224058,
                    6239224057, 6239224056, 6239224055, 6239224054, 6239224053, 6239224052,
                    6239224051, 6239224050, 6239224049, 6239224048, 6239224047, 6239224046,
                    6239224045, 6239224044, 6239224043, 6239224042, 6239224041, 6239224040,
                    6239224039, 6239224038, 6239224037, 6239224036, 6239224035, 6239224034,
                    6239224033, 6239224032, 6239224031, 6239224030, 6239224029, 6239224028,
                    6239224027, 6239224026, 6239224025, 6239224024, 6239224023, 6239224022,
                    6239224021, 6239224020, 6239224019, 6239224018, 6239224017, 6239224016,
                    6239224015, 6239224014, 6239224013, 6239224012, 6239224011, 6239224010,
                    6239224009, 6239224008, 6239224007, 6239224006, 6239224005, 6239224004,
                    6239224003, 6239224002, 6239224001, 6239224000, 6239223999, 6239223998,
                    6239223997, 6239223996, 6239223995, 6239223994, 6239223993, 6239223992,
                    6239223991, 6239223990, 6239223989, 6239223988, 6239223987, 6239223986,
                    6239223985, 6239223984, 6239223983, 6239223982, 6239223981, 6239223980,
                    6239223979, 6239223978, 6239223977, 6239223976, 6239223975, 6239223974,
                    6239223973, 6239223972, 6239223971, 6239223970, 6239223969, 6239223968,
                    6239223967, 6239223966, 6239223965, 6239223964, 6239223963, 6239223962,
                    6239223961, 6239223960, 6239223959, 6239223958, 6239223957, 6239223956,
                    6239223955, 6239223954, 6239223953, 6239223952, 6239223951, 6239223950,
                    6239223949, 6239223948, 6239223947, 6239223946, 6239223945, 6239223944,
                    6239223943, 6239223942, 6239223941, 6239223940, 6239223939, 6239223938,
                    6239223937, 6239223936, 6239223935, 6239223934, 6239223933, 6239223932,
                    6239223931, 6239223930, 6239223929, 6239223928, 6239223927, 6239223926,
                    6239223925, 6239223924, 6239223923, 6239223922, 6239223921, 6239223920,
                    6239223919, 6239223918, 6239223917, 6239223916, 6239223915, 6239223914,
                    6239223913, 6239223912, 6239223911, 6239223910, 6239223909, 6239223908,
                    6239223907, 6239223906, 6239223905, 6239223904, 6239223903, 6239223902,
                    6239223901, 6239223900, 6239223899, 6239223898, 6239223897, 6239223896,
                    6239223895, 6239223894, 6239223893, 6239223892, 6239223891, 6239223890,
                    6239223889, 6239223888, 6239223887, 6239223886, 6239223885, 6239223884,
                    6239223883, 6239223882, 6239223881, 6239223880, 6239223879, 6239223878,
                    6239223877, 6239223876, 6239223875, 6239223874, 6239223873, 6239223872,
                    6239223871, 6239223870, 6239223869, 6239223868, 6239223867, 6239223866,
                    6239223865, 6239223864, 6239223863, 6239223862, 6239223861, 6239223860,
                    6239223859, 6239223858, 6239223857, 6239223856, 6239223855, 6239223854,
                    6239223853, 6239223852, 6239223851, 6239223850, 6239223849, 6239223848,
                    6239223847, 6239223846, 6239223845, 6239223844, 6239223843, 6239223842,
                    6239223841, 6239223840, 6239223839, 6239223838, 6239223837, 6239223836,
                    6239223835, 6239223834, 6239223833, 6239223832, 6239223831, 6239223830,
                    6239223829, 6239223828, 6239223827, 6239223826, 6239223825, 6239223824,
                    6239223823, 6239223822, 6239223821, 6239223820, 6239223819, 6239223818,
                    6239223817, 6239223816, 6239223815, 6239223814, 6239223813, 6239223812,
                    6239223811, 6239223810, 6239223809, 6239223808, 6239223807, 6239223806,
                    6239223805, 6239223804, 6239223803, 6239223802, 6239223801, 6239223800,
                    6239223799, 6239223798, 6239223797, 6239223796, 6239223795, 6239223794,
                    6239223793, 6239223792, 6239223791, 6239223790, 6239223789, 6239223788,
                    6239223787, 6239223786, 6239223785, 6239222584, 6239222583, 6239222582,
                    6239222581, 6239222580, 6239222579, 6239222578, 6239222577, 6239222576,
                    6239222575, 6239222574, 6239222573, 6239222572, 6239222571, 6239222570,
                    6239222569, 6239222568, 6239222567, 6239222566, 6239222565, 6239222564,
                    6239222563, 6239222562, 6239222561, 6239222560, 6239222559, 6239222558,
                    6239222557, 6239222556, 6239222555, 6239222554, 6239222553, 6239222552,
                    6239222551, 6239222550, 6239222549, 6239222548,
                ],
                way.nodes
            );
        }
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
        let exp_rel = Relation {
            id: 529891,
            members: vec![
                Member {
                    ref_: 670634766,
                    role: String::from(""),
                    type_: String::from("node"),
                },
                Member {
                    ref_: 670634768,
                    role: String::from(""),
                    type_: String::from("node"),
                },
            ],
            tags: Some(Vec::from([
                (String::from("name"), String::from("Saint-Barthélemy III")),
                (
                    String::from("note"),
                    String::from("la Barriere des Quatre Vents"),
                ),
                (String::from("ref"), String::from("9712303")),
                (String::from("site"), String::from("geodesic")),
                (
                    String::from("source"),
                    String::from("©IGN 2010 dans le cadre de la cartographie réglementaire"),
                ),
                (String::from("type"), String::from("site")),
                (
                    String::from("url"),
                    String::from(
                        "http://ancien-geodesie.ign.fr/fiche_geodesie_OM.asp?num_site=9712303&X=519509&Y=1980304",
                    ),
                ),
            ])),
            ..Default::default()
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
            tags: Some(Vec::from([
                (String::from("admin_level"), String::from("8")),
                (String::from("boundary"), String::from("administrative")),
                (String::from("local_name"), String::from("Statia")),
                (String::from("name"), String::from("Sint Eustatius")),
                (String::from("name:el"), String::from("Άγιος Ευστάθιος")),
                (String::from("name:fr"), String::from("Saint-Eustache")),
                (String::from("name:nl"), String::from("Sint Eustatius")),
                (String::from("type"), String::from("boundary")),
            ])),
            ..Default::default()
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
    fn boundary_update() {
        let tmpdir_path = tempfile::tempdir().unwrap();
        let tmpdir = tmpdir_path.path().to_str().unwrap();
        OsmBin::init(&tmpdir);
        let mut osmbin = OsmBin::new_writer(&tmpdir).unwrap();
        osmbin.import(PBF_SAINT_BARTHELEMY).unwrap();

        assert_eq!(true, osmbin.read_node(2619283348).is_none());
        assert_approx_eq!(17.9070278, osmbin.read_node(2619283351).unwrap().lat());
        assert_approx_eq!(17.9005419, osmbin.read_node(2619283352).unwrap().lat());
        for i in 2619283353..2619283400 {
            assert_eq!(true, osmbin.read_node(i).is_none());
        }

        assert_eq!(true, osmbin.read_way(255316715).is_none());
        assert_eq!(true, osmbin.read_way(255316716).is_none());
        assert_eq!(true, osmbin.read_way(255316717).is_none());
        assert_eq!(5, osmbin.read_way(255316718).unwrap().nodes.len());
        assert_eq!(6, osmbin.read_way(255316725).unwrap().nodes.len());
        for i in 255316726..255316750 {
            assert_eq!(true, osmbin.read_way(i).is_none());
        }

        drop(osmbin);
        let mut osmbin = OsmBin::new_writer(&tmpdir).unwrap();
        osmbin.update(OSM_BOUNDARY_UPDATE).unwrap();

        assert_eq!(4, osmbin.stats.num_nodes);
        assert_eq!(2, osmbin.stats.num_seek_node_crd);
        assert_eq!(4, osmbin.stats.num_ways);
        assert_eq!(2, osmbin.stats.num_seek_way_idx);

        assert_approx_eq!(18.1085101, osmbin.read_node(2619283348).unwrap().lat());
        assert_approx_eq!(17.9070278, osmbin.read_node(2619283351).unwrap().lat());
        assert_approx_eq!(17.9005419, osmbin.read_node(2619283352).unwrap().lat());
        assert_approx_eq!(18.1153011, osmbin.read_node(2619283354).unwrap().lat());
        assert_eq!(true, osmbin.read_node(2619283355).is_none());
        assert_approx_eq!(18.0159423, osmbin.read_node(2619283356).unwrap().lat());
        assert_approx_eq!(18.0159415, osmbin.read_node(2619283357).unwrap().lat());
        for i in 2619283358..2619283400 {
            assert_eq!(true, osmbin.read_node(i).is_none());
        }

        assert_eq!(true, osmbin.read_way(255316715).is_none());
        assert_eq!(3, osmbin.read_way(255316716).unwrap().nodes.len());
        assert_eq!(true, osmbin.read_way(255316717).is_none());
        assert_eq!(5, osmbin.read_way(255316718).unwrap().nodes.len());
        assert_eq!(6, osmbin.read_way(255316725).unwrap().nodes.len());
        assert_eq!(2, osmbin.read_way(255316727).unwrap().nodes.len());
        assert_eq!(true, osmbin.read_way(255316728).is_none());
        assert_eq!(4, osmbin.read_way(255316729).unwrap().nodes.len());
        assert_eq!(6, osmbin.read_way(255316730).unwrap().nodes.len());
        for i in 255316731..255316750 {
            assert_eq!(true, osmbin.read_way(i).is_none());
        }
    }

    #[test]
    fn bytes5_to_int() {
        assert_eq!(0x00_00_00_00_00, OsmBin::bytes5_to_int([0, 0, 0, 0, 0]));
        assert_eq!(
            0x12_23_45_67_89,
            OsmBin::bytes5_to_int([0x12, 0x23, 0x45, 0x67, 0x89])
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
            assert_eq!(n, OsmBin::bytes5_to_int(OsmBin::int_to_bytes5(n)));
            assert_eq!(14 * n, OsmBin::bytes5_to_int(OsmBin::int_to_bytes5(14 * n)));
            assert_eq!(
                1098 * n,
                OsmBin::bytes5_to_int(OsmBin::int_to_bytes5(1098 * n))
            );
            assert_eq!(
                4898481 * n,
                OsmBin::bytes5_to_int(OsmBin::int_to_bytes5(4898481 * n))
            );
        }
    }
    #[test]
    #[should_panic]
    fn int_to_bytes5_too_big() {
        OsmBin::int_to_bytes5(0x99_12_23_45_67_89);
    }

    #[test]
    fn coord() {
        for n in (-1800000000_i32..1800000000_i32).step_by(100000) {
            assert_eq!(n, OsmBin::bytes4_to_coord(OsmBin::coord_to_bytes4(n)));
            assert_eq!(
                n + 13198,
                OsmBin::bytes4_to_coord(OsmBin::coord_to_bytes4(n + 13198))
            );
            assert_eq!(
                n + 401,
                OsmBin::bytes4_to_coord(OsmBin::coord_to_bytes4(n + 401))
            );
            assert_eq!(
                n + 50014,
                OsmBin::bytes4_to_coord(OsmBin::coord_to_bytes4(n + 50014))
            );
        }
    }

    #[test]
    fn to_digits() {
        assert_eq!(vec![0, 0, 0, 0, 0, 0, 0, 0, 0], OsmBin::to_digits(0));
        assert_eq!(vec![0, 0, 0, 0, 0, 1, 2, 3, 4], OsmBin::to_digits(1234));
        assert_eq!(
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
            OsmBin::to_digits(123456789)
        );
        assert_eq!(
            vec![7, 8, 9, 0, 0, 0, 0, 0, 0],
            OsmBin::to_digits(789000000)
        );
    }
}
