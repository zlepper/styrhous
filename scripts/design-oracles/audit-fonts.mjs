// Inventories the fonts the extracted boards actually render text with.
//
// Usage: npm run audit-fonts [-- ID ...]
//
// Prints every (family, CSS font-size, font-weight) combination used for text, every Bricolage
// Grotesque size/weight without a static instance (Chrome's `font-optical-sizing: auto` sets
// opsz = font-size in CSS px, so each size needs its own instance), and any text Chrome drew with
// a host font instead of a Harbour face or the DejaVu fallback, with the offending characters.
// Exits 1 if a Bricolage instance is missing or any glyph comes from a host font.
import { forEachBoard } from "./browser.mjs";
import { boardIdsFromArgv, compareBoardIds, runMain } from "./lib.mjs";

async function main() {
  const ids = boardIdsFromArgv();
  const combos = new Map();
  const hostFonts = new Map();
  const missingBricolage = new Map();
  const addBoard = (map, key, init, id) => {
    const entry = map.get(key) ?? { ...init, boards: new Set() };
    entry.boards.add(id);
    map.set(key, entry);
    return entry;
  };
  const options = { concurrency: 4, allowMissingBricolage: true };
  await forEachBoard(ids, options, async ({ id, page, missingBricolage: missing }) => {
    for (const size of missing) addBoard(missingBricolage, size, {}, id);
    for (const run of await auditBoard(page)) {
      addBoard(combos, `${run.family}|${run.size}|${run.weight}`, run, id);
      for (const font of run.platformFonts.filter((f) => !f.isCustomFont)) {
        const init = { familyName: font.familyName, cssFamily: run.family, glyphs: 0, chars: new Set() };
        const host = addBoard(hostFonts, `${font.familyName}|${run.family}`, init, id);
        host.glyphs += font.glyphCount;
        for (const ch of run.hostChars) host.chars.add(ch);
      }
    }
  });

  const boardList = (entry) => [...entry.boards].sort(compareBoardIds).join(" ");
  console.log("family | size px | weight | boards");
  const rows = [...combos.values()].sort(
    (a, b) => a.family.localeCompare(b.family) || a.size - b.size || a.weight - b.weight,
  );
  for (const row of rows) console.log(`${row.family} | ${row.size} | ${row.weight} | ${boardList(row)}`);

  console.log("\nBricolage Grotesque sizes without a static instance (size/weight):");
  if (missingBricolage.size === 0) console.log("  none");
  for (const [key, entry] of missingBricolage) console.log(`  ${key}: ${boardList(entry)}`);

  console.log("\nHost fonts used for text (re-run instance-fonts.sh to extend the fallback subset):");
  if (hostFonts.size === 0) console.log("  none");
  for (const fb of hostFonts.values()) {
    const chars = [...fb.chars].map((c) => `${c} U+${c.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")}`);
    console.log(
      `  ${fb.familyName} (CSS ${fb.cssFamily}): ${fb.glyphs} glyph(s); chars: ${chars.join(", ") || "?"}; ` +
        `boards: ${boardList(fb)}`,
    );
  }
  if (missingBricolage.size > 0 || hostFonts.size > 0) process.exitCode = 1;
}

/** Returns one record per element that directly contains visible text. */
async function auditBoard(page) {
  const runs = await page.evaluate(() => {
    const out = [];
    const walker = document.createTreeWalker(document.querySelector(".board"), NodeFilter.SHOW_ELEMENT);
    for (let el = walker.currentNode; el; el = walker.nextNode()) {
      const text = [...el.childNodes]
        .filter((n) => n.nodeType === Node.TEXT_NODE)
        .map((n) => n.textContent)
        .join("");
      if (!text.trim()) continue;
      const style = getComputedStyle(el);
      if (style.visibility === "hidden" || style.display === "none") continue;
      el.setAttribute("data-font-audit", String(out.length));
      out.push({
        family: style.fontFamily.split(",")[0].replace(/["']/g, "").trim(),
        size: parseFloat(style.fontSize),
        weight: Number(style.fontWeight),
        text,
      });
    }
    return out;
  });

  const cdp = await page.context().newCDPSession(page);
  await cdp.send("DOM.enable");
  await cdp.send("CSS.enable");
  const platformFonts = async (selector) => {
    const { root } = await cdp.send("DOM.getDocument", { depth: 0 });
    const { nodeIds } = await cdp.send("DOM.querySelectorAll", { nodeId: root.nodeId, selector });
    const result = [];
    for (const nodeId of nodeIds) result.push((await cdp.send("CSS.getPlatformFontsForNode", { nodeId })).fonts);
    return result;
  };

  const runFonts = await platformFonts("[data-font-audit]");
  if (runFonts.length !== runs.length) throw new Error("font audit node count mismatch");
  runs.forEach((run, i) => {
    run.platformFonts = runFonts[i];
    run.hostChars = [];
  });

  // Pin down which characters of a host-font run came from the host: render each one on its own
  // with the run's font and ask Chrome which platform font drew it.
  const hostRuns = runs.filter((run) => run.platformFonts.some((font) => !font.isCustomFont));
  if (hostRuns.length) {
    const probes = await page.evaluate((items) => {
      const container = document.createElement("div");
      container.style.cssText = "position:absolute;left:0;top:0;white-space:pre";
      const probes = [];
      items.forEach(({ index, text }) => {
        const el = document.querySelector(`[data-font-audit="${index}"]`);
        const style = getComputedStyle(el);
        for (const ch of new Set(text.replace(/\s/g, ""))) {
          const span = document.createElement("span");
          span.textContent = ch;
          span.style.font = style.font;
          span.style.setProperty("font-family", style.fontFamily, "important");
          span.setAttribute("data-char-probe", String(probes.length));
          container.append(span);
          probes.push({ run: index, ch });
        }
      });
      document.body.append(container);
      return probes;
    }, hostRuns.map((run) => ({ index: runs.indexOf(run), text: run.text })));
    const probeFonts = await platformFonts("[data-char-probe]");
    if (probeFonts.length !== probes.length) throw new Error("font audit probe count mismatch");
    probes.forEach(({ run, ch }, i) => {
      if (probeFonts[i].some((font) => !font.isCustomFont)) runs[run].hostChars.push(ch);
    });
  }
  await cdp.detach();
  return runs;
}

runMain(import.meta, main);
