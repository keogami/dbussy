use clap::{Parser, ValueEnum};
use std::error::Error;
use zbus::blocking::{Connection, Proxy};
use zbus::names::{InterfaceName, BusName};
use zbus::zvariant::ObjectPath;

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

fn gen_proxy<'a, N, P, I>(bus_type: BusType, name: N, path: P, iface: I) -> Result<zbus::blocking::Proxy<'a>, Box<dyn Error>>
where
    N: TryInto<BusName<'a>>,
    P: TryInto<ObjectPath<'a>>,
    I: TryInto<InterfaceName<'a>>,
    N::Error: Into<zbus::Error>,
    P::Error: Into<zbus::Error>,
    I::Error: Into<zbus::Error>,
{
    let conn = match bus_type {
        BusType::Session => Connection::session()?,
        BusType::System => Connection::system()?,
    };

    let proxy = Proxy::new(&conn, name, path, iface)?;

    Ok(proxy)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let _proxy = gen_proxy(args.bus, args.name, args.path, args.interface)?;
    let _jq = jq_rs::compile(&args.query)?;

    Ok(())
}
