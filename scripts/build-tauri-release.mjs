import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

import { loadOAuthReleaseEnvironment } from "./oauth-release-environment.mjs";

let oauthConfigurationSource;
try {
  oauthConfigurationSource = loadOAuthReleaseEnvironment();
} catch (error) {
  console.error(error instanceof Error ? error.message : "OAuth release configuration failed.");
  process.exit(2);
}

console.log(`OAuth release configuration loaded from ${oauthConfigurationSource}.`);

const target = await mkdtemp(path.join(tmpdir(), "zerochi-release-"));
const environment = {
  ...process.env,
  CARGO_TARGET_DIR: target,
  ZEROCHI_FRESH_RELEASE_BUILD: "1",
};

function run(command, args) {
  return spawnSync(command, args, {
    cwd: process.cwd(),
    env: environment,
    stdio: "inherit",
  }).status ?? 1;
}

if (run("npm", ["run", "tauri", "build"]) !== 0) {
  await rm(target, { recursive: true, force: true });
  process.exit(1);
}

if (run(process.execPath, ["scripts/check-oauth-build-artifacts.mjs"]) !== 0) {
  await rm(target, { recursive: true, force: true });
  process.exit(1);
}

console.log(`Release bundle ready at: ${path.join(target, "release", "bundle")}`);
console.log("Upload only the final app/installer; do not cache the temporary Cargo target.");
