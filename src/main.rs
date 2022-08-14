use clap::{Parser, ValueEnum};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum BusType {
    System,
    Session,
}

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// type of bus to listen on
    #[clap(short, long, arg_enum)]
    bus: BusType,

    /// name of the service
    #[clap(short, long)]
    name: String,

    /// interface to listen to
    #[clap(short, long)]
    interface: String,

    /// path of the object
    #[clap(short, long)]
    path: String,

    /// JQL query to filter and manipulate dbus signal body
    #[clap(short, long)]
    query: String,
}

fn main() {
    let _args = Args::parse();
}
