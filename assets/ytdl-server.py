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
import json
import os
import signal
import socket
import struct
import sys

# Preprocessed players kept (about 4 MB each; YouTube changes it every few days).
KEEP_PLAYERS = 2
MAX_REQUEST = 1 << 20


def fallback(ytdlp_dir, args):
    os.execvp("python3", ["python3", ytdlp_dir, *args])


def recv_request(conn):
    fds = array.array("i")
    data, anc, _flags, _addr = conn.recvmsg(MAX_REQUEST, socket.CMSG_LEN(3 * fds.itemsize))
    for level, kind, payload in anc:
        if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
            fds.frombytes(payload[: len(payload) - (len(payload) % fds.itemsize)])
    return json.loads(data), list(fds)


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
    """yt-dlp's Deno script, carrying the request for the resident Deno."""

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
    if json.loads(text).get("type") != "result":
        raise OSError(text[:300])
    return text


def start_jsc(deno, sock_path):
    """A Deno that stays running with yt-dlp's challenge solver loaded (see
    ytdl-jsc.js), and yt-dlp's Deno provider sending its work there. On any
    problem yt-dlp runs its own Deno, as usual."""
    try:
        import yt_dlp_ejs.yt.solver as ejs_solver
        from yt_dlp.extractor.youtube.jsc._builtin import deno as deno_jcp
        from yt_dlp.extractor.youtube.jsc._builtin import ejs
    except ImportError:
        return None
    here = os.path.dirname(os.path.abspath(__file__))
    with open(os.path.join(here, "ytdl-jsc.js"), encoding="utf-8") as f:
        daemon = f.read()
    script = sock_path + ".js"
    with open(script, "w", encoding="utf-8") as f:
        f.write(f"{ejs_solver.lib()}\nObject.assign(globalThis, lib);\n{ejs_solver.core()}\n{daemon}")
    try:
        os.unlink(sock_path)
    except FileNotFoundError:
        pass
    import subprocess

    proc = subprocess.Popen(
        [deno, "run", "--no-prompt", "--no-remote", "--no-npm", "--no-lock", "--no-config", "--cached-only",
         f"--allow-read={sock_path}", f"--allow-write={sock_path}", f"--allow-net=unix:{sock_path}",
         script, sock_path],
        stdin=subprocess.PIPE, stdout=subprocess.DEVNULL)

    orig_construct = ejs.EJSBaseJCP._construct_stdin
    orig_run = deno_jcp.DenoJCP._run_js_runtime

    def construct(self, player, preprocessed, requests, /):
        out = _Stdin(orig_construct(self, player, preprocessed, requests))
        reqs = [{"type": r.type.value, "challenges": r.input.challenges} for r in requests]
        out.data = {"key": requests[0].input.player_url if requests else "",
                    "type": "preprocessed", "preprocessed_player": player, "requests": reqs} if preprocessed else {
                    "key": requests[0].input.player_url if requests else "",
                    "type": "player", "player": player, "requests": reqs, "output_preprocessed": True}
        return out

    def run_js(self, stdin, /):
        data = getattr(stdin, "data", None)
        if data is not None and os.path.exists(sock_path):
            try:
                return jsc_request(sock_path, data)
            except (OSError, ValueError) as e:
                self.logger.debug(f"resident deno failed ({e}); running deno")
        return orig_run(self, stdin)

    ejs.EJSBaseJCP._construct_stdin = construct
    deno_jcp.DenoJCP._run_js_runtime = run_js
    return proc


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

    parent = os.getppid()
    try:
        os.unlink(sock_path)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(sock_path)
    os.chmod(sock_path, 0o600)
    srv.listen(8)
    srv.settimeout(5)
    signal.signal(signal.SIGCHLD, signal.SIG_IGN)  # no zombies
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    try:
        while os.getppid() == parent:
            try:
                conn, _ = srv.accept()
            except socket.timeout:
                continue
            if cache_dir:
                prune_players(cache_dir)
            if os.fork() == 0:
                srv.close()
                handle(conn)  # never returns
            conn.close()
    finally:
        if jsc_proc:
            jsc_proc.kill()
        for p in (sock_path, sock_path + ".jsc", sock_path + ".jsc.js"):
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


def handle(conn):
    code = 1
    try:
        conn.settimeout(None)
        req, fds = recv_request(conn)
        if len(fds) != 3:
            raise ValueError("expected stdin, stdout and stderr")
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
        try:
            os.write(2, f"ytdl-server: {e!r}\n".encode())
        except OSError:
            pass
    try:
        conn.sendall(struct.pack("!i", code))
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
