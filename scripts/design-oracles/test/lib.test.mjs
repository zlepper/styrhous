import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { boardIdFromFileName, fallbackUnicodeRange, selectBoardIds } from "../lib.mjs";

test("selectBoardIds returns every known board when none are requested", () => {
  assert.deepEqual(selectBoardIds([], ["Main", "C01", "A02"]), ["Main", "C01", "A02"]);
});

test("selectBoardIds keeps the requested order and rejects unknown IDs", () => {
  assert.deepEqual(selectBoardIds(["A02", "Main"], ["Main", "C01", "A02"]), ["A02", "Main"]);
  assert.throws(
    () => selectBoardIds(["A02", "Z9", "Q1"], ["Main", "A02"]),
    { message: "Unknown board ID(s): Z9, Q1. Known: Main, A02" },
  );
});

test("boardIdFromFileName takes the ID before the first dash", () => {
  assert.equal(boardIdFromFileName("A02-Pods.dc.html"), "A02");
  assert.equal(boardIdFromFileName("C00b-Theme-layers.dc.html"), "C00b");
  assert.equal(boardIdFromFileName("Main.dc.html"), "Main");
});

test("fallbackUnicodeRange reads the trimmed range, empty when no fallback is needed", (t) => {
  const root = mkdtempSync(join(tmpdir(), "harbour-fonts-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "dejavu"));
  const rangeFile = join(root, "dejavu", "unicode-range.txt");

  writeFileSync(rangeFile, "U+21B5, U+2318, U+232B\n");
  assert.equal(fallbackUnicodeRange(root), "U+21B5, U+2318, U+232B");

  writeFileSync(rangeFile, "\n");
  assert.equal(fallbackUnicodeRange(root), "");
});
