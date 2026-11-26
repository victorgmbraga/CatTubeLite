//! Playing a video: mpv with yt-dlp finding the streams.
//!
//! YouTube changes often, and a yt-dlp from a distribution goes stale
//! within weeks, so CatTube keeps its own copy (the official single-file
//! release, run with the system's Python) up to date in its data folder.
//! Full quality (above 360p) needs YouTube's
//! script challenges solved, which yt-dlp does with a JavaScript runtime:
//! the system's Deno or Node if there is one, else CatTube's own Deno.
//!
//! The quality choice: a highest resolution, a preferred codec (AV1, VP9 or AVC) and HDR on or off;
//! yt-dlp falls back to the next best format when that one is missing.

use crate::process::Cmd;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const YT_DLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp";
/// The checksums yt-dlp publishes with each release.
pub const YT_DLP_SUMS_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";
/// yt-dlp's name in [`YT_DLP_SUMS_URL`].
pub const YT_DLP_ASSET: &str = "yt-dlp";
#[cfg(target_arch = "aarch64")]
macro_rules! deno_asset {
    () => {
        "deno-aarch64-unknown-linux-gnu.zip"
    };
}
#[cfg(not(target_arch = "aarch64"))]
macro_rules! deno_asset {
    () => {
        "deno-x86_64-unknown-linux-gnu.zip"
    };
}
pub const DENO_ASSET: &str = deno_asset!();
pub const DENO_URL: &str = concat!(
    "https://github.com/denoland/deno/releases/latest/download/",
    deno_asset!()
);
pub const DENO_SUM_URL: &str = concat!(
    "https://github.com/denoland/deno/releases/latest/download/",
    deno_asset!(),
    ".sha256sum"
);
/// How often CatTube asks yt-dlp to update itself.
pub const UPDATE_EVERY: Duration = Duration::from_secs(24 * 3600);
/// How often CatTube's own Deno is checked for a new release.
pub const DENO_UPDATE_EVERY: Duration = Duration::from_secs(30 * 24 * 3600);

/// `~/.local/share/cattube`: yt-dlp, Deno, positions.
pub fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()))
                .join(".local/share")
        })
        .join("cattube")
}

pub fn yt_dlp_path(data: &Path) -> PathBuf {
    data.join("yt-dlp")
}

/// yt-dlp unpacked from its single-file release. Run from a zip, Python
/// compiles every module it imports again on each run (about 0.6 s on a
/// desktop, seconds on a TV box); from a folder it keeps them compiled in
/// `__pycache__`. The single file stays: it is what updates itself (`-U`).
pub fn yt_dlp_dir(data: &Path) -> PathBuf {
    data.join("yt-dlp.d")
}

/// The script that runs [`yt_dlp_dir`] (mpv needs one program to run).
pub fn yt_dlp_launcher(data: &Path) -> PathBuf {
    data.join("yt-dlp-unpacked")
}

/// The launcher's text: Python running the unpacked folder.
pub fn yt_dlp_launcher_script(data: &Path) -> String {
    let dir = yt_dlp_dir(data)
        .display()
        .to_string()
        .replace('\'', "'\\''");
    format!(
        "#!/bin/sh\n# yt-dlp unpacked by CatTube, so Python keeps its compiled modules.\nexec python3 '{dir}' \"$@\"\n"
    )
}

/// What mpv runs as yt-dlp: the unpacked copy once it is in place, else
/// the single file.
pub fn yt_dlp_runner(data: &Path) -> PathBuf {
    let launcher = yt_dlp_launcher(data);
    if launcher.is_file() && yt_dlp_dir(data).join("__main__.py").is_file() {
        launcher
    } else {
        yt_dlp_path(data)
    }
}

/// Read yt-dlp's version directly from the unpacked `yt_dlp/version.py`
/// without spawning Python. Returns `None` if absent or malformed.
pub fn yt_dlp_version_from_unpacked(data: &Path) -> Option<String> {
    let text = std::fs::read_to_string(yt_dlp_dir(data).join("yt_dlp/version.py")).ok()?;
    parse_yt_dlp_version(&text)
}

fn parse_yt_dlp_version(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("__version__")?.trim_start();
        let val = rest.strip_prefix('=')?.trim();
        let unquoted = val
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .or_else(|| val.strip_prefix('"').and_then(|s| s.strip_suffix('"')))?;
        let v = unquoted.trim();
        (!v.is_empty() && v.len() <= 64 && v.bytes().all(|b| b.is_ascii_graphic()))
            .then(|| v.to_string())
    })
}

/// Unpack the single-file yt-dlp (a zip) into `dir`.
pub fn unzip_all_cmd(zip: &Path, dir: &Path) -> Cmd {
    Cmd::new("python3")
        .args([
            "-I".to_string(),
            "-c".to_string(),
            "import sys,zipfile;zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])".to_string(),
            zip.display().to_string(),
            dir.display().to_string(),
        ])
        .timeout(Duration::from_secs(120))
}

/// Import what playing a YouTube video needs from an unpacked yt-dlp, so
/// Python compiles it now (in the background) rather than at the first
/// video. Fails when that copy cannot run.
pub fn warm_yt_dlp_cmd(dir: &Path) -> Cmd {
    Cmd::new("python3")
        .args([
            "-c".to_string(),
            "import sys\nsys.path.insert(0, sys.argv[1])\nimport yt_dlp\ntry:\n  yt_dlp.YoutubeDL({'quiet': True}).get_info_extractor('Youtube')\nexcept Exception:\n  pass\nprint(yt_dlp.version.__version__)"
                .to_string(),
            dir.display().to_string(),
        ])
        .timeout(Duration::from_secs(180))
}

pub fn own_deno_path(data: &Path) -> PathBuf {
    data.join("deno")
}

/// `curl` saving `url` to `out` (follows GitHub's redirects).
pub fn download_cmd(url: &str, out: &Path) -> Cmd {
    Cmd::new("curl")
        .args([
            "-sSL".to_string(),
            "--fail".to_string(),
            "-m".to_string(),
            "300".to_string(),
            "-o".to_string(),
            out.display().to_string(),
            "--".to_string(),
            url.to_string(),
        ])
        .timeout(Duration::from_secs(320))
}

/// `curl` printing a small text file (a release's checksums).
pub fn fetch_text_cmd(url: &str) -> Cmd {
    Cmd::new("curl")
        .args(["-sSL", "--fail", "-m", "30", "--", url])
        .timeout(Duration::from_secs(35))
}

/// A file's SHA-256 with the system's Python (yt-dlp needs Python anyway).
pub fn sha256_cmd(path: &Path) -> Cmd {
    Cmd::new("python3")
        .args([
            "-I".to_string(),
            "-c".to_string(),
            "import hashlib,sys\nh=hashlib.sha256()\nwith open(sys.argv[1],'rb') as f:\n  for b in iter(lambda:f.read(65536),b''):\n    h.update(b)\nprint(h.hexdigest())"
                .to_string(),
            path.display().to_string(),
        ])
        .timeout(Duration::from_secs(120))
}

/// `name`'s SHA-256 in a checksum list (`<hash>  <name>` lines), lowercase.
pub fn expected_sha256(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// Unpack Deno's zip with the system's Python (yt-dlp needs Python anyway).
pub fn unzip_cmd(zip: &Path, dir: &Path) -> Cmd {
    Cmd::new("python3")
        .args([
            "-I".to_string(),
            "-c".to_string(),
            "import sys,zipfile;zipfile.ZipFile(sys.argv[1]).extract('deno',sys.argv[2])"
                .to_string(),
            zip.display().to_string(),
            dir.display().to_string(),
        ])
        .timeout(Duration::from_secs(120))
}

pub fn update_cmd(yt_dlp: &Path) -> Cmd {
    Cmd::new(yt_dlp.display().to_string())
        .args(["-U", "--no-colors"])
        .timeout(Duration::from_secs(180))
}

/// Sanitize an explicit `CATTUBE_JS_RUNTIME` override (`deno:/path`,
/// `quickjs:/path`, `node:/path`, `bun:/path`). Rejects shell/option
/// separators so it can safely enter `--ytdl-raw-options=js-runtimes=...`.
fn sanitize_js_runtime(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 512 {
        return None;
    }
    if s.chars()
        .any(|c| c.is_control() || matches!(c, ',' | '"' | '\'' | ';' | '\n' | '\r'))
    {
        return None;
    }
    let name = s.split(':').next().unwrap_or("");
    matches!(name, "deno" | "node" | "quickjs" | "bun").then(|| s.to_string())
}

/// The JavaScript runtime yt-dlp is given: `CATTUBE_JS_RUNTIME` if set, else
/// system `deno`, CatTube's own `deno`, system `qjs`/`quickjs`, or `node`.
pub fn js_runtime(data: &Path, find: &dyn Fn(&str) -> Option<PathBuf>) -> Option<String> {
    js_runtime_with_env(
        data,
        find,
        std::env::var("CATTUBE_JS_RUNTIME").ok().as_deref(),
    )
}

pub fn js_runtime_with_env(
    data: &Path,
    find: &dyn Fn(&str) -> Option<PathBuf>,
    env: Option<&str>,
) -> Option<String> {
    if let Some(forced) = env.and_then(sanitize_js_runtime) {
        return Some(forced);
    }
    if let Some(p) = find("deno") {
        return Some(format!("deno:{}", p.display()));
    }
    // Only marked executable once fully unpacked: a file left by an
    // interrupted install is not a runtime, and gets replaced.
    let own = own_deno_path(data);
    if crate::process::find_executable(&own.display().to_string()).is_some() {
        return Some(format!("deno:{}", own.display()));
    }
    if let Some(p) = find("qjs").or_else(|| find("quickjs")) {
        return Some(format!("quickjs:{}", p.display()));
    }
    find("node").map(|p| format!("node:{}", p.display()))
}

// --------------------------------------------------------------- quality

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Codec {
    /// Best on newer graphics (Intel 11th gen+, AMD RDNA2+).
    Av1,
    /// Plays well on most TVs and boxes.
    #[default]
    Vp9,
    /// For old or slow hardware.
    Avc,
}

impl Codec {
    pub const ALL: [Codec; 3] = [Codec::Av1, Codec::Vp9, Codec::Avc];

    pub fn key(self) -> &'static str {
        match self {
            Self::Av1 => "av1",
            Self::Vp9 => "vp9",
            Self::Avc => "avc",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Av1 => "AV1",
            Self::Vp9 => "VP9",
            Self::Avc => "AVC (H.264)",
        }
    }

    pub fn from_key(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|c| c.key() == s)
            .unwrap_or_default()
    }

    fn vcodec_prefix(self) -> &'static str {
        match self {
            Self::Av1 => "av01",
            Self::Vp9 => "vp09",
            Self::Avc => "avc1",
        }
    }
}

/// The highest resolutions Settings offers (0 = no limit).
pub const RESOLUTIONS: [u32; 7] = [0, 4320, 2160, 1440, 1080, 720, 480];

pub fn resolution_label(h: u32) -> String {
    match h {
        0 => "Best".into(),
        4320 => "8K".into(),
        2160 => "4K".into(),
        h => format!("{h}p"),
    }
}

/// yt-dlp's format choice: the preferred codec at most `max_h` high, then
/// any codec at that height, then anything. `hdr = false` keeps to SDR.
///
/// On low-power TV boxes (`low_power = true`):
/// - H.264 up to 720p allows up to 60 fps (hardware decoders such as Amlogic
///   S905X handle 720p60 H.264 smoothly), while higher resolutions and other
///   codecs still prefer `<= 30 fps`.
/// - When the preferred codec is not AV1, fallbacks exclude AV1
///   (`[vcodec!^=av01]`) before trying any codec so boxes without an AV1
///   hardware decoder do not silently drop into software AV1 decoding.
pub fn format_string(max_h: u32, codec: Codec, hdr: bool, low_power: bool) -> String {
    let h = if max_h == 0 {
        String::new()
    } else {
        format!("[height<={max_h}]")
    };
    let sdr = if hdr { "" } else { "[dynamic_range=SDR]" };
    let p = codec.vcodec_prefix();
    // VP9 also shows up as plain "vp9".
    let alt = if codec == Codec::Vp9 {
        format!("/bv*{h}{sdr}[vcodec=vp9]+ba")
    } else {
        String::new()
    };
    let general = format!("bv*{h}{sdr}[vcodec^={p}]+ba{alt}/bv*{h}{sdr}+ba/bv*{h}+ba/b{h}/b");
    if !low_power {
        return general;
    }
    // Try capped formats with either separate or combined audio before
    // falling back to higher frame rates or an unknown frame rate.
    let sdr = if hdr { "" } else { "[dynamic_range=?SDR]" };
    let cap30 = format!("{h}{sdr}[fps<=30]");
    let preferred = if codec == Codec::Vp9 {
        "[vcodec~='^(vp09|vp9$)']".to_string()
    } else {
        format!("[vcodec^={p}]")
    };
    let pair = |filter: &str| format!("bv*{filter}+ba/b{filter}");
    let mut parts = Vec::new();
    match codec {
        Codec::Avc => {
            if max_h > 720 {
                // Keep >720p at <=30 fps (e.g. 1080p30) before stepping down to 720p60 H.264.
                // Note: `{h}` comes first so the OSD's `height<=(%d+)` match still sees `max_h`.
                parts.push(format!(
                    "({})",
                    pair(&format!("{h}[height>720]{sdr}[fps<=30]{preferred}"))
                ));
                parts.push(format!(
                    "({})",
                    pair(&format!("[height<=720]{sdr}[fps<=?60]{preferred}"))
                ));
            } else if max_h > 0 {
                // Up to 720p H.264 allows up to 60 fps (and unknown fps on live streams).
                parts.push(format!(
                    "({})",
                    pair(&format!("{h}{sdr}[fps<=?60]{preferred}"))
                ));
            } else {
                // Uncapped ("Best"): keep `{h}` absent so the OSD still reports Best.
                parts.push(format!("({})", pair(&format!("{cap30}{preferred}"))));
            }
            // Fall back to non-AV1 (e.g. VP9) before ever touching software AV1.
            parts.push(pair(&format!("{cap30}[vcodec!^=av01]")));
            parts.push(pair(&format!("{h}{sdr}[vcodec!^=av01]")));
            parts.push(pair(&cap30));
        }
        Codec::Vp9 => {
            parts.push(format!("({})", pair(&format!("{cap30}{preferred}"))));
            // Fall back to non-AV1 (e.g. H.264) before ever touching software AV1.
            parts.push(pair(&format!("{cap30}[vcodec!^=av01]")));
            parts.push(pair(&format!("{h}{sdr}[vcodec!^=av01]")));
            parts.push(pair(&cap30));
        }
        Codec::Av1 => {
            parts.push(format!("({})", pair(&format!("{cap30}{preferred}"))));
            parts.push(pair(&cap30));
        }
    }
    parts.push(general);
    parts.join("/")
}

/// Buffer sizes offered (MiB): 32 MiB for 1 GiB TV boxes, up to 768 MiB for
/// shaky connections on desktops.
pub const BUFFERS: [u32; 4] = [32, 64, 256, 768];

/// Countries for recommendations (YouTube's region codes).
pub const REGIONS: [&str; 16] = [
    "US", "GB", "IN", "CA", "AU", "DE", "FR", "ES", "IT", "NL", "BR", "MX", "JP", "KR", "PL", "TR",
];

// ------------------------------------------------------------------ mpv

/// Everything that shapes one playback.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayOptions {
    pub low_power: bool,
    /// How much to load ahead, in MiB (the Buffer setting).
    pub buffer_mib: u32,
    pub video_id: String,
    pub title: String,
    pub start: f64,
    pub max_height: u32,
    pub codec: Codec,
    pub hdr: bool,
    /// Subtitle languages to fetch ("en"); empty = none.
    pub sub_langs: String,
    pub subs_on: bool,
    /// `RRGGBB` for the control bar.
    pub accent: String,
    /// Back hides the video (it plays on behind CatTube's list) instead
    /// of stopping it: on CatosTV, which can bring it back.
    pub hide_on_back: bool,
    /// Show the performance readout over the video.
    pub stats: bool,
    /// `--hwdec` to pass, if any (see [`hwdec_choice`]).
    pub hwdec: Option<String>,
}

/// Does this `mpv.conf` choose a video decoder (`hwdec=` outside a
/// profile, or in `[default]`)?
pub fn mpv_conf_sets_hwdec(text: &str) -> bool {
    let mut top = true;
    text.lines().map(str::trim).any(|line| {
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            top = section.trim() == "default";
            return false;
        }
        let line = line.strip_prefix("--").unwrap_or(line);
        top && !line.starts_with('#')
            && line
                .split_once('=')
                .is_some_and(|(k, v)| k.trim() == "hwdec" && !v.trim().is_empty())
    })
}

/// The `mpv.conf` files mpv reads: the user's (`$MPV_HOME`, else
/// `~/.config/mpv`) and the system's.
pub fn mpv_conf_paths() -> Vec<PathBuf> {
    let user = std::env::var_os("MPV_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::env::xdg("XDG_CONFIG_HOME", ".config").join("mpv"));
    vec![user.join("mpv.conf"), PathBuf::from("/etc/mpv/mpv.conf")]
}

/// The `--hwdec` CatTube passes, in order: `CATTUBE_HWDEC` (forces one,
/// "no" for the CPU); nothing when an `mpv.conf` already chooses (a TV box
/// may need `v4l2m2m-copy`, which mpv's safe list leaves out, and the
/// command line would override the file); `auto-safe` on a low-power
/// device; else nothing (mpv's own default).
pub fn hwdec_choice(env: Option<&str>, conf_sets: bool, low_power: bool) -> Option<String> {
    let forced: String = env
        .unwrap_or("")
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ','))
        .take(64)
        .collect();
    if !forced.is_empty() {
        Some(forced)
    } else if conf_sets {
        None
    } else {
        low_power.then(|| "auto-safe".to_string())
    }
}

/// mpv's command line for a video. `data` holds yt-dlp, `ui` the control
/// bar script and key bindings, `js` the runtime for yt-dlp.
pub fn mpv_argv(
    o: &PlayOptions,
    data: &Path,
    ui: &Path,
    socket: &Path,
    js: Option<&str>,
) -> Vec<String> {
    let mut v: Vec<String> = [
        "mpv",
        "--fullscreen",
        "--force-window=immediate",
        "--idle=no",
        "--keep-open=no",
        // CatTube keeps its own places; a system mpv.conf that saves and
        // restores them (volume too) would fight the start time.
        "--save-position-on-quit=no",
        "--resume-playback=no",
        // Do not take keys from stdin or print status lines, while keeping
        // errors on stdout so CatTube can explain why a video failed to start.
        "--no-input-terminal",
        "--term-status-msg=",
        "--msg-level=all=error",
        "--wayland-app-id=cattube-player",
        "--osc=no",
        // Only CatTube's keys: mpv's built-in ones (volume, the number
        // keys, mouse wheel, stats, ...) would change things the
        // controls don't offer.
        "--no-input-default-bindings",
        "--cache=yes",
        // YouTube's video addresses only work a few seconds after they are
        // handed out (yt-dlp's "available_at"); until then they answer
        // 403. Ask again for up to 12 s instead of giving up.
        "--stream-lavf-o=reconnect=1,reconnect_on_http_error=403,reconnect_delay_max=12",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if o.low_power {
        v.extend(
            [
                "--profile=fast",
                "--scale=bilinear",
                "--cscale=bilinear",
                "--dscale=bilinear",
                "--dither-depth=no",
                "--deband=no",
            ]
            .into_iter()
            .map(str::to_string),
        );
    }
    if let Some(h) = &o.hwdec {
        v.push(format!("--hwdec={h}"));
    }
    v.push(format!(
        "--demuxer-max-bytes={}MiB",
        o.buffer_mib.clamp(16, 1024)
    ));
    v.push(format!(
        "--demuxer-max-back-bytes={}MiB",
        (o.buffer_mib / 4).clamp(8, 256)
    ));
    v.push(format!("--input-ipc-server={}", socket.display()));
    v.push(format!(
        "--input-conf={}",
        ui.join("cattube-input.conf").display()
    ));
    v.push(format!("--script={}", ui.join("cattube-osd.lua").display()));
    let accent: String = o
        .accent
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(6)
        .collect();
    let mut opts = vec![
        format!("ytdl_hook-ytdl_path={}", yt_dlp_runner(data).display()),
        "ytdl_hook-try_ytdl_first=yes".to_string(),
    ];
    if accent.len() == 6 {
        opts.push(format!("cattube_osd-accent={accent}"));
    }
    if o.hide_on_back {
        opts.push("cattube_osd-hide=yes".to_string());
    }
    if o.low_power {
        opts.push("cattube_osd-low_power=yes".into());
    }
    if o.stats {
        opts.push("cattube_osd-stats=yes".into());
    }
    opts.push(format!(
        "cattube_osd-position_dir={}",
        data.join("positions").display()
    ));
    v.push(format!("--script-opts={}", opts.join(",")));
    v.push(format!(
        "--ytdl-format={}",
        format_string(o.max_height, o.codec, o.hdr, o.low_power)
    ));
    let mut raw: Vec<String> = Vec::new();
    if let Some(js) = js {
        raw.push(format!("js-runtimes={js}"));
    }
    let langs: String = o
        .sub_langs
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(16)
        .collect();
    if !langs.is_empty() {
        // The language and its regional forms (en-GB), not YouTube's
        // machine translations into it (en-de, "English from German").
        // yt-dlp ignores case, so the region part is matched with (?-i).
        raw.push(format!(
            "sub-langs=\"{langs},{langs}-(?-i:[A-Z]).*,{langs}-orig\""
        ));
        raw.push("write-subs=".into());
        raw.push("write-auto-subs=".into());
    }
    if !raw.is_empty() {
        v.push(format!("--ytdl-raw-options={}", raw.join(",")));
    }
    v.push(if o.subs_on {
        "--sid=auto".into()
    } else {
        "--sid=no".into()
    });
    // Always normal speed, even when a user mpv.conf sets another.
    v.push("--speed=1".into());
    let title: String = o
        .title
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    // No title known: yt-dlp's (the video's own) shows.
    if !title.trim().is_empty() {
        v.push(format!("--force-media-title={title}"));
    }
    if o.start.is_finite() && o.start > 0.0 {
        v.push(format!("--start={:.3}", o.start));
    }
    v.push("--osd-duration=1500".into());
    v.push("--osd-bar-align-y=0.86".into());
    v.push("--".into());
    v.push(crate::youtube::watch_url(&o.video_id));
    v
}

pub fn player_socket() -> PathBuf {
    crate::env::runtime_dir().join("cattube-mpv.sock")
}

// ------------------------------------------------------------- positions

/// Where each video was left (seconds), for resuming: `positions.conf`,
/// newest last, at most `MAX_POSITIONS`.
pub const MAX_POSITIONS: usize = 500;

static POSITIONS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn parse_checkpoint_fields(text: &str) -> Option<Checkpoint> {
    let mut fields = text.split_whitespace();
    let position: f64 = fields.next()?.parse().ok()?;
    let duration: f64 = fields.next()?.parse().ok()?;
    let finished = match fields.next()? {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    if fields.next().is_some()
        || !position.is_finite()
        || position < 0.0
        || !duration.is_finite()
        || duration < 0.0
    {
        return None;
    }
    Some(Checkpoint {
        position,
        duration: (duration > 0.0).then_some(duration),
        finished,
    })
}

/// Parse `positions.conf`: accepts both legacy 2-field lines (`<id> <pos>`)
/// and compacted 4-field lines (`<id> <pos> <dur> <0|1>`).
pub fn parse_position_records(text: &str) -> Vec<(String, Checkpoint)> {
    text.lines()
        .filter_map(|l| {
            let mut fields = l.split_whitespace();
            let id = fields.next()?;
            if !crate::youtube::is_video_id(id) {
                return None;
            }
            let position: f64 = fields.next()?.parse().ok()?;
            if !position.is_finite() || position < 0.0 {
                return None;
            }
            match fields.next() {
                None => Some((
                    id.to_string(),
                    Checkpoint {
                        position,
                        duration: None,
                        finished: false,
                    },
                )),
                Some(dur_s) => {
                    let duration: f64 = dur_s.parse().ok()?;
                    let finished = match fields.next()? {
                        "0" => false,
                        "1" => true,
                        _ => return None,
                    };
                    if fields.next().is_some() || !duration.is_finite() || duration < 0.0 {
                        return None;
                    }
                    Some((
                        id.to_string(),
                        Checkpoint {
                            position,
                            duration: (duration > 0.0).then_some(duration),
                            finished,
                        },
                    ))
                }
            }
        })
        .collect()
}

pub fn position_records_text(list: &[(String, Checkpoint)]) -> String {
    list.iter()
        .map(|(id, p)| {
            format!(
                "{id} {:.3} {:.3} {}\n",
                p.position,
                p.duration.unwrap_or(0.0),
                u8::from(p.finished)
            )
        })
        .collect()
}

pub fn parse_positions(text: &str) -> Vec<(String, f64)> {
    parse_position_records(text)
        .into_iter()
        .filter(|(_, p)| !p.finished)
        .map(|(id, p)| (id, p.position))
        .collect()
}

/// Preserve partial playback, including short videos and their final seconds.
pub fn remember(list: &mut Vec<(String, f64)>, id: &str, pos: f64, _duration: Option<f64>) {
    list.retain(|(x, _)| x != id);
    if pos.is_finite() && pos > 0.0 {
        list.push((id.to_string(), pos));
    }
    let n = list.len();
    if n > MAX_POSITIONS {
        list.drain(..n - MAX_POSITIONS);
    }
}

fn remember_record(list: &mut Vec<(String, Checkpoint)>, id: &str, point: Checkpoint) {
    if !crate::youtube::is_video_id(id) || !point.position.is_finite() || point.position < 0.0 {
        return;
    }
    list.retain(|(x, _)| x != id);
    if point.finished || point.position > 0.0 {
        list.push((id.to_string(), point));
    }
    let n = list.len();
    if n > MAX_POSITIONS {
        list.drain(..n - MAX_POSITIONS);
    }
}

pub fn positions_text(list: &[(String, f64)]) -> String {
    list.iter()
        .map(|(id, s)| format!("{id} {s:.3}\n"))
        .collect()
}

pub fn position_clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Written atomically by the player itself, including on pause, seek and exit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkpoint {
    pub position: f64,
    pub duration: Option<f64>,
    pub finished: bool,
}

pub fn store_checkpoint(data: &Path, id: &str, point: Checkpoint) -> std::io::Result<()> {
    if !crate::youtube::is_video_id(id) || !point.position.is_finite() || point.position < 0.0 {
        return Err(std::io::Error::other("invalid playback position"));
    }
    let folder = data.join("positions");
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{id}.pos"));
    let tmp = crate::util::unique_tmp_path(&path, "tmp");
    let body = format!(
        "{:.3} {:.3} {}\n",
        point.position,
        point.duration.unwrap_or(0.0),
        u8::from(point.finished)
    );
    let res = std::fs::write(&tmp, body).and_then(|()| std::fs::rename(&tmp, &path));
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

fn read_checkpoint_with_inode(path: &Path) -> Option<(Checkpoint, u64, u64)> {
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;
    let mut f = std::fs::File::open(path).ok()?;
    let meta = f.metadata().ok()?;
    let mut text = String::new();
    f.read_to_string(&mut text).ok()?;
    let point = parse_checkpoint_fields(&text)?;
    Some((point, meta.dev(), meta.ino()))
}

fn remove_if_same_inode(path: &Path, dev: u64, ino: u64) {
    use std::os::unix::fs::MetadataExt;
    if let Ok(meta) = std::fs::metadata(path)
        && meta.dev() == dev
        && meta.ino() == ino
    {
        let _ = std::fs::remove_file(path);
    }
}

fn write_position_records(path: &Path, list: &[(String, Checkpoint)]) -> std::io::Result<()> {
    let tmp = crate::util::unique_tmp_path(path, "tmp");
    let res = std::fs::write(&tmp, position_records_text(list))
        .and_then(|()| std::fs::rename(&tmp, path));
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// Merge finished/stopped playback positions into `positions.conf` atomically,
/// folding in any per-video `positions/<id>.pos` checkpoint written by mpv and
/// removing the merged `.pos` file once `positions.conf` is safely updated.
pub fn save_positions_merged(
    data: &Path,
    ended: &[(String, f64, Option<f64>)],
) -> std::io::Result<()> {
    if ended.is_empty() {
        return Ok(());
    }
    let _guard = POSITIONS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::create_dir_all(data)?;
    let path = data.join("positions.conf");
    let mut list = std::fs::read_to_string(&path)
        .map(|t| parse_position_records(&t))
        .unwrap_or_default();
    let folder = data.join("positions");
    let mut merged_files: Vec<(PathBuf, u64, u64)> = Vec::new();
    let mut changed = false;
    for (id, pos, dur) in ended {
        if !crate::youtube::is_video_id(id) {
            continue;
        }
        let cp_path = folder.join(format!("{id}.pos"));
        if let Some((point, dev, ino)) = read_checkpoint_with_inode(&cp_path) {
            remember_record(&mut list, id, point);
            merged_files.push((cp_path, dev, ino));
            changed = true;
        } else if pos.is_finite() && *pos > 0.0 {
            let point = Checkpoint {
                position: *pos,
                duration: dur.filter(|d| d.is_finite() && *d > 0.0),
                finished: false,
            };
            remember_record(&mut list, id, point);
            changed = true;
        }
    }
    if changed {
        write_position_records(&path, &list)?;
        for (cp_path, dev, ino) in merged_files {
            remove_if_same_inode(&cp_path, dev, ino);
        }
    }
    Ok(())
}

/// Fold leftover `positions/*.pos` files into `positions.conf` and remove
/// them, so later startups and History loads read one file instead of
/// hundreds of per-video files. Only call when no player is actively writing
/// to `positions/`.
pub fn compact_checkpoints(data: &Path) -> std::io::Result<()> {
    let _guard = POSITIONS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let folder = data.join("positions");
    let mut files: Vec<_> = match std::fs::read_dir(&folder) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()?.to_str()? != "pos" {
                    return None;
                }
                let id = path.file_stem()?.to_str()?.to_string();
                if !crate::youtube::is_video_id(&id) {
                    return None;
                }
                Some((entry.metadata().ok()?.modified().ok()?, id, path))
            })
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if files.is_empty() {
        return Ok(());
    }
    files.sort_by_key(|f| f.0);
    let stale = files.len().saturating_sub(MAX_POSITIONS);
    for (_, _, path) in files.drain(..stale) {
        let _ = std::fs::remove_file(path);
    }
    let path = data.join("positions.conf");
    let base = std::fs::read_to_string(&path)
        .map(|t| parse_position_records(&t))
        .unwrap_or_default();
    let mut order: HashMap<String, (usize, Checkpoint)> =
        HashMap::with_capacity(base.len() + files.len());
    let mut seq = 0usize;
    for (id, point) in base {
        order.insert(id, (seq, point));
        seq += 1;
    }
    let mut merged_files: Vec<(PathBuf, u64, u64)> = Vec::with_capacity(files.len());
    let mut invalid_files: Vec<PathBuf> = Vec::new();
    for (_, id, cp_path) in files {
        if let Some((point, dev, ino)) = read_checkpoint_with_inode(&cp_path) {
            if point.finished || point.position > 0.0 {
                order.insert(id, (seq, point));
                seq += 1;
            } else {
                order.remove(&id);
            }
            merged_files.push((cp_path, dev, ino));
        } else {
            invalid_files.push(cp_path);
        }
    }
    if !merged_files.is_empty() {
        let mut entries: Vec<(usize, String, Checkpoint)> = order
            .into_iter()
            .map(|(id, (idx, point))| (idx, id, point))
            .collect();
        entries.sort_unstable_by_key(|(idx, _, _)| *idx);
        if entries.len() > MAX_POSITIONS {
            entries.drain(..entries.len() - MAX_POSITIONS);
        }
        let compacted: Vec<(String, Checkpoint)> =
            entries.into_iter().map(|(_, id, p)| (id, p)).collect();
        std::fs::create_dir_all(data)?;
        write_position_records(&path, &compacted)?;
        for (cp_path, dev, ino) in merged_files {
            remove_if_same_inode(&cp_path, dev, ino);
        }
    }
    for bad in invalid_files {
        let _ = std::fs::remove_file(bad);
    }
    Ok(())
}

pub fn checkpoint(data: &Path, id: &str) -> Option<Checkpoint> {
    if !crate::youtube::is_video_id(id) {
        return None;
    }
    let text = std::fs::read_to_string(data.join("positions").join(format!("{id}.pos"))).ok()?;
    parse_checkpoint_fields(&text)
}

/// Player checkpoints take precedence over the older watcher snapshots.
/// Returns both the ordered resume list (newest last) and the checkpoints
/// merged from `positions.conf` and any live `positions/*.pos` files.
pub fn load_positions_and_checkpoints(
    data: &Path,
) -> (Vec<(String, f64)>, HashMap<String, Checkpoint>) {
    let base = std::fs::read_to_string(data.join("positions.conf"))
        .map(|s| parse_position_records(&s))
        .unwrap_or_default();
    let mut files: Vec<_> = std::fs::read_dir(data.join("positions"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()?.to_str()? != "pos" {
                return None;
            }
            let id = path.file_stem()?.to_str()?.to_string();
            if !crate::youtube::is_video_id(&id) {
                return None;
            }
            Some((entry.metadata().ok()?.modified().ok()?, id, path))
        })
        .collect();
    files.sort_by_key(|f| f.0);
    let stale = files.len().saturating_sub(MAX_POSITIONS);
    for (_, _, path) in files.drain(..stale) {
        let _ = std::fs::remove_file(path);
    }
    // Track newest order index per video id in O(N) instead of O(N^2) Vec::retain.
    let mut order: HashMap<String, (usize, f64)> = HashMap::with_capacity(base.len() + files.len());
    let mut checkpoints = HashMap::with_capacity(base.len() + files.len());
    let mut seq = 0usize;
    for (id, point) in base {
        checkpoints.insert(id.clone(), point);
        if point.finished {
            order.remove(&id);
        } else if point.position.is_finite() && point.position > 0.0 {
            order.insert(id, (seq, point.position));
            seq += 1;
        } else {
            order.remove(&id);
        }
    }
    for (_, id, _) in files {
        if let Some(point) = checkpoint(data, &id) {
            checkpoints.insert(id.clone(), point);
            if point.finished {
                order.remove(&id);
            } else if point.position.is_finite() && point.position > 0.0 {
                order.insert(id, (seq, point.position));
                seq += 1;
            } else {
                order.remove(&id);
            }
        }
    }
    let mut entries: Vec<(usize, String, f64)> = order
        .into_iter()
        .map(|(id, (idx, pos))| (idx, id, pos))
        .collect();
    entries.sort_unstable_by_key(|(idx, _, _)| *idx);
    if entries.len() > MAX_POSITIONS {
        entries.drain(..entries.len() - MAX_POSITIONS);
    }
    let list = entries.into_iter().map(|(_, id, p)| (id, p)).collect();
    (list, checkpoints)
}

/// Player checkpoints take precedence over the older watcher snapshots.
pub fn load_positions(data: &Path) -> Vec<(String, f64)> {
    load_positions_and_checkpoints(data).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hwdec_follows_the_env_then_mpv_conf_then_the_device() {
        assert_eq!(
            hwdec_choice(Some("v4l2m2m-copy"), true, true).as_deref(),
            Some("v4l2m2m-copy")
        );
        assert_eq!(
            hwdec_choice(Some(" no "), false, false).as_deref(),
            Some("no")
        );
        // Only a decoder name gets through, never another option.
        assert_eq!(
            hwdec_choice(Some("vaapi --vo=x"), false, false).as_deref(),
            Some("vaapi--vox")
        );
        assert_eq!(hwdec_choice(Some(""), true, true), None);
        assert_eq!(hwdec_choice(None, true, true), None);
        assert_eq!(
            hwdec_choice(None, false, true).as_deref(),
            Some("auto-safe")
        );
        assert_eq!(hwdec_choice(None, false, false), None);
    }

    #[test]
    fn mpv_conf_hwdec_counts_outside_profiles_only() {
        assert!(mpv_conf_sets_hwdec("vo=gpu\nhwdec=v4l2m2m-copy\n"));
        assert!(mpv_conf_sets_hwdec("  hwdec = auto  "));
        assert!(mpv_conf_sets_hwdec("--hwdec=vaapi"));
        assert!(mpv_conf_sets_hwdec("[tv]\nfs=yes\n[default]\nhwdec=drm"));
        assert!(!mpv_conf_sets_hwdec("# hwdec=vaapi\nhwdec-codecs=all"));
        assert!(!mpv_conf_sets_hwdec("[fast]\nhwdec=vaapi"));
        assert!(!mpv_conf_sets_hwdec("hwdec="));
    }

    #[test]
    fn formats() {
        assert_eq!(
            format_string(1080, Codec::Avc, false, false),
            "bv*[height<=1080][dynamic_range=SDR][vcodec^=avc1]+ba/bv*[height<=1080][dynamic_range=SDR]+ba/bv*[height<=1080]+ba/b[height<=1080]/b"
        );
        assert_eq!(
            format_string(0, Codec::Vp9, true, false),
            "bv*[vcodec^=vp09]+ba/bv*[vcodec=vp9]+ba/bv*+ba/bv*+ba/b/b"
        );
        assert_eq!(Codec::from_key("av1"), Codec::Av1);
        assert_eq!(Codec::from_key("?"), Codec::Vp9);
        assert_eq!(resolution_label(2160), "4K");
        assert_eq!(resolution_label(720), "720p");
    }

    #[test]
    fn low_power_tries_all_capped_formats_before_uncapped_fallbacks() {
        let vp9_sel = format_string(720, Codec::Vp9, false, true);
        let cap30 = "[height<=720][dynamic_range=?SDR][fps<=30]";
        let vp9 = "[vcodec~='^(vp09|vp9$)']";
        let no_av1 = "[vcodec!^=av01]";
        assert_eq!(
            vp9_sel,
            format!(
                "(bv*{cap30}{vp9}+ba/b{cap30}{vp9})/bv*{cap30}{no_av1}+ba/b{cap30}{no_av1}/bv*[height<=720][dynamic_range=?SDR]{no_av1}+ba/b[height<=720][dynamic_range=?SDR]{no_av1}/bv*{cap30}+ba/b{cap30}/{}",
                format_string(720, Codec::Vp9, false, false)
            )
        );

        // H.264 up to 720p allows 60 fps and excludes AV1 before falling back to any codec.
        let avc720 = format_string(720, Codec::Avc, false, true);
        assert!(avc720.starts_with(
            "(bv*[height<=720][dynamic_range=?SDR][fps<=?60][vcodec^=avc1]+ba/b[height<=720][dynamic_range=?SDR][fps<=?60][vcodec^=avc1])/"
        ));
        let no_av1_pos = avc720.find("[vcodec!^=av01]").expect("av1 excluded");
        let any_pos = avc720
            .find("/bv*[height<=720][dynamic_range=?SDR][fps<=30]+ba")
            .unwrap();
        assert!(no_av1_pos < any_pos);

        // Above 720p, H.264 prefers >720p at <=30 fps, then 720p60, keeping max_h as the first height<= match.
        let avc1080 = format_string(1080, Codec::Avc, false, true);
        assert!(avc1080.starts_with(
            "(bv*[height<=1080][height>720][dynamic_range=?SDR][fps<=30][vcodec^=avc1]+ba/"
        ));
        assert!(
            avc1080.contains(")/(bv*[height<=720][dynamic_range=?SDR][fps<=?60][vcodec^=avc1]+ba/")
        );

        // Best (0) carries no height<= cap so the OSD quality menu still detects Best.
        let avc_best = format_string(0, Codec::Avc, false, true);
        assert!(!avc_best.contains("height<="));
        assert!(avc_best.contains("[vcodec!^=av01]"));

        // Explicit AV1 choice does not insert an AV1 exclusion.
        let av1_sel = format_string(720, Codec::Av1, false, true);
        assert!(!av1_sel.contains("[vcodec!^=av01]"));
        assert!(
            av1_sel.starts_with("(bv*[height<=720][dynamic_range=?SDR][fps<=30][vcodec^=av01]+ba/")
        );
    }

    #[test]
    fn mpv_command_line() {
        let o = PlayOptions {
            low_power: false,
            buffer_mib: 256,
            video_id: "WhWc3b3KhnY".into(),
            title: "Spring\n".into(),
            start: 61.4,
            max_height: 2160,
            codec: Codec::Vp9,
            hdr: true,
            sub_langs: "en".into(),
            subs_on: false,
            accent: "7c6cf0".into(),
            hide_on_back: false,
            stats: false,
            hwdec: None,
        };
        let v = mpv_argv(
            &o,
            Path::new("/d"),
            Path::new("/ui"),
            Path::new("/run/s.sock"),
            Some("deno:/d/deno"),
        );
        // Waits for YouTube's address to become usable instead of failing.
        assert!(v.iter().any(
            |a| a.starts_with("--stream-lavf-o=") && a.contains("reconnect_on_http_error=403")
        ));
        assert_eq!(
            v.last().unwrap(),
            "https://www.youtube.com/watch?v=WhWc3b3KhnY"
        );
        assert!(v.contains(&"--start=61.400".to_string()));
        let mut short = o.clone();
        short.start = 0.5;
        assert!(
            mpv_argv(
                &short,
                Path::new("/d"),
                Path::new("/ui"),
                Path::new("/sock"),
                None
            )
            .contains(&"--start=0.500".to_string())
        );
        assert!(v.contains(&"--resume-playback=no".to_string()));
        assert!(v.contains(&"--speed=1".to_string()));
        assert!(v.contains(&"--force-media-title=Spring".to_string()));
        assert!(v.contains(&"--script-opts=ytdl_hook-ytdl_path=/d/yt-dlp,ytdl_hook-try_ytdl_first=yes,cattube_osd-accent=7c6cf0,cattube_osd-position_dir=/d/positions".to_string()));
        assert!(v.contains(&"--ytdl-raw-options=js-runtimes=deno:/d/deno,sub-langs=\"en,en-(?-i:[A-Z]).*,en-orig\",write-subs=,write-auto-subs=".to_string()));
        assert!(v.contains(&"--sid=no".to_string()));
        assert!(v.contains(&"--demuxer-max-bytes=256MiB".to_string()));
        assert!(v.contains(&"--input-ipc-server=/run/s.sock".to_string()));
        assert!(v.contains(&"--no-input-default-bindings".to_string()));
        // Zero starts normally; every positive saved position is passed.
        let o2 = PlayOptions {
            start: 0.0,
            sub_langs: String::new(),
            low_power: true,
            stats: true,
            hwdec: hwdec_choice(None, false, true),
            ..o
        };
        let v2 = mpv_argv(
            &o2,
            Path::new("/d"),
            Path::new("/ui"),
            Path::new("/s"),
            None,
        );
        assert!(!v2.iter().any(|a| a.starts_with("--start")));
        assert!(!v2.iter().any(|a| a.starts_with("--ytdl-raw-options")));
        assert!(v2.iter().any(|a| a == "--hwdec=auto-safe"));
        assert!(v2.iter().any(|a| a == "--profile=fast"));
        assert!(v2.iter().any(|a| a == "--scale=bilinear"));
        assert!(v2.iter().any(|a| a == "--dither-depth=no"));
        assert!(v2.iter().any(|a| a.contains("cattube_osd-low_power=yes")));
        assert!(v2.iter().any(|a| a.contains("cattube_osd-stats=yes")));
        assert!(!v.iter().any(|a| a.contains("cattube_osd-stats")));
        assert!(
            v2.iter()
                .any(|a| a.starts_with("--ytdl-format=") && a.contains("[fps<=30]"))
        );
    }

    #[test]
    fn js_runtime_choice() {
        let none = |_: &str| None;
        assert_eq!(js_runtime_with_env(Path::new("/nope"), &none, None), None);
        let sys = |n: &str| (n == "node").then(|| PathBuf::from("/usr/bin/node"));
        assert_eq!(
            js_runtime_with_env(Path::new("/nope"), &sys, None).as_deref(),
            Some("node:/usr/bin/node")
        );
        let qjs = |n: &str| (n == "qjs").then(|| PathBuf::from("/usr/bin/qjs"));
        assert_eq!(
            js_runtime_with_env(Path::new("/nope"), &qjs, None).as_deref(),
            Some("quickjs:/usr/bin/qjs")
        );
        let quickjs = |n: &str| (n == "quickjs").then(|| PathBuf::from("/usr/bin/quickjs"));
        assert_eq!(
            js_runtime_with_env(Path::new("/nope"), &quickjs, None).as_deref(),
            Some("quickjs:/usr/bin/quickjs")
        );
        let deno = |n: &str| (n == "deno").then(|| PathBuf::from("/usr/bin/deno"));
        assert_eq!(
            js_runtime_with_env(Path::new("/nope"), &deno, None).as_deref(),
            Some("deno:/usr/bin/deno")
        );
        // Env override takes precedence when valid; malformed/unknown values are ignored.
        assert_eq!(
            js_runtime_with_env(Path::new("/nope"), &deno, Some("quickjs:/usr/bin/qjs")).as_deref(),
            Some("quickjs:/usr/bin/qjs")
        );
        assert_eq!(
            js_runtime_with_env(Path::new("/nope"), &deno, Some("evil:/bin/sh")).as_deref(),
            Some("deno:/usr/bin/deno")
        );
        assert_eq!(
            js_runtime_with_env(Path::new("/nope"), &deno, Some("quickjs:/bin/qjs,foo=bar"))
                .as_deref(),
            Some("deno:/usr/bin/deno")
        );
    }

    #[test]
    fn an_unfinished_own_deno_is_not_a_runtime() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("cattube-deno-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let deno = own_deno_path(&root);
        std::fs::write(&deno, b"truncated").unwrap();
        std::fs::set_permissions(&deno, std::fs::Permissions::from_mode(0o644)).unwrap();
        let none = |_: &str| None;
        assert_eq!(js_runtime(&root, &none), None);
        std::fs::set_permissions(&deno, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            js_runtime(&root, &none),
            Some(format!("deno:{}", deno.display()))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn mpv_runs_the_unpacked_yt_dlp_once_it_is_in_place() {
        let root = std::env::temp_dir().join(format!("cattube-ytu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(yt_dlp_runner(&root), yt_dlp_path(&root));
        std::fs::write(yt_dlp_launcher(&root), yt_dlp_launcher_script(&root)).unwrap();
        // A launcher without its folder (an interrupted unpack) is not used.
        assert_eq!(yt_dlp_runner(&root), yt_dlp_path(&root));
        std::fs::create_dir_all(yt_dlp_dir(&root)).unwrap();
        std::fs::write(yt_dlp_dir(&root).join("__main__.py"), "").unwrap();
        assert_eq!(yt_dlp_runner(&root), yt_dlp_launcher(&root));
        assert_eq!(yt_dlp_version_from_unpacked(&root), None);
        std::fs::create_dir_all(yt_dlp_dir(&root).join("yt_dlp")).unwrap();
        std::fs::write(
            yt_dlp_dir(&root).join("yt_dlp/version.py"),
            "# comment\n__version__ = '2026.11.20'\nRELEASE_GIT_HEAD = 'abc'\n",
        )
        .unwrap();
        assert_eq!(
            yt_dlp_version_from_unpacked(&root).as_deref(),
            Some("2026.11.20")
        );
        assert_eq!(
            parse_yt_dlp_version("__version__ = \"2026.01.02\"").as_deref(),
            Some("2026.01.02")
        );
        assert_eq!(parse_yt_dlp_version("__version__ = ''"), None);
        let _ = std::fs::remove_dir_all(root);
        // The folder is quoted for the shell, even with a quote in it.
        let s = yt_dlp_launcher_script(Path::new("/home/o'neil/d"));
        assert!(
            s.ends_with("exec python3 '/home/o'\\''neil/d/yt-dlp.d' \"$@\"\n"),
            "{s}"
        );
    }

    #[test]
    fn checksum_lists() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!("{a}  yt-dlp.tar.gz\n{b}  yt-dlp\nshort  other\n{a} *star.zip\n");
        assert_eq!(expected_sha256(&sums, "yt-dlp"), Some("b".repeat(64)));
        assert_eq!(expected_sha256(&sums, "star.zip"), Some(a));
        assert_eq!(expected_sha256(&sums, "other"), None);
        assert_eq!(expected_sha256(&sums, "missing"), None);
    }

    #[test]
    fn sha256_command_hashes_a_file() {
        use crate::process::Runner;
        let p = std::env::temp_dir().join(format!("cattube-sha-{}", std::process::id()));
        std::fs::write(&p, b"abc").unwrap();
        let out = crate::process::SystemRunner.run(&sha256_cmd(&p)).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(
            out.stdout.trim(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn positions() {
        let mut l = parse_positions("WhWc3b3KhnY 120\nbad\nB4EPW7JUMTM 30\n");
        assert_eq!(l.len(), 2);
        remember(&mut l, "WhWc3b3KhnY", 200.0, Some(1000.0));
        assert_eq!(l.last().unwrap(), &("WhWc3b3KhnY".to_string(), 200.0));
        // Stopping near the end still keeps the unwatched final seconds.
        remember(&mut l, "B4EPW7JUMTM", 990.0, Some(1000.0));
        assert!(l.iter().any(|(i, p)| i == "B4EPW7JUMTM" && *p == 990.0));
        assert_eq!(
            positions_text(&l),
            "WhWc3b3KhnY 200.000\nB4EPW7JUMTM 990.000\n"
        );
    }
    #[test]
    fn exact_player_checkpoint_wins_over_delayed_watcher_save() {
        let root = std::env::temp_dir().join(format!(
            "cattube-positions-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("positions")).unwrap();
        std::fs::write(
            root.join("positions.conf"),
            "WhWc3b3KhnY 100\nB4EPW7JUMTM 900\n",
        )
        .unwrap();
        std::fs::write(
            root.join("positions/WhWc3b3KhnY.pos"),
            "123.456 130.000 0\n",
        )
        .unwrap();
        std::fs::write(
            root.join("positions/B4EPW7JUMTM.pos"),
            "1000.000 1000.000 1\n",
        )
        .unwrap();
        let (loaded, checkpoints) = load_positions_and_checkpoints(&root);
        assert_eq!(loaded, vec![("WhWc3b3KhnY".into(), 123.456)]);
        assert_eq!(checkpoints.len(), 2);
        assert!(checkpoints["B4EPW7JUMTM"].finished);
        assert_eq!(checkpoints["WhWc3b3KhnY"].duration, Some(130.0));
        assert_eq!(parse_positions(&positions_text(&loaded)), loaded);
        std::fs::write(root.join("positions/WhWc3b3KhnY.pos"), "0.500 20.000 0\n").unwrap();
        assert_eq!(load_positions(&root), vec![("WhWc3b3KhnY".into(), 0.5)]);
        std::fs::write(root.join("positions/WhWc3b3KhnY.pos"), "NaN 20 0\n").unwrap();
        assert!(checkpoint(&root, "WhWc3b3KhnY").is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_positions_merged_creates_dir_merges_and_leaves_no_tmp_files() {
        let root = std::env::temp_dir().join(format!(
            "cattube-pos-merge-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        save_positions_merged(&root, &[("WhWc3b3KhnY".into(), 42.5, Some(200.0))]).unwrap();
        save_positions_merged(
            &root,
            &[
                ("B4EPW7JUMTM".into(), 10.0, Some(100.0)),
                ("WhWc3b3KhnY".into(), 88.0, Some(200.0)),
            ],
        )
        .unwrap();
        assert_eq!(
            load_positions(&root),
            vec![("B4EPW7JUMTM".into(), 10.0), ("WhWc3b3KhnY".into(), 88.0),]
        );
        let leftovers: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn position_records_round_trip_and_compact_checkpoints() {
        let root = std::env::temp_dir().join(format!(
            "cattube-pos-compact-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("positions")).unwrap();
        // Legacy 2-field + new 4-field + malformed lines.
        let mixed = "WhWc3b3KhnY 100\nB4EPW7JUMTM 300.000 300.000 1\nccccccccccc 10 20\nddddddddddd 10 20 9\n";
        let records = parse_position_records(mixed);
        assert_eq!(records.len(), 2);
        assert_eq!(
            records[0],
            (
                "WhWc3b3KhnY".into(),
                Checkpoint {
                    position: 100.0,
                    duration: None,
                    finished: false,
                }
            )
        );
        assert_eq!(
            records[1],
            (
                "B4EPW7JUMTM".into(),
                Checkpoint {
                    position: 300.0,
                    duration: Some(300.0),
                    finished: true,
                }
            )
        );
        assert_eq!(
            parse_position_records(&position_records_text(&records)),
            records
        );
        // parse_positions excludes finished 4-field entries.
        assert_eq!(parse_positions(mixed), vec![("WhWc3b3KhnY".into(), 100.0)]);

        // save_positions_merged prefers .pos checkpoints and removes them once merged.
        store_checkpoint(
            &root,
            "WhWc3b3KhnY",
            Checkpoint {
                position: 145.5,
                duration: Some(250.0),
                finished: false,
            },
        )
        .unwrap();
        store_checkpoint(
            &root,
            "B4EPW7JUMTM",
            Checkpoint {
                position: 600.0,
                duration: Some(600.0),
                finished: true,
            },
        )
        .unwrap();
        save_positions_merged(
            &root,
            &[
                ("WhWc3b3KhnY".into(), 120.0, Some(250.0)),
                ("B4EPW7JUMTM".into(), 598.0, Some(600.0)),
            ],
        )
        .unwrap();
        assert!(!root.join("positions/WhWc3b3KhnY.pos").exists());
        assert!(!root.join("positions/B4EPW7JUMTM.pos").exists());
        // A failed/immediate exit (pos == 0 with no .pos) must not erase saved progress.
        save_positions_merged(&root, &[("WhWc3b3KhnY".into(), 0.0, None)]).unwrap();
        let (resumable, checkpoints) = load_positions_and_checkpoints(&root);
        assert_eq!(resumable, vec![("WhWc3b3KhnY".into(), 145.5)]);
        assert_eq!(checkpoints["WhWc3b3KhnY"].duration, Some(250.0));
        assert!(checkpoints["B4EPW7JUMTM"].finished);
        assert_eq!(checkpoints["B4EPW7JUMTM"].duration, Some(600.0));

        // compact_checkpoints folds leftover legacy/crash .pos files and removes invalid ones.
        store_checkpoint(
            &root,
            "aaaaaaaaaaa",
            Checkpoint {
                position: 42.0,
                duration: Some(90.0),
                finished: false,
            },
        )
        .unwrap();
        std::fs::write(root.join("positions/bbbbbbbbbbb.pos"), "NaN 20 0\n").unwrap();
        compact_checkpoints(&root).unwrap();
        assert_eq!(
            std::fs::read_dir(root.join("positions")).unwrap().count(),
            0
        );
        let (resumable, checkpoints) = load_positions_and_checkpoints(&root);
        assert_eq!(
            resumable,
            vec![("WhWc3b3KhnY".into(), 145.5), ("aaaaaaaaaaa".into(), 42.0)]
        );
        assert_eq!(checkpoints["aaaaaaaaaaa"].duration, Some(90.0));
        assert!(checkpoints["B4EPW7JUMTM"].finished);
        std::fs::remove_dir_all(root).unwrap();
    }
}
