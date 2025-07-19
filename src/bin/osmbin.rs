use clap::Parser;

use osm_replication_rust::osm::{OsmReader, OsmUpdate, OsmWriter};
use osm_replication_rust::osmbin;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Directory for osmbin database")]
    pub dir: String,
    #[clap(flatten)]
    command: Command,
    #[arg(long, help = "Verbose mode")]
    pub verbose: bool,
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
    #[arg(long, help = "Check database")]
    pub check: Option<u64>,
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
            "relation_full" => {
                let relation = osmbin.read_relation_full(id, &[]);
                if let Some(relation) = relation {
                    println!("{} members", relation.members.len());
                    if args.verbose {
                        println!("{relation:?}");
                    }
                    osmbin.print_stats();
                } else {
                    println!("Relation not found");
                }
            }
            _ => panic!("--read option {elem} not recognized"),
        };
    }
    if let Some(check) = args.command.check {
        let mut osmbin = osmbin::OsmBin::new(&args.dir).unwrap();
        if let Err(e) = osmbin.check_database(check) {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
