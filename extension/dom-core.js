// Shared DOM observation and target-proof core.
//
// This file is the single source of truth for how the bridge describes an
// element, proves that a target has not changed, and decides that a snapshot
// is stale. It is loaded twice with different hosts:
//
//   * `content.js` runs it in the top document's content-script isolated
//     world;
//   * `frame-agent.js` runs it inside a cross-origin subframe, in a dedicated
//     CDP isolated world.
//
// The bridge injects nothing into the page, so every element the core sees is
// page-owned and observable: there is no exclusion predicate.
//
// Nothing here reaches the extension messaging surface, dispatches an event,
// activates a control, or writes a value: it is a read-only description and
// proof library. Every
// error string is part of the published protocol and is asserted verbatim by
// tests/extension_contract.rs.
//
// Top-level declarations are functions only, so re-injecting the file into a
// world that already ran it can never throw a redeclaration error.

function createDomCore() {
  const GEOMETRY_TOLERANCE_PX = 2;

  const candidateSelector = [
    "a[href]", "button", "input", "textarea", "select", "summary", "details", "[contenteditable='true']",
    "[role='button']", "[role='link']", "[role='checkbox']", "[role='radio']", "[role='switch']", "[role='tab']",
    "[role='menuitem']", "[role='option']", "[tabindex]:not([tabindex='-1'])", "[onclick]",
  ].join(",");

  function clean(value, max = 300) {
    return String(value ?? "").replace(/[\u0000-\u001f\u007f]+/g, " ").replace(/\s+/g, " ").trim().slice(0, max);
  }

  function normalizedFieldIdentifier(value) {
    return String(value ?? "")
      .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "");
  }

  function isSensitiveFieldMetadata({ type = "", autocomplete = "", name = "" } = {}) {
    if (String(type).toLowerCase() === "password") return true;
    const autocompleteTokens = String(autocomplete).toLowerCase().split(/\s+/).filter(Boolean);
    if (autocompleteTokens.some((token) => token === "current-password"
      || token === "new-password"
      || token === "one-time-code"
      || token.startsWith("cc-"))) return true;
    const identifier = normalizedFieldIdentifier(name);
    return /(?:^|-)(?:password|passwd|one-time-(?:code|password)|otp|passcode|verification-code|cc-(?:name|given-name|additional-name|family-name|number|exp|exp-month|exp-year|csc|type)|card-number|credit-card-number|payment-card-number|cardholder(?:-name)?|card-name|card-type|card-exp(?:iry|iration)?|card-exp-(?:month|year)|exp-(?:month|year)|cvv2?|cvc2?|card-security-code|security-code)(?:$|-)/.test(identifier);
  }

  function visible(element) {
    if (!(element instanceof Element)) return false;
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.display !== "none" && style.visibility !== "hidden" && Number(style.opacity) !== 0 && rect.width > 1 && rect.height > 1;
  }

  function labelledBy(element) {
    const ids = clean(element.getAttribute("aria-labelledby"), 500).split(" ").filter(Boolean);
    const root = element.getRootNode();
    return ids.map((id) => clean(root.getElementById?.(id)?.textContent || document.getElementById(id)?.textContent)).filter(Boolean).join(" ");
  }

  function accessibleName(element) {
    const direct = clean(element.getAttribute("aria-label"));
    if (direct) return direct;
    const referenced = labelledBy(element);
    if (referenced) return referenced;
    if (element.labels?.length) {
      const label = clean([...element.labels].map((item) => item.innerText || item.textContent).join(" "));
      if (label) return label;
    }
    const safeInputValue = element instanceof HTMLInputElement
      && ["button", "submit", "reset"].includes(element.type)
      ? element.value
      : "";
    const safeElementText = element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement
      ? ""
      : element.innerText || element.textContent;
    return clean(
      element.alt || element.title || element.placeholder ||
      safeInputValue ||
      safeElementText || element.getAttribute("name") || element.id,
    );
  }

  function roleOf(element) {
    const explicit = clean(element.getAttribute("role"), 60);
    if (explicit) return explicit;
    const tag = element.tagName.toLowerCase();
    if (tag === "a") return "link";
    if (tag === "button" || tag === "summary") return "button";
    if (tag === "select") return "select";
    if (tag === "textarea") return "textbox";
    if (tag === "input") {
      if (["checkbox", "radio", "button", "submit", "reset"].includes(element.type)) return element.type;
      return "textbox";
    }
    return tag;
  }

  function boundsOf(element) {
    const rect = element.getBoundingClientRect();
    return {
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    };
  }

  function describe(element, ref) {
    const rect = element.getBoundingClientRect();
    const type = clean(element.getAttribute("type"), 60).toLowerCase();
    const autocomplete = clean(element.getAttribute("autocomplete"), 100).toLowerCase();
    const fieldName = clean(element.getAttribute("name"), 100);
    const sensitive = isSensitiveFieldMetadata({ type, autocomplete, name: fieldName });
    return {
      ref,
      role: roleOf(element),
      name: accessibleName(element),
      type,
      autocomplete,
      fieldName,
      disabled: Boolean(element.disabled) || element.getAttribute("aria-disabled") === "true",
      checked: "checked" in element ? Boolean(element.checked) : undefined,
      selected: "selected" in element ? Boolean(element.selected) : undefined,
      sensitive,
      inViewport: rect.bottom > 0 && rect.right > 0 && rect.top < innerHeight && rect.left < innerWidth,
      bounds: boundsOf(element),
      tree: element.getRootNode() instanceof ShadowRoot ? "shadow" : "document",
    };
  }

  function targetSignature(element) {
    const href = element instanceof HTMLAnchorElement ? clean(element.href, 500) : "";
    return [
      element.tagName.toLowerCase(), roleOf(element), accessibleName(element),
      clean(element.getAttribute("type"), 60).toLowerCase(),
      clean(element.getAttribute("name"), 100),
      clean(element.getAttribute("aria-disabled"), 10),
      clean(element.getAttribute("aria-checked"), 10),
      href,
    ].join("\u001f");
  }

  function sameBounds(left, right) {
    return ["x", "y", "width", "height"].every((key) => Math.abs(Number(left[key]) - Number(right[key])) <= GEOMETRY_TOLERANCE_PX);
  }

  function composedContains(ancestor, candidate) {
    let current = candidate;
    while (current) {
      if (current === ancestor) return true;
      current = current.assignedSlot || current.parentElement || current.getRootNode?.().host || null;
    }
    return false;
  }

  function deepElementFromPoint(x, y) {
    let element = document.elementFromPoint(x, y);
    while (element?.shadowRoot) {
      const deeper = element.shadowRoot.elementFromPoint(x, y);
      if (!deeper || deeper === element) break;
      element = deeper;
    }
    return element;
  }

  function pointTarget(x, y) {
    if (!Number.isFinite(x) || !Number.isFinite(y) || x < 0 || y < 0 || x >= innerWidth || y >= innerHeight) {
      throw new Error("BAD_COORDINATES: target point is outside the current viewport");
    }
    const element = deepElementFromPoint(x, y);
    if (!(element instanceof Element)) throw new Error("TARGET_MISSING: no page element is available at that point");
    return element;
  }

  function validateRecord(record, { requireHitTest = false } = {}) {
    const { element } = record;
    if (!visible(element)) throw new Error("TARGET_CHANGED: target is no longer visible; observe again");
    const currentSignature = targetSignature(element);
    const currentBounds = boundsOf(element);
    if (currentSignature !== record.signature) throw new Error("TARGET_CHANGED: target identity changed; observe again");
    if (!sameBounds(currentBounds, record.bounds)) throw new Error("TARGET_CHANGED: target geometry changed; observe again");
    const description = describe(element, record.ref);
    if (requireHitTest) {
      const x = currentBounds.x + currentBounds.width / 2;
      const y = currentBounds.y + currentBounds.height / 2;
      if (!description.inViewport) throw new Error("TARGET_OUT_OF_VIEWPORT: scroll, then observe the page again");
      const hit = deepElementFromPoint(x, y);
      if (!hit || (!composedContains(element, hit) && !composedContains(hit, element))) {
        throw new Error("TARGET_OCCLUDED: another element covers the observed target; observe again");
      }
    }
    return {
      description,
      proof: { signature: record.signature, bounds: record.bounds },
    };
  }

  function compareProof(actual, expected) {
    if (!expected || actual.signature !== expected.signature || !sameBounds(actual.bounds, expected.bounds)) {
      throw new Error("TARGET_CHANGED: target proof no longer matches; observe again");
    }
  }

  function composedCandidates(root = document) {
    const found = [];
    const seen = new Set();
    const visit = (scope) => {
      for (const element of scope.querySelectorAll(candidateSelector)) {
        if (!seen.has(element)) {
          seen.add(element);
          found.push(element);
        }
      }
      for (const element of scope.querySelectorAll("*")) {
        if (element.shadowRoot) visit(element.shadowRoot);
      }
    };
    visit(root);
    return found;
  }

  return {
    GEOMETRY_TOLERANCE_PX,
    candidateSelector,
    clean,
    normalizedFieldIdentifier,
    isSensitiveFieldMetadata,
    visible,
    labelledBy,
    accessibleName,
    roleOf,
    boundsOf,
    describe,
    targetSignature,
    sameBounds,
    composedContains,
    deepElementFromPoint,
    pointTarget,
    validateRecord,
    compareProof,
    composedCandidates,
    createRevisionTracker,
  };
}

// Document-revision tracking. A snapshot is only ever valid while the
// revision it was taken at is still current, so every mutation, scroll, and
// resize bumps the revision and records the coaching reason the next stale
// action reports.
function createRevisionTracker({ isTracking = () => true } = {}) {
  let documentRevision = 0;
  let invalidationReason = "the page has not been observed";

  function invalidate(reason) {
    documentRevision += 1;
    if (isTracking()) invalidationReason = reason;
  }

  const mutationObserver = new MutationObserver((mutations) => {
    if (mutations.length > 0) invalidate("the document mutated");
  });
  mutationObserver.observe(document, {
    subtree: true,
    childList: true,
    attributes: true,
    characterData: true,
  });
  addEventListener("scroll", () => invalidate("the page scrolled"), { capture: true, passive: true });
  addEventListener("resize", () => invalidate("the viewport resized"), { passive: true });

  return {
    read: () => documentRevision,
    reason: () => invalidationReason,
    clearReason: () => { invalidationReason = ""; },
    invalidate,
  };
}

if (!globalThis.__LBB_DOM_CORE__) globalThis.__LBB_DOM_CORE__ = createDomCore;
// The service worker imports this file to obtain the exact installable source
// it evaluates into a subframe's isolated world. Rebuilding the text from the
// live function objects keeps one copy of the core in the package: there is
// no second, drifting transcription of these bodies anywhere.
if (!globalThis.__LBB_DOM_CORE_SOURCE__) {
  globalThis.__LBB_DOM_CORE_SOURCE__ = () => `${createRevisionTracker}\n${createDomCore}\nglobalThis.__LBB_DOM_CORE__ = createDomCore;\n`;
}
