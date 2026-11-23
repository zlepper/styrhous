// Renders the extracted Harbour boards into docs/design-oracles/harbour/<ID>.png.
//
// Usage: npm run render [-- ID ...]   (no IDs renders every board)
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { forEachBoard } from "./browser.mjs";
import { boardIdsFromArgv, harbourDir, runMain } from "./lib.mjs";

const maxAttempts = 5;

async function main() {
  const ids = boardIdsFromArgv();
  const started = Date.now();
  await forEachBoard(ids, { concurrency: 4 }, async (board) => {
    const png = await stableScreenshot(board);
    board.assertNoNetwork();
    writeFileSync(join(harbourDir, `${board.id}.png`), png);
    console.log(`rendered ${board.id} (${board.meta.width}x${board.meta.height})`);
  });
  console.log(`Rendered ${ids.length} board(s) in ${((Date.now() - started) / 1000).toFixed(1)}s`);
}

/**
 * Screenshots the board until two consecutive captures are byte-identical.
 *
 * Uses CDP `captureBeyondViewport`, which re-rasterizes the whole page synchronously for the
 * capture. Playwright's regular screenshot reuses the compositor's tiles, whose antialiasing of
 * mask images and rounded box-shadows depends on raster timing and process-wide Skia caches, so
 * identical boards came out a few pixels different between runs.
 */
async function stableScreenshot({ page, meta }) {
  const box = await page.locator("body > .board").boundingBox();
  if (!box || box.x !== 0 || box.y !== 0 || box.width !== meta.width || box.height !== meta.height) {
    throw new Error(
      `${meta.id}: .board box ${JSON.stringify(box)} does not match ${meta.width}x${meta.height} at 0,0`,
    );
  }
  const cdp = await page.context().newCDPSession(page);
  const capture = async () => {
    const { data } = await cdp.send("Page.captureScreenshot", {
      format: "png",
      captureBeyondViewport: true,
      clip: { x: 0, y: 0, width: meta.width, height: meta.height, scale: 1 },
    });
    return Buffer.from(data, "base64");
  };
  try {
    let previous = await capture();
    for (let attempt = 1; attempt < maxAttempts; attempt++) {
      const current = await capture();
      if (current.equals(previous)) return current;
      previous = current;
    }
  } finally {
    await cdp.detach();
  }
  throw new Error(`${meta.id}: rendering did not stabilise after ${maxAttempts} screenshots`);
}

runMain(import.meta, main);
