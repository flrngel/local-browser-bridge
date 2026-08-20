// Cross-origin subframe agent.
//
// This is the only bridge code that ever runs inside an out-of-process
// iframe. The service worker evaluates `dom-core.js` followed by this file
// into a dedicated CDP isolated world (`Page.createIsolatedWorld` with
// `grantUniveralAccess: false`), never the page's main world, so the frame
// cannot redefine `getBoundingClientRect` or `elementFromPoint` under the
// proofs below.
//
// Hard invariants, all statically asserted by tests/extension_contract.rs:
//
//   * read-only: no click, no focus, no event dispatch, no value write, no
//     extension messaging surface, and no CDP input domain;
//   * frame-local coordinates only: every rectangle the agent reads and
//     returns is relative to this frame's own viewport, and no request it
//     receives ever carries a top-level coordinate;
//   * `call()` is synchronous and never throws, so the background never has
//     to interpret `Runtime.evaluate`'s `exceptionDetails` shape;
//   * every request is keyed by the lease nonce handed out by `install`, so
//     an agent left over from an older lease refuses everything.

function createFrameAgent(core) {
  const FRAME_AGENT_ELEMENT_CAP = 120;
  let nonce = "";
  let agentGeneration = "";
  let refs = new Map();
  let snapshotRevision = -1;
  const revisions = core.createRevisionTracker({ isTracking: () => Boolean(agentGeneration) });

  function randomToken() {
    return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}${Math.random().toString(36).slice(2, 10)}`;
  }

  function assertLease(request) {
    if (!nonce || String(request.nonce ?? "") !== nonce) {
      throw new Error("FRAME_AGENT_STALE: this frame agent belongs to an older control lease");
    }
  }

  function assertFresh(requestedGeneration) {
    if (!agentGeneration || String(requestedGeneration ?? "") !== agentGeneration) {
      throw new Error("STALE_SNAPSHOT: observe the page again before acting");
    }
    if (snapshotRevision !== revisions.read()) {
      throw new Error(`STALE_SNAPSHOT: ${revisions.reason()}; observe the page again before acting`);
    }
  }

  function resolveRecord(request) {
    assertFresh(request.agentGeneration);
    const record = refs.get(String(request.key ?? ""));
    if (!record?.element?.isConnected) throw new Error("STALE_REF: the element changed; observe the page again");
    return record;
  }

  function install() {
    nonce = randomToken();
    agentGeneration = "";
    refs = new Map();
    snapshotRevision = -1;
    return { nonce, agentGeneration };
  }

  function snapshot(request) {
    const limit = Math.min(
      FRAME_AGENT_ELEMENT_CAP,
      Math.max(1, Number(request.limit) || FRAME_AGENT_ELEMENT_CAP),
    );
    agentGeneration = randomToken();
    refs = new Map();
    const elements = [];
    let total = 0;
    for (const element of core.composedCandidates()) {
      if (!core.visible(element)) continue;
      total += 1;
      if (elements.length >= limit) continue;
      const key = `e${elements.length + 1}`;
      const description = core.describe(element, key);
      refs.set(key, {
        element,
        ref: key,
        signature: core.targetSignature(element),
        bounds: description.bounds,
      });
      elements.push({ key, ...description });
    }
    snapshotRevision = revisions.read();
    revisions.clearReason();
    return {
      agentGeneration,
      revision: snapshotRevision,
      total,
      truncated: total > elements.length,
      origin: String(location.origin ?? ""),
      url: `${location.origin}${location.pathname}`,
      viewport: { width: innerWidth, height: innerHeight, devicePixelRatio },
      scroll: {
        x: Math.round(scrollX),
        y: Math.round(scrollY),
        maxY: Math.max(0, document.documentElement.scrollHeight - innerHeight),
      },
      elements,
    };
  }

  function run(request) {
    switch (String(request.method ?? "")) {
      case "install":
        return install();
      case "state":
        assertLease(request);
        return {
          agentGeneration,
          revision: revisions.read(),
          snapshotRevision,
          origin: String(location.origin ?? ""),
          url: `${location.origin}${location.pathname}`,
        };
      case "snapshot":
        assertLease(request);
        return snapshot(request);
      case "describe": {
        assertLease(request);
        const record = resolveRecord(request);
        return core.validateRecord(record).description;
      }
      case "prepareClick": {
        assertLease(request);
        const record = resolveRecord(request);
        const validated = core.validateRecord(record, { requireHitTest: true });
        return { ...validated.description, key: record.ref, proof: validated.proof };
      }
      case "commitClick": {
        assertLease(request);
        const record = resolveRecord(request);
        const validated = core.validateRecord(record, { requireHitTest: true });
        core.compareProof(validated.proof, request.proof);
        return { validated: true, key: record.ref, bounds: validated.description.bounds };
      }
      default:
        throw new Error("FRAME_AGENT_FAILED: unknown frame agent request");
    }
  }

  function call(request) {
    try {
      return { ok: true, result: run(request ?? {}) };
    } catch (error) {
      return { ok: false, error: String(error?.message ?? error) };
    }
  }

  return { call };
}

// Exactly one agent per isolated world, ever. `Page.createIsolatedWorld`
// hands back the SAME world for a given `worldName`, and every observation
// re-evaluates this source into it, so building a second agent would register
// a second whole-document MutationObserver plus another capture-phase scroll
// listener and resize listener, none of which the first agent ever releases:
// thirty observations of three frames would leave ninety live observers on
// exactly the third-party iframes this feature exists for. The guard mirrors
// content.js's __LOCAL_BROWSER_BRIDGE_CONTENT__ one; re-keying per lease is
// the `install` request's job, not a rebuild's.
function installFrameAgentOnce() {
  if (!globalThis.__LBB_FRAME_AGENT__) {
    globalThis.__LBB_FRAME_AGENT__ = createFrameAgent(globalThis.__LBB_DOM_CORE__({}));
  }
  return globalThis.__LBB_FRAME_AGENT__;
}

// Installed only by the source the service worker evaluates into a frame's
// isolated world; importing this file into the worker itself just publishes
// the builder, because a worker has no document to observe.
if (!globalThis.__LBB_FRAME_AGENT_SOURCE__) {
  globalThis.__LBB_FRAME_AGENT_SOURCE__ = () => `${createFrameAgent}\n${installFrameAgentOnce}\ninstallFrameAgentOnce();\n`;
}
