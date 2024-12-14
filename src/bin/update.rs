use clap::Parser;
use std::error::Error;

use osm_replication_rust::update;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Polygon directory")]
    pub polygons: String,
    #[arg(long, help = "Directory for osmbin database")]
    pub osmbin: String,
    #[arg(long, help = "Diffs directory")]
    pub diffs: String,
    #[arg(
        long,
        help = "URL where to fetch original diffs",
        default_value = "https://planet.openstreetmap.org/replication/minute/"
    )]
    pub url_diffs: String,
    #[arg(long, help = "Max state to download")]
    pub max_state: Option<u64>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    match update::Update::update(
        &args.osmbin,
        &args.polygons,
        &args.diffs,
        &args.url_diffs,
        args.max_state,
    ) {
        Ok(o) => Ok(o),
        Err(e) => Err(Box::new(e)),
    }
}
