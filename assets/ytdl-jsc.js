// Kept running by ytdl-server.py. yt-dlp runs a new Deno for every video,
// which parses the whole (preprocessed) YouTube player again: seconds on a TV
// box. Here the player's n and sig functions are built once per player and
// reused, and yt-dlp's solver library (meriyah/astring) is loaded lazily only
// on a full-player miss.
//   deno run ... ytdl-jsc.js SOCKET [LIB_JS]     until stdin closes
// Request: `cached` probe, `preprocessed`, or `player`, plus `key` (player URL).
// Answer: `{type: 'miss'}` for an uncached probe, or what jsc() would return.

const [sockPath, libPath] = Deno.args;
const KEEP = 2;
const solvers = new Map();
let jscFn = typeof globalThis.__cattube_jsc === 'function'
  ? globalThis.__cattube_jsc
  : (typeof jsc === 'function' ? jsc : null);

function loadJsc() {
  if (jscFn) return jscFn;
  if (libPath) {
    const code = Deno.readTextFileSync(libPath);
    (0, eval)(code);
    jscFn = typeof globalThis.__cattube_jsc === 'function'
      ? globalThis.__cattube_jsc
      : (typeof jsc === 'function' ? jsc : null);
  }
  if (!jscFn) throw new Error('jsc solver library not loaded');
  return jscFn;
}

function solversFor(key, prepared) {
  let s = solvers.get(key);
  if (!s) {
    s = { n: null, sig: null };
    Function('_result', prepared)(s); // as yt-dlp's getFromPrepared()
    solvers.set(key, s);
    while (solvers.size > KEEP) solvers.delete(solvers.keys().next().value);
  }
  return s;
}

function runSolvers(s, requests) {
  return {
    type: 'result',
    responses: requests.map((req) => {
      const solver = s[req.type];
      if (!solver) return { type: 'error', error: `Failed to extract ${req.type} function` };
      try {
        return {
          type: 'result',
          data: Object.fromEntries(req.challenges.map((c) => [c, solver(c)])),
        };
      } catch (error) {
        return { type: 'error', error: error instanceof Error ? `${error.message}\n${error.stack}` : `${error}` };
      }
    }),
  };
}

function solve(input) {
  if (input.type === 'cached') {
    const s = solvers.get(input.key);
    if (!s) return { type: 'miss' };
    // Refresh insertion order so the active player stays cached.
    solvers.delete(input.key);
    solvers.set(input.key, s);
    return runSolvers(s, input.requests);
  }
  if (input.type === 'player') {
    const output = loadJsc()({ ...input, output_preprocessed: true });
    if (output.type === 'result' && output.preprocessed_player) {
      solversFor(input.key, output.preprocessed_player);
    }
    return output;
  }
  const s = solversFor(input.key, input.preprocessed_player);
  return runSolvers(s, input.requests);
}

async function readAll(conn) {
  const chunks = [];
  const buf = new Uint8Array(1 << 16);
  let total = 0;
  for (;;) {
    const n = await conn.read(buf);
    if (n === null) break;
    chunks.push(buf.slice(0, n));
    total += n;
  }
  const all = new Uint8Array(total);
  let off = 0;
  for (const c of chunks) { all.set(c, off); off += c.length; }
  return new TextDecoder().decode(all);
}

async function writeAll(conn, data) {
  let off = 0;
  while (off < data.length) off += await conn.write(data.subarray(off));
}

async function handle(conn) {
  try {
    let output;
    try {
      output = solve(JSON.parse(await readAll(conn)));
    } catch (error) {
      output = { type: 'error', error: `${error}` };
    }
    await writeAll(conn, new TextEncoder().encode(JSON.stringify(output)));
  } catch (_) {
    // the client went away
  } finally {
    try { conn.close(); } catch (_) { /* closed */ }
  }
}

// ytdl-server.py holds our stdin: when it goes, so do we.
(async () => {
  const buf = new Uint8Array(64);
  while ((await Deno.stdin.read(buf)) !== null) { /* nothing is sent */ }
  Deno.exit(0);
})();

const listener = Deno.listen({ transport: 'unix', path: sockPath });
for await (const conn of listener) await handle(conn);
