use super::{
    discovery::Discovery,
    model::{Answer, Member, Mode, Room},
    protocol::{self, Advert},
    storage,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub enum Command {
    Create {
        name: String,
        code: String,
        player: String,
    },
    Discover {
        code: String,
    },
    Join {
        endpoint: String,
        room_id: Option<String>,
        player: String,
    },
    Configure {
        mode: Mode,
        indices: Vec<usize>,
    },
    Ready,
    Start,
    Answer {
        position: usize,
        choice: usize,
    },
    Leave,
}
#[derive(Clone, Debug)]
pub enum Event {
    Rooms(Vec<Advert>),
    State(Box<Room>),
    Status(String),
    Error(String),
    Diagnostics(Vec<String>),
    Left,
}
#[derive(Clone, Serialize, Deserialize)]
struct Packet {
    version: u32,
    #[serde(default = "unknown_version")]
    app_version: String,
    fingerprint: String,
    sender: Member,
    room_id: Option<String>,
    join: bool,
    room: Option<Room>,
    pending: Vec<Answer>,
}
#[derive(Serialize, Deserialize)]
struct Reply {
    room: Option<Room>,
    error: Option<String>,
}
struct Shared {
    room: Mutex<Option<Room>>,
    endpoint: Mutex<String>,
    stop: AtomicBool,
    connections: AtomicUsize,
}
pub struct Service {
    id: String,
    commands: SyncSender<Command>,
    events: Mutex<Receiver<Event>>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
fn id() -> String {
    format!("{:032x}", rand::random::<u128>())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
fn unknown_version() -> String {
    "0.1.0 (legacy)".into()
}
fn observed_endpoint(claimed: &str, peer: SocketAddr) -> Result<String, String> {
    let address: SocketAddr = claimed.parse().map_err(|_| "非法监听地址".to_owned())?;
    if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
        return Err("非法监听地址".into());
    }
    Ok(SocketAddr::new(peer.ip(), address.port()).to_string())
}
fn update_own_endpoint(own: &mut Member, stream: &TcpStream) -> Result<(), String> {
    let local = stream.local_addr().map_err(|e| e.to_string())?;
    own.endpoint = observed_endpoint(&own.endpoint, local)?;
    Ok(())
}
fn correct_snapshot(room: &mut Room, member: &str, endpoint: &str) {
    if let Some(m) = room.members.iter_mut().find(|m| m.id == member) {
        m.endpoint = endpoint.into();
    }
}
fn local_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:80")?;
            s.local_addr()
        })
        .map_or_else(|_| "127.0.0.1".into(), |a| a.ip().to_string())
}
impl Service {
    pub fn new() -> Result<Self, String> {
        Self::with_address("0.0.0.0:0", None)
    }
    fn with_address(bind: &str, path: Option<PathBuf>) -> Result<Self, String> {
        crate::questions::embedded_fingerprint()?;
        let listener = TcpListener::bind(bind).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let endpoint = format!(
            "{}:{port}",
            if bind.starts_with("127.") {
                "127.0.0.1".into()
            } else {
                local_ip()
            }
        );
        let id = id();
        let own = Member {
            id: id.clone(),
            name: String::new(),
            endpoint: endpoint.clone(),
            online: true,
            ready: false,
        };
        let (commands, rx) = mpsc::sync_channel(64);
        let (tx, events) = mpsc::sync_channel(128);
        let shared = Arc::new(Shared {
            room: Mutex::new(None),
            endpoint: Mutex::new(endpoint.clone()),
            stop: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
        });
        let state = shared.clone();
        let path = path.unwrap_or_else(|| {
            #[cfg(windows)]
            let base = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData/Local"))
                })
                .unwrap_or_else(std::env::temp_dir);
            #[cfg(not(windows))]
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
                .unwrap_or_else(std::env::temp_dir);
            base.join("dioxus-app").join(format!("{id}.json"))
        });
        let worker = thread::spawn(move || run(listener, own, rx, tx, state, path));
        Ok(Self {
            id,
            commands,
            events: Mutex::new(events),
            shared,
            worker: Some(worker),
        })
    }
    #[cfg(test)]
    pub(super) fn test_node() -> Self {
        Self::with_address(
            "127.0.0.1:0",
            Some(std::env::temp_dir().join(format!("dioxus-room-test-{}.json", id()))),
        )
        .unwrap()
    }
    #[cfg(test)]
    pub(super) fn test_state(&self) -> Option<Room> {
        self.shared.room.lock().unwrap().clone()
    }
    #[cfg(test)]
    pub(super) fn test_crash(mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.worker.take() {
            t.join().unwrap();
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn endpoint(&self) -> String {
        self.shared
            .endpoint
            .lock()
            .map_or_else(|_| "地址不可用".into(), |endpoint| endpoint.clone())
    }
    pub fn command(&self, c: Command) -> Result<(), String> {
        self.commands.try_send(c).map_err(|e| e.to_string())
    }
    pub fn poll(&self) -> Vec<Event> {
        self.events
            .lock()
            .map_or_else(|_| Vec::new(), |rx| rx.try_iter().collect())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.commands.try_send(Command::Leave);
        thread::sleep(Duration::from_millis(60));
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn emit(tx: &SyncSender<Event>, event: Event) {
    let _ = tx.try_send(event);
}
fn connect(endpoint: &str) -> Result<TcpStream, String> {
    let address: SocketAddr = endpoint.parse().map_err(|_| "请输入 IP:端口".to_owned())?;
    let s = TcpStream::connect_timeout(&address, Duration::from_millis(100))
        .map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    s.set_write_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    let _ = s.set_nodelay(true);
    Ok(s)
}
fn exchange(stream: &mut TcpStream, p: &Packet) -> Result<Reply, String> {
    protocol::write_message(stream, p)
        .and_then(|()| protocol::read_message(stream))
        .map_err(|e| e.to_string())
}
fn run(
    listener: TcpListener,
    mut own: Member,
    commands: Receiver<Command>,
    events: SyncSender<Event>,
    shared: Arc<Shared>,
    path: PathBuf,
) {
    let (incoming_tx, incoming) = mpsc::sync_channel::<Packet>(128);
    let (hash, bank_len) = match crate::questions::embedded_fingerprint()
        .and_then(|hash| crate::questions::embedded().map(|bank| (hash, bank.len())))
    {
        Ok(value) => value,
        Err(error) => {
            emit(&events, Event::Error(error));
            return;
        }
    };
    let mut discovery = Discovery::new()
        .map_err(|e| emit(&events, Event::Error(format!("发现不可用，请手动 IP：{e}"))))
        .ok();
    let mut discovery_retry = Instant::now();
    let mut searching_since = None;
    let mut receipts = std::collections::BTreeSet::<String>::new();
    let mut room: Option<Room> = None;
    let mut pending = Vec::<Answer>::new();
    let mut streams = BTreeMap::<String, TcpStream>::new();
    let mut seen = BTreeMap::<String, Instant>::new();
    let mut code = String::new();
    let mut last = Instant::now() - Duration::from_secs(2);
    let mut published = None;
    let mut handlers = Vec::new();
    while !shared.stop.load(Ordering::Relaxed) {
        handlers.retain(|handle: &JoinHandle<()>| !handle.is_finished());
        for _ in 0..32 {
            let (mut socket, peer) = match listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    emit(&events, Event::Error(format!("TCP 监听失败：{error}")));
                    break;
                }
            };
            if shared.connections.fetch_add(1, Ordering::Relaxed) >= 32 {
                shared.connections.fetch_sub(1, Ordering::Relaxed);
                continue;
            }
            let shared = shared.clone();
            let tx = incoming_tx.clone();
            let hash = hash.clone();
            let diagnostics = events.clone();
            handlers.push(thread::spawn(move || {
                let _ = socket.set_read_timeout(Some(Duration::from_millis(1500)));
                let _ = socket.set_write_timeout(Some(Duration::from_millis(200)));
                let mut last_rejection = None;
                while !shared.stop.load(Ordering::Relaxed) {
                    let Ok(mut packet) = protocol::read_message::<Packet>(&mut socket) else {
                        break;
                    };
                    let error = if let Some(error) = protocol::compatibility_error(
                        packet.version,
                        &packet.app_version,
                        &packet.fingerprint,
                        &hash,
                    ) {
                        Some(error)
                    } else if packet.sender.name.trim().is_empty()
                        || packet.sender.name.chars().count() > 9
                        || observed_endpoint(&packet.sender.endpoint, peer).is_err()
                        || packet.pending.len() > 300
                        || packet
                            .room
                            .as_ref()
                            .is_some_and(|r| r.validate(bank_len).is_err())
                    {
                        Some("非法房间数据".into())
                    } else {
                        None
                    };
                    if let Some(message) = &error
                        && last_rejection
                            .is_none_or(|at: Instant| at.elapsed() > Duration::from_secs(5))
                    {
                        emit(
                            &diagnostics,
                            Event::Error(format!("拒绝 {peer}：{message}")),
                        );
                        last_rejection = Some(Instant::now());
                    }
                    if error.is_none() {
                        if let Ok(endpoint) = observed_endpoint(&packet.sender.endpoint, peer) {
                            packet.sender.endpoint = endpoint;
                        }
                        if tx.try_send(packet).is_err() {
                            break;
                        }
                    }
                    let snapshot = shared.room.lock().ok().and_then(|r| r.clone());
                    if protocol::write_message(
                        &mut socket,
                        &Reply {
                            room: snapshot,
                            error,
                        },
                    )
                    .is_err()
                    {
                        break;
                    }
                }
                shared.connections.fetch_sub(1, Ordering::Relaxed);
            }));
        }
        for command in commands.try_iter() {
            let result: Result<(), String> = (|| {
                match command {
                    Command::Create { name, code, player } => {
                        own.name = player.trim().into();
                        own.ready = true;
                        room = Some(Room::new(
                            name.trim().into(),
                            code,
                            own.clone(),
                            hash.clone(),
                        )?);
                        streams.clear();
                        seen.clear();
                        let message = if discovery.as_ref().is_none_or(|d| !d.responder_available())
                        {
                            "房间已创建，但 UDP 发现监听不可用；请让其他成员手动连接本机 TCP 地址"
                        } else {
                            "房间已创建"
                        };
                        emit(&events, Event::Status(message.into()));
                    }
                    Command::Discover { code: c } => {
                        code = c;
                        searching_since = Some(Instant::now());
                        if let Some(d) = &mut discovery {
                            d.probe(&code);
                        }
                        emit(
                            &events,
                            Event::Status("搜索房间中；也可手动连接 IP:端口".into()),
                        );
                    }
                    Command::Join {
                        endpoint,
                        room_id,
                        player,
                    } => {
                        own.name = player.trim().into();
                        own.ready = false;
                        let mut s = connect(&endpoint)?;
                        update_own_endpoint(&mut own, &s)?;
                        let reply = exchange(
                            &mut s,
                            &Packet {
                                version: protocol::VERSION,
                                app_version: env!("CARGO_PKG_VERSION").into(),
                                fingerprint: hash.clone(),
                                sender: own.clone(),
                                room_id,
                                join: true,
                                room: None,
                                pending: vec![],
                            },
                        )?;
                        if let Some(e) = reply.error {
                            return Err(e);
                        }
                        let mut state = reply.room.ok_or("对方没有房间")?;
                        let host = state.host.clone();
                        correct_snapshot(&mut state, &host, &endpoint);
                        correct_snapshot(&mut state, &own.id, &own.endpoint);
                        if state.fingerprint != hash {
                            return Err("题库不一致".into());
                        }
                        state.validate(bank_len)?;
                        if state.started && !state.members.iter().any(|m| m.id == own.id) {
                            return Err("已经开始，不能加入新成员".into());
                        }
                        room = Some(state);
                        streams.insert(endpoint, s);
                        emit(&events, Event::Status("连接中，等待成员确认".into()));
                    }
                    Command::Configure { mode, indices } => {
                        let r = room.as_mut().ok_or("没有房间")?;
                        if r.host != own.id || r.started {
                            return Err("只有房主可修改等待室".into());
                        }
                        if indices.is_empty()
                            || indices.len() > bank_len
                            || indices.iter().any(|i| *i >= bank_len)
                        {
                            return Err("题序不合法".into());
                        }
                        r.mode = mode;
                        r.indices = indices;
                    }
                    Command::Ready => {
                        own.ready = true;
                        if let Some(r) = &mut room
                            && let Some(m) = r.members.iter_mut().find(|m| m.id == own.id)
                        {
                            m.ready = true;
                        }
                    }
                    Command::Start => {
                        let r = room.as_mut().ok_or("没有房间")?;
                        if r.host != own.id {
                            return Err("只有房主可开始".into());
                        }
                        r.start(r.indices.clone(), r.mode, bank_len)?;
                    }
                    Command::Answer { position, choice } => {
                        let r = room.as_mut().ok_or("没有房间")?;
                        let a = Answer {
                            member: own.id.clone(),
                            position,
                            choice,
                            submission: id(),
                        };
                        if r.submit(a.clone(), bank_len)? {
                            pending.push(a);
                            emit(
                                &events,
                                Event::Status(
                                    "答案已记录本机，等待其他成员实际收到；单人时无远端副本".into(),
                                ),
                            );
                        }
                    }
                    Command::Leave => {
                        if let Some(r) = &mut room {
                            if let Some(m) = r.members.iter_mut().find(|m| m.id == own.id) {
                                m.online = false;
                            }
                            if r.host == own.id
                                && let Some(next) = r
                                    .members
                                    .iter()
                                    .filter(|m| m.online)
                                    .map(|m| m.id.clone())
                                    .min()
                            {
                                r.host = next;
                                r.term += 1;
                            }
                            if let Ok(mut guard) = shared.room.lock() {
                                *guard = Some(r.clone());
                            }
                            let packet = Packet {
                                version: protocol::VERSION,
                                app_version: env!("CARGO_PKG_VERSION").into(),
                                fingerprint: hash.clone(),
                                sender: Member {
                                    online: false,
                                    ..own.clone()
                                },
                                room_id: Some(r.id.clone()),
                                join: false,
                                room: Some(r.clone()),
                                pending: pending.clone(),
                            };
                            for s in streams.values_mut() {
                                let _ = exchange(s, &packet);
                            }
                            let _ = storage::save(&path, r, &pending);
                        }
                        room = None;
                        pending.clear();
                        streams.clear();
                        published = None;
                        emit(&events, Event::Left);
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                emit(&events, Event::Error(e));
            }
        }
        for packet in incoming.try_iter() {
            let Some(r) = &mut room else { continue };
            if packet.join {
                if packet.room_id.as_ref().is_some_and(|id| id != &r.id) {
                    continue;
                }
                if r.host == own.id
                    && let Err(e) = r.add_member(packet.sender.clone())
                {
                    emit(&events, Event::Error(e));
                }
            } else if packet.room_id.as_ref() != Some(&r.id) {
                continue;
            }
            if !r.members.iter().any(|m| m.id == packet.sender.id) {
                continue;
            }
            seen.insert(packet.sender.id.clone(), Instant::now());
            if let Some(other) = packet.room {
                for a in &other.answers {
                    receipts.insert(a.submission.clone());
                }
                if let Err(e) = r.merge(&other) {
                    emit(&events, Event::Error(e));
                }
            }
            if let Some(m) = r.members.iter_mut().find(|m| m.id == packet.sender.id) {
                m.ready = packet.sender.ready;
                m.online = packet.sender.online;
                m.endpoint = packet.sender.endpoint;
            }
            for a in packet.pending {
                let _ = r.submit(a, bank_len);
            }
        }
        if last.elapsed() > Duration::from_millis(300) {
            last = Instant::now();
            if let Some(r) = &mut room {
                // Every member has a listener from service creation, so election endpoints never refer to ephemeral accepted ports.
                let mut packet = Packet {
                    version: protocol::VERSION,
                    app_version: env!("CARGO_PKG_VERSION").into(),
                    fingerprint: hash.clone(),
                    sender: own.clone(),
                    room_id: Some(r.id.clone()),
                    join: !r.members.iter().any(|m| m.id == own.id),
                    room: Some(r.clone()),
                    pending: pending.clone(),
                };
                let targets: Vec<_> = r
                    .members
                    .iter()
                    .filter(|m| m.id != own.id)
                    .map(|m| (m.id.clone(), m.endpoint.clone()))
                    .collect();
                for (member, endpoint) in targets {
                    if !streams.contains_key(&endpoint)
                        && let Ok(s) = connect(&endpoint)
                    {
                        streams.insert(endpoint.clone(), s);
                    }
                    let result = streams.get_mut(&endpoint).map(|s| {
                        update_own_endpoint(&mut own, s)?;
                        packet.sender = own.clone();
                        if let Some(snapshot) = &mut packet.room {
                            correct_snapshot(snapshot, &own.id, &own.endpoint);
                        }
                        exchange(s, &packet)
                    });
                    match result {
                        Some(Ok(reply)) => {
                            seen.insert(member.clone(), Instant::now());
                            if let Some(e) = reply.error {
                                emit(&events, Event::Error(e));
                            } else if let Some(mut other) = reply.room
                                && other.fingerprint == hash
                                && other.validate(bank_len).is_ok()
                            {
                                correct_snapshot(&mut other, &member, &endpoint);
                                correct_snapshot(&mut other, &own.id, &own.endpoint);
                                for a in &other.answers {
                                    receipts.insert(a.submission.clone());
                                }
                                let _ = r.merge(&other);
                            }
                        }
                        _ => {
                            streams.remove(&endpoint);
                        }
                    }
                }
                for m in &mut r.members {
                    if m.id == own.id {
                        m.online = true;
                        m.ready = own.ready;
                        m.endpoint = own.endpoint.clone();
                    } else {
                        m.online = seen
                            .get(&m.id)
                            .is_some_and(|t| t.elapsed() < Duration::from_millis(1800));
                    }
                }
                if !r.members.iter().any(|m| m.id == r.host && m.online)
                    && let Some(candidate) = r
                        .members
                        .iter()
                        .filter(|m| m.online)
                        .map(|m| m.id.clone())
                        .min()
                {
                    r.host = candidate;
                    r.term += 1;
                    emit(
                        &events,
                        Event::Status("房主已接替，保留已收到的答案".into()),
                    );
                }
                if r.host == own.id {
                    r.tick(now());
                }
                pending.retain(|a| !receipts.contains(&a.submission) && a.member == own.id);
            }
            if discovery.is_none() && discovery_retry.elapsed() > Duration::from_secs(5) {
                discovery_retry = Instant::now();
                discovery = Discovery::new()
                    .map_err(|error| {
                        emit(
                            &events,
                            Event::Error(format!("发现初始化失败（将重试）：{error}")),
                        )
                    })
                    .ok();
            }
            let mut diagnostics = vec![format!(
                "本机 v{} · 协议 {} · 题库 {} · TCP监听 {}",
                env!("CARGO_PKG_VERSION"),
                protocol::VERSION,
                protocol::short_hash(&hash),
                own.endpoint
            )];
            if let Some(d) = &mut discovery {
                let advert = room.as_ref().filter(|r| r.host == own.id).map(|r| Advert {
                    room_id: r.id.clone(),
                    name: r.name.clone(),
                    code: r.code.clone(),
                    endpoint: own.endpoint.clone(),
                    term: r.term,
                    host: r.host.clone(),
                });
                d.poll(advert.as_ref());
                d.probe(&code);
                emit(&events, Event::Rooms(d.rooms(&code)));
                diagnostics.extend(d.diagnostics());
                if searching_since.is_some_and(|at| at.elapsed() > Duration::from_secs(3)) {
                    if d.rooms(&code).is_empty() {
                        emit(
                            &events,
                            Event::Status("未找到房间；请检查网络诊断或手动 IP:端口连接".into()),
                        );
                    }
                    searching_since = None;
                }
            } else {
                diagnostics.push("发现初始化不可用，将重试；仍可手动 TCP 连接".into());
            }
            emit(&events, Event::Diagnostics(diagnostics));
        }
        if let Ok(mut endpoint) = shared.endpoint.lock() {
            *endpoint = own.endpoint.clone();
        }
        if room != published {
            if let Some(r) = &room {
                if let Err(e) = storage::save(&path, r, &pending) {
                    emit(&events, Event::Error(format!("保存失败：{e}")));
                }
                emit(&events, Event::State(Box::new(r.clone())));
            }
            published = room.clone();
        }
        if let Ok(mut guard) = shared.room.lock() {
            *guard = room.clone();
        }
        thread::sleep(Duration::from_millis(15));
    }
    streams.clear();
    for handler in handlers {
        let _ = handler.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_observed_peer_preserves_claimed_listener_port() {
        assert_eq!(
            observed_endpoint("10.9.0.1:42740", "192.168.4.2:59876".parse().unwrap()).unwrap(),
            "192.168.4.2:42740"
        );
    }

    #[test]
    fn endpoint_rejects_malformed_and_invalid_listener() {
        for claimed in ["bad", "127.0.0.1:0", "0.0.0.0:123", "239.1.1.1:123"] {
            assert!(observed_endpoint(claimed, "127.0.0.1:55555".parse().unwrap()).is_err());
        }
    }

    #[test]
    fn endpoint_outgoing_uses_connection_local_ip_not_source_port() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (_accepted, _) = listener.accept().unwrap();
        let mut own = Member {
            id: "own".into(),
            name: "me".into(),
            endpoint: "10.9.0.1:12345".into(),
            online: true,
            ready: false,
        };
        update_own_endpoint(&mut own, &stream).unwrap();
        assert_eq!(own.endpoint, "127.0.0.1:12345");
        let mut room = Room::new("test".into(), "1234".into(), own.clone(), "hash".into()).unwrap();
        correct_snapshot(&mut room, &own.id, "127.0.0.1:23456");
        assert_eq!(room.members[0].endpoint, "127.0.0.1:23456");
    }

    #[test]
    fn compatibility_distinguishes_protocol_and_bank() {
        let protocol = protocol::compatibility_error(1, "0.1.0", "old", "new").unwrap();
        assert!(
            protocol.contains("协议版本不一致")
                && protocol.contains("0.1.0")
                && protocol.contains("new")
        );
        let bank = protocol::compatibility_error(2, "0.1.1", "old", "new").unwrap();
        assert!(bank.contains("题库指纹不一致"));
        assert_eq!(
            protocol::compatibility_error(2, "0.1.1", "same", "same"),
            None
        );
    }
}
