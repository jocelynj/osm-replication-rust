use clap::Parser;
use fd_lock::RwLock;
use std::error::Error;
use std::fs::File;

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

    let lock_file = String::from(&args.diffs) + "/../update.lock";
    let mut f = RwLock::new(
        File::options()
            .append(true)
            .create(true)
            .open(lock_file.clone())
            .unwrap(),
    );
    let lock = match f.try_write() {
        Ok(o) => o,
        Err(e) => panic!("Couldn't take lock {lock_file}: {e}"),
    };

    let result = update::Update::update(
        &args.osmbin,
        &args.polygons,
        &args.diffs,
        &args.url_diffs,
        args.max_state,
    );
    drop(lock);

    match result {
        Ok(o) => Ok(o),
        Err(e) => Err(Box::new(e)),
    }
}
