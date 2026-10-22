use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::model::{Answer, Room};

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    room: Room,
    pending: Vec<Answer>,
}

fn validate(room: &Room, pending: &[Answer]) -> Result<(), String> {
    // The real bank bounds are checked again by the caller after loading.
    room.validate(usize::MAX)?;
    for answer in pending {
        if !room.started
            || answer.position >= room.indices.len()
            || answer.choice >= 4
            || answer.submission.trim().is_empty()
            || !room.members.iter().any(|member| member.id == answer.member)
        {
            return Err("待发送答案无效".into());
        }
    }
    Ok(())
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

struct Temporary(PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Persists a versioned snapshot by syncing a sibling temporary file before rename.
pub fn save(path: &Path, room: &Room, pending: &[Answer]) -> Result<(), String> {
    validate(room, pending)?;
    let bytes = serde_json::to_vec(&Snapshot {
        version: VERSION,
        room: room.clone(),
        pending: pending.to_vec(),
    })
    .map_err(|e| format!("序列化房间失败：{e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("房间存档过大".into());
    }
    let directory = parent(path);
    fs::create_dir_all(directory).map_err(|e| format!("创建存档目录失败：{e}"))?;
    let name = path
        .file_name()
        .ok_or_else(|| "存档路径必须包含文件名".to_string())?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(
        ".{}.{:032x}.tmp",
        std::process::id(),
        rand::random::<u128>()
    ));
    let temporary = Temporary(directory.join(temp_name));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary.0)
        .map_err(|e| format!("创建临时存档失败：{e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("写入存档失败：{e}"))?;
    file.sync_all().map_err(|e| format!("同步存档失败：{e}"))?;
    drop(file);
    fs::rename(&temporary.0, path).map_err(|e| format!("替换存档失败：{e}"))?;
    // Windows cannot open directories with File::open; the snapshot itself is synced above.
    #[cfg(unix)]
    File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| format!("同步存档目录失败：{e}"))?;
    Ok(())
}

/// Loads a structurally validated snapshot; callers must validate local bank bounds.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Snapshot reader reserved for explicit recovery; automatic restart identity is not yet exposed"
    )
)]
pub fn load(path: &Path) -> Result<(Room, Vec<Answer>), String> {
    let file = File::open(path).map_err(|e| format!("打开存档失败：{e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("读取存档失败：{e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("房间存档过大".into());
    }
    let snapshot: Snapshot =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析存档失败：{e}"))?;
    if snapshot.version != VERSION {
        return Err(format!("不支持的存档版本：{}", snapshot.version));
    }
    validate(&snapshot.room, &snapshot.pending)?;
    Ok((snapshot.room, snapshot.pending))
}

#[cfg(test)]
mod tests {
    use super::super::model::{Member, Mode};
    use super::*;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("room-storage-{:032x}", rand::random::<u128>()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("nested/state.json")
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn room() -> Room {
        let member = Member {
            id: "host".into(),
            name: "主持人".into(),
            endpoint: "localhost:1".into(),
            online: true,
            ready: false,
        };
        let mut room = Room::new(
            "测试房间".into(),
            "1234".into(),
            member,
            "fingerprint".into(),
        )
        .unwrap();
        room.start(vec![2, 0], Mode::Independent, 3).unwrap();
        room
    }
    fn answer() -> Answer {
        Answer {
            member: "host".into(),
            position: 0,
            choice: 2,
            submission: "submission-1".into(),
        }
    }

    #[test]
    fn save_load_roundtrip_retains_pending_and_ledger() {
        let directory = Directory::new();
        let mut room = room();
        room.submit(answer(), 3).unwrap();
        let pending = vec![answer()];
        save(&directory.path(), &room, &pending).unwrap();
        assert_eq!(load(&directory.path()).unwrap(), (room, pending));
    }
    #[test]
    fn save_replaces_existing_snapshot_and_leaves_no_temporary_files() {
        let directory = Directory::new();
        let mut room = room();
        save(&directory.path(), &room, &[]).unwrap();
        room.notice = Some("更新".into());
        save(&directory.path(), &room, &[]).unwrap();
        assert_eq!(load(&directory.path()).unwrap().0, room);
        assert_eq!(
            fs::read_dir(directory.path().parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }
    #[test]
    fn rejected_save_does_not_destroy_previous_snapshot() {
        let directory = Directory::new();
        let mut room = room();
        save(&directory.path(), &room, &[]).unwrap();
        let before = fs::read(directory.path()).unwrap();
        room.cursor = 99;
        assert!(save(&directory.path(), &room, &[]).is_err());
        assert_eq!(fs::read(directory.path()).unwrap(), before);
    }
    #[test]
    fn load_rejects_missing_truncated_and_unknown_version() {
        let directory = Directory::new();
        let path = directory.0.join("state.json");
        assert!(load(&path).is_err());
        fs::write(&path, b"{\"version\":").unwrap();
        assert!(load(&path).is_err());
        let snapshot = Snapshot {
            version: VERSION + 1,
            room: room(),
            pending: vec![],
        };
        fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert!(load(&path).is_err());
    }
    #[test]
    fn load_rejects_invalid_snapshot_and_pending() {
        let directory = Directory::new();
        let path = directory.0.join("state.json");
        let mut snapshot = Snapshot {
            version: VERSION,
            room: room(),
            pending: vec![],
        };
        snapshot.room.host = "missing".into();
        fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert!(load(&path).is_err());
        snapshot.room = room();
        let mut invalid = answer();
        invalid.position = 99;
        snapshot.pending.push(invalid);
        fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert!(load(&path).is_err());
    }
    #[test]
    fn rename_failure_cleans_temporary_file() {
        let directory = Directory::new();
        let target = directory.0.join("occupied");
        fs::create_dir(&target).unwrap();
        assert!(save(&target, &room(), &[]).is_err());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
    #[test]
    fn load_rejects_oversized_file() {
        let directory = Directory::new();
        let path = directory.0.join("huge.json");
        File::create(&path).unwrap().set_len(MAX_BYTES + 1).unwrap();
        assert!(load(&path).is_err());
    }
}
