use super::protocol::{Advert, VERSION};
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    collections::BTreeMap,
    io,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    time::{Duration, Instant},
};
pub const PORT: u16 = 42739;
#[derive(Serialize, Deserialize)]
enum Packet {
    Probe { version: u32, code: String },
    Room { version: u32, advert: Advert },
}
pub struct Discovery {
    socket: UdpSocket,
    found: BTreeMap<String, (Advert, Instant)>,
}
impl Discovery {
    pub fn new() -> io::Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
        socket.set_broadcast(true)?;
        socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)).into())?;
        socket.set_nonblocking(true)?;
        let socket: UdpSocket = socket.into();
        socket.join_multicast_v4(&Ipv4Addr::new(239, 255, 42, 39), &Ipv4Addr::UNSPECIFIED)?;
        Ok(Self {
            socket,
            found: BTreeMap::new(),
        })
    }
    pub fn probe(&self, code: &str) {
        if let Ok(data) = serde_json::to_vec(&Packet::Probe {
            version: VERSION,
            code: code.into(),
        }) {
            let _ = self.socket.send_to(&data, (Ipv4Addr::BROADCAST, PORT));
            let _ = self
                .socket
                .send_to(&data, (Ipv4Addr::new(239, 255, 42, 39), PORT));
        }
    }
    pub fn poll(&mut self, advert: Option<&Advert>) {
        let mut buf = [0; 2048];
        for _ in 0..64 {
            let Ok((len, source)) = self.socket.recv_from(&mut buf) else {
                break;
            };
            match serde_json::from_slice::<Packet>(&buf[..len]) {
                Ok(Packet::Probe { version, code }) if version == VERSION => {
                    if let Some(ad) = advert.filter(|a| code.is_empty() || a.code == code)
                        && let Ok(data) = serde_json::to_vec(&Packet::Room {
                            version: VERSION,
                            advert: ad.clone(),
                        })
                    {
                        let _ = self.socket.send_to(&data, source);
                        let _ = self
                            .socket
                            .send_to(&data, (Ipv4Addr::new(239, 255, 42, 39), PORT));
                    }
                }
                Ok(Packet::Room {
                    version,
                    mut advert,
                }) if version == VERSION => {
                    if let Ok(address) = advert.endpoint.parse::<SocketAddr>() {
                        advert.endpoint = SocketAddr::new(source.ip(), address.port()).to_string();
                        if advert.room_id.len() <= 64 && advert.name.len() <= 64 {
                            self.found
                                .insert(advert.room_id.clone(), (advert, Instant::now()));
                        }
                    }
                }
                _ => {}
            }
        }
        self.found
            .retain(|_, (_, at)| at.elapsed() < Duration::from_secs(4));
    }
    pub fn rooms(&self, code: &str) -> Vec<Advert> {
        self.found
            .values()
            .filter(|(a, _)| code.is_empty() || a.code == code)
            .map(|(a, _)| a.clone())
            .collect()
    }
}
