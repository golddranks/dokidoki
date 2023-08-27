use std::{
    net::{IpAddr, ToSocketAddrs, SocketAddr},
    str::from_utf8,
    time::{Duration, Instant},
};

use argh::FromArgs;

use pnet::{
    datalink::NetworkInterface,
    packet::{ip::IpNextHeaderProtocols, udp::{self, UdpPacket}, Packet},
    transport::{transport_channel, udp_packet_iter, TransportChannelType, TransportProtocol, TransportSender, TransportReceiver, UdpTransportChannelIterator},
};

const DEFAULT_PORT: u16 = 17178;
const MAX_IP: usize = 65535;
const DEF_MTU: usize = 1500;

/*
fn joo() -> Result<(), Box<dyn Error>> {
    let my_addr = "2404:7a80:9621:7100:a98e:5d1e:242d:ec8d:1234".to_socket_addrs()?
        .into_iter()
        .next()
        .ok_or("no domain found")?
        .ip();

    let peer_addr = "mon.lan:1234"
        .to_socket_addrs()?
        .into_iter()
        .next()
        .ok_or("no domain found")?
        .ip();

    println!("My: {}, Peer: {}", my_addr, peer_addr);

    let udp = IpNextHeaderProtocols::Udp;
    let (mut sx, mut rx) = transport_channel(
        MAX_IP,
        TransportChannelType::Layer4(
            match peer_addr {
                IpAddr::V4(_) => TransportProtocol::Ipv4(udp),
                IpAddr::V6(_) => TransportProtocol::Ipv6(udp),
            }
        ),
    )?;




    let ip_buf = &mut [0_u8; MAX_IP];

    let mut packet = pnet::packet::udp::MutableUdpPacket::new(&mut ip_buf[0..12]).ok_or("mäh TODO")?;
    packet.set_source(1234);
    packet.set_destination(1234);
    packet.set_length(12);
    packet.set_payload(b"haha");
    println!("{:?}", packet.to_immutable().packet());
    let checksum = match (peer_addr, my_addr) {
        (IpAddr::V4(pa), IpAddr::V4(my)) =>
            udp::ipv4_checksum(&packet.to_immutable(), &my, &pa),
        (IpAddr::V6(pa), IpAddr::V6(my)) =>
            udp::ipv6_checksum(&packet.to_immutable(), &my, &pa),
        _ => unreachable!(),
    };
    packet.set_checksum(checksum);
    println!("{:?}", packet);
    sx.send_to(packet, peer_addr)?;

    println!("sent");

    let mut rx_iter = udp_packet_iter(&mut rx);

    while let Ok((packet, addr)) = rx_iter.next() {
        println!("Rx! {:?}, {}", from_utf8(packet.payload())?, addr);
    }

    Ok(())
} */

fn logic<'a, 'state>(state: &'a mut State<'state>, time: Instant) -> IoOp<'a, 'state> {
    IoOp::Receive(Duration::from_secs(1), &mut state.packet)
}

#[derive(Debug)]
enum IoOp<'a, 'state> {
    Log(LogMsg),
    Resolve(&'a str, &'a mut Option<SocketAddr>),
    Send(u8),
    Receive(Duration, &'a mut Option<(UdpPacket<'state>, IpAddr)>),
}

#[derive(Debug)]
struct State<'state> {
    peers: Vec<Peer>,
    my_iface: NetworkInterface,
    my_addr: IpAddr,
    packet: Option<(UdpPacket<'state>, IpAddr)>,
}

struct Io<'a> {
    sx: TransportSender,
    rx: UdpTransportChannelIterator<'a>,
}

#[derive(Debug)]
enum Error {
    NoInterfaceWithSpecifiedNameFound,
    NoInterfaceWithSpecifiedIpFound,
    NoGlobalIpFound,
    NoInterfacesWithGlobalIpsFound,
    InvalidIpAddress,
    DnsResolveFailed,
    ErrorOpeningSocket,
    UdpReceiveFailed,
}

impl Io<'_> {
    fn log(&self, msg: LogMsg) {
        println!("{:?}", msg)
    }

    fn resolve(dns: &str) -> Result<SocketAddr, Error> {
        dns.to_socket_addrs().or(Err(Error::DnsResolveFailed))?.into_iter().next().ok_or(Error::DnsResolveFailed)
    }

    fn get_iface(args: &Args) -> Result<(NetworkInterface, IpAddr), Error> {
        fn get_addr(iface: &NetworkInterface) -> Option<IpAddr> {
            iface.ips.iter().find(|inet| match inet.ip() {
                IpAddr::V4(_) => true,
                IpAddr::V6(ip) => (ip.segments()[0] & 0xffc0) != 0xfe80,
            }).map(|ipnet| ipnet.ip())
        }
        if let Some(arg_iface) = &args.iface {
            for iface in pnet::datalink::interfaces() {
                if iface.name == *arg_iface {
                    if let Some(addr) = get_addr(&iface) {
                        return Ok((iface, addr))
                    } else {
                        return Err(Error::NoGlobalIpFound)
                    }
                }
            }
            return Err(Error::NoInterfaceWithSpecifiedNameFound)
        } else if let Some(arg_ip) = &args.ip {
            let arg_ip: IpAddr = arg_ip.parse().or(Err(Error::InvalidIpAddress))?;
            for iface in pnet::datalink::interfaces() {
                if let Some(ipnet) = iface.ips.iter().find(|ipnet| ipnet.ip() == arg_ip) {
                    let addr = ipnet.ip();
                    return Ok((iface, addr))
                }
            }
            return Err(Error::NoInterfaceWithSpecifiedIpFound)
        } else {
            for iface in pnet::datalink::interfaces() {
                if iface.is_up() && !iface.is_loopback() {
                    if let Some(addr) = get_addr(&iface) {
                        return Ok((iface, addr))
                    }
                }
            }
            return Err(Error::NoInterfacesWithGlobalIpsFound)
        }
    }

    fn receive(&mut self, dur: Duration) -> Result<Option<()>, Error> {
        println!("receive!");
        dbg!(self.rx.next_with_timeout(dur).or(Err(Error::UdpReceiveFailed)).map(|a| a.map(|a| println!("{:?}", a))))
    }

    fn time() -> Instant {
        std::time::Instant::now()
    }
}

#[derive(Debug)]
enum LogMsg {
    SentRequest(),
    ReceivedRequest(),
    SentResponse(),
    ReceivedResponse(),
    ResolvedDns(),
}

#[derive(FromArgs)]
/// Juuh
struct Args {
    /// interface
    #[argh(option)]
    iface: Option<String>,

    /// my IP
    #[argh(option)]
    ip: Option<String>,

    /// peers
    #[argh(positional)]
    peers: Vec<String>,
}

#[derive(Debug)]
struct Peer {
    dns: Option<String>,
    addr: Option<SocketAddr>,
}

fn main() -> Result<(), Error> {
    let args: Args = argh::from_env();
    let (my_iface, my_addr) = Io::get_iface(&args)?;
    let udp = IpNextHeaderProtocols::Udp;
    let (sx, mut rx) = transport_channel(
        MAX_IP,
        TransportChannelType::Layer4(
            match my_addr {
                IpAddr::V4(_) => TransportProtocol::Ipv4(udp),
                IpAddr::V6(_) => TransportProtocol::Ipv6(udp),
            }
        ),
    ).or(Err(Error::ErrorOpeningSocket))?;
    let mut io = Io {
        sx,
        rx: udp_packet_iter(&mut rx),
    };
    let mut peers = Vec::with_capacity(args.peers.len());
    for mut peer in args.peers {
        if peer.rsplit_once(":").is_none() {
            peer = format!("{peer}:{DEFAULT_PORT}");
        }
        peers.push(

            if let Ok(addr) = peer.parse::<SocketAddr>() {
                Peer {
                    dns: None,
                    addr: Some(addr),
                }
            } else {
                let addr = Io::resolve(&peer).ok();
                Peer {
                    dns: Some(peer),
                    addr,
                }
            }
        )
    }
    let mut state = State {
        peers,
        my_iface,
        my_addr,
        packet: None,
    };
    dbg!(&state);
    loop {
        match logic(&mut state, Io::time()) {
            IoOp::Log(msg) => io.log(msg),
            IoOp::Resolve(dns, target) => *target = Io::resolve(dns).ok(),
            IoOp::Send(_) => todo!(),
            IoOp::Receive(dur, target) => { io.receive(dur); },
        }
    }
    Ok(())
}
