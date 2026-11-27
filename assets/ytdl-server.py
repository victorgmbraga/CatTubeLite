"""A yt-dlp that stays loaded, so a video opens without starting it again.

On a TV box, starting yt-dlp (Python and its modules) takes seconds, and
solving YouTube's JavaScript challenge takes more: Deno has to parse the
whole player script for every video. This keeps one yt-dlp loaded, keeps
its preprocessed copy of the player between videos (yt-dlp has that cache,
switched off), and runs each request in a fork of the loaded process.

    python3 -I ytdl-server.py serve SOCKET YT_DLP_DIR [DENO]
        Listens on SOCKET until CatTube (its parent) goes away. With DENO,
        also keeps a Deno running with the player's functions built
        (ytdl-jsc.js), instead of yt-dlp starting one per video.
    python3 -I -S ytdl-server.py run SOCKET YT_DLP_DIR [yt-dlp args...]
        What mpv runs as yt-dlp: hands the arguments and its stdin, stdout
        and stderr to the server and exits with yt-dlp's code. Without a
        server it runs yt-dlp itself, as before.
"""

import array
from collections import OrderedDict
import json
import os
import re
import select
import signal
import socket
import stat
import struct
import sys
import time
from urllib.parse import parse_qs, urlparse

# Preprocessed players kept (about 4 MB each; YouTube changes it every few days).
KEEP_PLAYERS = 2
MAX_REQUEST = 1 << 20
RESOLVE_CACHE_MAX = 6
RESOLVE_CACHE_TTL = 900
_VIDEO_ID_RE = re.compile(r"^[A-Za-z0-9_-]{11}$")
_SUB_EXT_ORDER = ("vtt", "srt", "ttml", "srv3", "srv2", "srv1")


def fallback(ytdlp_dir, args):
    os.execvp("python3", ["python3", ytdlp_dir, *args])


def _write_private_file(path, text):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        f.write(text)


def recv_request(conn):
    fds = array.array("i")
    data, anc, _flags, _addr = conn.recvmsg(MAX_REQUEST, socket.CMSG_LEN(3 * fds.itemsize))
    for level, kind, payload in anc:
        if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
            fds.frombytes(payload[: len(payload) - (len(payload) % fds.itemsize)])
    if fds:
        return json.loads(data), list(fds)
    buf = bytearray(data)
    while b"\n" not in buf:
        if len(buf) >= MAX_REQUEST:
            raise ValueError("request too long")
        part = conn.recv(min(4096, MAX_REQUEST - len(buf)))
        if not part:
            break
        buf += part
    line = bytes(buf).split(b"\n", 1)[0]
    if not line:
        raise ValueError("empty request")
    return json.loads(line), []


def run(sock_path, ytdlp_dir, args):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        s.connect(sock_path)
        msg = json.dumps({"args": args, "cwd": os.getcwd()}).encode()
        if len(msg) >= MAX_REQUEST:
            raise OSError("request too long")
        s.sendmsg([msg], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", [0, 1, 2]))])
    except OSError:
        s.close()
        fallback(ytdlp_dir, args)
    # mpv may stop waiting (the user went back): end the request with us
    signal.signal(signal.SIGTERM, lambda *_: os._exit(143))
    code = b""
    while len(code) < 4:
        part = s.recv(4 - len(code))
        if not part:
            sys.exit(1)  # the server went away mid-request
        code += part
    sys.exit(struct.unpack("!i", code)[0])


def prune_players(cache_dir):
    """yt-dlp never removes old preprocessed players: keep the newest few."""
    d = os.path.join(cache_dir, "challenge-solver")
    try:
        names = [os.path.join(d, n) for n in os.listdir(d) if n.startswith("player")]
        names.sort(key=os.path.getmtime, reverse=True)
        for old in names[KEEP_PLAYERS:]:
            os.remove(old)
    except OSError:
        pass


class _Stdin(str):
    """Lazy yt-dlp Deno stdin: avoids building the ~4 MB JS payload unless fallback is needed."""

    data = None


def jsc_request(sock_path, data):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(60)
    try:
        s.connect(sock_path)
        s.sendall(json.dumps(data).encode())
        s.shutdown(socket.SHUT_WR)
        out = bytearray()
        while part := s.recv(1 << 16):
            out += part
    finally:
        s.close()
    text = out.decode()
    return text, json.loads(text)


def start_jsc(deno, sock_path):
    """A Deno that stays running with yt-dlp's challenge solver loaded (see
    ytdl-jsc.js), and yt-dlp's Deno provider sending its work there. On any
    problem yt-dlp runs its own Deno, as usual."""
    try:
        import yt_dlp_ejs.yt.solver as ejs_solver
        from yt_dlp.extractor.youtube.jsc._builtin import deno as deno_jcp
    except ImportError:
        return None
    here = os.path.dirname(os.path.abspath(__file__))
    with open(os.path.join(here, "ytdl-jsc.js"), encoding="utf-8") as f:
        daemon = f.read()
    lib_path = sock_path + ".lib.js"
    script = sock_path + ".js"
    _write_private_file(
        lib_path,
        f"{ejs_solver.lib()}\nObject.assign(globalThis, lib);\n{ejs_solver.core()}\nglobalThis.__cattube_jsc = jsc;\n",
    )
    _write_private_file(script, daemon)
    try:
        os.unlink(sock_path)
    except FileNotFoundError:
        pass
    import subprocess

    proc = subprocess.Popen(
        [
            deno, "run", "--no-prompt", "--no-remote", "--no-npm", "--no-lock",
            "--no-config", "--cached-only",
            f"--allow-read={sock_path},{lib_path}",
            f"--allow-write={sock_path}",
            f"--allow-net=unix:{sock_path}",
            script, sock_path, lib_path,
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
    )

    orig_construct = deno_jcp.DenoJCP._construct_stdin
    orig_run = deno_jcp.DenoJCP._run_js_runtime

    def construct(self, player, preprocessed, requests, /):
        out = _Stdin(" ")
        reqs = [{"type": r.type.value, "challenges": r.input.challenges} for r in requests]
        key = requests[0].input.player_url if requests else ""
        out.data = (self, player, preprocessed, requests, key, reqs)
        return out

    def run_js(self, stdin, /):
        lazy = getattr(stdin, "data", None)
        if lazy is None:
            return orig_run(self, stdin)
        owner, player, preprocessed, requests, key, reqs = lazy
        if os.path.exists(sock_path):
            try:
                if key:
                    text, parsed = jsc_request(
                        sock_path,
                        {"type": "cached", "key": key, "requests": reqs},
                    )
                    if parsed.get("type") == "result":
                        return text
                    if parsed.get("type") != "miss":
                        raise OSError(text[:300])
                payload = (
                    {
                        "key": key,
                        "type": "preprocessed",
                        "preprocessed_player": player,
                        "requests": reqs,
                    }
                    if preprocessed
                    else {
                        "key": key,
                        "type": "player",
                        "player": player,
                        "requests": reqs,
                        "output_preprocessed": True,
                    }
                )
                text, parsed = jsc_request(sock_path, payload)
                if parsed.get("type") == "result":
                    return text
                raise OSError(text[:300])
            except (OSError, ValueError) as e:
                self.logger.debug(f"resident deno failed ({e}); running deno")
        real_stdin = orig_construct(owner, player, preprocessed, requests)
        return orig_run(self, real_stdin)

    deno_jcp.DenoJCP._construct_stdin = construct
    deno_jcp.DenoJCP._run_js_runtime = run_js
    return proc


def _reap_children(child_pids):
    for pid in list(child_pids):
        try:
            done, _ = os.waitpid(pid, os.WNOHANG)
            if done != 0:
                child_pids.discard(pid)
        except ChildProcessError:
            child_pids.discard(pid)


def serve(sock_path, ytdlp_dir, deno=None):
    sys.path.insert(0, ytdlp_dir)
    import yt_dlp  # noqa: F401  (the slow part, done once)
    import yt_dlp.extractor.youtube  # noqa: F401

    try:
        from yt_dlp.extractor.youtube.jsc._builtin import ejs

        ejs.EJSBaseJCP._ENABLE_PREPROCESSED_PLAYER_CACHE = True
    except (ImportError, AttributeError):
        pass  # another yt-dlp: still faster without the cache
    jsc_proc = None
    if deno:
        try:
            jsc_proc = start_jsc(deno, sock_path + ".jsc")
        except Exception as e:  # noqa: BLE001
            print(f"ytdl-server: no resident deno: {e!r}", file=sys.stderr)
    try:
        from yt_dlp.cache import Cache

        cache_dir = Cache(yt_dlp.YoutubeDL({"quiet": True}))._get_root_dir()
    except Exception:
        cache_dir = None

    watch_stdin = False
    try:
        watch_stdin = stat.S_ISFIFO(os.fstat(0).st_mode)
    except OSError:
        pass

    parent = os.getppid()
    resolver_state = {
        "yt_dlp": yt_dlp,
        "js_runtimes": {"deno": {"path": deno}} if deno else {},
        "ydl": {},
        "cache": OrderedDict(),
    }
    try:
        os.unlink(sock_path)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(sock_path)
    os.chmod(sock_path, 0o600)
    srv.listen(8)
    srv.setblocking(False)
    child_pids = set()
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    try:
        while os.getppid() == parent:
            _reap_children(child_pids)
            read_fds = [srv]
            if watch_stdin:
                read_fds.append(0)
            try:
                ready, _, _ = select.select(read_fds, [], [], 5.0)
            except InterruptedError:
                continue
            if watch_stdin and 0 in ready:
                try:
                    if not os.read(0, 64):
                        break
                except OSError:
                    break
            if srv not in ready:
                continue
            try:
                conn, _ = srv.accept()
            except BlockingIOError:
                continue
            if cache_dir:
                prune_players(cache_dir)
            conn.setblocking(True)
            conn.settimeout(5.0)
            try:
                req, fds = recv_request(conn)
            except (OSError, ValueError):
                try:
                    conn.close()
                except OSError:
                    pass
                continue
            if not fds and req.get("mode") == "resolve":
                try:
                    handle_resolve(conn, req, resolver_state)
                finally:
                    try:
                        conn.close()
                    except OSError:
                        pass
                continue
            pid = os.fork()
            if pid == 0:
                srv.close()
                if jsc_proc and jsc_proc.stdin:
                    try:
                        jsc_proc.stdin.close()
                    except OSError:
                        pass
                if watch_stdin:
                    try:
                        devnull = os.open(os.devnull, os.O_RDONLY)
                        if devnull != 0:
                            os.dup2(devnull, 0)
                            os.close(devnull)
                    except OSError:
                        pass
                handle_parsed(conn, req, fds)  # never returns
            child_pids.add(pid)
            conn.close()
            for fd in fds:
                try:
                    os.close(fd)
                except OSError:
                    pass
    finally:
        _reap_children(child_pids)
        if jsc_proc:
            try:
                if jsc_proc.stdin:
                    jsc_proc.stdin.close()
            except OSError:
                pass
            try:
                jsc_proc.terminate()
                jsc_proc.wait(timeout=1)
            except Exception:  # noqa: BLE001
                try:
                    jsc_proc.kill()
                    jsc_proc.wait(timeout=1)
                except Exception:  # noqa: BLE001
                    pass
        for p in (sock_path, sock_path + ".jsc", sock_path + ".jsc.js", sock_path + ".jsc.lib.js"):
            try:
                os.unlink(p)
            except OSError:
                pass


def limit_caption_translations(args):
    """yt-dlp lists every automatic caption translation (about 180
    languages, times each format and player client): thousands of URLs,
    10+ s of a TV box's CPU on videos with several caption tracks, of which
    mpv only loads `--sub-langs`. Keep the translations into those
    languages; anything other than plain language codes keeps them all."""
    langs = []
    for i, a in enumerate(args):
        if a == "--":
            break
        if a == "--sub-langs" and i + 1 < len(args):
            langs += args[i + 1].split(",")
        elif a.startswith("--sub-langs="):
            langs += a.split("=", 1)[1].split(",")
    if not langs:
        return
    import re

    bases = set()
    for lang in langs:
        m = re.match(r"[A-Za-z]{2,3}(?=$|[-(.])", lang)
        if not m:
            return  # "all", exclusions, other patterns: leave yt-dlp alone
        bases.add(m.group(0).lower())
    try:
        from yt_dlp.extractor.youtube import _video
    except ImportError:
        return
    orig = getattr(_video, "traverse_obj", None)
    if orig is None:
        return

    def traverse_obj(obj, *paths, **kwargs):
        res = orig(obj, *paths, **kwargs)
        if isinstance(res, list) and any(isinstance(p, tuple) and "translationLanguages" in p for p in paths):
            res = [x for x in res if isinstance(x, dict)
                   and str(x.get("languageCode", "")).split("-")[0].lower() in bases]
        return res

    _video.traverse_obj = traverse_obj


def _exit_code_from_waitstatus(status):
    if os.WIFEXITED(status):
        return os.WEXITSTATUS(status)
    if os.WIFSIGNALED(status):
        return 128 + os.WTERMSIG(status)
    return 1


def handle_stream(conn, req):
    args = req.get("args")
    if not isinstance(args, list) or not all(isinstance(a, str) for a in args):
        raise ValueError("invalid args")
    out_r, out_w = os.pipe()
    err_r, err_w = os.pipe()
    signal.signal(signal.SIGCHLD, signal.SIG_DFL)
    signal.signal(signal.SIGTERM, signal.SIG_DFL)
    pid = os.fork()
    if pid == 0:
        conn.close()
        os.close(out_r)
        os.close(err_r)
        devnull = os.open(os.devnull, os.O_RDONLY)
        if devnull != 0:
            os.dup2(devnull, 0)
            os.close(devnull)
        os.dup2(out_w, 1)
        os.dup2(err_w, 2)
        os.close(out_w)
        os.close(err_w)
        os.chdir(req.get("cwd") or "/")
        sys.argv = ["yt-dlp", *args]
        import yt_dlp

        try:
            limit_caption_translations(args)
        except Exception:  # noqa: BLE001
            pass
        code = 1
        try:
            yt_dlp.main()
            code = 0
        except SystemExit as e:
            code = e.code if isinstance(e.code, int) else (0 if e.code is None else 1)
        except BaseException as e:  # noqa: BLE001
            print(f"ytdl-server: {e!r}", file=sys.stderr)
            code = 1
        sys.stdout.flush()
        sys.stderr.flush()
        os._exit(code)

    os.close(out_w)
    os.close(err_w)
    os.set_blocking(out_r, False)
    os.set_blocking(err_r, False)

    def kill_child(*_args):
        try:
            os.kill(pid, signal.SIGTERM)
        except OSError:
            pass

    signal.signal(signal.SIGTERM, kill_child)
    client_alive = True
    try:
        while out_r is not None or err_r is not None:
            watch = [fd for fd in (out_r, err_r) if fd is not None]
            if client_alive:
                watch.append(conn)
            try:
                ready, _, _ = select.select(watch, [], [])
            except InterruptedError:
                continue
            if client_alive and conn in ready:
                try:
                    if not conn.recv(64):
                        client_alive = False
                        kill_child()
                        break
                except OSError:
                    client_alive = False
                    kill_child()
                    break
            for fd, tag in ((out_r, 1), (err_r, 2)):
                if fd is not None and fd in ready:
                    try:
                        chunk = os.read(fd, 16384)
                    except BlockingIOError:
                        continue
                    except OSError:
                        chunk = b""
                    if not chunk:
                        os.close(fd)
                        if tag == 1:
                            out_r = None
                        else:
                            err_r = None
                    else:
                        try:
                            conn.sendall(struct.pack("!BI", tag, len(chunk)) + chunk)
                        except OSError:
                            client_alive = False
                            kill_child()
                            break
            if not client_alive:
                break
    finally:
        for fd in (out_r, err_r):
            if fd is not None:
                try:
                    os.close(fd)
                except OSError:
                    pass
        _, status = os.waitpid(pid, 0)
        code = _exit_code_from_waitstatus(status)
        if client_alive:
            try:
                conn.sendall(struct.pack("!BI", 3, 4) + struct.pack("!i", code))
            except OSError:
                pass


def _is_http_url(url):
    return isinstance(url, str) and (url.startswith("https://") or url.startswith("http://"))


def _url_expire_ts(url):
    try:
        parsed = urlparse(url)
        q = parse_qs(parsed.query)
        if "expire" in q and q["expire"]:
            return int(q["expire"][0])
        m = re.search(r"/expire/(\d+)(?:/|$)", parsed.path)
        if m:
            return int(m.group(1))
    except (ValueError, TypeError):
        pass
    return None


def _codec_family(vcodec):
    if not vcodec or vcodec == "none":
        return ""
    if vcodec.startswith("av01"):
        return "av01"
    if vcodec.startswith("vp09") or vcodec == "vp9":
        return "vp09"
    if vcodec.startswith("avc1"):
        return "avc1"
    return vcodec.split(".", 1)[0]


def _compact_videos(formats, is_live):
    buckets = {}
    for f in formats or ():
        url = f.get("url")
        if not _is_http_url(url):
            continue
        vcodec = f.get("vcodec") or ""
        if not vcodec or vcodec == "none":
            continue
        proto = f.get("protocol") or ""
        fid = str(f.get("format_id") or "")
        note = (f.get("format_note") or "").lower()
        if proto == "mhtml" or fid.startswith("sb") or "storyboard" in note:
            continue
        height = int(f.get("height") or 0)
        fps = float(f.get("fps") or 0.0)
        acodec = f.get("acodec") or "none"
        dr = f.get("dynamic_range") or ""
        hdr = bool(dr and dr != "SDR")
        tbr = float(f.get("tbr") or f.get("vbr") or 0.0)
        avail = f.get("available_at")
        entry = {
            "url": url,
            "height": height,
            "fps": fps,
            "vcodec": vcodec,
            "acodec": acodec,
            "hdr": hdr,
            "protocol": proto,
            "tbr": tbr,
            "format_id": fid,
            "available_at": int(avail) if avail else None,
            "_headers": f.get("http_headers") or {},
        }
        fps_bucket = 30 if fps <= 30.5 else 60
        is_muxed = acodec not in ("", "none")
        key = (height, _codec_family(vcodec), hdr, fps_bucket, is_muxed)
        prev = buckets.get(key)
        if prev is None or entry["tbr"] > prev["tbr"]:
            buckets[key] = entry
    ranked = sorted(
        buckets.values(),
        key=lambda x: (
            1 if (is_live and x["acodec"] not in ("", "none")) or (not is_live and x["acodec"] in ("", "none")) else 0,
            x["height"],
            x["tbr"],
        ),
        reverse=True,
    )
    return ranked[:18]


def _compact_audios(formats):
    buckets = {}
    for f in formats or ():
        url = f.get("url")
        if not _is_http_url(url):
            continue
        if (f.get("vcodec") or "none") != "none":
            continue
        acodec = f.get("acodec") or ""
        if not acodec or acodec == "none":
            continue
        track = f.get("audio_track") if isinstance(f.get("audio_track"), dict) else {}
        lang = str(f.get("language") or track.get("language") or "")
        track_id = str(track.get("id") or "")
        note = str(f.get("format_note") or "")
        title = str(track.get("displayName") or note or lang)
        is_default = bool(
            track.get("audioIsDefault")
            or (f.get("language_preference") or 0) > 0
            or "original" in note.lower()
            or "default" in note.lower()
        )
        abr = float(f.get("abr") or f.get("tbr") or 0.0)
        avail = f.get("available_at")
        ac_fam = "opus" if "opus" in acodec else ("mp4a" if acodec.startswith("mp4a") else acodec.split(".", 1)[0])
        key = (lang, track_id, ac_fam)
        entry = {
            "url": url,
            "acodec": acodec,
            "abr": abr,
            "language": lang,
            "title": title,
            "is_default": is_default,
            "protocol": f.get("protocol") or "",
            "available_at": int(avail) if avail else None,
        }
        prev = buckets.get(key)
        if prev is None or (entry["is_default"], entry["abr"]) > (prev["is_default"], prev["abr"]):
            buckets[key] = entry
    ranked = sorted(buckets.values(), key=lambda x: (x["is_default"], x["abr"]), reverse=True)
    return ranked[:12]


def _sub_lang_matches(lang, bases):
    low = lang.lower()
    base = low.split("-", 1)[0]
    if base not in bases:
        return False
    if low == base or low == f"{base}-orig":
        return True
    suffix = lang[len(base) + 1 :]
    return bool(suffix) and suffix[0].isupper()


def _compact_subtitles(info, sub_langs):
    bases = {
        s.strip().split("-", 1)[0].lower()
        for s in (sub_langs or "").split(",")
        if s.strip() and re.fullmatch(r"[A-Za-z]{2,3}(?:-[A-Za-z0-9]+)*", s.strip())
    }
    if not bases:
        return []
    out = []
    seen = set()
    for auto, field in ((False, "subtitles"), (True, "automatic_captions")):
        tracks_by_lang = info.get(field) or {}
        if not isinstance(tracks_by_lang, dict):
            continue
        for lang, variants in tracks_by_lang.items():
            if not _sub_lang_matches(lang, bases) or not isinstance(variants, list):
                continue
            best = None
            best_rank = len(_SUB_EXT_ORDER) + 1
            for var in variants:
                if not isinstance(var, dict):
                    continue
                url = var.get("url")
                if not _is_http_url(url) or "tlang=" in url:
                    continue
                ext = str(var.get("ext") or "").lower()
                rank = _SUB_EXT_ORDER.index(ext) if ext in _SUB_EXT_ORDER else len(_SUB_EXT_ORDER)
                if rank < best_rank:
                    best_rank = rank
                    best = (url, str(var.get("name") or lang))
            if best and (lang, auto) not in seen:
                seen.add((lang, auto))
                out.append({"lang": lang, "name": best[1], "url": best[0], "auto": auto})
    return out


def _compact_chapters(info):
    out = []
    for c in info.get("chapters") or ():
        if not isinstance(c, dict):
            continue
        title = str(c.get("title") or "").strip()
        start = c.get("start_time")
        if title and isinstance(start, (int, float)) and start >= 0:
            out.append({"start_time": float(start), "title": title})
    return out


def _has_adequate_formats(info, is_live):
    fmts = info.get("formats") or []
    if is_live:
        return any(_is_http_url(f.get("url")) and (f.get("vcodec") or "none") != "none" for f in fmts)
    return any(
        _is_http_url(f.get("url"))
        and (f.get("vcodec") or "none") != "none"
        and ((f.get("acodec") or "none") == "none" or (f.get("height") or 0) > 360)
        for f in fmts
    )


class _QuietLogger:
    def __init__(self):
        self.last_error = ""

    def debug(self, msg):
        pass

    def warning(self, msg):
        pass

    def error(self, msg):
        self.last_error = str(msg)


def _get_ydl(state, fast, is_live):
    key = (fast, is_live)
    entry = state["ydl"].get(key)
    if entry is not None:
        return entry
    yt_args = {"skip": ["translated_subs"]}
    if fast:
        yt_args = {
            "skip": ["translated_subs"] if is_live else ["translated_subs", "dash"],
            "player_skip": ["webpage", "configs"],
        }
    logger = _QuietLogger()
    opts = {
        "quiet": True,
        "no_warnings": True,
        "no_color": True,
        "skip_download": True,
        "noplaylist": True,
        "noprogress": True,
        "logger": logger,
        "extractor_args": {"youtube": yt_args},
    }
    if state.get("js_runtimes"):
        opts["js_runtimes"] = state["js_runtimes"]
    ydl = state["yt_dlp"].YoutubeDL(opts)
    state["ydl"][key] = (ydl, logger)
    return ydl, logger


def _cache_valid(entry, mono_now, wall_now):
    if mono_now - entry["mono"] > RESOLVE_CACHE_TTL:
        return False
    stream = entry["stream"]
    for item in stream.get("videos", ()) + stream.get("audios", ()):
        exp = _url_expire_ts(item.get("url", ""))
        if exp is not None and wall_now + 60 >= exp:
            return False
    return True


def _resolve_video(video_id, sub_langs, is_live, state):
    if not isinstance(video_id, str) or not _VIDEO_ID_RE.fullmatch(video_id):
        raise ValueError("invalid video_id")
    mono_now = time.monotonic()
    wall_now = time.time()
    cache = state["cache"]
    cache_key = (video_id, sub_langs)
    cached = cache.get(cache_key)
    if cached is not None:
        if _cache_valid(cached, mono_now, wall_now):
            cache.move_to_end(cache_key)
            return cached["stream"]
        del cache[cache_key]

    url = f"https://www.youtube.com/watch?v={video_id}"
    info = None
    last_err = None
    fast_ydl, fast_log = _get_ydl(state, True, is_live)
    fast_log.last_error = ""
    try:
        info = fast_ydl.extract_info(url, download=False)
    except Exception as e:  # noqa: BLE001
        last_err = e
        info = None
    if not info or not _has_adequate_formats(info, bool(info.get("is_live") or is_live)):
        full_ydl, full_log = _get_ydl(state, False, is_live)
        full_log.last_error = ""
        try:
            info = full_ydl.extract_info(url, download=False)
        except Exception as e:  # noqa: BLE001
            msg = full_log.last_error or fast_log.last_error
            if msg:
                raise ValueError(msg) from e
            raise last_err or e from e
    if not isinstance(info, dict):
        raise ValueError("no video info")

    actual_live = bool(info.get("is_live") or is_live)
    videos = _compact_videos(info.get("formats"), actual_live)
    if not videos:
        raise ValueError("no playable video formats")
    audios = _compact_audios(info.get("formats"))
    raw_headers = videos[0].pop("_headers", None) or info.get("http_headers") or {}
    for v in videos[1:]:
        v.pop("_headers", None)
    user_agent = str(raw_headers.get("User-Agent") or "")
    headers = {
        str(k): str(v)
        for k, v in raw_headers.items()
        if str(k).lower() not in ("user-agent", "accept-encoding") and v
    }
    duration = info.get("duration")
    stream = {
        "video_id": video_id,
        "title": str(info.get("title") or ""),
        "is_live": actual_live,
        "duration": float(duration) if isinstance(duration, (int, float)) and duration > 0 else None,
        "user_agent": user_agent,
        "headers": headers,
        "videos": videos,
        "audios": audios,
        "subtitles": _compact_subtitles(info, sub_langs),
        "chapters": _compact_chapters(info),
    }
    cache[cache_key] = {"mono": mono_now, "stream": stream}
    cache.move_to_end(cache_key)
    while len(cache) > RESOLVE_CACHE_MAX:
        cache.popitem(last=False)
    return stream


def handle_resolve(conn, req, state):
    try:
        video_id = req.get("video_id") or ""
        sub_langs = str(req.get("sub_langs") or "")
        is_live = bool(req.get("is_live", False))
        stream = _resolve_video(video_id, sub_langs, is_live, state)
        resp = {"ok": True, "stream": stream}
    except Exception as e:  # noqa: BLE001
        resp = {"ok": False, "error": str(e)}
    try:
        conn.sendall((json.dumps(resp) + "\n").encode())
    except OSError:
        pass


def handle(conn):
    conn.setblocking(True)
    conn.settimeout(None)
    try:
        req, fds = recv_request(conn)
    except BaseException:  # noqa: BLE001
        try:
            conn.close()
        except OSError:
            pass
        os._exit(0)
    handle_parsed(conn, req, fds)


def handle_parsed(conn, req, fds):
    code = 1
    legacy_mode = False
    try:
        conn.setblocking(True)
        conn.settimeout(None)
        if not fds and req.get("mode") == "stream":
            handle_stream(conn, req)
            os._exit(0)
        if len(fds) != 3:
            raise ValueError("expected stdin, stdout and stderr")
        legacy_mode = True
        for fd, target in zip(fds, (0, 1, 2)):
            os.dup2(fd, target)
            os.close(fd)
        os.chdir(req.get("cwd") or "/")
        sys.argv = ["yt-dlp", *req["args"]]
        signal.signal(signal.SIGCHLD, signal.SIG_DFL)  # yt-dlp waits for its own children
        signal.signal(signal.SIGTERM, signal.SIG_DFL)
        # the client going away (mpv gave up) ends this request too
        import threading

        def watch():
            try:
                conn.recv(1)
            except OSError:
                pass
            os.kill(os.getpid(), signal.SIGTERM)

        threading.Thread(target=watch, daemon=True).start()
        import yt_dlp

        try:
            limit_caption_translations(req["args"])
        except Exception:  # noqa: BLE001
            pass
        try:
            yt_dlp.main()
            code = 0
        except SystemExit as e:
            code = e.code if isinstance(e.code, int) else (0 if e.code is None else 1)
        except BaseException as e:  # noqa: BLE001
            print(f"ytdl-server: {e!r}", file=sys.stderr)
            code = 1
        sys.stdout.flush()
        sys.stderr.flush()
    except BaseException as e:  # noqa: BLE001
        if legacy_mode:
            try:
                os.write(2, f"ytdl-server: {e!r}\n".encode())
            except OSError:
                pass
    if legacy_mode:
        try:
            conn.sendall(struct.pack("!i", code))
        except OSError:
            pass
    try:
        conn.close()
    except OSError:
        pass
    os._exit(0)


if __name__ == "__main__":
    if len(sys.argv) < 4 or sys.argv[1] not in ("serve", "run"):
        sys.exit(__doc__)
    if sys.argv[1] == "serve":
        serve(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else None)
    else:
        run(sys.argv[2], sys.argv[3], sys.argv[4:])
