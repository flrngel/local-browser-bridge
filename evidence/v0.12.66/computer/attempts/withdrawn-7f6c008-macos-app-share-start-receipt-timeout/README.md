# Withdrawn macOS candidate: app-share start-receipt timeout

This directory preserves the sanitized macOS negative result from workflow run
`33350856503`, attempt `4`. It is not release evidence.

The frozen v0.12.66 candidate was bound to source commit
`2dcc8eccebaed6837a3eb71012e4826d7e9de922`, release-candidate artifact
`9751125754`, and raw artifact ZIP SHA-256
`7f6c008b06bd5bb5503ddf46539765dd5bd09f26d2332ca4943926922c44070c`.
The canonical checksum manifest SHA-256 was
`8a0833bfdd3ba2b4f0ce55d663e001293103a922b32db1d91f46010419a0452c`,
and the packaged macOS archive SHA-256 was
`57136a81cbc9d743c04ce49908e1aecad1fb22a6ff72ed3aa58244f193d29a80`.

The fresh packaged quiet lane passed 207 of 207 checks. Its result SHA-256 is
`c1c6cced766ae79530bc218676e98c656f434ce075d3e13889aecd06bcb11127`,
and its log SHA-256 is
`686f106be5bb2e0783fb42eb9d90e52238bec1a29178370eb74cf88fe59eb804`.
All six screenshots passed mandatory visual review: the primary fixture alone
showed the quiet lane, exact v0.12.66 semantic value, completed semantic action,
persistent stream, one background pixel click, and settled 820x520 resize.

The deliberate-concurrency lane then passed its fresh source-only readiness
gate and its 60-transition native quiet-seat gate. It reached the exact-app
READY presentation without observed foreground, focus, cursor, HID, or active
Space changes. The create-once request marker bound bundle
`dev.flrngel.local-browser-bridge.acceptance.app-share`, window
`LBB macOS Acceptance App Share`, button `START APP-SHARE CHECK`, and
Accessibility identifier `lbb-app-share-start`.

The separately bounded Mac Codex watcher task
`01a056f8-6eda-7162-8608-fb5a68d898ec` observed the exact bundle, window, and
button. Its retained observation did not contain the `window` value required by
the authorized exact call `sky.click({window: observation.window,
element_index})`. The watcher therefore failed closed: it made no `sky.click`
call, used no coordinates, performed no keyboard or pointer action, and did not
retry. No app-share button click occurred.

Only the request marker exists. No start marker and no completion marker were
created. At `2026-08-31T09:04:22.764Z`, the runner terminated with the exact
fatal reason `fresh exact-app-share start receipt timed out`. The schema-9
negative result records `failed-release-candidate`, 85 completed checks, six
fixture-only screenshots, `startReceiptAcknowledged: false`,
`acceptanceButtonActionObserved: false`, and `actionDispatched: false`. The
candidate server, helper, and fixture then stopped, and the scratch directory
was removed.

The deliberate negative result SHA-256 is
`27bdde60a2b42eadb87d549e951a9f0bdc59fcb5787e68a09038e15c00049eb9`,
its log SHA-256 is
`d1f427b7c722f2f1a09136df6ea3f3ec79d49a45aa1dec05c786c46ec2aa149f`,
and the sole request-marker SHA-256 is
`71228ae89cc2b9bfcdd1b9622b9209a7d26f818287f539b78f6a0f9f63b506de`.

The retained generated inventory is byte-exact and limited to 17 allowlisted
files: each lane's six exact-window fixture screenshots, result, and log, plus
the deliberate lane's sole request marker. All twelve images were visually
checked to contain only the primary fixture and no unrelated or sensitive
pixels. The inventory excludes packages, executables, scratch and socket data,
absolute paths, credentials, raw desktop data, watcher transcripts, control
channel material, and operator-only review manifests.

The lane was not retried or resumed. No macOS aggregate, Windows acceptance,
stock-Chrome acceptance, evidence finalization, tag, publication, or GitHub
Release followed from this attempt. These retained files cannot be promoted,
combined with another attempt, relabeled as passing evidence, or reused by a
later workflow attempt. Continuing the release requires a complete workflow
rerun with a new attempt, artifact, evidence set, and receipt.
