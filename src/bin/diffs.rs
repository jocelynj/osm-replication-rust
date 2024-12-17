use clap::Parser;
use std::fs;

use osm_replication_rust::diffs;
use osm_replication_rust::osm::OsmUpdate;
use osm_replication_rust::osmxml;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Polygon directory")]
    pub polygons: String,
    #[arg(long, help = "Directory for osmbin database")]
    pub osmbin: String,
    #[arg(
        long,
        help = "Use OsmCache instead of OsmBin for recursive diffs",
        required = false
    )]
    pub use_osmcache: bool,
    #[arg(long, help = "Source osc file")]
    pub source: String,
    #[arg(long, help = "Source state.txt file")]
    pub state: String,
    #[arg(long, help = "Destination osc directory")]
    pub dest_dir: String,
    #[arg(long, help = "Destination osc suffix")]
    pub dest_suffix: String,
}

fn main() {
    let args = Args::parse();

    let polys = diffs::Poly::get_poly_from_dir(&args.polygons);
    let dest_modified_time = fs::metadata(&args.source).unwrap().modified().unwrap();

    let dest = String::from("/dev/null");
    let mut osmxml = osmxml::bbox::OsmXmlBBox::new_osmbin(&dest, &args.osmbin).unwrap();
    osmxml.update(&args.source).unwrap();

    let diff = if args.use_osmcache {
        let osmcache = osmxml.get_reader().get_cache();
        diffs::Diff::new_osmcache(
            osmcache,
            &args.dest_dir,
            &args.dest_suffix,
            dest_modified_time,
            &args.state,
        )
    } else {
        diffs::Diff::new_osmbin(
            &args.osmbin,
            &args.dest_dir,
            &args.dest_suffix,
            dest_modified_time,
            &args.state,
        )
    };
    diff.generate_diff_recursive(&polys, &args.source, 0)
        .unwrap();
}
