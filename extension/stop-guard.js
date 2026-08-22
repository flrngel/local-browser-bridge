// This isolated-world listener is registered at document_start, before page
// scripts can install window/document capture listeners. It does not decide
// whether an event may stop control: content.js later installs the verifier
// that owns the closed shadow root and exact control-session state.
if (!globalThis.__LOCAL_BROWSER_BRIDGE_STOP_GUARD__) {
  const seenEvents = new WeakSet();
  let activationHandler = null;

  const forwardTrustedActivation = (event) => {
    if (!event?.isTrusted || seenEvents.has(event) || typeof activationHandler !== "function") return;
    seenEvents.add(event);
    try { activationHandler(event); } catch {}
  };

  for (const type of ["pointerdown", "click", "keydown"]) {
    // Window is intentional: it precedes hostile window/document capture
    // listeners registered later by the page. The document listener is a
    // redundant same-isolated-world path; seenEvents makes forwarding once.
    window.addEventListener(type, forwardTrustedActivation, true);
    document.addEventListener(type, forwardTrustedActivation, true);
  }

  const guard = Object.freeze({
    install(handler) {
      if (typeof handler !== "function") return false;
      activationHandler = handler;
      return true;
    },
  });
  Object.defineProperty(globalThis, "__LOCAL_BROWSER_BRIDGE_STOP_GUARD__", {
    value: guard,
    configurable: false,
    enumerable: false,
    writable: false,
  });
}
