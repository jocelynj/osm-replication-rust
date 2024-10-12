use clap::Parser;

use osmbin_rust::osmxml;
use osmbin_rust::osm::{OsmWriter, OsmUpdate};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Source OSM file")]
    pub source: String,
    #[arg(long, help = "Destination OSM file")]
    pub dest: String,
}

fn main() {
    let args = Args::parse();

    let mut osmxml = osmxml::OsmXml::new(&args.dest).unwrap();
    if args.source.ends_with(".osm") || args.source.ends_with(".osm.gz") {
        osmxml.import(&args.source).unwrap();
    } else if args.source.ends_with(".osc") || args.source.ends_with(".osc.gz") {
        osmxml.update(&args.source).unwrap();
    } else {
        panic!("Not supported file type: {}", args.source);
    }
}
