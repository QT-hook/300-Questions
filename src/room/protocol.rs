use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

pub const VERSION: u32 = 2;

pub fn compatibility_error(
    version: u32,
    app: &str,
    hash: &str,
    local_hash: &str,
) -> Option<String> {
    let details = format!(
        "本机 v{} / 协议 {} / 题库 {}；对方 v{} / 协议 {} / 题库 {}",
        env!("CARGO_PKG_VERSION"),
        VERSION,
        short_hash(local_hash),
        app,
        version,
        short_hash(hash)
    );
    if version != VERSION {
        Some(format!(
            "协议版本不一致：{details}。请所有参与者一起升级到 v0.1.1 或更新的相同版本"
        ))
    } else if hash != local_hash {
        Some(format!("题库指纹不一致：{details}。请使用相同题库的发布包"))
    } else {
        None
    }
}

pub fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}
pub const MAX_FRAME: usize = 65536;
pub const MAX_SNAPSHOT: usize = 4 * 1024 * 1024;

// A bounded logical JSON document is carried in independently bounded frames.
// A zero-length frame terminates the document; partial documents are never applied.
pub fn write_message<T: Serialize>(stream: &mut impl Write, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > MAX_SNAPSHOT {
        return Err(io::Error::other("snapshot exceeds 4 MiB"));
    }
    for chunk in bytes.chunks(MAX_FRAME) {
        stream.write_all(&(chunk.len() as u32).to_be_bytes())?;
        stream.write_all(chunk)?;
    }
    stream.write_all(&0_u32.to_be_bytes())?;
    stream.flush()
}
pub fn read_message<T: DeserializeOwned>(stream: &mut impl Read) -> io::Result<T> {
    let mut bytes = Vec::new();
    loop {
        let mut size = [0; 4];
        stream.read_exact(&mut size)?;
        let size = u32::from_be_bytes(size) as usize;
        if size == 0 {
            break;
        }
        if size > MAX_FRAME || bytes.len() + size > MAX_SNAPSHOT {
            return Err(io::Error::other("message limit"));
        }
        let start = bytes.len();
        bytes.resize(start + size, 0);
        stream.read_exact(&mut bytes[start..])?;
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Advert {
    pub room_id: String,
    pub name: String,
    pub code: String,
    pub endpoint: String,
    pub term: u64,
    pub host: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_roundtrip_large_snapshots() {
        let value = "x".repeat(200_000);
        let mut data = Vec::new();
        write_message(&mut data, &value).unwrap();
        assert_eq!(read_message::<String>(&mut &data[..]).unwrap(), value);
    }
    #[test]
    fn oversized_and_truncated_frames_rejected() {
        assert!(read_message::<String>(&mut &(65537_u32.to_be_bytes())[..]).is_err());
        assert!(read_message::<String>(&mut &[0, 0, 0, 5, 1][..]).is_err());
        assert!(read_message::<String>(&mut &[0, 0, 0, 1, b'!', 0, 0, 0, 0][..]).is_err());
    }
}
