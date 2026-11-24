//! Small helpers: atomic writes, URL encoding, grid movement.

use crate::input::Action;
use std::io;

/// Build a process- and thread-unique sibling temporary path next to `path`.
pub fn unique_tmp_path(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.{}.{n}.{suffix}", std::process::id()))
}

/// Replace a settings file in one step, surviving a power cut. A symlinked
/// file (a managed dotfile) stays a symlink: its target is what is replaced.
pub fn write_atomic(path: &std::path::Path, text: &str) -> io::Result<()> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    replace_file(&target, text)
}

/// TV boxes are often switched off at the wall: the data reaches the disk
/// before the rename, and the rename before this returns.
fn replace_file(path: &std::path::Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = unique_tmp_path(path, "tmp");
    let res = (|| -> io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o666)
            .open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()
    })()
    .and_then(|()| std::fs::rename(&tmp, path));
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return res;
    }
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => std::path::Path::new("."),
    };
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Percent-encode everything but unreserved characters (RFC 3986).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Move inside a grid of `n` items, `cols` wide. `None`: Up from the first
/// row (leave the grid).
pub fn grid_move(i: usize, action: Action, cols: usize, n: usize) -> Option<usize> {
    if n == 0 {
        return if action == Action::Up { None } else { Some(0) };
    }
    Some(match action {
        Action::Left if !i.is_multiple_of(cols) => i - 1,
        Action::Right if i % cols < cols - 1 && i + 1 < n => i + 1,
        Action::Up if i >= cols => i - cols,
        Action::Up => return None,
        Action::Down if i + cols < n => i + cols,
        // Last partial row: Down from above moves to the last item.
        Action::Down if i / cols < (n - 1) / cols => n - 1,
        _ => i,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn atomic_write_replaces_content_and_keeps_symlinks() {
        let dir = std::env::temp_dir().join(format!("cattube-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real.conf");
        let link = dir.join("settings.conf");
        std::fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        super::write_atomic(&link, "new").unwrap();
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "new");
        let fresh = dir.join("fresh.conf");
        super::write_atomic(&fresh, "first").unwrap();
        assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "first");
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            3,
            "no temporary left"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    use super::*;

    #[test]
    fn encoding_and_grid() {
        assert_eq!(percent_encode("a b/é"), "a%20b%2F%C3%A9");
        assert_eq!(grid_move(0, Action::Right, 3, 5), Some(1));
        assert_eq!(grid_move(1, Action::Down, 3, 5), Some(4));
        assert_eq!(grid_move(2, Action::Down, 3, 5), Some(4));
        assert_eq!(grid_move(1, Action::Up, 3, 5), None);
        let p = std::path::Path::new("/tmp/settings.conf");
        let t1 = unique_tmp_path(p, "tmp");
        let t2 = unique_tmp_path(p, "tmp");
        assert_ne!(t1, t2);
        assert_eq!(t1.parent(), p.parent());
    }
}
