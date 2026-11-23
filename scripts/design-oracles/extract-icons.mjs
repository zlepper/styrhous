// Writes the design's icon set into crates/components/assets/icons/<name>.svg.
//
// Usage: npm run extract-icons   (after npm run extract)
//
// The boards draw icons as CSS masks: `.ic-<name>` classes in the shared stylesheet hold
// Heroicons outline (stroke-width 1.5) SVGs as data URIs. The app embeds exactly those SVGs so
// egui and Chromium rasterize the same paths. The stroke becomes white so egui can tint the
// image with any role colour (mask + background-color in CSS, image tint in egui).
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { appIconsDir, runMain, srcDir } from "./lib.mjs";

function main() {
  const css = readFileSync(join(srcDir, "harbour.css"), "utf8");
  const pattern = /\.ic-([a-z0-9-]+)\{-webkit-mask-image:url\("data:image\/svg\+xml,([^"]*)"\)/g;
  mkdirSync(appIconsDir, { recursive: true });
  const names = [];
  for (const [, name, encoded] of css.matchAll(pattern)) {
    const svg = decodeURIComponent(encoded);
    if (!/stroke="black"/.test(svg) || /fill="(?!none)/.test(svg)) {
      throw new Error(`.ic-${name}: expected a black-stroked outline icon`);
    }
    const tintable = svg.replace(/stroke="black"/g, 'stroke="white"').replace(/>\s+</g, "><");
    writeFileSync(join(appIconsDir, `${name}.svg`), `${tintable}\n`);
    names.push(name);
  }
  if (names.length === 0) throw new Error("no .ic-* icons found in harbour.css");
  console.log(`Wrote ${names.length} icons to ${relative(process.cwd(), appIconsDir) || appIconsDir}`);
}

runMain(import.meta, main);
