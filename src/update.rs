use anstyle;
use chrono;
use std::cmp::min;
use std::fs;
use std::io;
use std::io::{BufWriter, ErrorKind};
use std::os::unix;
use std::path::Path;
use std::thread;
use std::time;
use thiserror;
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
    pub fn update(
        dir_osmbin: &str,
        dir_polygon: &str,
        dir_diffs: &str,
        url_diffs: &str,
        max_state: Option<u64>,
    ) -> Result<(), Error> {
        let polys = diffs::Poly::get_poly_from_dir(dir_polygon);

        let state_file = dir_diffs.to_string() + "planet/minute/state.txt";
        let cur_state = match Self::read_state_from_file(&state_file) {
            Err(e) => {
                let red = anstyle::Style::new().fg_color(Some(anstyle::AnsiColor::Red.into()));
                eprintln!("{red}Error: Please put a valid state file on {state_file}{red:#}");
                return Err(e);
            }
            Ok(o) => o,
        };

        let remote_state = url_diffs.to_string() + "state.txt";
        let mut remote_state = match Self::read_state_from_url(&remote_state) {
            Err(e) => {
                let red = anstyle::Style::new().fg_color(Some(anstyle::AnsiColor::Red.into()));
                eprintln!("{red}Error: Couldn’t download state file from {remote_state}{red:#}");
                return Err(e);
            }
            Ok(o) => o,
        };
        if let Some(s) = max_state {
            remote_state = min(remote_state, s);
        }

        if cur_state == remote_state {
            printlnt!("No update necessary from {}", cur_state);
            return Ok(());
        } else if (cur_state + 1) == remote_state {
            printlnt!("Need to update {}", cur_state + 1);
        } else {
            printlnt!("Need to update from {} to {remote_state}", cur_state + 1);
        }

        #[allow(clippy::range_plus_one)]
        for n in (cur_state + 1)..(remote_state + 1) {
            printlnt!("{n}");
            let n_split = format!(
                "{:03}/{:03}/{:03}",
                (n / 1_000_000) % 1000,
                (n / 1_000) % 1000,
                n % 1000
            );
            let n_split = n_split.as_str();

            let orig_state = dir_diffs.to_string() + "planet/minute/" + n_split + ".state.txt";
            let orig_diff = dir_diffs.to_string() + "planet/minute/" + n_split + ".osc.gz";
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
            let osmcache = osmxml.get_reader().get_cache();
            let diff = diffs::Diff::new_osmcache(
                osmcache,
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
        Ok(())
    }

    fn read_state_from_file(filename: &str) -> Result<u64, Error> {
        let content = match fs::read_to_string(filename) {
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Err(Error::StateNotFound(filename.to_string()))
            }
            r => r.unwrap(),
        };
        Self::read_state(&content, filename)
    }

    fn read_state_from_url(url: &str) -> Result<u64, Error> {
        let remote_state = match ureq::get(url)
            .set("User-Agent", "osm-extract-replication")
            .call()
        {
            Err(e) => return Err(Error::Network(Box::new(e))),
            Ok(o) => o,
        };
        let remote_state = remote_state.into_string().unwrap();
        Self::read_state(&remote_state, url)
    }

    fn read_state(content: &str, source: &str) -> Result<u64, Error> {
        for l in content.lines() {
            if l.starts_with("sequenceNumber=") {
                return Ok(l.split('=').nth(1).unwrap().parse().unwrap());
            }
        }
        Err(Error::StateIncorrect(source.to_string()))
    }

    fn download(url: &str, filename: &str) -> Result<(), Error> {
        match fs::create_dir_all(Path::new(&filename).parent().unwrap()) {
            Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
            r => r.unwrap(),
        };
        let response;
        let mut i = 0;
        loop {
            match ureq::get(url)
                .set("User-Agent", "osm-extract-replication")
                .call()
            {
                Err(e) => {
                    if i == 4 {
                        return Err(Error::Network(Box::new(e)));
                    }
                }
                Ok(o) => {
                    response = o;
                    break;
                }
            };
            println!("Error when fetching {url} - will retry again");
            thread::sleep(time::Duration::from_secs(1));
            i += 1;
        }
        let last_modified = response.header("Last-Modified").unwrap();
        let last_modified = chrono::DateTime::parse_from_rfc2822(last_modified).unwrap();
        let file = match fs::File::create(filename) {
            Err(e) => return Err(Error::IO(e)),
            Ok(o) => o,
        };
        let mut writer = BufWriter::new(file);
        match io::copy(&mut response.into_reader(), &mut writer) {
            Err(e) => return Err(Error::IO(e)),
            Ok(o) => o,
        };
        drop(writer);
        let file = match fs::File::open(filename) {
            Err(e) => return Err(Error::IO(e)),
            Ok(o) => o,
        };
        match file.set_modified(last_modified.into()) {
            Err(e) => Err(Error::IO(e)),
            Ok(o) => Ok(o),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error(transparent)]
    Network(#[from] Box<ureq::Error>),
    #[error("state file {0} not found")]
    StateNotFound(String),
    #[error("state file {0} has an incorrect format")]
    StateIncorrect(String),
}
