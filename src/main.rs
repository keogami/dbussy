use clap::{Parser, ValueEnum};
use serde_json::json;
use std::error::Error;
use zbus::blocking::{Connection, Proxy, SignalIterator};
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

fn iterate_messages(iter: SignalIterator, jq: &mut jq_rs::JqProgram) -> Result<(), Box<dyn Error>> {
    for message in iter {
        let structure: zbus::zvariant::Structure = message.body()?;

        let data = serde_json::to_value(structure.fields())?;

        let signature = serde_json::to_value(structure.full_signature())?;

        let value = json!({
            "data": data,
            "signature": signature,
        });

        let filtered = jq.run(&value.to_string())?;

        println!("{}", filtered);
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let proxy = gen_proxy(args.bus, args.name, args.path, args.interface)?;
    let mut jq = jq_rs::compile(&args.query)?;

    iterate_messages(proxy.receive_all_signals()?, &mut jq)?;
    Ok(())
}
