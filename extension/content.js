if (!globalThis.__LOCAL_BROWSER_BRIDGE_CONTENT__) {
  globalThis.__LOCAL_BROWSER_BRIDGE_CONTENT__ = true;

  function randomHex128() {
    return [...crypto.getRandomValues(new Uint8Array(16))]
      .map((value) => value.toString(16).padStart(2, "0"))
      .join("");
  }

  // The bridge injects nothing into the page. The visible, page-independent
  // signals that a remote agent holds this tab are Chrome's own debugger
  // infobar and its Cancel button, the named Local Browser Bridge tab group,
  // and Release control in the extension popup. This script only observes the
  // document and proves targets; the control session it mirrors here is the
  // background service worker's, and every mutating command is refused unless
  // the exact session and epoch the background bound are presented again.
  const CONTROL_LAST_SEEN_GRACE_MS = 35_000;
  const CONTROL_WATCHDOG_INTERVAL_MS = 2_500;
  const WAIT_POLL_INTERVAL_MS = 250;
  const WAIT_TIMEOUT_DEFAULT_MS = 5_000;
  const WAIT_TIMEOUT_MIN_MS = 100;
  const WAIT_TIMEOUT_MAX_MS = 12_000;
  // Frame owners are reported so an observation can state how many iframes
  // the top document holds versus how many the background actually merged.
  const FRAME_OWNER_REPORT_MAX = 32;
  let generation = "";
  let refs = new Map();
  let pointRefs = new Map();
  let snapshotRevision = -1;
  let activeControlSessionId = "";
  let activeControlEpoch = 0;
  let controlExpiresAt = 0;
  let controlLastSeenAt = 0;
  const revokedControlSessions = new Set();

  // The observation and target-proof core is shared verbatim with the
  // cross-origin frame agent.
  if (typeof globalThis.__LBB_DOM_CORE__ !== "function") {
    throw new Error("DOM_CORE_MISSING: extension/dom-core.js must load before extension/content.js");
  }
  const core = globalThis.__LBB_DOM_CORE__();
  const {
    clean,
    visible,
    boundsOf,
    describe,
    targetSignature,
    pointTarget,
    validateRecord,
    compareProof,
    composedCandidates,
  } = core;
  const revisions = core.createRevisionTracker({
    isTracking: () => Boolean(generation),
  });

  function assertFresh(requestedGeneration) {
    if (!generation || requestedGeneration !== generation) throw new Error("STALE_SNAPSHOT: observe the page again before acting");
    if (snapshotRevision !== revisions.read()) {
      throw new Error(`STALE_SNAPSHOT: ${revisions.reason()}; observe the page again before acting`);
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
    // A frame-scoped ref (<generation>.f2.e5) is resolved by the background
    // against a subframe agent and must never reach the ref map of the top
    // document, where its element key would collide with another element.
    if (/\.f[1-9][0-9]?\.e[1-9][0-9]{0,3}$/.test(value)) {
      throw new Error("FRAME_REF_MISROUTED: this ref belongs to a subframe; the background routes frame refs");
    }
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
    snapshotRevision = revisions.read();
    revisions.clearReason();
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
      frameOwners: frameOwners(),
    };
  }

  // Every iframe/frame owner element the top document holds, whether or not
  // the background can attach to it. The background compares this list with
  // the frame targets it actually merged, so an unmerged frame is reported
  // as skipped instead of silently disappearing from the observation.
  function frameOwners() {
    return [...document.querySelectorAll("iframe,frame")]
      .slice(0, FRAME_OWNER_REPORT_MAX)
      .map((owner, index) => {
        let origin = "";
        try {
          origin = new URL(owner.getAttribute("src") || "", location.href).origin;
        } catch {}
        return {
          index,
          tagName: owner.tagName.toLowerCase(),
          origin: clean(origin, 300),
          sameOrigin: origin === location.origin,
          bounds: boundsOf(owner),
          visible: visible(owner),
        };
      });
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
    let lastRevision = revisions.read();
    let lastRevisionChangeAt = startedAt;
    for (;;) {
      if (revisions.read() !== lastRevision) {
        lastRevision = revisions.read();
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

  // Mirrors the background's lease into this document so a mutating command
  // must present the exact session and epoch. It renders nothing.
  function bindControl(message) {
    const sessionId = String(message.sessionId || "");
    const epoch = Number(message.controlEpoch ?? message.epoch);
    if (!sessionId
      || !Number.isSafeInteger(epoch)
      || epoch <= 0
      || revokedControlSessions.has(sessionId)) {
      throw new Error("CONTROL_REVOKED: refusing an invalid or revoked browser control session");
    }
    activeControlSessionId = sessionId;
    activeControlEpoch = epoch;
    controlExpiresAt = Number(message.expiresAt) || 0;
    controlLastSeenAt = Date.now();
    return { bound: true, sessionId, epoch };
  }

  function releaseControl(message) {
    const requestedSessionId = String(message?.sessionId || "");
    if (requestedSessionId) {
      revokedControlSessions.add(requestedSessionId);
      if (revokedControlSessions.size > 16) revokedControlSessions.delete(revokedControlSessions.values().next().value);
    }
    if (requestedSessionId
      && activeControlSessionId
      && requestedSessionId !== activeControlSessionId) {
      return { bound: true, ignoredStaleSession: true };
    }
    activeControlSessionId = "";
    activeControlEpoch = 0;
    controlExpiresAt = 0;
    controlLastSeenAt = 0;
    return { bound: false };
  }

  async function reconcileControl() {
    try {
      const response = await chrome.runtime.sendMessage({ type: "LBB_CONTROL", action: "reconcile" });
      if (response?.ok && response.result?.active) {
        bindControl(response.result);
      } else if (activeControlSessionId) {
        releaseControl({ sessionId: activeControlSessionId });
      }
    } catch {
      if (activeControlSessionId && (Date.now() >= controlExpiresAt || Date.now() - controlLastSeenAt > CONTROL_LAST_SEEN_GRACE_MS)) {
        releaseControl({ sessionId: activeControlSessionId });
      }
    }
  }

  function scheduleInitialControlReconcile() {
    // content.js runs at document_idle, which does not imply that slow images,
    // frames, or other subresources have reached readyState "complete".
    // Reconcile immediately so an authorized navigation can rebind this fresh
    // document without waiting for the load event.
    queueMicrotask(() => void reconcileControl());
  }

  // Local fail-closed expiry. The background owns lease lifetime and revokes
  // through its own heartbeat; this only stops the document from honoring a
  // binding it can no longer confirm.
  function expireStaleControl() {
    if (!activeControlSessionId) return;
    const now = Date.now();
    const expired = controlExpiresAt > 0 && now >= controlExpiresAt;
    const unseen = controlLastSeenAt > 0 && now - controlLastSeenAt > CONTROL_LAST_SEEN_GRACE_MS;
    if (!expired && !unseen) return;
    releaseControl({ sessionId: activeControlSessionId });
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
        const token = randomHex128();
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
        revisions.invalidate("the page scrolled");
        return { x: Math.round(scrollX), y: Math.round(scrollY), snapshotInvalidated: true };
      case "control.bind":
        return bindControl(message);
      case "control.release":
        return releaseControl(message);
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
  setInterval(() => expireStaleControl(), CONTROL_WATCHDOG_INTERVAL_MS);
  scheduleInitialControlReconcile();
}
