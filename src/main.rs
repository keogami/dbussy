use clap::{Parser, ValueEnum};
use serde_json::json;
use zbus::export::serde::Serialize;
use zbus::export::serde::ser::{SerializeSeq, SerializeTuple};
use std::collections::HashMap;
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

    /// The Signal name to listen to. If not provided, events from all members are reported
    #[clap(short, long)]
    signal: Option<String>,
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

struct SaneValue<'a>(zbus::zvariant::Value<'a>);

impl Serialize for SaneValue<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: zbus::export::serde::Serializer
    {
        use zbus::zvariant::Value;
        use std::os::unix::prelude::AsRawFd;
        use zbus::export::serde::ser::{Error, SerializeMap};
        match &self.0 {
            &Value::U8(n) => serializer.serialize_u8(n),
            &Value::U16(n) => serializer.serialize_u16(n),
            &Value::U32(n) => serializer.serialize_u32(n),
            &Value::U64(n) => serializer.serialize_u64(n),

            &Value::I16(n) => serializer.serialize_i16(n),
            &Value::I32(n) => serializer.serialize_i32(n),
            &Value::I64(n) => serializer.serialize_i64(n),

            &Value::F64(n) => serializer.serialize_f64(n),
            &Value::Fd(n) => serializer.serialize_i32(n.as_raw_fd()),
            &Value::Bool(v) => serializer.serialize_bool(v),

            Value::Str(v) => serializer.serialize_str(v.as_str()),
            Value::ObjectPath(v) => serializer.serialize_str(v.as_str()),
            Value::Signature(v) => serializer.serialize_str(v.as_str()),
            Value::Value(v) => SaneValue((**v).to_owned()).serialize(serializer),

            Value::Array(v) => {
                let v = v.to_vec();
                let mut seq = serializer.serialize_seq(Some(v.len()))?;
                for element in v {
                    seq.serialize_element(&SaneValue(element))?;
                }
                seq.end()
            },

            Value::Structure(v) => {
                let v = v.fields();
                let mut seq = serializer.serialize_tuple(v.len())?;
                for element in v {
                    seq.serialize_element(&SaneValue(element.to_owned()))?;
                }
                seq.end()
            },

            Value::Dict(v) => {
                let d = v.to_owned();
                let v: HashMap<String, Value> = match HashMap::try_from(d) {
                    Ok(map) => map,
                    Err(err) => return Err(Error::custom(format!("Dict couldn't be converted into a HashMap for serialization: {}", err))),
                };
                let mut map = serializer.serialize_map(Some(v.len()))?;
                for (key, value) in v {
                    map.serialize_entry(&SaneValue(Value::from(key)), &SaneValue(value))?;
                }
                map.end()
            },
        }
    }
}

fn iterate_messages(iter: SignalIterator, jq: &mut jq_rs::JqProgram) -> Result<(), Box<dyn Error>> {
    for message in iter {
        let structure: zbus::zvariant::Structure = message.body()?;
        let structure = SaneValue(zbus::zvariant::Value::Structure(structure));

        let data = serde_json::to_value(&structure)?;
        let signature = serde_json::to_value(structure.0.value_signature())?;

        let value = json!({
            "data": data,
            "signature": signature,
        });

        let mut filtered = jq.run(&value.to_string())?;
        filtered.pop(); // removing the trailing newline from the jq

        println!("{}", filtered);
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let proxy = gen_proxy(args.bus, args.name, args.path, args.interface)?;
    let mut jq = jq_rs::compile(&args.query)?;

    let iter = match args.signal {
        Some(signal) => proxy.receive_signal(signal)?,
        None => proxy.receive_all_signals()?,
    };

    iterate_messages(iter, &mut jq)?;
    Ok(())
}
