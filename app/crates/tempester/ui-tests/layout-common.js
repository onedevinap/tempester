/* What the browser tests of the Panel page share (layout-stability.js, layout-overlap.js): the standalone preview
   built from a UI folder (tools/preview_ui.py, a stub bridge, nothing is sent), served on 127.0.0.1 by
   `python3 -m http.server`, and the browsers (Chrome, WebKit through playwright-core).
   env: PLAYWRIGHT_CORE (path of the playwright-core package; default: the npx cache), CHROME (Chrome binary),
        PLAYWRIGHT_BROWSERS_PATH (where WebKit is installed, when HOME is not the real one). */
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn, spawnSync } = require("child_process");

const CORE =
  process.env.PLAYWRIGHT_CORE ||
  path.join(
    os.homedir(),
    ".npm/_npx/9833c18b2d85bc59/node_modules/playwright-core",
  );
const CHROME =
  process.env.CHROME ||
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

function buildPreview(uiDir, workDir, repo) {
  fs.mkdirSync(workDir, { recursive: true });
  const out = path.join(workDir, "index.html");
  const run = spawnSync(
    "python3",
    [
      "-I",
      path.join(repo, "tools/preview_ui.py"),
      out,
      "--ui",
      uiDir,
      "--params",
      path.join(repo, "profile/dsi-tempest/sound_params.json"),
    ],
    { encoding: "utf8" },
  );
  if (run.status !== 0) throw new Error("preview_ui.py: " + run.stderr);
}

function serve(workDir, port) {
  return spawn(
    "python3",
    [
      "-m",
      "http.server",
      String(port),
      "--bind",
      "127.0.0.1",
      "--directory",
      workDir,
    ],
    { stdio: "ignore" },
  );
}

/* Builds the preview, serves it, and gives `run(port)` the address; the server always stops. */
async function withPreview(uiDir, workDir, repo, run) {
  const port = 20000 + Math.floor(Math.random() * 20000);
  buildPreview(uiDir, workDir, repo);
  const server = serve(workDir, port);
  await new Promise((resolve) => setTimeout(resolve, 800));
  try {
    return await run(port);
  } finally {
    server.kill();
  }
}

async function launch(engine) {
  const pw = require(CORE);
  return engine === "webkit"
    ? pw.webkit.launch()
    : pw.chromium.launch({ executablePath: CHROME });
}

/* A page on the Panel tab, in the view asked ("full" or "machine"). */
async function openPanel(browser, port, view, width, height = 1000) {
  const page = await browser.newPage({ viewport: { width, height } });
  await page.goto(`http://127.0.0.1:${port}/index.html#panel`);
  await page.waitForTimeout(400);
  if (view === "machine")
    await page
      .getByRole("group", { name: "Panel view" })
      .getByRole("button", { name: "Machine" })
      .click();
  await page.waitForTimeout(400);
  return page;
}

const receive = (page, message) =>
  page.evaluate((m) => window.Tempester.bridge.receive(m), message);

module.exports = { withPreview, launch, openPanel, receive };
