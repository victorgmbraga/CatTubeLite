<p align="center">
  <img src="packaging/io.github.coderastic.CatTube.svg" width="128" alt="CatTubeLite">
</p>

<p align="center">
  <strong>English</strong> · <a href="README.pt-BR.md">Português (BR)</a>
</p>

# CatTubeLite

A lightweight fork of [CatTube](https://github.com/coderastic/CatTube) — a native Linux YouTube app for TV screens and remotes — optimized for constrained ARM64 Linux TV boxes (such as BTV9 with a Mali-450 GPU and 1–2 GiB of RAM) while remaining fully compatible with standard Linux desktops.

Like upstream CatTube, it opens full screen, works with arrows, OK and Back on a remote or keyboard (and game controllers on CatosTV), has no ads, and plays videos through `mpv` with SponsorBlock. This fork drops the parts that need a YouTube account or a phone: sign-in, comments, live chat, dislike counts and the phone remote.

## What changed in this fork

CatTubeLite is based on upstream CatTube `1.0.4` and introduces these main sets of changes:

### Lightweight flat UI & rendering
- **Pure black flat theme (`FLAT`):** removes animated background glows, blurred video backdrops, translucent glass layers, soft shadows, specular sheens, redundant overdraw fills, fragment shader dithering (`dithering: false`), and toast fade animations to avoid expensive passes on GPUs such as Mali-450.
- **Zero wasted work under the player:** while opening or covered by fullscreen `mpv`, CatTube stops drawing the catalogue and fetching thumbnails; in `low_power`, it also clears the GL texture cache so `mpv` and the JS runtime have more free RAM on 1 GiB boxes.
- **OpenGL (`glow`) default on `low_power`:** defaults to `glow` instead of `wgpu` on low-power devices, with automatic fallback between `glow` and `wgpu` unless `CATTUBE_RENDERER` is explicitly set.
- **Faster text & list layout:** adds a bounded thread-local text measurement cache (`fit` / `two_lines` using binary search), O(1) resume-position lookups, reduced corner tessellation in reduced-motion mode, vertical culling in Settings, and eliminates per-frame filesystem checks and input-driven `Page` clones.

### ARM64 & low-power playback
- **Automatic low-power profile (`CATTUBE_LOW_POWER`):** enabled automatically with up to 3 GiB of RAM, on Mali-400/450 or VideoCore IV GPUs, and on ARM machines below 6 GiB (overrideable with `CATTUBE_LOW_POWER=1` or `0`). Fresh configurations default to **720p SDR**, **H.264**, and a **32 MiB** buffer on `<= 1.5 GiB` RAM devices (**64 MiB** otherwise) without overwriting existing saved settings.
- **ARM64 JS runtimes, 720p60 H.264 & AV1 exclusion:** downloads the native `aarch64-unknown-linux-gnu` Deno binary on ARM64, also supports system QuickJS (`qjs`/`quickjs`) or `CATTUBE_JS_RUNTIME`, allows up to **720p60** for H.264 in `low_power` while keeping `<= 30 fps` for higher resolutions / other codecs, and excludes AV1 before falling back to any codec when AVC or VP9 is preferred.
- **Lightweight `mpv`, event-driven IPC & OSD:** passes `--hwdec=auto-safe` (unless `CATTUBE_HWDEC` or an `mpv.conf` chooses the decoder), `--profile=fast`, `--scale=bilinear`, `--cscale=bilinear`, `--dscale=bilinear`, `--dither-depth=no`, and `--deband=no` in `low_power`; subscribes to `eof-reached`, `duration`, `pause` and `ytdl_is_live` via `observe_property` instead of polling IPC continuously; stops the OSD tick timer while paused (`1 s` when playing in `low_power`), skips stacked card frost layers and libass Gaussian blurs (`\blur140` / `\blur70`), lowers spinner geometry, writes periodic checkpoints every `30 s` in `low_power` (`5 s` standard), surfaces friendly error toasts when `mpv`/`yt-dlp` fails to start a video, and maps the TV remote `GO_BACK` key in `mpv`.
- **In-memory batched related-video atlas:** builds the in-player Related Videos overlay directly in memory (capped at 12 videos at half scale `840×480` in `low_power`, 24 full-scale standard), fetches `mqdefault` thumbnails in parallel, and flushes in batches of 4 without temporary per-thumbnail files.

### Memory, network & I/O architecture
- **Bounded RAM & disk image caches:** caps decoded texture memory (`16 MiB` in `low_power`, `64 MiB` standard, LRU eviction with focus priority, max side `320 px` / `640 px`) and automatically prunes `~/.cache/cattube/images` on a dedicated maintenance thread (`64 MiB` / 1,500 files in `low_power`; `256 MiB` / 4,000 files standard) after startup I/O settles, plus abandoned `.part` files.
- **Bounded worker & maintenance pools:** separates interactive UI requests (`4` workers in `low_power`, `8` standard) from background tool updates and cache pruning (`cattube-maint`) so startup never stalls interactive browsing, and reads `yt-dlp`'s version directly from `yt-dlp.d/yt_dlp/version.py` without spawning Python.
- **In-process thumbnail fetching (`ureq`):** fetches missing thumbnails and avatars in-process over persistent keep-alive connections (decoding directly from memory and delivering disk-cache hits before waiting on network misses), and uses `mqdefault` / compact `240 px` avatars in `low_power`.
- **Flash-friendly persistence & no wasted Home reloads:** compacts finished per-video `positions/*.pos` files into a 4-field `positions.conf` (backward-compatible with legacy 2-field files), skips `fsync` on disposable `home.txt` cache writes, avoids re-fetching anonymous Home after closing a video, uses a 200 ms UI-timer debounce for search suggestions, and loads History and playback positions off the UI thread.

### Stability, compatibility & atomic persistence
- **Hardened `mpv` lifecycle & IPC:** adds a shared `PlayerHandle` with early-exit detection and bounded `reap_or_kill`, fixes partial-line/EOF handling in `IpcConn`, adds compatibility fallbacks for older `mpv` versions (`loadfile` `< 0.38` and `overlay-add` `< 0.39`), escapes ASS strings with UTF-8-safe truncation in the OSD, and fixes the Up Next card thumbnail width (`256 px`).
- **Bug fixes:** fixes panics on shrinking search suggestions and extreme thumbnail aspect ratios, empty search results locking out the keyboard, SponsorBlock end-of-video outro skipping, `--play` first-run startup and speed persistence, account profile merging without overwriting existing accounts, `views_text` rounding, and localized video menu labels, live badges, and search suggestions.
- **Atomic writes:** writes checkpoints, merged `positions.conf`, search history, blurred backdrops, and downloaded tools via unique sibling temporary files and atomic renames. Settings are also flushed to disk, so they survive a power cut.
- **One player at a time:** CatTube processes (the app, `--play`) coordinate over the `mpv` socket. Whichever starts a video saves the current position and closes the other's player first.
- **Verified tools:** `yt-dlp` and Deno are checked against their published SHA-256 before use. Deno is installed atomically, tested before activation, repaired after an interrupted install, and checked for updates every 30 days.
- **Resilient background jobs:** a crashing job no longer takes down its worker, and the screen waiting on it shows an error instead of loading forever.

### Fewer features
- **No sign-in:** CatTubeLite never signs in to YouTube and keeps no tokens. The sign-in screen, accounts, Subscriptions, Playlists and Channels tabs, and the Like / Dislike / Subscribe / Watch later actions are gone. A leftover `~/.config/cattube/accounts.conf` from upstream CatTube is ignored; delete it if you no longer need it.
- **No comments, live chat or dislike counts:** the player has no Comments or Chat buttons, and the video menu no longer asks Return YouTube Dislike for counts.
- **No phone remote:** the YouTube app on a phone can no longer pair with or cast to CatTubeLite. The Link a phone setting, `CATTUBE_CAST_ENABLED_FILE` and the `--remote` service are gone; `--remote`, `--tv-code` and `--pair` now exit with code 2 without opening a window. A leftover `~/.config/cattube/remote.conf` is ignored.
- **No Arabic:** the interface comes in English or Brazilian Portuguese, and a `language = ar` left in `settings.conf` opens in English. Arabic support is gone entirely: the bundled Noto Sans Arabic font, Arabic shaping and right-to-left layout, and the Arabic keyboard layout. Arabic titles may show as boxes or with letters in the wrong order. In the subtitle language choice, Portuguese takes Arabic's place.
- **What stays:** Home (YouTube's live streams and popular topics), History (kept on this TV), Search, channel and playlist pages, SponsorBlock and Up next.

## What it does

- **YouTube without an account.** Home shows live streams and popular topics, History lists what was watched on this TV, and Search finds the rest. Channel and playlist pages open from any video.
- **Search** with debounced suggestions as you type and recent searches when the box is empty. Results show matching channels first, followed by a paginated grid of videos.
- **A player made for the remote.** OK shows the controls, Left and Right skip 10 seconds, Up goes to the progress bar. Quality, subtitles and audio tracks are a button away; in this fork videos always play at normal speed and Back closes the player (there is no Stop button), chapters show on the bar, and changing quality keeps your position. Only these controls act on the player: `mpv`'s own keys (volume, the number keys, mouse wheel, and so on) are turned off.
- **SponsorBlock.** Sponsor segments, intros, outros, reminders and other chosen categories are skipped and marked on the progress bar.
- **Video menu.** Hold OK on a video to see its views and date, start over or open the channel.
- **Up next.** Plays the next video after a short countdown, loops, or stops — configurable in Settings.
- **History & resume.** Tracks watched videos and resumes where you left off.
- **Mouse support.** Hover to focus, click to open.

## Languages

Choose **Settings → Playback → Interface language** for **Português (Brasil)**. The player's on-screen controls stay in English. This fork has no Arabic support (see [Fewer features](#fewer-features)). Subtitle language is configured separately.

## Build & install

CatTubeLite is built from source. You need Rust (edition 2024), `mpv`, `curl`, and Python 3.

```sh
git clone https://github.com/victorgmbraga/CatTubeLite.git
cd CatTubeLite
cargo build --release --locked
./target/release/cattube
```

To run the test suite:

```sh
cargo test --locked --workspace
```

To build a package for the current architecture and install it into `~/.local`:

```sh
packaging/tarball.sh
tar -xzf dist/cattube-*-"$(uname -m)".tar.gz -C dist
dist/cattube-*-"$(uname -m)"/install.sh
```

CatTubeLite is built without the `wgpu` (Vulkan) renderer by default, on every architecture: it draws with OpenGL and the binary is about 31% smaller. For a desktop build with Vulkan, use `cargo build --release --features wgpu`, or `CATTUBE_WGPU=1 packaging/tarball.sh`.

### Your own name and logo

For a build made only for yourself, two build-time variables change the app's name and logo. Without them nothing changes, and the released packages built by CI always keep CatTube's own name and mark.

| Variable | What it changes |
|---|---|
| `CATTUBE_NAME` | The name in the window title, the header, Settings and the desktop entry (menus and launchers) |
| `CATTUBE_LOGO` | A PNG drawn in place of the cat mark (in the header and on the channel and Settings screens), also installed as the desktop icon |

Build the package with them and install it as usual (on the TV box too, for `aarch64`):

```sh
CATTUBE_NAME="My Tube" CATTUBE_LOGO=/path/to/logo.png packaging/tarball.sh
tar -xzf dist/cattube-*-"$(uname -m)".tar.gz -C dist
dist/cattube-*-"$(uname -m)"/install.sh
```

- **The logo:** it must be a PNG (SVG is not accepted), square or wider than tall, with a transparent background and at least 256 px tall. In the app it is drawn at the height of the original mark, keeping its proportions. If the file is missing or is not a PNG, the build stops with an error.
- **Installing over an earlier install:** `install.sh` copies only the icon the package has (`share/pixmaps/io.github.coderastic.CatTube.png` for a logo, the cat SVG otherwise) and removes the other one left by an earlier install.
- **A plain `cargo build`:** it reads the same variables, but then only the app changes. The desktop entry and the icon come from `packaging/tarball.sh`.
- **What stays the same:** the `cattube` command, the `~/.config/cattube` and `~/.local/share/cattube` folders, and the internal id `io.github.coderastic.CatTube` (window app id and desktop file name). Settings, history and CatosTV keep working as before.

The logo file is yours to supply: this repository ships no third-party logo. Do not redistribute a build that carries another company's name or logo.

The package does not bundle `mpv`, `curl` or Python 3, so they must be installed on the device.

On first start, CatTubeLite downloads its own copy of [yt-dlp](https://github.com/yt-dlp/yt-dlp) and [Deno](https://deno.com) (using the native ARM64 build on `aarch64`) into `~/.local/share/cattube`, verifies them, and keeps them updated automatically. yt-dlp is also unpacked next to its single-file release (`yt-dlp.d`, about 16 MB), so Python keeps its modules compiled instead of compiling them again for every video; this makes each video start faster, by about 0.6 s on a desktop and by more on a TV box.

> This fork does not ship Flatpak or AppImage packages. For those, see the original project, [coderastic/CatTube](https://github.com/coderastic/CatTube).

## Low-power devices & diagnostics

| Environment variable | Values | Effect |
|---|---|---|
| `CATTUBE_HWDEC` | an mpv `--hwdec` value (`v4l2m2m-copy`, `vaapi`, `no`…) | Forces the video decoder. Unset, a `hwdec=` in your `mpv.conf` (`~/.config/mpv/` or `/etc/mpv/`) is used as is; with neither, low-power devices get `auto-safe` and others mpv's default. `auto-safe` leaves out `v4l2m2m-copy`, which some TV boxes need: set it in `mpv.conf` or here |
| `CATTUBE_JS_RUNTIME` | `quickjs:/usr/bin/qjs`, `deno:/path`, `node:/path`… | Overrides the JavaScript runtime passed to `yt-dlp`. Unset, CatTubeLite checks system `deno`, its own `deno`, system `qjs`/`quickjs`, then `node` |
| `CATTUBE_YTDL_RESIDENT` | `0` | Turns off the resident `yt-dlp`. By default CatTubeLite keeps one `yt-dlp` (and one Deno with YouTube's player functions ready) loaded between videos, so a video opens in about a third of the time on a TV box, at the cost of ~100 MB of RAM |
| `CATTUBE_LOW_POWER` | `1` / `0` | Forces low-power or standard profile (detected from RAM, GPU and architecture when unset). Among other things, low-power cards use YouTube's 320×180 video pictures (`mqdefault`) instead of the 480×360 letterboxed ones: a third of the pixels to decode, a smaller download, and a sharper card |
| `CATTUBE_RENDERER` | `glow` / `wgpu` | Forces UI renderer. Only builds with `--features wgpu` have `wgpu`; they default to `glow` in `low_power` and `wgpu` otherwise, with automatic fallback when unset |

To see hardware decoding, dropped frames and memory over the video, turn on **Settings → About → Player statistics**. You can also query `mpv`'s IPC socket at `$XDG_RUNTIME_DIR/cattube-mpv.sock` (`/tmp/cattube-$(id -u)/cattube-mpv.sock` without it) for `hwdec-current` and `frame-drop-count`.

## Keys

| Remote / keyboard | In the lists | In the player |
|---|---|---|
| Arrows | move around | Left / Right skip 10 s (with the controls hidden, the progress bar shows alone for a moment), Up / Down between seek bar, controls and related videos |
| Rewind / Forward, Previous / Next | | skip 30 s / 90 s, with the same progress bar |
| OK, Enter | open, play | show controls, press focused button |
| Hold OK | video menu | |
| Back, Esc, `GO_BACK` | back | hide controls, then stop |

## Command line

| Option | What it does |
|---|---|
| `--section NAME` | open on `home`, `history` or `search` |
| `--play VIDEO_ID` | play one video, then close |
| `--feed home` | print Home's videos (for a TV home screen); other lists exit with code 2 |
| `--stop` | close the player |
| `--windowed` | start in a window instead of full screen |

## Your files

- `~/.config/cattube/` — settings.
- `~/.local/share/cattube/` — `yt-dlp` (and its unpacked copy `yt-dlp.d` with the `yt-dlp-unpacked` launcher), `deno`, watch history, playback checkpoints, `positions.conf` and recent searches. Live streams keep no position: they always start where the stream is now.
- `~/.cache/cattube/` — cached thumbnails (auto-pruned).

The cache folder also keeps `home.txt`, the last Home: it shows at once on the next start (if under a week old and for the same language and country) while a fresh one loads, and stays if that fails.

## Credits & thanks

- Forked from [CatTube](https://github.com/coderastic/CatTube) by [coderastic](https://github.com/coderastic).
- [yt-dlp](https://github.com/yt-dlp/yt-dlp), [Deno](https://deno.com), and [mpv](https://mpv.io) power video extraction and playback.
- [SponsorBlock](https://sponsor.ajay.app) provides the segments to skip.

## License

MIT, see [`LICENSE`](LICENSE). The bundled Inter (cut down to Latin, Greek and Cyrillic by `packaging/subset-fonts.sh`) and Noto Emoji fonts use the SIL Open Font License; their notices are in [`assets/fonts/`](assets/fonts/). CatTubeLite is not affiliated with YouTube or Google.
