use clap::Parser;

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

fn main() {
    let args = Args::parse();

    update::Update::update(&args.osmbin, &args.polygons, &args.diffs, &args.url_diffs, args.max_state);
}
