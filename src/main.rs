use clap::Parser;

use osmbin_rust::osmbin;
use osmbin_rust::osm::{OsmReader, OsmWriter, OsmUpdate};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Directory for osmbin database")]
    pub dir: String,
    #[clap(flatten)]
    command: Command,
}

#[derive(Parser, Debug)]
#[group(required = true, multiple = true)]
struct Command {
    #[arg(long, help = "Init database")]
    pub init: bool,
    #[arg(long, help = "Import file to database")]
    pub import: Option<String>,
    #[arg(long, help = "Apply diff file to database")]
    pub update: Option<String>,
    #[arg(long, num_args=2, value_names=["ELEM", "ID"], help="Read node/way/relation id from database")]
    pub read: Vec<String>,
}

fn main() {
    let args = Args::parse();

    if args.command.init {
        osmbin::OsmBin::init(&args.dir);
    }
    if args.command.import.is_some() {
        let mut osmbin = osmbin::OsmBin::new_writer(&args.dir).unwrap();
        osmbin.import(&args.command.import.unwrap()).unwrap();
    }
    if args.command.update.is_some() {
        let mut osmbin = osmbin::OsmBin::new_writer(&args.dir).unwrap();
        osmbin.update(&args.command.update.unwrap()).unwrap();
    }
    if !args.command.read.is_empty() {
        let elem = args.command.read[0].clone();
        let id: u64 = args.command.read[1]
            .trim()
            .parse()
            .expect("ID should be a number");

        let mut osmbin = osmbin::OsmBin::new(&args.dir).unwrap();
        match elem.as_str() {
            "node" => println!("{:?}", osmbin.read_node(id)),
            "way" => println!("{:?}", osmbin.read_way(id)),
            "relation" => println!("{:?}", osmbin.read_relation(id)),
            "relation_full" => (),
            _ => panic!("--read option {elem} not recognized"),
        };
    }
}
