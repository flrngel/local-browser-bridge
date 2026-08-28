---
name: local-browser-bridge
description: Use Local Browser Bridge's authenticated loopback API to inspect or operate browser tabs and one native application window, or to implement and debug a compatible connector. Use when a task mentions Local Browser Bridge, its Agent Fetch URL, browser-control leases, page observations, native computer frames, or the bridge protocol. Do not use for unrelated browser automation that already has a first-party connector.
license: MIT
metadata:
  author: flrngel
---

# Local Browser Bridge

Use the narrowest bridge surface that the client actually supports. Do not
reimplement the connector WebSocket when the bearer HTTP API or Agent Fetch API
already fits the task.

Operation requires a running Local Browser Bridge and either local HTTP access
or a private Agent Fetch URL. Connector development instead requires source
access. Native computer actions also require the optional helper.

## Choose the interface

- If the user supplied a private Agent Fetch base URL and the client can only
  make GET requests, use the Agent Fetch API.
- If the client can send headers and JSON bodies, prefer
  `POST /api/v1/command` with bearer authentication.
- Read the WebSocket transport material only when implementing a browser
  extension, computer helper, or protocol-compatible server.
- If the current agent cannot load skills, use the canonical
  `docs/PROTOCOL.md` in the Local Browser Bridge repository instead. The
  installed references in this skill are byte-for-byte sections generated from
  that document.

Treat the Agent Fetch base URL, master token, and derived capability as
credentials. Never place them in source, issue text, screenshots, analytics,
command output intended for sharing, or browser history. Do not guess or scan
for credentials; obtain them through the user's installed bridge UI or an
explicitly provided environment boundary.

## Operate the bridge

For browser work:

1. Read [references/browser.md](references/browser.md).
2. Check `status` and `tabs.list` before selecting a target.
3. Start an explicit `browser.control.start` lease for the exact tab.
4. Run `page.observe`, then bind every action to the returned control session,
   turn, generation, and pointer sequence where the method accepts them.
5. Give each mutation a stable, unique `callId`. Reuse it only for an identical
   replay; never reuse it with different parameters.
6. Reobserve after mutation, staleness, cancellation, or an unknown outcome.
   Release control when the task is complete.

For native application work:

1. Read [references/computer.md](references/computer.md).
2. Check `computer.status`, select one exact window, and observe it.
3. Prefer advertised semantic element actions over coordinates. Coordinate
   actions must use the fresh frame ID and delivered image coordinate space.
4. Treat a live-share frame as short-lived authority. Reobserve after any
   action or geometry change; never retry a stale frame under a new `callId`.
5. Stop a persistent share when it is no longer needed. The helper shares the
   signed-in user's session; it is not an isolated desktop or independent input
   seat.

For GET/POST construction, cancellation, shell authority, or recovery from an
error, read [references/http.md](references/http.md). A 202 cancellation means
only that cancellation was requested. `COMMAND_OUTCOME_UNKNOWN` is terminal for
that action identity: observe and reconcile state instead of retrying the side
effect.

## Implement or debug a connector

Read [references/transport.md](references/transport.md) before changing
authentication, handshake, session replacement, command/result matching,
event sequencing, timeouts, or cancellation. Preserve all session, sequence,
origin, Host, version, capability, and queue bounds exactly. A socket connection
is not readiness; readiness begins only after the compatible authenticated
hello completes.

Then read only the surface being implemented:

- [references/browser.md](references/browser.md) for lease freshness, page
  methods, frames, dialogs, batches, waits, and key grammar.
- [references/computer.md](references/computer.md) for exact-window frames,
  semantic and coordinate input, capture sharing, and proof boundaries.
- [references/http.md](references/http.md) for REST, Agent Fetch, shell,
  idempotency, cancellation, and the error taxonomy.

## Safety and recovery rules

- Never weaken a refusal, freshness check, user-pause latch, or human approval
  boundary to make an action succeed.
- Never infer target effect from dispatch alone. Only a target-owned
  postcondition can confirm an effect.
- Follow the returned taxonomy and recovery hint. In particular, hand
  `needs_user` back to the user, reobserve stale state, and reconnect only when
  the protocol calls for it.
- Do not silently enable shell or computer authority. Those are separate,
  explicit current-user capabilities.
- Do not claim that synthetic pointer or activity diagnostics identify a human
  or physical device.
