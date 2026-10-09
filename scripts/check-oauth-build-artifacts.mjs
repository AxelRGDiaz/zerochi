import { lstat, open, readdir, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const configurationValues = [
  process.env.ZEROCHI_GOOGLE_CLIENT_ID,
  process.env.ZEROCHI_GOOGLE_CLIENT_SECRET,
];
if (configurationValues.some((value) => !value)) {
  console.error("The complete OAuth configuration is required for the artifact scan.");
  process.exit(2);
}

const releaseTarget = process.env.CARGO_TARGET_DIR
  ? path.join(process.env.CARGO_TARGET_DIR, "release")
  : path.join("src-tauri", "target", "release");
const roots = ["dist", releaseTarget];
const needles = configurationValues.map((value) => Buffer.from(value, "utf8"));
const leaks = [];

async function visit(candidate) {
  const metadata = await lstat(candidate);
  if (metadata.isSymbolicLink()) return;
  if (metadata.isDirectory()) {
    for (const entry of await readdir(candidate)) {
      await visit(path.join(candidate, entry));
    }
    return;
  }
  if (!metadata.isFile()) return;

  const handle = await open(candidate, "r");
  try {
    const probe = Buffer.alloc(Math.min(8192, metadata.size));
    await handle.read(probe, 0, probe.length, 0);
    if (probe.includes(0)) return;
  } finally {
    await handle.close();
  }

  const contents = await readFile(candidate);
  if (needles.some((needle) => contents.includes(needle))) {
    leaks.push(path.relative(process.cwd(), candidate));
  }
}

for (const root of roots) {
  try {
    await visit(root);
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
}

if (leaks.length > 0) {
  console.error("OAuth configuration found in textual build artifacts:");
  for (const leak of leaks) console.error(`- ${leak}`);
  process.exit(1);
}

console.log("OAuth textual artifact scan passed.");
