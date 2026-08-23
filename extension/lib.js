export const VERSION = "0.12.8";
export const PROTOCOL_VERSION = 1;
export const DEFAULT_PORT = 17_373;

const RISK_PATTERNS = [
  ["delete or remove data", /\b(delete|remove|erase|destroy|cancel subscription|close account)\b/i],
  ["send or publish information", /\b(send|submit|publish|post|comment|reply|share|upload|invite)\b/i],
  ["purchase or payment", /\b(buy|purchase|pay|checkout|place order|confirm order|subscribe)\b/i],
  ["account or permission change", /\b(change password|reset password|grant access|permission|make admin|create account|sign up)\b/i],
];

export function normalizeAllowedHost(value) {
  if (typeof value !== "string") return null;
  let host = value.trim().toLowerCase();
  if (!host) return null;
  if (host === "*") return "*";
  if (host.startsWith("*.")) {
    const suffix = host.slice(2);
    return validHostname(suffix) ? `*.${suffix}` : null;
  }
  try {
    if (host.includes("://")) host = new URL(host).hostname.toLowerCase();
  } catch {
    return null;
  }
  return validHostname(host) ? host : null;
}

function validHostname(host) {
  return host === "localhost" || /^([a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)*[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(host) || /^\d{1,3}(?:\.\d{1,3}){3}$/.test(host);
}

export function hostAllowed(hostname, allowedHosts) {
  const host = String(hostname ?? "").toLowerCase();
  return allowedHosts.some((entry) => {
    if (entry === "*") return true;
    if (entry.startsWith("*.")) {
      const suffix = entry.slice(2);
      return host === suffix || host.endsWith(`.${suffix}`);
    }
    return host === entry;
  });
}

export function isUrlAllowed(rawUrl, allowedHosts, bridgePort = DEFAULT_PORT, fullAccess = false, trustedBlank = false) {
  if (rawUrl === "about:blank") {
    return fullAccess || trustedBlank
      ? { allowed: true, url: rawUrl }
      : { allowed: false, reason: "Untracked blank tabs are blocked in Safe mode" };
  }
  let url;
  try {
    url = new URL(rawUrl);
  } catch {
    return { allowed: false, reason: "Invalid URL" };
  }
  if (url.protocol !== "http:" && url.protocol !== "https:" && !(fullAccess && url.protocol === "file:")) {
    return { allowed: false, reason: fullAccess ? "Browser-internal pages are not controllable" : "Only HTTP and HTTPS pages are controllable" };
  }
  const bridgeOrigin = (url.hostname === "127.0.0.1" || url.hostname === "localhost")
    && Number(url.port || (url.protocol === "https:" ? 443 : 80)) === Number(bridgePort);
  if (bridgeOrigin && url.pathname !== "/demo") {
    return { allowed: false, reason: "The bridge cannot control its own control surface" };
  }
  if (!fullAccess && !hostAllowed(url.hostname, allowedHosts)) {
    return { allowed: false, reason: `${url.hostname} is not in the extension allowlist` };
  }
  return { allowed: true, url: url.href };
}

export function safeUrlForDisplay(rawUrl) {
  if (rawUrl === "about:blank") return rawUrl;
  try {
    const url = new URL(rawUrl);
    return url.protocol === "file:" ? `file://${url.pathname}` : `${url.origin}${url.pathname}`;
  } catch {
    return "unavailable";
  }
}

export function classifyRisk({ name = "", text = "", role = "", type = "" } = {}) {
  const haystack = `${name} ${text} ${role} ${type}`.slice(0, 1_000);
  for (const [risk, pattern] of RISK_PATTERNS) {
    if (pattern.test(haystack)) return risk;
  }
  return null;
}

function normalizedFieldIdentifier(value) {
  return String(value ?? "")
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function isSensitiveField({ type = "", autocomplete = "", name = "", fieldName = "" } = {}) {
  if (String(type).toLowerCase() === "password") return true;
  const autocompleteTokens = String(autocomplete).toLowerCase().split(/\s+/).filter(Boolean);
  if (autocompleteTokens.some((token) => token === "current-password"
    || token === "new-password"
    || token === "one-time-code"
    || token.startsWith("cc-"))) return true;
  const identifier = normalizedFieldIdentifier(name || fieldName);
  return /(?:^|-)(?:password|passwd|one-time-(?:code|password)|otp|passcode|verification-code|cc-(?:name|given-name|additional-name|family-name|number|exp|exp-month|exp-year|csc|type)|card-number|credit-card-number|payment-card-number|cardholder(?:-name)?|card-name|card-type|card-exp(?:iry|iration)?|card-exp-(?:month|year)|exp-(?:month|year)|cvv2?|cvc2?|card-security-code|security-code)(?:$|-)/.test(identifier);
}

export function allowedKey(key) {
  return ["Tab", "Enter", "Escape", "Backspace", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PageUp", "PageDown", "Home", "End"].includes(key);
}
