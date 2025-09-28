use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use socket2::{Domain, Protocol, Socket, Type};

/// Creates and configures a UDP socket for TS packet reception
/// Handles both unicast and multicast addresses
pub fn create_udp_socket(addr: &str) -> anyhow::Result<Socket> {
    let sock_addr: SocketAddr = addr.parse()?;
    let ip = match sock_addr.ip() {
        IpAddr::V4(v4) => v4,
        _ => anyhow::bail!("only IPv4 is supported"),
    };

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&sock_addr.into())?;

    // Join multicast group if the address is multicast
    if ip.is_multicast() {
        let iface = Ipv4Addr::UNSPECIFIED; // default interface
        socket.join_multicast_v4(&ip, &iface)?;
    }

    socket.set_nonblocking(true)?;
    Ok(socket)
}