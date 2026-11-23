import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { compareOne, parseManifest, parseValue, pngSize, renderScoreboard, stripComment } from "../compare.mjs";

test("parseManifest reads boards with their regions", () => {
  const manifest = `# Harbour design oracles
[[board]]
id = "C02"
title = "Buttons # and links"   # trailing comment
oracle = "docs/design-oracles/harbour/C02.png"
snapshot = "crates/components/tests/snapshots/c02.png"

[[board.regions]]
name = "primary"
x = 0
y = 12
width = 104
height = 32

[[board]]
id = "Main"
title = "Say \\"hi\\""
oracle = "docs/design-oracles/harbour/Main.png"
snapshot = ""
`;
  assert.deepEqual(parseManifest(manifest), [
    {
      id: "C02",
      title: "Buttons # and links",
      oracle: "docs/design-oracles/harbour/C02.png",
      snapshot: "crates/components/tests/snapshots/c02.png",
      regions: [{ name: "primary", x: 0, y: 12, width: 104, height: 32 }],
    },
    {
      id: "Main",
      title: 'Say "hi"',
      oracle: "docs/design-oracles/harbour/Main.png",
      snapshot: "",
      regions: [],
    },
  ]);
});

test("parseManifest rejects what it does not understand", () => {
  const board = '[[board]]\nid = "A"\ntitle = "T"\noracle = "o.png"\nsnapshot = ""\n';
  const region = (fields) => `${board}[[board.regions]]\n${fields}`;
  const cases = [
    ["[[board.regions]]\nname = \"r\"\n", /line 1: \[\[board.regions\]\] before any \[\[board\]\]/],
    ['id = "A"\n', /line 1: unsupported syntax/],
    [`${board}[table]\n`, /line 6: unsupported syntax/],
    [`${board}flag = true\n`, /line 6: unsupported value true/],
    ['[[board]]\nid = "A"\ntitle = "T"\noracle = "o.png"\n', /board A: missing string snapshot/],
    [region("x = 0\ny = 0\nwidth = 1\nheight = 1\n"), /board A: region without a name/],
    [region('name = "r"\nx = -1\ny = 0\nwidth = 1\nheight = 1\n'), /region r: x must be a non-negative integer/],
    [region('name = "r"\nx = 0\ny = 0\nwidth = 1\n'), /region r: height must be a non-negative integer/],
  ];
  for (const [text, error] of cases) assert.throws(() => parseManifest(text), error, text);
});

test("stripComment drops comments outside strings only", () => {
  assert.equal(stripComment('id = "A" # note'), 'id = "A" ');
  assert.equal(stripComment('title = "a # b"'), 'title = "a # b"');
  assert.equal(stripComment('title = "say \\"#\\"" # x'), 'title = "say \\"#\\"" ');
  assert.equal(stripComment("# whole line"), "");
  assert.equal(stripComment("x = 1"), "x = 1");
});

test("parseValue accepts integers and JSON-escaped strings", () => {
  assert.equal(parseValue("42", "here"), 42);
  assert.equal(parseValue("-7", "here"), -7);
  assert.equal(parseValue('"a \\"b\\" \\u00e9"', "here"), 'a "b" é');
  assert.equal(parseValue('""', "here"), "");
  for (const value of ["1.5", "true", "'single'", '"open', '"a" "b"']) {
    assert.throws(() => parseValue(value, "manifest line 3"), {
      message: `manifest line 3: unsupported value ${value}`,
    });
  }
});

test("pngSize reads the IHDR dimensions and rejects other files", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "harbour-png-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const header = Buffer.alloc(33);
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).copy(header, 0);
  header.writeUInt32BE(13, 8);
  header.write("IHDR", 12, "latin1");
  header.writeUInt32BE(1536, 16);
  header.writeUInt32BE(70000, 20);
  const png = join(dir, "board.png");
  writeFileSync(png, header);
  assert.deepEqual(pngSize(png), { width: 1536, height: 70000 });

  const text = join(dir, "board.txt");
  writeFileSync(text, "definitely not a png, but long enough");
  assert.throws(() => pngSize(text), /board\.txt is not a PNG/);
});

test("renderScoreboard tabulates comparisons and escapes notes", () => {
  const rows = [
    {
      board: "CAL",
      region: "",
      status: "equivalent",
      exit: 0,
      rawMae: 0.0012345678,
      alignedMae: 0.001,
      summary: { moved: 0, resized: 1, added: 2, removed: 3, changed: 4, canvas_size: "1536x1024" },
      diff: "CAL/diff.png",
      nonAaSummary: { moved: 0, resized: 1, added: 2, removed: 3, changed: 1 },
      nonAaDiff: "CAL/non-aa/diff.png",
      note: "",
    },
    { board: "C02", region: "primary", status: "error", exit: 3, note: "kineprism: bad | input" },
    { board: "Main", region: "", status: "no snapshot" },
  ];
  assert.equal(
    renderScoreboard(rows, new Date("2026-01-02T03:04:05Z")),
    `# Harbour oracle scoreboard

Generated 2026-01-02T03:04:05.000Z by scripts/design-oracles/compare.sh. Exit: 0 equivalent, 1 differences, 2 Kineprism error.

| Board | Region | Status | Exit | Raw MAE | Global aligned MAE | Moved | Resized | Added | Removed | Changed | Changed (non-AA) | Canvas size | Diff | Non-AA diff | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CAL | (full) | equivalent | 0 | 0.001235 | 0.001000 | 0 | 1 | 2 | 3 | 4 | 1 | 1536x1024 | [diff](CAL/diff.png) | [non-AA diff](CAL/non-aa/diff.png) |  |
| C02 | primary | error | 3 |  |  |  |  |  |  |  |  |  |  |  | kineprism: bad \\| input |
| Main | (full) | no snapshot |  |  |  |  |  |  |  |  |  |  |  |  |  |
`,
  );
});

test("renderScoreboard explains an empty selection", () => {
  assert.match(renderScoreboard([], new Date(0)), /\n\nNo boards with a snapshot were selected\.\n$/);
});

test("parseManifest reserves the non-AA pass directory as a region name", () => {
  const manifest = `[[board]]
id = "C02"
title = "Buttons"
oracle = "a.png"
snapshot = "b.png"

[[board.regions]]
name = "non-aa"
x = 0
y = 0
width = 1
height = 1
`;
  assert.throws(() => parseManifest(manifest), /region name "non-aa" is reserved for the non-AA pass/);
});

/** A minimal PNG header, enough for pngSize. */
function writePng(path, width, height) {
  const header = Buffer.alloc(33);
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).copy(header, 0);
  header.writeUInt32BE(13, 8);
  header.write("IHDR", 12, "latin1");
  header.writeUInt32BE(width, 16);
  header.writeUInt32BE(height, 20);
  writeFileSync(path, header);
}

// Stands in for the Kineprism CLI (an external binary): records its arguments next to the report
// it writes, and fails the pass named by FAKE_KINEPRISM_FAIL ("primary" or "non-aa").
const FAKE_KINEPRISM = `
const { appendFileSync, mkdirSync, writeFileSync } = require("node:fs");
const { join } = require("node:path");
const args = process.argv.slice(2);
const dir = args[args.indexOf("--output-dir") + 1];
const nonAa = args.includes("--ignore-antialiasing");
mkdirSync(dir, { recursive: true });
appendFileSync(join(process.env.FAKE_KINEPRISM_LOG, "calls.jsonl"), JSON.stringify(args) + "\\n");
if (process.env.FAKE_KINEPRISM_FAIL === (nonAa ? "non-aa" : "primary")) {
  process.stderr.write("fake failure\\n");
  process.exit(2);
}
const summary = { moved: 0, resized: 0, added: 0, removed: 0, changed: nonAa ? 3 : 40, canvas_size: 0 };
writeFileSync(join(dir, "report.json"), JSON.stringify({
  equivalent: false,
  metrics: { raw: { mae: 0.0004 }, global_aligned: { mae: 0.0004 } },
  summary,
}));
process.exit(1);
`;

function fakeComparison(t, fail) {
  const dir = mkdtempSync(join(tmpdir(), "harbour-compare-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const script = join(dir, "fake-kineprism.cjs");
  writeFileSync(script, FAKE_KINEPRISM);
  writePng(join(dir, "oracle.png"), 1536, 1024);
  writePng(join(dir, "snapshot.png"), 1536, 1024);
  process.env.FAKE_KINEPRISM_LOG = dir;
  if (fail) process.env.FAKE_KINEPRISM_FAIL = fail;
  else delete process.env.FAKE_KINEPRISM_FAIL;
  t.after(() => {
    delete process.env.FAKE_KINEPRISM_LOG;
    delete process.env.FAKE_KINEPRISM_FAIL;
  });
  const board = { id: "C02", oracle: join(dir, "oracle.png"), snapshot: join(dir, "snapshot.png") };
  const target = { name: "primary", x: 1, y: 2, width: 3, height: 4, dir: join(dir, "out", "C02", "primary") };
  const row = compareOne({ command: process.execPath, prefix: [script] }, board, target);
  let calls = [];
  try {
    calls = readFileSync(join(dir, "calls.jsonl"), "utf8").trim().split("\n").map((line) => JSON.parse(line));
  } catch {}
  return { row, calls, target };
}

test("compareOne adds an antialiasing-insensitive pass beside the primary comparison", (t) => {
  const { row, calls, target } = fakeComparison(t);
  assert.equal(calls.length, 2);
  const region = ["--region-x", "1", "--region-y", "2", "--region-width", "3", "--region-height", "4"];
  assert.deepEqual(calls[0].slice(2), ["--output-dir", target.dir, "--force", ...region]);
  assert.deepEqual(calls[1].slice(2), ["--output-dir", join(target.dir, "non-aa"), "--force", ...region, "--ignore-antialiasing"]);
  assert.deepEqual(calls[1].slice(0, 2), calls[0].slice(0, 2), "both passes compare the same images");
  assert.equal(row.status, "different");
  assert.equal(row.rawMae, 0.0004);
  assert.equal(row.summary.changed, 40);
  assert.equal(row.nonAaSummary.changed, 3);
  assert.match(row.diff, /C02\/primary\/diff\.png$/);
  assert.match(row.nonAaDiff, /C02\/primary\/non-aa\/diff\.png$/);
  assert.equal(row.note, "");
});

test("a failing non-AA pass only adds a note to the primary result", (t) => {
  const { row, calls } = fakeComparison(t, "non-aa");
  assert.equal(calls.length, 2);
  assert.equal(row.status, "different");
  assert.equal(row.summary.changed, 40);
  assert.equal(row.nonAaSummary, undefined);
  assert.equal(row.nonAaDiff, undefined);
  assert.equal(row.note, "non-AA pass: kineprism: fake failure");
});

test("a failing primary pass is an error row and skips the non-AA pass", (t) => {
  const { row, calls } = fakeComparison(t, "primary");
  assert.equal(calls.length, 1);
  assert.equal(row.status, "error");
  assert.equal(row.exit, 2);
  assert.equal(row.note, "kineprism: fake failure");
});
