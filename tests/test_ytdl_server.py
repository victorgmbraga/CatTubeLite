#!/usr/bin/env python3
"""Tests for assets/ytdl-server.py and assets/ytdl-jsc.js."""

import array
import importlib.util
import json
import os
import shutil
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import threading
import time
import types
import unittest

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SERVER_PATH = os.path.join(REPO_ROOT, "assets", "ytdl-server.py")
JSC_PATH = os.path.join(REPO_ROOT, "assets", "ytdl-jsc.js")
QJS_PATH = os.path.join(REPO_ROOT, "assets", "ytdl-jsc-qjs.js")


def load_server_module():
    spec = importlib.util.spec_from_file_location("ytdl_server", SERVER_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def make_fake_ytdlp(root):
    pkg = os.path.join(root, "yt_dlp")
    ext = os.path.join(pkg, "extractor", "youtube")
    os.makedirs(ext, exist_ok=True)
    with open(os.path.join(root, "__main__.py"), "w", encoding="utf-8") as f:
        f.write("import yt_dlp; yt_dlp.main()\n")
    with open(os.path.join(pkg, "__init__.py"), "w", encoding="utf-8") as f:
        f.write(
            "import json, os, sys\n"
            "LOG_PATH = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'extract_calls.jsonl')\n"
            "class YoutubeDL:\n"
            "    def __init__(self, opts=None): self.opts = opts or {}\n"
            "    def extract_info(self, url, download=False):\n"
            "        yt_args = (self.opts.get('extractor_args') or {}).get('youtube') or {}\n"
            "        with open(LOG_PATH, 'a', encoding='utf-8') as lf:\n"
            "            lf.write(json.dumps({'url': url, 'player_skip': yt_args.get('player_skip'), 'skip': yt_args.get('skip')}) + '\\n')\n"
            "        if 'retry360vid' in url:\n"
            "            if yt_args.get('player_skip'):\n"
            "                return {\n"
            "                    'title': 'Retry 360', 'is_live': False, 'duration': 90,\n"
            "                    'http_headers': {'User-Agent': 'UA/Retry'},\n"
            "                    'formats': [{'url': 'https://cdn.example/360.mp4', 'height': 360, 'fps': 30, 'vcodec': 'avc1.42001E', 'acodec': 'mp4a.40.2', 'protocol': 'https', 'tbr': 500, 'format_id': '18'}],\n"
            "                }\n"
            "            return {\n"
            "                'title': 'Retry 1080', 'is_live': False, 'duration': 90,\n"
            "                'http_headers': {'User-Agent': 'UA/Full'},\n"
            "                'formats': [\n"
            "                    {'url': 'https://cdn.example/1080.mp4', 'height': 1080, 'fps': 30, 'vcodec': 'avc1.640028', 'acodec': 'none', 'protocol': 'https', 'tbr': 3200, 'format_id': '137'},\n"
            "                    {'url': 'https://cdn.example/audio.webm', 'vcodec': 'none', 'acodec': 'opus', 'abr': 128, 'language': 'en', 'format_note': 'Original', 'protocol': 'https'},\n"
            "                ],\n"
            "            }\n"
            "        return {\n"
            "            'title': 'Sample Video', 'is_live': False, 'duration': 200,\n"
            "            'http_headers': {'User-Agent': 'UA/Test', 'Referer': 'https://www.youtube.com/'},\n"
            "            'formats': [\n"
            "                {'url': 'https://cdn.example/1080.mp4', 'height': 1080, 'fps': 30, 'vcodec': 'avc1.640028', 'acodec': 'none', 'protocol': 'https', 'tbr': 3200, 'format_id': '137'},\n"
            "                {'url': 'https://cdn.example/360.mp4', 'height': 360, 'fps': 30, 'vcodec': 'avc1.42001E', 'acodec': 'mp4a.40.2', 'protocol': 'https', 'tbr': 500, 'format_id': '18'},\n"
            "                {'url': 'https://cdn.example/audio.webm', 'vcodec': 'none', 'acodec': 'opus', 'abr': 128, 'language': 'en', 'format_note': 'Default', 'protocol': 'https'},\n"
            "            ],\n"
            "            'subtitles': {'en': [{'ext': 'vtt', 'url': 'https://www.youtube.com/api/timedtext?lang=en&fmt=vtt', 'name': 'English'}], 'fr': [{'ext': 'vtt', 'url': 'https://www.youtube.com/api/timedtext?lang=fr', 'name': 'French'}]},\n"
            "            'automatic_captions': {'en': [{'ext': 'vtt', 'url': 'https://www.youtube.com/api/timedtext?lang=en&kind=asr&fmt=vtt', 'name': 'English (auto)'}], 'en-pt': [{'ext': 'vtt', 'url': 'https://www.youtube.com/api/timedtext?lang=pt&tlang=en', 'name': 'Translated'}]},\n"
            "            'chapters': [{'start_time': 0, 'title': 'Intro'}, {'start_time': 60, 'title': 'Body'}],\n"
            "        }\n"
            "def main():\n"
            "    args = sys.argv[1:]\n"
            "    sys.stdout.write('OUT:' + ' '.join(args) + '\\n')\n"
            "    sys.stderr.write('ERR:ok\\n')\n"
            "    sys.exit(7 if '--fail' in args else 0)\n"
        )
    with open(os.path.join(pkg, "cache.py"), "w", encoding="utf-8") as f:
        f.write("class Cache:\n    def __init__(self, ydl): pass\n    def _get_root_dir(self): return '/nonexistent'\n")
    with open(os.path.join(pkg, "extractor", "__init__.py"), "w", encoding="utf-8") as f:
        f.write("")
    with open(os.path.join(ext, "__init__.py"), "w", encoding="utf-8") as f:
        f.write("")
    with open(os.path.join(ext, "_video.py"), "w", encoding="utf-8") as f:
        f.write("def traverse_obj(obj, *paths, **kwargs): return obj\n")


class YtdlServerTests(unittest.TestCase):
    def test_serve_stream_mode_and_clean_stdin_eof_shutdown(self):
        with tempfile.TemporaryDirectory() as tmp:
            ytdlp_dir = os.path.join(tmp, "yt-dlp.d")
            make_fake_ytdlp(ytdlp_dir)
            sock_path = os.path.join(tmp, "ytdl.sock")

            proc = subprocess.Popen(
                [sys.executable, "-I", SERVER_PATH, "serve", sock_path, ytdlp_dir],
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
            )
            try:
                deadline = time.monotonic() + 5.0
                while not os.path.exists(sock_path) and time.monotonic() < deadline:
                    time.sleep(0.02)
                if not os.path.exists(sock_path):
                    self.fail(proc.stderr.read().decode())

                # Stream protocol request
                s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                s.connect(sock_path)
                req = json.dumps({"mode": "stream", "args": ["--hello", "--fail"], "cwd": tmp}) + "\n"
                s.sendall(req.encode())
                stdout_buf = bytearray()
                stderr_buf = bytearray()
                exit_code = None
                while True:
                    hdr = s.recv(5)
                    while len(hdr) < 5:
                        more = s.recv(5 - len(hdr))
                        if not more:
                            break
                        hdr += more
                    self.assertEqual(len(hdr), 5)
                    tag, length = struct.unpack("!BI", hdr)
                    payload = bytearray()
                    while len(payload) < length:
                        part = s.recv(length - len(payload))
                        if not part:
                            break
                        payload += part
                    self.assertEqual(len(payload), length)
                    if tag == 1:
                        stdout_buf += payload
                    elif tag == 2:
                        stderr_buf += payload
                    elif tag == 3:
                        exit_code = struct.unpack("!i", payload)[0]
                        break
                s.close()
                self.assertEqual(stdout_buf.decode(), "OUT:--hello --fail\n")
                self.assertEqual(stderr_buf.decode(), "ERR:ok\n")
                self.assertEqual(exit_code, 7)

                # Legacy SCM_RIGHTS run mode still works
                r_out, w_out = os.pipe()
                r_err, w_err = os.pipe()
                r_in, w_in = os.pipe()
                os.close(w_in)
                s2 = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                s2.connect(sock_path)
                msg = json.dumps({"args": ["--legacy"], "cwd": tmp}).encode()
                s2.sendmsg([msg], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", [r_in, w_out, w_err]))])
                os.close(r_in)
                os.close(w_out)
                os.close(w_err)
                code_bytes = s2.recv(4)
                s2.close()
                self.assertEqual(struct.unpack("!i", code_bytes)[0], 0)
                self.assertEqual(os.read(r_out, 1024).decode(), "OUT:--legacy\n")
                self.assertEqual(os.read(r_err, 1024).decode(), "ERR:ok\n")
                os.close(r_out)
                os.close(r_err)

                # Closing stdin shuts down cleanly and unlinks socket
                proc.stdin.close()
                proc.stdin = None
                rc = proc.wait(timeout=3.0)
                self.assertEqual(rc, 0)
                self.assertFalse(os.path.exists(sock_path))
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.wait(timeout=2)
                if proc.stderr:
                    proc.stderr.close()

    def test_start_jsc_cache_first_probe_and_fallback(self):
        mod = load_server_module()
        with tempfile.TemporaryDirectory() as tmp:
            # Install fake yt_dlp + yt_dlp_ejs modules in sys.modules
            saved_mods = {}
            fake_names = [
                "yt_dlp_ejs",
                "yt_dlp_ejs.yt",
                "yt_dlp_ejs.yt.solver",
                "yt_dlp",
                "yt_dlp.extractor",
                "yt_dlp.extractor.youtube",
                "yt_dlp.extractor.youtube.jsc",
                "yt_dlp.extractor.youtube.jsc._builtin",
                "yt_dlp.extractor.youtube.jsc._builtin.ejs",
                "yt_dlp.extractor.youtube.jsc._builtin.deno",
            ]
            for name in fake_names:
                saved_mods[name] = sys.modules.get(name)

            orig_construct_calls = []
            orig_run_calls = []

            try:
                ejs_pkg = types.ModuleType("yt_dlp_ejs")
                ejs_yt = types.ModuleType("yt_dlp_ejs.yt")
                ejs_solver = types.ModuleType("yt_dlp_ejs.yt.solver")
                ejs_solver.lib = lambda: "const lib = { marker: true };"
                ejs_solver.core = lambda: "function jsc(i) { return { type: 'result', preprocessed_player: 'p', responses: [] }; }"

                class FakeEJSBaseJCP:
                    def _construct_stdin(self, player, preprocessed, requests, /):
                        orig_construct_calls.append((player, preprocessed))
                        return f"REAL:{player}"

                class FakeDenoJCP(FakeEJSBaseJCP):
                    def __init__(self):
                        self.logger = types.SimpleNamespace(debug=lambda *_a: None)

                    def _run_js_runtime(self, stdin, /):
                        orig_run_calls.append(stdin)
                        return '{"type":"result","responses":[]}'

                ejs_mod = types.ModuleType("yt_dlp.extractor.youtube.jsc._builtin.ejs")
                ejs_mod.EJSBaseJCP = FakeEJSBaseJCP
                deno_mod = types.ModuleType("yt_dlp.extractor.youtube.jsc._builtin.deno")
                deno_mod.DenoJCP = FakeDenoJCP

                sys.modules["yt_dlp_ejs"] = ejs_pkg
                sys.modules["yt_dlp_ejs.yt"] = ejs_yt
                sys.modules["yt_dlp_ejs.yt.solver"] = ejs_solver
                sys.modules["yt_dlp"] = types.ModuleType("yt_dlp")
                sys.modules["yt_dlp.extractor"] = types.ModuleType("yt_dlp.extractor")
                sys.modules["yt_dlp.extractor.youtube"] = types.ModuleType("yt_dlp.extractor.youtube")
                sys.modules["yt_dlp.extractor.youtube.jsc"] = types.ModuleType("yt_dlp.extractor.youtube.jsc")
                sys.modules["yt_dlp.extractor.youtube.jsc._builtin"] = types.ModuleType("yt_dlp.extractor.youtube.jsc._builtin")
                sys.modules["yt_dlp.extractor.youtube.jsc._builtin.ejs"] = ejs_mod
                sys.modules["yt_dlp.extractor.youtube.jsc._builtin.deno"] = deno_mod

                # Create dummy executable for Popen
                fake_deno = os.path.join(tmp, "fake-deno")
                with open(fake_deno, "w", encoding="utf-8") as f:
                    f.write("#!/usr/bin/env python3\nimport sys\nsys.stdin.read()\n")
                os.chmod(fake_deno, 0o755)

                jsc_sock = os.path.join(tmp, "test.jsc")
                seen_requests = []
                cache = set()
                ready_srv = threading.Event()
                stop_srv = threading.Event()
                srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)

                def jsc_server():
                    srv.bind(jsc_sock)
                    srv.listen(4)
                    srv.settimeout(0.05)
                    ready_srv.set()
                    while not stop_srv.is_set():
                        try:
                            conn, _ = srv.accept()
                        except socket.timeout:
                            continue
                        chunks = []
                        while p := conn.recv(65536):
                            chunks.append(p)
                        req = json.loads(b"".join(chunks))
                        seen_requests.append(req)
                        if req["type"] == "cached":
                            if req["key"] in cache:
                                resp = {"type": "result", "responses": [{"type": "result", "data": {"abc": "xyz"}}]}
                            else:
                                resp = {"type": "miss"}
                        else:
                            cache.add(req["key"])
                            resp = {"type": "result", "responses": [{"type": "result", "data": {"abc": "xyz"}}]}
                        conn.sendall(json.dumps(resp).encode())
                        conn.close()

                proc = mod.start_jsc(fake_deno, jsc_sock)
                self.assertIsNotNone(proc)
                try:
                    # Check private 0o600 permissions on generated JS files
                    self.assertEqual(stat.S_IMODE(os.stat(jsc_sock + ".js").st_mode), 0o600)
                    self.assertEqual(stat.S_IMODE(os.stat(jsc_sock + ".lib.js").st_mode), 0o600)

                    t = threading.Thread(target=jsc_server, daemon=True)
                    t.start()
                    self.assertTrue(ready_srv.wait(timeout=2.0))

                    # Ensure EJSBaseJCP._construct_stdin was NOT patched globally
                    self.assertEqual(FakeEJSBaseJCP()._construct_stdin("P", True, []), "REAL:P")
                    self.assertEqual(len(orig_construct_calls), 1)

                    req_obj = types.SimpleNamespace(
                        type=types.SimpleNamespace(value="n"),
                        input=types.SimpleNamespace(player_url="player-v1", challenges=["abc"]),
                    )
                    jcp = FakeDenoJCP()

                    # 1st call: miss -> sends preprocessed_player, never calls orig_construct
                    stdin1 = jcp._construct_stdin("BIG_PREPROCESSED_4MB", True, [req_obj])
                    self.assertEqual(len(orig_construct_calls), 1)
                    res1 = json.loads(jcp._run_js_runtime(stdin1))
                    self.assertEqual(res1["type"], "result")
                    self.assertEqual([r["type"] for r in seen_requests], ["cached", "preprocessed"])
                    self.assertEqual(len(orig_run_calls), 0)

                    # 2nd call: cache hit -> ONLY sends tiny cached probe!
                    stdin2 = jcp._construct_stdin("BIG_PREPROCESSED_4MB", True, [req_obj])
                    res2 = json.loads(jcp._run_js_runtime(stdin2))
                    self.assertEqual(res2["type"], "result")
                    self.assertEqual([r["type"] for r in seen_requests], ["cached", "preprocessed", "cached"])
                    self.assertEqual(len(orig_construct_calls), 1)
                    self.assertEqual(len(orig_run_calls), 0)

                    # 3rd call when socket disappears -> materializes real stdin and falls back to orig_run
                    stop_srv.set()
                    t.join(timeout=2)
                    srv.close()
                    os.unlink(jsc_sock)
                    stdin3 = jcp._construct_stdin("BIG_PREPROCESSED_4MB", True, [req_obj])
                    jcp._run_js_runtime(stdin3)
                    self.assertEqual(orig_run_calls, ["REAL:BIG_PREPROCESSED_4MB"])
                finally:
                    stop_srv.set()
                    if proc.stdin:
                        proc.stdin.close()
                    proc.terminate()
                    proc.wait(timeout=2)
            finally:
                for name, prev in saved_mods.items():
                    if prev is None:
                        sys.modules.pop(name, None)
                    else:
                        sys.modules[name] = prev

    @unittest.skipUnless(shutil.which("node"), "node required to exercise ytdl-jsc.js harness")
    def test_ytdl_jsc_lazy_lib_load_and_cache_protocol(self):
        with tempfile.TemporaryDirectory() as tmp:
            lib_path = os.path.join(tmp, "lib.js")
            with open(lib_path, "w", encoding="utf-8") as f:
                f.write(
                    "globalThis.__libLoaded = (globalThis.__libLoaded || 0) + 1;\n"
                    "function jsc(input) {\n"
                    "  return {\n"
                    "    type: 'result',\n"
                    "    preprocessed_player: '_result.n = (x) => \"n:\" + x;',\n"
                    "    responses: input.requests.map(r => ({ type: 'result', data: Object.fromEntries(r.challenges.map(c => [c, 'n:' + c])) }))\n"
                    "  };\n"
                    "}\n"
                    "globalThis.__cattube_jsc = jsc;\n"
                )
            with open(JSC_PATH, "r", encoding="utf-8") as f:
                src = f.read()
            # Slice before Deno-specific I/O tail and drive solve() directly under Node.
            core_js = src.split("async function readAll", 1)[0]
            harness = os.path.join(tmp, "harness.js")
            with open(harness, "w", encoding="utf-8") as f:
                f.write(
                    "const fs = require('fs');\n"
                    "const assert = require('assert');\n"
                    f"globalThis.Deno = {{ args: ['unused.sock', {json.dumps(lib_path)}], readTextFileSync: (p) => fs.readFileSync(p, 'utf8') }};\n"
                    f"{core_js}\n"
                    "// 1. cached miss\n"
                    "assert.deepStrictEqual(solve({ type: 'cached', key: 'k1', requests: [{ type: 'n', challenges: ['a'] }] }), { type: 'miss' });\n"
                    "assert.strictEqual(globalThis.__libLoaded, undefined);\n"
                    "// 2. preprocessed never loads heavy lib\n"
                    "const r1 = solve({ type: 'preprocessed', key: 'k1', preprocessed_player: '_result.n = (x) => \"p:\" + x;', requests: [{ type: 'n', challenges: ['a'] }] });\n"
                    "assert.deepStrictEqual(r1, { type: 'result', responses: [{ type: 'result', data: { a: 'p:a' } }] });\n"
                    "assert.strictEqual(globalThis.__libLoaded, undefined);\n"
                    "// 3. cached hit reuses compiled solver\n"
                    "const r2 = solve({ type: 'cached', key: 'k1', requests: [{ type: 'n', challenges: ['b'] }] });\n"
                    "assert.deepStrictEqual(r2, { type: 'result', responses: [{ type: 'result', data: { b: 'p:b' } }] });\n"
                    "assert.strictEqual(globalThis.__libLoaded, undefined);\n"
                    "// 4. raw player loads lib lazily once\n"
                    "const r3 = solve({ type: 'player', key: 'k2', player: 'raw', requests: [{ type: 'n', challenges: ['c'] }] });\n"
                    "assert.strictEqual(globalThis.__libLoaded, 1);\n"
                    "assert.deepStrictEqual(r3.responses, [{ type: 'result', data: { c: 'n:c' } }]);\n"
                    "const r4 = solve({ type: 'cached', key: 'k2', requests: [{ type: 'n', challenges: ['d'] }] });\n"
                    "assert.deepStrictEqual(r4, { type: 'result', responses: [{ type: 'result', data: { d: 'n:d' } }] });\n"
                    "assert.strictEqual(globalThis.__libLoaded, 1);\n"
                )
            subprocess.run([shutil.which("node"), harness], check=True)

    def test_serve_resolve_mode_cache_retry_and_validation(self):
        def resolve_req(sock_path, payload):
            s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            try:
                s.connect(sock_path)
                s.sendall((json.dumps(payload) + "\n").encode())
                chunks = []
                while part := s.recv(65536):
                    chunks.append(part)
                return json.loads(b"".join(chunks))
            finally:
                s.close()

        with tempfile.TemporaryDirectory() as tmp:
            ytdlp_dir = os.path.join(tmp, "yt-dlp.d")
            make_fake_ytdlp(ytdlp_dir)
            sock_path = os.path.join(tmp, "ytdl.sock")
            log_path = os.path.join(ytdlp_dir, "extract_calls.jsonl")

            proc = subprocess.Popen(
                [sys.executable, "-I", SERVER_PATH, "serve", sock_path, ytdlp_dir],
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
            )
            try:
                deadline = time.monotonic() + 5.0
                while not os.path.exists(sock_path) and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertTrue(os.path.exists(sock_path))

                # 1. Resolve once -> uses player_skip fast path, filters tlang= & non-matching langs
                r1 = resolve_req(sock_path, {"mode": "resolve", "video_id": "WhWc3b3KhnY", "sub_langs": "en", "is_live": False})
                self.assertTrue(r1["ok"], r1)
                stream = r1["stream"]
                self.assertEqual(stream["title"], "Sample Video")
                self.assertEqual([v["height"] for v in stream["videos"]], [1080, 360])
                self.assertEqual(len(stream["audios"]), 1)
                self.assertEqual(len(stream["chapters"]), 2)
                sub_urls = [sub["url"] for sub in stream["subtitles"]]
                self.assertFalse(any("tlang=" in u for u in sub_urls), sub_urls)
                self.assertEqual({sub["lang"] for sub in stream["subtitles"]}, {"en"})

                # 2. Second identical request hits RESOLVE_CACHE (extract_info not called again)
                r2 = resolve_req(sock_path, {"mode": "resolve", "video_id": "WhWc3b3KhnY", "sub_langs": "en", "is_live": False})
                self.assertTrue(r2["ok"])
                self.assertEqual(r2["stream"]["videos"][0]["url"], stream["videos"][0]["url"])
                with open(log_path, "r", encoding="utf-8") as f:
                    calls = [json.loads(line) for line in f]
                self.assertEqual(len(calls), 1)
                self.assertEqual(calls[0]["player_skip"], ["webpage", "configs"])

                # 3. 360p-only fast-path triggers automatic retry without player_skip
                r3 = resolve_req(sock_path, {"mode": "resolve", "video_id": "retry360vid", "sub_langs": "", "is_live": False})
                self.assertTrue(r3["ok"], r3)
                self.assertEqual([v["height"] for v in r3["stream"]["videos"]], [1080])
                with open(log_path, "r", encoding="utf-8") as f:
                    calls = [json.loads(line) for line in f]
                self.assertEqual(len(calls), 3)
                self.assertIsNotNone(calls[1]["player_skip"])
                self.assertIsNone(calls[2]["player_skip"])

                # 4. Invalid video_id returns ok=false error
                r4 = resolve_req(sock_path, {"mode": "resolve", "video_id": "../bad", "sub_langs": "", "is_live": False})
                self.assertEqual(r4, {"ok": False, "error": "invalid video_id"})

                proc.stdin.close()
                proc.stdin = None
                self.assertEqual(proc.wait(timeout=5), 0)
            finally:
                if proc.stdin:
                    proc.stdin.close()
                if proc.stderr:
                    proc.stderr.close()
                if proc.poll() is None:
                    proc.kill()
                    proc.wait(timeout=2)

    def test_start_jsc_qjs_cache_probe_and_fork_guard(self):
        mod = load_server_module()
        with tempfile.TemporaryDirectory() as tmp:
            saved_mods = {}
            fake_names = [
                "yt_dlp_ejs",
                "yt_dlp_ejs.yt",
                "yt_dlp_ejs.yt.solver",
                "yt_dlp",
                "yt_dlp.extractor",
                "yt_dlp.extractor.youtube",
                "yt_dlp.extractor.youtube.jsc",
                "yt_dlp.extractor.youtube.jsc._builtin",
                "yt_dlp.extractor.youtube.jsc._builtin.ejs",
                "yt_dlp.extractor.youtube.jsc._builtin.quickjs",
            ]
            for name in fake_names:
                saved_mods[name] = sys.modules.get(name)

            orig_construct_calls = []
            orig_run_calls = []
            orig_getpid = mod.os.getpid

            try:
                ejs_pkg = types.ModuleType("yt_dlp_ejs")
                ejs_yt = types.ModuleType("yt_dlp_ejs.yt")
                ejs_solver = types.ModuleType("yt_dlp_ejs.yt.solver")
                ejs_solver.lib = lambda: "const lib = { marker: true };"
                ejs_solver.core = lambda: "function jsc(i) { return { type: 'result', preprocessed_player: 'p', responses: [] }; }"

                class FakeQuickJSJCP:
                    logger = types.SimpleNamespace(debug=lambda *_a: None)

                    def _construct_stdin(self, player, preprocessed, requests, /):
                        orig_construct_calls.append(player)
                        return f"QJS_REAL:{player}"

                    def _run_js_runtime(self, stdin, /):
                        orig_run_calls.append(stdin)
                        return "FALLBACK"

                qjs_mod = types.ModuleType("yt_dlp.extractor.youtube.jsc._builtin.quickjs")
                qjs_mod.QuickJSJCP = FakeQuickJSJCP

                sys.modules["yt_dlp_ejs"] = ejs_pkg
                sys.modules["yt_dlp_ejs.yt"] = ejs_yt
                sys.modules["yt_dlp_ejs.yt.solver"] = ejs_solver
                sys.modules["yt_dlp"] = types.ModuleType("yt_dlp")
                sys.modules["yt_dlp.extractor"] = types.ModuleType("yt_dlp.extractor")
                sys.modules["yt_dlp.extractor.youtube"] = types.ModuleType("yt_dlp.extractor.youtube")
                sys.modules["yt_dlp.extractor.youtube.jsc"] = types.ModuleType("yt_dlp.extractor.youtube.jsc")
                sys.modules["yt_dlp.extractor.youtube.jsc._builtin"] = types.ModuleType("yt_dlp.extractor.youtube.jsc._builtin")
                sys.modules["yt_dlp.extractor.youtube.jsc._builtin.quickjs"] = qjs_mod

                log_path = os.path.join(tmp, "qjs_reqs.jsonl")
                fake_qjs = os.path.join(tmp, "fake-qjs")
                with open(fake_qjs, "w", encoding="utf-8") as f:
                    f.write(
                        "#!/usr/bin/env python3\n"
                        "import json, sys\n"
                        f"log_path = {log_path!r}\n"
                        "cache = set()\n"
                        "for line in sys.stdin:\n"
                        "    req = json.loads(line)\n"
                        "    with open(log_path, 'a', encoding='utf-8') as lf:\n"
                        "        lf.write(json.dumps(req) + '\\n')\n"
                        "    if req['type'] == 'cached':\n"
                        "        if req['key'] in cache:\n"
                        "            resp = {'type': 'result', 'responses': [{'type': 'result', 'data': {'abc': 'qjs-ok'}}]}\n"
                        "        else:\n"
                        "            resp = {'type': 'miss'}\n"
                        "    else:\n"
                        "        cache.add(req['key'])\n"
                        "        resp = {'type': 'result', 'responses': [{'type': 'result', 'data': {'abc': 'qjs-ok'}}]}\n"
                        "    sys.stdout.write(json.dumps(resp) + '\\n')\n"
                        "    sys.stdout.flush()\n"
                    )
                os.chmod(fake_qjs, 0o755)

                jsc_sock = os.path.join(tmp, "test-qjs.jsc")
                proc = mod.start_jsc_qjs(fake_qjs, jsc_sock)
                self.assertIsNotNone(proc)
                try:
                    self.assertEqual(stat.S_IMODE(os.stat(jsc_sock + ".js").st_mode), 0o600)
                    self.assertEqual(stat.S_IMODE(os.stat(jsc_sock + ".lib.js").st_mode), 0o600)

                    req_obj = types.SimpleNamespace(
                        type=types.SimpleNamespace(value="n"),
                        input=types.SimpleNamespace(player_url="player-q1", challenges=["abc"]),
                    )
                    jcp = FakeQuickJSJCP()

                    # 1st call: miss -> sends preprocessed
                    s1 = jcp._construct_stdin("PRE_4MB", True, [req_obj])
                    r1 = json.loads(jcp._run_js_runtime(s1))
                    self.assertEqual(r1["type"], "result")
                    self.assertEqual(len(orig_construct_calls), 0)
                    self.assertEqual(len(orig_run_calls), 0)

                    # 2nd call: cache hit
                    s2 = jcp._construct_stdin("PRE_4MB", True, [req_obj])
                    r2 = json.loads(jcp._run_js_runtime(s2))
                    self.assertEqual(r2["responses"][0]["data"]["abc"], "qjs-ok")

                    with open(log_path, "r", encoding="utf-8") as f:
                        seen = [json.loads(line)["type"] for line in f]
                    self.assertEqual(seen, ["cached", "preprocessed", "cached"])

                    # Simulated forked child PID bypasses resident pipe and falls back to orig_run
                    real_pid = orig_getpid()
                    mod.os.getpid = lambda: real_pid + 1
                    s3 = jcp._construct_stdin("PRE_4MB", True, [req_obj])
                    self.assertEqual(jcp._run_js_runtime(s3), "FALLBACK")
                    self.assertEqual(orig_run_calls, ["QJS_REAL:PRE_4MB"])
                finally:
                    mod.os.getpid = orig_getpid
                    for pipe in (proc.stdin, proc.stdout):
                        if pipe:
                            pipe.close()
                    proc.terminate()
                    proc.wait(timeout=2)
            finally:
                mod.os.getpid = orig_getpid
                for name, prev in saved_mods.items():
                    if prev is None:
                        sys.modules.pop(name, None)
                    else:
                        sys.modules[name] = prev

    @unittest.skipUnless(shutil.which("node"), "node required to exercise ytdl-jsc-qjs.js harness")
    def test_ytdl_jsc_qjs_lazy_lib_load_and_cache_protocol(self):
        with tempfile.TemporaryDirectory() as tmp:
            lib_path = os.path.join(tmp, "lib.js")
            with open(lib_path, "w", encoding="utf-8") as f:
                f.write(
                    "globalThis.__qjsLibLoaded = (globalThis.__qjsLibLoaded || 0) + 1;\n"
                    "function jsc(input) {\n"
                    "  return {\n"
                    "    type: 'result',\n"
                    "    preprocessed_player: '_result.n = (x) => \"n:\" + x;',\n"
                    "    responses: input.requests.map(r => ({ type: 'result', data: Object.fromEntries(r.challenges.map(c => [c, 'n:' + c])) }))\n"
                    "  };\n"
                    "}\n"
                    "globalThis.__cattube_jsc = jsc;\n"
                )
            reqs = [
                json.dumps({"type": "cached", "key": "k1", "requests": [{"type": "n", "challenges": ["a"]}]}),
                json.dumps({"type": "preprocessed", "key": "k1", "preprocessed_player": "_result.n = (x) => 'p:' + x;", "requests": [{"type": "n", "challenges": ["a"]}]}),
                json.dumps({"type": "cached", "key": "k1", "requests": [{"type": "n", "challenges": ["b"]}]}),
                json.dumps({"type": "player", "key": "k2", "player": "raw", "requests": [{"type": "n", "challenges": ["c"]}]}),
                json.dumps({"type": "cached", "key": "k2", "requests": [{"type": "n", "challenges": ["d"]}]}),
            ]
            harness = os.path.join(tmp, "harness-qjs.js")
            with open(harness, "w", encoding="utf-8") as f:
                f.write(
                    "const fs = require('fs');\n"
                    "const vm = require('vm');\n"
                    "const assert = require('assert');\n"
                    f"const queue = {json.dumps(reqs)};\n"
                    "const outputs = [];\n"
                    f"globalThis.scriptArgs = ['ytdl-jsc-qjs.js', {json.dumps(lib_path)}];\n"
                    "globalThis.std = {\n"
                    "  in: { getline: () => queue.length ? queue.shift() : null },\n"
                    "  out: { puts: (s) => outputs.push(s), flush: () => {} },\n"
                    "  loadFile: (p) => fs.readFileSync(p, 'utf8'),\n"
                    "  evalScript: (code) => vm.runInThisContext(code),\n"
                    "};\n"
                    f"vm.runInThisContext(fs.readFileSync({json.dumps(QJS_PATH)}, 'utf8'));\n"
                    "assert.strictEqual(outputs.length, 5);\n"
                    "const parsed = outputs.map((s) => JSON.parse(s));\n"
                    "assert.deepStrictEqual(parsed[0], { type: 'miss' });\n"
                    "assert.deepStrictEqual(parsed[1], { type: 'result', responses: [{ type: 'result', data: { a: 'p:a' } }] });\n"
                    "assert.deepStrictEqual(parsed[2], { type: 'result', responses: [{ type: 'result', data: { b: 'p:b' } }] });\n"
                    "assert.strictEqual(globalThis.__qjsLibLoaded, 1);\n"
                    "assert.deepStrictEqual(parsed[3].responses, [{ type: 'result', data: { c: 'n:c' } }]);\n"
                    "assert.deepStrictEqual(parsed[4], { type: 'result', responses: [{ type: 'result', data: { d: 'n:d' } }] });\n"
                    "assert.strictEqual(globalThis.__qjsLibLoaded, 1);\n"
                )
            subprocess.run([shutil.which("node"), harness], check=True)


if __name__ == "__main__":
    unittest.main()
