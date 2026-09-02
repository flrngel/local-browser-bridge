# Computer use

The optional native computer helper observes and operates one selected
desktop application window. Method signatures are in
[API reference](API_REFERENCE.md#computer-methods-computer_methods-srccomputer_protocolrs);
this page covers enablement, permissions, the observe-act loop, proof layers,
and platform limits. Full mechanics: [internals/PROTOCOL.md](internals/PROTOCOL.md#native-computer-commands).

## This is a shared seat, not isolation

Read this before enabling it. The helper runs in your current login session
with your input. It does **not** create a second desktop, a separate input
queue, a virtual display, or a security boundary from your account, files, or
credentials. A sealed action can avoid moving the shared hardware cursor and
can leave the foreground application unchanged before/after — but the person
at the keyboard and the helper share one seat, and true independent
concurrency needs a VM, RDP session, or separate login instead. See
[Limitations](LIMITATIONS.md#non-interruption-is-not-isolation) for the full
argument.

## Enablement and permissions

The helper is a separate process (`local-computer-helper`, or **Start Computer
Helper** from the tray/menu-bar) that connects outbound to the server. It is
off by default; browser-only control needs neither the helper nor its
permissions.

**macOS** (13+): the packaged helper app needs two TCC grants, requested on
first use:

- **Screen Recording** — required to capture pixels and start the persistent
  ScreenCaptureKit stream. Without it, capture fails and no pixels are
  returned.
- **Accessibility** — required for semantic elements, invoking actions,
  setting values, and routing input to the exact target process/window.
  Without it, `computer.invoke`/`computer.setValue` and target-routed
  pointer/keyboard input are unavailable; the helper reports the route
  unavailable rather than falling back to global input.

Grant permissions to the packaged `.app` identity, not a raw `cargo run`
binary — TCC grants attach to application identity, and a copied inner
executable will not carry them.

**Windows**: the helper must run in the signed-in interactive session — not
Session 0, not a service, and it requests no administrator elevation for
normal use. It has no separate permission-grant step, but `inputReady` and
`semanticReady` in `computer.status` are conservative environment probes:
both are `false` if the helper cannot read the input desktop, foreground HWND,
GUI-thread focus HWND, or hardware cursor position, even before any action is
attempted.

## The observe-act-reobserve loop

1. `computer.status` — reports connected windows and readiness.
2. Select a window: `computer.observe` (one-shot snapshot) or
   `computer.share.start` (persistent stream, default max 4 FPS, 1–10
   configurable) — both return a `frameId` bound to the exact
   `(process, native window)` and its geometry.
3. Act with that `frameId`: `computer.click`, `computer.move`, `computer.drag`,
   `computer.scroll`, `computer.typeText`, `computer.key`, `computer.invoke`,
   `computer.setValue`.
4. A frame stays usable for at most **three seconds**, and only while the
   share ID, PID, native window ID, and geometry still match exactly.
   Anything else — including simply waiting too long — fails
   `COMPUTER_STALE_FRAME`; call `computer.observe` again.
5. `computer.share.stop` when done, or disconnect the helper.

An outcome-unknown mutation (a cancel, a client disconnect mid-action, a
connector timeout) immediately quarantines publication for that helper
session: further mutations return 409 `NO_COMPUTER_FRAME` until an explicit
`computer.observe` or a fresh `computer.share.start` recovers it.

## Coordinate spaces

Coordinates default to image pixels; pass `"coordinateSpace": "normalized1000"`
to use 0–1000 across the current frame instead. Without a stored frame to
convert against, a normalized-coordinate command fails `NO_COMPUTER_FRAME`
rather than guessing.

## Keys

`computer.key` shares only the chord *syntax* with browser `page.key` — the
accepted key *set* is platform-specific and narrower:

- **macOS**: Control/Alt/Shift/Meta modifiers, navigation/editing keys,
  `F1`–`F12`, ASCII letters/digits, and US-keyboard punctuation
  `= - ] ' ; \ , / . [`.
- **Windows**: Control/Alt/Shift plus the same navigation/editing keys,
  `F1`–`F12`, ASCII letters/digits, and `` ; = , - . / ` [ \ ] ' ``. Every
  Windows-key chord, `Alt+Tab`, `Alt+Escape`, `Control+Escape`, and
  `Control+Alt+Delete` are refused — those are global or secure shortcuts.

A name accepted only by `page.key` (or any other unmapped token) fails
per-action with `COMPUTER_BACKGROUND_UNAVAILABLE`. Use `computer.typeText` for
text.

## The three proof layers

Every mutation result keeps these separate — read `inputDelivery` before
trusting any claim of effect:

1. **Sealed exact-target route** (`inputDelivery`) — the resolved route,
   support level, and explicit negatives for shared-seat, global-HID, and
   cursor-mutation use. This is local helper provenance, not an OS or target
   receipt.
2. **OS API acceptance** (`osAcceptanceSignalAvailable` /
   `osAcceptanceObserved`) — whether the chosen API has a synchronous return
   signal, and that signal when available. On macOS, the private
   `SLEventPostToPid` pixel/key route returns `void`: its dispatch is
   recorded with *no* acceptance signal, and that is never described as a
   delivery receipt.
3. **Target postcondition** (`effect`) — only an application-owned read-back
   or allowlisted postcondition supports `effect: "Confirmed"`. Dispatch and
   invariant evidence alone never confirm target effect.

`cursorPositionUnchanged` and the `sharedPointer*`/`hidSystemPointerActivity*`
fields are diagnostic samples of one shared global pointer — they can
corroborate that concurrent activity happened, never identify who caused it,
and an unknown monitor state fails closed rather than assuming quiet.

## Refusal codes you will see

| Code | Taxonomy | Meaning |
|---|---|---|
| `COMPUTER_HANDSHAKE_PENDING` | `unavailable` (503) | Helper not connected yet |
| `COMPUTER_STALE_FRAME` | (409) | The `frameId` aged out or the window moved/changed identity; observe again |
| `NO_COMPUTER_FRAME` | (409) | Publication is quarantined after an outcome-unknown mutation; observe or share-start to recover |
| `COMPUTER_PERMISSION_REQUIRED` | (403, standing refusal) | Missing macOS TCC grant; grant it and relaunch the helper |
| `COMPUTER_BACKGROUND_UNAVAILABLE` | | Requested key/action has no route on this platform |
| `COMPUTER_BACKGROUND_CONTRACT_VIOLATION` | | A required foreground/focus/pointer invariant became unknown or violated; message names only a closed-vocabulary stage and invariant, never raw window/process/cursor data |
| `COMPUTER_OUTCOME_UNKNOWN` | `outcome_unknown` (504) | Native text/key dispatch lost its receiver proof mid-action; re-observe, never blind-retry |
| `COMPUTER_SHARE_SESSION_EXHAUSTED` | `unavailable` (503) | 256 retired share epochs exceeded; reconnect the helper transport |

## Platform limits (summary)

- **Capture**: one selected window only, must be on-screen and non-minimized
  when a share starts; protected/DRM content can render blank; PNG frames
  capped at 1,000,000 pixels; no audio, no video codec/WebRTC.
- **macOS**: cross-Space capture/input is not supported; a minimized target
  is not capturable; secure input and some GPU-rendered surfaces can refuse
  capture or delivery; pixel/key delivery depends on dynamically resolved,
  undocumented private SkyLight symbols that a macOS update can break (the
  helper fails closed rather than falling back to global input).
- **Windows**: UI Automation traversal is bounded (1,500 nodes, 25 levels,
  500 actionable controls, 750ms between provider calls); elevated targets,
  secure desktops, and some frameworks (Chromium, Electron, WPF, games) can
  refuse background delivery; no global `SendInput` fallback.
- Neither platform offers a native content picker or OS-managed per-window
  consent dialog — window selection happens in the bridge control page.

Full matrix: [Capabilities](CAPABILITIES.md#native-application-control) and
[Limitations](LIMITATIONS.md#native-capture-limits). Troubleshooting
permission prompts and helper crashes: [Troubleshooting](TROUBLESHOOTING.md).
