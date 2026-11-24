//! YouTube's TV interface (InnerTube, the `TVHTML5` client, the one
//! YouTube's TV app uses): request bodies and the parsing of its answers
//! into plain [`Tile`]s, [`Shelf`]s and [`Details`].
//!
//! Pure: it builds requests and reads replies; the app's jobs send them
//! with `curl`. Facts (checked against live replies, kept in `testdata/`):
//!
//! | What | Request (`POST https://www.youtube.com/youtubei/v1/…`) |
//! |---|---|
//! | Home | `browse` of YouTube's topic channels (see [`feeds::TOPICS`]) |
//! | A channel / playlist | `browse` `UC…` / `VL…` |
//! | Search | `search` `query` |
//! | A video's page | `next` `videoId` (details, related) |
//! | More of a list | the same endpoint with `continuation` |
//!
//! Lists come as shelves (`shelfRenderer`) of tiles, either the older
//! `tileRenderer` or the newer `lockupViewModel`; both become a [`Tile`].

use crate::mpv::{Json, json_string, parse_json};

pub const API: &str = "https://www.youtube.com/youtubei/v1";
/// The YouTube TV app's client (CatTube presents itself as that app).
pub const CLIENT_NAME: &str = "TVHTML5";
pub const CLIENT_NUMBER: &str = "7";
pub const CLIENT_VERSION: &str = "7.20260101.00.00";
/// The TV app's user agent (Cobalt, YouTube's TV browser).
pub const USER_AGENT: &str = "Mozilla/5.0 (ChromiumStylePlatform) Cobalt/25.lts.30.1034943-gold (unlike Gecko), Unknown_TV_Unknown_0/Unknown (Unknown, Unknown)";

// ------------------------------------------------------------------- model

/// What a tile opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Kind {
    #[default]
    Video,
    Short,
    Live,
    Channel,
    Playlist,
}

/// One tile: a video, a Short, a live stream, a channel or a playlist.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tile {
    pub kind: Kind,
    /// Video id, channel id (`UC…`) or playlist id (`PL…`, without `VL`).
    pub id: String,
    pub title: String,
    /// The channel ("Blender Studio") or, for a channel, its handle line.
    pub byline: String,
    pub channel_id: String,
    /// "10M views • 7 years ago".
    pub meta: String,
    /// "7:45", "LIVE", "24 videos".
    pub badge: String,
    pub thumb: String,
    /// 0–1 already watched, when known.
    pub progress: Option<f32>,
}

impl Tile {
    /// What to `browse` to open a channel or playlist tile.
    pub fn browse_id(&self) -> Option<String> {
        match self.kind {
            Kind::Channel => Some(self.id.clone()),
            Kind::Playlist => Some(format!("VL{}", self.id)),
            _ => None,
        }
    }

    pub fn playable(&self) -> bool {
        matches!(self.kind, Kind::Video | Kind::Short | Kind::Live)
    }

    /// The YouTube address of a video (for yt-dlp).
    pub fn watch_url(&self) -> String {
        watch_url(&self.id)
    }
}

pub fn watch_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

/// A titled row of tiles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shelf {
    pub title: String,
    pub tiles: Vec<Tile>,
    /// Ask for more of this row.
    pub continuation: Option<String>,
    /// Opens the whole list ("Popular videos", a playlist): a browse id.
    pub more: Option<String>,
}

/// A page: its rows, and more rows to ask for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Page {
    pub title: String,
    pub shelves: Vec<Shelf>,
    pub continuation: Option<String>,
}

impl Page {
    /// Every tile of every row (search results, playlists).
    pub fn tiles(&self) -> Vec<Tile> {
        self.shelves.iter().flat_map(|s| s.tiles.clone()).collect()
    }
}

/// A video's page: what it is, who made it, what to watch next.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Details {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub channel_id: String,
    pub channel_thumb: String,
    pub subscribers: String,
    pub date: String,
    pub views: String,
    pub description: String,
    /// The video YouTube plays next on its own.
    pub autoplay: Option<String>,
    pub related: Vec<Tile>,
}

// ------------------------------------------------------------------- text

/// `{simpleText}`, `{runs:[{text}]}` or `{content}`.
pub fn text(v: Option<&Json>) -> String {
    let Some(v) = v else {
        return String::new();
    };
    if let Some(s) = v.get("simpleText").and_then(Json::as_str) {
        return s.trim().to_string();
    }
    if let Some(s) = v.get("content").and_then(Json::as_str) {
        return s.trim().to_string();
    }
    if let Some(runs) = v.get("runs").and_then(Json::as_arr) {
        return runs
            .iter()
            .filter_map(|r| r.get("text").and_then(Json::as_str))
            .collect::<String>()
            .trim()
            .to_string();
    }
    v.as_str().unwrap_or("").trim().to_string()
}

fn path<'a>(v: &'a Json, keys: &[&str]) -> Option<&'a Json> {
    let mut cur = v;
    for k in keys {
        cur = cur.get(k)?;
    }
    Some(cur)
}

/// The largest thumbnail of a `{thumbnails:[…]}` or `{sources:[…]}`.
fn best_thumb(v: Option<&Json>) -> String {
    let Some(v) = v else {
        return String::new();
    };
    let list = v
        .get("thumbnails")
        .or_else(|| v.get("sources"))
        .and_then(Json::as_arr)
        .unwrap_or(&[]);
    let mut best: Option<(f64, &str)> = None;
    for t in list {
        let Some(url) = t.get("url").and_then(Json::as_str) else {
            continue;
        };
        let w = t.get("width").and_then(Json::as_f64).unwrap_or(0.0);
        if best.is_none_or(|(bw, _)| w > bw) {
            best = Some((w, url));
        }
    }
    let url = best.map_or("", |(_, u)| u);
    // Protocol-relative avatars ("//yt3.ggpht.com/…").
    if let Some(rest) = url.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        url.to_string()
    }
}

/// Smaller video pictures for TV boxes (`low_power`), set at start.
static SMALL_PICTURES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Draw cards from the smaller video pictures (see [`video_thumb`]).
pub fn use_small_pictures(on: bool) {
    SMALL_PICTURES.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// A video's picture for its card: a plain JPEG from `i.ytimg.com`,
/// without the signed resizing query. `hqdefault` (480×360, the video
/// letterboxed in it); with small pictures `mqdefault` (320×180, no bars),
/// which a TV box draws sharper (its cards take pictures up to 320 px, so
/// hqdefault is halved) from a third of the pixels and a smaller download.
pub fn video_thumb(id: &str) -> String {
    video_thumb_sized(
        id,
        SMALL_PICTURES.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// [`video_thumb`] at the larger size, whatever the device (the player's
/// row of related videos draws them bigger than the cards).
pub fn video_thumb_hq(id: &str) -> String {
    video_thumb_sized(id, false)
}

fn video_thumb_sized(id: &str, small: bool) -> String {
    let name = if small { "mqdefault" } else { "hqdefault" };
    format!("https://i.ytimg.com/vi/{id}/{name}.jpg")
}

pub fn is_video_id(s: &str) -> bool {
    s.len() == 11
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Channel, playlist and continuation ids: letters, digits, `-`, `_`,
/// `=`, `%`, bounded.
fn is_safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 4096
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'=' | b'%'))
}

// ------------------------------------------------------------------- tiles

fn command_target(cmd: Option<&Json>) -> (Option<String>, Option<String>) {
    let Some(cmd) = cmd else {
        return (None, None);
    };
    let cmd = cmd.get("innertubeCommand").unwrap_or(cmd);
    let video = path(cmd, &["watchEndpoint", "videoId"])
        .or_else(|| path(cmd, &["reelWatchEndpoint", "videoId"]))
        .and_then(Json::as_str)
        .map(str::to_string);
    let browse = path(cmd, &["browseEndpoint", "browseId"])
        .and_then(Json::as_str)
        .map(str::to_string);
    (video, browse)
}

/// The older TV tile.
fn parse_tile_renderer(t: &Json) -> Option<Tile> {
    let ct = t.get("contentType").and_then(Json::as_str).unwrap_or("");
    let (video, browse) = command_target(t.get("onSelectCommand"));
    let mut meta = path(t, &["metadata", "tileMetadataRenderer"]);
    let mut lines_key = "lines";
    // Carousel tiles (the big rows on topic pages) keep their title and
    // channel in what they show on focus.
    if text(meta.and_then(|m| m.get("title"))).is_empty()
        && let Some(spot) = t
            .get("onFocusCommand")
            .and_then(|c| find_named(c, "entityMetadataRenderer"))
    {
        meta = Some(spot);
        lines_key = "bylines";
    }
    let title = text(meta.and_then(|m| m.get("title")));
    // Lines: the first is the channel, the rest views / date / badges.
    let mut lines: Vec<String> = Vec::new();
    if let Some(ls) = meta.and_then(|m| m.get(lines_key)).and_then(Json::as_arr) {
        for l in ls {
            let items = path(l, &["lineRenderer", "items"])
                .and_then(Json::as_arr)
                .unwrap_or(&[]);
            let parts: Vec<String> = items
                .iter()
                .filter_map(|i| i.get("lineItemRenderer"))
                .map(|i| text(i.get("text")))
                .filter(|s| !s.is_empty() && s != "•")
                .collect();
            if !parts.is_empty() {
                lines.push(parts.join(" • "));
            }
        }
    }
    let header = path(t, &["header", "tileHeaderRenderer"]);
    let mut badge = String::new();
    let mut progress = None;
    let mut live = false;
    if let Some(ovs) = header
        .and_then(|h| h.get("thumbnailOverlays"))
        .and_then(Json::as_arr)
    {
        for o in ovs {
            if let Some(ts) = o.get("thumbnailOverlayTimeStatusRenderer") {
                badge = text(ts.get("text"));
                live |= ts.get("style").and_then(Json::as_str) == Some("LIVE");
            }
            if let Some(p) = o.get("thumbnailOverlayResumePlaybackRenderer") {
                progress = p
                    .get("percentDurationWatched")
                    .and_then(Json::as_f64)
                    .map(|p| (p / 100.0) as f32);
            }
        }
    }
    let thumb = best_thumb(header.and_then(|h| h.get("thumbnail")));
    let content_id = t.get("contentId").and_then(Json::as_str).unwrap_or("");
    let (kind, id) = match ct {
        "TILE_CONTENT_TYPE_CHANNEL" => (Kind::Channel, browse.unwrap_or_else(|| content_id.into())),
        "TILE_CONTENT_TYPE_PLAYLIST" => (
            Kind::Playlist,
            browse
                .as_deref()
                .and_then(|b| b.strip_prefix("VL"))
                .map(str::to_string)
                .unwrap_or_else(|| content_id.into()),
        ),
        "TILE_CONTENT_TYPE_SHORTS" => (Kind::Short, video.unwrap_or_else(|| content_id.into())),
        _ => (
            if live || badge == "LIVE" {
                Kind::Live
            } else {
                Kind::Video
            },
            video.unwrap_or_else(|| content_id.into()),
        ),
    };
    finish(Tile {
        kind,
        id,
        title,
        byline: lines.first().cloned().unwrap_or_default(),
        meta: lines.get(1..).map(|l| l.join(" • ")).unwrap_or_default(),
        badge,
        thumb,
        progress,
        ..Tile::default()
    })
}

/// The newer tile (search, Shorts shelves).
fn parse_lockup(t: &Json) -> Option<Tile> {
    let ct = t.get("contentType").and_then(Json::as_str).unwrap_or("");
    let id = t
        .get("contentId")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let meta = path(t, &["metadata", "lockupMetadataViewModel"]);
    let title = text(meta.and_then(|m| m.get("title")));
    let rows = meta
        .and_then(|m| path(m, &["metadata", "contentMetadataViewModel", "metadataRows"]))
        .and_then(Json::as_arr)
        .unwrap_or(&[]);
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            r.get("metadataParts")
                .and_then(Json::as_arr)
                .unwrap_or(&[])
                .iter()
                .map(|p| text(p.get("text")))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" • ")
        })
        .filter(|s| !s.is_empty())
        .collect();
    let image = path(t, &["contentImage", "thumbnailViewModel"]).or_else(|| {
        path(
            t,
            &[
                "contentImage",
                "collectionThumbnailViewModel",
                "primaryThumbnail",
                "thumbnailViewModel",
            ],
        )
    });
    let thumb = best_thumb(image.and_then(|i| i.get("image")));
    let mut badge = String::new();
    let mut progress = None;
    let mut live = false;
    for o in image
        .and_then(|i| i.get("overlays"))
        .and_then(Json::as_arr)
        .unwrap_or(&[])
    {
        let badges = o
            .get("thumbnailBottomOverlayViewModel")
            .or_else(|| o.get("thumbnailOverlayBadgeViewModel"))
            .and_then(|b| b.get("badges").or_else(|| b.get("thumbnailBadges")))
            .and_then(Json::as_arr)
            .unwrap_or(&[]);
        for b in badges {
            if let Some(bv) = b.get("thumbnailBadgeViewModel") {
                badge = bv
                    .get("text")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                live |= bv.get("badgeStyle").and_then(Json::as_str)
                    == Some("THUMBNAIL_OVERLAY_BADGE_STYLE_LIVE");
            }
        }
        if let Some(p) = path(
            o,
            &[
                "thumbnailBottomOverlayViewModel",
                "progressBar",
                "thumbnailOverlayProgressBarViewModel",
                "startPercent",
            ],
        ) {
            progress = p.as_f64().map(|p| (p / 100.0) as f32);
        }
    }
    // Shorts lockups carry the title in their accessibility text only.
    let title = if title.is_empty() {
        path(t, &["overlayMetadata", "primaryText"])
            .map(|p| text(Some(p)))
            .unwrap_or_default()
    } else {
        title
    };
    let kind = match ct {
        "LOCKUP_CONTENT_TYPE_CHANNEL" => Kind::Channel,
        "LOCKUP_CONTENT_TYPE_PLAYLIST" | "LOCKUP_CONTENT_TYPE_PODCAST" => Kind::Playlist,
        "LOCKUP_CONTENT_TYPE_SHORT" => Kind::Short,
        _ if live || badge == "LIVE" => Kind::Live,
        _ => Kind::Video,
    };
    finish(Tile {
        kind,
        id,
        title,
        byline: lines.first().cloned().unwrap_or_default(),
        meta: lines.get(1..).map(|l| l.join(" • ")).unwrap_or_default(),
        badge,
        thumb,
        progress,
        ..Tile::default()
    })
}

/// Keep a tile only when its id is sound for its kind; videos get the
/// plain picture address.
fn finish(mut t: Tile) -> Option<Tile> {
    let ok = match t.kind {
        Kind::Video | Kind::Short | Kind::Live => is_video_id(&t.id),
        Kind::Channel => t.id.starts_with("UC") && is_safe_id(&t.id),
        Kind::Playlist => is_safe_id(&t.id),
    };
    if !ok {
        return None;
    }
    if t.playable() {
        t.thumb = video_thumb(&t.id);
    }
    if t.title.is_empty() {
        t.title = match t.kind {
            Kind::Short => "Short".into(),
            _ => "Untitled".into(),
        };
    }
    Some(t)
}

/// Any tile object (`{tileRenderer}`, `{lockupViewModel}`, `{videoRenderer}`).
pub fn parse_tile(v: &Json) -> Option<Tile> {
    if let Some(t) = v.get("tileRenderer") {
        return parse_tile_renderer(t);
    }
    if let Some(t) = v.get("lockupViewModel") {
        return parse_lockup(t);
    }
    None
}

/// Every tile under `v`, in order (not looking into nested shelves twice).
fn collect_tiles(v: &Json, out: &mut Vec<Tile>) {
    if let Some(t) = parse_tile(v) {
        if !out.iter().any(|x| x.id == t.id && x.kind == t.kind) {
            out.push(t);
        }
        return;
    }
    match v {
        Json::Obj(kv) => {
            for (_, x) in kv {
                collect_tiles(x, out);
            }
        }
        Json::Arr(a) => {
            for x in a {
                collect_tiles(x, out);
            }
        }
        _ => {}
    }
}

/// A continuation token anywhere under `v` (the first one).
fn find_continuation(v: &Json) -> Option<String> {
    match v {
        Json::Obj(kv) => {
            for (k, x) in kv {
                if k == "nextContinuationData"
                    || k == "reloadContinuationData"
                    || k == "continuationCommand"
                {
                    let tok = x
                        .get("continuation")
                        .or_else(|| x.get("token"))
                        .and_then(Json::as_str);
                    if let Some(t) = tok.filter(|t| is_safe_id(t)) {
                        return Some(t.to_string());
                    }
                }
                if let Some(t) = find_continuation(x) {
                    return Some(t);
                }
            }
            None
        }
        Json::Arr(a) => a.iter().find_map(find_continuation),
        _ => None,
    }
}

/// A page's token belongs to the outer list, not to one of its shelves.
fn page_continuation(v: &Json) -> Option<String> {
    match v {
        Json::Obj(kv) => {
            for (key, value) in kv {
                if key == "shelfRenderer" {
                    continue;
                }
                if matches!(
                    key.as_str(),
                    "continuations" | "continuationItemRenderer" | "continuationItemView"
                ) {
                    if let Some(token) = find_continuation(value) {
                        return Some(token);
                    }
                } else if let Some(token) = page_continuation(value) {
                    return Some(token);
                }
            }
            None
        }
        Json::Arr(items) => items.iter().find_map(page_continuation),
        _ => None,
    }
}

fn shelf_title(sh: &Json) -> (String, Option<String>) {
    let header = path(sh, &["headerRenderer", "shelfHeaderRenderer"]);
    let lock = header.and_then(|h| path(h, &["avatarLockup", "avatarLockupRenderer", "title"]));
    let title = text(
        lock.or_else(|| header.and_then(|h| h.get("title")))
            .or_else(|| sh.get("title")),
    );
    // The title often links to the whole list (a channel tab, a playlist).
    let more = lock
        .and_then(|t| t.get("runs"))
        .and_then(Json::as_arr)
        .and_then(|r| r.first())
        .and_then(|r| path(r, &["navigationEndpoint", "browseEndpoint", "browseId"]))
        .and_then(Json::as_str)
        .filter(|b| is_safe_id(b))
        .map(str::to_string);
    (title, more)
}

/// Shelves under `v`, in order.
fn collect_shelves(v: &Json, out: &mut Vec<Shelf>) {
    if let Some(sh) = v.get("shelfRenderer") {
        let (title, more) = shelf_title(sh);
        let mut tiles = Vec::new();
        collect_tiles(sh.get("content").unwrap_or(sh), &mut tiles);
        if !tiles.is_empty() {
            out.push(Shelf {
                title,
                continuation: sh.get("content").and_then(find_continuation),
                tiles,
                more,
            });
        }
        return;
    }
    match v {
        Json::Obj(kv) => {
            for (_, x) in kv {
                collect_shelves(x, out);
            }
        }
        Json::Arr(a) => {
            for x in a {
                collect_shelves(x, out);
            }
        }
        _ => {}
    }
}

/// A `browse` or `search` reply as a page of rows. A list without rows (a
/// playlist, a grid) becomes one row.
pub fn parse_page(body: &str) -> Option<Page> {
    let v = parse_json(body.trim())?;
    if v.get("error").is_some() {
        return None;
    }
    let contents = v
        .get("contents")
        .or_else(|| v.get("continuationContents"))
        .unwrap_or(&v);
    let mut shelves = Vec::new();
    collect_shelves(contents, &mut shelves);
    if shelves.is_empty() {
        let mut tiles = Vec::new();
        collect_tiles(contents, &mut tiles);
        if !tiles.is_empty() {
            shelves.push(Shelf {
                tiles,
                ..Shelf::default()
            });
        }
    }
    let continuation = page_continuation(contents).or_else(|| {
        // A single list (playlist, grid) continues itself.
        (shelves.len() == 1)
            .then(|| find_continuation(contents))
            .flatten()
    });
    let title = [
        &["header", "tvSurfaceHeaderRenderer", "title"][..],
        &["metadata", "channelMetadataRenderer", "title"][..],
    ]
    .iter()
    .find_map(|p| path(&v, p).map(|t| text(Some(t))))
    .or_else(|| find_named(&v, "entityMetadataRenderer").map(|m| text(m.get("title"))))
    .or_else(|| find_named(&v, "channelHeaderRenderer").map(|m| text(m.get("title"))))
    .unwrap_or_default();
    Some(Page {
        title,
        shelves,
        continuation,
    })
}

fn find_named<'a>(v: &'a Json, name: &str) -> Option<&'a Json> {
    match v {
        Json::Obj(kv) => {
            for (k, x) in kv {
                if k == name {
                    return Some(x);
                }
                if let Some(f) = find_named(x, name) {
                    return Some(f);
                }
            }
            None
        }
        Json::Arr(a) => a.iter().find_map(|x| find_named(x, name)),
        _ => None,
    }
}

/// A `next` reply: the video's details and what to watch after it.
pub fn parse_details(body: &str) -> Option<Details> {
    let v = parse_json(body.trim())?;
    let meta = find_named(&v, "videoMetadataRenderer")?;
    let owner = path(meta, &["owner", "videoOwnerRenderer"]);
    let mut related = Vec::new();
    collect_tiles(&v, &mut related);
    let id = meta
        .get("videoId")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    related.retain(|t| t.id != id);
    let autoplay = find_named(&v, "autoplayVideoRenderer")
        .and_then(|a| find_named(a, "watchEndpoint"))
        .and_then(|w| w.get("videoId"))
        .and_then(Json::as_str)
        .filter(|i| is_video_id(i))
        .map(str::to_string);
    Some(Details {
        title: text(meta.get("title")),
        channel: text(owner.and_then(|o| o.get("title"))),
        channel_id: owner
            .and_then(|o| path(o, &["navigationEndpoint", "browseEndpoint", "browseId"]))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        channel_thumb: best_thumb(owner.and_then(|o| o.get("thumbnail"))),
        subscribers: text(owner.and_then(|o| o.get("subscriberCountText"))),
        date: text(meta.get("dateText")),
        views: text(meta.get("viewCountText").or_else(|| {
            find_named(meta, "videoViewCountRenderer").and_then(|r| r.get("viewCount"))
        })),
        description: text(meta.get("description")),
        autoplay,
        related,
        id,
    })
}

// --------------------------------------------------------------- requests

/// The `context` every request carries.
pub fn context(hl: &str, gl: &str) -> String {
    format!(
        "{{\"client\":{{\"clientName\":\"{CLIENT_NAME}\",\"clientVersion\":\"{CLIENT_VERSION}\",\"hl\":{},\"gl\":{}}}}}",
        json_string(hl),
        json_string(gl)
    )
}

pub fn endpoint_url(endpoint: &str) -> String {
    format!("{API}/{endpoint}?prettyPrint=false")
}

/// `browse` a page (`FEwhat_to_watch`, `UC…`, `VL…`).
pub fn browse_body(ctx: &str, browse_id: &str) -> Option<String> {
    is_safe_id(browse_id).then(|| {
        format!(
            "{{\"context\":{ctx},\"browseId\":{}}}",
            json_string(browse_id)
        )
    })
}

/// More of a list.
pub fn continuation_body(ctx: &str, token: &str) -> Option<String> {
    is_safe_id(token).then(|| {
        format!(
            "{{\"context\":{ctx},\"continuation\":{}}}",
            json_string(token)
        )
    })
}

pub fn search_body(ctx: &str, query: &str) -> String {
    format!(
        "{{\"context\":{ctx},\"query\":{}}}",
        json_string(query.trim())
    )
}

pub fn next_body(ctx: &str, video_id: &str) -> Option<String> {
    is_video_id(video_id).then(|| {
        format!(
            "{{\"context\":{ctx},\"videoId\":{}}}",
            json_string(video_id)
        )
    })
}

/// Search suggestions while typing.
pub fn suggest_url(query: &str) -> String {
    suggest_url_for(query, "en")
}

/// Search suggestions while typing, localised to `hl`.
pub fn suggest_url_for(query: &str, hl: &str) -> String {
    let hl = if hl.is_empty() || !hl.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        "en"
    } else {
        hl
    };
    format!(
        "https://suggestqueries-clients6.youtube.com/complete/search?client=youtube&ds=yt&hl={hl}&q={}",
        crate::util::percent_encode(query)
    )
}

/// The suggestions reply: JSON (`["q",[["one",0],…]]`), maybe wrapped
/// in `window.google.ac.h(…)`.
pub fn parse_suggestions(body: &str) -> Vec<String> {
    let s = body.trim();
    let s = s
        .find('[')
        .and_then(|a| s.rfind(']').map(|b| &s[a..=b]))
        .unwrap_or("");
    let Some(Json::Arr(top)) = parse_json(s) else {
        return Vec::new();
    };
    top.get(1)
        .and_then(Json::as_arr)
        .unwrap_or(&[])
        .iter()
        .filter_map(|e| match e {
            Json::Arr(a) => a.first().and_then(Json::as_str),
            other => other.as_str(),
        })
        .map(str::to_string)
        .take(8)
        .collect()
}

/// Home without the rows YouTube adds on its own: "Recommended shows"
/// (TV series from anywhere, in any language), seasonal promotions ("Your
/// home for festivities, fashion, and fireworks") and rows of Shorts.
pub fn tidy_home(p: &mut Page) {
    p.shelves.retain(|s| {
        s.title != "Recommended shows"
            && !s.title.starts_with("Your home for")
            && !s.tiles.iter().all(|t| t.kind == Kind::Short)
    });
}

/// The browse ids of CatTube's sections.
pub mod feeds {
    /// Home's key; its rows come from [`TOPICS`].
    pub const HOME: &str = "FEwhat_to_watch";
    /// History's key; the list itself is kept on this TV.
    pub const HISTORY: &str = "FEhistory";
    /// YouTube's own topic channels (Live, Gaming, News and Sports); they
    /// answer signed out too.
    pub const TOPICS: [(&str, &str); 4] = [
        ("Live", "UC4R8DWoMoI7CAwX8_LjQHig"),
        ("Gaming", "UCOpNcN46UbXVtpKMrmU4Abg"),
        ("News", "UCYfdidRxbB8Qhf0Nx7ioOYw"),
        ("Sports", "UCEgdi0XIXXZ-qJOFPf4JSKw"),
    ];
}

/// Home for someone not signed in (YouTube's Home then only asks to sign
/// in): the first rows of each topic channel, named after the topic.
pub fn explore(topics: Vec<(&str, Page)>, per_topic: usize) -> Page {
    let mut shelves = Vec::new();
    for (topic, page) in topics {
        for sh in page
            .shelves
            .into_iter()
            .filter(|s| !s.tiles.is_empty())
            .take(per_topic)
        {
            let title = if sh.title.trim().is_empty() || sh.title == "?" {
                topic.to_string()
            } else if sh.title.to_lowercase().contains(&topic.to_lowercase()) {
                sh.title
            } else {
                format!("{topic}: {}", sh.title)
            };
            shelves.push(Shelf { title, ..sh });
        }
    }
    Page {
        title: "Home".into(),
        shelves,
        continuation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let p = format!("{}/testdata/{name}.json", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(p).expect("fixture")
    }

    #[test]
    fn small_pictures_are_the_plain_16_9_jpeg() {
        assert_eq!(
            video_thumb_sized("dQw4w9WgXcQ", false),
            "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg"
        );
        assert_eq!(
            video_thumb_sized("dQw4w9WgXcQ", true),
            "https://i.ytimg.com/vi/dQw4w9WgXcQ/mqdefault.jpg"
        );
        assert_eq!(
            video_thumb_hq("dQw4w9WgXcQ"),
            video_thumb_sized("dQw4w9WgXcQ", false)
        );
    }

    #[test]
    fn carousel_tiles_take_their_spotlight_title() {
        let body = r#"{"contents":[{"shelfRenderer":{"content":{"horizontalListRenderer":{"items":[{"tileRenderer":{"style":"TILE_STYLE_YTLR_CAROUSEL","header":{"tileHeaderRenderer":{"thumbnail":{"thumbnails":[{"url":"https://i.ytimg.com/vi/TxeCpgGK9Ok/hqdefault.jpg","width":320,"height":180}]}}},"onSelectCommand":{"watchEndpoint":{"videoId":"TxeCpgGK9Ok"}},"onFocusCommand":{"commandExecutorCommand":{"commands":[{"updateCarouselHeaderCommand":{"spotlight":{"entityMetadataRenderer":{"title":{"runs":[{"text":"LIVE | All night"}]},"bylines":[{"lineRenderer":{"items":[{"lineItemRenderer":{"text":{"simpleText":"Some Streamer"}}}]}}]}}}}]}},"contentId":"TxeCpgGK9Ok","contentType":"TILE_CONTENT_TYPE_VIDEO"}}]}}}}]}"#;
        let p = parse_page(body).unwrap();
        let t = &p.shelves[0].tiles[0];
        assert_eq!(
            (t.id.as_str(), t.title.as_str(), t.byline.as_str()),
            ("TxeCpgGK9Ok", "LIVE | All night", "Some Streamer")
        );
    }

    #[test]
    fn explore_names_rows_after_topics() {
        let shelf = |t: &str, n: usize| Shelf {
            title: t.into(),
            tiles: vec![Tile::default(); n],
            continuation: None,
            more: None,
        };
        let page = |s: Vec<Shelf>| Page {
            title: String::new(),
            shelves: s,
            continuation: None,
        };
        let p = explore(
            vec![
                (
                    "Live",
                    page(vec![shelf("", 3), shelf("Live Now", 4), shelf("Recent", 2)]),
                ),
                (
                    "Gaming",
                    page(vec![shelf("Empty", 0), shelf("Top live games", 5)]),
                ),
            ],
            2,
        );
        let titles: Vec<&str> = p.shelves.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Live", "Live Now", "Gaming: Top live games"]);
    }

    #[test]
    fn search_page_has_rows_of_lockups() {
        let p = parse_page(&fixture("search")).unwrap();
        assert!(p.shelves.len() >= 2);
        assert_eq!(p.shelves[0].title, "Search results for blender open movie");
        let t = &p.shelves[0].tiles[0];
        assert_eq!(
            (t.kind, t.id.as_str(), t.title.as_str()),
            (Kind::Video, "WhWc3b3KhnY", "Spring - Blender Open Movie")
        );
        assert_eq!(t.byline, "Blender Studio");
        assert_eq!(t.meta, "10M views • 7 years ago");
        assert_eq!(t.badge, "7:45");
        assert_eq!(t.thumb, "https://i.ytimg.com/vi/WhWc3b3KhnY/hqdefault.jpg");
        // Every kind appears among the results.
        let kinds: std::collections::HashSet<Kind> = p.tiles().iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&Kind::Short));
        assert!(p.shelves.iter().any(|s| s.title == "Shorts"));
    }

    #[test]
    fn channel_page_with_old_tiles_and_list_links() {
        let p = parse_page(&fixture("channel")).unwrap();
        assert!(p.shelves.len() >= 3);
        assert_eq!(p.title, "Blender Studio");
        let all = p.tiles();
        let v = all.iter().find(|t| t.id == "WhWc3b3KhnY").unwrap();
        assert_eq!((v.kind, v.byline.as_str()), (Kind::Video, "Blender Studio"));
        assert_eq!(v.meta, "10M views • 7 years ago");
        assert_eq!(v.badge, "7:45");
        let pl = all.iter().find(|t| t.kind == Kind::Playlist).unwrap();
        assert!(pl.id.starts_with("PL"));
        assert_eq!(pl.browse_id().unwrap(), format!("VL{}", pl.id));
        let ch = all.iter().find(|t| t.kind == Kind::Channel).unwrap();
        assert!(ch.id.starts_with("UC"));
        // Row titles link to the whole list.
        let tut = p.shelves.iter().find(|s| s.title == "Tutorials").unwrap();
        assert_eq!(
            tut.more.as_deref(),
            Some("VLPLav47HAVZMjk8nQDr4iPHA8x4qnrcqRlZ")
        );
    }

    #[test]
    fn playlist_is_one_row() {
        let p = parse_page(&fixture("playlist")).unwrap();
        assert_eq!(p.shelves.len(), 1);
        assert!(p.shelves[0].tiles.len() >= 5);
        assert!(p.shelves[0].tiles.iter().all(|t| t.kind == Kind::Video));
        assert!(!p.title.is_empty());
    }

    #[test]
    fn signed_out_home_is_empty_not_broken() {
        let p = parse_page(&fixture("home")).unwrap();
        assert!(p.tiles().is_empty());
    }

    #[test]
    fn home_keeps_page_and_shelf_continuations_separate() {
        let shelves = r#"[{"shelfRenderer":{"title":{"simpleText":"For you"},"content":{"horizontalListRenderer":{"items":[{"tileRenderer":{"contentId":"WhWc3b3KhnY"}}],"continuations":[{"nextContinuationData":{"continuation":"ROW_MORE"}}]}}}},{"shelfRenderer":{"title":{"simpleText":"Your channels"},"content":{"horizontalListRenderer":{"items":[{"tileRenderer":{"contentId":"B4EPW7JUMTM"}}]}}}}]"#;
        for container in ["sectionListRenderer", "sectionListContinuation"] {
            let body = format!(
                r#"{{"continuationContents":{{"{container}":{{"contents":{shelves},"continuations":[{{"nextContinuationData":{{"continuation":"PAGE_MORE"}}}}]}}}}}}"#
            );
            let page = parse_page(&body).unwrap();
            assert_eq!(page.shelves.len(), 2);
            assert_eq!(page.shelves[0].continuation.as_deref(), Some("ROW_MORE"));
            assert_eq!(page.continuation.as_deref(), Some("PAGE_MORE"));
        }
    }

    #[test]
    fn home_append_commands_keep_loading_after_several_rows() {
        for item in [
            r#"{"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"PAGE_NEXT"}}}}"#,
            r#"{"continuationItemView":{"continuationCommand":{"token":"PAGE_NEXT"}}}"#,
        ] {
            let body = format!(
                r#"{{"onResponseReceivedActions":[{{"appendContinuationItemsAction":{{"continuationItems":[{{"shelfRenderer":{{"content":{{"items":[{{"tileRenderer":{{"contentId":"WhWc3b3KhnY"}}}}]}}}}}},{{"shelfRenderer":{{"content":{{"items":[{{"tileRenderer":{{"contentId":"B4EPW7JUMTM"}}}}]}}}}}},{item}]}}}}]}}"#
            );
            let page = parse_page(&body).unwrap();
            assert_eq!(page.shelves.len(), 2);
            assert_eq!(page.continuation.as_deref(), Some("PAGE_NEXT"));
        }
    }

    #[test]
    fn video_details_and_related() {
        let d = parse_details(&fixture("next")).unwrap();
        assert_eq!(d.id, "WhWc3b3KhnY");
        assert_eq!(d.title, "Spring - Blender Open Movie");
        assert_eq!(
            (d.channel.as_str(), d.channel_id.as_str()),
            ("Blender Studio", "UCz75RVbH8q2jdBJ4SnwuZZQ")
        );
        assert_eq!(d.subscribers, "577K subscribers");
        assert_eq!(d.autoplay.as_deref(), Some("B4EPW7JUMTM"));
        assert!(!d.related.is_empty());
        assert!(d.related.iter().all(|t| t.id != d.id));
        assert!(d.channel_thumb.starts_with("https://yt3.ggpht.com/"));
    }

    #[test]
    fn requests() {
        let ctx = context("en", "IN");
        assert_eq!(
            ctx,
            r#"{"client":{"clientName":"TVHTML5","clientVersion":"7.20260101.00.00","hl":"en","gl":"IN"}}"#
        );
        assert_eq!(
            browse_body(&ctx, "UCz75RVbH8q2jdBJ4SnwuZZQ").unwrap(),
            format!(r#"{{"context":{ctx},"browseId":"UCz75RVbH8q2jdBJ4SnwuZZQ"}}"#)
        );
        assert_eq!(browse_body(&ctx, "a\"b"), None);
        assert!(search_body(&ctx, "  cats \"x\" ").contains(r#""query":"cats \"x\"""#));
        assert_eq!(next_body(&ctx, "short"), None);
    }

    #[test]
    fn suggestions() {
        assert_eq!(
            parse_suggestions(
                r#"window.google.ac.h(["blen",[["blender",0,[512]],["blender tutorial",0]],{"k":1}])"#
            ),
            ["blender", "blender tutorial"]
        );
        assert_eq!(parse_suggestions(r#"["x",["a","b"]]"#), ["a", "b"]);
        assert!(parse_suggestions("<html>").is_empty());
        assert!(suggest_url_for("cat", "ar").contains("hl=ar&q=cat"));
        assert!(suggest_url_for("cat", "bad&hl").contains("hl=en&q=cat"));
    }

    #[test]
    fn home_drops_the_rows_youtube_adds_on_its_own() {
        let shelf = |title: &str, kind: Kind| Shelf {
            title: title.into(),
            tiles: vec![Tile {
                kind,
                ..Tile::default()
            }],
            continuation: None,
            more: None,
        };
        let mut p = Page {
            shelves: vec![
                shelf("Recommended", Kind::Video),
                shelf("Recommended shows", Kind::Video),
                shelf(
                    "Your home for festivities, fashion, and fireworks",
                    Kind::Video,
                ),
                shelf("Shorts", Kind::Short),
                shelf("Top channels you watch", Kind::Channel),
            ],
            ..Page::default()
        };
        tidy_home(&mut p);
        let left: Vec<&str> = p.shelves.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(left, ["Recommended", "Top channels you watch"]);
    }
}
