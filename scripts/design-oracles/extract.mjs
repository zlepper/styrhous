// Copies the Harbour design artboards into docs/design-oracles/harbour/src/ as plain HTML.
//
// Usage: npm run extract -- <design-project-dir>
//
// The design project holds `Main.dc.html`, `components/C*.dc.html` and `app/{A,L,Y}*.dc.html`.
// Each board is a static `<x-dc>` page: the `<helmet>` contents (fonts link + shared stylesheet)
// move into `<head>`, the Google Fonts link is replaced by fonts.css (@font-face rules for the
// static font files the app embeds), and the artifact runtime script plus the `text/x-dc` logic
// block are dropped. The boards share one stylesheet, which is also written to harbour.css for
// hand-written calibration boards; those (`"calibration": true` in their harbour-board meta, e.g.
// CAL.html) survive a re-extract. The new src/ is built in a staging directory and swapped in only
// once every board converted, so a failed extract leaves the previous src/ untouched.
import { existsSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import {
  boardIdFromFileName,
  harbourDir,
  isCalibrationBoard,
  listBoardIds,
  runMain,
  srcDir,
  writeFontsCss,
} from "./lib.mjs";

const boardPattern = /^(Main|C\d+b?|[ALY]\d+)-?.*\.dc\.html$/;
const boardDirs = ["", "components", "app"];

function main() {
  const projectDir = process.argv[2];
  if (!projectDir || !existsSync(join(projectDir, "Main.dc.html"))) {
    throw new Error("Usage: npm run extract -- <design-project-dir containing Main.dc.html>");
  }

  const files = new Map();
  // Calibration boards are kept as they are; a design board may not take their ID.
  const calibration = existsSync(srcDir) ? listBoardIds().filter(isCalibrationBoard) : [];
  for (const id of calibration) files.set(`${id}.html`, readFileSync(join(srcDir, `${id}.html`), "utf8"));

  const seen = new Map(calibration.map((id) => [id, `src/${id}.html (calibration)`]));
  let sharedStyle = null;
  for (const file of designBoardFiles(projectDir)) {
    const id = boardIdFromFileName(file.split("/").pop());
    if (seen.has(id)) throw new Error(`Duplicate board ID ${id}: ${seen.get(id)} and ${file}`);
    seen.set(id, file);
    const html = convertBoard(readFileSync(join(projectDir, file), "utf8"), id, file);
    const style = required(html.match(/<style>\n?([\s\S]*?)<\/style>/), "<style>", file)[1];
    if (sharedStyle !== null && style !== sharedStyle) throw new Error(`${file}: stylesheet differs from the other boards`);
    sharedStyle = style;
    files.set(`${id}.html`, html);
  }
  if (sharedStyle === null) throw new Error(`${projectDir}: no design boards found`);
  files.set("harbour.css", sharedStyle);

  const staging = mkdtempSync(join(harbourDir, ".src-staging-"));
  try {
    writeFontsCss(staging);
    for (const [name, content] of files) writeFileSync(join(staging, name), content);
    replaceDir(staging, srcDir);
  } catch (error) {
    rmSync(staging, { recursive: true, force: true });
    throw error;
  }
  const designCount = seen.size - calibration.length;
  console.log(`Extracted ${designCount} boards into ${relative(process.cwd(), srcDir) || srcDir}`);
}

/** The design board files of the project, relative to it. */
function designBoardFiles(projectDir) {
  return boardDirs.flatMap((sub) => {
    const dir = join(projectDir, sub);
    if (!existsSync(dir)) throw new Error(`${projectDir}: missing ${sub}/ directory`);
    return readdirSync(dir)
      .filter((name) => boardPattern.test(name))
      .map((name) => join(sub, name));
  });
}

/** Replaces `target` with the `staging` directory, restoring the old one if the swap fails. */
function replaceDir(staging, target) {
  const previous = `${staging}-previous`;
  const hadTarget = existsSync(target);
  if (hadTarget) renameSync(target, previous);
  try {
    renameSync(staging, target);
  } catch (error) {
    if (hadTarget) renameSync(previous, target);
    throw error;
  }
  rmSync(previous, { recursive: true, force: true });
}

function required(match, what, file) {
  if (!match) throw new Error(`${file}: could not find ${what}`);
  return match;
}

/** Unwraps one `<x-dc>` design board into a standalone HTML page linking fonts.css. */
export function convertBoard(source, id, file) {
  const title = required(source.match(/<title>([\s\S]*?)<\/title>/), "<title>", file)[1].trim();
  const helmet = required(source.match(/<helmet>([\s\S]*?)<\/helmet>/), "<helmet>", file)[1];
  const body = required(source.match(/<\/helmet>([\s\S]*?)<\/x-dc>/), "board markup", file)[1];
  const props = required(source.match(/data-props='([^']*)'/), "data-props", file)[1];
  const preview = JSON.parse(props).$preview;
  if (!preview?.width || !preview?.height) throw new Error(`${file}: data-props has no $preview size`);
  if (!/^\s*<div class="board"/.test(body)) throw new Error(`${file}: markup does not start with .board`);

  const head = helmet
    .replace(/<link rel="preconnect"[^>]*>/g, "")
    .replace(/<link rel="stylesheet" href="https:\/\/fonts\.googleapis\.com[^>]*>/g, "")
    .trim();
  if (/https?:\/\//.test(head.replace(/<style>[\s\S]*<\/style>/, ""))) {
    throw new Error(`${file}: helmet still references a remote resource after unwrapping`);
  }

  const meta = JSON.stringify({ id, width: preview.width, height: preview.height, source: file });
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>${title}</title>
<meta name="harbour-board" content='${meta}'>
<link rel="stylesheet" href="fonts.css">
${head}
</head>
<body>
${body.trim()}
</body>
</html>
`;
}

runMain(import.meta, main);
