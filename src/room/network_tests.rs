use super::{
    model::{Mode, Room},
    service::{Command, Event, Service},
};
use std::{
    thread,
    time::{Duration, Instant},
};
fn node() -> Service {
    Service::test_node()
}
fn state(s: &Service) -> Option<Room> {
    s.test_state()
}
fn wait(mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(
            start.elapsed() < Duration::from_secs(12),
            "network condition timed out"
        );
        thread::sleep(Duration::from_millis(30));
    }
}
fn setup(count: usize, mode: Mode) -> Vec<Service> {
    let nodes: Vec<_> = (0..count).map(|_| node()).collect();
    nodes[0]
        .command(Command::Create {
            name: "测试房间".into(),
            code: "1234".into(),
            player: "房主".into(),
        })
        .unwrap();
    wait(|| state(&nodes[0]).is_some());
    for n in &nodes[1..] {
        n.command(Command::Join {
            endpoint: nodes[0].endpoint(),
            room_id: None,
            player: "成员".into(),
        })
        .unwrap();
    }
    wait(|| {
        nodes
            .iter()
            .all(|n| state(n).is_some_and(|r| r.members.len() == count))
    });
    for n in &nodes {
        n.command(Command::Ready).unwrap();
    }
    wait(|| state(&nodes[0]).is_some_and(|r| r.members.iter().all(|m| m.ready)));
    nodes[0]
        .command(Command::Configure {
            mode,
            indices: vec![0, 1],
        })
        .unwrap();
    nodes[0].command(Command::Start).unwrap();
    wait(|| nodes.iter().all(|n| state(n).is_some_and(|r| r.started)));
    nodes
}
#[test]
fn independent_three_peers_graceful_host_loss_preserves_finished_and_late_scores() {
    let mut nodes = setup(3, Mode::Independent);
    for p in 0..2 {
        nodes[0]
            .command(Command::Answer {
                position: p,
                choice: 0,
            })
            .unwrap();
        wait(|| {
            nodes
                .iter()
                .all(|n| state(n).is_some_and(|r| r.answers.len() == p + 1))
        });
    }
    let old = nodes.remove(0);
    let host = old.id().to_owned();
    drop(old);
    wait(|| {
        nodes
            .iter()
            .all(|n| state(n).is_some_and(|r| r.host != host))
    });
    for n in &nodes {
        for p in 0..2 {
            n.command(Command::Answer {
                position: p,
                choice: 0,
            })
            .unwrap();
            wait(|| {
                state(n).is_some_and(|r| {
                    r.answers
                        .iter()
                        .any(|a| a.member == n.id() && a.position == p)
                })
            });
        }
    }
    wait(|| {
        nodes.iter().all(|n| {
            state(n).is_some_and(|r| {
                r.answers.len() == 6 && r.members.iter().all(|m| r.completed(&m.id))
            })
        })
    });
}
#[test]
fn sync_two_peers_crash_last_survivor_completes() {
    let mut nodes = setup(2, Mode::Sync);
    for n in &nodes {
        n.command(Command::Answer {
            position: 0,
            choice: 0,
        })
        .unwrap();
    }
    wait(|| {
        nodes
            .iter()
            .all(|n| state(n).is_some_and(|r| r.answers.len() == 2))
    });
    let old = nodes.remove(0);
    let host = old.id().to_owned();
    old.test_crash();
    wait(|| state(&nodes[0]).is_some_and(|r| r.host != host && r.cursor == 1));
    nodes[0]
        .command(Command::Answer {
            position: 1,
            choice: 0,
        })
        .unwrap();
    wait(|| {
        state(&nodes[0])
            .is_some_and(|r| r.cursor == 2 && r.completed(nodes[0].id()) && r.answers.len() == 3)
    });
}
#[test]
fn independent_three_peer_crash_then_last_survivor() {
    let mut nodes = setup(3, Mode::Independent);
    nodes[1]
        .command(Command::Answer {
            position: 0,
            choice: 0,
        })
        .unwrap();
    wait(|| {
        nodes
            .iter()
            .all(|n| state(n).is_some_and(|r| r.answers.len() == 1))
    });
    let old = nodes.remove(0);
    let first = old.id().to_owned();
    old.test_crash();
    wait(|| {
        nodes
            .iter()
            .all(|n| state(n).is_some_and(|r| r.host != first))
    });
    let host = state(&nodes[0]).unwrap().host;
    let idx = nodes.iter().position(|n| n.id() == host).unwrap();
    nodes.remove(idx).test_crash();
    wait(|| state(&nodes[0]).is_some_and(|r| r.host == nodes[0].id()));
    let pos = state(&nodes[0]).unwrap().position(nodes[0].id());
    for p in pos..2 {
        nodes[0]
            .command(Command::Answer {
                position: p,
                choice: 1,
            })
            .unwrap();
        wait(|| state(&nodes[0]).is_some_and(|r| r.position(nodes[0].id()) > p));
    }
    assert!(state(&nodes[0]).unwrap().completed(nodes[0].id()));
}
#[test]
fn discovery_and_listener_cleanup_are_real() {
    let a = node();
    a.command(Command::Create {
        name: "发现房间".into(),
        code: "4321".into(),
        player: "甲".into(),
    })
    .unwrap();
    wait(|| state(&a).is_some());
    let b = node();
    b.command(Command::Discover {
        code: "4321".into(),
    })
    .unwrap();
    wait(|| {
        b.poll()
            .iter()
            .any(|e| matches!(e,Event::Rooms(rooms) if rooms.iter().any(|r|r.code=="4321")))
    });
    let endpoint = a.endpoint();
    drop(a);
    wait(|| std::net::TcpStream::connect(&endpoint).is_err());
}
