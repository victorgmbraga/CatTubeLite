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
import select
import signal
import socket
import stat
import struct
import sys

# Preprocessed players kept (about 4 MB each; YouTube changes it every few days).
KEEP_PLAYERS = 2
MAX_REQUEST = 1 << 20


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
    try:
        os.unlink(sock_path)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(sock_path)
    os.chmod(sock_path, 0o600)
    srv.listen(8)
    srv.setblocking(False)
    signal.signal(signal.SIGCHLD, signal.SIG_IGN)  # no zombies
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    try:
        while os.getppid() == parent:
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
            if os.fork() == 0:
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
                handle(conn)  # never returns
            conn.close()
    finally:
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


def handle(conn):
    code = 1
    legacy_mode = False
    try:
        conn.setblocking(True)
        conn.settimeout(None)
        req, fds = recv_request(conn)
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
