export const VERSION = "0.8.0";
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

export function isUrlAllowed(rawUrl, allowedHosts, bridgePort = DEFAULT_PORT, fullAccess = false) {
  if (rawUrl === "about:blank") return { allowed: true, url: rawUrl };
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

export function isSensitiveField({ type = "", autocomplete = "", name = "" } = {}) {
  return type === "password" || /password|one-time-code|cc-number|cc-csc|current-password|new-password/i.test(`${autocomplete} ${name}`);
}

export function allowedKey(key) {
  return ["Tab", "Enter", "Escape", "Backspace", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PageUp", "PageDown", "Home", "End"].includes(key);
}
