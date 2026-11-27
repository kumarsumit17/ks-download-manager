// Builds the extension into extension/dist/ (load that folder unpacked in Chrome). Each
// script is bundled on its own as a classic script (content scripts cannot be modules),
// with the Vite already used by the app; no other build tool.
//
//   node extension/build.mjs          the production extension
//   node extension/build.mjs --dev    the DEV extension (extension/dev.json): its own ID, the
//                                     DEV app's native host, " DEV" in its name, DEV icons,
//                                     so it is loaded next to the production one
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "dist");
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

for (const name of ["background", "youtube", "hover", "popup"]) {
  await build({
    configFile: false,
    logLevel: "warn",
    build: {
      outDir: dist,
      emptyOutDir: false,
      minify: false,
      lib: { entry: join(here, "src", `${name}.ts`), formats: ["iife"], name: `ks_${name}`, fileName: () => `${name}.js` },
    },
  });
}
for (const f of ["manifest.json", "popup.html", "popup.css", "_locales", "icons"]) {
  cpSync(join(here, f), join(dist, f), { recursive: true });
}
if (process.argv.includes("--dev")) {
  const dev = JSON.parse(readFileSync(join(here, "dev.json"), "utf8"));
  const PROD_HOST = "com.ks.downloadmanager";
  // The host name, exactly once, in the background script (src/messages.ts HOST).
  const bg = join(dist, "background.js");
  const code = readFileSync(bg, "utf8");
  const quoted = JSON.stringify(PROD_HOST);
  if (code.split(quoted).length !== 2) throw new Error(`expected ${quoted} once in background.js`);
  writeFileSync(bg, code.replace(quoted, JSON.stringify(dev.host)));
  const file = join(dist, "manifest.json");
  const manifest = JSON.parse(readFileSync(file, "utf8"));
  manifest.key = dev.key;
  writeFileSync(file, JSON.stringify(manifest, null, 2) + "\n");
  for (const lang of ["en", "hi"]) {
    const m = join(dist, "_locales", lang, "messages.json");
    const msgs = JSON.parse(readFileSync(m, "utf8"));
    msgs.extName.message += dev.nameSuffix;
    writeFileSync(m, JSON.stringify(msgs, null, 2) + "\n");
  }
  rmSync(join(dist, "icons"), { recursive: true });
  cpSync(join(here, "icons-dev"), join(dist, "icons"), { recursive: true });
  console.log("DEV extension built: extension/dist");
} else {
  console.log("extension built: extension/dist");
}
