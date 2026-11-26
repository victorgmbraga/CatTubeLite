//! CatTube's network and playback jobs (run on background threads). The
//! YouTube API and the screens are in `cattube_core`; this
//! file only runs `curl` / `mpv` / yt-dlp and turns the answers into data.

use cattube_core::community;
use cattube_core::mpv::{self, Arg};
use cattube_core::player::{self, PlayOptions};
use cattube_core::process::{Cmd, Runner, SystemRunner};
use cattube_core::youtube::{self as yt, Details, Page};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TIMEOUT: Duration = Duration::from_secs(25);

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn run(cmd: &Cmd) -> Result<String, String> {
    match SystemRunner.run(cmd) {
        Ok(o) if o.success() => Ok(o.stdout),
        Ok(o) => Err(friendly(&format!("{} {}", o.stderr, o.stdout))),
        Err(e) => Err(friendly(&e.to_string())),
    }
}

/// A request's failure in a sentence for the TV.
pub fn friendly(t: &str) -> String {
    let t = t.to_ascii_lowercase();
    if t.contains("could not resolve")
        || t.contains("failed to connect")
        || t.contains("network is unreachable")
    {
        "No internet connection".into()
    } else if t.contains("timed out") || t.contains("timeout") {
        "YouTube took too long to answer".into()
    } else if t.contains("401") || t.contains("403") {
        "YouTube said no".into()
    } else {
        "YouTube didn't answer properly".into()
    }
}

/// Connections to YouTube kept open between requests: a new `curl` for
/// each one paid a process start and a TLS handshake every time (about
/// 0.1 s on a desktop, more on a TV box).
static AGENT: std::sync::LazyLock<ureq::Agent> = std::sync::LazyLock::new(|| {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(TIMEOUT)
        .max_idle_connections_per_host(4)
        .user_agent(yt::USER_AGENT)
        .build()
});

/// Persistent HTTP/TLS pool for thumbnails, avatars and plain GETs (e.g.
/// SponsorBlock, search suggestions) without YouTube InnerTube headers.
static PLAIN_AGENT: std::sync::LazyLock<ureq::Agent> = std::sync::LazyLock::new(|| {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .max_idle_connections_per_host(4)
        .build()
});

/// Maximum accepted thumbnail / avatar download size (8 MiB).
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

fn read_response_text(reply: Result<ureq::Response, ureq::Error>) -> Result<String, String> {
    match reply {
        Ok(r) => r.into_string().map_err(|e| friendly(&e.to_string())),
        Err(ureq::Error::Status(code, _)) => Err(friendly(&format!("http {code}"))),
        Err(ureq::Error::Transport(t)) => Err(friendly(&match t.kind() {
            ureq::ErrorKind::Dns => "could not resolve".to_string(),
            ureq::ErrorKind::ConnectionFailed => "failed to connect".to_string(),
            _ => t.to_string(),
        })),
    }
}

/// One InnerTube request to YouTube: a POST of JSON when there is a `body`,
/// else a GET. The answer's text, or a sentence for the TV.
fn curl(url: &str, body: Option<&str>) -> Result<String, String> {
    let request = |method: &str| {
        AGENT
            .request(method, url)
            .set("X-YouTube-Client-Name", yt::CLIENT_NUMBER)
            .set("X-YouTube-Client-Version", yt::CLIENT_VERSION)
            .set("Origin", "https://www.youtube.com")
            .set("Referer", "https://www.youtube.com/tv")
    };
    let reply = match body {
        Some(b) => request("POST")
            .set("Content-Type", "application/json")
            .send_string(b),
        None => request("GET").call(),
    };
    read_response_text(reply)
}

/// Plain GET without YouTube InnerTube headers (SponsorBlock, suggestions).
fn get_plain(url: &str) -> Result<String, String> {
    read_response_text(PLAIN_AGENT.get(url).call())
}

// --------------------------------------------------------------- lists

pub fn page(ctx: &str, browse_id: &str) -> Result<Page, String> {
    if browse_id == yt::feeds::HOME {
        return explore(ctx);
    }
    let body = yt::browse_body(ctx, browse_id).ok_or("That can't be opened")?;
    let reply = curl(&yt::endpoint_url("browse"), Some(&body))?;
    yt::parse_page(&reply).ok_or_else(|| "YouTube didn't answer properly".into())
}

/// Home: YouTube's topic channels, fetched side by side.
fn explore(ctx: &str) -> Result<Page, String> {
    let handles: Vec<_> = yt::feeds::TOPICS
        .iter()
        .map(|(name, id)| {
            let (ctx, id) = (ctx.to_string(), id.to_string());
            (*name, std::thread::spawn(move || page(&ctx, &id)))
        })
        .collect();
    let mut topics = Vec::new();
    let mut error = None;
    for (name, h) in handles {
        match h.join() {
            Ok(Ok(p)) => topics.push((name, p)),
            Ok(Err(e)) => error = Some(e),
            Err(_) => {}
        }
    }
    let p = yt::explore(topics, 2);
    match (p.shelves.is_empty(), error) {
        (true, Some(e)) => Err(e),
        _ => Ok(p),
    }
}

pub fn more(ctx: &str, token_: &str) -> Result<Page, String> {
    let body = yt::continuation_body(ctx, token_).ok_or("bad continuation")?;
    let reply = curl(&yt::endpoint_url("browse"), Some(&body))?;
    yt::parse_page(&reply).ok_or_else(|| "YouTube didn't answer properly".into())
}

/// The next results of a search (its continuation).
pub fn search_more(ctx: &str, token_: &str) -> Result<Page, String> {
    let body = yt::continuation_body(ctx, token_).ok_or("bad continuation")?;
    let reply = curl(&yt::endpoint_url("search"), Some(&body))?;
    yt::parse_page(&reply).ok_or_else(|| "YouTube didn't answer properly".into())
}

pub fn search(ctx: &str, q: &str) -> Result<Page, String> {
    let reply = curl(&yt::endpoint_url("search"), Some(&yt::search_body(ctx, q)))?;
    yt::parse_page(&reply).ok_or_else(|| "YouTube didn't answer properly".into())
}

pub fn suggestions(q: &str, hl: &str) -> Vec<String> {
    get_plain(&yt::suggest_url_for(q, hl))
        .map(|b| yt::parse_suggestions(&b))
        .unwrap_or_default()
}

pub fn details(ctx: &str, id: &str) -> Result<Details, String> {
    let body = yt::next_body(ctx, id).ok_or("not a video")?;
    let reply = curl(&yt::endpoint_url("next"), Some(&body))?;
    yt::parse_details(&reply).ok_or_else(|| "YouTube didn't answer properly".into())
}

// ------------------------------------------------------------- pictures

/// `images/<hash>.<ext>` in the cache folder for a picture address.
fn image_path(cache: &Path, url: &str) -> PathBuf {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in url.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let ext = if url.contains(".webp") {
        "webp"
    } else if url.contains(".png") {
        "png"
    } else {
        "jpg"
    };
    cache.join("images").join(format!("{h:016x}.{ext}"))
}

/// A softened copy of a picture (backdrops): scaled down and blurred, written
/// atomically so concurrent readers never see a half-written JPEG.
fn blur_into(src: &Path, out: &Path) -> Option<()> {
    let img = image::load_from_memory(&std::fs::read(src).ok()?).ok()?;
    let small = img.resize(480, 270, image::imageops::FilterType::Triangle);
    let soft = image::imageops::blur(&small.to_rgb8(), 7.0);
    let _ = std::fs::create_dir_all(out.parent()?);
    let tmp = cattube_core::util::unique_tmp_path(out, "part");
    if soft
        .save_with_format(&tmp, image::ImageFormat::Jpeg)
        .is_ok()
        && std::fs::rename(&tmp, out).is_ok()
    {
        Some(())
    } else {
        let _ = std::fs::remove_file(&tmp);
        None
    }
}

/// Existing disk-cache path for `url`, if already downloaded.
pub fn cached_image_path(cache: &Path, url: &str) -> Option<String> {
    let base = url.strip_suffix("#blur").unwrap_or(url);
    if !cattube_core::mpv::is_playable_url(base) {
        return None;
    }
    let path = image_path(cache, url);
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

/// Remove a corrupt/undecodable cached image file.
pub fn remove_cached_image(cache: &Path, url: &str) {
    let _ = std::fs::remove_file(image_path(cache, url));
}

/// Download `url` into `target` atomically via persistent HTTP/TLS (`ureq`)
/// and return the raw bytes so callers can decode directly from memory.
fn download_image_bytes(url: &str, target: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let resp = PLAIN_AGENT.get(url).call().ok()?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_IMAGE_BYTES {
        return None;
    }
    if let Some(parent) = target.parent()
        && std::fs::create_dir_all(parent).is_ok()
    {
        let tmp = cattube_core::util::unique_tmp_path(target, "part");
        if std::fs::write(&tmp, &bytes).is_err() || std::fs::rename(&tmp, target).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    Some(bytes)
}

/// Download a picture into the cache; returns its bytes in memory (decoding
/// directly avoids re-reading from flash right after writing).
pub fn fetch_image_bytes(cache: &Path, url: &str) -> Option<Vec<u8>> {
    if let Some(base) = url.strip_suffix("#blur") {
        let path = image_path(cache, url);
        if let Ok(bytes) = std::fs::read(&path) {
            return Some(bytes);
        }
        let src = fetch_image(cache, base)?;
        blur_into(Path::new(&src), &path)?;
        return std::fs::read(&path).ok();
    }
    if !cattube_core::mpv::is_playable_url(url) {
        return None;
    }
    let path = image_path(cache, url);
    if let Ok(bytes) = std::fs::read(&path) {
        return Some(bytes);
    }
    download_image_bytes(url, &path)
}

/// Download a picture into the cache; returns its path. An address ending
/// in `#blur` gives the blurred copy (the picture behind a focused video).
pub fn fetch_image(cache: &Path, url: &str) -> Option<String> {
    if let Some(base) = url.strip_suffix("#blur") {
        let path = image_path(cache, url);
        if path.is_file() {
            return Some(path.to_string_lossy().into_owned());
        }
        let src = fetch_image(cache, base)?;
        blur_into(Path::new(&src), &path)?;
        return Some(path.to_string_lossy().into_owned());
    }
    if !cattube_core::mpv::is_playable_url(url) {
        return None;
    }
    let path = image_path(cache, url);
    if path.is_file() {
        return Some(path.to_string_lossy().into_owned());
    }
    download_image_bytes(url, &path)?;
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

/// Download a batch of missing pictures in-process (bounded parallelism over
/// persistent keep-alive connections) and return their bytes in memory so
/// callers can decode immediately without re-reading from disk.
pub fn fetch_missing_image_bytes(
    cache: &Path,
    urls: &[String],
    low_power: bool,
) -> Vec<(String, Option<Vec<u8>>)> {
    if urls.len() <= 1 {
        return urls
            .iter()
            .map(|u| (u.clone(), fetch_image_bytes(cache, u)))
            .collect();
    }
    let parallel_max = if low_power { 4 } else { 8 };
    let mut out = Vec::with_capacity(urls.len());
    for chunk in urls.chunks(parallel_max) {
        let results: Vec<(String, Option<Vec<u8>>)> = std::thread::scope(|s| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|u| s.spawn(move || (u.clone(), fetch_image_bytes(cache, u))))
                .collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        out.extend(results);
    }
    out
}

/// Evict oldest cached images when `cache/images` grows beyond `max_bytes` or
/// `max_files`, and clean up abandoned `.part` files left by crashed runs.
pub fn prune_image_cache(cache: &Path, max_bytes: u64, max_files: usize) {
    prune_image_cache_with_age(cache, max_bytes, max_files, Duration::from_secs(3600));
}

fn prune_image_cache_with_age(
    cache: &Path,
    max_bytes: u64,
    max_files: usize,
    part_max_age: Duration,
) {
    let dir = cache.join("images");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total_bytes: u64 = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        if path.extension().is_some_and(|e| e == "part") {
            let age = now.duration_since(modified).unwrap_or(Duration::ZERO);
            if age >= part_max_age {
                let _ = std::fs::remove_file(&path);
            }
            continue;
        }
        let len = meta.len();
        total_bytes = total_bytes.saturating_add(len);
        files.push((modified, len, path));
    }
    if files.len() <= max_files && total_bytes <= max_bytes {
        return;
    }
    files.sort_unstable_by_key(|(m, _, _)| *m);
    let mut remaining = files.len();
    for (_, len, path) in files {
        if remaining <= max_files && total_bytes <= max_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total_bytes = total_bytes.saturating_sub(len);
            remaining -= 1;
        }
    }
}

// ---------------------------------------------------------------- tools

static TOOLS_LOCK: Mutex<()> = Mutex::new(());

/// Make sure yt-dlp (and a JavaScript runtime for it) are there, and keep
/// yt-dlp up to date. Returns a line for Settings.
pub fn ensure_tools(data: &Path) -> Result<String, String> {
    ensure_tools_inner(data, false)
}

/// Update yt-dlp now (Settings).
pub fn update_tools(data: &Path) -> Result<String, String> {
    ensure_tools_inner(data, true)
}

fn ensure_tools_inner(data: &Path, force_update: bool) -> Result<String, String> {
    let _guard = TOOLS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = std::fs::create_dir_all(data);
    let ytdlp = player::yt_dlp_path(data);
    let checked = data.join("yt-dlp.checked");
    if !ytdlp.is_file() {
        let tmp = cattube_core::util::unique_tmp_path(&ytdlp, "part");
        if let Err(e) = download_verified(
            player::YT_DLP_URL,
            player::YT_DLP_SUMS_URL,
            player::YT_DLP_ASSET,
            &tmp,
        ) {
            eprintln!("cattube: yt-dlp: {e}");
            let _ = std::fs::remove_file(&tmp);
            return Err("Could not download yt-dlp".to_string());
        }
        set_exec(&tmp);
        if let Err(e) = std::fs::rename(&tmp, &ytdlp) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }
        touch(&checked);
    } else if force_update || stale(&checked) {
        match run(&player::update_cmd(&ytdlp)) {
            Ok(_) => touch(&checked),
            Err(e) if force_update => return Err(format!("Could not update yt-dlp ({e})")),
            Err(_) => {}
        }
    }
    unpack_yt_dlp(data);
    let find = |n: &str| cattube_core::process::find_executable(n);
    if player::js_runtime(data, &find).is_none() {
        // A Deno that cannot run on this device is not fetched again for a day.
        if force_update || stale(&data.join(DENO_FAILED)) {
            install_deno(data)?;
        }
    } else if find("deno").is_none()
        && cattube_core::process::find_executable(
            &player::own_deno_path(data).display().to_string(),
        )
        .is_some()
    {
        refresh_deno(data, force_update);
    }
    Ok(ytdlp_version(data))
}

/// Which single-file yt-dlp an unpacked copy came from (kept in it).
const YT_DLP_SOURCE: &str = ".cattube-source";

/// The single file's size and change time: it changes when yt-dlp updates.
fn yt_dlp_stamp(zipapp: &Path) -> Option<String> {
    let meta = std::fs::metadata(zipapp).ok()?;
    let t = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(format!(
        "{} {}.{:09}",
        meta.len(),
        t.as_secs(),
        t.subsec_nanos()
    ))
}

/// Keep an unpacked copy of yt-dlp next to the single file, made again
/// when that changes (see [`player::yt_dlp_dir`]). It is unpacked and
/// tried away from its place and then swapped in; on any failure mpv
/// simply keeps running the single file.
fn unpack_yt_dlp(data: &Path) {
    let zipapp = player::yt_dlp_path(data);
    let Some(stamp) = yt_dlp_stamp(&zipapp) else {
        return;
    };
    let dir = player::yt_dlp_dir(data);
    let launcher = player::yt_dlp_launcher(data);
    let script = player::yt_dlp_launcher_script(data);
    let current = std::fs::read_to_string(dir.join(YT_DLP_SOURCE)).is_ok_and(|s| s == stamp);
    if !current {
        let new = cattube_core::util::unique_tmp_path(&dir, "dir");
        let ready = std::fs::create_dir(&new).is_ok()
            && run(&player::unzip_all_cmd(&zipapp, &new)).is_ok()
            && new.join("__main__.py").is_file()
            && run(&player::warm_yt_dlp_cmd(&new)).is_ok()
            && std::fs::write(new.join(YT_DLP_SOURCE), &stamp).is_ok();
        if !ready {
            eprintln!("cattube: could not unpack yt-dlp; using the single file");
            let _ = std::fs::remove_dir_all(&new);
            return;
        }
        let old = cattube_core::util::unique_tmp_path(&dir, "old");
        let moved_old = dir.exists() && std::fs::rename(&dir, &old).is_ok();
        if std::fs::rename(&new, &dir).is_err() {
            if moved_old {
                let _ = std::fs::rename(&old, &dir);
            }
            let _ = std::fs::remove_dir_all(&new);
            return;
        }
        if moved_old {
            let _ = std::fs::remove_dir_all(&old);
        }
    }
    if std::fs::read_to_string(&launcher).ok().as_deref() != Some(script.as_str()) {
        let tmp = cattube_core::util::unique_tmp_path(&launcher, "tmp");
        if std::fs::write(&tmp, &script).is_ok() {
            set_exec(&tmp);
            if std::fs::rename(&tmp, &launcher).is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

const DENO_CHECKED: &str = "deno.checked";
const DENO_FAILED: &str = "deno.failed";
const DENO_SHA256: &str = "deno.sha256";

/// Download `url` to `out` and compare it with the release's published
/// checksum. Returns that checksum.
fn download_verified(url: &str, sums_url: &str, asset: &str, out: &Path) -> Result<String, String> {
    run(&player::download_cmd(url, out))?;
    let sums = run(&player::fetch_text_cmd(sums_url))?;
    let want = player::expected_sha256(&sums, asset)
        .ok_or_else(|| format!("no checksum published for {asset}"))?;
    let got = run(&player::sha256_cmd(out))?;
    if got.trim().eq_ignore_ascii_case(&want) {
        Ok(want)
    } else {
        Err(format!("checksum mismatch for {asset}"))
    }
}

/// Fetch, verify and try Deno away from its final place, then move it in
/// with one rename: an interruption never leaves a broken runtime.
fn install_deno(data: &Path) -> Result<(), String> {
    let zip = cattube_core::util::unique_tmp_path(&data.join("deno.zip"), "part");
    let dir = cattube_core::util::unique_tmp_path(&data.join("deno-new"), "dir");
    let result = (|| {
        let sum = download_verified(
            player::DENO_URL,
            player::DENO_SUM_URL,
            player::DENO_ASSET,
            &zip,
        )
        .map_err(|e| {
            eprintln!("cattube: Deno: {e}");
            "Could not download Deno (needed for full quality)".to_string()
        })?;
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        run(&player::unzip_cmd(&zip, &dir)).map_err(|_| "Could not unpack Deno".to_string())?;
        let new = player::own_deno_path(&dir);
        set_exec(&new);
        let check = SystemRunner.run(
            &Cmd::new(new.display().to_string())
                .arg("--version")
                .timeout(Duration::from_secs(30)),
        );
        match check {
            Ok(o) if o.success() => {}
            Ok(o) if o.timed_out => return Err("Deno did not start in time".to_string()),
            _ => {
                touch(&data.join(DENO_FAILED));
                return Err("Deno does not run on this device".to_string());
            }
        }
        std::fs::rename(&new, player::own_deno_path(data)).map_err(|e| e.to_string())?;
        let _ = std::fs::write(data.join(DENO_SHA256), &sum);
        let _ = std::fs::remove_file(data.join(DENO_FAILED));
        touch(&data.join(DENO_CHECKED));
        Ok(())
    })();
    let _ = std::fs::remove_file(&zip);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Keep CatTube's own Deno current; the installed one stays on any failure.
fn refresh_deno(data: &Path, force: bool) {
    let checked = data.join(DENO_CHECKED);
    if !force && !stale_after(&checked, player::DENO_UPDATE_EVERY) {
        return;
    }
    let Ok(sums) = run(&player::fetch_text_cmd(player::DENO_SUM_URL)) else {
        return;
    };
    let Some(latest) = player::expected_sha256(&sums, player::DENO_ASSET) else {
        return;
    };
    let installed = std::fs::read_to_string(data.join(DENO_SHA256)).unwrap_or_default();
    if installed.trim() == latest {
        touch(&checked);
    } else if let Err(e) = install_deno(data) {
        eprintln!("cattube: Deno update: {e}");
    }
}

pub fn ytdlp_version(data: &Path) -> String {
    if let Some(v) = player::yt_dlp_version_from_unpacked(data) {
        return format!("yt-dlp {v}");
    }
    let runner = player::yt_dlp_runner(data);
    SystemRunner
        .run(&Cmd::new(runner.display().to_string()).arg("--version"))
        .ok()
        .filter(|o| o.success())
        .map(|o| format!("yt-dlp {}", o.stdout.trim()))
        .unwrap_or_else(|| "yt-dlp is not there yet".into())
}

fn set_exec(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));
}

fn touch(p: &Path) {
    let tmp = cattube_core::util::unique_tmp_path(p, "tmp");
    if std::fs::write(&tmp, now().to_string()).is_err() || std::fs::rename(&tmp, p).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

fn stale(p: &Path) -> bool {
    stale_after(p, player::UPDATE_EVERY)
}

fn stale_after(p: &Path, every: Duration) -> bool {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .is_none_or(|t| now().saturating_sub(t) > every.as_secs())
}

// ---------------------------------------------------------------- player

/// Capture the actual player before replacing or closing it, not its last poll.
pub fn checkpoint_player(data: &Path) {
    let Ok(mut conn) = mpv::IpcConn::connect(&player::player_socket()) else {
        return;
    };
    let budget = Duration::from_millis(80);
    let Ok(path) = conn.command_with_timeout(&[Arg::Str("get_property"), Arg::Str("path")], budget)
    else {
        return;
    };
    let Some(path) = path.as_str() else { return };
    let id = path
        .split(['?', '&'])
        .find_map(|part| part.strip_prefix("v="))
        .or_else(|| path.split('?').next()?.rsplit('/').next())
        .unwrap_or("");
    if !yt::is_video_id(id) {
        return;
    }
    let get = |conn: &mut mpv::IpcConn, name: &str| {
        conn.command_with_timeout(&[Arg::Str("get_property"), Arg::Str(name)], budget)
            .ok()
            .and_then(|j| j.as_f64())
    };
    if let Some(position) = get(&mut conn, "time-pos") {
        let duration = get(&mut conn, "duration");
        let same_video = conn
            .command_with_timeout(&[Arg::Str("get_property"), Arg::Str("path")], budget)
            .ok()
            .is_some_and(|j| j.as_str() == Some(path));
        if !same_video {
            return;
        }
        let finished =
            player::checkpoint(data, id).is_some_and(|p| p.finished && position >= p.position);
        let _ = player::store_checkpoint(
            data,
            id,
            player::Checkpoint {
                position,
                duration,
                finished,
            },
        );
    }
}

/// Shared handle to a running `mpv` child so both the UI and the
/// watcher can check exit status and reap it without hanging.
#[derive(Clone)]
pub struct PlayerHandle {
    child: std::sync::Arc<Mutex<Option<std::process::Child>>>,
}

impl PlayerHandle {
    pub fn new(child: std::process::Child) -> Self {
        Self {
            child: std::sync::Arc::new(Mutex::new(Some(child))),
        }
    }

    /// True once the process has exited (reaping it immediately if so).
    pub fn exited(&self) -> bool {
        let mut slot = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let Some(child) = slot.as_mut() else {
            return true;
        };
        match child.try_wait() {
            Ok(Some(_)) => {
                *slot = None;
                true
            }
            Ok(None) => false,
            Err(_) => {
                *slot = None;
                true
            }
        }
    }

    /// Terminate the player if it is still running and reap it.
    pub fn stop_and_reap(&self) {
        let mut slot = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut child) = slot.take() else {
            return;
        };
        if !matches!(child.try_wait(), Ok(Some(_))) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Wait briefly for an already-closing player, then kill and reap so the
    /// caller can never deadlock on a wedged `mpv`.
    pub fn reap_or_kill(&self, grace: Duration) {
        let deadline = Instant::now() + grace;
        while !self.exited() {
            if Instant::now() >= deadline {
                self.stop_and_reap();
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// How long a player from another CatTube process gets to close.
const PLAYER_CLOSE_GRACE: Duration = Duration::from_millis(1500);

/// Start mpv for a video (its own process group, stdin closed).
pub fn start_player(o: &PlayOptions, data: &Path) -> Result<PlayerHandle, String> {
    std::fs::create_dir_all(data.join("positions"))
        .map_err(|e| format!("Could not save playback positions ({e})"))?;
    let find = |n: &str| cattube_core::process::find_executable(n);
    let js = player::js_runtime(data, &find);
    let argv = player::mpv_argv(
        o,
        data,
        &cattube_core::env::data_dir(),
        &player::player_socket(),
        js.as_deref(),
    );
    // One screen, one player: another CatTube (`--play`, a second window)
    // shares this socket, so whichever starts a video closes the other's
    // first (keeping its place).
    checkpoint_player(data);
    if !mpv::quit_and_wait(&player::player_socket(), PLAYER_CLOSE_GRACE) {
        eprintln!("cattube: the previous player did not close in time");
    }
    let _ = std::fs::remove_file(player::player_socket());
    std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(std::process::Stdio::null())
        // In CatTube's own process group: a TV shell that closes CatTube
        // (signalling the group) closes the player with it.
        .spawn()
        .map(PlayerHandle::new)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "mpv is not installed".to_string(),
            _ => format!("Could not start the player ({e})"),
        })
}

/// CatTube's own window (its Wayland app id) and the player's.
pub const APP_WINDOW: &str = "io.github.coderastic.CatTube";
pub const PLAYER_WINDOW: &str = "cattube-player";

/// Bring a window to the front through CatosTV (`catostv-ctl raise`).
pub fn raise_window(window: &str) -> bool {
    std::process::Command::new("catostv-ctl")
        .args(["raise", window])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// What the player loop needs from the app.
pub struct Session {
    pub low_power: bool,
    pub ctx: String,
    pub cache: PathBuf,
    pub autoplay: crate::prefs::Autoplay,
    pub sponsorblock: Option<Vec<String>>,
    pub codec: cattube_core::player::Codec,
    pub hdr: bool,
    /// Videos after this one (a playlist or a row), in order.
    pub queue: Vec<yt::Tile>,
    /// Back in the player hid the video (`hide_on_back`): told the title
    /// of what plays on behind CatTube's list.
    pub on_hide: Option<Box<dyn Fn(yt::Tile) + Send>>,
    pub on_video: Option<Box<dyn Fn(yt::Tile) + Send>>,
}

/// How a playback ended, for the app.
#[derive(Debug, Default)]
pub struct Ended {
    /// (video, position, duration) for every video that played.
    pub positions: Vec<(String, f64, Option<f64>)>,
}

fn osd(conn: &mut mpv::IpcConn, text: &str) {
    let _ = conn.command(&[Arg::Str("show-text"), Arg::Str(text), Arg::Num(2500.0)]);
}

fn card(conn: &mut mpv::IpcConn, args: &[&str]) {
    let mut v = vec![
        Arg::Str("script-message-to"),
        Arg::Str("cattube_osd"),
        Arg::Str("cattube-card"),
    ];
    v.extend(args.iter().map(|a| Arg::Str(a)));
    let _ = conn.command(&v);
}

fn thumbnail_size(osd_width: f64, width: f64) -> (u32, u32) {
    let scale = (osd_width / 1920.0).clamp(0.5, 4.0);
    ((width * scale) as u32, (width * 9.0 / 16.0 * scale) as u32)
}

/// Decode and scale raw image bytes directly into BGRA pixels in memory.
fn bgra_from_bytes(bytes: &[u8], w: u32, h: u32) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(bytes).ok()?;
    let mut px = img
        .resize_to_fill(w, h, image::imageops::FilterType::Triangle)
        .to_rgba8()
        .into_raw();
    for p in px.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    Some((px, w, h))
}

/// Decode and scale a thumbnail directly into BGRA bytes in memory.
/// Uses `mqdefault` (320×180, no black bars) whenever it already covers the
/// target slot or in `low_power` mode.
fn bgra_thumbnail_pixels(
    cache: &Path,
    id: &str,
    osd_width: f64,
    width: f64,
    low_power: bool,
) -> Option<(Vec<u8>, u32, u32)> {
    let (w, h) = thumbnail_size(osd_width, width);
    let url = if low_power || w <= 320 {
        yt::video_thumb_mq(id)
    } else {
        yt::video_thumb_hq(id)
    };
    let bytes = fetch_image_bytes(cache, &url)?;
    bgra_from_bytes(&bytes, w, h)
}

/// Raw thumbnail pixels written once for a player overlay.
fn bgra_thumbnail(
    cache: &Path,
    id: &str,
    osd_width: f64,
    width: f64,
    low_power: bool,
    name: &str,
) -> Option<(String, u32, u32)> {
    let (px, w, h) = bgra_thumbnail_pixels(cache, id, osd_width, width, low_power)?;
    let path = player::player_socket().with_file_name(name);
    std::fs::write(&path, px).ok()?;
    Some((path.to_string_lossy().into_owned(), w, h))
}

/// The related browser shows up to six rows (three on low-power devices),
/// loading pictures in row batches without per-thumbnail temp files.
const RELATED: usize = 24;
const RELATED_LOW_POWER: usize = 12;
const RELATED_THUMB_W: f64 = 390.0;
const RELATED_STEP_X: usize = 430;
const RELATED_STEP_Y: usize = 320;
const RELATED_ATLAS_W: usize = 1680;
const RELATED_FLUSH_BATCH: usize = 4;
const RELATED_FLUSH_INTERVAL: Duration = Duration::from_millis(400);

fn related_atlas_geometry(low_power: bool) -> (usize, usize, usize, f64, f64) {
    let div = if low_power { 2usize } else { 1usize };
    (
        RELATED_ATLAS_W / div,
        RELATED_STEP_X / div,
        RELATED_STEP_Y / div,
        RELATED_THUMB_W / div as f64,
        1.0 / div as f64,
    )
}

fn related_json(tiles: &[yt::Tile], atlas: Option<(&str, usize, usize, usize, f64)>) -> String {
    let list: Vec<_> = tiles
        .iter()
        .map(|t| {
            format!(
                "{{\"id\":{},\"t\":{},\"c\":{}}}",
                mpv::json_string(&t.id),
                mpv::json_string(&t.title),
                mpv::json_string(&t.byline),
            )
        })
        .collect();
    let atlas = atlas.map_or_else(
        || "null".to_string(),
        |(p, w, h, v, s)| {
            format!(
                "{{\"p\":{},\"w\":{w},\"h\":{h},\"v\":{v},\"s\":{s}}}",
                mpv::json_string(p)
            )
        },
    );
    format!("{{\"list\":[{}],\"atlas\":{atlas}}}", list.join(","))
}

#[derive(Default)]
struct RelatedRequests {
    generation: std::sync::atomic::AtomicU64,
    paths: std::sync::Mutex<Vec<String>>,
}

impl RelatedRequests {
    fn begin(&self) -> u64 {
        let next = self
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        for path in self
            .paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            let _ = std::fs::remove_file(path);
        }
        next
    }

    fn current(&self, expected: u64) -> bool {
        self.generation.load(std::sync::atomic::Ordering::Relaxed) == expected
    }

    fn keep(&self, path: &str, expected: u64) -> bool {
        let mut paths = self.paths.lock().unwrap_or_else(|e| e.into_inner());
        if self.current(expected) {
            paths.push(path.to_string());
            true
        } else {
            let _ = std::fs::remove_file(path);
            false
        }
    }
}

fn send_related(
    session: &Session,
    id: &str,
    request: &str,
    requests: &RelatedRequests,
    expected: u64,
) {
    let current = || requests.current(expected);
    let send = |json: &str| {
        if current() {
            let _ = mpv::ipc_request(
                &player::player_socket(),
                &[
                    Arg::Str("script-message"),
                    Arg::Str("cattube-related-data"),
                    Arg::Str(id),
                    Arg::Str(json),
                    Arg::Str(request),
                ],
            );
        }
    };
    let details = match details(&session.ctx, id) {
        Ok(d) => d,
        Err(e) => {
            send(&format!("{{\"error\":{}}}", mpv::json_string(&e)));
            return;
        }
    };
    let limit = if session.low_power {
        RELATED_LOW_POWER
    } else {
        RELATED
    };
    let tiles: Vec<_> = details
        .related
        .into_iter()
        .filter(|t| t.playable())
        .take(limit)
        .collect();
    if tiles.is_empty() {
        send("{\"error\":\"No related videos\"}");
        return;
    }
    send(&related_json(&tiles, None));
    // One transparent image plane keeps every thumbnail in the same frame.
    // On low-power devices (1 GiB RAM TV boxes), build the atlas at half scale
    // (840x480 = 1.54 MiB instead of 6.15 MiB) and source from mqdefault.
    let (atlas_w, step_x, step_y, thumb_w, atlas_scale) = related_atlas_geometry(session.low_power);
    let (tw, th) = thumbnail_size(1920.0, thumb_w);
    let height = tiles.len().div_ceil(4) * step_y;
    let mut pixels = vec![0u8; atlas_w * height * 4];
    let safe_id: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let path =
        player::player_socket().with_file_name(format!("cattube-related-{safe_id}-{request}.bgra"));
    let tmp = path.with_extension("part");
    let name = path.to_string_lossy().into_owned();
    let mut tracked = false;
    let mut revision = 0;
    let mut pending = 0usize;
    let mut sent_at = Instant::now();
    for (chunk_idx, chunk) in tiles.chunks(RELATED_FLUSH_BATCH).enumerate() {
        if !current() {
            return;
        }
        let urls: Vec<String> = chunk
            .iter()
            .map(|t| {
                if session.low_power {
                    yt::video_thumb_mq(&t.id)
                } else {
                    yt::video_thumb_hq(&t.id)
                }
            })
            .collect();
        let fetched = fetch_missing_image_bytes(&session.cache, &urls, session.low_power);
        for (offset, (_, raw)) in fetched.into_iter().enumerate() {
            if !current() {
                return;
            }
            let i = chunk_idx * RELATED_FLUSH_BATCH + offset;
            if let Some(raw) = raw
                && let Some((bytes, w, h)) = bgra_from_bytes(&raw, tw, th)
                && bytes.len() == w as usize * h as usize * 4
            {
                for row in 0..h as usize {
                    let dest = ((i / 4 * step_y + row) * atlas_w + i % 4 * step_x) * 4;
                    let source = row * w as usize * 4;
                    pixels[dest..dest + w as usize * 4]
                        .copy_from_slice(&bytes[source..source + w as usize * 4]);
                }
                pending += 1;
            }
            let flush = pending > 0
                && (pending >= RELATED_FLUSH_BATCH
                    || sent_at.elapsed() >= RELATED_FLUSH_INTERVAL
                    || i + 1 == tiles.len());
            if flush {
                if !current() {
                    return;
                }
                if std::fs::write(&tmp, &pixels).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
                    if !current() {
                        let _ = std::fs::remove_file(&path);
                        return;
                    }
                    if !tracked {
                        if !requests.keep(&name, expected) {
                            return;
                        }
                        tracked = true;
                    }
                    revision += 1;
                    send(&related_json(
                        &tiles,
                        Some((&name, atlas_w, height, revision, atlas_scale)),
                    ));
                    sent_at = Instant::now();
                    pending = 0;
                } else {
                    let _ = std::fs::remove_file(&tmp);
                }
            }
        }
    }
}

fn episode_controls(conn: &mut mpv::IpcConn, previous: bool, next: bool, waiting: bool) {
    let _ = conn.command(&[
        Arg::Str("script-message-to"),
        Arg::Str("cattube_osd"),
        Arg::Str("cattube-episodes"),
        Arg::Str(if previous { "yes" } else { "no" }),
        Arg::Str(if next { "yes" } else { "no" }),
    ]);
    let _ = conn.command(&[
        Arg::Str("set_property"),
        Arg::Str("keep-open"),
        Arg::Str(if next || waiting { "yes" } else { "no" }),
    ]);
}

struct CardThumbnail((String, u32, u32));
impl Drop for CardThumbnail {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0.0);
    }
}

/// Design-canvas width of the Up Next card thumbnail (`TW` in `cattube-osd.lua`).
const CARD_THUMB_W: f64 = 256.0;

fn fetch_card_thumbnail(
    cache: PathBuf,
    id: String,
    width: f64,
    low_power: bool,
) -> std::sync::mpsc::Receiver<Option<CardThumbnail>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let pic = if yt::is_video_id(&id) {
            bgra_thumbnail(
                &cache,
                &id,
                width,
                CARD_THUMB_W,
                low_power,
                &format!("cattube-next-{}-{serial}.bgra", std::process::id()),
            )
            .map(CardThumbnail)
        } else {
            None
        };
        let _ = tx.send(pic);
    });
    rx
}

/// Replace the active file with per-file options; falls back to the 3-argument
/// `loadfile` signature used by mpv < 0.38 if mpv rejects the 4-argument form.
fn loadfile_replace(conn: &mut mpv::IpcConn, url: &str, opts: &str) {
    let r = conn.command(&[
        Arg::Str("loadfile"),
        Arg::Str(url),
        Arg::Str("replace"),
        Arg::Num(-1.0),
        Arg::Str(opts),
    ]);
    if r.is_err() && !conn.closed() {
        let _ = conn.command(&[
            Arg::Str("loadfile"),
            Arg::Str(url),
            Arg::Str("replace"),
            Arg::Str(opts),
        ]);
    }
}

fn safe_media_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect()
}

/// Wait for mpv to answer with a playing video (up to a minute). `None`: the
/// player went away, or was stopped for never getting there.
fn connect_to_player(player: &PlayerHandle, socket: &Path) -> Option<mpv::IpcConn> {
    let started = Instant::now();
    loop {
        if player.exited() {
            return None;
        }
        if let Ok(mut c) = mpv::IpcConn::connect(socket)
            && c.get_f64("time-pos").is_some()
        {
            return Some(c);
        }
        if started.elapsed() > Duration::from_secs(60) {
            player.stop_and_reap();
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// YouTube's pick for what follows `id`, delivered on `tx`.
fn spawn_next_lookup(s: &Session, id: String, tx: std::sync::mpsc::Sender<Option<yt::Tile>>) {
    let ctx = s.ctx.clone();
    std::thread::spawn(move || {
        let next = details(&ctx, &id).ok().and_then(|d| {
            let id = d.autoplay?;
            Some(
                d.related
                    .into_iter()
                    .find(|t| t.id == id)
                    .unwrap_or(yt::Tile {
                        thumb: yt::video_thumb(&id),
                        id,
                        title: "Next video".into(),
                        ..Default::default()
                    }),
            )
        });
        let _ = tx.send(next);
    });
}

fn spawn_segments_lookup(url: String, tx: std::sync::mpsc::Sender<Vec<community::Segment>>) {
    std::thread::spawn(move || {
        let segments = get_plain(&url)
            .ok()
            .map(|b| community::parse_segments(&b))
            .unwrap_or_default();
        let _ = tx.send(segments);
    });
}

/// SponsorBlock: jump over the segment at `p`, once. True when the skip runs
/// to the end of the video, which then ends here: seeking past the real end
/// can leave mpv paused without reporting it.
fn skip_segment(
    conn: &mut mpv::IpcConn,
    segments: &[community::Segment],
    skipped: &mut Vec<f64>,
    p: f64,
    dur: Option<f64>,
    looping: bool,
) -> bool {
    let Some((to, cat)) = community::skip_at(segments, p, dur, skipped) else {
        return false;
    };
    if let Some(seg) = segments.iter().find(|x| p >= x.start && p < x.end) {
        skipped.push(seg.start);
    }
    let to_end = dur.is_some_and(|d| to >= d - 1.0);
    if looping && to_end {
        skipped.clear();
        let _ = conn.command(&[Arg::Str("seek"), Arg::Num(0.0), Arg::Str("absolute")]);
    } else if to_end {
        let _ = conn.command(&[Arg::Str("set_property"), Arg::Str("pause"), Arg::Str("yes")]);
    } else {
        let _ = conn.command(&[Arg::Str("seek"), Arg::Num(to), Arg::Str("absolute")]);
    }
    osd(
        conn,
        &format!("Skipped: {}", community::category_label(&cat)),
    );
    to_end && !looping
}

/// Quality from the control bar: same video, same place.
fn change_quality(conn: &mut mpv::IpcConn, s: &Session, cur: &yt::Tile, height: u32, at: f64) {
    let fmt = player::format_string(height, s.codec, s.hdr, s.low_power);
    let _ = conn.command(&[
        Arg::Str("set_property"),
        Arg::Str("ytdl-format"),
        Arg::Str(&fmt),
    ]);
    let title = safe_media_title(&cur.title);
    let opts = if title.trim().is_empty() {
        format!("start={at:.3}")
    } else {
        format!(
            "start={at:.3},force-media-title={}",
            mpv::option_value(&title)
        )
    };
    loadfile_replace(conn, &cur.watch_url(), &opts);
    osd(
        conn,
        &format!("Quality: {}", player::resolution_label(height)),
    );
}

/// The newest valid request for the related-videos browser.
fn related_request(msgs: &[Vec<String>]) -> Option<String> {
    msgs.iter()
        .rev()
        .find(|m| m.first().map(String::as_str) == Some("cattube-related"))
        .and_then(|m| m.get(1))
        .filter(|s| s.len() <= 20 && !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .cloned()
}

/// A new request supersedes prior work, including replies from the previous
/// video. The Lua browser preserves its focus as each thumbnail arrives.
fn spawn_related(
    s: &Session,
    id: String,
    request: String,
    requests: std::sync::Arc<RelatedRequests>,
) {
    let rs = Session {
        low_power: s.low_power,
        ctx: s.ctx.clone(),
        cache: s.cache.clone(),
        autoplay: s.autoplay,
        sponsorblock: None,
        codec: s.codec,
        hdr: s.hdr,
        queue: Vec::new(),
        on_hide: None,
        on_video: None,
    };
    let expected = requests.begin();
    std::thread::spawn(move || send_related(&rs, &id, &request, &requests, expected));
}

/// The last related video picked in the player, if a valid one.
fn picked_related(msgs: &[Vec<String>]) -> Option<yt::Tile> {
    msgs.iter()
        .filter(|m| m.first().map(String::as_str) == Some("cattube-related-pick"))
        .filter_map(|m| {
            let id = m.get(1)?;
            yt::is_video_id(id).then(|| yt::Tile {
                id: id.clone(),
                title: m.get(2).cloned().unwrap_or_default(),
                byline: m.get(3).cloned().unwrap_or_default(),
                thumb: yt::video_thumb(id),
                ..yt::Tile::default()
            })
        })
        .next_back()
}

/// The "Up next" card, counting down from five.
fn show_up_next(conn: &mut mpv::IpcConn, next: &yt::Tile, thumb: Option<&CardThumbnail>) {
    let mut args: Vec<String> = vec!["next".into(), next.title.clone(), "5".into()];
    if let Some(pic) = thumb {
        let (p, w, h) = &pic.0;
        args.extend([p.clone(), w.to_string(), h.to_string()]);
    }
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    card(conn, &a);
}

/// Wait for the replacement video itself to load (not the previous video's
/// leftover `time-pos` or `eof-reached`). False: the player is gone.
fn wait_for_video(conn: &mut mpv::IpcConn, player: &PlayerHandle, id: &str) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(45) {
        if conn.closed() || player.exited() {
            return false;
        }
        let switched = conn.get_str("path").is_some_and(|p| p.contains(id));
        if switched
            && conn.get_f64("time-pos").is_some()
            && !conn.get_bool("eof-reached").unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    true
}

/// Watch the player until it goes away: SponsorBlock skips, quality
/// changes from the control bar, and what plays
/// next (an "Up next" card counting down, a loop, or nothing). Runs on
/// its own thread; `done` is called when the player has gone.
pub fn spawn_watcher(
    first: yt::Tile,
    player: PlayerHandle,
    s: Session,
    done: impl FnOnce(Ended) + Send + 'static,
) {
    let _ = std::thread::Builder::new()
        .name("cattube-player".into())
        .spawn(move || {
            let socket = player::player_socket();
            let mut ended = Ended::default();
            let Some(mut conn) = connect_to_player(&player, &socket) else {
                done(ended);
                return;
            };
            let mut queue = s.queue.clone();
            let mut history: Vec<yt::Tile> = Vec::new();
            let mut cur = first;
            let related_requests = std::sync::Arc::new(RelatedRequests::default());
            'videos: loop {
                cattube_core::watched::record(&player::data_home(), &cur);
                if let Some(on_video) = &s.on_video {
                    on_video(cur.clone());
                }
                related_requests.begin();
                let _ = conn.command(&[
                    Arg::Str("script-message"),
                    Arg::Str("cattube-video"),
                    Arg::Str(&cur.id),
                ]);
                // Network jobs never block Back, stop or the control bar.
                let mut next_id = if s.autoplay == crate::prefs::Autoplay::Next {
                    queue.first().cloned()
                } else {
                    None
                };
                let mut next_pending =
                    s.autoplay == crate::prefs::Autoplay::Next && next_id.is_none();
                let (next_tx, next_rx) = std::sync::mpsc::channel();
                if next_pending {
                    spawn_next_lookup(&s, cur.id.clone(), next_tx);
                }
                episode_controls(
                    &mut conn,
                    !history.is_empty(),
                    next_id.is_some(),
                    next_pending,
                );
                let _ = conn.command(&[
                    Arg::Str("set_property"),
                    Arg::Str("loop-file"),
                    Arg::Str(if s.autoplay == crate::prefs::Autoplay::Loop {
                        "inf"
                    } else {
                        "no"
                    }),
                ]);
                let (segments_tx, segments_rx) = std::sync::mpsc::channel();
                if let Some(url) = s
                    .sponsorblock
                    .as_ref()
                    .and_then(|cats| community::segments_url(&cur.id, cats))
                {
                    spawn_segments_lookup(url, segments_tx);
                }
                let mut segments: Vec<community::Segment> = Vec::new();
                let _ = conn.command(&[
                    Arg::Str("script-message"),
                    Arg::Str("cattube-segments"),
                    Arg::Str(""),
                ]);
                let mut skipped: Vec<f64> = Vec::new();
                let mut last = (0.0f64, None::<f64>);
                // A live stream has no place to come back to (its time is
                // the stream's, and mpv cannot seek a start into it): it is
                // never saved. Known from the card, else asked once.
                let mut live = cur.kind == yt::Kind::Live;
                let mut live_asked = live;
                let mut card_until: Option<Instant> = None;
                let mut last_count: Option<u64> = None;
                let mut quit_sent = false;
                // A skip that runs to the end (end cards, outros) ends the
                // video here: seeking past the real end can leave mpv
                // paused without reporting it.
                let mut skipped_to_end = false;
                let mut thumb: Option<CardThumbnail> = None;
                let width = conn.get_f64("osd-width").unwrap_or(1920.0);
                let mut thumb_job = next_id.as_ref().map(|n| {
                    fetch_card_thumbnail(s.cache.clone(), n.id.clone(), width, s.low_power)
                });
                loop {
                    if let Ok(next) = next_rx.try_recv() {
                        next_id = next;
                        next_pending = false;
                        thumb_job = next_id.as_ref().map(|n| {
                            fetch_card_thumbnail(s.cache.clone(), n.id.clone(), width, s.low_power)
                        });
                        episode_controls(&mut conn, !history.is_empty(), next_id.is_some(), false);
                    }
                    if let Ok(found) = segments_rx.try_recv() {
                        segments = found;
                        let marks = segments
                            .iter()
                            .map(|x| format!("{:.1}-{:.1}", x.start, x.end))
                            .collect::<Vec<_>>()
                            .join(",");
                        let _ = conn.command(&[
                            Arg::Str("script-message"),
                            Arg::Str("cattube-segments"),
                            Arg::Str(&marks),
                        ]);
                    }
                    if let Some(job) = &thumb_job
                        && let Ok(pic) = job.try_recv()
                    {
                        thumb = pic;
                        thumb_job = None;
                        if card_until.is_some()
                            && let Some(pic) = &thumb
                        {
                            let (p, w, h) = &pic.0;
                            card(&mut conn, &["thumb", p, &w.to_string(), &h.to_string()]);
                        }
                    }
                    let msgs = conn.messages(Duration::from_millis(400));
                    if conn.closed() || player.exited() {
                        if !live {
                            ended.positions.push((cur.id.clone(), last.0, last.1));
                        }
                        break 'videos;
                    }
                    let pos = conn.get_f64("time-pos");
                    let dur = conn.get_f64("duration").filter(|d| *d > 0.0);
                    if pos.is_some() && !live_asked {
                        live_asked = true;
                        live =
                            conn.get_str("metadata/by-key/ytdl_is_live").as_deref() == Some("true");
                    }
                    if let Some(p) = pos {
                        last = (p, dur);
                    }
                    // SponsorBlock: jump over the segment once.
                    if let Some(p) = pos {
                        let looping = s.autoplay == crate::prefs::Autoplay::Loop;
                        skipped_to_end |=
                            skip_segment(&mut conn, &segments, &mut skipped, p, dur, looping);
                    }
                    let want = |name: &str| {
                        msgs.iter()
                            .filter(|m| m.first().map(String::as_str) == Some(name))
                            .filter_map(|m| m.get(1).cloned())
                            .next_back()
                    };
                    // Back with the controls hidden: CatTube's list comes
                    // over the video, which plays on. Raised from here: the
                    // list's window, hidden behind the video, handles
                    // nothing until it shows. Stop it if that fails.
                    if msgs
                        .iter()
                        .any(|m| m.first().map(String::as_str) == Some("cattube-hide"))
                    {
                        if raise_window(APP_WINDOW) {
                            if let Some(f) = &s.on_hide {
                                f(cur.clone());
                            }
                        } else {
                            quit_sent = true;
                            let _ = conn.command(&[Arg::Str("quit")]);
                        }
                    }
                    // Quality from the control bar: same video, same place.
                    if let Some(h) = want("cattube-quality").and_then(|h| h.parse::<u32>().ok()) {
                        change_quality(&mut conn, &s, &cur, h, last.0);
                    }
                    // A new request supersedes prior work, including replies from
                    // the previous video. The Lua browser preserves its focus as
                    // each thumbnail arrives.
                    if let Some(request) = related_request(&msgs) {
                        spawn_related(&s, cur.id.clone(), request, related_requests.clone());
                    }
                    // A related video picked: it plays next, right away.
                    let picked = picked_related(&msgs);
                    let jump = match want("cattube-episode").as_deref() {
                        Some("next") => next_id.clone(),
                        Some("prev") => history.pop(),
                        _ => None,
                    }
                    .or(picked);
                    let eof = conn.get_bool("eof-reached").unwrap_or(false) || skipped_to_end;
                    // The end: "Up next" counts down five seconds.
                    if eof && !quit_sent && !next_pending && card_until.is_none() && jump.is_none()
                    {
                        match &next_id {
                            Some(n) => {
                                show_up_next(&mut conn, n, thumb.as_ref());
                                card_until = Some(Instant::now() + Duration::from_secs(5));
                                last_count = Some(5);
                            }
                            None => {
                                quit_sent = true;
                                let _ = conn.command(&[Arg::Str("quit")]);
                            }
                        }
                    }
                    let result = want("cattube-card-result");
                    if let Some(until) = card_until {
                        let ms = until.saturating_duration_since(Instant::now()).as_millis();
                        let left = (ms as u64).div_ceil(1000);
                        if last_count != Some(left) {
                            card(&mut conn, &["count", &left.to_string()]);
                            last_count = Some(left);
                        }
                        if result.as_deref() == Some("cancel") || result.as_deref() == Some("stop")
                        {
                            card(&mut conn, &["hide"]);
                            quit_sent = true;
                            let _ = conn.command(&[Arg::Str("quit")]);
                            card_until = None;
                            last_count = None;
                        }
                    }
                    let go_next = jump.clone().or_else(|| {
                        card_until
                            .filter(|u| Instant::now() >= *u || result.as_deref() == Some("play"))
                            .and(next_id.clone())
                    });
                    if let Some(n) = go_next {
                        card(&mut conn, &["hide"]);
                        if !live {
                            ended.positions.push((cur.id.clone(), last.0, last.1));
                        }
                        let backwards = want("cattube-episode").as_deref() == Some("prev");
                        if !backwards {
                            history.push(cur.clone());
                            if queue.first().is_some_and(|q| q.id == n.id) {
                                queue.remove(0);
                            }
                        }
                        let title = safe_media_title(&n.title);
                        let opts = format!("force-media-title={}", mpv::option_value(&title));
                        loadfile_replace(&mut conn, &n.watch_url(), &opts);
                        // The end left it paused on the last frame (for the card).
                        let _ = conn.command(&[
                            Arg::Str("set_property"),
                            Arg::Str("pause"),
                            Arg::Str("no"),
                        ]);
                        cur = n;
                        // Wait for the replacement video itself to load (not the previous
                        // video's leftover `time-pos` or `eof-reached`).
                        if !wait_for_video(&mut conn, &player, &cur.id) {
                            break 'videos;
                        }
                        continue 'videos;
                    }
                }
            }
            related_requests.begin();
            done(ended);
        });
}

#[cfg(test)]
mod player_tests {
    use super::*;

    #[test]
    fn player_messages_keep_the_newest_valid_request_and_pick() {
        let m = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let msgs = vec![
            m(&["cattube-related", "1"]),
            m(&["cattube-related-pick", "aaaaaaaaaaa", "First", "Channel"]),
            m(&["cattube-related", "22"]),
            m(&["cattube-related-pick", "bbbbbbbbbbb", "Second"]),
            m(&["cattube-related-pick", "not a video id"]),
        ];
        assert_eq!(related_request(&msgs).as_deref(), Some("22"));
        let pick = picked_related(&msgs).unwrap();
        assert_eq!(
            (pick.id.as_str(), pick.title.as_str()),
            ("bbbbbbbbbbb", "Second")
        );
        assert_eq!(related_request(&[m(&["cattube-related", "1; rm"])]), None);
        assert!(picked_related(&[]).is_none());
    }

    #[test]
    fn downloads_must_match_their_published_checksum() {
        let root = std::env::temp_dir().join(format!("cattube-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let tool = root.join("tool");
        let sums = root.join("sums");
        let out = root.join("out");
        std::fs::write(&tool, b"abc").unwrap();
        let url = format!("file://{}", tool.display());
        let sums_url = format!("file://{}", sums.display());
        std::fs::write(
            &sums,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  tool\n",
        )
        .unwrap();
        assert!(download_verified(&url, &sums_url, "tool", &out).is_ok());
        std::fs::write(&tool, b"tampered").unwrap();
        assert!(download_verified(&url, &sums_url, "tool", &out).is_err());
        assert!(download_verified(&url, &sums_url, "absent", &out).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn card_thumbnail_matches_osd_slot_dimensions() {
        assert_eq!(thumbnail_size(1920.0, CARD_THUMB_W), (256, 144));
        assert_eq!(thumbnail_size(1280.0, CARD_THUMB_W), (170, 96));
        assert_eq!(thumbnail_size(3840.0, CARD_THUMB_W), (512, 288));
        // Low-power related atlas is half scale (4x less RAM/I/O) and tiles fit cleanly.
        let (aw, sx, sy, tw, scale) = related_atlas_geometry(true);
        assert_eq!((aw, sx, sy, scale), (840, 215, 160, 0.5));
        let (w, h) = thumbnail_size(1920.0, tw);
        assert_eq!(3 * sx + w as usize, aw);
        assert!((h as usize) < sy);
    }

    #[test]
    fn player_handle_detects_early_exit_and_kills_stuck_child() {
        let fast = std::process::Command::new("sh")
            .args(["-c", "exit 0"])
            .spawn()
            .unwrap();
        let handle = PlayerHandle::new(fast);
        let t0 = Instant::now();
        while !handle.exited() && t0.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(handle.exited());

        let stuck = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let stuck_handle = PlayerHandle::new(stuck);
        assert!(!stuck_handle.exited());
        let at = Instant::now();
        stuck_handle.reap_or_kill(Duration::from_millis(40));
        assert!(stuck_handle.exited());
        assert!(at.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn loadfile_replace_falls_back_to_three_arg_signature_for_older_mpv() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixStream;
        let (a, mut b) = UnixStream::pair().unwrap();
        let server = std::thread::spawn(move || {
            let mut reader = BufReader::new(b.try_clone().unwrap());
            let mut seen = Vec::new();
            for reply_err in ["invalid parameter", "success"] {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let req = mpv::parse_json(line.trim()).unwrap();
                let id = req.get("request_id").unwrap().as_f64().unwrap() as u64;
                let cmd = req.get("command").unwrap().as_arr().unwrap().len();
                seen.push(cmd);
                writeln!(
                    b,
                    r#"{{"error":"{reply_err}","data":null,"request_id":{id}}}"#
                )
                .unwrap();
            }
            seen
        });
        let mut conn = mpv::IpcConn::from_stream(a).unwrap();
        loadfile_replace(
            &mut conn,
            "https://www.youtube.com/watch?v=WhWc3b3KhnY",
            "start=1.000",
        );
        drop(conn);
        // 5 elements: ["loadfile", url, "replace", -1, opts] -> fallback to 4 elements without -1.
        assert_eq!(server.join().unwrap(), vec![5, 4]);
    }

    #[test]
    fn ureq_image_fetch_downloads_caches_and_decodes_in_memory() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let root = std::env::temp_dir().join(format!(
            "cattube-img-http-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut jpeg = Vec::new();
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([12, 34, 56]));
        img.write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = jpeg.clone();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut req = [0u8; 512];
            let _ = s.read(&mut req);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            s.write_all(head.as_bytes()).unwrap();
            s.write_all(&body).unwrap();
        });
        let url = format!("http://{addr}/thumb.jpg");
        let got = fetch_missing_image_bytes(&root, std::slice::from_ref(&url), true);
        server.join().unwrap();
        assert_eq!(got.len(), 1);
        let bytes = got[0].1.as_ref().expect("downloaded bytes");
        assert_eq!(bytes, &jpeg);
        assert!(cached_image_path(&root, &url).is_some());
        assert!(crate::images::decode_bytes(bytes, 320).is_some());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn prune_image_cache_evicts_oldest_files_and_stale_parts() {
        let root = std::env::temp_dir().join(format!(
            "cattube-cache-prune-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let img = root.join("images");
        std::fs::create_dir_all(&img).unwrap();
        let write_sleep = |name: &str, size: usize| {
            let p = img.join(name);
            std::fs::write(&p, vec![b'x'; size]).unwrap();
            std::thread::sleep(Duration::from_millis(15));
            p
        };
        let old1 = write_sleep("01.jpg", 100);
        let old2 = write_sleep("02.jpg", 100);
        let new1 = write_sleep("03.jpg", 100);
        let new2 = write_sleep("04.jpg", 100);
        let stale_part = write_sleep("abandoned.part", 50);

        // Max 2 files and 250 bytes, and prune any .part immediately.
        prune_image_cache_with_age(&root, 250, 2, Duration::ZERO);
        assert!(!old1.exists());
        assert!(!old2.exists());
        assert!(!stale_part.exists());
        assert!(new1.exists());
        assert!(new2.exists());

        // Byte cap below 200 evicts the next oldest image too.
        prune_image_cache_with_age(&root, 150, 10, Duration::from_secs(3600));
        assert!(!new1.exists());
        assert!(new2.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn fetch_images_batch_uses_cached_entries_blurs_atomically_and_rejects_bad_urls() {
        let root = std::env::temp_dir().join(format!(
            "cattube-img-batch-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let url = "https://i.ytimg.com/vi/WhWc3b3KhnY/hqdefault.jpg";
        let blur_url = format!("{url}#blur");
        let cached = image_path(&root, url);
        std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
        let img = image::RgbImage::from_pixel(16, 9, image::Rgb([30, 60, 90]));
        img.save_with_format(&cached, image::ImageFormat::Jpeg)
            .unwrap();

        let res = fetch_missing_image_bytes(
            &root,
            &[
                url.to_string(),
                blur_url.clone(),
                "javascript:alert(1)".to_string(),
            ],
            true,
        );
        assert_eq!(res.len(), 3);
        assert!(res[0].1.as_ref().is_some_and(|b| !b.is_empty()));
        assert!(res[1].1.as_ref().is_some_and(|b| !b.is_empty()));
        assert!(image_path(&root, &blur_url).is_file());
        assert!(res[2].1.is_none());
        // No leftover .part files.
        let parts: Vec<_> = std::fs::read_dir(root.join("images"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "part"))
            .collect();
        assert!(parts.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
