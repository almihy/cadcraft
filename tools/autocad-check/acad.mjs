// Runs AutoCAD's Core Console over drawings, one process per file with a time limit (a hung open is ended and counts
// as a failure). It never touches the AutoCAD window the owner has open.
//
//   node acad.mjs count <folder> [--jobs 4] [--limit 180]   each *.dwg → <file>.acad.txt (counts) and <file>.acad.log
//   node acad.mjs variants <in folder> <out folder> <list file>   each listed file saved as 2000, 2004 and 2013
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const CONSOLE = "C:\\Program Files\\Autodesk\\AutoCAD 2025\\accoreconsole.exe";
const ONLY = process.argv.includes("--only") ? new RegExp(process.argv[process.argv.indexOf("--only") + 1], "i") : null;
const here = dirname(fileURLToPath(import.meta.url));
const arg = (name, d) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? Number(process.argv[i + 1]) : d;
};

function walk(dir, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (/\.dwg$/i.test(e.name)) out.push(p);
  }
  return out;
}

/** One Core Console run: resolves { ok, timedOut, seconds, log }. */
function run(dwg, script, limit) {
  return new Promise((done) => {
    const t0 = Date.now();
    const p = spawn(CONSOLE, ["/i", dwg, "/s", script, "/l", "en-US"], { windowsHide: false });
    let log = "";
    const take = (b) => (log += b.toString("utf16le"));
    p.stdout.on("data", take);
    p.stderr.on("data", take);
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      p.kill();
    }, limit * 1000);
    p.on("close", (code) => {
      clearTimeout(timer);
      done({ ok: code === 0 && !timedOut, code, timedOut, seconds: (Date.now() - t0) / 1000, log });
    });
    p.on("error", (e) => {
      clearTimeout(timer);
      done({ ok: false, code: -1, timedOut, seconds: 0, log: String(e) });
    });
  });
}

async function pool(items, jobs, work) {
  let next = 0;
  await Promise.all(
    Array.from({ length: jobs }, async () => {
      while (next < items.length) {
        const i = next++;
        await work(items[i], i);
      }
    }),
  );
}

const mode = process.argv[2];
if (mode === "count") {
  const root = resolve(process.argv[3]);
  const files = walk(root);
  const limit = arg("limit", 180);
  await pool(files, arg("jobs", 4), async (f, i) => {
    rmSync(`${f}.acad.txt`, { force: true });
    const r = await run(f, join(here, "count.scr"), limit);
    writeFileSync(`${f}.acad.log`, r.log);
    const got = existsSync(`${f}.acad.txt`);
    const state = r.timedOut ? "TIMED OUT" : got ? "ok" : `NO COUNTS (exit ${r.code})`;
    console.log(`[${i + 1}/${files.length}] ${state} ${r.seconds.toFixed(1)}s ${relative(root, f)}`);
  });
} else if (mode === "variants") {
  const [inDir, outDir, list] = [resolve(process.argv[3]), resolve(process.argv[4]), process.argv[5]];
  const names = readFileSync(list, "utf8").split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
  const todo = [];
  for (const n of names) for (const v of ["2000", "2004", "2013"]) todo.push([n, v]);
  await pool(todo, arg("jobs", 4), async ([n, v]) => {
    const src = join(inDir, n);
    const dst = join(outDir, v, n.replace(/[\\/]/g, "__"));
    mkdirSync(dirname(dst), { recursive: true });
    rmSync(dst, { force: true });
    const scr = join(outDir, `.save-${v}-${basename(dst)}.scr`);
    writeFileSync(scr, `_.FILEDIA 0\n_.SAVEAS ${v} "${dst.replace(/\\/g, "/")}"\n`);
    const r = await run(src, scr, arg("limit", 180));
    rmSync(scr, { force: true });
    console.log(`${existsSync(dst) ? "ok" : "FAILED"} ${v} ${n} ${r.seconds.toFixed(1)}s${r.timedOut ? " TIMED OUT" : ""}`);
    if (!existsSync(dst)) writeFileSync(`${dst}.failed.log`, r.log);
  });
} else {
  console.error("usage: node acad.mjs count <folder> | variants <in> <out> <list>");
  process.exit(2);
}
void statSync;
