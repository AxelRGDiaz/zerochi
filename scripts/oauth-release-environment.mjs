import { lstatSync } from "node:fs";
import { loadEnvFile } from "node:process";
import path from "node:path";

const CLIENT_ID_ENV = "ZEROCHI_GOOGLE_CLIENT_ID";
const CLIENT_SECRET_ENV = "ZEROCHI_GOOGLE_CLIENT_SECRET";
const MAX_DOTENV_BYTES = 64 * 1024;

function hasValue(environment, name) {
  return typeof environment[name] === "string" && environment[name].length > 0;
}

function isPresent(environment, name) {
  return Object.prototype.hasOwnProperty.call(environment, name);
}

function loadApprovedDotenvValues(dotenvPath) {
  const originalEnvironment = { ...process.env };
  try {
    loadEnvFile(dotenvPath);
    return {
      [CLIENT_ID_ENV]: process.env[CLIENT_ID_ENV],
      [CLIENT_SECRET_ENV]: process.env[CLIENT_SECRET_ENV],
    };
  } finally {
    for (const name of Object.keys(process.env)) {
      if (!Object.prototype.hasOwnProperty.call(originalEnvironment, name)) {
        delete process.env[name];
      }
    }
    Object.assign(process.env, originalEnvironment);
  }
}

export function loadOAuthReleaseEnvironment({
  environment = process.env,
  cwd = process.cwd(),
  statFile = lstatSync,
  loadFile = loadApprovedDotenvValues,
} = {}) {
  const hasClientId = isPresent(environment, CLIENT_ID_ENV);
  const hasClientSecret = isPresent(environment, CLIENT_SECRET_ENV);

  if (hasClientId !== hasClientSecret) {
    throw new Error(
      "OAuth release configuration is incomplete; provide both variables through the same source.",
    );
  }

  if (hasClientId && hasClientSecret) {
    if (!hasValue(environment, CLIENT_ID_ENV) || !hasValue(environment, CLIENT_SECRET_ENV)) {
      throw new Error("OAuth release process variables must both be non-empty.");
    }
    return "process environment";
  }

  const dotenvPath = path.resolve(cwd, ".env");
  let metadata;
  try {
    metadata = statFile(dotenvPath);
  } catch {
    throw new Error(
      "OAuth release configuration is missing; export both variables or create the ignored root .env.",
    );
  }

  if (metadata.isSymbolicLink() || !metadata.isFile()) {
    throw new Error("The root .env must be a regular local file, not a symlink.");
  }
  if (metadata.size > MAX_DOTENV_BYTES) {
    throw new Error("The root .env is too large to use as release configuration.");
  }

  try {
    const loaded = loadFile(dotenvPath);
    environment[CLIENT_ID_ENV] = loaded?.[CLIENT_ID_ENV];
    environment[CLIENT_SECRET_ENV] = loaded?.[CLIENT_SECRET_ENV];
  } catch {
    throw new Error("The root .env could not be parsed for the release build.");
  }

  if (!hasValue(environment, CLIENT_ID_ENV) || !hasValue(environment, CLIENT_SECRET_ENV)) {
    throw new Error(
      "The root .env must define both ZEROCHI_GOOGLE_CLIENT_ID and ZEROCHI_GOOGLE_CLIENT_SECRET.",
    );
  }

  return "ignored root .env";
}
