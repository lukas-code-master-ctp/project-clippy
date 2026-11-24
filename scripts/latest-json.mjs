// Arma latest.json, el archivo que Brote lee para saber si hay una versión nueva.
// Uso: node scripts/latest-json.mjs <versión> <url base de descarga>
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [version, baseUrl] = process.argv.slice(2);
const bundle = "src-tauri/target/release/bundle";

function entry(dir, suffix) {
  const file = readdirSync(join(bundle, dir)).find((f) => f.endsWith(suffix));
  if (!file) throw new Error(`no encontré *${suffix} en ${dir}`);
  return {
    signature: readFileSync(join(bundle, dir, `${file}.sig`), "utf8").trim(),
    url: `${baseUrl}/${encodeURIComponent(file)}`,
  };
}

const nsis = entry("nsis", "-setup.exe");
const msi = entry("msi", ".msi");
const latest = {
  version,
  notes: `Brote ${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": nsis,
    "windows-x86_64-nsis": nsis,
    "windows-x86_64-msi": msi,
  },
};
writeFileSync("latest.json", JSON.stringify(latest, null, 2));
console.log(JSON.stringify(latest, null, 2));
