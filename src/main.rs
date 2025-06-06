use anyhow::Context;
use clap::{Parser, ValueEnum};
use serde_json::json;
use std::collections::HashMap;
use std::error::Error;
use zbus::blocking::{Connection, Proxy, SignalIterator};
use zbus::export::serde::ser::{SerializeSeq, SerializeTuple};
use zbus::export::serde::Serialize;
use zbus::names::{BusName, InterfaceName};
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

fn gen_proxy<'a, N, P, I>(
    bus_type: BusType,
    name: N,
    path: P,
    iface: I,
) -> anyhow::Result<zbus::blocking::Proxy<'a>>
where
    N: TryInto<BusName<'a>>,
    P: TryInto<ObjectPath<'a>>,
    I: TryInto<InterfaceName<'a>>,
    N::Error: Into<zbus::Error>,
    P::Error: Into<zbus::Error>,
    I::Error: Into<zbus::Error>,
{
    let conn = match bus_type {
        BusType::Session => Connection::session().context("Couldn't connect to session bus")?,
        BusType::System => Connection::system().context("Couldn't connect to system bus")?,
    };

    let proxy = Proxy::new(&conn, name, path, iface)?;

    Ok(proxy)
}

struct SaneValue<'a>(zbus::zvariant::Value<'a>);

impl Serialize for SaneValue<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: zbus::export::serde::Serializer,
    {
        use std::os::unix::prelude::AsRawFd;
        use zbus::export::serde::ser::{Error, SerializeMap};
        use zbus::zvariant::Value;
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
            }

            Value::Structure(v) => {
                let v = v.fields();
                let mut seq = serializer.serialize_tuple(v.len())?;
                for element in v {
                    seq.serialize_element(&SaneValue(element.to_owned()))?;
                }
                seq.end()
            }

            Value::Dict(v) => {
                let d = v.to_owned();
                let v: HashMap<String, Value> = match HashMap::try_from(d) {
                    Ok(map) => map,
                    Err(err) => {
                        return Err(Error::custom(format!(
                            "Dict couldn't be converted into a HashMap for serialization: {}",
                            err
                        )))
                    }
                };
                let mut map = serializer.serialize_map(Some(v.len()))?;
                for (key, value) in v {
                    map.serialize_entry(&SaneValue(Value::from(key)), &SaneValue(value))?;
                }
                map.end()
            }
        }
    }
}

fn iterate_messages(iter: SignalIterator, jq: &mut jq_rs::JqProgram) -> anyhow::Result<()> {
    for message in iter {
        let structure: zbus::zvariant::Structure =
            message.body().context("Couldn't deserialize message")?;
        let structure = SaneValue(zbus::zvariant::Value::Structure(structure));

        let data = serde_json::to_value(&structure).expect("Serializer to not panic");
        let signature = serde_json::to_value(structure.0.value_signature())
            .expect("Signature to be serializable to valid json");

        let signal = match message.member() {
            Some(name) => name.as_str().to_owned(),
            None => "".to_owned(),
        };

        let value = json!({
            "data": data,
            "signature": signature,
            "signal": signal,
        });

        let mut filtered = jq.run(&value.to_string()).context("Jq failed to run")?;
        filtered.pop(); // removing the trailing newline from the jq

        println!("{}", filtered);
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let proxy = gen_proxy(args.bus, args.name, args.path, args.interface)
        .context("Couldn't generate proxy to bus")?;
    let mut jq = jq_rs::compile(&args.query).context("Couldn't compile the jq command")?;

    let iter = match args.signal {
        Some(signal) => proxy
            .receive_signal(signal)
            .context("Couldn't start receiving for the signal")?,
        None => proxy
            .receive_all_signals()
            .context("Couldn't start receiving for all signals")?,
    };

    iterate_messages(iter, &mut jq).context("Failed while receiving messages")?;
    Ok(())
}
