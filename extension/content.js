if (!globalThis.__LOCAL_BROWSER_BRIDGE_CONTENT__) {
  globalThis.__LOCAL_BROWSER_BRIDGE_CONTENT__ = true;

  function randomHex128() {
    return [...crypto.getRandomValues(new Uint8Array(16))]
      .map((value) => value.toString(16).padStart(2, "0"))
      .join("");
  }

  // A fresh identifier per document denies page scripts a stable selector for
  // the page-owned safety surface. It is still page DOM, so the background
  // independently requires bounded render/layout and hit-test acknowledgements
  // and Chrome's own debugger
  // warning remains the trusted, page-independent handback surface.
  const CONTROL_HOST_ID = `__local_browser_bridge_control_${randomHex128()}__`;
  const CONTROL_MARKER_ID = `__local_browser_bridge_marker_${randomHex128()}__`;
  const CONTROL_LAST_SEEN_GRACE_MS = 35_000;
  const CONTROL_WATCHDOG_INTERVAL_MS = 2_500;
  const CONTROL_UI_WATCHDOG_INTERVAL_MS = 500;
  const CONTROL_BROWSER_ACK_TIMEOUT_MS = 2_000;
  const CONTROL_ACCESSIBLE_LABEL = "Local Browser Bridge browser control";
  const CONTROL_ACCESSIBILITY_ANCESTRY_MAX = 64;
  const CAPTURE_STALE_MS = 15_000;
  const RENDER_ACK_TIMEOUT_MS = 250;
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
  let controlUiLossReportPendingSessionId = "";
  let controlUiRetopDepth = 0;
  let earlyStopGuardReady = false;
  const handledStopActivationEvents = new WeakSet();
  let lastStopPointerActivation = null;
  globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_IDS__ = activeCaptureIds;
  globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_STARTED_AT__ = captureStartedAt;
  globalThis.__LOCAL_BROWSER_BRIDGE_CAPTURE_DEPTH__ = captureDepth;
  const revokedControlSessions = new Set();

  function isControlNode(node) {
    const host = controlUi?.host;
    const shadow = controlUi?.shadow;
    return Boolean(host && (
      node === host
      || node === shadow
      || shadow?.contains?.(node)
    ));
  }

  function reinsertControlUiWhenLost() {
    if (controlUi?.host && !controlUi.host.isConnected && lastControlState) {
      controlUi = null;
      // Outside the deliberately acknowledged capture interval, disappearing
      // UI is a revoked safety condition, not something to paper over by
      // silently recreating the surface after a page removed it.
      if (activeControlSessionId && captureDepth === 0) {
        queueMicrotask(() => void failClosedOnLostControlUi());
      }
    }
  }

  // The observation and target-proof core is shared verbatim with the
  // cross-origin frame agent; the only host-specific input is which nodes are
  // the bridge's own control overlay and therefore never observable targets.
  if (typeof globalThis.__LBB_DOM_CORE__ !== "function") {
    throw new Error("DOM_CORE_MISSING: extension/dom-core.js must load before extension/content.js");
  }
  const core = globalThis.__LBB_DOM_CORE__({ isExcludedNode: isControlNode });
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
    isExcludedNode: isControlNode,
    onMutationBatch: reinsertControlUiWhenLost,
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
    host.setAttribute("aria-label", CONTROL_ACCESSIBLE_LABEL);
    host.addEventListener("toggle", (event) => {
      if (event.newState === "closed") queueMicrotask(() => void failClosedOnLostControlUi());
    });
    const shadow = host.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = `
      :host { all: initial !important; display: block !important; position: fixed !important;
        inset: 12px 12px auto auto !important; margin: 0 !important; padding: 0 !important; border: 0 !important;
        width: max-content !important; min-width: 0 !important; max-width: none !important;
        height: max-content !important; min-height: 0 !important; max-height: none !important;
        overflow: visible !important; opacity: 1 !important; visibility: visible !important;
        filter: none !important; backdrop-filter: none !important; -webkit-backdrop-filter: none !important;
        mask: none !important; mask-image: none !important; -webkit-mask: none !important;
        -webkit-mask-image: none !important; clip: auto !important; clip-path: none !important;
        transform: none !important; translate: none !important; rotate: none !important; scale: none !important;
        perspective: none !important; transform-style: flat !important; zoom: 1 !important;
        content-visibility: visible !important; contain: none !important; mix-blend-mode: normal !important;
        isolation: isolate !important; animation: none !important; transition: none !important;
        background: transparent !important; pointer-events: none !important; z-index: 2147483647 !important;
        color-scheme: dark !important; }
      :host::before, :host::after { all: initial !important; content: none !important; display: none !important;
        position: static !important; inset: auto !important; width: 0 !important; height: 0 !important;
        opacity: 0 !important; background: none !important; filter: none !important; mask: none !important;
        -webkit-mask: none !important; clip: auto !important; clip-path: none !important;
        transform: none !important; pointer-events: none !important; }
      :host::backdrop { content: none !important; background: transparent !important;
        background-image: none !important; opacity: 1 !important; filter: none !important;
        backdrop-filter: none !important; -webkit-backdrop-filter: none !important;
        mask: none !important; -webkit-mask: none !important; clip-path: none !important;
        transform: none !important; pointer-events: none !important;
        animation: none !important; transition: none !important; }
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
    // The document_start guard sees this click before later page capture
    // listeners. Keep the target listener as a same-event fallback; the
    // verifier deduplicates it after the guard path.
    stop.addEventListener("click", handleControlStopActivation);
    pill.append(dot, label, detail, stop);
    const cursor = document.createElement("div");
    cursor.className = "cursor";
    cursor.innerHTML = '<svg viewBox="0 0 27 35" aria-hidden="true"><path d="M2.3 1.8v26.1l6.5-6.1 4.3 10.7 5.4-2.3-4.4-10.5h9.2L2.3 1.8Z" fill="#fff" stroke="#111815" stroke-width="2.4" stroke-linejoin="round"/></svg>';
    // The marker never leaves the closed shadow tree. Its per-document id is
    // returned only over the isolated extension channel, letting the browser
    // process bind the top-layer NodeId to this exact host rather than to
    // page-forgeable host attributes.
    const marker = document.createElement("span");
    marker.id = CONTROL_MARKER_ID;
    marker.hidden = true;
    marker.setAttribute("aria-hidden", "true");
    shadow.append(style, pill, cursor, marker);
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
    controlUi = { host, shadow, pill, label, detail, stop, cursor, marker };
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
      && rect.height > 1
      && rect.left >= 0
      && rect.top >= 0
      && rect.right <= innerWidth
      && rect.bottom <= innerHeight;
  }

  function controlHostSafelyRendered() {
    const host = controlUi?.host;
    if (!rendered(host)) return false;
    const style = getComputedStyle(host);
    const pseudoSuppressed = ["::before", "::after"].every((pseudo) => {
      const pseudoStyle = getComputedStyle(host, pseudo);
      return pseudoStyle.display === "none" && pseudoStyle.content === "none";
    });
    const backdropStyle = getComputedStyle(host, "::backdrop");
    const backdropSuppressed = ["transparent", "rgba(0, 0, 0, 0)"].includes(backdropStyle.backgroundColor)
      && backdropStyle.backgroundImage === "none"
      && Number(backdropStyle.opacity) === 1
      && backdropStyle.filter === "none"
      && backdropStyle.backdropFilter === "none"
      && (backdropStyle.webkitBackdropFilter === undefined || backdropStyle.webkitBackdropFilter === "none")
      && backdropStyle.pointerEvents === "none";
    return pseudoSuppressed
      && backdropSuppressed
      && style.display === "block"
      && style.position === "fixed"
      && style.visibility === "visible"
      && Number(style.opacity) === 1
      && style.filter === "none"
      && style.backdropFilter === "none"
      && (style.webkitBackdropFilter === undefined || style.webkitBackdropFilter === "none")
      && style.maskImage === "none"
      && (style.webkitMaskImage === undefined || style.webkitMaskImage === "none")
      && style.clipPath === "none"
      && style.transform === "none"
      && (style.translate === undefined || style.translate === "none")
      && (style.rotate === undefined || style.rotate === "none")
      && (style.scale === undefined || style.scale === "none")
      && style.contentVisibility === "visible"
      && style.mixBlendMode === "normal";
  }

  function controlElementHitPoints(element) {
    const rect = element?.getBoundingClientRect?.();
    if (!rect || rect.width <= 1 || rect.height <= 1) return [];
    return [
      [0.5, 0.5],
      [0.25, 0.25],
      [0.75, 0.25],
      [0.25, 0.75],
      [0.75, 0.75],
    ].map(([xRatio, yRatio]) => ({
      x: rect.left + (rect.width * xRatio),
      y: rect.top + (rect.height * yRatio),
    }));
  }

  function controlElementTopmost(element) {
    if (!controlUi?.host || !controlUi?.shadow || !rendered(element)) return false;
    if (typeof document.elementFromPoint !== "function"
      || typeof controlUi.shadow.elementFromPoint !== "function") return false;
    const points = controlElementHitPoints(element);
    return points.length > 0 && points.every(({ x, y }) => {
      // Closed-shadow hits are retargeted to the host for document callers;
      // the retained ShadowRoot proves the same point belongs to the expected
      // pill or Stop subtree rather than an unrelated shadow descendant.
      if (document.elementFromPoint(x, y) !== controlUi.host) return false;
      const shadowHit = controlUi.shadow.elementFromPoint(x, y);
      return shadowHit === element || Boolean(shadowHit && element.contains(shadowHit));
    });
  }

  function controlBrowserHitPoints() {
    if (captureDepth > 0) return [];
    const pill = controlElementHitPoints(controlUi?.pill);
    const stop = controlElementHitPoints(controlUi?.stop);
    return [...pill.slice(0, 3), ...stop.slice(0, 2)].map(({ x, y }) => ({
      x: Math.round(x),
      y: Math.round(y),
    }));
  }

  function controlAccessibilityReady() {
    const host = controlUi?.host;
    if (!host?.isConnected
      || host.parentElement !== document.documentElement
      || host.parentNode !== document.documentElement
      || host.getAttribute("aria-hidden") !== "false"
      || host.getAttribute("aria-label") !== CONTROL_ACCESSIBLE_LABEL) return false;
    const seen = new Set();
    let current = host;
    for (let depth = 0; depth < CONTROL_ACCESSIBILITY_ANCESTRY_MAX; depth += 1) {
      if (!current || seen.has(current) || typeof current.getAttribute !== "function") return false;
      seen.add(current);
      if (current.hidden || current.inert || current.getAttribute("aria-hidden") === "true") return false;
      if (current.parentElement) {
        current = current.parentElement;
        continue;
      }
      let root;
      try {
        root = current.getRootNode?.();
      } catch {
        return false;
      }
      if (root === document) return true;
      // parentElement stops at a ShadowRoot. Continue through its host so a
      // hostile open or closed outer shadow tree cannot hide/inert the real
      // bridge host outside the ordinary element ancestry.
      if (!root?.host) return false;
      current = root.host;
    }
    return false;
  }

  function viewTransitionActive() {
    try {
      if ("activeViewTransition" in document && document.activeViewTransition) return true;
      if (document.documentElement.matches(":active-view-transition")) return true;
    } catch {
      // A browser that cannot evaluate the paint-order selector cannot prove
      // that no view-transition layer is above the top layer.
      return true;
    }
    try {
      return document.getAnimations({ subtree: true }).some((animation) => {
        const pseudo = String(animation.effect?.pseudoElement || "");
        return pseudo.startsWith("::view-transition")
          && !["finished", "idle"].includes(animation.playState);
      });
    } catch {
      return true;
    }
  }

  function ensureControlUiLatestTopLayer() {
    const host = controlUi?.host;
    if (!host?.isConnected) return false;
    try {
      host.hidden = false;
      host.inert = false;
      host.setAttribute("aria-hidden", "false");
      host.setAttribute("aria-label", CONTROL_ACCESSIBLE_LABEL);
      // Reopening a manual popover moves it to the end of the top layer. This
      // is intentionally unconditional so existing passive popovers move
      // below the warning. Chrome-process point/tail proof and its absolute
      // dirty-event deadline handle new or continuously renewed surfaces.
      if (host.matches(":popover-open")) host.hidePopover();
      host.showPopover();
      return host.matches(":popover-open");
    } catch {
      return false;
    }
  }

  function controlUiRenderState(topLayerReordered = false) {
    const hostConnected = Boolean(controlUi?.host?.isConnected);
    const popoverOpen = Boolean(hostConnected && controlUi.host.matches(":popover-open"));
    const pillVisible = Boolean(popoverOpen && rendered(controlUi?.pill));
    const stopVisible = Boolean(popoverOpen && rendered(controlUi?.stop));
    return {
      hostConnected,
      popoverOpen,
      topLayerReordered,
      earlyStopGuardReady,
      accessibilityReady: controlAccessibilityReady(),
      hostId: CONTROL_HOST_ID,
      markerId: CONTROL_MARKER_ID,
      viewTransitionActive: viewTransitionActive(),
      viewport: { width: innerWidth, height: innerHeight },
      controlHitPoints: controlBrowserHitPoints(),
      hostVisible: Boolean(popoverOpen && controlHostSafelyRendered()),
      pillVisible,
      stopVisible,
      pillTopmost: Boolean(pillVisible && controlElementTopmost(controlUi?.pill)),
      stopTopmost: Boolean(stopVisible && controlElementTopmost(controlUi?.stop)),
      cursorVisible: Boolean(popoverOpen && rendered(controlUi?.cursor)),
      capturing: captureDepth > 0,
      captureDepth,
      activeCaptureIds: [...activeCaptureIds],
    };
  }

  function waitForRenderOpportunity() {
    return new Promise((resolve) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve();
      };
      // Two animation frames give Chrome an opportunity to update layout and
      // hit testing. The timeout is only a bounded fallback when frames are
      // throttled; neither path is proof that pixels reached a physical display.
      const timer = setTimeout(finish, RENDER_ACK_TIMEOUT_MS);
      if (typeof requestAnimationFrame !== "function") {
        queueMicrotask(finish);
        return;
      }
      requestAnimationFrame(() => requestAnimationFrame(finish));
    });
  }

  async function confirmControlUiRender() {
    controlUiRetopDepth += 1;
    try {
      const topLayerReordered = ensureControlUiLatestTopLayer();
      applyCaptureVisibility();
      await waitForRenderOpportunity();
      return controlUiRenderState(topLayerReordered);
    } finally {
      controlUiRetopDepth = Math.max(0, controlUiRetopDepth - 1);
    }
  }

  function controlUiVisiblyAvailable(state = controlUiRenderState()) {
    return state.hostConnected
      && state.popoverOpen
      && state.hostVisible
      && state.accessibilityReady
      && state.viewTransitionActive === false
      && state.pillVisible
      && state.stopVisible
      && state.pillTopmost
      && state.stopTopmost;
  }

  async function reportControlUiLoss(sessionId) {
    controlUiLossReportPendingSessionId = sessionId;
    try {
      const response = await chrome.runtime.sendMessage({
        type: "LBB_CONTROL_UI",
        action: "indicatorLost",
        sessionId,
      });
      if (!response?.ok) throw new Error(response?.error || "Indicator-loss revocation failed");
      if (response.result?.active !== false && activeControlSessionId === sessionId) {
        throw new Error("Indicator-loss revocation was not acknowledged as inactive");
      }
      hideControl({ sessionId });
      return true;
    } catch {
      // Failures retry on the next short watchdog tick. Never hide a still
      // active warning merely because the revocation message was interrupted.
      return false;
    } finally {
      if (controlUiLossReportPendingSessionId === sessionId) {
        controlUiLossReportPendingSessionId = "";
      }
    }
  }

  async function requestBrowserStackCheck(sessionId, state) {
    let timer = null;
    try {
      return await Promise.race([
        chrome.runtime.sendMessage({
          type: "LBB_CONTROL_UI",
          action: "indicatorCheck",
          sessionId,
          browserState: {
            hostId: state.hostId,
            markerId: state.markerId,
            viewTransitionActive: state.viewTransitionActive,
            viewport: state.viewport,
            controlHitPoints: state.controlHitPoints,
            capturing: state.capturing,
          },
        }).catch(() => null),
        new Promise((resolve) => {
          timer = setTimeout(() => resolve(null), CONTROL_BROWSER_ACK_TIMEOUT_MS);
        }),
      ]);
    } finally {
      if (timer) clearTimeout(timer);
    }
  }

  async function failClosedOnLostControlUi() {
    const sessionId = activeControlSessionId;
    if (!sessionId
      || captureDepth > 0
      || controlUiRetopDepth > 0
      || controlUiLossReportPendingSessionId === sessionId) {
      return false;
    }
    // A later hit-testable top-layer surface is a detected loss and revokes
    // before this watchdog ever reorders the bridge popover above it.
    if (!controlUiVisiblyAvailable()) return reportControlUiLoss(sessionId);

    // A clean sample is then reopened at the top-layer tail before the
    // Chrome-process point/tail proof and its separately bounded dirty check.
    controlUiRetopDepth += 1;
    try {
      if (!ensureControlUiLatestTopLayer()) return reportControlUiLoss(sessionId);
      applyCaptureVisibility();
      await waitForRenderOpportunity();
      if (activeControlSessionId !== sessionId
        || captureDepth > 0
        || controlUiRetopDepth !== 1) return false;
      const state = controlUiRenderState();
      if (!controlUiVisiblyAvailable(state)) return reportControlUiLoss(sessionId);
      const response = await requestBrowserStackCheck(sessionId, state);
      if (response?.ok && response.result?.active === true) return false;
      if (response?.ok && response.result?.active === false) {
        hideControl({ sessionId });
        return true;
      }
      return reportControlUiLoss(sessionId);
    } finally {
      controlUiRetopDepth = Math.max(0, controlUiRetopDepth - 1);
    }
  }

  function showControlStopFailure() {
    stopFailureMessage = "Stop failed—use Chrome Cancel or the extension popup.";
    const ui = createControlUi();
    ui.pill.classList.add("error");
    ui.label.textContent = "Local Browser Bridge may still be using this tab";
    ui.detail.textContent = stopFailureMessage;
    ui.stop.textContent = "Retry Stop";
    ui.stop.disabled = false;
    ensureControlUiLatestTopLayer();
    applyCaptureVisibility();
    return controlUiRenderState();
  }

  function stopPointOwnedByControl(x, y) {
    const ui = controlUi;
    if (!ui?.host?.isConnected
      || !ui.stop?.isConnected
      || typeof document.elementFromPoint !== "function"
      || typeof ui.shadow?.elementFromPoint !== "function") return false;
    if (!Number.isFinite(x) || !Number.isFinite(y) || x < 0 || y < 0 || x >= innerWidth || y >= innerHeight) {
      return false;
    }
    if (document.elementFromPoint(x, y) !== ui.host) return false;
    const shadowHit = ui.shadow.elementFromPoint(x, y);
    return shadowHit === ui.stop || Boolean(shadowHit && ui.stop.contains(shadowHit));
  }

  function trustedKeyboardStopActivation(event) {
    if (event.type !== "keydown" || !["Enter", " ", "Spacebar"].includes(event.key)) return false;
    const ui = controlUi;
    return Boolean(ui?.host?.isConnected
      && ui.stop?.isConnected
      && ui.shadow?.activeElement === ui.stop
      && document.activeElement === ui.host
      && rendered(ui.stop));
  }

  function handleControlStopActivation(event) {
    if (!event?.isTrusted || handledStopActivationEvents.has(event)) return false;
    handledStopActivationEvents.add(event);
    const sessionId = activeControlSessionId;
    if (!sessionId
      || revokedControlSessions.has(sessionId)
      || !controlUi?.host?.matches(":popover-open")) return false;

    const keyboard = trustedKeyboardStopActivation(event);
    const pointer = (event.type === "pointerdown" || event.type === "click")
      && Number(event.button) === 0
      && stopPointOwnedByControl(Number(event.clientX), Number(event.clientY));
    if (!keyboard && !pointer) return false;

    const now = performance.now();
    if (event.type === "click"
      && lastStopPointerActivation?.sessionId === sessionId
      && now - lastStopPointerActivation.at < 1_000
      && Math.abs(Number(event.clientX) - lastStopPointerActivation.x) <= 2
      && Math.abs(Number(event.clientY) - lastStopPointerActivation.y) <= 2) {
      return true;
    }
    if (event.type === "pointerdown") {
      lastStopPointerActivation = {
        sessionId,
        at: now,
        x: Number(event.clientX),
        y: Number(event.clientY),
      };
    }
    void requestControlStop(controlUi.stop);
    return true;
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
    return confirmControlUiRender();
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
    controlUiLossReportPendingSessionId = "";
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
    return confirmControlUiRender();
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

  function scheduleInitialControlReconcile() {
    // content.js runs at document_idle, which does not imply that slow images,
    // frames, or other subresources have reached readyState "complete".
    // Reconcile immediately so an authorized navigation can bind this fresh
    // exact control proof for this document without waiting for the load event.
    queueMicrotask(() => void reconcileControl());
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

  // stop-guard.js must already be present from document_start. Recovery
  // injection cannot retroactively establish listener order on an old page;
  // a missing guard leaves this false and browser control fails closed until
  // the tab is normally navigated or reloaded.
  earlyStopGuardReady = globalThis.__LOCAL_BROWSER_BRIDGE_STOP_GUARD__?.install?.(handleControlStopActivation) === true;

  addEventListener("pageshow", () => void reconcileControl());
  setInterval(() => {
    expireStaleCaptures();
    void expireStaleControl();
  }, CONTROL_WATCHDOG_INTERVAL_MS);
  setInterval(() => void failClosedOnLostControlUi(), CONTROL_UI_WATCHDOG_INTERVAL_MS);
  scheduleInitialControlReconcile();
}
