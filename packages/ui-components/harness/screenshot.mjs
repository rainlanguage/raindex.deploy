// Render one harness scene in a real headless Chromium and write a PNG.
//
// This is the driver. It expects `node`, a Chromium binary, and (ideally) a
// font directory to be available; the `screenshot.sh` wrapper provisions those
// via nix. Run it directly if you already have them:
//
//   CHROMIUM=/path/to/chromium \
//   HARNESS_FONT_DIR=/path/to/ttf-dir \
//   node harness/screenshot.mjs --scene toast-error-decoded --out /abs/out.png
//
// Flags:
//   --scene <name>        scene key from scenes/index.ts (required)
//   --out <path>          output PNG path (required)
//   --width <px>          viewport width  (default 900)
//   --height <px>         viewport height (default 360)
//   --port <n>            vite dev port   (default 5544)
//   --virtual-time <ms>   Chromium virtual-time budget, lets mount/transitions
//                         settle before capture (default 6000)

import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { access, mkdir, mkdtemp, stat, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const require = createRequire(import.meta.url);
const harnessDir = fileURLToPath(new URL(".", import.meta.url));
const uiDir = path.resolve(harnessDir, "..");

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i !== -1 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

const scene = arg("scene");
const out = arg("out");
const width = Number(arg("width", "900"));
const height = Number(arg("height", "360"));
const port = Number(arg("port", "5544"));
const virtualTime = Number(arg("virtual-time", "6000"));

if (!scene || !out) {
  console.error(
    "usage: node harness/screenshot.mjs --scene <name> --out <path.png> [--width --height --port]",
  );
  process.exit(2);
}
const outAbs = path.resolve(out);

// --- bootstrap: SvelteKit's generated tsconfig ---------------------------
// vite's esbuild transform of any src/*.ts reads ../tsconfig.json, which
// `extends` ./.svelte-kit/tsconfig.json. That file is a `svelte-kit sync`
// artifact (gitignored). Generate a minimal, resolvable stub when it is absent
// so the harness works from a fresh clone without a full monorepo install.
async function ensureSvelteKitTsconfig() {
  const p = path.join(uiDir, ".svelte-kit", "tsconfig.json");
  if (existsSync(p)) return;
  await mkdir(path.dirname(p), { recursive: true });
  await writeFile(
    p,
    JSON.stringify(
      {
        compilerOptions: {
          target: "es2022",
          module: "es2022",
          moduleResolution: "bundler",
          lib: ["esnext", "DOM", "DOM.Iterable"],
          esModuleInterop: true,
          resolveJsonModule: true,
          skipLibCheck: true,
          baseUrl: "..",
        },
      },
      null,
      "\t",
    ) + "\n",
  );
  console.log("[harness] wrote minimal .svelte-kit/tsconfig.json");
}

function waitForServer(url, timeoutMs = 60000) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const tick = async () => {
      try {
        await fetch(url);
        resolve();
      } catch {
        if (Date.now() > deadline) {
          reject(new Error("vite dev server did not start in time"));
        } else setTimeout(tick, 300);
      }
    };
    tick();
  });
}

async function makeFontconfig() {
  const dir = process.env.HARNESS_FONT_DIR;
  if (!dir) return undefined;
  const tmp = await mkdtemp(path.join(tmpdir(), "harness-fc-"));
  const conf = path.join(tmp, "fonts.conf");
  await writeFile(
    conf,
    `<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig>
  <dir>${dir}</dir>
  <cachedir>${path.join(tmp, "cache")}</cachedir>
  <config></config>
</fontconfig>
`,
  );
  return conf;
}

let vite;
function stopVite() {
  if (vite && !vite.killed) {
    try {
      process.kill(-vite.pid);
    } catch {
      vite.kill("SIGTERM");
    }
  }
}

async function main() {
  await ensureSvelteKitTsconfig();

  // vite's package.json `exports` does not expose ./bin/vite.js, so resolve the
  // package root (package.json is always exported) and join the bin path.
  const viteBin = path.join(
    path.dirname(require.resolve("vite/package.json")),
    "bin",
    "vite.js",
  );
  vite = spawn(
    process.execPath,
    [
      viteBin,
      "--config",
      path.join(harnessDir, "vite.config.ts"),
      "--port",
      String(port),
      "--strictPort",
    ],
    { cwd: uiDir, stdio: "inherit", detached: true },
  );

  const url = `http://localhost:${port}/?scene=${encodeURIComponent(scene)}`;
  await waitForServer(`http://localhost:${port}/`);

  const chromium = process.env.CHROMIUM || "chromium";
  const fontconfig = await makeFontconfig();
  const env = { ...process.env };
  if (fontconfig) env.FONTCONFIG_FILE = fontconfig;

  await mkdir(path.dirname(outAbs), { recursive: true });

  const args = [
    "--headless=new",
    "--disable-gpu",
    "--no-sandbox",
    "--hide-scrollbars",
    "--force-device-scale-factor=2",
    `--virtual-time-budget=${virtualTime}`,
    `--window-size=${width},${height}`,
    `--screenshot=${outAbs}`,
    url,
  ];

  console.log(`[harness] rendering scene "${scene}" -> ${outAbs}`);
  const code = await new Promise((resolve) => {
    const c = spawn(chromium, args, { env, stdio: "inherit" });
    c.on("exit", resolve);
    c.on("error", (e) => {
      console.error(
        `[harness] failed to launch chromium (${chromium}): ${e.message}`,
      );
      resolve(1);
    });
  });

  if (code !== 0) throw new Error(`chromium exited with code ${code}`);
  await access(outAbs);
  const { size } = await stat(outAbs);
  if (size < 1000) {
    throw new Error(`screenshot suspiciously small (${size} bytes)`);
  }
  console.log(`[harness] wrote ${outAbs} (${size} bytes)`);
}

main()
  .then(() => {
    stopVite();
    process.exit(0);
  })
  .catch((err) => {
    console.error(`[harness] ${err.message}`);
    stopVite();
    process.exit(1);
  });
