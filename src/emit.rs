use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use crate::profile::ResolvedConfig;

pub struct TestEmitter {
    socket: UdpSocket,
    target: SocketAddrV4,
}

impl TestEmitter {
    pub fn new(cfg: &ResolvedConfig) -> Result<Self> {
        if !cfg.emit.allowed {
            bail!("test emission was not enabled at launch; relaunch with --allow-emit");
        }
        if cfg.emit.group == cfg.stream.group && cfg.emit.port == cfg.stream.dst_port {
            bail!("safety boundary: output destination equals input destination");
        }

        let interface = cfg.emit.interface_ip.as_deref()
            .map(Ipv4Addr::from_str)
            .transpose()
            .context("invalid TEST multicast interface IPv4 address")?;
        let bind_ip = interface.unwrap_or(Ipv4Addr::UNSPECIFIED);

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.bind(&SockAddr::from(SocketAddrV4::new(bind_ip, 0)))?;
        socket.set_multicast_ttl_v4(1)?;
        socket.set_multicast_loop_v4(false)?;
        if let Some(iface) = interface {
            socket.set_multicast_if_v4(&iface)?;
        }
        let socket: UdpSocket = socket.into();

        let group = Ipv4Addr::from_str(&cfg.emit.group).context("invalid TEST multicast group")?;
        if !group.is_multicast() { bail!("TEST output address {} is not multicast", cfg.emit.group); }
        Ok(Self { socket, target: SocketAddrV4::new(group, cfg.emit.port) })
    }

    pub fn send(&self, raw: &[u8]) -> Result<usize> {
        Ok(self.socket.send_to(raw, self.target)?)
    }
}
