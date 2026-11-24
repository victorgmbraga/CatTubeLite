//! Build-time branding, for a build made only for yourself:
//!   CATTUBE_NAME="My Tube"     the name shown in the app and its window
//!   CATTUBE_LOGO=/path/to.png  a square PNG drawn instead of the cat mark
//! Unset, the app is CatTube with its own mark.
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=CATTUBE_NAME");
    println!("cargo:rerun-if-env-changed=CATTUBE_LOGO");
    let name = env::var("CATTUBE_NAME")
        .ok()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "CatTube".to_string());
    println!("cargo:rustc-env=CATTUBE_APP_NAME={name}");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("logo.png");
    let logo = match env::var("CATTUBE_LOGO")
        .ok()
        .filter(|p| !p.trim().is_empty())
    {
        Some(path) => {
            println!("cargo:rerun-if-changed={path}");
            let bytes =
                fs::read(&path).unwrap_or_else(|e| panic!("CATTUBE_LOGO: cannot read {path}: {e}"));
            assert!(
                bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                "CATTUBE_LOGO: {path} is not a PNG"
            );
            bytes
        }
        None => Vec::new(),
    };
    fs::write(out, logo).unwrap();
}
