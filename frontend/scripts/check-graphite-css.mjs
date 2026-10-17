import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../src");
const frozen = path.join(root, "features/project0/styles/graphite.css");
const tokenFile = path.join(root, "shared/ui/graphite/tokens.css");
const errors = [];
function* walk(dir) {
  for (const ent of readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) yield* walk(p);
    else if (/\.(?:css|tsx)$/.test(ent.name)) yield p;
  }
}
for (const file of walk(root)) {
  if (file === frozen || file === tokenFile) continue;
  const text = readFileSync(file, "utf8");
  const rel = path.relative(root, file).replaceAll(path.sep, "/");
  if (file.endsWith(".css") &&
     (rel.startsWith("features/") || rel.startsWith("shared/ui/"))) {
    if (/#[a-fA-F0-9]{3,8}\b/.test(text)) errors.push(rel + ": raw hex in feature/shared CSS");
    if (/\b(?:red|blue|green|white|black)\s*[;}]/i.test(text))
      errors.push(rel + ": named CSS color");
  }
  if (file.endsWith(".tsx") && !rel.startsWith("features/project0/") &&
      /(?:bg-|text-|border-)\[#[\da-fA-F]{3,8}\]/.test(text))
    errors.push(rel + ": arbitrary Tailwind hex; use Graphite token");
}
if (errors.length) {
  for (const msg of errors) console.error(msg);
  process.exitCode = 1;
} else console.log("Graphite CSS token boundaries: PASS");
