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
- **Pure black flat theme (`FLAT`):** removes animated background glows, blurred video backdrops, translucent glass layers, soft shadows, specular sheens, redundant overdraw fills, and toast fade animations to avoid expensive alpha-blended passes on GPUs such as Mali-450.
- **OpenGL (`glow`) default on `low_power`:** defaults to `glow` instead of `wgpu` on low-power devices, with automatic fallback between `glow` and `wgpu` unless `CATTUBE_RENDERER` is explicitly set.
- **Faster text & list layout:** adds a bounded thread-local text measurement cache (`fit` / `two_lines` using binary search), an ASCII fast path that skips Unicode BiDi/shaping work, reduced corner tessellation in reduced-motion mode, vertical culling in Settings, and eliminates per-frame filesystem checks and input-driven `Page` clones.

### ARM64 & low-power playback
- **Automatic low-power profile (`CATTUBE_LOW_POWER`):** enabled automatically with up to 3 GiB of RAM, on Mali-400/450 or VideoCore IV GPUs, and on ARM machines below 6 GiB (overrideable with `CATTUBE_LOW_POWER=1` or `0`). Fresh configurations default to **720p SDR**, **H.264**, and a **64 MiB** forward demuxer buffer without overwriting existing saved settings.
- **ARM64 Deno & 30 fps priority:** downloads the native `aarch64-unknown-linux-gnu` Deno binary on ARM64 and prioritizes `<= 30 fps` streams before falling back to higher frame rates.
- **Lightweight `mpv` & OSD pipeline:** passes `--hwdec=auto-safe`, `--profile=fast`, `--scale=bilinear`, `--cscale=bilinear`, `--dscale=bilinear`, `--dither-depth=no`, and `--deband=no` in `low_power`; replaces CPU-heavy libass Gaussian blurs (`\blur140` / `\blur70`) in the OSD with flat scrim bands; lowers spinner geometry; removes per-key `cattube-input` IPC wakes; replaces per-frame position writes with a periodic checkpoint timer (`10 s` in `low_power`, `5 s` standard); and maps the TV remote `GO_BACK` key in `mpv`.
- **In-memory batched related-video atlas:** builds the in-player Related Videos overlay directly in memory (capped at 12 videos in `low_power`, 24 standard), prefetches rows in parallel, and flushes in batches of 4 instead of writing temporary files per thumbnail.

### Memory, network & I/O architecture
- **Bounded RAM & disk image caches:** caps decoded texture memory (`16 MiB` in `low_power`, `64 MiB` standard, LRU eviction with focus priority, max side `320 px` / `640 px`) and automatically prunes `~/.cache/cattube/images` on startup (`64 MiB` / 1,500 files in `low_power`; `256 MiB` / 4,000 files standard) plus abandoned `.part` files.
- **Bounded worker thread pool:** replaces per-request thread spawning with a fixed worker pool (`4` workers in `low_power`, `8` standard).
- **Parallel thumbnail batching:** fetches missing thumbnails in batches via `curl --parallel`, reusing HTTP/2 + TLS connections.
- **Real search debounce & non-blocking I/O:** uses a 200 ms UI-timer debounce with generation cancellation for search suggestions, loads History and playback positions off the UI thread with single-pass `O(N)` checkpoint merging, and replaces 10 ms polling loops in subprocess execution with event-driven EOF waits, pipe size caps, and protection against leaked grandchild file descriptors.

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
- **No Arabic interface:** the interface comes in English or Brazilian Portuguese. A `language = ar` left in `settings.conf` opens in English. Arabic titles, search text and the Arabic keyboard layout still work.
- **What stays:** Home (YouTube's live streams and popular topics), History (kept on this TV), Search, channel and playlist pages, SponsorBlock and Up next.

## What it does

- **YouTube without an account.** Home shows live streams and popular topics, History lists what was watched on this TV, and Search finds the rest. Channel and playlist pages open from any video.
- **Search** with debounced suggestions as you type and recent searches when the box is empty. Results show matching channels first, followed by a paginated grid of videos.
- **A player made for the remote.** OK shows the controls, Left and Right skip 10 seconds, Up goes to the progress bar. Quality, subtitles and audio tracks are a button away; in this fork videos always play at normal speed and Back closes the player (there is no Stop button), chapters show on the bar, and changing quality keeps your position.
- **SponsorBlock.** Sponsor segments, intros, outros, reminders and other chosen categories are skipped and marked on the progress bar.
- **Video menu.** Hold OK on a video to see its views and date, start over or open the channel.
- **Up next.** Plays the next video after a short countdown, loops, or stops — configurable in Settings.
- **History & resume.** Tracks watched videos and resumes where you left off.
- **Mouse support.** Hover to focus, click to open.

## Languages

Choose **Settings → Playback → Interface language** for **Português (Brasil)**. The player's on-screen controls stay in English. This fork has no Arabic interface; Arabic titles and search text still display properly, and the on-screen keyboard keeps its Arabic layout (**ABC → العربية**). Subtitle language is configured separately.

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

On `aarch64` the package is built without the `wgpu` renderer (`--no-default-features`), which makes the binary about 31% smaller. It then always draws with OpenGL.

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

On first start, CatTubeLite downloads its own copy of [yt-dlp](https://github.com/yt-dlp/yt-dlp) and [Deno](https://deno.com) (using the native ARM64 build on `aarch64`) into `~/.local/share/cattube`, verifies them, and keeps them updated automatically.

> This fork does not ship Flatpak or AppImage packages. For those, see the original project, [coderastic/CatTube](https://github.com/coderastic/CatTube).

## Low-power devices & diagnostics

| Environment variable | Values | Effect |
|---|---|---|
| `CATTUBE_LOW_POWER` | `1` / `0` | Forces low-power or standard profile (detected from RAM, GPU and architecture when unset) |
| `CATTUBE_RENDERER` | `glow` / `wgpu` | Forces UI renderer (defaults to `glow` in `low_power`, `wgpu` otherwise, with automatic fallback when unset) |

To see hardware decoding, dropped frames and memory over the video, turn on **Settings → About → Player statistics**. You can also query `mpv`'s IPC socket at `$XDG_RUNTIME_DIR/cattube-mpv.sock` (`/tmp/cattube-$(id -u)/cattube-mpv.sock` without it) for `hwdec-current` and `frame-drop-count`.

## Keys

| Remote / keyboard | In the lists | In the player |
|---|---|---|
| Arrows | move around | Left / Right skip 10 s, Up / Down between seek bar, controls and related videos |
| OK, Enter | open, play | show controls, press focused button |
| Hold OK, F2 | video menu | subtitle menu (fallback) |
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
- `~/.local/share/cattube/` — `yt-dlp`, `deno`, watch history, playback checkpoints, `positions.conf` and recent searches.
- `~/.cache/cattube/` — cached thumbnails (auto-pruned).

## Credits & thanks

- Forked from [CatTube](https://github.com/coderastic/CatTube) by [coderastic](https://github.com/coderastic).
- [yt-dlp](https://github.com/yt-dlp/yt-dlp), [Deno](https://deno.com), and [mpv](https://mpv.io) power video extraction and playback.
- [SponsorBlock](https://sponsor.ajay.app) provides the segments to skip.

## License

MIT, see [`LICENSE`](LICENSE). Bundled Inter and Noto Sans Arabic fonts use the SIL Open Font License; their notices are in [`assets/fonts/`](assets/fonts/). CatTubeLite is not affiliated with YouTube or Google.
