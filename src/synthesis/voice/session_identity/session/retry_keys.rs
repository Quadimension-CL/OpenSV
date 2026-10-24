use std::{collections::BTreeMap, fs::File, io::Read, path::{Path, PathBuf}, sync::{Mutex, OnceLock}};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
use super::super::{Result, VoiceError};

struct Entry { digest: [u8;32], key: Zeroizing<u64> }
static KEYS: OnceLock<Mutex<BTreeMap<PathBuf, Entry>>> = OnceLock::new();

fn source(path: &Path) -> Result<(PathBuf,[u8;32])> {
    let path = path.canonicalize()?;
    let file = File::open(&path)?;
    if !file.metadata()?.is_file() { return Err(VoiceError::new("session 来源不是文件")); }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(1024*1024+1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024*1024 { return Err(VoiceError::new("所选会话文件超过 1 MiB。")); }
    Ok((path,Sha256::digest(&*bytes).into()))
}

pub(super) fn remember(path: &Path, key: u64) -> Result<()> {
    let (path,digest) = source(path)?;
    let mut keys = KEYS.get_or_init(Mutex::default).lock().map_err(|_| VoiceError::new("session 临时密钥状态不可用"))?;
    if keys.len() >= 8 && !keys.contains_key(&path) { keys.clear(); }
    keys.insert(path, Entry {digest,key:Zeroizing::new(key)});
    Ok(())
}

pub(super) fn get(path: &Path) -> Result<Option<Zeroizing<u64>>> {
    let Some(keys) = KEYS.get() else {return Ok(None)};
    let (path,digest) = source(path)?;
    let keys = keys.lock().map_err(|_| VoiceError::new("session 临时密钥状态不可用"))?;
    Ok(keys.get(&path).filter(|e| e.digest==digest).map(|e| Zeroizing::new(*e.key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_retry_cache_is_bound_to_source_path_and_content() {
        let a=std::env::temp_dir().join(format!("rynthv-private-retry-{}.bin",uuid::Uuid::new_v4()));
        let b=std::env::temp_dir().join(format!("rynthv-private-retry-{}.bin",uuid::Uuid::new_v4()));
        std::fs::write(&a,b"synthetic encrypted source").unwrap();
        std::fs::write(&b,b"synthetic encrypted source").unwrap();
        let key=uuid::Uuid::new_v4().as_u128() as u64;
        remember(&a,key).unwrap();
        assert_eq!(*get(&a).unwrap().unwrap(),key);
        assert!(get(&b).unwrap().is_none());
        std::fs::write(&a,b"changed synthetic source").unwrap();
        assert!(get(&a).unwrap().is_none());
        std::fs::remove_file(&a).unwrap();std::fs::remove_file(&b).unwrap();
    }
}
