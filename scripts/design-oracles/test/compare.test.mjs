import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { parseManifest, parseValue, pngSize, renderScoreboard, stripComment } from "../compare.mjs";

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
      note: "",
    },
    { board: "C02", region: "primary", status: "error", exit: 3, note: "kineprism: bad | input" },
    { board: "Main", region: "", status: "no snapshot" },
  ];
  assert.equal(
    renderScoreboard(rows, new Date("2026-01-02T03:04:05Z")),
    `# Harbour oracle scoreboard

Generated 2026-01-02T03:04:05.000Z by scripts/design-oracles/compare.sh. Exit: 0 equivalent, 1 differences, 2 Kineprism error.

| Board | Region | Status | Exit | Raw MAE | Global aligned MAE | Moved | Resized | Added | Removed | Changed | Canvas size | Diff | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CAL | (full) | equivalent | 0 | 0.001235 | 0.001000 | 0 | 1 | 2 | 3 | 4 | 1536x1024 | [diff](CAL/diff.png) |  |
| C02 | primary | error | 3 |  |  |  |  |  |  |  |  |  | kineprism: bad \\| input |
| Main | (full) | no snapshot |  |  |  |  |  |  |  |  |  |  |  |
`,
  );
});

test("renderScoreboard explains an empty selection", () => {
  assert.match(renderScoreboard([], new Date(0)), /\n\nNo boards with a snapshot were selected\.\n$/);
});
