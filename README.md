<p align="center">
  <img src="packaging/io.github.coderastic.CatTube.svg" width="128" alt="CatTubeLite">
</p>

<p align="center">
  <strong>English</strong> · <a href="README.pt-BR.md">Português (BR)</a>
</p>

# CatTubeLite

A lightweight fork of [CatTube](https://github.com/coderastic/CatTube) — a native Linux YouTube app for TV screens and remotes — optimized for constrained ARM64 Linux TV boxes (such as BTV9 with a Mali-450 GPU and 1–2 GiB of RAM) while remaining fully compatible with standard Linux desktops.

Like upstream CatTube, it opens full screen, works with arrows, OK and Back on a remote or keyboard (and game controllers on CatosTV), has no ads, and plays videos through `mpv` with SponsorBlock, Return YouTube Dislike, phone casting, comments and live chat.

## What changed in this fork

CatTubeLite is based on upstream CatTube `1.0.4` and introduces four main sets of changes:

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
- **Parallel thumbnail batching & OAuth serialization:** fetches missing thumbnails in batches via `curl --parallel` (reusing HTTP/2 + TLS connections), caps subscription feed concurrency (`--parallel-max` 8 in `low_power`, 24 standard), and serializes OAuth token refreshes to prevent concurrent refresh bursts.
- **Real search debounce & non-blocking I/O:** uses a 200 ms UI-timer debounce with generation cancellation for search suggestions, loads History and playback positions off the UI thread with single-pass `O(N)` checkpoint merging, and replaces 10 ms polling loops in subprocess execution with event-driven EOF waits, pipe size caps, and protection against leaked grandchild file descriptors.
- **Lighter `--remote` daemon:** sleeps 1 s instead of 200 ms when casting is disabled, runs stale `yt-dlp` update checks in the background when tools are already installed, and throttles `mpv` state polling to 1 Hz.

### Stability, compatibility & atomic persistence
- **Hardened `mpv` lifecycle & IPC:** adds a shared `PlayerHandle` with early-exit detection and bounded `reap_or_kill`, fixes partial-line/EOF handling in `IpcConn`, adds compatibility fallbacks for older `mpv` versions (`loadfile` `< 0.38` and `overlay-add` `< 0.39`), escapes ASS strings with UTF-8-safe truncation in the OSD, fixes the Up Next card thumbnail width (`256 px`), and guards live chat/comments threads against stale video switches.
- **Bug fixes:** fixes panics on shrinking search suggestions and extreme thumbnail aspect ratios, empty search results locking out the keyboard, SponsorBlock end-of-video outro skipping, `--play` first-run startup and speed persistence, account profile merging without overwriting existing accounts, `views_text` rounding, and localized video menu labels, live badges, and search suggestions.
- **Atomic writes:** writes accounts, remote pairing, checkpoints, merged `positions.conf`, search history, blurred backdrops, and downloaded tools via unique sibling temporary files and atomic renames. Settings, accounts and remote pairing are also flushed to disk, so they survive a power cut.
- **One player at a time:** the app and the `--remote` daemon coordinate over the `mpv` socket. Whichever starts a video saves the current position and closes the other's player first.
- **Verified tools:** `yt-dlp` and Deno are checked against their published SHA-256 before use. Deno is installed atomically, tested before activation, repaired after an interrupted install, and checked for updates every 30 days.
- **Resilient background jobs:** a crashing job no longer takes down its worker, and the screen waiting on it shows an error instead of loading forever.

## What it does

- **Your YouTube.** Home, Subscriptions, History, Playlists and Channels. You sign in the TV way: CatTubeLite shows a code and you enter it at `google.com/device` on your phone. Multiple accounts are supported. Signed out, Home shows live streams and popular topics.
- **Search** with debounced suggestions as you type and recent searches when the box is empty. Results show matching channels first, followed by a paginated grid of videos.
- **A player made for the remote.** OK shows the controls, Left and Right skip 10 seconds, Up goes to the progress bar. Speed, quality, subtitles and audio tracks are a button away, chapters show on the bar, and changing quality keeps your position.
- **SponsorBlock.** Sponsor segments, intros, outros, reminders and other chosen categories are skipped and marked on the progress bar.
- **Dislikes** via Return YouTube Dislike. Hold OK on a video to like/dislike it, subscribe, save to Watch later or open the channel.
- **Comments and live chat** next to the video.
- **Up next.** Plays the next video after a short countdown, loops, or stops — configurable in Settings.
- **Phone remote / casting.** In the YouTube mobile app go to Settings → Watch on TV → Link with TV code and enter the code from Settings.
- **History & resume.** Tracks watched videos and resumes where you left off.
- **Mouse support.** Hover to focus, click to open.

## Languages

Choose **Settings → Playback → Interface language** for **Português (Brasil)** or **العربية** (Arabic). The player's on-screen controls stay in English. Arabic titles and search text also display properly in the English interface, and the on-screen keyboard includes an Arabic layout (**ABC → العربية**). Subtitle language is configured separately.

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

The package does not bundle `mpv`, `curl` or Python 3, so they must be installed on the device.

On first start, CatTubeLite downloads its own copy of [yt-dlp](https://github.com/yt-dlp/yt-dlp) and [Deno](https://deno.com) (using the native ARM64 build on `aarch64`) into `~/.local/share/cattube`, verifies them, and keeps them updated automatically.

> This fork does not ship Flatpak or AppImage packages. For those, see the original project, [coderastic/CatTube](https://github.com/coderastic/CatTube).

## Low-power devices & diagnostics

| Environment variable | Values | Effect |
|---|---|---|
| `CATTUBE_LOW_POWER` | `1` / `0` | Forces low-power or standard profile (detected from RAM, GPU and architecture when unset) |
| `CATTUBE_RENDERER` | `glow` / `wgpu` | Forces UI renderer (defaults to `glow` in `low_power`, `wgpu` otherwise, with automatic fallback when unset) |
| `CATTUBE_CAST_ENABLED_FILE` | path | Optional preference file containing `cast_enabled = true` or `false` to toggle phone casting live |

To see hardware decoding, dropped frames and memory over the video, turn on **Settings → About → Player statistics**. You can also query `mpv`'s IPC socket at `$XDG_RUNTIME_DIR/cattube-mpv.sock` (`/tmp/cattube-$(id -u)/cattube-mpv.sock` without it) for `hwdec-current` and `frame-drop-count`.

## Keys

| Remote / keyboard | In the lists | In the player |
|---|---|---|
| Arrows | move around | Left / Right skip 10 s, Up / Down between seek bar, controls and related videos |
| OK, Enter | open, play | show controls, press focused button |
| Hold OK, F2 | video menu | subtitle menu (fallback) |
| Back, Esc, `GO_BACK` | back | hide controls, then stop |

## Shared casting switch

On CatosTV, the **Cast to this TV** switch also controls the paired-phone connection. Turning it off closes phone casting and blocks new pairing while keeping local browsing and playback available. Other host shells can set `CATTUBE_CAST_ENABLED_FILE` to a file containing `cast_enabled = true` or `cast_enabled = false`; changes apply while running.

## Command line

| Option | What it does |
|---|---|
| `--section NAME` | open on `home`, `subscriptions`, `history`, `playlists`, `channels` or `search` |
| `--play VIDEO_ID` | play one video, then close |
| `--remote` | stay online for the phone app, without a window |
| `--tv-code` | print the code for Link with TV code |
| `--feed subscriptions` | print the newest videos from your subscriptions (for a TV home screen) |
| `--windowed` | start in a window instead of full screen |

## Your files

- `~/.config/cattube/` — settings, signed-in accounts (`0600`) and phone pairing (`0600`).
- `~/.local/share/cattube/` — `yt-dlp`, `deno`, watch history, playback checkpoints, `positions.conf` and recent searches.
- `~/.cache/cattube/` — cached thumbnails (auto-pruned) and YouTube client metadata.

Signing out in Settings also revokes the token at Google.

## Credits & thanks

- Forked from [CatTube](https://github.com/coderastic/CatTube) by [coderastic](https://github.com/coderastic).
- [yt-dlp](https://github.com/yt-dlp/yt-dlp), [Deno](https://deno.com), and [mpv](https://mpv.io) power video extraction and playback.
- [SponsorBlock](https://sponsor.ajay.app) and [Return YouTube Dislike](https://returnyoutubedislike.com) provide community metadata.
- [yt-cast-receiver](https://github.com/patrickkfkan/yt-cast-receiver) documented the YouTube Lounge phone-remote protocol.

## License

MIT, see [`LICENSE`](LICENSE). Bundled Inter and Noto Sans Arabic fonts use the SIL Open Font License; their notices are in [`assets/fonts/`](assets/fonts/). CatTubeLite is not affiliated with YouTube or Google.
