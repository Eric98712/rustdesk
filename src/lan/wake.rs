use std::{io, net::Ipv4Addr, thread, time::Duration};

const USAGE: &str = "Usage: rustdesk --wol MAC [BROADCAST_IPV4 [SOURCE_IPV4]]";

struct Destination {
    mac: wol::MacAddr,
    broadcast: Ipv4Addr,
    source: Ipv4Addr,
}

impl Destination {
    fn parse(args: &[String]) -> io::Result<Self> {
        if !(1..=3).contains(&args.len()) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE));
        }
        let mac = args[0].parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid MAC address; use AA:BB:CC:DD:EE:FF",
            )
        })?;
        let broadcast = args
            .get(1)
            .map_or(Ok(Ipv4Addr::BROADCAST), |value| value.parse())
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid broadcast IPv4 address",
                )
            })?;
        let source = args
            .get(2)
            .map_or(Ok(Ipv4Addr::UNSPECIFIED), |value| value.parse())
            .map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid source IPv4 address")
            })?;
        Ok(Self {
            mac,
            broadcast,
            source,
        })
    }
}

pub(crate) fn run(args: &[String]) -> io::Result<()> {
    let destination = Destination::parse(args)?;
    for attempt in 0..3 {
        wol::send_wol(
            destination.mac,
            Some(destination.broadcast.into()),
            Some(destination.source.into()),
        )?;
        if attempt < 2 {
            thread::sleep(Duration::from_millis(250));
        }
    }
    println!(
        "Sent 3 wake packets to {}:9 via {}. Packet transmission does not confirm wake-up.",
        destination.broadcast, destination.source
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn accepts_manual_mac_and_explicit_lan_route_without_discovery() {
        let destination = Destination::parse(&args(&["AA:BB:CC:DD:EE:FF"])).unwrap();
        assert_eq!(destination.mac.0, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(destination.broadcast, Ipv4Addr::BROADCAST);
        assert_eq!(destination.source, Ipv4Addr::UNSPECIFIED);
        let destination = Destination::parse(&args(&[
            "AA-BB-CC-DD-EE-FF",
            "192.168.50.255",
            "192.168.50.2",
        ]))
        .unwrap();
        assert_eq!(destination.broadcast, Ipv4Addr::new(192, 168, 50, 255));
        assert_eq!(destination.source, Ipv4Addr::new(192, 168, 50, 2));
    }

    #[test]
    fn rejects_invalid_arguments_before_sending() {
        for values in [
            vec![],
            vec!["invalid"],
            vec!["AA:BB:CC:DD:EE:FF", "not-an-ip"],
            vec!["AA:BB:CC:DD:EE:FF", "::1"],
            vec!["AA:BB:CC:DD:EE:FF", "192.168.50.255", "not-an-ip"],
            vec![
                "AA:BB:CC:DD:EE:FF",
                "192.168.50.255",
                "192.168.50.2",
                "extra",
            ],
        ] {
            assert_eq!(
                Destination::parse(&args(&values)).err().unwrap().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}
