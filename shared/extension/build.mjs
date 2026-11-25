// Builds the extension into extension/dist/ (load that folder unpacked in Chrome). Each
// script is bundled on its own as a classic script (content scripts cannot be modules),
// with the Vite already used by the app; no other build tool.
import { cpSync, mkdirSync, rmSync } from "node:fs";
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
console.log("extension built: extension/dist");
