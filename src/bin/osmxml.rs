use clap::Parser;

use osm_replication_rust::osm::{OsmUpdate, OsmWriter};
use osm_replication_rust::osmxml;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Source OSM file")]
    pub source: String,
    #[arg(long, help = "Destination OSM file")]
    pub dest: String,
    #[arg(long, help = "Add bbox field", requires = "osmbin")]
    pub bbox: bool,
    #[arg(
        long,
        help = "Filter with given polygon",
        requires = "osmbin",
        conflicts_with = "bbox"
    )]
    pub filter: Option<String>,
    #[arg(long, help = "Directory for osmbin database", required = false)]
    pub osmbin: String,
}

fn main() {
    let args = Args::parse();

    if args.source.ends_with(".osm") || args.source.ends_with(".osm.gz") {
        let mut osmxml = osmxml::OsmXml::new(&args.dest).unwrap();
        osmxml.import(&args.source).unwrap();
    } else if args.source.ends_with(".osc") || args.source.ends_with(".osc.gz") {
        if args.bbox {
            let mut osmxml =
                osmxml::bbox::OsmXmlBBox::new_osmbin(&args.dest, &args.osmbin).unwrap();
            osmxml.update(&args.source).unwrap();
        } else if let Some(filter) = args.filter {
            let mut osmxml =
                osmxml::filter::OsmXmlFilter::new_osmbin(&args.dest, &args.osmbin, &filter)
                    .unwrap();
            osmxml.update(&args.source).unwrap();
        } else {
            let mut osmxml = osmxml::OsmXml::new(&args.dest).unwrap();
            osmxml.update(&args.source).unwrap();
        }
    } else {
        panic!("Not supported file type: {}", args.source);
    }
}
