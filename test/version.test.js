import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { VERSION } from "../extension/lib.js";

test("keeps package and extension versions aligned", async () => {
  const packageJson = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
  const manifest = JSON.parse(await readFile(new URL("../extension/manifest.json", import.meta.url), "utf8"));
  assert.equal(packageJson.version, "0.2.0");
  assert.equal(manifest.version, packageJson.version);
  assert.equal(VERSION, packageJson.version);
});
