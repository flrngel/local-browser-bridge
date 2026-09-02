# Release-attempt history, v0.12.38 – v0.12.68

The last published release before this history begins was **v0.12.37**
(2026-08-26). Between then and v0.12.68 (2026-09-01, HEAD at the time this
page was written), 31 version numbers were bumped on `main`, 47 candidate
build attempts ran, and the publish workflow never ran once. This page is the
one place that history lives — the per-version narrative previously
duplicated across `docs/SOTA_AUDIT.md`, `docs/maintainers/DEVELOPMENT.md`,
`docs/WINDOWS_ACCEPTANCE_HANDOFF.md`, and several capability/limitations
pages has moved here or been removed. The prior acceptance harness itself is
preserved at tag
[`archive/operator-acceptance-harness-0.12.68`](https://github.com/flrngel/local-browser-bridge/tree/archive/operator-acceptance-harness-0.12.68)
before its removal; see [Release process](../maintainers/RELEASE.md) for
what replaced it.

## Root cause

**This was a harness-and-process stall, not a product-quality stall.** Of 38
recorded terminal failures across those 47 attempts, only 6 were genuine
product defects (all fixed on `main`, 4 of 6 verified live); 22 were bugs in
the acceptance tooling or its runbook; 8 were operator-environment slips
(a missing `rg`, an unset `GH_TOKEN`, a `find` on a missing directory under
`set -e`); 2 were self-imposed process rules. The acceptance harness — a
two-machine, largely manual Windows/macOS operator procedure — was about
**1.7×** the size of the product it was testing (~78,000 lines of
scripts/evidence/harness-tests vs. ~46,000 lines of server, extension, and
helper source), and every harness fix shipped untested until the next
one-shot, no-retry live run exposed the next harness bug. The rule "one
candidate, one attempt, bump the version on any failure" turned each trivial
tooling slip into a new version number and a fresh 47-minute rebuild, so 31
versions were consumed in under five days without a single publish. Neither
Windows stock-Chrome acceptance nor shell/Agent Fetch acceptance was ever
reached by any attempt in this window.

The practical resolution — replacing the harness with CI-hosted acceptance
(reusable GitHub Actions workflow, Chrome for Testing plus the native helper
on GitHub-hosted `windows-latest`/`macos-26` runners, no human operator
required) — is documented in [Release process](../maintainers/RELEASE.md).

## Per-version ledger

Class: **PRODUCT** = shipped server/extension/helper misbehaved · **HARNESS**
= acceptance tooling/runbook/fixture defect · **ENV** = operator machine
state or interruption · **PROCESS** = a self-imposed rule blocked it, not a
failure · **N/A** = not attempted · **UNREC** = candidate built, no retained
outcome record.

| Version | What changed | Attempts | Failed at | Class |
|---|---|---:|---|---|
| 0.12.38 | Verified one-command install | 0 | not attempted (superseded same day) | N/A |
| 0.12.39 | Shell + GET-only Agent Fetch | 3 | macOS quiet (shared-seat contamination); macOS deliberate ×2 | ENV, ENV, HARNESS |
| 0.12.40 | Anchor app-share focus evidence | 1 | **both macOS lanes passed** — withdrawn for an unrelated feature, Windows/Chrome never started | PROCESS |
| 0.12.41 | Uninstallers | 1 | unrecorded | UNREC |
| 0.12.42 | Tray/menu-bar desktop host | 2 | macOS package-inventory allowlist missing the new app bundle | HARNESS |
| 0.12.43 | Align macOS package acceptance | 4 | readiness cmd swallowed under `set -e`; input-seat contamination; HID activity; unrecorded | HARNESS, ENV, ENV, UNREC |
| 0.12.44 | Agent Skill | 5 | unrecorded | UNREC |
| 0.12.45 | Align macOS archive trust inventory | 1 | Windows: runner expected `local-browser-bridge`, host reports `local-browser-bridge-desktop` | HARNESS |
| 0.12.46 | Align Windows desktop version probe | 1 | macOS: active-receiver watcher 8s vs. server boundary 15s | HARNESS |
| 0.12.47 | Cover macOS receiver probe boundary | 1 | macOS finalizer: `export A=.. B=$A` under `set -u` produced an empty path (both lanes passed) | HARNESS |
| 0.12.48 | Harden macOS acceptance finalization | 1 | macOS deliberate: 1,000ms evidence-only age ceiling vs. actual 1,040.6ms | HARNESS |
| 0.12.49 | Allow fresh macOS share frames | 1 | Windows: Toolhelp basename filter found no exact-image child (both macOS passed) | HARNESS |
| 0.12.50 | Verify renamed Windows helper images | 1 | macOS: coordinator passed a nonexistent scratch parent | HARNESS |
| 0.12.51 | Prepare 0.12.51 | 1 | Windows trust gate: missing `GH_TOKEN` | HARNESS |
| 0.12.52 | Require noninteractive Windows trust token | 1 | macOS quiet: 2s `computer.move` not seen by fixture in 10s | HARNESS |
| 0.12.53 | Route macOS pointer moves to exact window | 1 | Same symptom, unchanged | HARNESS (speculative) |
| 0.12.54–0.12.57 | Local macOS cancellation/move diagnostics (no `main` commit) | 0 | fixture/harness timing races; unchanged package later passed 207/207 | HARNESS |
| 0.12.58 | Make macOS cancellation deterministic | 4 | Windows: worker PID stable 265 polls, Toolhelp saw zero children (3 attempts unrecorded) | HARNESS |
| 0.12.59 | Bind Windows helper readiness to authenticated process | 1 | Windows: live image path did not string-equal candidate path (alias) | HARNESS |
| 0.12.60 | Bind Windows helper image by file identity | 1 | Windows first `computer.observe`: WGC compositor frame age exceeded the monotonic range | **PRODUCT** |
| 0.12.61 | Preserve WGC frame age precision | 1 | Windows first `computer.observe`: compositor timestamp ahead of the monotonic clock | **PRODUCT** |
| 0.12.62 | Accept future WGC compositor timestamps | 1 | Windows recovery suite: helper connection ended after `computer.share.start` was enqueued (baseline observe+screenshot **passed** — first Windows pass in this span) | **PRODUCT** |
| 0.12.63 | Prepare 0.12.63 | 1 | macOS deliberate: share pump took the controller before the queued click (action-starvation race) | **PRODUCT** |
| 0.12.64 | Prioritize actions after aged share frames | 1 | macOS: product action dispatched and postcondition observed; completion receipt at 11,765ms rejected by an inconsistent 10s (vs. 18s) consumer bound | HARNESS |
| 0.12.65 | Align macOS app-share completion window | 1 | Windows recovery suite: same `COMMAND_OUTCOME_UNKNOWN` as 0.12.62 | **PRODUCT** |
| 0.12.66 | Prepare 0.12.66 | 9 | shell/tooling errors ×5; Windows `COMPUTER_HELPER_WATCHDOG` in recovery suite; Git resolver ambiguity; **both macOS lanes passed 207/207 + 231/231**, finalizer's 30-minute interval expired; unrecorded | HARNESS ×4, ENV ×3, **PRODUCT** ×1, PROCESS ×1 |
| 0.12.67 | Bound WGC capture startup and recovery evidence | 1 | Windows foreground-arm gate: no operator click arrived in 300s (macOS not run) | ENV / PROCESS (click-gate design) |
| 0.12.68 | Automate the foreground acceptance gate | 1 | not attempted (candidate built, no acceptance record before this history was written) | N/A |

## What was and was not exercised, end to end

No packaged 0.12.38+ build was ever exercised end to end on either OS across
this span:

- **macOS computer**: both packaged lanes passed repeatedly (0.12.40, 0.12.47,
  0.12.49, 0.12.58, 0.12.59, 0.12.60, 0.12.62, 0.12.65, 0.12.66). This is the
  only lane that ever reliably completed.
- **macOS browser**: no lane exists — stock-Chrome acceptance was Windows-only
  by design.
- **Windows computer**: furthest reached was trust → fixture → readiness →
  foreground-arm → baseline observe + one screenshot, then the first
  recovery-suite step (0.12.62/0.12.65/0.12.66).
- **Windows stock Chrome**: never started for any version in this span; no
  `browser-acceptance.json` exists anywhere in this repository's git history.
- **Shell / Agent Fetch** (added in 0.12.39): no acceptance lane ever
  referenced them; only CI source tests covered this surface.

## Confirmed product defects (all fixed on `main`)

1. **Windows WGC frame-age / QPC conversion** (0.12.60, 0.12.61) —
   `computer.observe` failed `COMPUTER_CAPTURE_FAILED` on first use. Fixed in
   `src/computer/share_windows.rs`. Verified live: baseline observe +
   screenshot passed on Windows in 0.12.62, 0.12.65, and 0.12.66.
2. **macOS helper share-pump action starvation** (0.12.63) — a queued click
   lost the controller to the next share pump, returning 409
   `COMPUTER_STALE_FRAME`. Fixed in `src/bin/local-computer-helper.rs`.
   Verified live in 0.12.64 and 0.12.66.
3. **Windows `computer.share.start` exceeded/killed the helper in the
   recovery suite** (0.12.62, 0.12.65, 0.12.66) — fixed in
   `src/computer/share_windows.rs` (a 10-second absolute startup/rollback
   budget) and `src/error_taxonomy.rs`. Not verified live within this
   window: 0.12.67 died at the (now-removed) operator click gate before
   reaching this path again.

Not a confirmed product defect: the 0.12.52/0.12.53 macOS `computer.move`
timeout. Product changes were made speculatively, but subsequent local
diagnostics (0.12.55–0.12.57) attributed the symptom to the test fixture and
harness ordering — the unchanged package later passed 207/207.

No product defect was found in the server, extension, shell, Agent Fetch,
installers, tray host, or Agent Skill code added in this span, because no
acceptance lane ever reached them.
