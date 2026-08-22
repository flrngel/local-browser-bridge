# Windows v0.12.2 stock-user-Chrome acceptance protocol

This directory defines the gated, human-operated browser acceptance run for
v0.12.2 on the designated interactive Windows acceptance host. It is evidence
infrastructure, not passing release evidence. Do not mark the release accepted
until the exact frozen candidate passes this protocol and
`browser-acceptance.json` is finalized.

The run uses the already installed, ordinary Google Chrome 140 or later and
its existing user session. The extension is loaded manually from
`chrome://extensions`.
Do not relaunch Chrome, add browser flags, enable a remote-debugging port, use
`--load-extension`, use a disposable profile, call CDP directly, or use an
extension-loading test API. Browser UI work, including Chrome's own **Cancel**
button, must go through the Google Chrome MCP connection. The evidence scripts
never launch or relaunch Chrome and never load an extension through a hidden or
test API; the API matrix mutates only the bridge-created test tabs described
below.

Only localhost demo pages may be used. Never open, inspect, act on, or retain a
screenshot of an unrelated tab, account, extension, bookmark, download, avatar,
notification, desktop area, or user file.

## Evidence boundary

Retain only these files:

- `candidate-preflight.json` and `candidate-postflight.json`;
- `browser-api-matrix.json` from the machine-executed API driver;
- eleven sanitized PNGs and their eleven JSON sidecars;
- a candidate-bound `operator-results.json` initialized from the template;
- `browser-acceptance.json`.

Do not retain API response bodies, terminal output, PowerShell transcripts,
Google Chrome MCP transcripts or commands, the bearer credential, filesystem
locations, tab or window IDs, raw extension storage, Chrome account/profile
state, or raw screenshots. The reduced `tokenConfigured:false` cleanup result is
allowed; no token value or storage dump is. IDs, generations, refs, coordinates,
and call IDs may exist in memory during the run but must not enter retained
evidence.

The screenshot tool strips source metadata by decoding and re-encoding the
image, rejects retained text, EXIF, ICC-profile, and timestamp chunks, records
dimensions and SHA-256, and uses OCR denylist checks when Tesseract is
available. OCR cannot identify every sensitive pixel and performs no pixel
redaction. A human must inspect every tight crop before setting
`-ManualVisualReviewConfirmed`.

## 1. Freeze and bind the candidate

The release coordinator supplies these values through a channel independent of
the downloaded candidate:

- the stable version, exactly `0.12.2`;
- the 40-character lowercase `FINAL_SHA`;
- the lowercase SHA-256 of `SHA256SUMS.txt`.

Do not calculate the last value from the same manifest and treat that as an
external binding. The manifest itself must be LF-terminated ASCII with exactly
these four lines, in this order, using two ASCII spaces between each lowercase
hash and filename:

```text
<64 hex>  local-browser-bridge-v0.12.2-windows-x86_64.exe
<64 hex>  local-computer-helper-v0.12.2-windows-x86_64.exe
<64 hex>  local-browser-bridge-v0.12.2-macos-universal.tar.gz
<64 hex>  local-browser-bridge-extension-v0.12.2.zip
```

Resolve the unpacked-folder identity before extracting anything:

1. For an upgrade, release control and disable the existing Local Browser
   Bridge card before changing any byte in its backing folder.
2. Use that card's already known, operator-owned stable unpacked folder as
   `$ExtensionDirectory`; replace its contents with the verified ZIP's exact
   eleven-file inventory, then re-enable and reload that same card after preflight.
   Do not click **Load unpacked** again.
3. If the stable folder is not already known and operator-owned, stop instead
   of guessing; never infer ownership from a folder name alone. As an
   alternative, remove the old card only after the user
   confirms it is test-owned, then extract into a brand-new empty folder and
   load that folder once.
4. For a new installation with no existing card, use a brand-new empty folder.

Never load a second folder while an older card remains.
More than one matching card is a failed preflight; do not delete an unowned
card merely to make the evidence pass. `Expand-Archive -Force` below is allowed
only after this identity decision and only for that empty or confirmed
product-owned destination. The binder rejects any stale or extra extracted
entry.

Use a detached, clean checkout at `FINAL_SHA`. Keep all scratch and evidence
outputs outside that checkout. In an acceptance-only PowerShell 5.1+ window:

```powershell
Set-PSReadLineOption -HistorySaveStyle SaveNothing
$Version = "0.12.2"
$FinalSha = "REPLACE_WITH_COORDINATOR_FINAL_SHA"
$ManifestSha = "REPLACE_WITH_COORDINATOR_SHA256SUMS_SHA256"
$Repository = "REPLACE_WITH_CLEAN_DETACHED_CHECKOUT"
$Candidate = "REPLACE_WITH_FROZEN_CANDIDATE_DIRECTORY"
$ExtensionDirectory = "REPLACE_WITH_EXACT_FOLDER_CHROME_WILL_LOAD"
$EvidenceDirectory = "REPLACE_WITH_NEW_PRIVATE_EVIDENCE_DIRECTORY"

git -C $Repository rev-parse HEAD
git -C $Repository status --porcelain=v1 --untracked-files=all
Expand-Archive -LiteralPath "$Candidate\local-browser-bridge-extension-v$Version.zip" `
  -DestinationPath $ExtensionDirectory -Force

& "$Repository\scripts\browser-evidence-candidate.ps1" `
  -Mode Preflight `
  -Version $Version `
  -FinalSha $FinalSha `
  -Repository $Repository `
  -ChecksumManifest "$Candidate\SHA256SUMS.txt" `
  -ChecksumManifestSha256 $ManifestSha `
  -ServerExecutable "$Candidate\local-browser-bridge-v$Version-windows-x86_64.exe" `
  -ExtensionZip "$Candidate\local-browser-bridge-extension-v$Version.zip" `
  -ExtractedExtension $ExtensionDirectory `
  -OutputRecord "$EvidenceDirectory\candidate-preflight.json"
```

The binder refuses a dirty or wrong checkout, prerelease SemVer, a noncanonical
manifest, wrong server or ZIP name/hash, ZIP links/directories/extra entries,
nondeterministic ZIP timestamps, any extracted extra file, mismatched
`manifest.json`/`lib.js` versions, a Chrome floor other than 140, changed or
reordered permission/host-permission arrays, any noncanonical background,
action, content-security-policy, or content-script stage (including the early
`stop-guard.js` `document_start` ordering), and any payload byte that differs
from the clean checkout. Its allowlist is exactly the eleven files emitted by
`scripts/package-extension.sh`.

Preflight also generates a fresh 256-bit run nonce. Every later reduced
artifact carries the same exact `candidateBinding` object: that nonce, the
preflight-record digest, `FINAL_SHA`, checksum-manifest digest, server digest,
extension-ZIP digest, and extracted-payload-tree digest. The values identify
only this frozen candidate/run domain; they contain no path, browser ID, token,
or profile state. Initialize the operator checklist now so its binding fields
are copied mechanically rather than typed:

```powershell
& "$Repository\scripts\write-browser-evidence-record.ps1" `
  -Mode InitializeOperator `
  -PreflightRecord "$EvidenceDirectory\candidate-preflight.json" `
  -OutputRecord "$EvidenceDirectory\operator-results.json"
```

`$ExtensionDirectory` is therefore the machine-verified unpacked payload: its
exact eleven-file inventory and every file byte match both the frozen extension
ZIP and the clean checkout at `FINAL_SHA`. Do not browse to or select any other
directory in Chrome. This preflight must pass before **Load unpacked** is
clicked.

## 2. Start the exact candidate server without a credential file

Stay in the same no-history PowerShell window. Generate the acceptance
credential in memory and pass it through the environment, not an argument or
file. Do not redirect or capture server output.

```powershell
$bytes = New-Object byte[] 32
$rng = [Security.Cryptography.RandomNumberGenerator]::Create()
try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
$env:LBB_TOKEN = [Convert]::ToBase64String($bytes).TrimEnd("=").Replace("+", "-").Replace("/", "_")
$env:LBB_PORT = "17373"
$ServerProcess = Start-Process `
  -FilePath "$Candidate\local-browser-bridge-v$Version-windows-x86_64.exe" `
  -ArgumentList "--no-update-check" `
  -NoNewWindow `
  -PassThru
```

Never take a terminal screenshot. The server binary hash is already part of
the candidate binding. Confirm it listens only on `127.0.0.1:17373`.

The following helper keeps JSON responses in memory and supports expected HTTP
errors without writing them. Never pipe its result to a file, transcript,
clipboard, or `Tee-Object`.

```powershell
$BridgeBase = "http://127.0.0.1:17373"
$BridgeHeaders = @{ Authorization = "Bearer $env:LBB_TOKEN" }
$script:CallOrdinal = 0

function Invoke-BridgeCommand {
  param([Parameter(Mandatory=$true)][string]$Method, [hashtable]$Params = @{})
  $script:CallOrdinal += 1
  $callId = "browser-accept-$($script:CallOrdinal)-$([Guid]::NewGuid().ToString('N'))"
  $body = @{ method = $Method; params = $Params; callId = $callId } | ConvertTo-Json -Depth 12 -Compress
  try {
    $response = Invoke-WebRequest -UseBasicParsing -Uri "$BridgeBase/api/v1/command" `
      -Method Post -Headers $BridgeHeaders -ContentType "application/json" -Body $body
    return [pscustomobject]@{ Status = [int]$response.StatusCode; Body = ($response.Content | ConvertFrom-Json) }
  } catch {
    $response = $_.Exception.Response
    if ($null -eq $response) { throw }
    $reader = New-Object IO.StreamReader($response.GetResponseStream())
    try { $content = $reader.ReadToEnd() } finally { $reader.Dispose() }
    return [pscustomobject]@{ Status = [int]$response.StatusCode; Body = ($content | ConvertFrom-Json) }
  }
}

function Wait-ReducedControlStatus {
  param([bool]$Active, [bool]$HumanPaused, [AllowNull()][string]$Reason)
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  do {
    $reply = Invoke-BridgeCommand -Method "browser.control.status"
    $control = $reply.Body.result
    $observedReason = if ($null -ne $control.humanPause) { $control.humanPause.reason } else { $null }
    if ($reply.Status -eq 200 -and $control.active -eq $Active -and
        $control.humanPaused -eq $HumanPaused -and
        $control.revocationPending -eq $false -and $observedReason -eq $Reason) {
      return
    }
    Start-Sleep -Milliseconds 200
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "browser.control.status did not reach the required reduced state"
}
```

## 3. Load through real `chrome://extensions`

Using the Google Chrome MCP connection to the user's already running Chrome:

1. Open a new, dedicated temporary Chrome window.
2. Navigate that window to `chrome://extensions`.
3. Note only in memory whether **Developer mode** was already enabled, then
   enable it if necessary. Do not persist the initial profile setting.
4. Follow the one-identity upgrade rule above. For a new installation, click
   **Load unpacked** once and select the exact `$ExtensionDirectory` that
   passed candidate preflight. Do not screenshot or retain the path.
5. Filter visually only to Local Browser Bridge. Confirm exactly one card,
   enabled, version `0.12.2`, no load errors, and one extension ID matching 32
   letters `a` through `p`. Record only `idPatternValid: true`; never copy the
   actual ID into evidence.
6. Open **Details** and review Chrome's user-visible permission and host-access
   disclosures for this card. Chrome does not display every raw MV3 permission
   key, so the candidate binder—not this UI—is the authority for the exact
   ordered manifest arrays: `tabs`, `scripting`, `storage`, `alarms`,
   `debugger`, and `tabGroups`, plus HTTP, HTTPS, and file host patterns. Do not
   inspect other extension cards. Capture `extension-details` as a tight crop
   of only Chrome's disclosed permission and host-access UI; exclude the
   extension ID.
7. Open the extension popup, keep port `17373`, enter the in-memory credential,
   and select **Save and connect**. The password field clears immediately.
   Confirm `Version 0.12.2` and a connected status before capturing the popup.

The early trusted Stop guard is installed at `document_start`. A document that
was already open when this extension version loaded cannot gain that ordering
through late injection: observation may recover, but control must fail closed.
Never reload an unrelated user tab. Reload only an operator-owned local test
tab if one must be reused. This protocol avoids that ambiguity: the API matrix
creates its dedicated `/demo` tab through `tabs.new` only after the candidate
extension is loaded, connected, and version-checked.

At this point take the `extensions-card`, `extension-details`, and
`popup-connected` tight crops.
The extension-card crop must exclude the actual extension ID line. The token
field must be empty and no other extension, toolbar item, avatar, tab,
bookmark, or account detail may appear.

## 4. Exercise every browser command family on localhost

Full Access may be enabled for this local-only matrix. The driver creates its
own `http://127.0.0.1:17373/demo` target with the bridge's policy-checked
`tabs.new { url }` command, rather than controlling a top-level `about:blank`.
It also creates and closes one secondary blank tab to prove the tab lifecycle,
never targets a baseline user tab, stops its control lease, and leaves only its
demo tab active in the dedicated window for the visual and handback tests
below. In the reduced matrix, `cleanupComplete:true` means the transient blank
and control lease were cleaned up and the active demo was intentionally handed
back; the operator must close that demo at the final cleanup boundary.
The required `tabs.list` call returns the extension's reduced inventory in
memory; the driver uses only ID equality to select the two IDs returned by its
own `tabs.new` calls and neither reads other fields from unrelated entries nor
retains the inventory.

Run the driver in a child Windows PowerShell process. The child inherits the
in-memory credential and erases its own copy on exit; no credential appears in
an argument or evidence file, and the parent acceptance shell keeps its copy
for the handback checks.

```powershell
$WindowsPowerShell = "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"
$MatrixHandoffJson = & $WindowsPowerShell -NoLogo -NoProfile -NonInteractive `
  -File "$Repository\scripts\test-windows-browser-api.ps1" `
  -Version $Version `
  -Port 17373 `
  -PreflightRecord "$EvidenceDirectory\candidate-preflight.json" `
  -OutputPath "$EvidenceDirectory\browser-api-matrix.json" `
  -PassThruOwnedTarget
if ($LASTEXITCODE -ne 0) { throw "The browser API matrix failed." }
$OwnedTarget = $MatrixHandoffJson | ConvertFrom-Json
$MatrixHandoffJson = $null
$PreflightInMemory = Get-Content -LiteralPath "$EvidenceDirectory\candidate-preflight.json" -Raw | ConvertFrom-Json
if ($OwnedTarget.runNonce -cne $PreflightInMemory.runNonce -or
    [long]$OwnedTarget.tabId -le 0 -or [long]$OwnedTarget.groupId -lt 0 -or
    $OwnedTarget.focusedByExactOwnedTabActivation -ne $true) {
  throw "The exact bridge-owned target handoff is invalid."
}
$PreflightInMemory = $null
```

The passing reduced record is the coverage authority. Besides its seven-field
candidate binding, it contains only the exact method name, fixed stage, derived
pass, three proof booleans, canonical screenshot filename or `N/A`, fixed
machine-proof label, and thirteen aggregate booleans.
It records no response body, URL, tab/window/session ID, ref, generation,
coordinate, call ID, token, path, stdout, Chrome command, or profile state.
The child emits one separate handoff object into `$OwnedTarget`; it exists only
in the parent process memory and contains the exact bridge-returned tab/group
IDs, the run nonce, and a verified-focus boolean. Do not print, log, serialize,
or retain that object.
Do not infer coverage from the advertised list. The driver actually calls all
25 methods, uses a fresh call identity for every command, re-observes after
each page mutation, verifies Chrome 140+, exercises a real dialog lifecycle,
proves the control pill and Stop surface survive page top-layer competition,
proves authorized navigation and reload replace the public per-document host
identity while the browser-process gate resolves the new secret closed-shadow marker,
and polls `browser.control.status` until machine releases are inactive,
unpaused, and cleanup-complete. The marker value never enters the page, API
record, terminal, or retained evidence.

The `topLayerControlUiIntegrity` aggregate is a real stock-Chrome cell, not a
source-only claim. Before the extension creates its host, `/demo.css` has
already installed hostile generic popover declarations that collapse and mask
the host, opaque `::before`/`::after` content with `pointer-events:none`, and an
opaque noninteractive `::backdrop`. The driver requires the live host's
computed opacity, filter, mask, and transform to be safe, requires both host
pseudo-elements to have suppressed content and display, and—when Chrome exposes
computed backdrop values—requires a transparent backdrop with no image,
filter, mask, clipping, transform, or pointer interaction. These raw computed
details stay in memory. The later
`page-control-pill` screenshot must still visibly show real page content plus
the pill and Stop surface; that human-reviewed crop is the pixel-level proof.
Computed style, layout, and hit testing are renderer-state checks, not proof of physical display or compositor output,
and a page can race after any sampled instant. The screenshot proves only its
captured instant. Chrome's independent
browser-owned debugger notice remains the trusted handback surface if page UI
is later suppressed.

The browser-process proof has three independent bindings. Chrome's experimental
`DOM.getTopLayerElements` result must contain the expected host, a unique random
marker inside the expected closed shadow tree must resolve back to that exact
host and the controlled root document, and five stable pill/Stop points must
resolve to the host with `DOM.getNodeForLocation(ignorePointerEventsNone:true)`.
After skipping child-document nodes, that exact host must also be the tail of
the controlled root document's top-layer order; this catches sparse coverage
between the sampled points without treating a child document's independent
stack as a main-page occluder. Any unavailable, errored, stale, ambiguous,
cross-document, or mismatched proof fails closed.
This protocol intentionally gates Chrome 140+ because these experimental CDP
details are version-sensitive; the live conformance cells below remain
mandatory for every candidate.

Before any hostile cell, two complete bridge-owned evaluate/automatic-observe
cycles must remain active. Those cycles deliberately cause the extension's own
hide/show and root top-layer events, and prove the genuine host remains the
exact direct child of `document.documentElement`, open, visible, non-inert, and
`aria-hidden="false"`, while the controlled root remains visible, non-inert,
and not aria-hidden. Bridge-owned events must not consume a later page-loss
signal or cause a false revocation.

First, a benign manual popover inside a same-process `srcdoc` iframe remains
open across a controlled main-page action. The driver repeats that check with
33 additional same-origin `srcdoc` frames and open child popovers, beyond the
old 32-frame bound. Neither cell may falsely revoke the main-document pill
merely because Chrome reports more than one document's top-layer elements.
Next, the driver opens the demo's opaque, full-viewport,
`pointer-events:none` manual page popover. The watchdog must re-top the real
host and the next action succeeds only after both browser-process proofs bind
that host; the passive fixture stays open below it for the later screenshot.
A later pointer-active popover must instead trigger exact
`control_ui_hidden` revocation.

The remaining negative cells are deliberately adversarial. One opaque passive
popover hides and re-shows itself every animation frame; a narrow opaque
pointer-passive strip deliberately avoids the five stable hit-point rows while
re-topping in every frame; one page-created fake copies the public host ID and
attributes after renaming the real host but lacks the secret closed-shadow
marker; a second case leaves the genuine ID unchanged while a different exact
page object simultaneously copies that ID and re-tops itself; a long View Transition
paints above the top layer; and animation-frame loops repeatedly
apply `hidden`, `aria-hidden:true`, and `inert` first to the real host and then
independently to `document.documentElement`. Three more cases reparent the
exact host under a `display:contents` light-DOM, open-shadow, or closed-shadow
wrapper. Every wrapper must revoke: a wrapper, matching public attributes, or
closed tree does not substitute for the exact host being the direct child of
the controlled root. For each cell, the driver proves control is active
immediately before the scheduled attack, then polls only
`browser.control.status`—which does not call the UI repaint/reuse path—until
the exact `control_ui_hidden` release with `active:false`, `humanPaused:false`,
and `requiresExplicitStart:true`. The passive/sparse/fake/View-Transition/
accessibility cells must release within one 500 ms watchdog interval plus a
2.5 second browser/message scheduling margin, well before the independent
ten-second heartbeat. A separate command/automatic-observation cell reacts to
the extension's own host-open mutation, installs an opaque passive top-layer
cover, and blocks the owned localhost renderer for five seconds. The retained
service-worker revocation timestamp must be strictly after the page attack,
within the three-second deadline plus a one-second scheduling tolerance, and
before the renderer stall ends. This proves the dirty deadline is independent
of the in-flight renderer and former mutation-depth window. Timings, status
bodies, DOM identities, raw attributes, and computed details stay in memory;
the aggregate retains only `topLayerControlUiIntegrity:true`.

Each hostile fixture restores its exact prior local state or proves the old
extension-owned host was retired, removes every wrapper/occluder it created,
and then requires an explicit restart plus fresh observation. The new genuine
host must again be a direct child of the root. A final burst of four real screenshot observations
spans more than two watchdog intervals and must
remain active, proving intentional capture hiding does not look like hostile
suppression. The page also installs later window/document capture listeners
for `pointerdown`, `click`, and `keydown` that call
`stopImmediatePropagation()` on control-host events. The physical Stop cell
below must still release through the earlier `document_start` guard.

Snapshot freshness has a separate exact-object negative. A page-created
`display:contents` wrapper copies the randomized public host ID, then reparents
the local demo form and mutates its action after a snapshot was published.
Mutation exclusion must trust only the retained genuine host and closed-shadow
objects: the stale click must fail HTTP 409 `STALE_SNAPSHOT` with recovery hint
`reobserve`. Only an explicit fresh observation and a fresh ref may authorize
the next click. The fixture then restores the exact form parent, sibling, and
action and removes its duplicate-ID wrapper. It performs no second mutation
outside the fake before the stale-ref refusal, so an unrelated page mutation
cannot make this negative pass.

Every row below is machine-proven by the allowlisted method result in
`browser-api-matrix.json` and follows the exact canonical record order; the
driver may execute dependency steps in a different safe order. Each method
record contains only its name/stage, `passed`, the three independently required
booleans `commandInvoked`, `resultVerified`, and `postconditionVerified`, an
exact retained screenshot filename or the literal `N/A`, and the fixed
`machineProof` value `machine-command-result-postcondition`. It never contains
the command, response, postcondition values, IDs, refs, coordinates, or paths.
`passed:true` is accepted only when all three proof booleans are true. A visual
filename is supplemental to—not a replacement for—the machine proof; `N/A`
explicitly means the stronger machine-verifiable artifact is authoritative for
that nonvisual/status/security method.

| Record item | Method | Local-only postcondition | Retained screenshot field |
|---:|---|---|---|
| 1 | `status` | Connected, compatible v0.12.2 extension and 25 advertised methods | `browser-03-popup-connected.png` |
| 2 | `browser.control.start` | Start an explicit lease on the demo tab | `browser-04-native-debugger-warning.png` |
| 3 | `browser.control.status` | `active:true`, `humanPaused:false`, cleanup not pending | `N/A` — reduced state is stronger |
| 4 | `browser.control.stop` | Client release returns inactive without human pause | `N/A` — exact reduced state is stronger; in-page Stop evidence below is a distinct human handback trigger |
| 5 | `tabs.list` | Reconcile only bridge-created tabs; IDs stay in memory | `N/A` — bounded inventory assertions are stronger |
| 6 | `tabs.activate` | Activate only the two bridge-created test tabs | `N/A` — returned active target binding is stronger |
| 7 | `tabs.new` | Create a policy-checked demo and a secondary `about:blank` | `N/A` — returned ownership plus reconciliation is stronger |
| 8 | `tabs.close` | Close only the secondary blank tab | `N/A` — absence from the next inventory is stronger |
| 9 | `page.observe` | Fresh generation, refs, viewport, screenshot, and lease bindings; capture/watchdog overlap stays active | `N/A` — bounded screenshot/viewport assertions run in memory |
| 10 | `page.navigate` | Navigate to `/demo?step=2`, verify route text, and prove a new per-document host plus gated marker | `N/A` — route and marker rotation are stronger |
| 11 | `page.back` | Return to `/demo` | `N/A` — exact route postcondition is stronger |
| 12 | `page.forward` | Return to `/demo?step=2` | `N/A` — exact route postcondition is stronger |
| 13 | `page.reload` | Reload, rotate the public host identity, resolve the new secret marker, and reobserve | `N/A` — marker rotation and fresh observation are stronger |
| 14 | `page.click` | Submit and observe the expected greeting | `browser-06-action-result.png` |
| 15 | `page.fill` | Fill **Display name** with nonsensitive fixture text | `browser-06-action-result.png` |
| 16 | `page.select` | Select **Blue** | `browser-06-action-result.png` |
| 17 | `page.key` | Send `End`; the demo key log reports the key | `N/A` — exact key-log postcondition is stronger |
| 18 | `page.scroll` | Boundedly bring targets into view, including the bottom marker, then reobserve | `N/A` — fresh viewport/element assertions are stronger |
| 19 | `page.clickAt` | Click fresh coordinates; log says `coordinate:true` | `N/A` — exact local log postcondition is stronger |
| 20 | `page.typeText` | Type fixture text into a trusted focused field | `N/A` — exact field-value postcondition is stronger |
| 21 | `page.evaluate` | Inspect local state; run iframe, passive/perpetual/sparse, duplicate-object, root/host/wrapper, renderer-stall, View-Transition, and snapshot-exclusion cells; separately schedule a local `confirm` | `browser-05-page-control-pill.png` |
| 22 | `page.waitFor` | Wait for deterministic visible text | `N/A` — deterministic match result is stronger |
| 23 | `page.hover` | Hover a freshly observed local button ref | `N/A` — hover result plus fresh target binding is stronger |
| 24 | `page.batch` | Run one snapshot-bound `page.scroll` sub-action | `N/A` — ordered sub-result and fresh observation are stronger |
| 25 | `page.handleDialog` | Dismiss the recorded confirm with `accept:false` | `N/A` — recorded dialog lifecycle is stronger |

After the matrix succeeds, never rediscover a target by public URL, title, tab
order, folder name, or current focus. Use only `[long]$OwnedTarget.tabId`, which
is the exact ID returned by the driver's policy-checked `tabs.new`. Call
`tabs.activate` on that exact ID before every Google Chrome MCP visual step;
the extension activates the exact tab and focuses its containing window. Then
call `tabs.list` only to prove `activeTabId` equals that same in-memory ID and
that exactly one matching entry is active. Stop on any mismatch instead of
selecting another demo-looking tab. The operator must also visually confirm
that the focused window is the dedicated temporary window created in step 3;
only then set `dedicatedWindowBoundToOwnedTarget:true` in the operator record.
Start a new explicit lease on that exact ID without navigating or reloading: the
still-open, pointer-pass-through page popover is the visual half of the
top-layer cell. Require `page.waitFor` to find `Route: /demo?step=2`, then use
fresh `page.observe` results to fill **Display name** with `Bridge Matrix`,
select **Blue**, and click **Show greeting**. Require `Hello, Bridge Matrix.
blue selected.` before taking the `action-result` crop. Never reuse a ref or
generation after a mutation.

With that lease active, confirm Chrome shows its browser-owned
**Local Browser Bridge started debugging this browser** notice and the page
independently shows **Local Browser Bridge is using this tab**, its virtual
pointer, and **Stop** above the visible **Page top-layer acceptance fixture**.
Capture separate tight crops named `native-debugger-warning` and
`page-control-pill`; the latter must include only enough of the named fixture
to prove ordering. Then capture the local demo's verified action result as
`action-result`.

## 5. Prove Stop, Cancel, and trusted Resume

These are human handback tests. Do not invoke page JavaScript, the extension
debugger API, accessibility scripts, or DOM `.click()` to trigger them.

### In-page Stop

1. With a live explicit lease, use Google Chrome MCP's normal UI click on the
   visible in-page **Stop** button. The demo's armed window/document capture
   listeners attempt `stopImmediatePropagation()`; do not disable them.
2. Poll `browser.control.status` with `Wait-ReducedControlStatus -Active $false
   -HumanPaused $true -Reason "released_by_user"`. Do **not** poll raw
   `/api/state`; the asynchronous revocation event may precede the complete
   pause state, while the command result publishes the authoritative reduced
   control status.
3. Confirm both the Chrome warning and page control overlay are gone. Capture
   `stop-after` as a tight crop of the former page-indicator area.
4. Call `browser.control.start` and `tabs.new`. Each must fail HTTP 423 with
   `HUMAN_CONTROL_PAUSED`, taxonomy state `needs_user`, action `handback`, and
   `retriable:false`. Neither may reach the extension mutation path.
5. Open the extension popup and capture `stop-paused-popup`. Click the popup's
   **Resume remote control** button with Google Chrome MCP.
6. Poll `browser.control.status` until inactive, `humanPaused:false`, and no
   revocation pending. Then call `browser.control.start`, require success, and
   poll `browser.control.status` again until active.

### Chrome-native Cancel

1. With the restarted lease active, use Google Chrome MCP to click **Cancel**
   in Chrome's browser-owned debugging notice. Do not click the page pill.
2. Poll `browser.control.status` until inactive, `humanPaused:true`, reason
   `canceled_by_user`, and no revocation pending.
3. Confirm both visible indicators are gone and capture `cancel-after` as a
   tight crop of the former page-indicator area.
4. Again require HTTP 423 `HUMAN_CONTROL_PAUSED` for both
   `browser.control.start` and `tabs.new`, with the same taxonomy fields.
5. Capture the paused popup as `cancel-paused-popup`; click trusted popup
   **Resume remote control**; poll inactive/unpaused; start explicitly; poll
   active. Return to the owned demo and capture `resume-active` as a tight crop
   of the reappeared page pill plus nonsensitive local demo content. Never retain
   the active popup status; it contains the in-memory numeric tab ID.

The retained operator record stores only these reduced booleans, reason enums,
HTTP status, error code, and taxonomy. It must never contain the underlying
response, tab ID, session ID, timestamps, URLs, refs, coordinates, or Chrome MCP
interaction transcript.

## 6. Sanitize screenshots

Google Chrome MCP captures go first to a private raw scratch directory. Inspect
each image and choose the smallest rectangle that proves only the named UI.
The finalizer requires these exact filenames and purposes. No retained crop may
contain the unpacked extension ID, including a 32-character string made only of
letters `a` through `p`:

For popup crops, include only the connection fields needed by
`popup-connected` or the **Visible browser control** section needed by a paused
state. Exclude **Current site**, the **Allowed sites** list, approval details,
all populated inputs, and any numeric tab ID. The sanitizer cannot recognize
every hostname or opaque numeric identifier; the manual crop review is the
authority for those pixels.

| File | Purpose | User-visible state proved |
|---|---|---|
| `browser-01-extensions-card.png` | `extensions-card` | Exactly one enabled candidate card and version, with ID excluded |
| `browser-02-extension-details.png` | `extension-details` | Chrome's user-visible permission and host-access surface, with ID excluded |
| `browser-03-popup-connected.png` | `popup-connected` | Candidate version connected; credential field empty |
| `browser-04-native-debugger-warning.png` | `native-debugger-warning` | Chrome-owned debugger warning on the controlled demo |
| `browser-05-page-control-pill.png` | `page-control-pill` | Real local page content plus the page-owned pill, virtual pointer, and Stop above the named page top-layer fixture; exclude IDs and tokens |
| `browser-06-action-result.png` | `action-result` | Later replay of fill/select/click produced the expected greeting |
| `browser-07-stop-after.png` | `stop-after` | Indicators disappeared after trusted in-page Stop |
| `browser-08-stop-paused-popup.png` | `stop-paused-popup` | Popup reports the in-page-Stop human pause |
| `browser-09-cancel-after.png` | `cancel-after` | Indicators disappeared after Chrome-native Cancel |
| `browser-10-cancel-paused-popup.png` | `cancel-paused-popup` | Popup reports the Chrome-Cancel human pause |
| `browser-11-resume-active.png` | `resume-active` | The page pill reappeared on nonsensitive local demo content after trusted popup Resume and explicit restart; no popup or tab ID is included |

Run the sanitizer once per image, substituting the reviewed crop coordinates:

```powershell
& "$Repository\scripts\sanitize-browser-evidence-screenshot.ps1" `
  -Mode Sanitize `
  -InputImage "REPLACE_WITH_RAW_CHROME_MCP_CAPTURE" `
  -OutputImage "$EvidenceDirectory\browser-01-extensions-card.png" `
  -OutputRecord "$EvidenceDirectory\browser-01-extensions-card.json" `
  -PreflightRecord "$EvidenceDirectory\candidate-preflight.json" `
  -Purpose "extensions-card" `
  -CropX 0 -CropY 0 -CropWidth 1 -CropHeight 1 `
  -ManualVisualReviewConfirmed
```

The numeric crop above is a placeholder, not a valid evidence rectangle. The
sanitizer and finalizer reject any retained crop smaller than 120 by 32 pixels;
that lower bound only rejects empty proof and does not replace visual review.
Never set the confirmation switch before viewing the actual output region. If OCR is
available and fails or detects a denylisted pattern, the tool retains nothing.
If OCR is unavailable, the sidecar records that fact and human review remains
mandatory. Delete every raw capture after all eleven sanitized images pass visual
review; no script claims to redact unknown pixels automatically.

## 7. Postflight, cleanup, and final record

Before changing or removing the loaded candidate folder, bind the same bytes a
second time:

```powershell
& "$Repository\scripts\browser-evidence-candidate.ps1" `
  -Mode Postflight `
  -Version $Version `
  -FinalSha $FinalSha `
  -Repository $Repository `
  -ChecksumManifest "$Candidate\SHA256SUMS.txt" `
  -ChecksumManifestSha256 $ManifestSha `
  -ServerExecutable "$Candidate\local-browser-bridge-v$Version-windows-x86_64.exe" `
  -ExtensionZip "$Candidate\local-browser-bridge-extension-v$Version.zip" `
  -ExtractedExtension $ExtensionDirectory `
  -PreflightRecord "$EvidenceDirectory\candidate-preflight.json" `
  -OutputRecord "$EvidenceDirectory\candidate-postflight.json"
```

Postflight proves that the checkout, manifest, server, ZIP, and extracted
payload are byte-identical at the two explicit preflight/postflight boundaries.
It does not claim continuous filesystem monitoring and cannot exclude a
privileged actor temporarily swapping bytes and restoring them between those
boundaries. Run on a private acceptance host, keep the frozen candidate and
verified extracted directory non-shared, and fail the release on any suspected
mid-run tampering.

Then clean up, in this order:

1. Call `browser.control.stop`; verify inactive/unpaused and no pending cleanup.
2. In the popup, disable Bridge control so it is disconnected.
3. With Google Chrome MCP, click the trusted popup's **Clear saved token**
   button. The `clearSavedToken` action disconnects first, removes only the
   saved token, and refuses success unless the returned popup state has
   `tokenConfigured:false`. Re-read the live popup UI and verify the button is
   disabled. Do not inspect `chrome.storage` or retain the popup response.
4. Restore **Developer mode** to its initial setting if this run changed it.
5. Call `tabs.close` only with `[long]$OwnedTarget.tabId`, verify the exact ID is
   absent from the next `tabs.list`, clear `$OwnedTarget`, then close the
   dedicated temporary Chrome window through Google Chrome MCP only after
   visually confirming it contains no unrelated tab. Never reacquire by URL or
   title.
6. Either keep the verified extension as the single enabled identity, or remove
   it only if the user confirmed it was a test-owned copy. Record exactly one
   allowed disposition. Do not alter unrelated extensions.
7. Stop only `$ServerProcess`, wait for it, and verify port 17373 is released.
8. Set `$env:LBB_TOKEN = $null`, clear the byte array, `Clear-History`, delete
   raw screenshots and all non-retained scratch, and release the Google Chrome
   MCP session. Do not terminate any unrelated process.

Edit only the candidate-bound `operator-results.json` created by
`InitializeOperator`; never copy the raw template over it or alter its
`candidateBinding`. Flip an assertion only after its observation passed; enter
only the four-part Chrome version, and set `idPatternValid` without retaining
the actual extension ID. Set `loadedDirectoryByteMatchesCandidateZip:true` only after selecting the
exact preflight-bound `$ExtensionDirectory`; the preflight and postflight
inventories are the machine proof, while this reduced boolean binds the manual
Chrome picker decision without retaining its path. Set `permissionsUiReviewed`
and `hostAccessUiReviewed` only after reviewing the Details surface;
the binder supplies the exact raw arrays. Set
`developerModeRestored:true` only after the toggle matches its in-memory
initial state, without recording what that state was. For `savedTokenClear`,
set `trustedPopupClick`, `popupStateVerifiedAfterClear`, and
`clearButtonDisabled` true only after the MCP-driven trusted click and live UI
check; retain only the `tokenConfigured` field with the returned reduced value
`false`, never the token or raw extension storage. The exact schema deliberately
has no place for raw
responses, credentials, paths, terminal output, Chrome MCP commands, or
profile state. Choose
`kept-single-enabled-identity` or
`removed-test-owned-copy` for `extensionDisposition`.

Finally run:

```powershell
$Sidecars = Get-ChildItem -LiteralPath $EvidenceDirectory -Filter "browser-??-*.json" |
  Sort-Object Name |
  ForEach-Object { $_.FullName }

& "$Repository\scripts\write-browser-evidence-record.ps1" `
  -Mode Finalize `
  -PreflightRecord "$EvidenceDirectory\candidate-preflight.json" `
  -PostflightRecord "$EvidenceDirectory\candidate-postflight.json" `
  -ApiMatrixRecord "$EvidenceDirectory\browser-api-matrix.json" `
  -OperatorResults "$EvidenceDirectory\operator-results.json" `
  -ScreenshotRecords $Sidecars `
  -OutputRecord "$EvidenceDirectory\browser-acceptance.json"
```

The finalizer imports and hashes the passing machine record for all 25 methods.
It requires exact equality of the seven-field `candidateBinding` across the
preflight, postflight domain, API matrix, operator record, all eleven screenshot
sidecars, and final record; a same-version artifact from another run or commit
cannot be mixed in.
It also requires exactly one candidate extension card, binder-verified exact
manifest permissions, Chrome's reviewed Details surface, real stock
Chrome/manual-load declarations, both authoritative
`browser.control.status` polls, both HTTP 423 refusal pairs, trusted popup
Resume and active-status polls, all eleven reviewed image hashes, candidate
preflight-to-postflight hash equality, restored Developer mode, and complete
cleanup. Cleanup includes the trusted popup token-clear action and its exact
reduced `tokenConfigured:false` postcondition. A missing, extra, duplicated,
stale, unsafe, or placeholder field fails closed.
