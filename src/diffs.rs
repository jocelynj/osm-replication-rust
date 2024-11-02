use std::error::Error;
use std::fmt;
use std::fs;
use std::fs::File;
use std::io::ErrorKind;
use std::os::unix;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::osm::OsmUpdate;
use crate::osmbin;
use crate::osmxml;

pub struct Poly {
    file: Option<PathBuf>,
    name: String,
    hier_name: String,
    inners: Vec<Poly>,
}

pub struct Diff {
    dir_osmbin: String,
    dest_diff_dir: PathBuf,
    dest_diff_file: PathBuf,
    dest_diff_tmp_file: PathBuf,
    dest_modified_time: SystemTime,
    orig_state_file: PathBuf,
    dest_state_file: PathBuf,
}

impl Diff {
    pub fn new(
        dir_osmbin: &str,
        dest_diff_dir: &str,
        dest_diff_file: &str,
        dest_modified_time: SystemTime,
        orig_state_file: &str,
    ) -> Diff {
        let dest_diff_tmp_file;
        let dest_state_file;
        if let Some(prefix) = dest_diff_file.strip_suffix(".osc.gz") {
            dest_diff_tmp_file = PathBuf::from(prefix.to_owned() + "-tmp.osc.gz");
            dest_state_file = PathBuf::from(prefix.to_owned() + ".state.txt");
        } else {
            panic!("Filename given should end with '.osc.gz': {dest_diff_file}");
        };
        Diff {
            dir_osmbin: dir_osmbin.to_string(),
            dest_diff_dir: PathBuf::from(dest_diff_dir),
            dest_diff_file: PathBuf::from(dest_diff_file),
            dest_diff_tmp_file,
            dest_modified_time,
            orig_state_file: PathBuf::from(orig_state_file),
            dest_state_file,
        }
    }
    pub fn generate_diff(
        &self,
        poly: &Poly,
        orig_diff: &str,
        lvl: usize,
    ) -> Result<String, Box<dyn Error>> {
        let poly_file = poly
            .file
            .as_ref()
            .expect("poly should have a filename provided");
        println!("{}{}", " ".repeat(lvl), poly.name);
        let dest_diff_tmp_path = Path::new(&self.dest_diff_dir)
            .join(&poly.hier_name)
            .join(&self.dest_diff_tmp_file);
        match fs::create_dir_all(dest_diff_tmp_path.parent().unwrap()) {
            Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
            r => r.unwrap(),
        };
        let reader = osmbin::OsmBin::new(&self.dir_osmbin).unwrap();
        let dest_diff_tmp = dest_diff_tmp_path.to_str().unwrap();
        let mut osmxml = osmxml::filter::OsmXmlFilter::new_reader(
            dest_diff_tmp,
            reader,
            poly_file.to_str().unwrap(),
        )
        .unwrap();
        osmxml.update(orig_diff).unwrap();

        let dest_state_file = Path::new(&self.dest_diff_dir)
            .join(&poly.hier_name)
            .join(&self.dest_state_file);
        match fs::hard_link(&self.orig_state_file, &dest_state_file) {
            Err(err) if err.kind() == ErrorKind::AlreadyExists => (),
            r => r.unwrap(),
        };

        File::open(&dest_diff_tmp_path)
            .unwrap()
            .set_modified(self.dest_modified_time)
            .unwrap();

        let dest_diff_path = Path::new(&self.dest_diff_dir)
            .join(&poly.hier_name)
            .join(&self.dest_diff_file);
        fs::rename(&dest_diff_tmp_path, &dest_diff_path).unwrap();

        let state_file = Path::new(&self.dest_diff_dir)
            .join(&poly.hier_name)
            .join("minute/state.txt");
        match fs::remove_file(&state_file) {
            Err(err) if err.kind() == ErrorKind::NotFound => (),
            r => r.unwrap(),
        };
        unix::fs::symlink(
            self.dest_state_file.strip_prefix("minute/").unwrap(),
            &state_file,
        )
        .unwrap();

        let dest_diff = dest_diff_path.to_str().unwrap();
        Ok(String::from(dest_diff))
    }

    pub fn generate_diff_recursive(
        &self,
        poly: &Poly,
        orig_diff: &str,
        lvl: usize,
    ) -> Result<(), Box<dyn Error>> {
        let orig_diff: &str = if poly.file.is_some() {
            &self.generate_diff(poly, orig_diff, lvl).unwrap()
        } else {
            orig_diff
        };

        for p in &poly.inners {
            self.generate_diff_recursive(p, orig_diff, lvl + 2).unwrap();
        }
        Ok(())
    }
}

impl Poly {
    pub fn get_poly_from_dir(dir: &str) -> Poly {
        let path = Path::new(dir);
        Self::get_poly_from_path(path, None, ".")
    }

    fn get_poly_from_path(dir: &Path, file: Option<PathBuf>, hier: &str) -> Poly {
        let mut inners: Vec<Poly> = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "poly" {
                        let name = path.file_stem().unwrap().to_string_lossy().to_string();
                        let dir = path.parent().unwrap().join(path.file_stem().unwrap());
                        let mut hier_name = String::from(hier);
                        hier_name.push('/');
                        hier_name.push_str(&name);
                        if dir.exists() {
                            inners.push(Self::get_poly_from_path(&dir, Some(path), &hier_name));
                        } else {
                            inners.push(Poly {
                                file: Some(path),
                                name,
                                hier_name,
                                inners: vec![],
                            });
                        }
                    }
                }
            } else if path.is_dir() {
                let mut poly = path.clone();
                poly.set_extension("poly");
                if poly.exists() {
                    continue;
                }
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                let mut hier_name = String::from(hier);
                hier_name.push('/');
                hier_name.push_str(&name);
                inners.push(Self::get_poly_from_path(&path, None, &hier_name));
            }
        }
        inners.sort_by(|a, b| {
            a.file
                .as_ref()
                .unwrap()
                .to_str()
                .cmp(&b.file.as_ref().unwrap().to_str())
        });
        let name;
        if let Some(ref f) = file {
            name = f.file_stem().unwrap().to_string_lossy().to_string();
        } else {
            name = String::from("");
        }
        Poly {
            file,
            name,
            hier_name: hier.to_string(),
            inners,
        }
    }

    fn fmt_inners(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        if let Some(c) = &self.file {
            writeln!(
                f,
                "{}{}",
                " ".repeat(indent),
                c.file_stem().unwrap().to_str().unwrap()
            )?;
        } else {
            writeln!(f, "{}{:?}", " ".repeat(indent), &self.file)?;
        }
        for i in &self.inners {
            i.fmt_inners(f, indent + 2)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Poly {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_inners(f, 0)?;
        Ok(())
    }
}
