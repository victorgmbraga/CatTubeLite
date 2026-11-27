// Kept running by ytdl-server.py on tight-memory systems (`qjs --std`).
// Holds the parsed YouTube player n/sig solvers in memory across videos and
// lazily evaluates yt-dlp's EJS solver library only on a full-player miss.
//   qjs --std ytdl-jsc-qjs.js LIB_JS     reads one JSON request per line on stdin
// Request: `cached` probe, `preprocessed`, or `player`, plus `key` (player URL).
// Answer: `{type: 'miss'}` for an uncached probe, or what jsc() would return.

const libPath = (typeof scriptArgs !== 'undefined' && scriptArgs[1]) || '';
const KEEP = 2;
const solvers = new Map();
let jscFn = typeof globalThis.__cattube_jsc === 'function'
  ? globalThis.__cattube_jsc
  : (typeof globalThis.jsc === 'function' ? globalThis.jsc : null);

function loadJsc() {
  if (jscFn) return jscFn;
  if (libPath) {
    if (typeof std.loadScript === 'function') {
      std.loadScript(libPath);
    } else if (typeof std.evalScript === 'function') {
      std.evalScript(std.loadFile(libPath));
    } else {
      (0, eval)(std.loadFile(libPath));
    }
    jscFn = typeof globalThis.__cattube_jsc === 'function'
      ? globalThis.__cattube_jsc
      : (typeof globalThis.jsc === 'function' ? globalThis.jsc : null);
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

for (;;) {
  const line = std.in.getline();
  if (line === null) break;
  if (!line.trim()) continue;
  let output;
  try {
    output = solve(JSON.parse(line));
  } catch (error) {
    output = { type: 'error', error: `${error}` };
  }
  std.out.puts(JSON.stringify(output) + '\n');
  std.out.flush();
}
