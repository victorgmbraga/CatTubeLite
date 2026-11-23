//! Where CatTube keeps its files, and small environment helpers.

use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(fallback))
}

/// `~/.config/cattube`: accounts and settings.
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("cattube")
}

/// `~/.cache/cattube`: pictures from the server.
pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("cattube")
}

/// Short-lived files (request headers, the player's control socket).
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PRIVATE_RUNTIME.get_or_init(private_runtime_dir).clone())
}

static PRIVATE_RUNTIME: OnceLock<PathBuf> = OnceLock::new();

/// Without `XDG_RUNTIME_DIR` (a box started without a login session), a
/// folder only this user can enter: these files hold tokens and the player's
/// control socket, and fixed names directly in `/tmp` could be planted.
fn private_runtime_dir() -> PathBuf {
    let uid = std::fs::metadata("/proc/self").map_or(0, |m| m.uid());
    let shared = std::env::temp_dir().join(format!("cattube-{uid}"));
    if ensure_private_dir(&shared, uid) {
        return shared;
    }
    // Someone else holds that name: fall back to our own cache folder.
    let own = cache_dir().join("run");
    let _ = std::fs::create_dir_all(cache_dir());
    ensure_private_dir(&own, uid);
    own
}

/// `dir` exists, is a real directory, is ours and closed to everyone else.
fn ensure_private_dir(dir: &Path, uid: u32) -> bool {
    let _ = std::fs::DirBuilder::new().mode(0o700).create(dir);
    std::fs::symlink_metadata(dir)
        .is_ok_and(|m| m.file_type().is_dir() && m.uid() == uid && m.mode() & 0o077 == 0)
}

/// The player's on-screen controls and key bindings (`cattube-osd.lua`,
/// `cattube-input.conf`): `$CATTUBE_DATA`, then `../share/cattube` next to the
/// binary (installed packages), then `assets/` (running from the source).
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("CATTUBE_DATA").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(prefix) = exe.parent().and_then(|b| b.parent())
    {
        let d = prefix.join("share/cattube");
        if d.join("cattube-osd.lua").is_file() {
            return d;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Use the TV-box defaults on ARM64, with an override for any machine.
pub fn low_power() -> bool {
    match std::env::var("CATTUBE_LOW_POWER").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => cfg!(target_arch = "aarch64"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_dir_refuses_open_folders_and_symlinks() {
        let uid = std::fs::metadata("/proc/self").unwrap().uid();
        let root = std::env::temp_dir().join(format!("cattube-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let fresh = root.join("fresh");
        assert!(ensure_private_dir(&fresh, uid));
        assert_eq!(std::fs::metadata(&fresh).unwrap().mode() & 0o777, 0o700);
        assert!(!ensure_private_dir(&fresh, uid + 1), "someone else's");

        let open = root.join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        assert!(!ensure_private_dir(&open, uid));

        let link = root.join("link");
        std::os::unix::fs::symlink(&fresh, &link).unwrap();
        assert!(!ensure_private_dir(&link, uid));
        let _ = std::fs::remove_dir_all(root);
    }
}
