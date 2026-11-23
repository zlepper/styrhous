// Compares egui snapshots against their Harbour oracles with Kineprism and writes a scoreboard.
//
// Usage: node compare.mjs [--manifest PATH] [ID ...]
//
// Without IDs, every manifest board with a non-empty `snapshot` is compared; named IDs are
// compared (or reported as missing a snapshot) regardless. Each comparison writes Kineprism's
// report.json and annotated PNGs to target/design-oracles/<ID>/ (regions to
// target/design-oracles/<ID>/<region>/), and target/design-oracles/scoreboard.md summarises all.
// A comparison that fails (Kineprism error, missing report or image) becomes an error row and the
// others still run, so the scoreboard is always written. Exits 0 when every comparison is
// equivalent, 1 otherwise, and 2 when the run itself fails (unreadable manifest, unknown ID).
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, openSync, readFileSync, readSync, closeSync, rmSync, writeFileSync } from "node:fs";
import { isAbsolute, join, relative } from "node:path";
import { compareOutDir, manifestPath, repoRoot, runMain, selectBoardIds } from "./lib.mjs";

const defaultKineprism = join(
  repoRoot,
  "..",
  "better-image-diff",
  "target",
  "x86_64-unknown-linux-gnu",
  "release",
  "kineprism",
);

function main() {
  const args = process.argv.slice(2);
  let manifest = manifestPath;
  const manifestFlag = args.indexOf("--manifest");
  if (manifestFlag !== -1) {
    manifest = args[manifestFlag + 1];
    if (!manifest) throw new Error("--manifest needs a path");
    args.splice(manifestFlag, 2);
  }

  const boards = parseManifest(readFileSync(manifest, "utf8"));
  const byId = new Map(boards.map((board) => [board.id, board]));
  const requested = selectBoardIds(args, [...byId.keys()]);
  const selected = args.length ? requested.map((id) => byId.get(id)) : boards.filter((b) => b.snapshot);

  const kineprism = resolveKineprism();
  mkdirSync(compareOutDir, { recursive: true });
  const rows = [];
  for (const board of selected) {
    if (!board.snapshot) {
      rows.push({ board: board.id, region: "", status: "no snapshot" });
      continue;
    }
    const outDir = join(compareOutDir, board.id);
    rmSync(outDir, { recursive: true, force: true });
    const targets = [{ name: "", dir: outDir }];
    for (const region of board.regions) targets.push({ ...region, dir: join(outDir, safeName(region.name)) });
    for (const target of targets) {
      // One failing comparison is reported in its row; the others still run.
      try {
        rows.push(compareOne(kineprism, board, target));
      } catch (error) {
        rows.push({ board: board.id, region: target.name, status: "error", note: error.message });
      }
    }
  }

  const scoreboard = join(compareOutDir, "scoreboard.md");
  writeFileSync(scoreboard, renderScoreboard(rows));
  process.stdout.write(readFileSync(scoreboard, "utf8"));
  console.log(`\nWrote ${relative(process.cwd(), scoreboard)}`);
  const failed = rows.some((r) => r.status !== "equivalent");
  process.exitCode = failed ? 1 : 0;
}

/** Minimal TOML subset used by manifest.toml: [[board]], [[board.regions]], string/int keys. */
export function parseManifest(text) {
  const boards = [];
  let current = null;
  text.split(/\r?\n/).forEach((raw, index) => {
    const line = stripComment(raw).trim();
    const where = `manifest line ${index + 1}`;
    if (!line) return;
    if (line === "[[board]]") {
      current = { regions: [] };
      boards.push(current);
      return;
    }
    if (line === "[[board.regions]]") {
      if (!boards.length) throw new Error(`${where}: [[board.regions]] before any [[board]]`);
      current = {};
      boards.at(-1).regions.push(current);
      return;
    }
    const match = line.match(/^([A-Za-z_][\w-]*)\s*=\s*(.+)$/);
    if (!match || !current) throw new Error(`${where}: unsupported syntax: ${raw}`);
    current[match[1]] = parseValue(match[2], where);
  });

  for (const board of boards) {
    for (const key of ["id", "title", "oracle", "snapshot"]) {
      if (typeof board[key] !== "string") throw new Error(`board ${board.id ?? "?"}: missing string ${key}`);
    }
    for (const region of board.regions) {
      if (typeof region.name !== "string" || !region.name) {
        throw new Error(`board ${board.id}: region without a name`);
      }
      for (const key of ["x", "y", "width", "height"]) {
        if (!Number.isInteger(region[key]) || region[key] < 0) {
          throw new Error(`board ${board.id} region ${region.name}: ${key} must be a non-negative integer`);
        }
      }
    }
  }
  return boards;
}

export function stripComment(line) {
  let inString = false;
  for (let i = 0; i < line.length; i++) {
    if (line[i] === "\\" && inString) i++;
    else if (line[i] === '"') inString = !inString;
    else if (line[i] === "#" && !inString) return line.slice(0, i);
  }
  return line;
}

export function parseValue(value, where) {
  if (/^-?\d+$/.test(value)) return Number(value);
  if (/^"(?:[^"\\]|\\.)*"$/.test(value)) return JSON.parse(value);
  throw new Error(`${where}: unsupported value ${value}`);
}

function resolveKineprism() {
  const configured = process.env.KINEPRISM;
  if (configured) return { command: configured, prefix: [] };
  if (existsSync(defaultKineprism)) return { command: defaultKineprism, prefix: [] };
  return { command: "npx", prefix: ["--yes", "@zlepper/kineprism@0.0.5"] };
}

function compareOne(kineprism, board, target) {
  const row = { board: board.id, region: target.name };
  const expected = repoPath(board.oracle);
  // A pending egui_kittest candidate (<name>.new.png, written when the snapshot test fails) is
  // what the next UPDATE_SNAPSHOTS would accept, so it is compared instead of the fixture.
  const fixture = repoPath(board.snapshot);
  const candidate = fixture.replace(/\.png$/, ".new.png");
  const actual = existsSync(candidate) ? candidate : fixture;
  if (!existsSync(expected)) return { ...row, status: "error", note: `oracle missing: ${board.oracle}` };
  if (!existsSync(actual)) return { ...row, status: "error", note: `snapshot missing: ${board.snapshot}` };
  const expectedSize = pngSize(expected);
  const actualSize = pngSize(actual);
  const notes = actual === candidate ? ["compared the pending .new.png candidate"] : [];
  const sizeMismatch = expectedSize.width !== actualSize.width || expectedSize.height !== actualSize.height;
  if (sizeMismatch) {
    notes.push(`size mismatch: oracle ${fmtSize(expectedSize)}, snapshot ${fmtSize(actualSize)}`);
  }

  mkdirSync(target.dir, { recursive: true });
  const regionArgs = target.name
    ? ["--region-x", target.x, "--region-y", target.y, "--region-width", target.width, "--region-height", target.height].map(String)
    : [];
  const result = spawnSync(
    kineprism.command,
    [...kineprism.prefix, expected, actual, "--output-dir", target.dir, "--force", ...regionArgs],
    { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
  );
  if (result.error) return { ...row, status: "error", exit: "", note: `kineprism: ${result.error.message}` };
  const exit = result.status;
  if (exit !== 0 && exit !== 1) {
    const message = (result.stderr || result.stdout || "").trim().split("\n").at(-1);
    return { ...row, status: "error", exit, note: [...notes, `kineprism: ${message}`].join("; ") };
  }

  let report;
  try {
    report = JSON.parse(readFileSync(join(target.dir, "report.json"), "utf8"));
  } catch (error) {
    const note = `kineprism exited ${exit} without a readable report.json: ${error.message}`;
    return { ...row, status: "error", exit, note: [...notes, note].join("; ") };
  }
  return {
    ...row,
    exit,
    status: report.equivalent && !sizeMismatch ? "equivalent" : sizeMismatch ? "size mismatch" : "different",
    rawMae: report.metrics?.raw?.mae,
    alignedMae: report.metrics?.global_aligned?.mae,
    summary: report.summary,
    diff: relative(compareOutDir, join(target.dir, "diff.png")),
    note: notes.join("; "),
  };
}

function repoPath(path) {
  return isAbsolute(path) ? path : join(repoRoot, path);
}

/** Width and height from a PNG IHDR chunk. */
export function pngSize(path) {
  const header = Buffer.alloc(24);
  const fd = openSync(path, "r");
  try {
    readSync(fd, header, 0, 24, 0);
  } finally {
    closeSync(fd);
  }
  if (header.toString("latin1", 12, 16) !== "IHDR") throw new Error(`${path} is not a PNG`);
  return { width: header.readUInt32BE(16), height: header.readUInt32BE(20) };
}

const fmtSize = ({ width, height }) => `${width}x${height}`;
const safeName = (name) => name.replace(/[^\w.-]+/g, "_");
const fmtMetric = (value) => (typeof value === "number" ? value.toFixed(6) : "");

export function renderScoreboard(rows, generatedAt = new Date()) {
  const header = [
    "Board",
    "Region",
    "Status",
    "Exit",
    "Raw MAE",
    "Global aligned MAE",
    "Moved",
    "Resized",
    "Added",
    "Removed",
    "Changed",
    "Canvas size",
    "Diff",
    "Notes",
  ];
  const lines = [
    "# Harbour oracle scoreboard",
    "",
    `Generated ${generatedAt.toISOString()} by scripts/design-oracles/compare.sh. Exit: 0 equivalent, 1 differences, 2 Kineprism error.`,
    "",
    `| ${header.join(" | ")} |`,
    `|${header.map(() => " --- ").join("|")}|`,
  ];
  for (const r of rows) {
    const s = r.summary ?? {};
    const cells = [
      r.board,
      r.region || "(full)",
      r.status,
      r.exit ?? "",
      fmtMetric(r.rawMae),
      fmtMetric(r.alignedMae),
      s.moved ?? "",
      s.resized ?? "",
      s.added ?? "",
      s.removed ?? "",
      s.changed ?? "",
      s.canvas_size ?? "",
      r.diff ? `[diff](${r.diff})` : "",
      (r.note ?? "").replace(/\|/g, "\\|"),
    ];
    lines.push(`| ${cells.join(" | ")} |`);
  }
  if (rows.length === 0) lines.push("", "No boards with a snapshot were selected.");
  return `${lines.join("\n")}\n`;
}

runMain(import.meta, main, { failureCode: 2 });
