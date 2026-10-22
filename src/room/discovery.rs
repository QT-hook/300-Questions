use super::protocol::{Advert, VERSION};
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

pub const PORT: u16 = 42739;
const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 39);
const RETRY: Duration = Duration::from_secs(5);
const EXPIRY: Duration = Duration::from_secs(4);

#[derive(Serialize, Deserialize)]
enum Packet {
    Probe { version: u32, code: String },
    Room { version: u32, advert: Advert },
}

struct InterfaceSender {
    address: Ipv4Addr,
    broadcast: Ipv4Addr,
    socket: UdpSocket,
}

pub struct Discovery {
    // Replies must return to this persistent ephemeral port, not a shared listener.
    search: UdpSocket,
    responder: Option<UdpSocket>,
    listen_address: SocketAddr,
    targets: Vec<SocketAddr>,
    interfaces: Vec<InterfaceSender>,
    memberships: BTreeSet<Ipv4Addr>,
    multicast: Ipv4Addr,
    refresh_interfaces: bool,
    last_refresh: Option<Instant>,
    diagnostics: BTreeMap<String, String>,
    found: BTreeMap<String, (Advert, Instant)>,
}

fn search_socket(address: Ipv4Addr) -> io::Result<UdpSocket> {
    let socket = UdpSocket::bind((address, 0))?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

fn responder_socket(address: SocketAddr) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.bind(&address.into())?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

impl Discovery {
    pub fn new() -> io::Result<Self> {
        let mut discovery = Self::with_config(
            search_socket(Ipv4Addr::UNSPECIFIED)?,
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)),
            vec![
                (Ipv4Addr::BROADCAST, PORT).into(),
                (Ipv4Addr::LOCALHOST, PORT).into(),
                (GROUP, PORT).into(),
            ],
            GROUP,
            true,
        );
        discovery.maintain();
        Ok(discovery)
    }

    fn with_config(
        search: UdpSocket,
        listen_address: SocketAddr,
        targets: Vec<SocketAddr>,
        multicast: Ipv4Addr,
        refresh_interfaces: bool,
    ) -> Self {
        let broadcast = search.set_broadcast(true);
        let mut discovery = Self {
            search,
            responder: None,
            listen_address,
            targets,
            interfaces: Vec::new(),
            memberships: BTreeSet::new(),
            multicast,
            refresh_interfaces,
            last_refresh: None,
            diagnostics: BTreeMap::new(),
            found: BTreeMap::new(),
        };
        if let Err(error) = broadcast {
            discovery.record("search broadcast", error);
        }
        discovery
    }

    /// Persistent, deduplicated diagnostic snapshots; searching never clears them.
    pub fn diagnostics(&self) -> Vec<String> {
        self.diagnostics.values().cloned().collect()
    }

    pub fn responder_available(&self) -> bool {
        self.responder.is_some()
    }

    fn record(&mut self, operation: &str, error: impl std::fmt::Display) {
        self.diagnostics
            .insert(operation.to_owned(), format!("{operation}: {error}"));
    }

    fn maintain(&mut self) {
        if self.last_refresh.is_some_and(|at| at.elapsed() < RETRY) {
            return;
        }
        self.last_refresh = Some(Instant::now());
        if !self.responder_available() {
            match responder_socket(self.listen_address) {
                Ok(socket) => self.responder = Some(socket),
                Err(error) => self.record("discovery listener (search still available)", error),
            }
        }
        if self.refresh_interfaces {
            match if_addrs::get_if_addrs() {
                Ok(interfaces) => {
                    let mut addresses = BTreeMap::new();
                    for interface in interfaces {
                        if let if_addrs::IfAddr::V4(ip) = interface.addr
                            && !ip.ip.is_loopback()
                            && !ip.ip.is_unspecified()
                            && let Some(broadcast) = ip.broadcast
                            && broadcast != ip.ip
                        {
                            addresses.insert(ip.ip, broadcast);
                        }
                    }
                    self.interfaces
                        .retain(|sender| addresses.get(&sender.address) == Some(&sender.broadcast));
                    for (address, broadcast) in addresses {
                        if self
                            .interfaces
                            .iter()
                            .any(|sender| sender.address == address)
                        {
                            continue;
                        }
                        match search_socket(address).and_then(|socket| {
                            socket.set_broadcast(true)?;
                            Ok(socket)
                        }) {
                            Ok(socket) => self.interfaces.push(InterfaceSender {
                                address,
                                broadcast,
                                socket,
                            }),
                            Err(error) => {
                                self.record(&format!("interface sender {address}"), error)
                            }
                        }
                    }
                }
                Err(error) => self.record("interface enumeration", error),
            }
        }
        let mut desired: BTreeSet<_> = self.interfaces.iter().map(|s| s.address).collect();
        desired.insert(Ipv4Addr::UNSPECIFIED);
        if let Some(socket) = &self.responder {
            let mut errors = Vec::new();
            self.memberships.retain(|address| {
                if desired.contains(address) {
                    true
                } else {
                    if let Err(error) = socket.leave_multicast_v4(&self.multicast, address) {
                        errors.push((format!("leave multicast {address}"), error));
                    }
                    false
                }
            });
            let missing: Vec<_> = desired.difference(&self.memberships).copied().collect();
            for address in missing {
                match socket.join_multicast_v4(&self.multicast, &address) {
                    Ok(()) => {
                        self.memberships.insert(address);
                    }
                    Err(error) => errors.push((format!("join multicast {address}"), error)),
                }
            }
            for (operation, error) in errors {
                self.record(&operation, error);
            }
        }
    }

    pub fn probe(&mut self, code: &str) {
        self.maintain();
        let data = match serde_json::to_vec(&Packet::Probe {
            version: VERSION,
            code: code.into(),
        }) {
            Ok(data) => data,
            Err(error) => {
                self.record("encode probe", error);
                return;
            }
        };
        let mut errors = Vec::new();
        for target in &self.targets {
            if let Err(error) = self.search.send_to(&data, target) {
                errors.push((format!("probe {target}"), error));
            }
        }
        for sender in &self.interfaces {
            let target = SocketAddr::from((sender.broadcast, self.listen_address.port()));
            // Also try the wildcard socket if a platform cannot route the bound sender.
            for socket in [&sender.socket, &self.search] {
                if let Err(error) = socket.send_to(&data, target) {
                    errors.push((
                        format!("probe interface {} to {target}", sender.address),
                        error,
                    ));
                }
            }
        }
        for (operation, error) in errors {
            if !matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) {
                self.record(&operation, error);
            }
        }
    }

    pub fn poll(&mut self, advert: Option<&Advert>) {
        self.maintain();
        let mut errors = Vec::new();
        let mut listener_failed = false;
        let sockets = std::iter::once((&self.search, false))
            .chain(self.responder.iter().map(|socket| (socket, true)))
            .chain(self.interfaces.iter().map(|sender| (&sender.socket, false)));
        for (socket, listener) in sockets {
            let mut buf = [0; 2048];
            for _ in 0..64 {
                let (len, source) = match socket.recv_from(&mut buf) {
                    Ok(received) => received,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        errors.push((
                            if listener {
                                "receive listener"
                            } else {
                                "receive search"
                            },
                            error.to_string(),
                        ));
                        listener_failed |= listener;
                        break;
                    }
                };
                match serde_json::from_slice::<Packet>(&buf[..len]) {
                    Ok(Packet::Probe { version, code }) if version == VERSION => {
                        if let Some(ad) = advert.filter(|a| code.is_empty() || a.code == code) {
                            match serde_json::to_vec(&Packet::Room {
                                version: VERSION,
                                advert: ad.clone(),
                            }) {
                                Ok(data) => {
                                    if let Err(error) = socket.send_to(&data, source)
                                        && !matches!(
                                            error.kind(),
                                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                                        )
                                    {
                                        errors.push(("reply to probe", error.to_string()));
                                    }
                                }
                                Err(error) => errors.push(("encode advert", error.to_string())),
                            }
                        }
                    }
                    Ok(Packet::Room {
                        version,
                        mut advert,
                    }) if version == VERSION => {
                        if let Ok(address) = advert.endpoint.parse::<SocketAddr>()
                            && address.port() != 0
                            && advert.room_id.len() <= 64
                            && advert.name.len() <= 64
                        {
                            advert.endpoint =
                                SocketAddr::new(source.ip(), address.port()).to_string();
                            self.found
                                .insert(advert.room_id.clone(), (advert, Instant::now()));
                        }
                    }
                    _ => {}
                }
            }
        }
        for (operation, error) in errors {
            self.record(operation, error);
        }
        if listener_failed {
            self.responder = None;
            self.memberships.clear();
        }
        // Expire even when no socket is readable, or a receive failed.
        self.found.retain(|_, (_, at)| at.elapsed() < EXPIRY);
    }

    pub fn rooms(&self, code: &str) -> Vec<Advert> {
        self.found
            .values()
            .filter(|(a, at)| at.elapsed() < EXPIRY && (code.is_empty() || a.code == code))
            .map(|(a, _)| a.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discovery(targets: Vec<SocketAddr>, multicast: Ipv4Addr) -> Discovery {
        Discovery::with_config(
            search_socket(Ipv4Addr::LOCALHOST).unwrap(),
            (Ipv4Addr::LOCALHOST, 0).into(),
            targets,
            multicast,
            false,
        )
    }

    fn advert() -> Advert {
        Advert {
            room_id: "room".into(),
            name: "test".into(),
            code: "secret".into(),
            endpoint: "192.0.2.1:12345".into(),
            term: 1,
            host: "host".into(),
        }
    }

    fn exchange(host: &mut Discovery, searchers: &mut [&mut Discovery]) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            host.poll(Some(&advert()));
            for searcher in searchers.iter_mut() {
                searcher.poll(None);
            }
            if searchers.iter().all(|s| !s.rooms("").is_empty()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("unicast discovery timed out");
    }

    #[test]
    fn two_searchers_receive_unicast_replies_at_distinct_persistent_ports() {
        let mut host = discovery(vec![], GROUP);
        host.maintain();
        let address = host.responder.as_ref().unwrap().local_addr().unwrap();
        let mut first = discovery(vec![address], GROUP);
        let mut second = discovery(vec![address], GROUP);
        let first_address = first.search.local_addr().unwrap();
        assert_ne!(first_address, second.search.local_addr().unwrap());
        first.probe("secret");
        second.probe("secret");
        exchange(&mut host, &mut [&mut first, &mut second]);
        assert_eq!(first.search.local_addr().unwrap(), first_address);
        assert_eq!(first.rooms("")[0].endpoint, "127.0.0.1:12345");
        assert_eq!(second.rooms("secret").len(), 1);
    }

    #[test]
    fn listener_failure_does_not_prevent_search_and_retries() {
        let mut host = discovery(vec![], GROUP);
        host.maintain();
        let target = host.responder.as_ref().unwrap().local_addr().unwrap();
        let occupied = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut searcher = discovery(vec![target], GROUP);
        searcher.listen_address = occupied.local_addr().unwrap();
        searcher.probe("");
        assert!(!searcher.responder_available());
        exchange(&mut host, &mut [&mut searcher]);
        assert!(!searcher.diagnostics().is_empty());
        let diagnostics = searcher.diagnostics();
        drop(occupied);
        searcher.last_refresh = None;
        searcher.poll(None);
        assert!(searcher.responder_available());
        assert_eq!(searcher.diagnostics(), diagnostics);
    }

    #[test]
    fn multicast_failure_preserves_unicast_and_diagnostics() {
        let mut host = discovery(vec![], Ipv4Addr::LOCALHOST);
        host.maintain();
        assert!(host.responder_available());
        assert!(
            host.diagnostics()
                .iter()
                .any(|s| s.contains("join multicast"))
        );
        let target = host.responder.as_ref().unwrap().local_addr().unwrap();
        let mut searcher = discovery(vec![target], GROUP);
        searcher.probe("");
        exchange(&mut host, &mut [&mut searcher]);
        host.probe("");
        assert!(
            host.diagnostics()
                .iter()
                .any(|s| s.contains("join multicast"))
        );
    }

    #[test]
    fn cache_filters_codes_and_expires_without_incoming_packets() {
        let mut host = discovery(vec![], GROUP);
        host.maintain();
        let target = host.responder.as_ref().unwrap().local_addr().unwrap();
        let mut searcher = discovery(vec![target], GROUP);
        searcher.probe("secret");
        exchange(&mut host, &mut [&mut searcher]);
        assert!(searcher.rooms("other").is_empty());
        assert_eq!(searcher.rooms("secret").len(), 1);
        searcher.found.get_mut("room").unwrap().1 = Instant::now() - EXPIRY;
        assert!(searcher.rooms("").is_empty());
        searcher.poll(None);
        assert!(searcher.found.is_empty());
    }

    #[test]
    fn responder_does_not_reply_to_wrong_code() {
        let mut host = discovery(vec![], GROUP);
        host.maintain();
        let target = host.responder.as_ref().unwrap().local_addr().unwrap();
        let client = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let data = serde_json::to_vec(&Packet::Probe {
            version: VERSION,
            code: "wrong".into(),
        })
        .unwrap();
        client.send_to(&data, target).unwrap();
        host.poll(Some(&advert()));
        assert!(client.recv_from(&mut [0; 2048]).is_err());
    }
}
