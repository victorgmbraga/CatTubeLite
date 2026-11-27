// Kept running by ytdl-server.py, after yt-dlp's own challenge solver (lib
// and core). yt-dlp runs a new Deno for every video, which parses the whole
// (preprocessed) YouTube player again: seconds on a TV box. Here the
// player's n and sig functions are built once per player and reused.
//   deno run ... ytdl-jsc.js SOCKET     until stdin closes
// Request: what yt-dlp would pass to jsc(), plus `key` (the player's URL).
// Answer: what jsc() would return.

const KEEP = 2;
const solvers = new Map();

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

function solve(input) {
  if (input.type === 'player') {
    const output = jsc({ ...input, output_preprocessed: true });
    if (output.type === 'result' && output.preprocessed_player) {
      solversFor(input.key, output.preprocessed_player);
    }
    return output;
  }
  const s = solversFor(input.key, input.preprocessed_player);
  return {
    type: 'result',
    responses: input.requests.map((req) => {
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

const listener = Deno.listen({ transport: 'unix', path: Deno.args[0] });
for await (const conn of listener) await handle(conn);
