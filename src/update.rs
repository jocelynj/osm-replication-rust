use chrono;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::io::{BufWriter, ErrorKind};
use std::os::unix;
use std::path::Path;
use ureq;

use crate::diffs;
use crate::osm::OsmUpdate;
use crate::osmbin;
use crate::osmxml;

macro_rules! printlnt {
    ($($arg:tt)*) => {
        println!("{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), format_args!($($arg)*));
    };
}

pub struct Update {}

impl Update {
    pub fn update(dir_osmbin: &str, dir_polygon: &str, dir_diffs: &str, url_diffs: &str) {
        let polys = diffs::Poly::get_poly_from_dir(dir_polygon);

        let state_file = dir_diffs.to_string() + "europe/minute/state.txt";
        let cur_state = Self::read_state_from_file(&state_file).unwrap();

        let remote_state = url_diffs.to_string() + "state.txt";
        let remote_state = Self::read_state_from_url(&remote_state).unwrap();

        println!("{cur_state} - {remote_state}");

        for n in (cur_state + 1)..remote_state {
            printlnt!("{n}");
            let n_split = format!(
                "{:03}/{:03}/{:03}",
                (n / 1_000_000) % 1000,
                (n / 1_000) % 1000,
                n % 1000
            );
            let n_split = n_split.as_str();

            let orig_state = dir_diffs.to_string() + "europe/minute/" + n_split + ".state.txt";
            let orig_diff = dir_diffs.to_string() + "europe/minute/" + n_split + ".osc.gz";
            let bbox_state = dir_diffs.to_string() + "bbox/minute/" + n_split + ".state.txt";
            let bbox_diff = dir_diffs.to_string() + "bbox/minute/" + n_split + ".osc.gz";
            let dest_suffix = String::from("minute/") + n_split + ".osc.gz";

            printlnt!("  download");
            Self::download(&(url_diffs.to_string() + n_split + ".osc.gz"), &orig_diff).unwrap();
            Self::download(
                &(url_diffs.to_string() + n_split + ".state.txt"),
                &orig_state,
            )
            .unwrap();

            printlnt!("  bbox");
            match fs::create_dir_all(Path::new(&bbox_diff).parent().unwrap()) {
                Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
                r => r.unwrap(),
            };
            let mut osmxml = osmxml::bbox::OsmXmlBBox::new_osmbin(&bbox_diff, dir_osmbin).unwrap();
            osmxml.update(&orig_diff).unwrap();

            match fs::hard_link(&orig_state, &bbox_state) {
                Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
                r => r.unwrap(),
            };

            let bbox_state_file = dir_diffs.to_string() + "bbox/minute/state.txt";
            let bbox_state_file = Path::new(&bbox_state_file);
            match fs::remove_file(bbox_state_file) {
                Err(err) if err.kind() == ErrorKind::NotFound => (),
                r => r.unwrap(),
            };
            unix::fs::symlink(n_split.to_string() + ".state.txt", bbox_state_file).unwrap();

            printlnt!("  diff generation");
            let dest_modified_time = fs::metadata(&orig_diff).unwrap().modified().unwrap();
            let diff = diffs::Diff::new(
                dir_osmbin,
                dir_diffs,
                &dest_suffix,
                dest_modified_time,
                &orig_state,
            );
            diff.generate_diff_recursive(&polys, &bbox_diff, 0).unwrap();

            printlnt!("  osmbin update");
            let mut osmbin = osmbin::OsmBin::new_writer(dir_osmbin).unwrap();
            osmbin.update(&orig_diff).unwrap();

            let state_file = Path::new(&state_file);
            match fs::remove_file(state_file) {
                Err(err) if err.kind() == ErrorKind::NotFound => (),
                r => r.unwrap(),
            };
            unix::fs::symlink(n_split.to_string() + ".state.txt", state_file).unwrap();
        }
    }

    fn read_state_from_file(filename: &str) -> Result<u64, Box<dyn Error>> {
        let content = match fs::read_to_string(filename) {
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Err(Box::new(io::Error::new(
                    ErrorKind::NotFound,
                    format!("State file {filename} not found"),
                )));
            }
            r => r.unwrap(),
        };
        Self::read_state(&content, filename)
    }

    fn read_state_from_url(url: &str) -> Result<u64, Box<dyn Error>> {
        let remote_state = ureq::get(url)
            .set("User-Agent", "osm-extract-replication")
            .call()
            .unwrap();
        let remote_state = remote_state.into_string().unwrap();
        Self::read_state(&remote_state, url)
    }

    fn read_state(content: &str, source: &str) -> Result<u64, Box<dyn Error>> {
        for l in content.lines() {
            if l.starts_with("sequenceNumber=") {
                return Ok(l.split('=').nth(1).unwrap().parse().unwrap());
            }
        }
        Err(StateNotFound {
            filename: source.to_string(),
        }
        .into())
    }

    fn download(url: &str, filename: &str) -> Result<(), io::Error> {
        match fs::create_dir_all(Path::new(&filename).parent().unwrap()) {
            Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
            r => r.unwrap(),
        };
        let response = ureq::get(url)
            .set("User-Agent", "osm-extract-replication")
            .call()
            .unwrap();
        let last_modified = response.header("Last-Modified").unwrap();
        let last_modified = chrono::DateTime::parse_from_rfc2822(last_modified).unwrap();
        let file = fs::File::create(filename)?;
        let mut writer = BufWriter::new(file);
        io::copy(&mut response.into_reader(), &mut writer)?;
        drop(writer);
        let file = fs::File::open(filename)?;
        file.set_modified(last_modified.into())
    }
}

#[derive(Debug)]
pub struct StateNotFound {
    pub filename: String,
}
impl Error for StateNotFound {}
impl fmt::Display for StateNotFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Incorrect state file: {}", self.filename)
    }
}
