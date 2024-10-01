use clap::Parser;

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
    #[arg(long, num_args=2, value_names=["ELEM", "ID"], help="Read node/way/relation id from database")]
    pub read: Vec<String>,
}

fn main() {
    let args = Args::parse();

    if args.command.init {
        osmbin_rust::OsmBin::init(&args.dir);
    }
    if !args.command.read.is_empty() {
        let elem = args.command.read[0].clone();
        let id: u64 = args.command.read[1]
            .trim()
            .parse()
            .expect("ID should be a number");

        let mut osmbin = osmbin_rust::OsmBin::new(&args.dir).unwrap();
        match elem.as_str() {
            "node" => println!("{:?}", osmbin.read_node(id)),
            "way" => println!("{:?}", osmbin.read_way(id)),
            "relation" => println!("{:?}", osmbin.read_relation(id)),
            "relation_full" => (),
            _ => panic!("--read option {elem} not recognized"),
        };
    }
}
