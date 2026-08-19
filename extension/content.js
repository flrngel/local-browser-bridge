if (!globalThis.__LOCAL_BROWSER_BRIDGE_CONTENT__) {
  globalThis.__LOCAL_BROWSER_BRIDGE_CONTENT__ = true;

  const CONTROL_HOST_ID = "__local_browser_bridge_control__";
  const GEOMETRY_TOLERANCE_PX = 2;
  const CONTROL_LAST_SEEN_GRACE_MS = 35_000;
  const CONTROL_WATCHDOG_INTERVAL_MS = 2_500;
  const CAPTURE_STALE_MS = 15_000;
  const PAINT_ACK_TIMEOUT_MS = 250;
  const WAIT_POLL_INTERVAL_MS = 250;
  const WAIT_TIMEOUT_DEFAULT_MS = 5_000;
  const WAIT_TIMEOUT_MIN_MS = 100;
  const WAIT_TIMEOUT_MAX_MS = 12_000;
  let generation = "";
  let refs = new Map();
  let pointRefs = new Map();
  let documentRevision = 0;
  let snapshotRevision = -1;
  let invalidationReason = "the page has not been observed";
  let controlUi = null;
  let lastControlState = null;
  let activeControlSessionId = "";
  let activeControlEpoch = 0;
  let controlExpiresAt = 0;
  let controlLastSeenAt = 0;
  const activeCaptureIds = globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_IDS__ instanceof Set
    ? globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_IDS__
    : new Set();
  const captureStartedAt = globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_STARTED_AT__ instanceof Map
    ? globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_STARTED_AT__
    : new Map();
  let captureDepth = activeCaptureIds.size;
  let stopFailureMessage = "";
  globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_IDS__ = activeCaptureIds;
  globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_STARTED_AT__ = captureStartedAt;
  globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_DEPTH__ = captureDepth;
  const revokedControlSessions = new Set();

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

  function isControlNode(node) {
    if (!(node instanceof Element)) return false;
    return node.id === CONTROL_HOST_ID || Boolean(node.closest?.(`#${CONTROL_HOST_ID}`));
  }

  function isOnlyControlUiMutation(mutation) {
    if (isControlNode(mutation.target)) return true;
    const changed = [...mutation.addedNodes, ...mutation.removedNodes];
    return changed.length > 0 && changed.every((node) => isControlNode(node));
  }

  function invalidate(reason) {
    documentRevision += 1;
    if (generation) invalidationReason = reason;
  }

  const mutationObserver = new MutationObserver((mutations) => {
    if (controlUi?.host && !controlUi.host.isConnected && lastControlState) {
      controlUi = null;
      queueMicrotask(() => {
        if (lastControlState) void showControl(lastControlState).catch(() => {});
      });
    }
    if (mutations.some((mutation) => !isOnlyControlUiMutation(mutation))) invalidate("the document mutated");
  });
  mutationObserver.observe(document, {
    subtree: true,
    childList: true,
    attributes: true,
    characterData: true,
  });
  addEventListener("scroll", () => invalidate("the page scrolled"), { capture: true, passive: true });
  addEventListener("resize", () => invalidate("the viewport resized"), { passive: true });

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
    const surface = document.elementsFromPoint(x, y);
    if (surface.some((element, index) => index === 0 && isControlNode(element))) {
      throw new Error("CONTROL_UI_OCCLUSION: release control with the visible Stop button or choose another point");
    }
    const element = deepElementFromPoint(x, y);
    if (!(element instanceof Element) || isControlNode(element)) throw new Error("TARGET_MISSING: no page element is available at that point");
    return element;
  }

  function assertFresh(requestedGeneration) {
    if (!generation || requestedGeneration !== generation) throw new Error("STALE_SNAPSHOT: observe the page again before acting");
    if (snapshotRevision !== documentRevision) {
      throw new Error(`STALE_SNAPSHOT: ${invalidationReason}; observe the page again before acting`);
    }
  }

  function assertActiveControl(requestedSessionId, requestedEpoch) {
    if (!activeControlSessionId
      || requestedSessionId !== activeControlSessionId
      || !Number.isSafeInteger(requestedEpoch)
      || requestedEpoch !== activeControlEpoch) {
      throw new Error("CONTROL_REVOKED: the browser control session is no longer active");
    }
  }

  function parseElementRef(ref) {
    const value = String(ref ?? "");
    const separator = value.lastIndexOf(".");
    if (separator < 0) return { embeddedGeneration: "", key: value };
    return { embeddedGeneration: value.slice(0, separator), key: value.slice(separator + 1) };
  }

  function assertEmbeddedGeneration(ref) {
    const { embeddedGeneration, key } = parseElementRef(ref);
    if (embeddedGeneration && embeddedGeneration !== generation) {
      throw new Error(`STALE_REF: snapshot ${embeddedGeneration} superseded by ${generation}; observe the page again and use fresh refs`);
    }
    return key;
  }

  function resolveRecord(ref, requestedGeneration) {
    // An epoch-embedded ref fails with coaching before any map lookup; the
    // explicit generation parameter stays authoritative afterwards.
    const key = assertEmbeddedGeneration(ref);
    assertFresh(requestedGeneration);
    const record = refs.get(key);
    if (!record?.element?.isConnected) throw new Error("STALE_REF: the element changed; observe the page again");
    return record;
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
        if (!seen.has(element) && !isControlNode(element)) {
          seen.add(element);
          found.push(element);
        }
      }
      for (const element of scope.querySelectorAll("*")) {
        if (element.shadowRoot && !isControlNode(element)) visit(element.shadowRoot);
      }
    };
    visit(root);
    return found;
  }

  function snapshot() {
    generation = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
    refs = new Map();
    pointRefs = new Map();
    const elements = [];
    for (const element of composedCandidates()) {
      if (elements.length >= 500 || !visible(element)) continue;
      const key = `e${elements.length + 1}`;
      const ref = `${generation}.${key}`;
      const description = describe(element, ref);
      refs.set(key, { element, ref, signature: targetSignature(element), bounds: description.bounds });
      elements.push(description);
    }
    snapshotRevision = documentRevision;
    invalidationReason = "";
    return {
      generation,
      revision: snapshotRevision,
      title: clean(document.title, 500),
      url: `${location.origin}${location.pathname}`,
      viewport: { width: innerWidth, height: innerHeight, devicePixelRatio },
      scroll: { x: Math.round(scrollX), y: Math.round(scrollY), maxY: Math.max(0, document.documentElement.scrollHeight - innerHeight) },
      selectedText: clean(window.getSelection()?.toString(), 5_000),
      bodyText: clean(document.body?.innerText, 20_000),
      elements,
    };
  }

  function pageContainsText(needle) {
    return String(document.body?.innerText ?? "").includes(needle)
      || String(document.title ?? "").includes(needle);
  }

  function evaluateWaitConditions(conditions, quietSinceMs) {
    const evaluated = {};
    if (typeof conditions.text === "string") evaluated.text = pageContainsText(conditions.text);
    if (typeof conditions.textGone === "string") evaluated.textGone = !pageContainsText(conditions.textGone);
    if (typeof conditions.urlPrefix === "string") evaluated.urlPrefix = location.href.startsWith(conditions.urlPrefix);
    if (Number.isFinite(conditions.mutationQuietMs)) evaluated.mutationQuiet = quietSinceMs >= conditions.mutationQuietMs;
    return evaluated;
  }

  function waitDelay(milliseconds) {
    return new Promise((resolve) => setTimeout(resolve, milliseconds));
  }

  async function waitForConditions(message) {
    const conditions = {};
    if (typeof message.text === "string") conditions.text = message.text;
    if (typeof message.textGone === "string") conditions.textGone = message.textGone;
    if (typeof message.urlPrefix === "string") conditions.urlPrefix = message.urlPrefix;
    if (Number.isFinite(message.mutationQuietMs)) conditions.mutationQuietMs = message.mutationQuietMs;
    if (Object.keys(conditions).length === 0) {
      throw new Error("BAD_REQUEST: page.waitFor needs text, textGone, urlPrefix, or mutationQuietMs");
    }
    const timeoutMs = Math.min(
      Math.max(Number(message.timeoutMs) || WAIT_TIMEOUT_DEFAULT_MS, WAIT_TIMEOUT_MIN_MS),
      WAIT_TIMEOUT_MAX_MS,
    );
    const startedAt = Date.now();
    let lastRevision = documentRevision;
    let lastRevisionChangeAt = startedAt;
    for (;;) {
      if (documentRevision !== lastRevision) {
        lastRevision = documentRevision;
        lastRevisionChangeAt = Date.now();
      }
      const evaluated = evaluateWaitConditions(conditions, Date.now() - lastRevisionChangeAt);
      if (Object.values(evaluated).every(Boolean)) {
        return { satisfied: true, elapsedMs: Date.now() - startedAt, conditions: evaluated };
      }
      const elapsedMs = Date.now() - startedAt;
      if (elapsedMs >= timeoutMs) {
        // A timed-out wait is a normal result for the caller: satisfied is
        // false, nothing was dispatched, and repeating the same wait is
        // pointless without a new plan.
        throw new Error(`WAIT_TIMEOUT: conditions were not satisfied within ${timeoutMs}ms (satisfied: false, elapsedMs: ${elapsedMs}); observe the page and decide the next step instead of retrying the same wait`);
      }
      await waitDelay(Math.min(WAIT_POLL_INTERVAL_MS, timeoutMs - elapsedMs));
    }
  }

  function setNativeValue(element, value) {
    if (element instanceof HTMLInputElement) {
      const descriptor = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value");
      descriptor?.set?.call(element, value);
    } else if (element instanceof HTMLTextAreaElement) {
      const descriptor = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value");
      descriptor?.set?.call(element, value);
    } else if (element.isContentEditable) {
      element.textContent = value;
    } else {
      throw new Error("Element is not fillable");
    }
    element.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
    element.dispatchEvent(new Event("change", { bubbles: true }));
  }

  function createControlUi() {
    if (controlUi?.host?.isConnected) {
      if (!controlUi.host.matches(":popover-open")) controlUi.host.showPopover();
      return controlUi;
    }
    document.getElementById(CONTROL_HOST_ID)?.remove();
    const host = document.createElement("div");
    host.id = CONTROL_HOST_ID;
    host.setAttribute("popover", "manual");
    host.setAttribute("aria-hidden", "false");
    host.setAttribute("aria-label", "Local Browser Bridge browser control");
    const shadow = host.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = `
      :host { all: initial; position: fixed; inset: 12px 12px auto auto; margin: 0; padding: 0; border: 0;
        width: max-content; height: max-content; overflow: visible; background: transparent; pointer-events: none;
        z-index: 2147483647; color-scheme: dark; }
      .pill { pointer-events: auto; display: flex; align-items: center; gap: 9px; padding: 7px 8px 7px 11px;
        border: 1px solid rgba(255,255,255,.24); border-radius: 999px; background: rgba(19,24,21,.94);
        box-shadow: 0 6px 24px rgba(0,0,0,.34); color: #f5faf7; font: 700 12px/1.2 system-ui, sans-serif;
        backdrop-filter: blur(10px); }
      .pill.error { border-color: rgba(255,151,129,.7); background: rgba(67,25,22,.97); }
      .pill.error .dot { background: #ff977f; box-shadow: 0 0 0 4px rgba(255,151,127,.16); }
      .dot { width: 8px; height: 8px; border-radius: 50%; background: #a8ee57; box-shadow: 0 0 0 4px rgba(168,238,87,.14); }
      .detail { color: #aebbb3; font: 500 10px/1.2 ui-monospace, monospace; }
      button { all: unset; cursor: pointer; padding: 5px 9px; border-radius: 999px; background: #f5faf7; color: #172019;
        font: 800 11px/1 system-ui, sans-serif; }
      button:focus-visible { outline: 2px solid #a8ee57; outline-offset: 2px; }
      button:disabled { cursor: wait; opacity: .55; }
      .cursor { position: fixed; left: 0; top: 0; width: 27px; height: 35px; pointer-events: none; opacity: 0;
        transform: translate3d(-100px,-100px,0); will-change: transform; filter: drop-shadow(0 2px 2px rgba(0,0,0,.45)); }
      .cursor.visible { opacity: 1; }
      svg { display: block; width: 27px; height: 35px; }
    `;
    const pill = document.createElement("div");
    pill.className = "pill";
    pill.setAttribute("role", "status");
    const dot = document.createElement("span");
    dot.className = "dot";
    const label = document.createElement("span");
    label.textContent = "Local Browser Bridge is using this tab";
    const detail = document.createElement("span");
    detail.className = "detail";
    const stop = document.createElement("button");
    stop.type = "button";
    stop.textContent = "Stop";
    stop.setAttribute("aria-label", "Stop Local Browser Bridge browser control");
    stop.addEventListener("click", () => void requestControlStop(stop));
    pill.append(dot, label, detail, stop);
    const cursor = document.createElement("div");
    cursor.className = "cursor";
    cursor.innerHTML = '<svg viewBox="0 0 27 35" aria-hidden="true"><path d="M2.3 1.8v26.1l6.5-6.1 4.3 10.7 5.4-2.3-4.4-10.5h9.2L2.3 1.8Z" fill="#fff" stroke="#111815" stroke-width="2.4" stroke-linejoin="round"/></svg>';
    shadow.append(style, pill, cursor);
    document.documentElement.append(host);
    try {
      host.showPopover();
    } catch (error) {
      host.remove();
      throw new Error(`CONTROL_UI_RENDER_FAILED: Chrome refused the control popover: ${error.message}`);
    }
    if (!host.matches(":popover-open")) {
      host.remove();
      throw new Error("CONTROL_UI_RENDER_FAILED: the control popover did not enter the top layer");
    }
    controlUi = { host, pill, label, detail, stop, cursor };
    applyCaptureVisibility();
    return controlUi;
  }

  function applyCaptureVisibility() {
    if (controlUi?.pill) controlUi.pill.style.visibility = captureDepth > 0 ? "hidden" : "visible";
    if (controlUi?.cursor) controlUi.cursor.style.visibility = "visible";
  }

  function syncCaptureGlobals() {
    captureDepth = activeCaptureIds.size;
    globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_DEPTH__ = captureDepth;
    applyCaptureVisibility();
  }

  function reconcileCaptureIds(ids) {
    if (!Array.isArray(ids)) return;
    const now = Date.now();
    const expected = new Set(ids.map(String).filter(Boolean));
    for (const id of [...activeCaptureIds]) {
      if (!expected.has(id)) {
        activeCaptureIds.delete(id);
        captureStartedAt.delete(id);
      }
    }
    for (const id of expected) {
      activeCaptureIds.add(id);
      if (!captureStartedAt.has(id)) captureStartedAt.set(id, now);
    }
    syncCaptureGlobals();
  }

  function rendered(element) {
    if (!element?.isConnected) return false;
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    const checkVisibility = typeof element.checkVisibility === "function"
      ? element.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })
      : true;
    return checkVisibility
      && style.display !== "none"
      && style.visibility !== "hidden"
      && style.visibility !== "collapse"
      && Number(style.opacity) !== 0
      && rect.width > 1
      && rect.height > 1;
  }

  function controlUiPaintState() {
    const hostConnected = Boolean(controlUi?.host?.isConnected);
    const popoverOpen = Boolean(hostConnected && controlUi.host.matches(":popover-open"));
    return {
      hostConnected,
      popoverOpen,
      pillVisible: Boolean(popoverOpen && rendered(controlUi?.pill)),
      stopVisible: Boolean(popoverOpen && rendered(controlUi?.stop)),
      cursorVisible: Boolean(popoverOpen && rendered(controlUi?.cursor)),
      capturing: captureDepth > 0,
      captureDepth,
      activeCaptureIds: [...activeCaptureIds],
    };
  }

  function waitForPaint() {
    return new Promise((resolve) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve();
      };
      const timer = setTimeout(finish, PAINT_ACK_TIMEOUT_MS);
      if (typeof requestAnimationFrame !== "function") {
        queueMicrotask(finish);
        return;
      }
      requestAnimationFrame(() => requestAnimationFrame(finish));
    });
  }

  async function confirmControlUiPaint() {
    applyCaptureVisibility();
    await waitForPaint();
    return controlUiPaintState();
  }

  function showControlStopFailure() {
    stopFailureMessage = "Stop failed—use Chrome Cancel or the extension popup.";
    const ui = createControlUi();
    ui.pill.classList.add("error");
    ui.label.textContent = "Local Browser Bridge may still be using this tab";
    ui.detail.textContent = stopFailureMessage;
    ui.stop.textContent = "Retry Stop";
    ui.stop.disabled = false;
    applyCaptureVisibility();
    return controlUiPaintState();
  }

  async function requestControlStop(stop) {
    const stoppedSessionId = activeControlSessionId;
    stop.disabled = true;
    try {
      const response = await chrome.runtime.sendMessage({ type: "LBB_CONTROL_UI", action: "stop" });
      if (!response?.ok) throw new Error(response?.error || "Control release failed");
      if (response.result?.active !== false) throw new Error("Control release was not acknowledged as inactive");
      hideControl({ sessionId: stoppedSessionId });
      return { stopped: true };
    } catch (error) {
      showControlStopFailure();
      return { stopped: false, error: error.message };
    } finally {
      if (controlUi?.stop === stop) stop.disabled = false;
    }
  }

  async function showControl(message) {
    const sessionId = String(message.sessionId || "");
    const epoch = Number(message.controlEpoch ?? message.epoch);
    if (!sessionId
      || !Number.isSafeInteger(epoch)
      || epoch <= 0
      || revokedControlSessions.has(sessionId)) {
      throw new Error("CONTROL_REVOKED: refusing an invalid or revoked browser control session");
    }
    if (activeControlSessionId && activeControlSessionId !== sessionId) stopFailureMessage = "";
    lastControlState = structuredClone(message);
    activeControlSessionId = sessionId;
    activeControlEpoch = epoch;
    controlExpiresAt = Number(message.expiresAt) || 0;
    controlLastSeenAt = Date.now();
    reconcileCaptureIds(message.activeCaptureIds);
    const ui = createControlUi();
    if (message.cursor) updateCursor(message.cursor, message);
    if (stopFailureMessage) {
      showControlStopFailure();
    } else {
      ui.pill.classList.remove("error");
      ui.label.textContent = "Local Browser Bridge is using this tab";
      ui.detail.textContent = `turn ${Number(message.turn) || 0} · move ${Number(message.moveSequence) || 0}`;
      ui.stop.textContent = "Stop";
    }
    return confirmControlUiPaint();
  }

  function updateCursor(cursorState, metadata = {}) {
    controlLastSeenAt = Date.now();
    if (Number(metadata.expiresAt) > 0) controlExpiresAt = Number(metadata.expiresAt);
    if (lastControlState) {
      lastControlState.cursor = structuredClone(cursorState);
      lastControlState.turn = cursorState.turn;
      lastControlState.moveSequence = cursorState.moveSequence;
    }
    const ui = createControlUi();
    const x = Math.round(Number(cursorState.x) || 0);
    const y = Math.round(Number(cursorState.y) || 0);
    ui.cursor.style.transform = `translate3d(${x}px, ${y}px, 0)`;
    ui.cursor.classList.toggle("visible", Boolean(cursorState.visible));
    ui.detail.textContent = `turn ${Number(cursorState.turn) || 0} · move ${Number(cursorState.moveSequence) || 0}`;
    return { x, y, visible: Boolean(cursorState.visible) };
  }

  function hideControl(message) {
    const requestedSessionId = String(message?.sessionId || "");
    if (requestedSessionId) {
      revokedControlSessions.add(requestedSessionId);
      if (revokedControlSessions.size > 16) revokedControlSessions.delete(revokedControlSessions.values().next().value);
    }
    if (requestedSessionId
      && activeControlSessionId
      && requestedSessionId !== activeControlSessionId) {
      return { visible: true, ignoredStaleSession: true };
    }
    lastControlState = null;
    activeControlSessionId = "";
    activeControlEpoch = 0;
    controlExpiresAt = 0;
    controlLastSeenAt = 0;
    stopFailureMessage = "";
    activeCaptureIds.clear();
    captureStartedAt.clear();
    syncCaptureGlobals();
    if (!controlUi?.host) return { visible: false };
    try { controlUi.host.hidePopover(); } catch {}
    controlUi.host.remove();
    controlUi = null;
    return { visible: false };
  }

  async function setCaptureMode(capturing, captureId, authoritativeIds) {
    const id = String(captureId || "legacy");
    if (capturing) {
      activeCaptureIds.add(id);
      captureStartedAt.set(id, Date.now());
    } else {
      activeCaptureIds.delete(id);
      captureStartedAt.delete(id);
    }
    if (Array.isArray(authoritativeIds)) reconcileCaptureIds(authoritativeIds);
    else syncCaptureGlobals();
    return confirmControlUiPaint();
  }

  function expireStaleCaptures() {
    const now = Date.now();
    let changed = false;
    for (const id of [...activeCaptureIds]) {
      const startedAt = Number(captureStartedAt.get(id)) || 0;
      if (startedAt > 0 && now - startedAt <= CAPTURE_STALE_MS) continue;
      activeCaptureIds.delete(id);
      captureStartedAt.delete(id);
      changed = true;
    }
    if (changed) syncCaptureGlobals();
    return changed;
  }

  async function reconcileControl() {
    try {
      const response = await chrome.runtime.sendMessage({ type: "LBB_CONTROL_UI", action: "reconcile" });
      if (response?.ok && response.result?.active) {
        await showControl(response.result);
      } else if (activeControlSessionId) {
        hideControl({ sessionId: activeControlSessionId });
      }
    } catch {
      if (activeControlSessionId && (Date.now() >= controlExpiresAt || Date.now() - controlLastSeenAt > CONTROL_LAST_SEEN_GRACE_MS)) {
        hideControl({ sessionId: activeControlSessionId });
      }
    }
  }

  async function expireStaleControl() {
    if (!activeControlSessionId) return;
    const now = Date.now();
    const expired = controlExpiresAt > 0 && now >= controlExpiresAt;
    const unseen = controlLastSeenAt > 0 && now - controlLastSeenAt > CONTROL_LAST_SEEN_GRACE_MS;
    if (!expired && !unseen) return;
    hideControl({ sessionId: activeControlSessionId });
    await chrome.runtime.sendMessage({ type: "LBB_CONTROL_UI", action: "stop" }).catch(() => {});
  }

  async function handle(message) {
    switch (message.method) {
      case "snapshot":
        return snapshot();
      case "assertGeneration":
        assertFresh(message.generation);
        return { current: true, viewport: { width: innerWidth, height: innerHeight, devicePixelRatio } };
      case "wait":
        // Read-only condition wait: no control session, no snapshot
        // freshness requirement, and never any input dispatch.
        return waitForConditions(message);
      case "describe": {
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        const record = resolveRecord(message.ref, message.generation);
        return validateRecord(record).description;
      }
      case "prepareClick": {
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        const record = resolveRecord(message.ref, message.generation);
        const validated = validateRecord(record, { requireHitTest: true });
        return { ...validated.description, proof: validated.proof };
      }
      case "commitClick": {
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        const record = resolveRecord(message.ref, message.generation);
        const validated = validateRecord(record, { requireHitTest: true });
        compareProof(validated.proof, message.proof);
        return { validated: true, bounds: validated.description.bounds };
      }
      case "preparePoint": {
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        assertFresh(message.generation);
        const x = Number(message.x);
        const y = Number(message.y);
        const element = pointTarget(x, y);
        const token = crypto.randomUUID();
        const proof = { signature: targetSignature(element), bounds: boundsOf(element) };
        pointRefs.set(token, element);
        return { token, proof, x, y };
      }
      case "commitPoint": {
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        assertFresh(message.generation);
        const element = pointTarget(Number(message.x), Number(message.y));
        const original = pointRefs.get(message.token);
        if (!original || original !== element || !element.isConnected) throw new Error("TARGET_CHANGED: point target changed; observe again");
        compareProof({ signature: targetSignature(element), bounds: boundsOf(element) }, message.proof);
        return { validated: true };
      }
      case "fill": {
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        const record = resolveRecord(message.ref, message.generation);
        const description = validateRecord(record, { requireHitTest: true }).description;
        if (description.sensitive && !message.allowSensitive) throw new Error("SENSITIVE_FIELD: enter this value manually");
        record.element.focus({ preventScroll: true });
        setNativeValue(record.element, String(message.text ?? ""));
        return { filled: true };
      }
      case "select": {
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        const record = resolveRecord(message.ref, message.generation);
        const description = validateRecord(record, { requireHitTest: true }).description;
        if (description.sensitive && !message.allowSensitive) throw new Error("SENSITIVE_FIELD: select this value manually");
        const element = record.element;
        if (!(element instanceof HTMLSelectElement)) throw new Error("Element is not a select control");
        const option = [...element.options].find((item) => item.value === message.value || item.label === message.value);
        if (!option) throw new Error("No option matched that value or label");
        element.value = option.value;
        element.dispatchEvent(new Event("input", { bubbles: true }));
        element.dispatchEvent(new Event("change", { bubbles: true }));
        return { selected: option.value, label: option.label };
      }
      case "scroll":
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        assertFresh(message.generation);
        window.scrollBy({ left: Number(message.deltaX) || 0, top: Number(message.deltaY) || 0, behavior: "instant" });
        invalidate("the page scrolled");
        return { x: Math.round(scrollX), y: Math.round(scrollY), snapshotInvalidated: true };
      case "control.show":
        return showControl(message);
      case "control.cursor":
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        return updateCursor(message.cursor ?? {}, message);
      case "control.hide":
        return hideControl(message);
      case "control.capture.begin":
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        return setCaptureMode(true, message.captureId, message.activeCaptureIds);
      case "control.capture.end":
        assertActiveControl(message.controlSessionId, message.controlEpoch);
        return setCaptureMode(false, message.captureId, message.activeCaptureIds);
      default:
        throw new Error("Unknown content command");
    }
  }

  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (message?.type !== "LBB_CONTENT") return undefined;
    handle(message)
      .then((result) => sendResponse({ ok: true, result }))
      .catch((error) => sendResponse({ ok: false, error: error.message }));
    return true;
  });

  addEventListener("pageshow", () => void reconcileControl());
  setInterval(() => {
    expireStaleCaptures();
    void expireStaleControl();
  }, CONTROL_WATCHDOG_INTERVAL_MS);
  if (document.readyState === "complete") queueMicrotask(() => void reconcileControl());
}
