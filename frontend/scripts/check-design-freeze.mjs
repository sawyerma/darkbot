import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Compares committed bytes, not just colors. Only the signed-off baseline owner
// may update the manifest after a separate screenshot/visual approval.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const filePath = path.join(root, "design", "reference", "FREEZE_SHA256");
const manifest = readFileSync(filePath, "utf8");
const entries = manifest.split(/\r?\n/).map(x => x.trim()).filter(x => x && !x.startsWith("#"));
if (entries.length !== 3) throw Error("Freeze manifest must contain exactly 3 files.");
let failures = 0;
const seen = new Set();
for (const entry of entries) {
  const match = /^([a-f0-9]{64}) {2}(.+)$/.exec(entry);
  if (!match) throw Error("Invalid manifest line: " + entry);
  const [, expected, relative] = match;
  if (seen.has(relative)) throw Error("Duplicate freeze path: " + relative);
  seen.add(relative);
  const target = path.resolve(root, relative);
  if (!target.startsWith(root + path.sep)) throw Error("Freeze path outside frontend/");
  const actual = createHash("sha256").update(readFileSync(target)).digest("hex");
  if (actual !== expected) {
    console.error("FROZEN FILE CHANGED:", relative, "expected", expected, "actual", actual);
    failures++;
  } else console.log("freeze OK:", relative);
}
process.exitCode = failures ? 1 : 0;
