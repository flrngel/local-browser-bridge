import assert from "node:assert/strict";
import test from "node:test";
import {
  allowedKey,
  classifyRisk,
  hostAllowed,
  isSensitiveField,
  isUrlAllowed,
  normalizeAllowedHost,
  safeUrlForDisplay,
} from "../extension/lib.js";

test("normalizes exact and wildcard hosts", () => {
  assert.equal(normalizeAllowedHost("HTTPS://Example.COM/path"), "example.com");
  assert.equal(normalizeAllowedHost("*.Example.com"), "*.example.com");
  assert.equal(normalizeAllowedHost("not a host"), null);
});

test("matches hosts without substring confusion", () => {
  assert.equal(hostAllowed("app.example.com", ["*.example.com"]), true);
  assert.equal(hostAllowed("example.com", ["*.example.com"]), true);
  assert.equal(hostAllowed("example.com.evil.test", ["example.com"]), false);
});

test("blocks unsupported, unapproved, and bridge-self URLs", () => {
  assert.equal(isUrlAllowed("file:///tmp/a", ["*"], 17_373).allowed, false);
  assert.equal(isUrlAllowed("https://safe.example/path", ["safe.example"], 17_373).allowed, true);
  assert.equal(isUrlAllowed("https://other.example/path", ["safe.example"], 17_373).allowed, false);
  assert.equal(isUrlAllowed("http://127.0.0.1:17373", ["127.0.0.1"], 17_373).allowed, false);
  assert.equal(isUrlAllowed("http://127.0.0.1:3000", ["127.0.0.1"], 17_373).allowed, true);
});

test("redacts query strings and fragments from displayed URLs", () => {
  assert.equal(safeUrlForDisplay("https://example.com/path?token=secret#section"), "https://example.com/path");
});

test("flags risky clicks and sensitive fields", () => {
  assert.equal(classifyRisk({ role: "button", name: "Delete account" }), "delete or remove data");
  assert.equal(classifyRisk({ role: "button", name: "Continue" }), null);
  assert.equal(isSensitiveField({ type: "password" }), true);
  assert.equal(isSensitiveField({ autocomplete: "one-time-code" }), true);
  assert.equal(isSensitiveField({ type: "text", name: "nickname" }), false);
});

test("only permits a fixed key vocabulary", () => {
  assert.equal(allowedKey("Enter"), true);
  assert.equal(allowedKey("Control+L"), false);
  assert.equal(allowedKey("A"), false);
});
