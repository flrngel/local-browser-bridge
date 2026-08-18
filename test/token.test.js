import assert from "node:assert/strict";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { loadOrCreateToken, tokensEqual } from "../src/token.js";

test("creates and reuses a persisted token", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lbb-token-"));
  const path = join(directory, "nested", "token");
  const first = await loadOrCreateToken(path);
  const second = await loadOrCreateToken(path);
  assert.equal(first, second);
  assert.equal((await readFile(path, "utf8")).trim(), first);
  assert.equal(first.length >= 40, true);
});

test("compares tokens without accepting length or content mismatches", () => {
  assert.equal(tokensEqual("abc", "abc"), true);
  assert.equal(tokensEqual("abc", "abd"), false);
  assert.equal(tokensEqual("abc", "abcd"), false);
});
