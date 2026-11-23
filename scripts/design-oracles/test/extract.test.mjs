import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { convertBoard } from "../extract.mjs";

const fixture = readFileSync(join(import.meta.dirname, "fixtures", "A99-Sample.dc.html"), "utf8");
const file = "app/A99-Sample.dc.html";

test("convertBoard unwraps <x-dc> into a standalone page linking fonts.css", () => {
  assert.equal(
    convertBoard(fixture, "A99", file),
    `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Sample board</title>
<meta name="harbour-board" content='{"id":"A99","width":320,"height":200,"source":"app/A99-Sample.dc.html"}'>
<link rel="stylesheet" href="fonts.css">
<style>
.board{width:320px;height:200px;background:url("https://example.invalid/allowed-inside-style.png")}
</style>
</head>
<body>
<div class="board"><h1>Pods</h1><p>Ready ↵</p></div>
</body>
</html>
`,
  );
});

test("convertBoard rejects boards it cannot unwrap faithfully", () => {
  const cases = [
    [fixture.replace(/<title>[\s\S]*<\/title>/, ""), /could not find <title>/],
    [fixture.replace(/<helmet>[\s\S]*<\/helmet>/, ""), /could not find <helmet>/],
    [fixture.replace('"$preview": {"width": 320, "height": 200}', '"other": 1'), /no \$preview size/],
    [fixture.replace('<div class="board">', "<section>"), /does not start with \.board/],
    [
      fixture.replace("</helmet>", '<script src="https://cdn.invalid/runtime.js"></script>\n</helmet>'),
      /references a remote resource/,
    ],
  ];
  for (const [source, error] of cases) {
    assert.throws(() => convertBoard(source, "A99", file), error);
  }
});
