// Screenshots of the panel for the README, from the mock backend (app/src/mock.ts) in headless Chrome.
// Run `npm run dev` in app/ first, then: node scripts/screenshots.mjs [out-dir]   (make screenshots does both)
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const URL = "http://localhost:1420/";
const OUT = process.argv[2] ?? "docs/images";
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PORT = 9333;

// The tray panel is 380×640; around it a plain backdrop so its rounded corners and shadow show.
const PANEL = { width: 380, height: 640 };
const MARGIN = 28;

const shots = [
  { name: "projects-dark", scheme: "dark", tab: "Projects" },
  { name: "projects-light", scheme: "light", tab: "Projects" },
  { name: "profiles-dark", scheme: "dark", tab: "Profiles" },
  { name: "settings-dark", scheme: "dark", tab: "Settings" },
];

const profile = mkdtempSync(join(tmpdir(), "ct-shots-"));
const chrome = spawn(CHROME, [
  "--headless=new",
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${profile}`,
  "--hide-scrollbars",
  "--no-first-run",
  "about:blank",
]);

try {
  const target = await retry(async () => {
    const res = await fetch(`http://127.0.0.1:${PORT}/json/new?${URL}`, { method: "PUT" });
    return res.json();
  });
  const cdp = await connect(target.webSocketDebuggerUrl);
  await cdp("Emulation.setDeviceMetricsOverride", {
    width: PANEL.width + 2 * MARGIN,
    height: PANEL.height + 2 * MARGIN,
    deviceScaleFactor: 2,
    mobile: false,
  });
  mkdirSync(OUT, { recursive: true });

  for (const shot of shots) {
    await cdp("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: shot.scheme }] });
    await cdp("Page.navigate", { url: URL });
    await waitFor(cdp, `!!document.querySelector(".row")`);
    await evaluate(cdp, `(() => {
      const backdrop = ${JSON.stringify(shot.scheme === "dark" ? "#0b0f14" : "#dfe4ea")};
      document.documentElement.style.cssText += ";background:" + backdrop + ";padding:${MARGIN}px;box-sizing:border-box";
      document.body.style.cssText += ";background:transparent;width:${PANEL.width}px;height:${PANEL.height}px";
      const tab = [...document.querySelectorAll("button, [role=tab]")].find((b) => b.textContent.trim().toLowerCase() === ${JSON.stringify(shot.tab.toLowerCase())}
        || b.getAttribute("title")?.toLowerCase().startsWith(${JSON.stringify(shot.tab.toLowerCase())}));
      tab?.click();
      document.activeElement?.blur();
    })()`);
    await new Promise((r) => setTimeout(r, 400));
    const { data } = await cdp("Page.captureScreenshot", { format: "png" });
    writeFileSync(join(OUT, `${shot.name}.png`), Buffer.from(data, "base64"));
    console.log(`${OUT}/${shot.name}.png`);
  }
} finally {
  // Chrome keeps writing to its profile until it has exited.
  await new Promise((resolve) => {
    chrome.once("exit", resolve);
    chrome.kill();
  });
  rmSync(profile, { recursive: true, force: true, maxRetries: 5 });
}

async function connect(url) {
  const ws = new WebSocket(url);
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = reject;
  });
  let next = 0;
  const pending = new Map();
  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    const p = pending.get(msg.id);
    if (!p) return;
    pending.delete(msg.id);
    msg.error ? p.reject(new Error(msg.error.message)) : p.resolve(msg.result);
  };
  return (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++next;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });
}

async function evaluate(cdp, expression) {
  const { result, exceptionDetails } = await cdp("Runtime.evaluate", { expression, returnByValue: true });
  if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
  return result.value;
}

async function waitFor(cdp, expression) {
  await retry(async () => {
    if (!(await evaluate(cdp, expression))) throw new Error(`timed out waiting for ${expression}`);
  });
}

async function retry(fn, attempts = 50) {
  for (let i = 1; ; i++) {
    try {
      return await fn();
    } catch (e) {
      if (i >= attempts) throw e;
      await new Promise((r) => setTimeout(r, 200));
    }
  }
}
