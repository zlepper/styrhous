// Offline Chromium page loading for the extracted Harbour boards.
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { chromium } from "@playwright/test";
import {
  bricolageFamily,
  bricolageOpszFamily,
  bricolageOpszSizes,
  fallbackFamily,
  fontFaces,
  readBoardMeta,
  srcDir,
  writeFontsCss,
} from "./lib.mjs";

/**
 * Hinting is disabled so glyph rasterization does not depend on the host's fontconfig defaults and
 * stays closest to egui's unhinted outlines.
 */
function launchBrowser() {
  return chromium.launch({ args: ["--font-render-hinting=none"] });
}

/**
 * Opens one board at its artboard size (deviceScaleFactor 1) with all network access blocked and
 * every Harbour font face loaded, with font stacks rewritten by applyFontStacks(). Throws if a
 * remote request is attempted, a face fails to load, or a Bricolage Grotesque size has no static
 * instance (unless `allowMissingBricolage`; the missing sizes are then returned as
 * `missingBricolage`).
 */
async function openBoard(browser, id, { allowMissingBricolage = false } = {}) {
  const meta = readBoardMeta(id);
  const faces = fontFaces();
  const context = await browser.newContext({
    viewport: { width: meta.width, height: meta.height },
    deviceScaleFactor: 1,
    colorScheme: "dark",
    offline: true,
  });
  const blocked = [];
  await context.route("**/*", (route) => {
    const url = route.request().url();
    if (url.startsWith("file:") || url.startsWith("data:")) return route.continue();
    blocked.push(url);
    return route.abort("blockedbyclient");
  });
  const page = await context.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("requestfailed", (request) => {
    if (!blocked.includes(request.url())) pageErrors.push(`request failed: ${request.url()}`);
  });
  const assertNoNetwork = () => {
    if (blocked.length) throw new Error(`${id}: blocked network request(s): ${blocked.join(", ")}`);
  };

  try {
    await page.goto(pathToFileURL(join(srcDir, `${id}.html`)).href, { waitUntil: "load" });
    const missingBricolage = await applyFontStacks(page);
    if (missingBricolage.length && !allowMissingBricolage) {
      throw new Error(
        `${id}: no static Bricolage Grotesque instance for ${missingBricolage.join(", ")}; ` +
          "add the sizes to instance-fonts.sh",
      );
    }
    await loadAllFonts(page, id, faces);
    // Two frames so data-URI mask images decoded after load are painted.
    await page.evaluate(
      () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
    );
    assertNoNetwork();
    if (pageErrors.length) throw new Error(`${id}: page errors: ${pageErrors.join("; ")}`);
    return { id, meta, page, missingBricolage, assertNoNetwork, close: () => context.close() };
  } catch (error) {
    await context.close();
    throw error;
  }
}

/**
 * Rewrites every board element's font stack to what the app uses:
 * - Chrome renders Bricolage Grotesque with `font-optical-sizing: auto` (opsz = font-size px); the
 *   app embeds one static instance per size, so Bricolage elements point at the instance family
 *   for their own computed size and weight.
 * - The fallback family (the DejaVu Sans subset) goes directly after the primary family, so
 *   glyphs the Harbour fonts lack never come from a host font.
 * Returns the Bricolage "size/weight" pairs without an instance.
 */
async function applyFontStacks(page) {
  return page.evaluate(
    ({ bricolage, sizes, familyFor, fallback }) => {
      const unquote = (name) => name.trim().replace(/^["']|["']$/g, "");
      // Generic families must stay unquoted, or they become (missing) named families.
      const generics = /^(serif|sans-serif|monospace|cursive|fantasy|math|emoji|fangsong|system-ui|ui-(serif|sans-serif|monospace|rounded))$/;
      const css = (name) => (generics.test(name) ? name : `"${name}"`);
      const missing = new Set();
      // Every computed style is read before any element changes, because children inherit.
      const updates = [...document.querySelectorAll(".board, .board *")].map((el) => {
        const style = getComputedStyle(el);
        const [primary, ...rest] = style.fontFamily.split(",").map(unquote);
        let family = primary;
        if (primary === bricolage) {
          const size = parseFloat(style.fontSize);
          const opsz = Math.min(96, Math.max(12, size));
          if (style.fontWeight === "700" && sizes.includes(opsz)) family = familyFor.replace("{opsz}", opsz);
          else missing.add(`${size}px/${style.fontWeight}`);
        }
        const stack = [family, fallback, ...rest.filter((name) => name !== fallback)];
        return { el, value: stack.map(css).join(", ") };
      });
      for (const { el, value } of updates) el.style.setProperty("font-family", value, "important");
      return [...missing];
    },
    {
      bricolage: bricolageFamily,
      sizes: bricolageOpszSizes(),
      familyFor: bricolageOpszFamily("{opsz}"),
      fallback: fallbackFamily,
    },
  );
}

/**
 * Loads every declared face, not only the ones this board uses, so a missing or broken font file
 * fails every render instead of silently falling back.
 */
async function loadAllFonts(page, id, faces) {
  const report = await page.evaluate(async (expected) => {
    const declared = [...document.fonts];
    const results = await Promise.allSettled(declared.map((face) => face.load()));
    await document.fonts.ready;
    const failed = declared
      .filter((face, i) => results[i].status === "rejected" || face.status !== "loaded")
      .map((face) => `${face.family} ${face.weight}`);
    const unchecked = expected
      .filter((f) => !document.fonts.check(`${f.checkWeight ?? f.weight} 16px "${f.family}"`))
      .map((f) => `${f.family} ${f.weight}`);
    return { declared: declared.length, failed, unchecked };
  }, faces);
  if (report.declared !== faces.length) {
    throw new Error(`${id}: expected ${faces.length} @font-face rules, found ${report.declared}`);
  }
  const failed = [...report.failed, ...report.unchecked];
  if (failed.length) throw new Error(`${id}: fonts failed to load: ${failed.join(", ")}`);
}

/**
 * Regenerates fonts.css, then opens each board (see openBoard; `openOptions` are passed through)
 * in one shared browser with at most `concurrency` boards in flight and runs `task(board)`. Each
 * board is closed afterwards; failures are reported per board and rethrown together at the end.
 */
export async function forEachBoard(ids, { concurrency = 4, ...openOptions }, task) {
  writeFontsCss();
  const browser = await launchBrowser();
  try {
    await forEachLimited(ids, concurrency, async (id) => {
      const board = await openBoard(browser, id, openOptions);
      try {
        await task(board);
      } finally {
        await board.close();
      }
    });
  } finally {
    await browser.close();
  }
}

/** Runs `task` for each item with at most `limit` in flight; rethrows the collected failures. */
async function forEachLimited(items, limit, task) {
  const failures = [];
  let next = 0;
  const worker = async () => {
    while (next < items.length) {
      const item = items[next++];
      try {
        await task(item);
      } catch (error) {
        failures.push(error);
        console.error(`FAIL ${item}: ${error.message}`);
      }
    }
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  if (failures.length) throw new Error(`${failures.length} of ${items.length} board(s) failed`);
}
