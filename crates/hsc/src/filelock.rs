//! The RTS's file locks (GHC's rts/FileLock.c), shared by both back ends: a file, by device and
//! inode, may be open for reading by any number of handles or for writing by one. `lockFile`
//! fails (-1) when the open would break that; `unlockFile` releases the lock a descriptor holds.

use std::collections::HashMap;
use std::sync::Mutex;

struct Locks {
    /// Per file: the number of readers, or -1 for a writer.
    files: HashMap<(u64, u64), i64>,
    /// The file each locking descriptor holds.
    fds: HashMap<i64, (u64, u64)>,
}

static LOCKS: Mutex<Option<Locks>> = Mutex::new(None);

pub fn lock_file(fd: i64, dev: u64, ino: u64, for_writing: bool) -> i64 {
    let mut guard = LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let locks = guard.get_or_insert_with(|| Locks { files: HashMap::new(), fds: HashMap::new() });
    let key = (dev, ino);
    match locks.files.get(&key).copied() {
        None => {
            locks.files.insert(key, if for_writing { -1 } else { 1 });
        }
        Some(n) if n > 0 && !for_writing => {
            locks.files.insert(key, n + 1);
        }
        Some(_) => return -1,
    }
    locks.fds.insert(fd, key);
    0
}

pub fn unlock_file(fd: i64) -> i64 {
    let mut guard = LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(locks) = guard.as_mut() else { return 1 };
    let Some(key) = locks.fds.remove(&fd) else { return 1 };
    match locks.files.get(&key).copied() {
        Some(n) if n > 1 => {
            locks.files.insert(key, n - 1);
        }
        _ => {
            locks.files.remove(&key);
        }
    }
    0
}
