//! Files that must survive a power cut or a full disk: written to a temporary file in the same folder, synced,
//! then renamed over the old one (the folder synced too), so a crash leaves the old file or the new one, never
//! a half-written one. The copy it replaces is kept as `<file>.prev` when asked. Everything is this user's only
//! (0600 files, 0700 folder). A file that is there but can't be read is never written over: it is moved aside
//! under a name of its own first, by the caller's choice.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Makes `dir` (and its parents) and keeps it this user's only.
pub fn private_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}

fn sync_dir(dir: &Path) {
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
}

/// Writes `data` to `path` safely (see the module text); with `keep_prev`, the file it replaces is kept as
/// `<path>.prev` (by a rename, never a hard link).
pub fn write_safely(path: &Path, data: &[u8], keep_prev: bool) -> Result<(), String> {
    let dir = path.parent().ok_or("no folder")?;
    private_dir(dir)?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(format!("couldn't write {}: {e}", path.display()));       // a full disk stops here: nothing changed
    }
    if keep_prev && path.exists() {
        let prev = prev_of(path);
        if let Err(e) = fs::rename(path, &prev) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("couldn't keep the previous copy: {e}"));
        }
    }
    if let Err(e) = fs::rename(&tmp, path) {
        if keep_prev {
            let _ = fs::rename(prev_of(path), path);                          // put the old one back
        }
        let _ = fs::remove_file(&tmp);
        return Err(format!("couldn't put {} in place: {e}", path.display()));
    }
    sync_dir(dir);
    Ok(())
}

pub fn prev_of(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".prev");
    PathBuf::from(p)
}

/// Moves `path` aside under a name of its own (`<path>.<why>-<n>`), never over another file kept aside; the
/// name it got.
pub fn set_aside(path: &Path, why: &str) -> Result<PathBuf, String> {
    for n in 1..10_000 {
        let mut p = path.as_os_str().to_owned();
        p.push(format!(".{why}-{n}"));
        let aside = PathBuf::from(p);
        if !aside.exists() {
            fs::rename(path, &aside).map_err(|e| format!("couldn't set {} aside: {e}", path.display()))?;
            if let Some(dir) = path.parent() {
                sync_dir(dir);
            }
            return Ok(aside);
        }
    }
    Err("too many copies set aside".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("names-msg-files-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn writes_safely_keeps_the_previous_copy_and_sets_aside_without_overwriting() {
        let d = dir("w");
        let f = d.join("store.bin");
        write_safely(&f, b"one", true).unwrap();
        write_safely(&f, b"two", true).unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"two");
        assert_eq!(fs::read(prev_of(&f)).unwrap(), b"one");
        assert_eq!(fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700);
        let a = set_aside(&f, "unreadable").unwrap();
        write_safely(&f, b"three", false).unwrap();
        let b = set_aside(&f, "unreadable").unwrap();
        assert_ne!(a, b);
        assert_eq!((fs::read(&a).unwrap(), fs::read(&b).unwrap()), (b"two".to_vec(), b"three".to_vec()));
        assert!(fs::read_dir(&d).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains(".tmp-")), "no temporary file left");
        let _ = fs::remove_dir_all(&d);
    }
}
