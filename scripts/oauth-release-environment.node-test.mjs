import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";

import { loadOAuthReleaseEnvironment } from "./oauth-release-environment.mjs";

const regularFile = {
  isFile: () => true,
  isSymbolicLink: () => false,
  size: 128,
};

test("a complete process pair takes precedence without loading dotenv", () => {
  const environment = {
    ZEROCHI_GOOGLE_CLIENT_ID: "process-id",
    ZEROCHI_GOOGLE_CLIENT_SECRET: "process-credential",
  };
  let loaded = false;

  const source = loadOAuthReleaseEnvironment({
    environment,
    loadFile: () => {
      loaded = true;
    },
  });

  assert.equal(source, "process environment");
  assert.equal(loaded, false);
});

test("an ignored root dotenv supplies the complete local release pair", () => {
  const environment = {};
  let loadedPath;

  const source = loadOAuthReleaseEnvironment({
    environment,
    cwd: "/workspace/zerochi",
    statFile: () => regularFile,
    loadFile: (dotenvPath) => {
      loadedPath = dotenvPath;
      return {
        ZEROCHI_GOOGLE_CLIENT_ID: "dotenv-id",
        ZEROCHI_GOOGLE_CLIENT_SECRET: "dotenv-credential",
      };
    },
  });

  assert.equal(source, "ignored root .env");
  assert.equal(loadedPath, path.resolve("/workspace/zerochi", ".env"));
});

test("empty process variables are rejected without falling back to dotenv", () => {
  const partialCases = [
    { ZEROCHI_GOOGLE_CLIENT_ID: "" },
    { ZEROCHI_GOOGLE_CLIENT_SECRET: "" },
    { ZEROCHI_GOOGLE_CLIENT_ID: "", ZEROCHI_GOOGLE_CLIENT_SECRET: "" },
  ];

  for (const environment of partialCases) {
    let loaded = false;
    assert.throws(
      () =>
        loadOAuthReleaseEnvironment({
          environment,
          loadFile: () => {
            loaded = true;
          },
        }),
      /incomplete|non-empty/,
    );
    assert.equal(loaded, false);
  }
});

test("a partial process pair fails instead of mixing process and dotenv", () => {
  const environment = { ZEROCHI_GOOGLE_CLIENT_ID: "process-id" };
  let loaded = false;

  assert.throws(
    () =>
      loadOAuthReleaseEnvironment({
        environment,
        loadFile: () => {
          loaded = true;
        },
      }),
    /provide both variables through the same source/,
  );
  assert.equal(loaded, false);
});

test("a missing or incomplete dotenv fails with a fixed diagnostic", () => {
  assert.throws(
    () => loadOAuthReleaseEnvironment({ environment: {}, statFile: () => { throw new Error(); } }),
    /export both variables or create the ignored root \.env/,
  );

  assert.throws(
    () =>
      loadOAuthReleaseEnvironment({
        environment: {},
        statFile: () => regularFile,
        loadFile: () => ({}),
      }),
    /must define both ZEROCHI_GOOGLE_CLIENT_ID and ZEROCHI_GOOGLE_CLIENT_SECRET/,
  );
});

test("dotenv must be a bounded regular non-symlink file", () => {
  assert.throws(
    () =>
      loadOAuthReleaseEnvironment({
        environment: {},
        statFile: () => ({ ...regularFile, isSymbolicLink: () => true }),
      }),
    /regular local file/,
  );

  assert.throws(
    () =>
      loadOAuthReleaseEnvironment({
        environment: {},
        statFile: () => ({ ...regularFile, size: 64 * 1024 + 1 }),
      }),
    /too large/,
  );
});
