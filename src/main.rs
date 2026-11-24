//! CatTube: a native Linux YouTube app for TV screens and remotes.
//!
//! One full-screen window, driven with arrows, OK (Enter) and Back (Esc).
//! The screens' logic lives in `cattube_core::tube`; this file runs the
//! window, the background jobs (YouTube requests, pictures, the
//! yt-dlp updates) and the player.

mod brand;
mod canvas;
mod i18n;
mod i18n_pt;
mod icons;
mod images;
mod jobs;
mod keys;
mod motion;
mod pointer;
mod prefs;
mod target;
mod theme;
mod ui;
mod widgets;
mod yt;

use canvas::Canvas;
use cattube_core::input::{Action, InputMapper};
use cattube_core::player::{self, Codec, PlayOptions, RESOLUTIONS};
use cattube_core::scan::Scan;
use cattube_core::tube::{Ctx, Effect, Screen, Section, SettingRow, Tube};
use cattube_core::youtube::{self as ytc, Details, Page, Tile};
use eframe::egui;
use jobs::Jobs;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use target::Target;

enum Msg {
    Page(u64, String, Result<Page, String>),
    More(u64, String, String, Result<Page, String>),
    Search(u64, String, Result<Page, String>),
    SearchMore(u64, Result<Page, String>),
    Suggestions(u64, String, Vec<String>),
    Details(String, Result<Details, String>),
    /// A background job with nothing to show.
    Quiet,
    Images(Vec<(String, Option<egui::ColorImage>)>),
    Tools(Result<String, String>),
    /// A player ended: its number (see `App::player_seq`).
    PlaybackDone(u64),
    /// That player's current video was hidden behind the browser.
    PlayerHidden(u64, Tile),
    PlayerChanged(u64, Tile),
    /// Local history and resume checkpoints loaded off the UI thread.
    History(Page, Vec<(String, f64)>),
    Positions(Vec<(String, f64)>),
}

const SUGGEST_DEBOUNCE: Duration = Duration::from_millis(200);
/// Shown when a background job crashed instead of answering.
const JOB_FAILED: &str = "Something went wrong, try again";
/// The longest the app shows its own "opening" screen for a video: the
/// player's window normally covers it well before.
const OPENING_MAX: Duration = Duration::from_secs(10);

enum PendingSuggest {
    Ready(String),
    Wait(Duration),
    None,
}

struct App {
    tube: Tube,
    prefs: prefs::Prefs,
    config: PathBuf,
    cache: PathBuf,
    data: PathBuf,
    jobs: Jobs<Msg>,
    mapper: InputMapper,
    pointer: pointer::Pointer,
    /// Where the pointer was last seen.
    pointer_at: Option<egui::Pos2>,
    /// Fading out before handing the screen back to CatosTV.
    closing: Option<Instant>,
    /// A video is opening: since when. The player (mpv) takes a moment to
    /// start and show its window, its own spinner on black; until then the
    /// app shows the same, so OK answers at once. Ends when the player's
    /// window takes the focus, the player goes, or after [`OPENING_MAX`].
    opening: Option<Instant>,
    hits: Vec<(egui::Rect, Target)>,
    images: images::Images,
    fetching: HashSet<String>,
    low_power: bool,
    toast: Option<(String, Instant)>,
    player: Option<yt::PlayerHandle>,
    /// Counts the players started; only the newest one's end counts.
    player_seq: u64,
    /// The current video, retained while browsing and returning to playback.
    now_playing: Option<Tile>,
    /// Video queued by `--play` while waiting for first-run tool downloads.
    pending_play: Option<Tile>,
    positions: Vec<(String, f64)>,
    tools: Scan<String>,
    yt_dlp_ready: bool,
    /// Started with `--play`: close when the video ends.
    quit_after_play: bool,
    was_focused: bool,
    search_generation: u64,
    search_query: String,
    suggest_generation: u64,
    pending_suggest: Option<(String, Instant)>,
    page_generation: u64,
    page_requests: HashSet<(String, String)>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, start: Start) -> Self {
        theme::setup(&cc.egui_ctx);
        let config = cattube_core::env::config_dir();
        let cache = cattube_core::env::cache_dir();
        let data = player::data_home();
        let prefs = prefs::load(&config);
        let low_power = cattube_core::env::low_power();
        theme::set_accent(&prefs.accent);
        i18n::set_language(prefs.language);
        let yt_dlp_ready = player::yt_dlp_path(&data).is_file();
        let ctx = Ctx {
            sponsorblock: prefs.sponsorblock,
            playing: false,
        };
        let (mut tube, mut effects) = Tube::open();
        tube.recent = std::fs::read_to_string(data.join("searches.txt"))
            .map(|t| {
                t.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .take(cattube_core::tube::RECENT_SEARCHES)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut app = App {
            tube: tube.clone(),
            prefs,
            config,
            cache: cache.clone(),
            data: data.clone(),
            jobs: Jobs::new(&cc.egui_ctx, if low_power { 4 } else { 8 }),
            mapper: InputMapper::new(),
            pointer: pointer::Pointer::new(),
            pointer_at: None,
            closing: None,
            opening: None,
            hits: Vec::new(),
            images: images::Images::new(low_power),
            fetching: HashSet::new(),
            low_power,
            toast: None,
            player: None,
            player_seq: 0,
            now_playing: None,
            pending_play: None,
            positions: Vec::new(),
            tools: Scan::Busy,
            yt_dlp_ready,
            quit_after_play: false,
            was_focused: true,
            search_generation: 0,
            search_query: String::new(),
            suggest_generation: 0,
            pending_suggest: None,
            page_generation: 0,
            page_requests: HashSet::new(),
        };
        let pos_data = data.clone();
        app.jobs
            .spawn(move || Msg::Positions(player::load_positions(&pos_data)));
        app.jobs.spawn_or(
            move || Msg::Tools(yt::ensure_tools(&data)),
            || Msg::Tools(Err(JOB_FAILED.into())),
        );
        let (cache_max_bytes, cache_max_files) = if low_power {
            (64 * 1024 * 1024, 1500)
        } else {
            (256 * 1024 * 1024, 4000)
        };
        app.jobs.spawn(move || {
            yt::prune_image_cache(&cache, cache_max_bytes, cache_max_files);
            Msg::Quiet
        });
        match start {
            Start::Play(id, title) => {
                app.quit_after_play = true;
                effects.clear();
                let tile = Tile {
                    id: id.clone(),
                    title,
                    ..Tile::default()
                };
                if app.yt_dlp_ready {
                    app.play(&cc.egui_ctx, tile, false, Vec::new());
                } else {
                    app.pending_play = Some(tile);
                }
            }
            Start::Section(s) => {
                tube.header = true;
                tube.header_index = Section::ALL.iter().position(|x| *x == s).unwrap_or(0);
                effects.extend(tube.handle(Action::Ok, ctx, &|_| false));
            }
            Start::Home => {}
        }
        app.tube = tube;
        for e in effects {
            app.effect(&cc.egui_ctx, e);
        }
        app
    }

    fn ctx(&self) -> Ctx {
        Ctx {
            sponsorblock: self.prefs.sponsorblock,
            playing: self.now_playing.is_some() && self.player.is_some(),
        }
    }

    fn yt_ctx(&self) -> String {
        ytc::context(self.prefs.language.key(), &self.prefs.region)
    }

    fn toast(&mut self, ctx: &egui::Context, text: &str) {
        self.toast = Some((text.to_string(), Instant::now() + Duration::from_secs(3)));
        ctx.request_repaint();
    }

    fn save_prefs(&mut self, ctx: &egui::Context) {
        if let Err(e) = prefs::save(&self.config, &self.prefs) {
            self.toast(ctx, &format!("Could not save the settings ({e})"));
        }
    }

    fn resume_at(&self, id: &str) -> Option<f64> {
        self.positions
            .iter()
            .find(|(x, _)| x == id)
            .map(|(_, s)| *s)
    }

    fn effect(&mut self, ctx: &egui::Context, e: Effect) {
        let yctx = self.yt_ctx();
        let generation = self.page_generation;
        match e {
            // History is what was watched here (read off the UI thread).
            Effect::Load(id) if id == ytc::feeds::HISTORY => {
                let data = self.data.clone();
                let failed = move || Msg::Page(generation, id, Err(JOB_FAILED.into()));
                self.jobs.spawn_or(
                    move || {
                        let (positions, checkpoints) =
                            player::load_positions_and_checkpoints(&data);
                        let mut list = std::fs::read_to_string(data.join("watched.conf"))
                            .map(|t| cattube_core::watched::parse(&t))
                            .unwrap_or_default();
                        for tile in &mut list {
                            if let Some(point) = checkpoints.get(&tile.id) {
                                tile.progress = if point.finished {
                                    Some(1.0)
                                } else {
                                    point
                                        .duration
                                        .map(|d| (point.position / d).clamp(0.0, 1.0) as f32)
                                };
                                tile.meta = if point.finished {
                                    "Watched".into()
                                } else {
                                    format!("Resume at {}", player::position_clock(point.position))
                                };
                                if tile.badge.is_empty()
                                    && let Some(d) = point.duration
                                {
                                    tile.badge = player::position_clock(d);
                                }
                            }
                        }
                        Msg::History(cattube_core::watched::page(&list), positions)
                    },
                    failed,
                );
            }
            Effect::Load(id) => {
                let key = (id.clone(), String::new());
                if !self.page_requests.insert(key.clone()) {
                    return;
                }
                let failed_id = id.clone();
                if !self.jobs.try_spawn_or(
                    move || {
                        let mut r = yt::page(&yctx, &id);
                        if id == ytc::feeds::HOME
                            && let Ok(p) = r.as_mut()
                        {
                            ytc::tidy_home(p);
                        }
                        Msg::Page(generation, id, r)
                    },
                    move || Msg::Page(generation, failed_id, Err(JOB_FAILED.into())),
                ) {
                    self.page_requests.remove(&key);
                }
            }
            Effect::LoadMore { browse_id, token } => {
                let key = (browse_id.clone(), token.clone());
                if !self.page_requests.insert(key.clone()) {
                    return;
                }
                let (failed_id, failed_token) = key.clone();
                if !self.jobs.try_spawn_or(
                    move || {
                        let mut r = yt::more(&yctx, &token);
                        if browse_id == ytc::feeds::HOME
                            && let Ok(p) = r.as_mut()
                        {
                            ytc::tidy_home(p);
                        }
                        Msg::More(generation, browse_id, token, r)
                    },
                    move || Msg::More(generation, failed_id, failed_token, Err(JOB_FAILED.into())),
                ) {
                    self.page_requests.remove(&key);
                }
            }
            Effect::Search(q) => {
                // Kept on this TV for next time (Search shows them first).
                self.tube.remember_search(&q);
                let list: String = self
                    .tube
                    .recent
                    .iter()
                    .map(|r| r.chars().filter(|c| !c.is_control()).collect::<String>() + "\n")
                    .collect();
                let _ = std::fs::create_dir_all(&self.data);
                let searches_path = self.data.join("searches.txt");
                let tmp = cattube_core::util::unique_tmp_path(&searches_path, "tmp");
                if std::fs::write(&tmp, list).is_err()
                    || std::fs::rename(&tmp, &searches_path).is_err()
                {
                    let _ = std::fs::remove_file(&tmp);
                }
                self.pending_suggest = None;
                self.suggest_generation += 1;
                self.search_generation += 1;
                self.search_query = q.clone();
                let generation = self.search_generation;
                let failed_q = q.clone();
                self.jobs.spawn_or(
                    move || {
                        let result = yt::search(&yctx, &q);
                        Msg::Search(generation, q, result)
                    },
                    move || Msg::Search(generation, failed_q, Err(JOB_FAILED.into())),
                )
            }
            Effect::SearchMore(t) => {
                let generation = self.search_generation;
                self.jobs.spawn_or(
                    move || Msg::SearchMore(generation, yt::search_more(&yctx, &t)),
                    move || Msg::SearchMore(generation, Err(JOB_FAILED.into())),
                )
            }
            Effect::Suggest(q) => {
                self.suggest_generation += 1;
                self.pending_suggest = Some((q, Instant::now() + SUGGEST_DEBOUNCE));
                ctx.request_repaint_after(SUGGEST_DEBOUNCE);
            }
            Effect::Details(id) => {
                let failed_id = id.clone();
                self.jobs.spawn_or(
                    move || {
                        let r = yt::details(&yctx, &id);
                        Msg::Details(id, r)
                    },
                    move || Msg::Details(failed_id, Err(JOB_FAILED.into())),
                )
            }
            Effect::Play { tile, from_start } => {
                // The rest of the row it came from plays after it.
                let queue = self.queue_after(&tile);
                self.play(ctx, *tile, from_start, queue);
            }
            Effect::Setting(row, d) => self.setting(ctx, row, d),
            Effect::Close => {
                // On CatosTV the window fades to dark first; the TV's own
                // screen then fades in from dark, so nothing flashes between.
                if std::env::var_os("CATOSTV_APP_ID").is_some() && self.closing.is_none() {
                    self.closing = Some(Instant::now());
                    ctx.request_repaint();
                    return;
                }
                if hand_back_screen() {
                    // The window is behind the TV's own screen now, where it
                    // draws nothing and so would never act on a close
                    // request: end here (the player goes with it).
                    if let Some(p) = self.player.take() {
                        if !p.exited() {
                            yt::checkpoint_player(&self.data);
                        }
                        p.stop_and_reap();
                    }
                    std::process::exit(0);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close)
            }
            Effect::Toast(t) => self.toast(ctx, &t),
            Effect::ShowPlayer => {
                if yt::raise_window(yt::PLAYER_WINDOW) {
                    self.tube.now_playing = false;
                } else {
                    self.toast(ctx, "Could not go back to the video");
                }
            }
        }
    }

    /// The tiles after `tile` in the row it was picked from (a playlist
    /// plays on in order).
    fn queue_after(&self, tile: &Tile) -> Vec<Tile> {
        let page = match self.tube.screen() {
            Some(Screen::Page { page, .. }) => page.items().first().cloned(),
            Some(Screen::Search { results, .. }) => results.items().first().cloned(),
            _ => self.tube.current().and_then(|p| p.items().first().cloned()),
        };
        let Some(p) = page else { return Vec::new() };
        // Only a playlist plays on by itself; other rows leave it to
        // YouTube's own "up next".
        let playlist = matches!(self.tube.screen(), Some(Screen::Page { browse_id, .. }) if browse_id.starts_with("VL"));
        if !playlist {
            return Vec::new();
        }
        p.shelves
            .iter()
            .find(|s| s.tiles.iter().any(|t| t.id == tile.id))
            .map(|s| {
                s.tiles
                    .iter()
                    .skip_while(|t| t.id != tile.id)
                    .skip(1)
                    .filter(|t| t.playable())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn setting(&mut self, ctx: &egui::Context, row: SettingRow, d: i32) {
        match row {
            SettingRow::Quality => {
                self.prefs.max_height = prefs::cycle(&RESOLUTIONS, self.prefs.max_height, -d)
            }
            SettingRow::Codec => self.prefs.codec = prefs::cycle(&Codec::ALL, self.prefs.codec, d),
            SettingRow::Hdr => self.prefs.hdr = !self.prefs.hdr,
            SettingRow::Subtitles => self.prefs.subs_on = !self.prefs.subs_on,
            SettingRow::Buffer => {
                self.prefs.buffer = prefs::cycle(&player::BUFFERS, self.prefs.buffer, d)
            }
            SettingRow::SubtitleLanguage => {
                let languages = ["en", "ar", "de", "fr", "es", "hi"];
                let cur = languages
                    .iter()
                    .copied()
                    .find(|v| *v == self.prefs.sub_lang)
                    .unwrap_or("en");
                self.prefs.sub_lang = prefs::cycle(&languages, cur, d).to_string();
            }
            SettingRow::Language | SettingRow::Region => {
                if row == SettingRow::Language {
                    self.prefs.language =
                        prefs::cycle(&i18n::Language::ALL, self.prefs.language, d);
                    i18n::set_language(self.prefs.language);
                } else {
                    let cur = player::REGIONS
                        .iter()
                        .copied()
                        .find(|r| *r == self.prefs.region)
                        .unwrap_or("US");
                    self.prefs.region = prefs::cycle(&player::REGIONS, cur, d).to_string();
                }
                self.reset_requests();
                // The lists are for the old country: load them again, but
                // stay on this row.
                let at = match self.tube.screen() {
                    Some(Screen::Settings { index, .. }) => *index,
                    _ => 0,
                };
                let effects = self.tube.reset();
                self.tube.stack.push(Screen::Settings {
                    index: at,
                    groups: false,
                });
                for e in effects {
                    self.effect(ctx, e);
                }
            }
            SettingRow::Autoplay => {
                self.prefs.autoplay = prefs::cycle(&prefs::Autoplay::ALL, self.prefs.autoplay, d)
            }
            SettingRow::SponsorBlock => self.prefs.sponsorblock = !self.prefs.sponsorblock,
            SettingRow::Category(i) => {
                let key = cattube_core::community::CATEGORIES[i].0.to_string();
                if self.prefs.skips(&key) {
                    self.prefs.sb_categories.retain(|c| *c != key);
                } else {
                    self.prefs.sb_categories.push(key);
                }
            }
            SettingRow::PlayerStats => self.prefs.player_stats = !self.prefs.player_stats,
            SettingRow::UpdateYtDlp => {
                self.tools = Scan::Busy;
                let data = self.data.clone();
                self.jobs.spawn_or(
                    move || Msg::Tools(yt::update_tools(&data)),
                    || Msg::Tools(Err(JOB_FAILED.into())),
                );
                return;
            }
        }
        self.save_prefs(ctx);
    }

    fn play(&mut self, ctx: &egui::Context, tile: Tile, from_start: bool, queue: Vec<Tile>) {
        if !from_start
            && self.player.is_some()
            && self
                .now_playing
                .as_ref()
                .is_some_and(|active| active.id == tile.id)
        {
            self.effect(ctx, Effect::ShowPlayer);
            return;
        }
        if let Some(old) = self.player.take() {
            if !old.exited() {
                yt::checkpoint_player(&self.data);
            }
            old.stop_and_reap();
        }
        let (r, g, b) = theme::accent_rgb();
        // Check only this video's single checkpoint file on demand instead of
        // scanning all 500 files on the UI thread.
        let start = if from_start {
            0.0
        } else if let Some(point) = player::checkpoint(&self.data, &tile.id) {
            if point.finished { 0.0 } else { point.position }
        } else {
            self.resume_at(&tile.id).unwrap_or(0.0)
        };
        let o = PlayOptions {
            low_power: self.low_power,
            video_id: tile.id.clone(),
            title: tile.title.clone(),
            start,
            max_height: self.prefs.max_height,
            codec: self.prefs.codec,
            hdr: self.prefs.hdr,
            sub_langs: self.prefs.sub_lang.clone(),
            subs_on: self.prefs.subs_on,
            accent: format!("{r:02x}{g:02x}{b:02x}"),
            buffer_mib: self.prefs.buffer,
            hide_on_back: on_catostv() && !self.quit_after_play,
            stats: self.prefs.player_stats,
        };
        if !self.yt_dlp_ready && !player::yt_dlp_path(&self.data).is_file() {
            return self.toast(ctx, "Getting the player ready, try again in a moment");
        }
        self.yt_dlp_ready = true;
        match yt::start_player(&o, &self.data) {
            Ok(child) => {
                self.opening = Some(Instant::now());
                ctx.request_repaint();
                let watcher_player = child.clone();
                self.player = Some(child);
                self.player_seq += 1;
                let seq = self.player_seq;
                self.now_playing = None;
                self.tube.now_playing = false;
                let (tx, rctx) = self.jobs.sender();
                let (htx, hctx) = self.jobs.sender();
                let (vtx, vctx) = self.jobs.sender();
                let s = yt::Session {
                    low_power: self.low_power,
                    ctx: self.yt_ctx(),
                    cache: self.cache.clone(),
                    autoplay: self.prefs.autoplay,
                    sponsorblock: self
                        .prefs
                        .sponsorblock
                        .then(|| self.prefs.sb_categories.clone()),
                    codec: self.prefs.codec,
                    hdr: self.prefs.hdr,
                    queue,
                    on_video: Some(Box::new(move |tile| {
                        let _ = vtx.send(Msg::PlayerChanged(seq, tile));
                        vctx.request_repaint();
                    })),
                    on_hide: Some(Box::new(move |title| {
                        let _ = htx.send(Msg::PlayerHidden(seq, title));
                        hctx.request_repaint();
                    })),
                };
                let (data, quit) = (self.data.clone(), self.quit_after_play);
                yt::spawn_watcher(tile, watcher_player, s, move |ended| {
                    // Where it stopped is saved right here: CatTube's window
                    // may be hidden (behind the TV's own screen), and a hidden
                    // window handles nothing until it shows again.
                    save_positions(&data, &ended.positions);
                    // Opened to play this one video: done, close now.
                    if quit {
                        let _ = hand_back_screen();
                        std::process::exit(0);
                    }
                    let _ = tx.send(Msg::PlaybackDone(seq));
                    rctx.request_repaint();
                });
            }
            Err(e) => self.toast(ctx, &e),
        }
    }

    fn message(&mut self, ctx: &egui::Context, msg: Msg) {
        match msg {
            Msg::Page(generation, id, r) => {
                if generation == self.page_generation {
                    self.page_requests.remove(&(id.clone(), String::new()));
                    self.tube.replace_page(&id, r);
                }
            }
            Msg::More(generation, id, tok, r) => {
                if generation == self.page_generation {
                    self.page_requests.remove(&(id.clone(), tok.clone()));
                    match r {
                        Ok(p) => self.tube.on_more(&id, &tok, p),
                        Err(e) => self.toast(ctx, &e),
                    }
                }
            }
            Msg::Search(generation, query, r) => {
                if search_matches(&self.tube, self.search_generation, generation, &query) {
                    self.tube.on_search(r);
                }
            }
            Msg::SearchMore(generation, r) => {
                if search_matches(
                    &self.tube,
                    self.search_generation,
                    generation,
                    &self.search_query,
                ) {
                    self.tube.on_search_more(r);
                }
            }
            Msg::Suggestions(generation, q, s) => {
                if generation == self.suggest_generation {
                    self.tube.on_suggestions(&q, s);
                }
            }
            Msg::Details(id, r) => self.tube.on_details(&id, r),
            Msg::Quiet => {}
            Msg::Images(batch) => {
                let frame = ctx.cumulative_frame_nr();
                for (url, image) in batch {
                    self.fetching.remove(&url);
                    self.images.complete(url, image, frame);
                }
            }
            Msg::Tools(r) => {
                let ok = r.is_ok();
                self.yt_dlp_ready = ok || player::yt_dlp_path(&self.data).is_file();
                self.tools = match r {
                    Ok(v) => Scan::Done(vec![v]),
                    Err(e) => {
                        self.toast(ctx, &e);
                        Scan::Failed(e)
                    }
                };
                if let Some(tile) = self.pending_play.take() {
                    if ok && self.yt_dlp_ready {
                        self.play(ctx, tile, false, Vec::new());
                    } else if self.quit_after_play {
                        self.effect(ctx, Effect::Close);
                    }
                }
            }
            Msg::History(page, positions) => {
                self.positions = positions;
                self.tube.replace_page(ytc::feeds::HISTORY, Ok(page));
            }
            Msg::Positions(positions) => {
                self.positions = positions;
            }
            Msg::PlayerChanged(seq, tile) => {
                if seq == self.player_seq && self.player.is_some() {
                    self.now_playing = Some(tile);
                    if self.tube.section == Section::History && keys::window_focused(ctx) {
                        self.effect(ctx, Effect::Load(ytc::feeds::HISTORY.to_string()));
                    }
                }
            }
            Msg::PlayerHidden(seq, tile) => {
                if seq == self.player_seq && self.player.is_some() {
                    self.opening = None;
                    let data = self.data.clone();
                    self.jobs
                        .spawn(move || Msg::Positions(player::load_positions(&data)));
                    self.now_playing = Some(tile);
                    // Focus on the bar: OK goes straight back.
                    self.tube.now_playing = true;
                    self.tube.header = false;
                    if self.tube.section == Section::History {
                        self.effect(ctx, Effect::Load(ytc::feeds::HISTORY.to_string()));
                    }
                }
            }
            Msg::PlaybackDone(seq) => {
                // An older player (replaced by a newer one): only where it
                // stopped counts. Its end must not bring CatTube over the
                // video playing now, or wait for that one to end.
                let current = seq == self.player_seq;
                if current {
                    self.opening = None;
                }
                if current && let Some(p) = self.player.take() {
                    p.reap_or_kill(Duration::from_millis(200));
                    self.now_playing = None;
                    self.tube.now_playing = false;
                }
                // Saved already when it ended (save_positions): reload off the UI thread.
                let data = self.data.clone();
                self.jobs
                    .spawn(move || Msg::Positions(player::load_positions(&data)));
                if !current {
                    return;
                }
                if self.quit_after_play {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                // Fresh progress must not steal focus from another window.
                if matches!(self.tube.section, Section::History | Section::Home)
                    && let Some(id) = self.tube.section.browse_id()
                {
                    self.effect(ctx, Effect::Load(id.to_string()));
                }
            }
        }
    }

    fn reset_requests(&mut self) {
        self.page_generation += 1;
        self.search_generation += 1;
        self.suggest_generation += 1;
        self.pending_suggest = None;
        self.page_requests.clear();
    }

    fn fetch_pictures(&mut self) {
        let max_in_flight = if self.low_power { 6 } else { 12 };
        let batch_size = if self.low_power { 4 } else { 8 };
        let max_side = if self.low_power { 320 } else { 640 };
        let mut batch = Vec::new();
        for url in self.images.take_wanted() {
            if self.fetching.len() >= max_in_flight || batch.len() >= batch_size {
                break;
            }
            if !self.fetching.insert(url.clone()) {
                continue;
            }
            batch.push(url);
        }
        if batch.is_empty() {
            return;
        }
        let cache = self.cache.clone();
        let low_power = self.low_power;
        let requested = batch.clone();
        let failed = batch.clone();
        if !self.jobs.try_spawn_or(
            move || {
                let fetched = yt::fetch_images_batch(&cache, &requested, low_power);
                let decoded = fetched
                    .into_iter()
                    .map(|(url, path)| {
                        let image = path.and_then(|p| {
                            let d = images::decode(&p, max_side);
                            if d.is_none() {
                                let _ = std::fs::remove_file(&p);
                            }
                            d
                        });
                        (url, image)
                    })
                    .collect();
                Msg::Images(decoded)
            },
            move || Msg::Images(failed.into_iter().map(|url| (url, None)).collect()),
        ) {
            for url in batch {
                self.fetching.remove(&url);
            }
        }
    }
}

fn search_matches(tube: &Tube, current: u64, received: u64, query: &str) -> bool {
    current == received
        && matches!(tube.screen(), Some(Screen::Search { osk, .. }) if osk.text().trim() == query.trim())
}

fn take_due_suggestion(
    tube: &Tube,
    pending: &mut Option<(String, Instant)>,
    now: Instant,
) -> PendingSuggest {
    let Some((query, due)) = pending.as_ref() else {
        return PendingSuggest::None;
    };
    let still_matching = matches!(
        tube.screen(),
        Some(Screen::Search { osk, .. }) if osk.text().trim() == query.trim() && osk.text().trim().len() >= 2
    );
    if !still_matching {
        *pending = None;
        return PendingSuggest::None;
    }
    if now >= *due {
        let (query, _) = pending.take().expect("checked above");
        PendingSuggest::Ready(query)
    } else {
        PendingSuggest::Wait(*due - now)
    }
}

fn hit_test(hits: &[(egui::Rect, Target)], pos: egui::Pos2) -> Option<Target> {
    hits.iter()
        .rev()
        .find(|(r, _)| r.contains(pos))
        .map(|(_, t)| *t)
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(Screen::Search { osk, .. }) = self.tube.stack.last_mut() {
            osk.configure_arabic(self.prefs.language == i18n::Language::Arabic);
        }
        for msg in self.jobs.drain() {
            self.message(ctx, msg);
        }
        match take_due_suggestion(&self.tube, &mut self.pending_suggest, Instant::now()) {
            PendingSuggest::Ready(q) => {
                self.suggest_generation += 1;
                let generation = self.suggest_generation;
                let hl = self.prefs.language.key().to_string();
                self.jobs.spawn(move || {
                    let s = yt::suggestions(&q, &hl);
                    Msg::Suggestions(generation, q, s)
                });
            }
            PendingSuggest::Wait(wait) => ctx.request_repaint_after(wait),
            PendingSuggest::None => {}
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let focused = keys::window_focused(&ctx);
        if !focused && self.was_focused {
            self.mapper.reset();
        }
        self.was_focused = focused;
        // The player's window is up (it has the focus now), or it never
        // came: the list shows again.
        if self
            .opening
            .is_some_and(|since| !focused || since.elapsed() > OPENING_MAX)
        {
            self.opening = None;
        }
        let now = keys::now(&ctx);
        let events = keys::collect(&ctx);
        let mut actions = self.mapper.map(&events, now, self.tube.text_mode());
        let mut p = self.pointer.collect(&ctx);
        if self.closing.is_some() || self.opening.is_some() {
            actions.clear();
            p = Default::default();
        }
        let c = self.ctx();
        let resumable = |id: &str| self.positions.iter().any(|(x, _)| x == id);
        let mut effects = Vec::new();
        for action in actions {
            effects.extend(self.tube.handle(action, c, &resumable));
        }
        // The pointer focuses what it is over; a click is OK on it.
        let point = |tube: &mut Tube, t: Option<Target>| match t {
            Some(Target::Tube(r, col)) => {
                tube.now_playing = false;
                ui::tube::focus(tube, r, col)
            }
            Some(t @ (Target::Key(..) | Target::Suggestion(_))) => {
                tube.now_playing = false;
                ui::tube::focus_search(tube, t)
            }
            Some(Target::NowPlaying) => {
                tube.header = false;
                tube.now_playing = true;
                true
            }
            _ => false,
        };
        // Only a pointer that really moves picks things: the window opening
        // under a resting pointer reports a move too.
        if let Some(pos) = p.moved_to {
            let moved = self
                .pointer_at
                .map_or(ctx.input(|i| i.time) > 1.5, |at| (at - pos).length() > 2.0);
            self.pointer_at = Some(pos);
            if moved {
                point(&mut self.tube, hit_test(&self.hits, pos));
            }
        }
        if let Some(pos) = p.click
            && point(&mut self.tube, hit_test(&self.hits, pos))
        {
            effects.extend(self.tube.handle(Action::Ok, c, &resumable));
        }
        if p.back {
            effects.extend(self.tube.handle(Action::Back, c, &resumable));
        }
        let (n, action) = if p.wheel > 0 {
            (p.wheel, Action::Up)
        } else {
            (-p.wheel, Action::Down)
        };
        for _ in 0..n.min(5) {
            effects.extend(self.tube.handle(action, c, &resumable));
        }
        for e in effects {
            self.effect(&ctx, e);
        }
        if let Some(deadline) = self.mapper.long_press_deadline() {
            ctx.request_repaint_after(Duration::from_secs_f64((deadline - now).max(0.0)));
        }

        let mut canvas: Canvas<Target> = Canvas::new(ui, 0);
        canvas.reduce_motion = self.low_power;
        canvas.background_at(None, false, now);
        let view = ui::tube::View {
            tube: &self.tube,
            prefs: &self.prefs,
            pics: &self.images,
            positions: &self.positions,
            tools: &self.tools,
            yt_dlp_ready: self.yt_dlp_ready,
            now,
            now_playing: self.now_playing.as_ref().filter(|_| self.player.is_some()),
        };
        ui::tube::draw(&canvas, &view);
        let at = Instant::now();
        if matches!(&self.toast, Some((_, until)) if at >= *until) {
            self.toast = None;
        }
        if let Some((text, until)) = &self.toast {
            let left = until.saturating_duration_since(at).as_secs_f32();
            const FADE: f32 = 0.3;
            let flat_toast = self.low_power || theme::FLAT;
            let opacity = if flat_toast {
                1.0
            } else {
                (left / FADE).min(1.0)
            };
            widgets::toast(&canvas, text, opacity);
            let next = if flat_toast {
                left
            } else if left > FADE {
                left - FADE
            } else {
                left.min(0.016)
            };
            ctx.request_repaint_after(Duration::from_secs_f32(next));
        }
        if let Some(since) = self.opening {
            // The player's loading screen, as `cattube-osd.lua` draws it
            // (1080p arc of radius 31, width 6, in the 720p design space),
            // timed from the same start, so the handover does not show.
            ui.painter()
                .rect_filled(ctx.content_rect(), 0.0, egui::Color32::BLACK);
            widgets::spinner(
                &canvas,
                640.0,
                360.0,
                31.0 * 2.0 / 3.0,
                since.elapsed().as_secs_f64(),
            );
        }
        self.hits = canvas.take_hits();
        self.images.trim();
        self.fetch_pictures();
        if let Some(since) = self.closing {
            const FADE: f32 = 0.22;
            let t = (since.elapsed().as_secs_f32() / FADE).min(1.0);
            let ease = t * t * (3.0 - 2.0 * t);
            ui.painter().rect_filled(
                ctx.content_rect(),
                0.0,
                egui::Color32::from_black_alpha((ease * 255.0) as u8),
            );
            if t >= 1.0 && since.elapsed().as_secs_f32() > FADE + 0.05 {
                // A dark frame is on screen: hand over now.
                self.effect(&ctx, Effect::Close);
            } else {
                ctx.request_repaint();
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(p) = self.player.take() {
            if !p.exited() {
                yt::checkpoint_player(&self.data);
            }
            p.stop_and_reap();
        }
    }
}

/// Where CatTube starts.
enum Start {
    Home,
    /// `--section history`: open on a tab.
    Section(Section),
    /// `--play VIDEO [--title TITLE]`: play one video, close when it ends.
    Play(String, String),
}

/// Closing while a TV home screen started CatTube (CatosTV sets
/// `CATOSTV_APP_ID` for the apps it opens): ask it to come back first, so
/// what is behind CatTube's window never shows in between. True when it
/// did.
/// Opened by CatosTV, which can put windows in front (`catostv-ctl`).
fn on_catostv() -> bool {
    std::env::var_os("CATOSTV_APP_ID").is_some()
}

fn hand_back_screen() -> bool {
    if std::env::var_os("CATOSTV_APP_ID").is_none() {
        return false;
    }
    let asked = std::process::Command::new("catostv-ctl")
        .arg("escape")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if asked {
        // The switch takes a moment; this window is already out of sight,
        // so waiting longer before it goes costs nothing.
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    asked
}

/// Add where videos stopped to `positions.conf` (read, merge, write
/// atomically: another CatTube, such as `--play`, may write it too).
fn save_positions(data: &std::path::Path, ended: &[(String, f64, Option<f64>)]) {
    let _ = player::save_positions_merged(data, ended);
}

/// `--feed home`: Home's videos, one per line, for a TV home screen.
/// Exit 2 for a list that needs a YouTube account (CatTubeLite has
/// none), 1 when YouTube did not answer.
fn feed(which: &str) -> i32 {
    if !matches!(which, "home" | "recommended") {
        return 2;
    }
    let p = prefs::load(&cattube_core::env::config_dir());
    let ctx = ytc::context(p.language.key(), &p.region);
    let page = yt::page(&ctx, ytc::feeds::HOME).map(|mut p| {
        ytc::tidy_home(&mut p);
        p
    });
    let page = match page {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cattube: {e}");
            return 1;
        }
    };
    let clean = |s: &str| {
        s.chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>()
    };
    for t in page.tiles().into_iter().filter(|t| t.playable()).take(24) {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            t.id,
            clean(&t.title),
            clean(&t.byline),
            clean(&t.meta),
            clean(&t.badge),
            clean(&t.thumb)
        );
    }
    0
}

#[cfg_attr(not(feature = "wgpu"), allow(unused_variables))]
fn renderer_for(choice: Option<&str>, low_power: bool) -> (eframe::Renderer, bool) {
    match choice {
        Some("glow") => (eframe::Renderer::Glow, true),
        #[cfg(feature = "wgpu")]
        Some("wgpu") => (eframe::Renderer::Wgpu, true),
        #[cfg(feature = "wgpu")]
        _ if !low_power => (eframe::Renderer::Wgpu, false),
        _ => (eframe::Renderer::Glow, false),
    }
}

fn renderer() -> (eframe::Renderer, bool) {
    renderer_for(
        std::env::var("CATTUBE_RENDERER").ok().as_deref(),
        cattube_core::env::low_power(),
    )
}

const HELP: &str = "cattube {VERSION}: a native Linux YouTube app for TV screens
and remotes. https://github.com/victorgmbraga/CatTubeLite

Usage: cattube [--windowed] [--section NAME | --play VIDEO_ID]
       cattube --feed home

  --section NAME   open on home, history or search
  --play VIDEO_ID  play one video, then close (--title TITLE names it)
  --stop           close the player
  --feed home      print Home's videos as
                   tab-separated lines: id, title, channel, facts, length,
                   picture; no window (for TV home screens)

Keys: arrows move, Enter = OK (hold for the video menu), Esc/Backspace = Back.
Settings live in ~/.config/cattube/; yt-dlp, Deno and
watch positions in ~/.local/share/cattube/.";

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("cattube {}", cattube_core::env::version());
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "{}",
            HELP.replace("{VERSION}", cattube_core::env::version())
        );
        return Ok(());
    }
    // The phone remote is gone from CatTubeLite: its flags must not open
    // the window (a TV shell may still start `--remote` as a service).
    if let Some(flag) = args
        .iter()
        .find(|a| matches!(a.as_str(), "--remote" | "--tv-code" | "--pair"))
    {
        eprintln!("cattube: {flag}: CatTubeLite has no phone remote");
        std::process::exit(2);
    }
    if args.iter().any(|a| a == "--stop") {
        // Close the player.
        let _ = cattube_core::mpv::ipc_request(
            &cattube_core::player::player_socket(),
            &[cattube_core::mpv::Arg::Str("quit")],
        );
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--feed") {
        std::process::exit(feed(args.get(i + 1).map(String::as_str).unwrap_or("home")));
    }
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let start = if let Some(id) = value("--play").filter(|v| {
        v.len() == 11
            && v.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    }) {
        let title = value("--title").unwrap_or_default();
        Start::Play(id, title)
    } else if let Some(s) = value("--section") {
        Section::ALL
            .into_iter()
            .find(|x| x.label().eq_ignore_ascii_case(&s))
            .map_or(Start::Home, Start::Section)
    } else {
        Start::Home
    };
    let windowed = args.iter().any(|a| a == "--windowed")
        || std::env::var("CATTUBE_WINDOWED").is_ok_and(|v| v == "1");
    let (renderer, explicit) = renderer();
    let viewport = egui::ViewportBuilder::default()
        .with_title(brand::NAME)
        .with_app_id(yt::APP_WINDOW)
        .with_decorations(windowed)
        .with_fullscreen(!windowed)
        .with_inner_size([1280.0, 720.0]);
    let options = eframe::NativeOptions {
        viewport,
        renderer,
        ..Default::default()
    };
    let start = std::cell::Cell::new(Some(start));
    let result = eframe::run_native(
        "CatTube",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, start.take().unwrap_or(Start::Home))))),
    );
    #[cfg(not(feature = "wgpu"))]
    let _ = explicit; // built with OpenGL only: nothing to fall back to
    #[cfg(feature = "wgpu")]
    if let Err(e) = &result
        && !explicit
    {
        let fallback = if renderer == eframe::Renderer::Wgpu {
            "glow"
        } else {
            "wgpu"
        };
        eprintln!("cattube: {renderer:?} renderer failed ({e}); retrying with {fallback}");
        if let Ok(exe) = std::env::current_exe() {
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new(exe)
                .args(std::env::args_os().skip(1))
                .env("CATTUBE_RENDERER", fallback)
                .exec();
            eprintln!("cattube: could not restart: {err}");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_replies_follow_the_current_query_and_request() {
        let (mut tube, _) = Tube::open();
        tube.header = true;
        tube.header_index = Section::ALL
            .iter()
            .position(|s| *s == Section::Search)
            .unwrap();
        tube.handle(Action::Ok, Ctx::default(), &|_| false);
        if let Some(Screen::Search { osk, .. }) = tube.stack.last_mut() {
            osk.set_text("new query");
        }
        assert!(search_matches(&tube, 2, 2, "new query"));
        assert!(!search_matches(&tube, 2, 1, "new query"));
        assert!(!search_matches(&tube, 2, 2, "old query"));
        tube.stack.pop();
        assert!(!search_matches(&tube, 2, 2, "new query"));
    }

    #[test]
    fn renderer_defaults_to_glow_on_low_power_and_honors_override() {
        assert_eq!(renderer_for(None, true), (eframe::Renderer::Glow, false));
        #[cfg(feature = "wgpu")]
        {
            assert_eq!(renderer_for(None, false), (eframe::Renderer::Wgpu, false));
            assert_eq!(
                renderer_for(Some("wgpu"), true),
                (eframe::Renderer::Wgpu, true)
            );
        }
        #[cfg(not(feature = "wgpu"))]
        {
            assert_eq!(renderer_for(None, false), (eframe::Renderer::Glow, false));
            assert_eq!(
                renderer_for(Some("wgpu"), false),
                (eframe::Renderer::Glow, false)
            );
        }
        assert_eq!(
            renderer_for(Some("glow"), false),
            (eframe::Renderer::Glow, true)
        );
    }

    #[test]
    fn suggest_debounce_cancels_stale_and_fires_once_when_due() {
        let (mut tube, _) = Tube::open();
        tube.header = true;
        tube.header_index = Section::ALL
            .iter()
            .position(|s| *s == Section::Search)
            .unwrap();
        tube.handle(Action::Ok, Ctx::default(), &|_| false);
        if let Some(Screen::Search { osk, .. }) = tube.stack.last_mut() {
            osk.set_text("cat");
        }
        let t0 = Instant::now();
        let mut pending = Some(("cat".to_string(), t0 + SUGGEST_DEBOUNCE));
        // Before deadline: waits without firing.
        assert!(matches!(
            take_due_suggestion(&tube, &mut pending, t0 + Duration::from_millis(50)),
            PendingSuggest::Wait(_)
        ));
        // At deadline: fires once and clears pending.
        assert!(matches!(
            take_due_suggestion(&tube, &mut pending, t0 + SUGGEST_DEBOUNCE),
            PendingSuggest::Ready(ref q) if q == "cat"
        ));
        assert!(pending.is_none());
        // If user edited the query before deadline, stale pending is dropped.
        let mut stale = Some(("cat".to_string(), t0 + SUGGEST_DEBOUNCE));
        if let Some(Screen::Search { osk, .. }) = tube.stack.last_mut() {
            osk.set_text("cats");
        }
        assert!(matches!(
            take_due_suggestion(&tube, &mut stale, t0 + SUGGEST_DEBOUNCE),
            PendingSuggest::None
        ));
        assert!(stale.is_none());
        // If user left Search screen, pending is dropped.
        let mut left = Some(("cats".to_string(), t0 + SUGGEST_DEBOUNCE));
        tube.stack.pop();
        assert!(matches!(
            take_due_suggestion(&tube, &mut left, t0 + SUGGEST_DEBOUNCE),
            PendingSuggest::None
        ));
        assert!(left.is_none());
    }
}
