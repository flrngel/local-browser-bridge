#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("InitializeOperator", "BuildOperator", "Finalize", "SelfTest")]
    [string]$Mode,
    [string]$PreflightRecord,
    [string]$PostflightRecord,
    [string]$ApiMatrixRecord,
    [string]$ComputerHelperRecord,
    [string]$ScopedApprovalRecord,
    [string]$IndependentReviewRecord,
    [string]$ExternalSurfacePreflightAttestation,
    [string]$ExternalSurfacePostflightAttestation,
    [string]$OperatorResults,
    [string]$BrowserVersion,
    [string[]]$ScreenshotRecords,
    [string]$DenyValuesFile,
    [string]$OutputRecord
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$script:Utf8NoBom = [Text.UTF8Encoding]::new($false, $true)

function ConvertFrom-JsonPreservingStrings {
    param([Parameter(Mandatory = $true, ValueFromPipeline = $true)][string]$Json)
    process {
        $command = Get-Command ConvertFrom-Json -CommandType Cmdlet -ErrorAction Stop
        if ($command.Parameters.ContainsKey("DateKind")) {
            Microsoft.PowerShell.Utility\ConvertFrom-Json -InputObject $Json -DateKind String
        }
        else {
            Microsoft.PowerShell.Utility\ConvertFrom-Json -InputObject $Json
        }
    }
}

$script:ExtensionFiles = @(
    "background.js", "content.js", "dom-core.js", "frame-agent.js", "lib.js",
    "manifest.json", "popup.css", "popup.html", "popup.js", "stop-guard.js", "LICENSE"
)
$script:Methods = @(
    "status", "browser.control.start", "browser.control.status", "browser.control.stop",
    "tabs.list", "tabs.activate", "tabs.new", "tabs.close", "page.observe",
    "page.navigate", "page.back", "page.forward", "page.reload", "page.click",
    "page.fill", "page.select", "page.key", "page.scroll", "page.clickAt",
    "page.typeText", "page.evaluate", "page.waitFor", "page.hover", "page.batch",
    "page.handleDialog"
)
$script:MethodStages = [ordered]@{
    "status" = "preflight"
    "browser.control.start" = "control"
    "browser.control.status" = "control"
    "browser.control.stop" = "cleanup"
    "tabs.list" = "tab-lifecycle"
    "tabs.activate" = "tab-lifecycle"
    "tabs.new" = "tab-lifecycle"
    "tabs.close" = "cleanup"
    "page.observe" = "freshness"
    "page.navigate" = "navigation"
    "page.back" = "navigation"
    "page.forward" = "navigation"
    "page.reload" = "navigation"
    "page.click" = "interaction"
    "page.fill" = "interaction"
    "page.select" = "interaction"
    "page.key" = "interaction"
    "page.scroll" = "interaction"
    "page.clickAt" = "interaction"
    "page.typeText" = "interaction"
    "page.evaluate" = "inspection"
    "page.waitFor" = "inspection"
    "page.hover" = "interaction"
    "page.batch" = "interaction"
    "page.handleDialog" = "dialog"
}
$script:MethodScreenshots = [ordered]@{
    "status" = "browser-03-popup-connected.png"
    "browser.control.start" = "browser-04-native-debugger-warning.png"
    "browser.control.status" = "N/A"
    "browser.control.stop" = "N/A"
    "tabs.list" = "N/A"
    "tabs.activate" = "N/A"
    "tabs.new" = "N/A"
    "tabs.close" = "N/A"
    "page.observe" = "N/A"
    "page.navigate" = "N/A"
    "page.back" = "N/A"
    "page.forward" = "N/A"
    "page.reload" = "N/A"
    "page.click" = "browser-06-action-result.png"
    "page.fill" = "browser-06-action-result.png"
    "page.select" = "browser-06-action-result.png"
    "page.key" = "N/A"
    "page.scroll" = "N/A"
    "page.clickAt" = "N/A"
    "page.typeText" = "N/A"
    "page.evaluate" = "browser-05-page-control-pill.png"
    "page.waitFor" = "N/A"
    "page.hover" = "N/A"
    "page.batch" = "N/A"
    "page.handleDialog" = "N/A"
}
$script:MethodScreenshotsV2 = [ordered]@{
    "status" = "N/A"
    "browser.control.start" = "browser-01-extension-loaded.png"
    "browser.control.status" = "N/A"
    "browser.control.stop" = "N/A"
    "tabs.list" = "N/A"
    "tabs.activate" = "N/A"
    "tabs.new" = "N/A"
    "tabs.close" = "N/A"
    "page.observe" = "N/A"
    "page.navigate" = "N/A"
    "page.back" = "N/A"
    "page.forward" = "N/A"
    "page.reload" = "N/A"
    "page.click" = "browser-02-api-action-result.png"
    "page.fill" = "browser-02-api-action-result.png"
    "page.select" = "browser-02-api-action-result.png"
    "page.key" = "N/A"
    "page.scroll" = "N/A"
    "page.clickAt" = "N/A"
    "page.typeText" = "N/A"
    "page.evaluate" = "browser-02-api-action-result.png"
    "page.waitFor" = "N/A"
    "page.hover" = "N/A"
    "page.batch" = "N/A"
    "page.handleDialog" = "N/A"
}
$script:Permissions = @("tabs", "scripting", "storage", "alarms", "debugger", "tabGroups")
$script:HostPermissions = @("http://*/*", "https://*/*", "file://*/*")
$script:ExpectedScreenshots = [ordered]@{
    "extensions-card" = "browser-01-extensions-card.png"
    "extension-details" = "browser-02-extension-details.png"
    "popup-connected" = "browser-03-popup-connected.png"
    "native-debugger-warning" = "browser-04-native-debugger-warning.png"
    "page-control-pill" = "browser-05-page-control-pill.png"
    "action-result" = "browser-06-action-result.png"
    "stop-after" = "browser-07-stop-after.png"
    "stop-paused-popup" = "browser-08-stop-paused-popup.png"
    "cancel-after" = "browser-09-cancel-after.png"
    "cancel-paused-popup" = "browser-10-cancel-paused-popup.png"
    "resume-active" = "browser-11-resume-active.png"
}
$script:ExpectedScreenshotsV2 = [ordered]@{
    "extension-loaded" = "browser-01-extension-loaded.png"
    "api-action-result" = "browser-02-api-action-result.png"
    "computer-share-action" = "browser-03-computer-share-action.png"
    "stop-paused" = "browser-04-stop-paused.png"
    "cancel-paused" = "browser-05-cancel-paused.png"
    "post-handback-resume" = "browser-06-post-handback-resume.png"
}
$script:ReviewStatement = "A human reviewed this tight crop; OCR is supplemental and unknown sensitive pixels are not automatically redacted."
$script:ReviewStatementV2 = "A separate agent reviewed this exact digest-bound crop; no sensitive pixels were observed, but visual judgment is not a pixel-safety proof."
$script:OperatorV2Version = "0.12.52"
$script:ReleaseCandidateBindingFields = @(
    "schemaVersion", "version", "releaseTag", "repository", "sourceSha",
    "workflowRunId", "workflowRunAttempt", "workflowEvent", "workflowRef", "workflowPath",
    "artifactId", "artifactName",
    "artifactZipBytes", "artifactZipSha256", "checksumManifestSha256",
    "attestationInvocationUri", "attestedAssetCount", "githubHostedRunner", "assets"
)
$script:BrowserChromeSurfaces = @(
    "local-browser-bridge-computer-helper"
)
$script:ExternalOrchestrationSurfaces = @(
    "user-orchestrator-secured-ssh-exported-file-review"
)
$script:ComputerHelperActionSource = "local-browser-bridge-computer-helper-via-loopback-api"
$script:ComputerHelperLifecycleEvents = @(
    "server-ready", "computer-disconnected-before-helper-start", "helper-started",
    "computer-connected", "helper-owner-forced-termination-requested",
    "helper-owner-forced-terminated", "computer-disconnected-after-helper-termination",
    "server-owner-forced-terminated"
)
$script:ComputerHelperEpochNames = @(
    "existing-chrome-bootstrap", "dedicated-chrome-extensions", "native-load-picker",
    "dedicated-chrome-installed", "extension-popup-setup", "dedicated-chrome-demo",
    "stop-paused-popup", "stop-recovered-demo", "cancel-paused-popup", "cancel-recovered-demo",
    "extension-popup-cleanup", "cleanup-chrome-extensions", "dedicated-chrome-close"
)
$script:ComputerHelperEpochSurfaces = @(
    "chrome-window", "chrome-window", "native-file-picker", "chrome-window",
    "extension-popup", "chrome-window", "extension-popup", "chrome-window", "extension-popup",
    "chrome-window", "extension-popup", "chrome-window", "chrome-window"
)
$script:ComputerHelperActionNames = @(
    "dedicated-window-created", "chrome-extensions-navigated", "developer-mode-ready",
    "load-unpacked-clicked", "native-picker-completed", "candidate-card-verified",
    "extension-popup-opened", "full-access-ready", "popup-token-saved",
    "extension-proof-revealed", "browser-api-result-revealed", "computer-demo-clicked",
    "in-page-stop-clicked", "stop-popup-opened", "stop-resume-clicked", "stop-recovery-verified",
    "chrome-native-cancel-clicked", "cancel-popup-opened", "cancel-resume-clicked", "cancel-recovery-verified",
    "cleanup-popup-opened",
    "token-clear-initiated", "token-clear-confirmed", "full-access-restored",
    "test-card-removed", "developer-mode-restored", "test-window-closed"
)
$script:ComputerHelperActionEpochIndexes = @(
    0, 1, 1, 1, 2, 3, 3, 4, 4, 5, 5, 5, 5, 5, 6, 7, 7, 7, 8, 9, 9, 10, 10, 10, 11, 11, 12
)
$script:ComputerHelperActionMutators = @(
    "computer.key", "computer.typeText", "conditional-developer-mode", "computer.click",
    "computer.typeText", "none", "computer.click", "conditional-full-access", "computer.typeText",
    "computer.click", "computer.click", "computer.click", "computer.click", "computer.click",
    "computer.click", "none", "computer.click", "computer.click", "computer.click", "none",
    "computer.click", "computer.click", "computer.click", "conditional-full-access", "computer.click",
    "conditional-developer-mode", "computer.key"
)
$script:ComputerHelperActionRiskRefs = @(
    "none", "none", "conditional-developer-mode", "installCandidate", "installCandidate", "none",
    "none", "fullAccessUse", "acceptanceTokenSave", "none", "none", "none", "none", "none",
    "none", "none", "none", "none", "none", "none", "none",
    "clearSavedTokenInitiate", "clearSavedTokenConfirm", "fullAccessUse", "extensionDisposition",
    "conditional-developer-mode", "none"
)
$script:CoveredApprovalActions = @(
    "conditional-developer-mode-change",
    "load-and-run-exact-unpacked-candidate",
    "conditional-full-access-change",
    "save-ephemeral-loopback-credential",
    "clear-ephemeral-loopback-credential",
    "remove-exact-test-owned-extension",
    "restore-captured-browser-settings",
    "failure-rollback"
)
$script:ComputerHelperBrowserMethods = @(
    "browser.control.start", "page.observe", "page.fill", "page.observe",
    "page.select", "page.observe", "page.click", "page.observe", "page.evaluate"
)
$script:ComputerHelperScreenshotEpochIndexes = @(5, 5, 5, 6, 8, 9)
$script:RequiredVisibleStatesV2 = [ordered]@{
    "extension-loaded" = "stock Chrome chrome://extensions shows exactly one enabled unpacked Local Browser Bridge v0.12.52 card with no load errors and Chrome's debugger-use indicator during the active bridge lease"
    "api-action-result" = "the loopback demo visibly shows Hello, Bridge Matrix. blue selected. after the browser API action"
    "computer-share-action" = "the exact shared Chrome window visibly shows the post-click demo state and synthetic session pointer from a fresh helper frame"
    "stop-paused" = "the trusted extension popup visibly shows the human pause and Resume remote control after the in-page Stop handback"
    "cancel-paused" = "the trusted extension popup visibly shows the human pause and Resume remote control after Chrome's browser-owned Cancel handback"
    "post-handback-resume" = "the exact demo visibly shows the restored Chrome debugger-use indicator and page control pill after both trusted-popup recovery cycles"
}
$script:ScreenshotCaptureSurfacesV2 = [ordered]@{
    "extension-loaded" = "machine-helper-visual"
    "api-action-result" = "machine-helper-visual"
    "computer-share-action" = "machine-helper-visual"
    "stop-paused" = "machine-helper-visual"
    "cancel-paused" = "machine-helper-visual"
    "post-handback-resume" = "machine-helper-visual"
}

function Assert-RequiredArgument {
    param([object]$Value, [string]$Name)
    if ($null -eq $Value -or ($Value -is [string] -and [String]::IsNullOrWhiteSpace($Value))) {
        throw "$Name is required for $Mode mode."
    }
}

function Resolve-RequiredFile {
    param([string]$Path, [string]$Label)
    $resolved = [IO.Path]::GetFullPath($Path)
    if (-not [IO.File]::Exists($resolved)) {
        throw "$Label does not exist."
    }
    $item = [IO.FileInfo]::new($resolved)
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "$Label must not be a reparse point."
    }
    Assert-NoReparseAncestorChain $resolved $Label
    return $resolved
}

function Assert-NoReparseAncestorChain {
    param([string]$Path, [string]$Label)
    $full = [IO.Path]::GetFullPath($Path)
    $directory = if ([IO.Directory]::Exists($full)) {
        [IO.DirectoryInfo]::new($full)
    }
    else { [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName($full)) }
    while ($null -ne $directory) {
        if (($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "$Label must not traverse a reparse-point directory."
        }
        $directory = $directory.Parent
    }
}

function Resolve-NewOutputFile {
    param([string]$Path)
    $resolved = [IO.Path]::GetFullPath($Path)
    if ([IO.Path]::GetExtension($resolved) -cne ".json") {
        throw "OutputRecord must use the .json extension."
    }
    if ([IO.File]::Exists($resolved) -or [IO.Directory]::Exists($resolved)) {
        throw "OutputRecord already exists; evidence is never overwritten."
    }
    $parent = [IO.Path]::GetDirectoryName($resolved)
    if ([String]::IsNullOrWhiteSpace($parent) -or -not [IO.Directory]::Exists($parent)) {
        throw "OutputRecord parent directory must already exist."
    }
    $parentInfo = [IO.DirectoryInfo]::new($parent)
    if (($parentInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "OutputRecord parent directory must not be a reparse point."
    }
    Assert-NoReparseAncestorChain $parent "OutputRecord parent"
    return $resolved
}

function Read-Json {
    param([string]$Path, [string]$Label)
    return (Read-JsonWithSha256 $Path $Label).Value
}

function Read-JsonWithSha256 {
    param([string]$Path, [string]$Label)
    $resolved = [IO.Path]::GetFullPath($Path)
    if (-not [IO.File]::Exists($resolved)) { throw "$Label does not exist." }
    $before = [IO.FileInfo]::new($resolved)
    if (($before.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "$Label must be an ordinary file, not a reparse point."
    }
    Assert-NoReparseAncestorChain $resolved $Label
    if ($before.Length -le 0 -or $before.Length -gt 2MB) {
        throw "$Label has an invalid size."
    }
    $stream = [IO.File]::Open(
        $resolved, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None
    )
    $bytes = $null
    $offset = 0
    try {
        if ($stream.Length -ne $before.Length -or $stream.Length -le 0 -or
            $stream.Length -gt 2MB -or $stream.Length -gt [int]::MaxValue) {
            throw "$Label changed size before its exclusive read."
        }
        $bytes = [byte[]]::new([int]$stream.Length)
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -le 0) { throw "$Label ended before its declared length." }
            $offset += $read
        }
        if ($stream.ReadByte() -ne -1) { throw "$Label grew during its exclusive read." }
    }
    finally { $stream.Dispose() }
    $hasher = $null
    $digestBytes = $null
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        $digestBytes = $hasher.ComputeHash($bytes)
        $sha256 = ([BitConverter]::ToString($digestBytes)).Replace("-", "").ToLowerInvariant()
        try { $value = ConvertFrom-JsonPreservingStrings ($script:Utf8NoBom.GetString($bytes)) }
        catch { throw "$Label is not strict UTF-8 JSON." }
        return [pscustomobject]@{
            Value = $value
            Sha256 = $sha256
            Bytes = [int64]$bytes.Length
        }
    }
    finally {
        if ($null -ne $digestBytes) { [Array]::Clear($digestBytes, 0, $digestBytes.Length) }
        if ($null -ne $hasher) { $hasher.Dispose() }
        [Array]::Clear($bytes, 0, $bytes.Length)
    }
}

function Get-Sha256 {
    param([string]$Path)
    $stream = [IO.File]::Open(
        $Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read
    )
    $hasher = $null
    $digest = $null
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        $digest = $hasher.ComputeHash($stream)
        return ([BitConverter]::ToString($digest)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        if ($null -ne $digest) { [Array]::Clear($digest, 0, $digest.Length) }
        if ($null -ne $hasher) { $hasher.Dispose() }
        $stream.Dispose()
    }
}

function Get-TextSha256 {
    param([string]$Value)
    $bytes = $script:Utf8NoBom.GetBytes($Value)
    $hasher = $null
    $digest = $null
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        $digest = $hasher.ComputeHash($bytes)
        return ([BitConverter]::ToString($digest)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        if ($null -ne $digest) { [Array]::Clear($digest, 0, $digest.Length) }
        if ($null -ne $hasher) { $hasher.Dispose() }
        [Array]::Clear($bytes, 0, $bytes.Length)
    }
}

function Assert-ExactKeys {
    param([object]$Object, [string[]]$Expected, [string]$Label)
    if ($null -eq $Object) {
        throw "$Label must be an object."
    }
    $actualKeys = @()
    if ($Object -is [Collections.IDictionary]) {
        foreach ($key in $Object.Keys) {
            if ($key -isnot [string]) {
                throw "$Label contains a non-string field name."
            }
            $actualKeys += $key
        }
    }
    else {
        $actualKeys = @($Object.PSObject.Properties.Name)
    }
    $actual = @($actualKeys | Sort-Object)
    $wanted = @($Expected | Sort-Object)
    if (($actual -join "`n") -cne ($wanted -join "`n")) {
        throw "$Label contains missing or unexpected fields."
    }
}

function Assert-ExactPropertyOrder {
    param([object]$Object, [string[]]$Expected, [string]$Label)
    if ($null -eq $Object) {
        throw "$Label must be an object."
    }
    $actual = @()
    if ($Object -is [Collections.IDictionary]) {
        foreach ($key in $Object.Keys) {
            if ($key -isnot [string]) {
                throw "$Label contains a non-string field name."
            }
            $actual += $key
        }
    }
    else {
        $actual = @($Object.PSObject.Properties.Name)
    }
    if (($actual -join "`n") -cne ($Expected -join "`n")) {
        throw "$Label fields are not in canonical order."
    }
}

function Assert-ExactArray {
    param([object[]]$Actual, [string[]]$Expected, [string]$Label)
    if (($Actual -join "`n") -cne ($Expected -join "`n")) {
        throw "$Label does not match the canonical ordered values."
    }
}

function Assert-ExactBoolean {
    param([object]$Actual, [bool]$Expected, [string]$Label)
    if ($Actual -isnot [bool] -or $Actual -ne $Expected) {
        throw "$Label must be $Expected."
    }
}

function Assert-UtcTimestamp {
    param([object]$Actual, [string]$Label)
    $parsed = [DateTimeOffset]::MinValue
    if ($Actual -isnot [string] -or
        [string]$Actual -cnotmatch '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{7}Z$' -or
        -not [DateTimeOffset]::TryParseExact(
            $Actual, "o", [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind, [ref]$parsed
        )) {
        throw "$Label must be a canonical Z-suffixed UTC round-trip timestamp."
    }
    if ($parsed.Offset -ne [TimeSpan]::Zero) {
        throw "$Label must be a canonical Z-suffixed UTC round-trip timestamp."
    }
    return $parsed
}

function Assert-ExternalSurfaceAttestationV3 {
    param(
        [object]$Record,
        [string]$RecordSha256,
        [object]$ReleaseBinding,
        [string]$ExpectedPhase
    )
    Assert-ExactKeys $Record @(
        "schemaVersion", "evidenceType", "phase", "releaseCandidateBinding",
        "releaseCandidateBindingSha256", "orchestrationSurface", "chromeMcpState",
        "computerUseState", "reviewerInputState", "attestorKind",
        "attestorSessionRef", "attestedAtUtc"
    ) "external-surface attestation"
    if ($ExpectedPhase -ceq "preflight") {
        $ExpectedChromeMcp = "not-used-before-candidate-execution"
        $ExpectedComputerUse = "released-before-candidate-execution"
        $ExpectedReviewerInput = "review-not-started"
    }
    elseif ($ExpectedPhase -ceq "postflight") {
        $ExpectedChromeMcp = "never-used-through-independent-review"
        $ExpectedComputerUse = "not-resumed-through-independent-review"
        $ExpectedReviewerInput = "exported-digest-bound-files-only"
    }
    else { throw "External-surface attestation phase is unsupported." }
    if ($Record.schemaVersion -ne 1 -or
        $Record.evidenceType -cne "stock-user-chrome-external-surface-attestation" -or
        $Record.phase -cne $ExpectedPhase -or
        $Record.orchestrationSurface -cne `
            "user-orchestrator-secured-ssh-exported-file-review" -or
        $Record.chromeMcpState -cne $ExpectedChromeMcp -or
        $Record.computerUseState -cne $ExpectedComputerUse -or
        $Record.reviewerInputState -cne $ExpectedReviewerInput -or
        $Record.attestorKind -cne "orchestrator-agent" -or
        [string]$Record.releaseCandidateBindingSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Record.attestorSessionRef -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$RecordSha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw "External-surface attestation identity, phase, or exact states are invalid."
    }
    $ExpectedReleaseDigest = Get-TextSha256 (
        $ReleaseBinding | ConvertTo-Json -Depth 20 -Compress
    )
    if ($Record.releaseCandidateBindingSha256 -cne $ExpectedReleaseDigest -or
        ($Record.releaseCandidateBinding | ConvertTo-Json -Depth 20 -Compress) -cne
            ($ReleaseBinding | ConvertTo-Json -Depth 20 -Compress)) {
        throw "External-surface attestation is not bound to the exact release candidate."
    }
    $AttestedAt = Assert-UtcTimestamp $Record.attestedAtUtc "external-surface attestation time"
    return [pscustomobject]@{
        Phase = $ExpectedPhase
        Sha256 = $RecordSha256
        ChromeMcpState = [string]$Record.chromeMcpState
        ComputerUseState = [string]$Record.computerUseState
        ReviewerInputState = [string]$Record.reviewerInputState
        AttestorSessionRef = [string]$Record.attestorSessionRef
        AttestedAt = $AttestedAt
        AttestedAtUtc = [string]$Record.attestedAtUtc
    }
}

function Assert-ExternalSurfaceAttestationPairV3 {
    param(
        [object]$Preflight,
        [object]$Postflight,
        [object]$Helper,
        [object]$Approval,
        [object]$Review
    )
    if ($Preflight.AttestorSessionRef -cne $Postflight.AttestorSessionRef -or
        $Preflight.AttestorSessionRef -ceq $Helper.operatorExchange.executorSessionRef -or
        $Preflight.AttestorSessionRef -ceq $Helper.operatorExchange.reviewerSessionRef -or
        $Preflight.AttestorSessionRef -cne $Approval.response.orchestratorSessionRef) {
        throw "External-surface attestations do not use the same distinct approval orchestrator session."
    }
    $StartedAt = Assert-UtcTimestamp $Helper.run.startedAtUtc "helper run start time"
    $FinishedAt = Assert-UtcTimestamp $Helper.run.finishedAtUtc "helper run finish time"
    $ReviewedAt = Assert-UtcTimestamp $Review.reviewedAtUtc "independent review time"
    $Now = [DateTimeOffset]::UtcNow
    if ($Preflight.AttestedAt -gt $StartedAt -or
        $Preflight.AttestedAt -lt $StartedAt.AddMinutes(-30)) {
        throw "External-surface preflight was not fresh and ordered before candidate execution."
    }
    if ($Postflight.AttestedAt -lt $FinishedAt -or
        $Postflight.AttestedAt -lt $ReviewedAt -or
        $Postflight.AttestedAt -le $Preflight.AttestedAt -or
        $Postflight.AttestedAt -gt $Now -or
        $Postflight.AttestedAt -gt $ReviewedAt.AddMinutes(30)) {
        throw "External-surface postflight does not close the helper and independent-review interval."
    }
    return [pscustomobject]@{
        PreflightSha256 = [string]$Preflight.Sha256
        PostflightSha256 = [string]$Postflight.Sha256
        ChromeMcpDisposition = [string]$Postflight.ChromeMcpState
        ComputerUseDisposition = [string]$Postflight.ComputerUseState
        ReviewerInputDisposition = [string]$Postflight.ReviewerInputState
        AttestorSessionRef = [string]$Preflight.AttestorSessionRef
        PreflightAttestedAtUtc = [string]$Preflight.AttestedAtUtc
        PostflightAttestedAtUtc = [string]$Postflight.AttestedAtUtc
    }
}

function Format-CanonicalUtc {
    param([DateTimeOffset]$Value)
    return $Value.UtcDateTime.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
}

function Assert-IntegerRange {
    param([object]$Actual, [int64]$Minimum, [int64]$Maximum, [string]$Label)
    $integerTypes = @(
        [TypeCode]::SByte, [TypeCode]::Byte, [TypeCode]::Int16, [TypeCode]::UInt16,
        [TypeCode]::Int32, [TypeCode]::UInt32, [TypeCode]::Int64, [TypeCode]::UInt64
    )
    if ($null -eq $Actual -or $Actual -is [bool] -or $Actual -isnot [ValueType] -or
        $integerTypes -notcontains [Type]::GetTypeCode($Actual.GetType()) -or
        [decimal]$Actual -lt $Minimum -or [decimal]$Actual -gt $Maximum) {
        throw "$Label must be an integer from $Minimum through $Maximum."
    }
}

function Assert-Hex {
    param([object]$Value, [int]$Length, [string]$Label)
    if ($Value -isnot [string] -or -not [regex]::IsMatch($Value, "^[0-9a-f]{$Length}$", [Text.RegularExpressions.RegexOptions]::CultureInvariant)) {
        throw "$Label is not canonical lowercase hexadecimal."
    }
}

function Assert-SafeInventory {
    param([object[]]$Inventory, [string]$Label)
    if ($Inventory.Count -ne $script:ExtensionFiles.Count) {
        throw "$Label must contain the exact extension inventory."
    }
    for ($index = 0; $index -lt $script:ExtensionFiles.Count; $index += 1) {
        $item = $Inventory[$index]
        Assert-ExactKeys $item @("name", "bytes", "sha256") "$Label item"
        if ($item.name -cne $script:ExtensionFiles[$index]) {
            throw "$Label item identity is invalid."
        }
        Assert-IntegerRange $item.bytes 1 2MB "$Label item bytes"
        Assert-Hex $item.sha256 64 "$Label item SHA-256"
    }
}

function Assert-CandidateBinding {
    param([object]$Candidate)
    $candidate = $Candidate
    if ($candidate.version -isnot [string] -or -not [regex]::IsMatch($candidate.version, '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')) {
        throw "Candidate version must be stable SemVer."
    }
    $candidateKeys = @("version", "finalSha", "gitClean", "checksumManifest", "server", "extension")
    if ($candidate.version -ceq $script:OperatorV2Version) {
        $candidateKeys = @("version", "finalSha", "gitClean", "checksumManifest", "server", "computerHelper", "extension")
    }
    Assert-ExactKeys $candidate $candidateKeys "candidate binding"
    Assert-Hex $candidate.finalSha 40 "FINAL_SHA"
    Assert-ExactBoolean $candidate.gitClean $true "candidate gitClean"
    $manifest = $candidate.checksumManifest
    Assert-ExactKeys $manifest @(
        "name", "bytes", "sha256", "externallySuppliedSha256", "canonicalEntryCount", "canonicalNamesInOrder"
    ) "candidate checksum manifest"
    if ($manifest.name -cne "SHA256SUMS.txt") {
        throw "Candidate checksum manifest identity is invalid."
    }
    Assert-IntegerRange $manifest.bytes 1 16384 "checksum manifest bytes"
    Assert-IntegerRange $manifest.canonicalEntryCount 4 4 "checksum manifest entry count"
    Assert-Hex $manifest.sha256 64 "checksum manifest SHA-256"
    Assert-Hex $manifest.externallySuppliedSha256 64 "external checksum manifest SHA-256"
    if ($manifest.sha256 -cne $manifest.externallySuppliedSha256) {
        throw "Checksum manifest was not externally bound."
    }
    $expectedNames = @(
        "local-browser-bridge-v$($candidate.version)-windows-x86_64.exe",
        "local-computer-helper-v$($candidate.version)-windows-x86_64.exe",
        "local-browser-bridge-v$($candidate.version)-macos-universal.tar.gz",
        "local-browser-bridge-extension-v$($candidate.version).zip"
    )
    Assert-ExactArray @($manifest.canonicalNamesInOrder) $expectedNames "checksum manifest names"

    Assert-ExactKeys $candidate.server @("name", "bytes", "sha256") "candidate server binding"
    if ($candidate.server.name -cne $expectedNames[0]) {
        throw "Candidate server identity is invalid."
    }
    Assert-IntegerRange $candidate.server.bytes 1 100MB "candidate server bytes"
    Assert-Hex $candidate.server.sha256 64 "candidate server SHA-256"
    if ($candidate.version -ceq $script:OperatorV2Version) {
        Assert-ExactKeys $candidate.computerHelper @("name", "bytes", "sha256") "candidate computer helper binding"
        if ($candidate.computerHelper.name -cne $expectedNames[1]) {
            throw "Candidate computer helper identity is invalid."
        }
        Assert-IntegerRange $candidate.computerHelper.bytes 1 100MB "candidate computer helper bytes"
        Assert-Hex $candidate.computerHelper.sha256 64 "candidate computer helper SHA-256"
    }

    $extension = $candidate.extension
    Assert-ExactKeys $extension @(
        "name", "bytes", "sha256", "manifestVersion", "libraryVersion", "minimumChromeVersion", "permissions", "hostPermissions", "archiveInventory",
        "extractedPayloadInventory", "checkoutPayloadInventory", "combinedPayloadSha256"
    ) "candidate extension binding"
    if ($extension.name -cne $expectedNames[3] -or $extension.manifestVersion -cne $candidate.version -or $extension.libraryVersion -cne $candidate.version -or $extension.minimumChromeVersion -cne "140") {
        throw "Candidate extension identity is invalid."
    }
    Assert-IntegerRange $extension.bytes 1 8MB "extension ZIP bytes"
    Assert-Hex $extension.sha256 64 "extension ZIP SHA-256"
    Assert-Hex $extension.combinedPayloadSha256 64 "combined extension payload SHA-256"
    Assert-ExactArray @($extension.permissions) $script:Permissions "candidate manifest permissions"
    Assert-ExactArray @($extension.hostPermissions) $script:HostPermissions "candidate manifest host permissions"
    Assert-SafeInventory @($extension.archiveInventory) "archive inventory"
    Assert-SafeInventory @($extension.extractedPayloadInventory) "extracted inventory"
    Assert-SafeInventory @($extension.checkoutPayloadInventory) "checkout inventory"
    $archive = $extension.archiveInventory | ConvertTo-Json -Depth 5 -Compress
    $extracted = $extension.extractedPayloadInventory | ConvertTo-Json -Depth 5 -Compress
    $checkout = $extension.checkoutPayloadInventory | ConvertTo-Json -Depth 5 -Compress
    if ($archive -cne $extracted -or $archive -cne $checkout) {
        throw "Candidate extension inventories are not byte-identical."
    }
    return $candidate
}

function Assert-ReleaseCandidateBinding {
    param([object]$Binding, [object]$Candidate, [string]$Label)
    Assert-ExactKeys $Binding $script:ReleaseCandidateBindingFields $Label
    Assert-ExactPropertyOrder $Binding $script:ReleaseCandidateBindingFields $Label
    if ($Binding.schemaVersion -ne 3 -or
        $Binding.version -cne $Candidate.version -or
        $Binding.releaseTag -cne "v$($Candidate.version)" -or
        $Binding.repository -cne "flrngel/local-browser-bridge" -or
        $Binding.sourceSha -cne $Candidate.finalSha -or
        [string]$Binding.workflowRunId -cnotmatch '^[1-9][0-9]*$' -or
        [string]$Binding.workflowRunAttempt -cnotmatch '^[1-9][0-9]*$' -or
        $Binding.workflowEvent -cne "workflow_dispatch" -or
        $Binding.workflowRef -cne "refs/heads/main" -or
        $Binding.workflowPath -cne ".github/workflows/deploy.yml" -or
        [string]$Binding.artifactId -cnotmatch '^[1-9][0-9]*$' -or
        $Binding.artifactName -cne "release-candidate" -or
        $Binding.artifactZipBytes -isnot [ValueType] -or [int64]$Binding.artifactZipBytes -le 0 -or
        [string]$Binding.artifactZipSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $Binding.checksumManifestSha256 -cne $Candidate.checksumManifest.sha256 -or
        $Binding.attestationInvocationUri -cne ("https://github.com/flrngel/local-browser-bridge/actions/runs/{0}/attempts/{1}" -f
            [string]$Binding.workflowRunId, [string]$Binding.workflowRunAttempt) -or
        $Binding.attestedAssetCount -ne 5 -or $Binding.githubHostedRunner -ne $true) {
        throw "$Label does not bind the exact release workflow attempt and candidate."
    }
    $expectedNames = @($Candidate.checksumManifest.canonicalNamesInOrder) + "SHA256SUMS.txt"
    $assets = @($Binding.assets)
    if ($assets.Count -ne 5) { throw "$Label must bind exactly five assets." }
    for ($index = 0; $index -lt 5; $index += 1) {
        $asset = $assets[$index]
        Assert-ExactKeys $asset @("file", "bytes", "sha256") "$Label asset"
        Assert-ExactPropertyOrder $asset @("file", "bytes", "sha256") "$Label asset"
        if ($asset.file -cne $expectedNames[$index]) { throw "$Label asset order is invalid." }
        Assert-IntegerRange $asset.bytes 1 100MB "$Label asset bytes"
        Assert-Hex $asset.sha256 64 "$Label asset SHA-256"
    }
    if ($assets[0].sha256 -cne $Candidate.server.sha256 -or
        $assets[1].sha256 -cne $Candidate.computerHelper.sha256 -or
        $assets[3].sha256 -cne $Candidate.extension.sha256 -or
        $assets[4].sha256 -cne $Candidate.checksumManifest.sha256) {
        throw "$Label asset digests do not match the exact Windows candidate payload."
    }
    return $Binding
}

function Assert-CandidatePreflight {
    param([object]$Record)
    $fields = @("schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed", "runNonce", "candidate")
    if ($Record.candidate.version -ceq $script:OperatorV2Version) {
        $fields = @(
            "schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed",
            "runNonce", "releaseCandidateBinding", "candidate"
        )
    }
    Assert-ExactKeys $Record $fields "candidate preflight record"
    Assert-ExactPropertyOrder $Record $fields "candidate preflight record"
    Assert-IntegerRange $Record.schemaVersion 1 1 "candidate preflight schemaVersion"
    if ($Record.evidenceType -cne "stock-user-chrome-candidate-binding" -or $Record.phase -cne "preflight") {
        throw "Candidate preflight identity is invalid."
    }
    Assert-ExactBoolean $Record.passed $true "candidate preflight passed"
    Assert-Hex $Record.runNonce 64 "candidate preflight run nonce"
    $candidate = Assert-CandidateBinding $Record.candidate
    if ($candidate.version -ceq $script:OperatorV2Version) {
        [void](Assert-ReleaseCandidateBinding $Record.releaseCandidateBinding $candidate "preflight releaseCandidateBinding")
    }
    return $candidate
}

function Assert-CandidatePostflight {
    param([object]$Record)
    $fields = @(
        "schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed", "runNonce", "candidate",
        "candidateBinding", "preflightRecordSha256", "unchanged"
    )
    if ($Record.candidate.version -ceq $script:OperatorV2Version) {
        $fields = @(
            "schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed", "runNonce",
            "releaseCandidateBinding", "candidate", "candidateBinding", "preflightRecordSha256", "unchanged"
        )
    }
    Assert-ExactKeys $Record $fields "candidate postflight record"
    Assert-ExactPropertyOrder $Record $fields "candidate postflight record"
    Assert-IntegerRange $Record.schemaVersion 1 1 "candidate postflight schemaVersion"
    if ($Record.evidenceType -cne "stock-user-chrome-candidate-binding" -or $Record.phase -cne "postflight") {
        throw "Candidate postflight identity is invalid."
    }
    Assert-ExactBoolean $Record.passed $true "candidate postflight passed"
    Assert-Hex $Record.runNonce 64 "candidate postflight run nonce"
    Assert-Hex $Record.preflightRecordSha256 64 "preflight record SHA-256"
    $unchangedKeys = @(
        "checkoutHead", "checkoutClean", "checksumManifest", "serverExecutable", "extensionZip", "extractedPayload"
    )
    if ($Record.candidate.version -ceq $script:OperatorV2Version) {
        $unchangedKeys = @(
            "checkoutHead", "checkoutClean", "checksumManifest", "serverExecutable",
            "computerHelperExecutable", "extensionZip", "extractedPayload"
        )
    }
    Assert-ExactKeys $Record.unchanged $unchangedKeys "candidate unchanged assertions"
    foreach ($property in $Record.unchanged.PSObject.Properties) {
        Assert-ExactBoolean $property.Value $true "candidate unchanged $($property.Name)"
    }
    $candidate = Assert-CandidateBinding $Record.candidate
    if ($candidate.version -ceq $script:OperatorV2Version) {
        [void](Assert-ReleaseCandidateBinding $Record.releaseCandidateBinding $candidate "postflight releaseCandidateBinding")
    }
    return $candidate
}

function Get-CandidateBindingDomain {
    param([object]$Preflight, [string]$PreflightSha256)
    $binding = [ordered]@{
        runNonce = [string]$Preflight.runNonce
        preflightRecordSha256 = $PreflightSha256
        finalSha = [string]$Preflight.candidate.finalSha
        checksumManifestSha256 = [string]$Preflight.candidate.checksumManifest.sha256
        serverSha256 = [string]$Preflight.candidate.server.sha256
    }
    if ($Preflight.candidate.version -ceq $script:OperatorV2Version) {
        $binding.computerHelperSha256 = [string]$Preflight.candidate.computerHelper.sha256
    }
    $binding.extensionZipSha256 = [string]$Preflight.candidate.extension.sha256
    $binding.extractedPayloadSha256 = [string]$Preflight.candidate.extension.combinedPayloadSha256
    return $binding
}

function Assert-CandidateBindingDomain {
    param([object]$Binding, [object]$Expected, [string]$Label)
    $fields = @(
        "runNonce", "preflightRecordSha256", "finalSha", "checksumManifestSha256",
        "serverSha256", "extensionZipSha256", "extractedPayloadSha256"
    )
    $hasComputerHelper = if ($Expected -is [Collections.IDictionary]) {
        $Expected.Contains("computerHelperSha256")
    }
    else { $null -ne $Expected.PSObject.Properties["computerHelperSha256"] }
    if ($hasComputerHelper) {
        $fields = @(
            "runNonce", "preflightRecordSha256", "finalSha", "checksumManifestSha256",
            "serverSha256", "computerHelperSha256", "extensionZipSha256", "extractedPayloadSha256"
        )
    }
    Assert-ExactKeys $Binding $fields $Label
    Assert-ExactPropertyOrder $Binding $fields $Label
    Assert-Hex $Binding.runNonce 64 "$Label run nonce"
    Assert-Hex $Binding.preflightRecordSha256 64 "$Label preflight record SHA-256"
    Assert-Hex $Binding.finalSha 40 "$Label FINAL_SHA"
    foreach ($name in @($fields | Where-Object { $_ -like "*Sha256" })) {
        Assert-Hex $Binding.$name 64 "$Label $name"
    }
    if (($Binding | ConvertTo-Json -Depth 5 -Compress) -cne ($Expected | ConvertTo-Json -Depth 5 -Compress)) {
        throw "$Label does not bind the exact preflight candidate and run."
    }
}

function Assert-PauseRefusal {
    param([object]$Refusal, [string]$Label)
    Assert-ExactKeys $Refusal @("httpStatus", "errorCode", "taxonomyState", "taxonomyAction", "retriable") $Label
    Assert-IntegerRange $Refusal.httpStatus 423 423 "$Label HTTP status"
    if ($Refusal.errorCode -cne "HUMAN_CONTROL_PAUSED" -or $Refusal.taxonomyState -cne "needs_user" -or $Refusal.taxonomyAction -cne "handback") {
        throw "$Label does not prove the fail-closed human-pause response."
    }
    Assert-ExactBoolean $Refusal.retriable $false "$Label retriable"
}

function Assert-ApiMatrixRecord {
    param([object]$Record, [string]$ExpectedVersion, [object]$ExpectedBinding)
    $expectedScreenshotMap = if ($ExpectedVersion -ceq $script:OperatorV2Version) {
        $script:MethodScreenshotsV2
    }
    else { $script:MethodScreenshots }
    $recordFields = @(
        "schemaVersion", "evidenceType", "version", "target", "candidateBinding", "passed", "methodCount", "methods", "assertions"
    )
    Assert-ExactKeys $Record $recordFields "API matrix record"
    Assert-ExactPropertyOrder $Record $recordFields "API matrix record"
    Assert-IntegerRange $Record.schemaVersion 1 1 "API matrix schemaVersion"
    Assert-IntegerRange $Record.methodCount $script:Methods.Count $script:Methods.Count "API matrix methodCount"
    if ($Record.evidenceType -cne "stock-user-chrome-api-matrix" -or
        $Record.version -cne $ExpectedVersion -or $Record.target -cne "loopback-demo" -or
        $Record.methodCount -ne $script:Methods.Count) {
        throw "API matrix record identity is invalid."
    }
    Assert-CandidateBindingDomain $Record.candidateBinding $ExpectedBinding "API matrix candidateBinding"
    Assert-ExactBoolean $Record.passed $true "API matrix passed"
    $methods = @($Record.methods)
    if ($methods.Count -ne $script:Methods.Count) {
        throw "API matrix must contain exactly 25 method results."
    }
    for ($index = 0; $index -lt $script:Methods.Count; $index += 1) {
        $item = $methods[$index]
        $methodFields = @(
            "name", "passed", "stage", "commandInvoked", "resultVerified",
            "postconditionVerified", "screenshot", "machineProof"
        )
        Assert-ExactKeys $item $methodFields "API matrix method"
        Assert-ExactPropertyOrder $item $methodFields "API matrix method"
        if ($item.name -cne $script:Methods[$index] -or
            $item.stage -cne $script:MethodStages[$script:Methods[$index]]) {
            throw "API matrix method order or stage is invalid."
        }
        Assert-ExactBoolean $item.passed $true "API matrix $($item.name)"
        Assert-ExactBoolean $item.commandInvoked $true "API matrix $($item.name) command"
        Assert-ExactBoolean $item.resultVerified $true "API matrix $($item.name) result"
        Assert-ExactBoolean $item.postconditionVerified $true "API matrix $($item.name) postcondition"
        if ($item.screenshot -cne $expectedScreenshotMap[$item.name]) {
            throw "API matrix $($item.name) screenshot mapping is invalid."
        }
        if ($item.machineProof -cne "machine-command-result-postcondition") {
            throw "API matrix $($item.name) machine proof is invalid."
        }
    }
    $expectedAssertions = @(
        "serverVersionMatched", "extensionVersionMatched", "browserFloorMet", "realExtensionConnected",
        "fullAccessEnabled", "capabilitiesComplete", "freshCommandIdentity",
        "freshObservationAfterPageMutation", "dynamicTargetDiscovery", "testOwnedTabsOnly",
        "topLayerControlUiIntegrity", "dialogLifecycle", "cleanupComplete"
    )
    Assert-ExactKeys $Record.assertions $expectedAssertions "API matrix assertions"
    Assert-ExactPropertyOrder $Record.assertions $expectedAssertions "API matrix assertions"
    foreach ($name in $expectedAssertions) {
        Assert-ExactBoolean $Record.assertions.$name $true "API matrix assertion $name"
    }
}

function Assert-Resume {
    param([object]$Resume, [string]$Label)
    Assert-ExactKeys $Resume @(
        "trustedPopupClick", "statusPollMethod", "statusPolledAfterResume", "reducedStatus",
        "postResumeStartSucceeded", "activeStatusPolled", "activeStatus"
    ) $Label
    Assert-ExactBoolean $Resume.trustedPopupClick $true "$Label trusted popup click"
    if ($Resume.statusPollMethod -cne "browser.control.status") {
        throw "$Label must poll browser.control.status after Resume."
    }
    Assert-ExactBoolean $Resume.statusPolledAfterResume $true "$Label status poll"
    Assert-ExactKeys $Resume.reducedStatus @("active", "humanPaused", "revocationPending") "$Label reduced status"
    Assert-ExactBoolean $Resume.reducedStatus.active $false "$Label inactive before restart"
    Assert-ExactBoolean $Resume.reducedStatus.humanPaused $false "$Label pause cleared"
    Assert-ExactBoolean $Resume.reducedStatus.revocationPending $false "$Label revocation cleanup"
    Assert-ExactBoolean $Resume.postResumeStartSucceeded $true "$Label post-Resume start"
    Assert-ExactBoolean $Resume.activeStatusPolled $true "$Label active status poll"
    Assert-ExactKeys $Resume.activeStatus @("active", "humanPaused", "revocationPending") "$Label active status"
    Assert-ExactBoolean $Resume.activeStatus.active $true "$Label active lease"
    Assert-ExactBoolean $Resume.activeStatus.humanPaused $false "$Label active pause"
    Assert-ExactBoolean $Resume.activeStatus.revocationPending $false "$Label active cleanup"
}

function Assert-HandbackCase {
    param([object]$Case, [string]$Name, [string]$Trigger, [string]$Reason)
    Assert-ExactKeys $Case @(
        "trigger", "statusPollMethod", "statusPolledAfterTrigger", "reducedStatus",
        "controlStartRefusal", "tabMutationRefusal", "indicatorsRemoved", "resume"
    ) "$Name handback"
    if ($Case.trigger -cne $Trigger -or $Case.statusPollMethod -cne "browser.control.status") {
        throw "$Name handback trigger or status polling method is invalid."
    }
    Assert-ExactBoolean $Case.statusPolledAfterTrigger $true "$Name post-trigger status poll"
    Assert-ExactKeys $Case.reducedStatus @("active", "humanPaused", "reason", "revocationPending") "$Name reduced status"
    Assert-ExactBoolean $Case.reducedStatus.active $false "$Name inactive status"
    Assert-ExactBoolean $Case.reducedStatus.humanPaused $true "$Name human pause"
    Assert-ExactBoolean $Case.reducedStatus.revocationPending $false "$Name revocation cleanup"
    if ($Case.reducedStatus.reason -cne $Reason) {
        throw "$Name handback reason is invalid."
    }
    Assert-PauseRefusal $Case.controlStartRefusal "$Name control-start refusal"
    Assert-PauseRefusal $Case.tabMutationRefusal "$Name tab-mutation refusal"
    Assert-ExactBoolean $Case.indicatorsRemoved $true "$Name indicator removal"
    Assert-Resume $Case.resume "$Name Resume"
}

function Assert-OperatorResultsV1 {
    param([object]$Operator, [string]$ExpectedVersion, [object]$ExpectedBinding)
    $operatorFields = @(
        "schemaVersion", "evidenceType", "candidateBinding", "environment", "extension", "handback", "cleanup"
    )
    Assert-ExactKeys $Operator $operatorFields "operator results"
    Assert-ExactPropertyOrder $Operator $operatorFields "operator results"
    Assert-IntegerRange $Operator.schemaVersion 1 1 "operator schemaVersion"
    if ($Operator.evidenceType -cne "stock-user-chrome-operator-observations") {
        throw "Operator results identity is invalid."
    }
    Assert-CandidateBindingDomain $Operator.candidateBinding $ExpectedBinding "operator candidateBinding"
    $environment = $Operator.environment
    Assert-ExactKeys $environment @(
        "platform", "browserProduct", "browserVersion", "stockUserChrome", "chromeMcpUsed",
        "manualChromeExtensionsLoad", "developerMode", "dedicatedWindowBoundToOwnedTarget", "browserLaunchFlagsUsed", "cdpInstallUsed",
        "automationTestProfileUsed", "localDemoOnly"
    ) "operator environment"
    if ($environment.platform -cne "windows-x86_64" -or $environment.browserProduct -cne "Google Chrome" -or
        -not [regex]::IsMatch([string]$environment.browserVersion, '^[0-9]{1,3}\.[0-9]{1,5}\.[0-9]{1,5}\.[0-9]{1,5}$') -or
        [int]([string]$environment.browserVersion).Split('.')[0] -lt 140) {
        throw "Operator browser identity is invalid."
    }
    foreach ($name in @("stockUserChrome", "chromeMcpUsed", "manualChromeExtensionsLoad", "developerMode", "dedicatedWindowBoundToOwnedTarget", "localDemoOnly")) {
        Assert-ExactBoolean $environment.$name $true "operator environment $name"
    }
    foreach ($name in @("browserLaunchFlagsUsed", "cdpInstallUsed", "automationTestProfileUsed")) {
        Assert-ExactBoolean $environment.$name $false "operator environment $name"
    }

    $extension = $Operator.extension
    Assert-ExactKeys $extension @(
        "cardCount", "idPatternValid", "version", "enabled", "loadErrors", "loadedVia", "loadedDirectoryByteMatchesCandidateZip", "permissionsUiReviewed",
        "hostAccessUiReviewed", "popupConnected", "nativeDebuggerWarningSeen", "pageControlPillSeen",
        "pageStopButtonSeen"
    ) "operator extension card"
    Assert-IntegerRange $extension.cardCount 1 1 "operator extension cardCount"
    Assert-IntegerRange $extension.loadErrors 0 0 "operator extension loadErrors"
    if ($extension.version -cne $ExpectedVersion -or $extension.loadedVia -cne "chrome://extensions-load-unpacked") {
        throw "Operator extension card identity is invalid."
    }
    foreach ($name in @("idPatternValid", "enabled", "loadedDirectoryByteMatchesCandidateZip", "permissionsUiReviewed", "hostAccessUiReviewed", "popupConnected", "nativeDebuggerWarningSeen", "pageControlPillSeen", "pageStopButtonSeen")) {
        Assert-ExactBoolean $extension.$name $true "operator extension $name"
    }

    Assert-ExactKeys $Operator.handback @("stop", "cancel") "handback matrix"
    Assert-HandbackCase $Operator.handback.stop "Stop" "in-page-stop" "released_by_user"
    Assert-HandbackCase $Operator.handback.cancel "Cancel" "chrome-native-cancel" "canceled_by_user"

    $cleanup = $Operator.cleanup
    Assert-ExactKeys $cleanup @(
        "controlReleased", "testTabsClosed", "testWindowClosed", "popupDisconnected", "serverStopped",
        "portReleased", "acceptanceCredentialClearedFromShell", "savedTokenClear", "chromeMcpReleased", "developerModeRestored", "extensionDisposition",
        "unrelatedTabsOrWindowsChanged", "unrelatedExtensionsChanged", "rawBrowserDataRetained"
    ) "cleanup results"
    foreach ($name in @(
        "controlReleased", "testTabsClosed", "testWindowClosed", "popupDisconnected", "serverStopped",
        "portReleased", "acceptanceCredentialClearedFromShell", "chromeMcpReleased", "developerModeRestored"
    )) {
        Assert-ExactBoolean $cleanup.$name $true "cleanup $name"
    }
    foreach ($name in @("unrelatedTabsOrWindowsChanged", "unrelatedExtensionsChanged", "rawBrowserDataRetained")) {
        Assert-ExactBoolean $cleanup.$name $false "cleanup $name"
    }
    Assert-ExactKeys $cleanup.savedTokenClear @(
        "trustedPopupClick", "popupStateVerifiedAfterClear", "tokenConfigured", "clearButtonDisabled"
    ) "saved token cleanup proof"
    foreach ($name in @("trustedPopupClick", "popupStateVerifiedAfterClear", "clearButtonDisabled")) {
        Assert-ExactBoolean $cleanup.savedTokenClear.$name $true "saved token cleanup $name"
    }
    Assert-ExactBoolean $cleanup.savedTokenClear.tokenConfigured $false "saved token cleanup tokenConfigured"
    if (@("kept-single-enabled-identity", "removed-test-owned-copy") -cnotcontains $cleanup.extensionDisposition) {
        throw "Cleanup extension disposition is invalid."
    }
}

function Assert-BrowserChromeSurfaceV2 {
    param([object]$Value, [string]$Label)
    if ($Value -isnot [string] -or $script:BrowserChromeSurfaces -cnotcontains $Value) {
        throw "$Label must use the candidate-bound Local Browser Bridge computer helper."
    }
}

function Assert-ToggleValueV2 {
    param([object]$Value, [string]$Label)
    if ($Value -isnot [string] -or @("enabled", "disabled") -cnotcontains $Value) {
        throw "$Label must be enabled or disabled."
    }
}

function Assert-ResumeV2 {
    param([object]$Resume, [string]$Label)
    Assert-ExactKeys $Resume @(
        "trustedPopupClick", "operatorSurface", "statusPollMethod", "statusPolledAfterResume",
        "reducedStatus", "postResumeStartSucceeded", "activeStatusPolled", "activeStatus"
    ) $Label
    Assert-ExactPropertyOrder $Resume @(
        "trustedPopupClick", "operatorSurface", "statusPollMethod", "statusPolledAfterResume",
        "reducedStatus", "postResumeStartSucceeded", "activeStatusPolled", "activeStatus"
    ) $Label
    Assert-ExactBoolean $Resume.trustedPopupClick $true "$Label trusted popup click"
    Assert-BrowserChromeSurfaceV2 $Resume.operatorSurface "$Label operator surface"
    if ($Resume.statusPollMethod -cne "browser.control.status") {
        throw "$Label must poll browser.control.status after Resume."
    }
    Assert-ExactBoolean $Resume.statusPolledAfterResume $true "$Label status poll"
    Assert-ExactKeys $Resume.reducedStatus @("active", "humanPaused", "revocationPending") "$Label reduced status"
    Assert-ExactBoolean $Resume.reducedStatus.active $false "$Label inactive before restart"
    Assert-ExactBoolean $Resume.reducedStatus.humanPaused $false "$Label pause cleared"
    Assert-ExactBoolean $Resume.reducedStatus.revocationPending $false "$Label revocation cleanup"
    Assert-ExactBoolean $Resume.postResumeStartSucceeded $true "$Label post-Resume start"
    Assert-ExactBoolean $Resume.activeStatusPolled $true "$Label active status poll"
    Assert-ExactKeys $Resume.activeStatus @("active", "humanPaused", "revocationPending") "$Label active status"
    Assert-ExactBoolean $Resume.activeStatus.active $true "$Label active lease"
    Assert-ExactBoolean $Resume.activeStatus.humanPaused $false "$Label active pause"
    Assert-ExactBoolean $Resume.activeStatus.revocationPending $false "$Label active cleanup"
}

function Assert-HandbackCaseV2 {
    param(
        [object]$Case,
        [string]$Name,
        [string]$Trigger,
        [string]$Reason,
        [bool]$BrowserChromeTrigger
    )
    $fields = if ($BrowserChromeTrigger) {
        @(
            "trigger", "operatorSurface", "statusPollMethod", "statusPolledAfterTrigger",
            "reducedStatus", "controlStartRefusal", "tabMutationRefusal", "indicatorsRemoved", "resume"
        )
    }
    else {
        @(
            "trigger", "statusPollMethod", "statusPolledAfterTrigger", "reducedStatus",
            "controlStartRefusal", "tabMutationRefusal", "indicatorsRemoved", "resume"
        )
    }
    Assert-ExactKeys $Case $fields "$Name handback"
    Assert-ExactPropertyOrder $Case $fields "$Name handback"
    if ($Case.trigger -cne $Trigger -or $Case.statusPollMethod -cne "browser.control.status") {
        throw "$Name handback trigger or status polling method is invalid."
    }
    if ($BrowserChromeTrigger) {
        Assert-BrowserChromeSurfaceV2 $Case.operatorSurface "$Name handback operator surface"
    }
    Assert-ExactBoolean $Case.statusPolledAfterTrigger $true "$Name post-trigger status poll"
    Assert-ExactKeys $Case.reducedStatus @("active", "humanPaused", "reason", "revocationPending") "$Name reduced status"
    Assert-ExactBoolean $Case.reducedStatus.active $false "$Name inactive status"
    Assert-ExactBoolean $Case.reducedStatus.humanPaused $true "$Name human pause"
    Assert-ExactBoolean $Case.reducedStatus.revocationPending $false "$Name revocation cleanup"
    if ($Case.reducedStatus.reason -cne $Reason) {
        throw "$Name handback reason is invalid."
    }
    Assert-PauseRefusal $Case.controlStartRefusal "$Name control-start refusal"
    Assert-PauseRefusal $Case.tabMutationRefusal "$Name tab-mutation refusal"
    Assert-ExactBoolean $Case.indicatorsRemoved $true "$Name indicator removal"
    Assert-ResumeV2 $Case.resume "$Name Resume"
}

function Assert-ApprovedCheckpointV3 {
    param([object]$Checkpoint, [string]$ApprovalId, [string]$Label, [bool]$ExpectedPerformed)
    Assert-ExactKeys $Checkpoint @("performed", "approvalRef") $Label
    Assert-ExactPropertyOrder $Checkpoint @("performed", "approvalRef") $Label
    Assert-ExactBoolean $Checkpoint.performed $ExpectedPerformed "$Label performed"
    if ($Checkpoint.approvalRef -cne $ApprovalId) {
        throw "$Label is not bound to the single scoped action-time approval."
    }
}

function Assert-OperatorResultsV2 {
    param([object]$Operator, [string]$ExpectedVersion, [object]$ExpectedBinding, [object]$ExpectedReleaseBinding, [object]$Candidate)
    $operatorFields = @(
        "schemaVersion", "evidenceType", "releaseCandidateBinding", "candidateBinding", "environment", "actionSurfaces",
        "computerHelperChain", "consentCheckpoints", "initialState", "extension",
        "handback", "screenshotCaptures", "independentVisualReview", "restoration", "cleanup", "retainedEvidence"
    )
    Assert-ExactKeys $Operator $operatorFields "v0.12.52 operator results"
    Assert-ExactPropertyOrder $Operator $operatorFields "v0.12.52 operator results"
    Assert-IntegerRange $Operator.schemaVersion 3 3 "v0.12.52 operator schemaVersion"
    if ($ExpectedVersion -cne $script:OperatorV2Version -or
        $Operator.evidenceType -cne "stock-user-chrome-operator-observations") {
        throw "The v0.12.52 operator schema is bound only to a v0.12.52 candidate."
    }
    [void](Assert-ReleaseCandidateBinding $Operator.releaseCandidateBinding $Candidate "v0.12.52 operator releaseCandidateBinding")
    if (($Operator.releaseCandidateBinding | ConvertTo-Json -Depth 10 -Compress) -cne
        ($ExpectedReleaseBinding | ConvertTo-Json -Depth 10 -Compress)) {
        throw "The operator record is not bound to the exact release candidate attempt."
    }
    Assert-CandidateBindingDomain $Operator.candidateBinding $ExpectedBinding "v0.12.52 operator candidateBinding"

    $environment = $Operator.environment
    $environmentFields = @(
        "platform", "browserProduct", "browserVersion", "stockUserChrome", "existingUserSession",
        "dedicatedTemporaryWindow", "browserLaunchFlagsUsed", "directCdpUsed",
        "automationTestProfileUsed", "localDemoOnly"
    )
    Assert-ExactKeys $environment $environmentFields "v0.12.52 operator environment"
    Assert-ExactPropertyOrder $environment $environmentFields "v0.12.52 operator environment"
    if ($environment.platform -cne "windows-x86_64" -or $environment.browserProduct -cne "Google Chrome" -or
        -not [regex]::IsMatch([string]$environment.browserVersion, '^[0-9]{1,3}\.[0-9]{1,5}\.[0-9]{1,5}\.[0-9]{1,5}$') -or
        [int]([string]$environment.browserVersion).Split('.')[0] -lt 140) {
        throw "The v0.12.52 operator browser identity is invalid."
    }
    foreach ($name in @("stockUserChrome", "existingUserSession", "dedicatedTemporaryWindow", "localDemoOnly")) {
        Assert-ExactBoolean $environment.$name $true "v0.12.52 operator environment $name"
    }
    foreach ($name in @("browserLaunchFlagsUsed", "directCdpUsed", "automationTestProfileUsed")) {
        Assert-ExactBoolean $environment.$name $false "v0.12.52 operator environment $name"
    }

    $surfaces = $Operator.actionSurfaces
    $surfaceFields = @(
        "bridgeApiMatrix", "computerHelperApi", "orchestrationAndConsent", "dedicatedWindowCreation",
        "chromeExtensionsPage", "nativeLoadUnpackedPicker", "extensionPopup", "chromeDebuggerNotice",
        "browserApiResult", "computerShareAction", "acceptanceScreenshots",
        "debuggerOwnerDuringBridgeLease", "competingDebuggerAttachmentAllowed",
        "chromeMcpUsedDuringBridgeLease", "chromeMcpReleaseEvidenceClaimed"
    )
    Assert-ExactKeys $surfaces $surfaceFields "v0.12.52 action surfaces"
    Assert-ExactPropertyOrder $surfaces $surfaceFields "v0.12.52 action surfaces"
    if ($surfaces.bridgeApiMatrix -cne "local-browser-bridge-api" -or
        $surfaces.computerHelperApi -cne "local-browser-bridge-computer-api" -or
        $surfaces.debuggerOwnerDuringBridgeLease -cne "local-browser-bridge-extension") {
        throw "The Local Browser Bridge extension must be the exclusive debugger owner during its lease."
    }
    if ($surfaces.orchestrationAndConsent -cne `
        "user-orchestrator-secured-ssh-exported-file-review") {
        throw "v0.12.52 requires the secured-SSH exported-file review orchestration surface."
    }
    foreach ($name in @(
        "dedicatedWindowCreation", "chromeExtensionsPage", "nativeLoadUnpackedPicker",
        "extensionPopup", "chromeDebuggerNotice", "browserApiResult",
        "computerShareAction", "acceptanceScreenshots"
    )) {
        Assert-BrowserChromeSurfaceV2 $surfaces.$name "v0.12.52 action surface $name"
    }
    Assert-ExactBoolean $surfaces.competingDebuggerAttachmentAllowed $false "competing debugger attachment"
    Assert-ExactBoolean $surfaces.chromeMcpUsedDuringBridgeLease $false "Chrome MCP use during the bridge lease"
    Assert-ExactBoolean $surfaces.chromeMcpReleaseEvidenceClaimed $false "Chrome MCP release-evidence claim"

    $helper = $Operator.computerHelperChain
    $helperFields = @(
        "candidateBoundExecutableStarted", "connectedThroughLoopbackServer", "serverApiOnly",
        "exactChromeWindowSelected", "chromeExtensionsLoadCompleted", "browserApiActionCompleted",
        "exactWindowShareActionCompleted", "freshFramesBoundToComputerAction", "screenshotEndpoint",
        "rawScreenshotCount", "topologyProtocolBound", "ownerForcedSupervisorTermination", "helperDisconnectedAfterTermination",
        "noHelperChildrenOrListenersRemain", "helperStopped"
    )
    Assert-ExactKeys $helper $helperFields "v0.12.52 computer helper chain"
    Assert-ExactPropertyOrder $helper $helperFields "v0.12.52 computer helper chain"
    foreach ($name in @(
        "candidateBoundExecutableStarted", "connectedThroughLoopbackServer", "serverApiOnly",
        "exactChromeWindowSelected", "chromeExtensionsLoadCompleted", "browserApiActionCompleted",
        "exactWindowShareActionCompleted", "freshFramesBoundToComputerAction",
        "topologyProtocolBound", "ownerForcedSupervisorTermination", "helperDisconnectedAfterTermination",
        "noHelperChildrenOrListenersRemain", "helperStopped"
    )) {
        Assert-ExactBoolean $helper.$name $true "v0.12.52 computer helper chain $name"
    }
    if ($helper.screenshotEndpoint -cne "/api/computer/screenshot") {
        throw "v0.12.52 computer helper screenshots must use /api/computer/screenshot."
    }
    Assert-IntegerRange $helper.rawScreenshotCount 6 6 "v0.12.52 computer helper raw screenshot count"

    $initial = $Operator.initialState
    Assert-ExactKeys $initial @(
        "capturedBeforeRelevantMutation", "candidateExtensionPresent", "developerMode",
        "fullAccess", "savedTokenConfigured"
    ) "v0.12.52 initial state"
    Assert-ExactBoolean $initial.capturedBeforeRelevantMutation $true "initial-state capture"
    Assert-ExactBoolean $initial.candidateExtensionPresent $false "initial candidate extension presence"
    Assert-ExactBoolean $initial.savedTokenConfigured $false "initial saved token configured"
    foreach ($name in @("developerMode", "fullAccess")) {
        Assert-ExactKeys $initial.$name @("value", "changedByRun") "initial $name"
        Assert-ToggleValueV2 $initial.$name.value "initial $name value"
        if ($initial.$name.changedByRun -isnot [bool]) { throw "initial $name changedByRun must be Boolean." }
        $expectedChanged = $initial.$name.value -ceq "disabled"
        Assert-ExactBoolean $initial.$name.changedByRun $expectedChanged "$name change tracking"
    }

    $consent = $Operator.consentCheckpoints
    Assert-ExactKeys $consent @(
        "scopedActionTimeApproval", "installCandidate", "developerModeChange", "fullAccessUse",
        "acceptanceTokenSave", "clearSavedToken", "removeTestOwnedExtension",
        "restoreCapturedBrowserSettings", "failureRollback"
    ) "v0.12.52 consent checkpoints"
    Assert-ExactPropertyOrder $consent @(
        "scopedActionTimeApproval", "installCandidate", "developerModeChange", "fullAccessUse",
        "acceptanceTokenSave", "clearSavedToken", "removeTestOwnedExtension",
        "restoreCapturedBrowserSettings", "failureRollback"
    ) "v0.12.52 consent checkpoints"
    $approvalSummary = $consent.scopedActionTimeApproval
    Assert-ExactKeys $approvalSummary @(
        "recordSha256", "approvalId", "obtainedAtActionTime", "consumedBeforeFirstCoveredAction",
        "consumedBeforeExpiry", "freshStateRevalidatedAfterApproval",
        "scopeUnchangedThroughRun", "singleCandidateRun"
    ) "scoped action-time approval summary"
    Assert-Hex $approvalSummary.recordSha256 64 "scoped approval record SHA-256"
    Assert-Hex $approvalSummary.approvalId 64 "scoped approval id"
    foreach ($name in @(
        "obtainedAtActionTime", "consumedBeforeFirstCoveredAction", "consumedBeforeExpiry",
        "freshStateRevalidatedAfterApproval", "scopeUnchangedThroughRun", "singleCandidateRun"
    )) {
        Assert-ExactBoolean $approvalSummary.$name $true "scoped action-time approval $name"
    }
    Assert-ApprovedCheckpointV3 $consent.installCandidate $approvalSummary.approvalId "candidate installation" $true
    Assert-ApprovedCheckpointV3 $consent.developerModeChange $approvalSummary.approvalId `
        "Developer Mode change" $initial.developerMode.changedByRun
    Assert-ApprovedCheckpointV3 $consent.fullAccessUse $approvalSummary.approvalId `
        "Full Access use" $true
    foreach ($name in @("acceptanceTokenSave", "clearSavedToken", "restoreCapturedBrowserSettings")) {
        Assert-ApprovedCheckpointV3 $consent.$name $approvalSummary.approvalId "$name checkpoint" $true
    }
    Assert-ApprovedCheckpointV3 $consent.removeTestOwnedExtension $approvalSummary.approvalId `
        "extension-disposition consent" $true
    Assert-ApprovedCheckpointV3 $consent.failureRollback $approvalSummary.approvalId "failure rollback" $false

    $extension = $Operator.extension
    Assert-ExactKeys $extension @(
        "cardCount", "version", "enabled", "loadErrors", "loadedVia",
        "loadedDirectoryByteMatchesCandidateZip", "popupConnected",
        "debuggerLeaseActiveAtFirstCapture", "nativeDebuggerUseIndicatorSeen"
    ) "v0.12.52 extension proof"
    Assert-IntegerRange $extension.cardCount 1 1 "v0.12.52 extension cardCount"
    Assert-IntegerRange $extension.loadErrors 0 0 "v0.12.52 extension loadErrors"
    if ($extension.version -cne $ExpectedVersion -or
        $extension.loadedVia -cne "chrome://extensions-load-unpacked") {
        throw "The v0.12.52 extension card identity is invalid."
    }
    foreach ($name in @(
        "enabled", "loadedDirectoryByteMatchesCandidateZip", "popupConnected",
        "debuggerLeaseActiveAtFirstCapture", "nativeDebuggerUseIndicatorSeen"
    )) {
        Assert-ExactBoolean $extension.$name $true "v0.12.52 extension $name"
    }

    Assert-ExactKeys $Operator.handback @("stop", "cancel") "v0.12.52 handback matrix"
    Assert-ExactPropertyOrder $Operator.handback @("stop", "cancel") "v0.12.52 handback matrix"
    Assert-HandbackCaseV2 $Operator.handback.stop "Stop" "in-page-stop" "released_by_user" $true
    Assert-HandbackCaseV2 $Operator.handback.cancel "Cancel" "chrome-native-cancel" "canceled_by_user" $true

    if ($Operator.screenshotCaptures.Count -ne $script:ExpectedScreenshotsV2.Count) {
        throw "v0.12.52 must bind exactly six machine-helper screenshot capture surfaces."
    }
    for ($index = 0; $index -lt $script:ExpectedScreenshotsV2.Count; $index += 1) {
        $purpose = @($script:ExpectedScreenshotsV2.Keys)[$index]
        $capture = $Operator.screenshotCaptures[$index]
        Assert-ExactKeys $capture @(
            "purpose", "image", "captureSurface", "requiredVisibleState"
        ) "v0.12.52 screenshot capture"
        if ($capture.purpose -cne $purpose -or
            $capture.image -cne $script:ExpectedScreenshotsV2[$purpose] -or
            $capture.captureSurface -cne "local-browser-bridge-computer-helper" -or
            $capture.captureSurface -cne $surfaces.acceptanceScreenshots -or
            $capture.requiredVisibleState -cne $script:RequiredVisibleStatesV2[$purpose]) {
            throw "v0.12.52 screenshot identity, helper surface, or visible-state criterion is invalid."
        }
    }

    $review = $Operator.independentVisualReview
    $reviewFields = @(
        "recordSha256", "executorSessionRef", "reviewerSessionRef", "independentSessionBoundary",
        "reviewedCropCount", "everySanitizedCropOpenedByReviewer", "allImageDigestsMatched",
        "requiredVisibleStateConfirmedByReviewer", "noSensitivePixelsObservedByReviewer",
        "noUncertaintyReported", "visualJudgmentNotPixelSafetyProof"
    )
    Assert-ExactKeys $review $reviewFields "v0.12.52 independent visual review"
    Assert-ExactPropertyOrder $review $reviewFields "v0.12.52 independent visual review"
    foreach ($name in @("recordSha256", "executorSessionRef", "reviewerSessionRef")) {
        Assert-Hex $review.$name 64 "v0.12.52 independent visual review $name"
    }
    if ($review.executorSessionRef -ceq $review.reviewerSessionRef) {
        throw "The independent visual reviewer must not reuse the executor session."
    }
    foreach ($name in @(
        "independentSessionBoundary", "everySanitizedCropOpenedByReviewer", "allImageDigestsMatched",
        "requiredVisibleStateConfirmedByReviewer", "noSensitivePixelsObservedByReviewer",
        "noUncertaintyReported", "visualJudgmentNotPixelSafetyProof"
    )) {
        Assert-ExactBoolean $review.$name $true "v0.12.52 independent visual review $name"
    }
    Assert-IntegerRange $review.reviewedCropCount 6 6 "v0.12.52 independently reviewed crop count"

    $restoration = $Operator.restoration
    Assert-ExactKeys $restoration @(
        "candidateExtensionPresence", "developerMode", "fullAccess", "savedToken"
    ) "v0.12.52 restoration"
    Assert-ExactKeys $restoration.candidateExtensionPresence @(
        "finalPresent", "matchesInitial", "verifiedFromLiveUi"
    ) "candidate-extension restoration"
    Assert-ExactBoolean $restoration.candidateExtensionPresence.finalPresent $false "final candidate-extension presence"
    Assert-ExactBoolean $restoration.candidateExtensionPresence.matchesInitial $true "candidate-extension restoration equality"
    Assert-ExactBoolean $restoration.candidateExtensionPresence.verifiedFromLiveUi $true "candidate-extension live-UI restoration verification"
    foreach ($name in @("developerMode", "fullAccess")) {
        Assert-ExactKeys $restoration.$name @(
            "finalValue", "matchesInitial", "verifiedFromLiveUi"
        ) "$name restoration"
        Assert-ToggleValueV2 $restoration.$name.finalValue "$name final value"
        if ($restoration.$name.finalValue -cne $initial.$name.value) {
            throw "$name final value does not equal its captured initial value."
        }
        Assert-ExactBoolean $restoration.$name.matchesInitial $true "$name restoration equality"
        Assert-ExactBoolean $restoration.$name.verifiedFromLiveUi $true "$name live-UI restoration verification"
    }
    Assert-ExactKeys $restoration.savedToken @(
        "finalConfigured", "matchesInitial", "verifiedFromLiveUi"
    ) "saved-token restoration"
    Assert-ExactBoolean $restoration.savedToken.finalConfigured $false "final saved-token state"
    Assert-ExactBoolean $restoration.savedToken.matchesInitial $true "saved-token restoration equality"
    Assert-ExactBoolean $restoration.savedToken.verifiedFromLiveUi $true "saved-token live-UI restoration verification"

    $cleanup = $Operator.cleanup
    $cleanupFields = @(
        "controlReleased", "testTabsClosed", "testWindowClosed", "popupDisconnected", "serverStopped",
        "portReleased", "acceptanceCredentialClearedFromShell", "savedTokenClear",
        "externalSurfacePreflightAttestationSha256", "externalSurfacePostflightAttestationSha256",
        "chromeMcpDisposition", "computerUseDisposition", "reviewerInputDisposition",
        "computerHelperStopped",
        "rawScreenshotScratchDeleted", "pendingReviewRecordsDeleted",
        "extractedExtensionInventoryVerifiedBeforeDeletion", "extractedExtensionDirectoryDeleted",
        "extensionDisposition", "unrelatedTargetMutationCommandsIssued",
        "unrelatedExtensionMutationCommandsIssued"
    )
    Assert-ExactKeys $cleanup $cleanupFields "v0.12.52 cleanup"
    foreach ($name in @(
        "controlReleased", "testTabsClosed", "testWindowClosed", "popupDisconnected", "serverStopped",
        "portReleased", "acceptanceCredentialClearedFromShell", "computerHelperStopped",
        "rawScreenshotScratchDeleted",
        "pendingReviewRecordsDeleted", "extractedExtensionInventoryVerifiedBeforeDeletion",
        "extractedExtensionDirectoryDeleted"
    )) {
        Assert-ExactBoolean $cleanup.$name $true "v0.12.52 cleanup $name"
    }
    if ([string]$cleanup.externalSurfacePreflightAttestationSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$cleanup.externalSurfacePostflightAttestationSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $cleanup.chromeMcpDisposition -cne "never-used-through-independent-review" -or
        $cleanup.computerUseDisposition -cne "not-resumed-through-independent-review" -or
        $cleanup.reviewerInputDisposition -cne "exported-digest-bound-files-only") {
        throw "External orchestration surfaces are not closed by the exact preflight/postflight attestation pair."
    }
    Assert-ExactKeys $cleanup.savedTokenClear @(
        "trustedPopupClick", "confirmationDialogShown", "confirmationAcceptedByExecutor", "approvalRef",
        "popupStateVerifiedAfterClear", "tokenConfigured", "clearButtonDisabled"
    ) "saved token cleanup"
    foreach ($name in @(
        "trustedPopupClick", "confirmationDialogShown", "confirmationAcceptedByExecutor",
        "popupStateVerifiedAfterClear", "clearButtonDisabled"
    )) {
        Assert-ExactBoolean $cleanup.savedTokenClear.$name $true "saved token cleanup $name"
    }
    Assert-ExactBoolean $cleanup.savedTokenClear.tokenConfigured $false "saved token cleanup tokenConfigured"
    if ($cleanup.savedTokenClear.approvalRef -cne $approvalSummary.approvalId) {
        throw "Saved-token cleanup is not bound to the scoped action-time approval."
    }
    if ($cleanup.extensionDisposition -cne "removed") {
        throw "The new-install-only candidate identity must be removed."
    }
    Assert-ExactBoolean $cleanup.unrelatedTargetMutationCommandsIssued $false `
        "unrelated target mutation command"
    Assert-ExactBoolean $cleanup.unrelatedExtensionMutationCommandsIssued $false `
        "unrelated extension mutation command"

    $retained = $Operator.retainedEvidence
    Assert-ExactKeys $retained @(
        "scope", "exactAllowlistVerified", "inputFileCount", "finalFileCount",
        "rawScreenshotsPresent", "pendingReviewRecordsPresent", "chromeMcpTranscriptPresent",
        "computerUseTranscriptPresent", "secretCredentialPresent",
        "filesystemLocationsOrBrowserIdsPresent", "externalToolAndPlatformLogsScope"
    ) "v0.12.52 retained evidence"
    if ($retained.scope -cne "acceptance-evidence-directory-only" -or
        $retained.externalToolAndPlatformLogsScope -cne "not-asserted") {
        throw "The v0.12.52 retained-evidence scope is invalid."
    }
    Assert-ExactBoolean $retained.exactAllowlistVerified $true "retained-evidence allowlist verification"
    Assert-IntegerRange $retained.inputFileCount 21 21 "retained-evidence input count"
    Assert-IntegerRange $retained.finalFileCount 22 22 "retained-evidence final count"
    foreach ($name in @(
        "rawScreenshotsPresent", "pendingReviewRecordsPresent", "chromeMcpTranscriptPresent",
        "computerUseTranscriptPresent", "secretCredentialPresent",
        "filesystemLocationsOrBrowserIdsPresent"
    )) {
        Assert-ExactBoolean $retained.$name $false "v0.12.52 retained evidence $name"
    }
}
function Assert-OperatorResults {
    param([object]$Operator, [string]$ExpectedVersion, [object]$ExpectedBinding, [object]$ExpectedReleaseBinding, [object]$Candidate)
    if ($ExpectedVersion -ceq "0.12.2") {
        Assert-OperatorResultsV1 $Operator $ExpectedVersion $ExpectedBinding
        return
    }
    if ($ExpectedVersion -ceq $script:OperatorV2Version) {
        Assert-OperatorResultsV2 $Operator $ExpectedVersion $ExpectedBinding $ExpectedReleaseBinding $Candidate
        return
    }
    throw "No stock-user-Chrome operator schema is registered for candidate version $ExpectedVersion."
}

function Get-PngFacts {
    param([string]$Path)
    $resolved = [IO.Path]::GetFullPath($Path)
    $item = [IO.FileInfo]::new($resolved)
    if (-not $item.Exists -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Retained screenshot must be an ordinary file."
    }
    Assert-NoReparseAncestorChain $resolved "retained screenshot"
    $stream = [IO.File]::Open(
        $resolved, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None
    )
    if ($stream.Length -lt 33 -or $stream.Length -gt 20MB) {
        $stream.Dispose()
        throw "Retained screenshot has an invalid PNG size."
    }
    $bytes = [byte[]]::new([int]$stream.Length)
    $readOffset = 0
    try {
        while ($readOffset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $readOffset, $bytes.Length - $readOffset)
            if ($read -le 0) { throw "Retained screenshot ended before its declared length." }
            $readOffset += $read
        }
        if ($stream.ReadByte() -ne -1) { throw "Retained screenshot grew during its exclusive read." }
    }
    finally { $stream.Dispose() }
    if ($bytes.Length -lt 33 -or $bytes.Length -gt 20MB) {
        throw "Retained screenshot has an invalid PNG size."
    }
    $signature = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    for ($index = 0; $index -lt $signature.Length; $index += 1) {
        if ($bytes[$index] -ne $signature[$index]) {
            throw "Retained screenshot is not a PNG."
        }
    }
    $offset = 8
    $width = 0L
    $height = 0L
    $sawImageData = $false
    $sawEnd = $false
    $chunkIndex = 0
    while ($offset -lt $bytes.Length) {
        if ($bytes.Length - $offset -lt 12) {
            throw "Retained screenshot has invalid PNG framing."
        }
        $length = ([uint32]$bytes[$offset] -shl 24) -bor
            ([uint32]$bytes[$offset + 1] -shl 16) -bor
            ([uint32]$bytes[$offset + 2] -shl 8) -bor
            [uint32]$bytes[$offset + 3]
        if ($length -gt 20MB -or ([uint64]$offset + 12 + $length) -gt $bytes.Length) {
            throw "Retained screenshot has an invalid PNG chunk length."
        }
        $type = [Text.Encoding]::ASCII.GetString($bytes, $offset + 4, 4)
        if (-not [regex]::IsMatch($type, '^[A-Za-z]{4}$')) {
            throw "Retained screenshot has an invalid PNG chunk type."
        }
        if ($chunkIndex -eq 0) {
            if ($type -cne "IHDR" -or $length -ne 13) {
                throw "Retained screenshot must begin with one canonical IHDR chunk."
            }
            $width = ([uint32]$bytes[$offset + 8] -shl 24) -bor
                ([uint32]$bytes[$offset + 9] -shl 16) -bor
                ([uint32]$bytes[$offset + 10] -shl 8) -bor
                [uint32]$bytes[$offset + 11]
            $height = ([uint32]$bytes[$offset + 12] -shl 24) -bor
                ([uint32]$bytes[$offset + 13] -shl 16) -bor
                ([uint32]$bytes[$offset + 14] -shl 8) -bor
                [uint32]$bytes[$offset + 15]
        }
        elseif ($type -ceq "IHDR") {
            throw "Retained screenshot contains a duplicate IHDR chunk."
        }
        if (@("tEXt", "zTXt", "iTXt", "eXIf", "iCCP", "tIME") -ccontains $type) {
            throw "Retained screenshot contains forbidden PNG metadata."
        }
        if ($type -ceq "IDAT") { $sawImageData = $true }
        $offset += 12 + [int]$length
        $chunkIndex += 1
        if ($type -ceq "IEND") {
            if ($length -ne 0 -or $offset -ne $bytes.Length) {
                throw "Retained screenshot has trailing data after IEND."
            }
            $sawEnd = $true
            break
        }
    }
    if (-not $sawImageData -or -not $sawEnd -or $width -le 0 -or $height -le 0) {
        throw "Retained screenshot is missing required PNG image data."
    }
    $hasher = [Security.Cryptography.SHA256]::Create()
    try {
        $digest = $hasher.ComputeHash($bytes)
        try {
            $sha256 = ([BitConverter]::ToString($digest)).Replace("-", "").ToLowerInvariant()
        }
        finally { [Array]::Clear($digest, 0, $digest.Length) }
    }
    finally { $hasher.Dispose() }
    $result = [pscustomobject]@{
        Width = [long]$width
        Height = [long]$height
        Bytes = [int64]$bytes.Length
        Sha256 = $sha256
    }
    [Array]::Clear($bytes, 0, $bytes.Length)
    return $result
}

function Assert-ScreenshotRecords {
    param([string[]]$Paths, [object]$ExpectedBinding, [object]$ExpectedReleaseBinding, [object]$Candidate, [string]$ExpectedVersion)
    $expectedScreenshots = if ($ExpectedVersion -ceq $script:OperatorV2Version) {
        $script:ExpectedScreenshotsV2
    }
    else { $script:ExpectedScreenshots }
    if ($Paths.Count -ne $expectedScreenshots.Count) {
        if ($ExpectedVersion -ceq $script:OperatorV2Version) {
            throw "Exactly six v0.12.52 screenshot sidecars are required."
        }
        throw "Exactly eleven screenshot sidecars are required."
    }
    $safe = [ordered]@{}
    foreach ($path in $Paths) {
        $recordPath = Resolve-RequiredFile $path "ScreenshotRecords entry"
        $record = Read-Json $recordPath "screenshot record"
        $screenshotFields = @(
            "schemaVersion", "evidenceType", "purpose", "candidateBinding", "image", "cropApplied",
            "metadataStrippedByDecodeAndReencode", "forbiddenMetadataChunksPresent", "ocrAvailable",
            "ocrDenylistChecked", "ocrDenylistMatches", "manualVisualReviewConfirmed",
            "automaticPixelRedactionPerformed", "unknownPixelSafetyClaimed", "reviewStatement"
        )
        if ($ExpectedVersion -ceq $script:OperatorV2Version) {
            $screenshotFields = @(
                "schemaVersion", "evidenceType", "purpose", "releaseCandidateBinding", "candidateBinding", "sourceCapture", "image", "cropApplied",
                "metadataStrippedByDecodeAndReencode", "forbiddenMetadataChunksPresent",
                "automatedTextInspectionPerformed", "independentVisualReviewRequired", "independentVisualReviewCompleted",
                "reviewRecordSha256", "reviewEntryRef",
                "automaticPixelRedactionPerformed", "unknownPixelSafetyClaimed", "reviewStatement"
            )
        }
        Assert-ExactKeys $record $screenshotFields "screenshot record"
        Assert-ExactPropertyOrder $record $screenshotFields "screenshot record"
        Assert-IntegerRange $record.schemaVersion 1 1 "screenshot schemaVersion"
        if ($record.evidenceType -cne "stock-user-chrome-screenshot" -or
            -not $expectedScreenshots.Contains($record.purpose) -or $safe.Contains($record.purpose)) {
            throw "Screenshot purpose is invalid or duplicated."
        }
        Assert-CandidateBindingDomain $record.candidateBinding $ExpectedBinding "screenshot candidateBinding"
        if ($ExpectedVersion -ceq $script:OperatorV2Version) {
            [void](Assert-ReleaseCandidateBinding $record.releaseCandidateBinding $Candidate "screenshot releaseCandidateBinding")
            if (($record.releaseCandidateBinding | ConvertTo-Json -Depth 10 -Compress) -cne
                ($ExpectedReleaseBinding | ConvertTo-Json -Depth 10 -Compress)) {
                throw "Screenshot sidecar is not bound to the exact release candidate attempt."
            }
            Assert-ExactKeys $record.sourceCapture @(
                "name", "endpoint", "bytes", "sha256", "width", "height"
            ) "screenshot source capture"
            $expectedRawName = [IO.Path]::GetFileNameWithoutExtension(
                $expectedScreenshots[$record.purpose]
            ) + ".raw.png"
            if ($record.sourceCapture.name -cne $expectedRawName -or
                $record.sourceCapture.endpoint -cne "/api/computer/screenshot") {
                throw "Screenshot source capture identity is invalid."
            }
            Assert-IntegerRange $record.sourceCapture.bytes 1001 100MB "source screenshot bytes"
            Assert-IntegerRange $record.sourceCapture.width 120 8192 "source screenshot width"
            Assert-IntegerRange $record.sourceCapture.height 32 8192 "source screenshot height"
            if ([int64]$record.sourceCapture.width * [int64]$record.sourceCapture.height -gt 50MB) {
                throw "Source screenshot dimensions exceed the pixel limit."
            }
            Assert-Hex $record.sourceCapture.sha256 64 "source screenshot SHA-256"
        }
        Assert-ExactKeys $record.image @("name", "bytes", "sha256", "width", "height") "screenshot image"
        $expectedName = $expectedScreenshots[$record.purpose]
        $expectedSidecarName = [IO.Path]::GetFileNameWithoutExtension($expectedName) + ".json"
        if ([IO.Path]::GetFileName($recordPath) -cne $expectedSidecarName -or
            $record.image.name -cne $expectedName) {
            throw "Screenshot image identity is invalid."
        }
        Assert-IntegerRange $record.image.bytes 1 20MB "screenshot image bytes"
        Assert-IntegerRange $record.image.width 120 8192 "screenshot image width"
        Assert-IntegerRange $record.image.height 32 8192 "screenshot image height"
        if ([int64]$record.image.width * [int64]$record.image.height -gt 50MB) {
            throw "Screenshot image dimensions exceed the pixel limit."
        }
        Assert-Hex $record.image.sha256 64 "screenshot SHA-256"
        $requiredTrueFields = @("cropApplied", "metadataStrippedByDecodeAndReencode")
        if ($ExpectedVersion -ceq $script:OperatorV2Version) {
            $requiredTrueFields += @("independentVisualReviewRequired", "independentVisualReviewCompleted")
        }
        else { $requiredTrueFields += "manualVisualReviewConfirmed" }
        foreach ($name in $requiredTrueFields) {
            Assert-ExactBoolean $record.$name $true "screenshot $name"
        }
        foreach ($name in @("forbiddenMetadataChunksPresent", "automaticPixelRedactionPerformed", "unknownPixelSafetyClaimed")) {
            Assert-ExactBoolean $record.$name $false "screenshot $name"
        }
        if ($ExpectedVersion -ceq $script:OperatorV2Version) {
            Assert-ExactBoolean $record.automatedTextInspectionPerformed $false "screenshot automated text inspection"
            Assert-Hex $record.reviewRecordSha256 64 "screenshot independent review record SHA-256"
            Assert-Hex $record.reviewEntryRef 64 "screenshot independent review entry reference"
        }
        else {
            Assert-IntegerRange $record.ocrDenylistMatches 0 0 "screenshot OCR denylist matches"
        }
        $expectedReviewStatement = if ($ExpectedVersion -ceq $script:OperatorV2Version) {
            $script:ReviewStatementV2
        }
        else { $script:ReviewStatement }
        if ($ExpectedVersion -ceq $script:OperatorV2Version) {
            if ($record.reviewStatement -cne $expectedReviewStatement) { throw "Screenshot review contract is invalid." }
        }
        elseif ($record.ocrAvailable -isnot [bool] -or $record.ocrDenylistChecked -isnot [bool] -or
            $record.ocrDenylistChecked -ne $record.ocrAvailable -or $record.reviewStatement -cne $expectedReviewStatement) {
            throw "Screenshot review contract is invalid."
        }
        $imagePath = Resolve-RequiredFile ([IO.Path]::Combine([IO.Path]::GetDirectoryName($recordPath), $record.image.name)) "sanitized screenshot"
        $pngFacts = Get-PngFacts $imagePath
        if ($pngFacts.Width -ne [long]$record.image.width -or $pngFacts.Height -ne [long]$record.image.height) {
            throw "Sanitized screenshot dimensions do not match their sidecar."
        }
        if ($pngFacts.Bytes -ne [int64]$record.image.bytes -or
            $pngFacts.Sha256 -cne $record.image.sha256) {
            throw "Sanitized screenshot bytes do not match their sidecar."
        }
        $safe[$record.purpose] = $record
    }
    foreach ($purpose in $expectedScreenshots.Keys) {
        if (-not $safe.Contains($purpose)) {
            throw "A required screenshot purpose is missing."
        }
    }
    $ordered = @()
    foreach ($purpose in $expectedScreenshots.Keys) {
        $ordered += $safe[$purpose]
    }
    return $ordered
}

function Read-DenyValues {
    param([string]$Path)
    if ([String]::IsNullOrWhiteSpace($Path)) { return @() }
    $resolved = Resolve-RequiredFile $Path "DenyValuesFile"
    $bytes = [IO.File]::ReadAllBytes($resolved)
    if ($bytes.Length -le 0 -or $bytes.Length -gt 65536) { throw "DenyValuesFile has an invalid size." }
    $values = @($script:Utf8NoBom.GetString($bytes) -split "`r?`n" | Where-Object { -not [String]::IsNullOrWhiteSpace($_) })
    if ($values.Count -eq 0) { throw "DenyValuesFile is empty." }
    foreach ($value in $values) {
        if ($value.Length -lt 4 -or $value.Length -gt 512) { throw "DenyValuesFile entry length is invalid." }
    }
    return $values
}

function Assert-SafeSerializedRecord {
    param([string]$Json, [string[]]$DenyValues)
    foreach ($value in $DenyValues) {
        if ($Json.IndexOf($value, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
            throw "Final evidence contains an exact denylisted value."
        }
    }
    try { $decoded = ConvertFrom-JsonPreservingStrings $Json }
    catch { throw "Final evidence is not valid JSON for decoded-string leak scanning." }
    $pending = New-Object Collections.Generic.Stack[object]
    $pending.Push($decoded)
    while ($pending.Count -gt 0) {
        $value = $pending.Pop()
        if ($null -eq $value) { continue }
        if ($value -is [string]) {
            foreach ($denyValue in $DenyValues) {
                if ($value.IndexOf($denyValue, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                    throw "Final evidence contains an exact decoded denylisted value."
                }
            }
            foreach ($decodedPattern in @(
                '(?i)(?:[a-z]:\\users\\|/users/|/home/)',
                '(?i)\b(?:authorization|bearer)\b',
                '(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b'
            )) {
                if ([regex]::IsMatch(
                    $value, $decodedPattern,
                    [Text.RegularExpressions.RegexOptions]::CultureInvariant
                )) {
                    throw "Final evidence contains a forbidden decoded string value."
                }
            }
        }
        elseif ($value -is [Collections.IDictionary]) {
            foreach ($child in $value.Values) { $pending.Push($child) }
        }
        elseif ($value -is [Collections.IEnumerable]) {
            foreach ($child in $value) { $pending.Push($child) }
        }
        elseif ($value -is [pscustomobject]) {
            foreach ($property in $value.PSObject.Properties) {
                $pending.Push($property.Value)
            }
        }
    }
    # Candidate bindings are exact, structurally validated lowercase commit or
    # SHA-256 values. Remove only quoted canonical binding atoms before the
    # generic credential/extension-ID heuristics, which would otherwise mistake
    # a legitimate digest substring for a 32-letter unpacked extension ID.
    $safeJson = [regex]::Replace(
        $Json,
        '"(?:[0-9a-f]{40}|[0-9a-f]{64})"',
        '""',
        [Text.RegularExpressions.RegexOptions]::CultureInvariant
    )
    foreach ($pattern in @(
        '(?i)\b(?:authorization|bearer)\b',
        '(?i)[a-z]:\\users\\',
        '(?i)/users/',
        '(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b',
        '(?i)(?<![a-p])[a-p]{32}(?![a-p])',
        '(?i)"(?:stdout|stderr|commandLine|userProfile|profilePath|filesystemPath)"\s*:'
    )) {
        if ([regex]::IsMatch($safeJson, $pattern, [Text.RegularExpressions.RegexOptions]::CultureInvariant)) {
            throw "Final evidence contains a forbidden raw-data pattern."
        }
    }
}

function Write-NewJson {
    param([string]$Path, [object]$Value)
    $temporary = "$Path.new"
    if ([IO.File]::Exists($temporary)) { throw "A stale temporary evidence record exists." }
    try {
        $json = $Value | ConvertTo-Json -Depth 30
        $bytes = $script:Utf8NoBom.GetBytes("$json`n")
        $stream = [IO.File]::Open(
            $temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
        )
        try {
            $stream.Write($bytes, 0, $bytes.Length)
            $stream.Flush($true)
        }
        finally {
            $stream.Dispose()
            [Array]::Clear($bytes, 0, $bytes.Length)
        }
        [IO.File]::Move($temporary, $Path)
    }
    catch {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
        throw
    }
}

function Assert-ScopedApprovalRecordV3 {
    param(
        [object]$Record,
        [object]$ExpectedBinding,
        [object]$ExpectedReleaseBinding,
        [object]$Candidate
    )
    $fields = @(
        "schemaVersion", "evidenceType", "releaseCandidateBinding", "candidateBinding",
        "approvalId", "request", "response", "consumption"
    )
    Assert-ExactKeys $Record $fields "scoped action-time approval record"
    Assert-ExactPropertyOrder $Record $fields "scoped action-time approval record"
    Assert-IntegerRange $Record.schemaVersion 1 1 "scoped action-time approval schemaVersion"
    if ($Record.evidenceType -cne "stock-user-chrome-scoped-action-approval") {
        throw "Scoped action-time approval identity is invalid."
    }
    [void](Assert-ReleaseCandidateBinding $Record.releaseCandidateBinding $Candidate `
        "scoped approval releaseCandidateBinding")
    if (($Record.releaseCandidateBinding | ConvertTo-Json -Depth 12 -Compress) -cne
        ($ExpectedReleaseBinding | ConvertTo-Json -Depth 12 -Compress)) {
        throw "Scoped approval is not bound to the exact release candidate attempt."
    }
    Assert-CandidateBindingDomain $Record.candidateBinding $ExpectedBinding `
        "scoped approval candidateBinding"
    Assert-Hex $Record.approvalId 64 "scoped approval id"

    $requestFields = @(
        "createdAtUtc", "expiresAtUtc", "challengeFrameRef", "scopeSha256", "coveredActions", "loopbackOnly",
        "dedicatedWindowOnly", "restoreCapturedState", "noUnrelatedExtensionMutation"
    )
    Assert-ExactKeys $Record.request $requestFields "scoped approval request"
    Assert-ExactPropertyOrder $Record.request $requestFields "scoped approval request"
    $createdAt = Assert-UtcTimestamp $Record.request.createdAtUtc "scoped approval request creation"
    $expiresAt = Assert-UtcTimestamp $Record.request.expiresAtUtc "scoped approval request expiry"
    if ($expiresAt -le $createdAt -or ($expiresAt - $createdAt).TotalMinutes -gt 30) {
        throw "Scoped approval request validity interval is invalid."
    }
    Assert-Hex $Record.request.challengeFrameRef 64 "scoped approval challenge frame reference"
    Assert-Hex $Record.request.scopeSha256 64 "scoped approval scope SHA-256"
    Assert-ExactArray @($Record.request.coveredActions) $script:CoveredApprovalActions `
        "scoped approval covered actions"
    foreach ($name in @(
        "loopbackOnly", "dedicatedWindowOnly", "restoreCapturedState", "noUnrelatedExtensionMutation"
    )) {
        Assert-ExactBoolean $Record.request.$name $true "scoped approval request $name"
    }

    $responseFields = @(
        "approvedBy", "deliveredBy", "orchestratorSessionRef", "confirmationMode",
        "confirmedAtUtc", "requestSha256", "singleCandidateRun"
    )
    Assert-ExactKeys $Record.response $responseFields "scoped approval response"
    Assert-ExactPropertyOrder $Record.response $responseFields "scoped approval response"
    if ($Record.response.approvedBy -cne "user" -or
        $Record.response.deliveredBy -cne "user-via-orchestrator" -or
        $Record.response.confirmationMode -cne "batched-action-time") {
        throw "Scoped approval was not granted by the user through the batched action-time checkpoint."
    }
    Assert-Hex $Record.response.orchestratorSessionRef 64 "scoped approval orchestrator session reference"
    $confirmedAt = Assert-UtcTimestamp $Record.response.confirmedAtUtc "scoped approval confirmation"
    if ($confirmedAt -lt $createdAt -or $confirmedAt -gt $expiresAt) {
        throw "Scoped approval confirmation is outside its exact request interval."
    }
    Assert-Hex $Record.response.requestSha256 64 "scoped approval request SHA-256"
    Assert-ExactBoolean $Record.response.singleCandidateRun $true "scoped approval single-candidate-run scope"

    $consumptionFields = @(
        "consumedBeforeFirstCoveredAction", "consumedBeforeExpiry", "preDispatchFrameRef",
        "preDispatchDecisionRef", "preDispatchVerifiedAtUtc", "freshStateRevalidatedAfterApproval",
        "scopeUnchangedThroughRun", "replayed",
        "cleanupAuthoritySurvivesFailure"
    )
    Assert-ExactKeys $Record.consumption $consumptionFields "scoped approval consumption"
    Assert-ExactPropertyOrder $Record.consumption $consumptionFields "scoped approval consumption"
    foreach ($name in @(
        "consumedBeforeFirstCoveredAction", "consumedBeforeExpiry",
        "freshStateRevalidatedAfterApproval", "scopeUnchangedThroughRun",
        "cleanupAuthoritySurvivesFailure"
    )) {
        Assert-ExactBoolean $Record.consumption.$name $true "scoped approval consumption $name"
    }
    foreach ($name in @("preDispatchFrameRef", "preDispatchDecisionRef")) {
        Assert-Hex $Record.consumption.$name 64 "scoped approval consumption $name"
    }
    $preDispatchVerifiedAt = Assert-UtcTimestamp `
        $Record.consumption.preDispatchVerifiedAtUtc "scoped approval pre-dispatch revalidation"
    if ($Record.request.challengeFrameRef -ceq $Record.consumption.preDispatchFrameRef -or
        $preDispatchVerifiedAt -le $confirmedAt -or $preDispatchVerifiedAt -gt $expiresAt) {
        throw "Scoped approval did not use a distinct fresh post-approval state revalidation."
    }
    Assert-ExactBoolean $Record.consumption.replayed $false "scoped approval replay state"
    return $Record
}

function Assert-IndependentReviewRecordV3 {
    param(
        [object]$Record,
        [object]$ExpectedBinding,
        [object]$ExpectedReleaseBinding,
        [object]$Candidate
    )
    $fields = @(
        "schemaVersion", "evidenceType", "releaseCandidateBinding", "candidateBinding",
        "executorSessionRef", "reviewerSessionRef", "independentSessionBoundary",
        "requestSha256", "reviewedAtUtc", "entries", "aggregate"
    )
    Assert-ExactKeys $Record $fields "independent visual review record"
    Assert-ExactPropertyOrder $Record $fields "independent visual review record"
    Assert-IntegerRange $Record.schemaVersion 1 1 "independent visual review schemaVersion"
    if ($Record.evidenceType -cne "stock-user-chrome-independent-visual-review") {
        throw "Independent visual review identity is invalid."
    }
    [void](Assert-ReleaseCandidateBinding $Record.releaseCandidateBinding $Candidate `
        "independent review releaseCandidateBinding")
    if (($Record.releaseCandidateBinding | ConvertTo-Json -Depth 12 -Compress) -cne
        ($ExpectedReleaseBinding | ConvertTo-Json -Depth 12 -Compress)) {
        throw "Independent visual review is not bound to the exact release candidate attempt."
    }
    Assert-CandidateBindingDomain $Record.candidateBinding $ExpectedBinding `
        "independent review candidateBinding"
    foreach ($name in @("executorSessionRef", "reviewerSessionRef", "requestSha256")) {
        Assert-Hex $Record.$name 64 "independent visual review $name"
    }
    if ($Record.executorSessionRef -ceq $Record.reviewerSessionRef) {
        throw "Independent visual review reused the executor session."
    }
    Assert-ExactBoolean $Record.independentSessionBoundary $true `
        "independent visual review session boundary"
    [void](Assert-UtcTimestamp $Record.reviewedAtUtc "independent visual review timestamp")

    if ($Record.entries.Count -ne $script:ExpectedScreenshotsV2.Count) {
        throw "Independent visual review must contain exactly six ordered entries."
    }
    for ($index = 0; $index -lt $Record.entries.Count; $index += 1) {
        $entry = $Record.entries[$index]
        $entryFields = @(
            "sequence", "purpose", "image", "sha256", "width", "height",
            "requiredVisibleStateSha256", "digestMatched", "requiredStateVerdict",
            "sensitivePixelsObserved", "uncertain"
        )
        Assert-ExactKeys $entry $entryFields "independent visual review entry"
        Assert-ExactPropertyOrder $entry $entryFields "independent visual review entry"
        $purpose = @($script:ExpectedScreenshotsV2.Keys)[$index]
        if ($entry.sequence -ne ($index + 1) -or $entry.purpose -cne $purpose -or
            $entry.image -cne $script:ExpectedScreenshotsV2[$purpose] -or
            $entry.requiredVisibleStateSha256 -cne (Get-TextSha256 $script:RequiredVisibleStatesV2[$purpose])) {
            throw "Independent visual review entries are missing, reordered, or bound to another criterion."
        }
        Assert-Hex $entry.sha256 64 "independent visual review image SHA-256"
        Assert-Hex $entry.requiredVisibleStateSha256 64 `
            "independent visual review visible-state SHA-256"
        Assert-IntegerRange $entry.width 120 8192 "independent visual review width"
        Assert-IntegerRange $entry.height 32 8192 "independent visual review height"
        if ([int64]$entry.width * [int64]$entry.height -gt 50MB) {
            throw "Independent visual review dimensions exceed the retained-image pixel limit."
        }
        Assert-ExactBoolean $entry.digestMatched $true "independent visual review digest match"
        if ($entry.requiredStateVerdict -cne "pass") {
            throw "Independent visual review did not confirm the required visible state."
        }
        Assert-ExactBoolean $entry.sensitivePixelsObserved $false `
            "independent visual review sensitive-pixel observation"
        Assert-ExactBoolean $entry.uncertain $false "independent visual review uncertainty"
    }

    $aggregateFields = @(
        "reviewedCropCount", "everySanitizedCropOpenedByReviewer", "allImageDigestsMatched",
        "requiredVisibleStateConfirmedByReviewer", "noSensitivePixelsObservedByReviewer",
        "noUncertaintyReported", "visualJudgmentNotPixelSafetyProof"
    )
    Assert-ExactKeys $Record.aggregate $aggregateFields "independent visual review aggregate"
    Assert-ExactPropertyOrder $Record.aggregate $aggregateFields "independent visual review aggregate"
    Assert-IntegerRange $Record.aggregate.reviewedCropCount 6 6 `
        "independent visual review crop count"
    foreach ($name in @($aggregateFields | Select-Object -Skip 1)) {
        Assert-ExactBoolean $Record.aggregate.$name $true "independent visual review aggregate $name"
    }
    return $Record
}
function Assert-ComputerHelperRecordV2 {
    param(
        [object]$Record,
        [object]$ExpectedBinding,
        [object]$ExpectedReleaseBinding,
        [object]$Candidate,
        [object]$Operator,
        [string]$ExpectedApiMatrixSha256
    )
    Assert-ExactKeys $Record @(
        "schemaVersion", "evidenceType", "version", "releaseCandidateBinding", "candidateBinding", "passed", "recordedBy",
        "run", "operatorExchange", "scopedActionApproval", "server", "helper", "extensionPayload",
        "initialState", "windowBinding", "lifecycle", "windowEpochs",
        "actions", "browserAction", "handback", "screenshots", "cleanup", "privacy"
    ) "v0.12.52 computer-helper record"
    Assert-IntegerRange $Record.schemaVersion 2 2 "computer-helper record schemaVersion"
    if ($Record.evidenceType -cne "stock-user-chrome-computer-helper-chain" -or
        $Record.version -cne $script:OperatorV2Version) {
        throw "The computer-helper record identity is invalid."
    }
    [void](Assert-ReleaseCandidateBinding $Record.releaseCandidateBinding $Candidate "computer-helper releaseCandidateBinding")
    if (($Record.releaseCandidateBinding | ConvertTo-Json -Depth 10 -Compress) -cne
        ($ExpectedReleaseBinding | ConvertTo-Json -Depth 10 -Compress)) {
        throw "The computer-helper record is not bound to the exact release candidate attempt."
    }
    Assert-ExactBoolean $Record.passed $true "computer-helper record passed"
    Assert-CandidateBindingDomain $Record.candidateBinding $ExpectedBinding "computer-helper candidateBinding"

    Assert-ExactKeys $Record.recordedBy @("name", "sha256", "source") "computer-helper recorder"
    $recorderPath = [IO.Path]::Combine($PSScriptRoot, "record-computer-helper-chain.ps1")
    if (-not [IO.File]::Exists($recorderPath) -or
        $Record.recordedBy.name -cne "record-computer-helper-chain.ps1" -or
        $Record.recordedBy.source -cne "candidate-final-sha-blob" -or
        $Record.recordedBy.sha256 -cne (Get-Sha256 $recorderPath)) {
        throw "The computer-helper record is not bound to the exact candidate recorder."
    }
    Assert-Hex $Record.recordedBy.sha256 64 "computer-helper recorder SHA-256"

    Assert-ExactKeys $Record.run @(
        "runNonce", "preflightRecordSha256", "apiMatrixRecordSha256",
        "startedAtUtc", "finishedAtUtc"
    ) "computer-helper run"
    foreach ($name in @("runNonce", "preflightRecordSha256", "apiMatrixRecordSha256")) {
        Assert-Hex $Record.run.$name 64 "computer-helper run $name"
    }
    if ($Record.run.runNonce -cne $ExpectedBinding.runNonce -or
        $Record.run.preflightRecordSha256 -cne $ExpectedBinding.preflightRecordSha256 -or
        $Record.run.apiMatrixRecordSha256 -cne $ExpectedApiMatrixSha256) {
        throw "The computer-helper record is replayed or bound to another preflight/API-matrix run."
    }
    $startedAt = Assert-UtcTimestamp $Record.run.startedAtUtc "computer-helper run start"
    $finishedAt = Assert-UtcTimestamp $Record.run.finishedAtUtc "computer-helper run finish"
    if ($finishedAt -lt $startedAt -or ($finishedAt - $startedAt).TotalHours -gt 2) {
        throw "The computer-helper run interval is invalid."
    }

    $exchange = $Record.operatorExchange
    Assert-ExactKeys $exchange @(
        "protocolVersion", "executorSessionRef", "reviewerSessionRef", "independentSessionBoundary",
        "requestCount", "statusDecisionCount", "freshFrameDecisionCount", "allRequestsCreateOnce",
        "allResponsesCreateOnce", "everyFrameDependentDecisionBoundToFreshFrame",
        "everyStatusDecisionBoundToFreshStatus", "responseChainSha256", "scratchDeleted"
    ) "computer-helper operator exchange"
    Assert-IntegerRange $exchange.protocolVersion 1 1 "operator exchange protocolVersion"
    foreach ($name in @("executorSessionRef", "reviewerSessionRef", "responseChainSha256")) {
        Assert-Hex $exchange.$name 64 "operator exchange $name"
    }
    if ($exchange.executorSessionRef -ceq $exchange.reviewerSessionRef) {
        throw "The operator exchange reused the executor session as its reviewer."
    }
    Assert-IntegerRange $exchange.requestCount 1 100000 "operator exchange request count"
    Assert-IntegerRange $exchange.statusDecisionCount 1 $exchange.requestCount `
        "operator exchange status-decision count"
    Assert-IntegerRange $exchange.freshFrameDecisionCount 1 $exchange.requestCount `
        "operator exchange fresh-frame-decision count"
    if ($exchange.requestCount -ne
        ($exchange.statusDecisionCount + $exchange.freshFrameDecisionCount + 1)) {
        throw "Operator exchange request count must equal status decisions, fresh-frame decisions, and the one scoped approval."
    }
    foreach ($name in @(
        "independentSessionBoundary", "allRequestsCreateOnce", "allResponsesCreateOnce",
        "everyFrameDependentDecisionBoundToFreshFrame", "everyStatusDecisionBoundToFreshStatus",
        "scratchDeleted"
    )) {
        Assert-ExactBoolean $exchange.$name $true "operator exchange $name"
    }

    $approval = $Record.scopedActionApproval
    Assert-ExactKeys $approval @(
        "recordSha256", "approvalId", "scopeSha256", "executorSessionRef",
        "approvalConfirmedAtUtc", "approvalExpiresAtUtc", "approvalChallengeFrameRef",
        "preDispatchFrameRef", "preDispatchDecisionRef", "preDispatchVerifiedAtUtc",
        "firstCoveredActionSequence",
        "firstCoveredActionDispatchedAtUtc", "consumedBeforeFirstCoveredAction",
        "consumedBeforeExpiry", "freshStateRevalidatedAfterApproval", "scopeUnchangedThroughRun"
    ) "computer-helper scoped action approval"
    foreach ($name in @(
        "recordSha256", "approvalId", "scopeSha256", "executorSessionRef",
        "approvalChallengeFrameRef", "preDispatchFrameRef", "preDispatchDecisionRef"
    )) {
        Assert-Hex $approval.$name 64 "computer-helper scoped approval $name"
    }
    if ($approval.executorSessionRef -cne $exchange.executorSessionRef) {
        throw "The scoped approval was obtained in a different executor session."
    }
    $approvalConfirmedAt = Assert-UtcTimestamp $approval.approvalConfirmedAtUtc `
        "computer-helper scoped approval confirmation"
    $approvalExpiresAt = Assert-UtcTimestamp $approval.approvalExpiresAtUtc `
        "computer-helper scoped approval expiry"
    $firstCoveredAt = Assert-UtcTimestamp $approval.firstCoveredActionDispatchedAtUtc `
        "computer-helper first covered action dispatch"
    $preDispatchVerifiedAt = Assert-UtcTimestamp $approval.preDispatchVerifiedAtUtc `
        "computer-helper pre-dispatch state revalidation"
    Assert-IntegerRange $approval.firstCoveredActionSequence 1 $script:ComputerHelperActionNames.Count `
        "computer-helper first covered action sequence"
    if ($approvalConfirmedAt -lt $startedAt -or $approvalExpiresAt -le $approvalConfirmedAt -or
        $preDispatchVerifiedAt -le $approvalConfirmedAt -or
        $preDispatchVerifiedAt -gt $firstCoveredAt -or
        $approval.approvalChallengeFrameRef -ceq $approval.preDispatchFrameRef -or
        $firstCoveredAt -le $approvalConfirmedAt -or $firstCoveredAt -gt $approvalExpiresAt -or
        $firstCoveredAt -gt $finishedAt) {
        throw "The scoped approval was not confirmed and unexpired at the first covered action."
    }
    foreach ($name in @(
        "consumedBeforeFirstCoveredAction", "consumedBeforeExpiry",
        "freshStateRevalidatedAfterApproval", "scopeUnchangedThroughRun"
    )) {
        Assert-ExactBoolean $approval.$name $true "computer-helper scoped approval $name"
    }

    Assert-ExactKeys $Record.server @(
        "executableName", "sha256", "processRef", "soleListener", "updateCheckDisabled"
    ) "computer-helper server identity"
    if ($Record.server.executableName -cne $Candidate.server.name -or
        $Record.server.sha256 -cne $Candidate.server.sha256 -or
        $Record.server.soleListener -cne "127.0.0.1:17373") {
        throw "The computer-helper record does not bind the exact candidate server and sole listener."
    }
    foreach ($name in @("sha256", "processRef")) {
        Assert-Hex $Record.server.$name 64 "computer-helper server $name"
    }
    Assert-ExactBoolean $Record.server.updateCheckDisabled $true "candidate server update-check suppression"

    Assert-ExactKeys $Record.helper @(
        "executableName", "sha256", "connectedThroughLoopbackServer", "serverApiOnly",
        "processRef", "sessionBindingSha256", "rawSessionIdentifierRetained", "topology"
    ) "computer-helper identity"
    if ($Record.helper.executableName -cne $Candidate.computerHelper.name -or
        $Record.helper.sha256 -cne $Candidate.computerHelper.sha256) {
        throw "The computer-helper record does not bind the exact candidate helper."
    }
    foreach ($name in @("sha256", "processRef", "sessionBindingSha256")) {
        Assert-Hex $Record.helper.$name 64 "computer-helper $name"
    }
    Assert-ExactBoolean $Record.helper.connectedThroughLoopbackServer $true "computer-helper loopback connection"
    Assert-ExactBoolean $Record.helper.serverApiOnly $true "computer-helper API-only transport"
    Assert-ExactBoolean $Record.helper.rawSessionIdentifierRetained $false "computer-helper raw session retention"
    Assert-ExactKeys $Record.helper.topology @(
        "exactImageDirectChildCount", "exactImageMatched", "directChildOfLaunchedSupervisor",
        "interactiveSessionMatched", "stableConsecutivePolls", "helloStateMatched",
        "protocolRoundTrip", "roundTripMethod", "supervisorProcessRef", "workerProcessRef"
    ) "computer-helper topology"
    Assert-IntegerRange $Record.helper.topology.exactImageDirectChildCount 1 1 `
        "computer-helper exact-image direct-child count"
    Assert-IntegerRange $Record.helper.topology.stableConsecutivePolls 2 1000 `
        "computer-helper stable topology polls"
    foreach ($name in @(
        "exactImageMatched", "directChildOfLaunchedSupervisor", "interactiveSessionMatched",
        "helloStateMatched", "protocolRoundTrip"
    )) {
        Assert-ExactBoolean $Record.helper.topology.$name $true "computer-helper topology $name"
    }
    if ($Record.helper.topology.roundTripMethod -cne "computer.status") {
        throw "The exact helper worker was not protocol-bound through computer.status."
    }
    foreach ($name in @("supervisorProcessRef", "workerProcessRef")) {
        Assert-Hex $Record.helper.topology.$name 64 "computer-helper topology $name"
    }

    Assert-ExactKeys $Record.extensionPayload @(
        "fileCount", "combinedPayloadSha256", "verifiedBeforeLoad",
        "verifiedAfterLoad", "verifiedAfterCleanup"
    ) "computer-helper extension payload"
    Assert-IntegerRange $Record.extensionPayload.fileCount 11 11 "computer-helper extension payload file count"
    if ($Record.extensionPayload.combinedPayloadSha256 -cne $ExpectedBinding.extractedPayloadSha256) {
        throw "The computer-helper loaded a payload other than the exact preflight candidate."
    }
    Assert-Hex $Record.extensionPayload.combinedPayloadSha256 64 "computer-helper extension payload digest"
    foreach ($name in @("verifiedBeforeLoad", "verifiedAfterLoad", "verifiedAfterCleanup")) {
        Assert-ExactBoolean $Record.extensionPayload.$name $true "computer-helper extension payload $name"
    }

    if ($Record.lifecycle.Count -ne $script:ComputerHelperLifecycleEvents.Count) {
        throw "The computer-helper record must contain the exact lifecycle sequence."
    }
    $expectedConnections = @(
        "disconnected", "disconnected", "disconnected", "connected",
        "connected", "connected", "disconnected", "disconnected"
    )
    $lastLifecycleTime = $startedAt
    for ($index = 0; $index -lt $Record.lifecycle.Count; $index += 1) {
        $event = $Record.lifecycle[$index]
        Assert-ExactKeys $event @(
            "sequence", "name", "atUtc", "source", "connectionState", "processRef",
            "executableSha256", "exitCode", "resultVerified"
        ) "computer-helper lifecycle event"
        Assert-IntegerRange $event.sequence ($index + 1) ($index + 1) "computer-helper lifecycle sequence"
        if ($event.name -cne $script:ComputerHelperLifecycleEvents[$index] -or
            $event.source -cne $script:ComputerHelperActionSource -or
            $event.connectionState -cne $expectedConnections[$index]) {
            throw "The computer-helper lifecycle identity or connection transition is invalid."
        }
        $eventTime = Assert-UtcTimestamp $event.atUtc "computer-helper lifecycle time"
        if ($eventTime -lt $lastLifecycleTime -or $eventTime -gt $finishedAt) {
            throw "The computer-helper lifecycle timestamps are not ordered within the run."
        }
        $lastLifecycleTime = $eventTime
        Assert-ExactBoolean $event.resultVerified $true "computer-helper lifecycle result"
        if (@(0, 7) -ccontains $index) {
            if ($event.processRef -cne $Record.server.processRef -or
                $event.executableSha256 -cne $Candidate.server.sha256) {
                throw "Server lifecycle identity differs from the candidate server."
            }
        }
        elseif (@(2, 4, 5) -ccontains $index) {
            if ($event.processRef -cne $Record.helper.processRef -or
                $event.executableSha256 -cne $Candidate.computerHelper.sha256) {
                throw "Helper lifecycle identity differs from the candidate helper."
            }
        }
        elseif ($event.processRef -cne "not-applicable" -or
            $event.executableSha256 -cne "not-applicable") {
            throw "A connection-only lifecycle event retained a process identifier."
        }
        if ($index -eq 5) {
            if (($event.exitCode -isnot [int] -and $event.exitCode -isnot [long]) -or
                [int64]$event.exitCode -eq 0) {
                throw "The owner-forced helper termination must not claim a graceful zero exit."
            }
        }
        elseif ($index -eq 7) {
            if ($event.exitCode -isnot [int] -and $event.exitCode -isnot [long]) {
                throw "The forced server termination exit code must be an integer."
            }
        }
        else {
            Assert-IntegerRange $event.exitCode -1 -1 "computer-helper lifecycle non-exit sentinel"
        }
    }

    if ($Record.windowEpochs.Count -ne $script:ComputerHelperEpochNames.Count) {
        throw "The computer-helper record must contain the exact thirteen window/share epochs."
    }
    $epochRefs = @{}
    $shareRefs = @{}
    for ($index = 0; $index -lt $Record.windowEpochs.Count; $index += 1) {
        $epoch = $Record.windowEpochs[$index]
        Assert-ExactKeys $epoch @(
            "sequence", "name", "surface", "application", "processRef", "epochRef", "targetRef", "shareRef",
            "selectedExactly", "shareStartSequence", "shareStartSucceeded", "firstFreshFrameRef",
            "lastFreshFrameRef", "freshObservationCount", "shareStopSequence",
            "shareStopSucceeded", "rawIdentifiersRetained"
        ) "computer-helper window epoch"
        Assert-IntegerRange $epoch.sequence ($index + 1) ($index + 1) "computer-helper epoch sequence"
        if ($epoch.name -cne $script:ComputerHelperEpochNames[$index] -or
            $epoch.surface -cne $script:ComputerHelperEpochSurfaces[$index] -or
            $epoch.application -cne "google-chrome") {
            throw "The computer-helper window epoch order or surface is invalid."
        }
        foreach ($name in @("processRef", "epochRef", "targetRef", "shareRef", "firstFreshFrameRef", "lastFreshFrameRef")) {
            Assert-Hex $epoch.$name 64 "computer-helper epoch $name"
        }
        if ($epochRefs.ContainsKey($epoch.epochRef) -or $shareRefs.ContainsKey($epoch.shareRef)) {
            throw "Computer-helper epoch or share references must be unique per run."
        }
        $epochRefs[$epoch.epochRef] = $epoch
        $shareRefs[$epoch.shareRef] = $true
        foreach ($name in @("selectedExactly", "shareStartSucceeded", "shareStopSucceeded")) {
            Assert-ExactBoolean $epoch.$name $true "computer-helper $($epoch.name) $name"
        }
        Assert-IntegerRange $epoch.freshObservationCount 2 10000 "computer-helper epoch observation count"
        Assert-IntegerRange $epoch.shareStartSequence 1 1000000 "computer-helper share-start sequence"
        Assert-IntegerRange $epoch.shareStopSequence 2 1000000 "computer-helper share-stop sequence"
        if ($epoch.shareStopSequence -le $epoch.shareStartSequence -or
            $epoch.firstFreshFrameRef -ceq $epoch.lastFreshFrameRef) {
            throw "The computer-helper epoch does not prove an open-to-fresh-to-closed share interval."
        }
        Assert-ExactBoolean $epoch.rawIdentifiersRetained $false "computer-helper raw identifier retention"
    }

    $dedicatedEpochIndexes = @(1, 3, 5, 7, 9, 11, 12)
    $dedicatedTargetRef = $Record.windowEpochs[1].targetRef
    $dedicatedProcessRef = $Record.windowEpochs[1].processRef
    foreach ($index in $dedicatedEpochIndexes) {
        if ($Record.windowEpochs[$index].targetRef -cne $dedicatedTargetRef -or
            $Record.windowEpochs[$index].processRef -cne $dedicatedProcessRef) {
            throw "Dedicated Chrome epochs must bind the same exact native window and process."
        }
    }
    foreach ($index in @(2, 4, 6, 8, 10)) {
        if ($Record.windowEpochs[$index].processRef -cne $dedicatedProcessRef) {
            throw "Native picker and extension-popup epochs must be process-linked to the dedicated Chrome window."
        }
    }
    if ($Record.windowEpochs[0].targetRef -ceq $dedicatedTargetRef -or
        $Record.windowEpochs[2].targetRef -ceq $dedicatedTargetRef) {
        throw "Bootstrap, picker, and helper-created dedicated Chrome targets must remain distinct."
    }
    Assert-ExactKeys $Record.windowBinding @(
        "application", "baselineChromeWindowCount", "baselineTargetSetSha256",
        "dedicatedTargetRef", "dedicatedProcessRef", "dedicatedAbsentBeforeCreation",
        "dedicatedCreatedAsOnlyNewChromeWindow", "sameDedicatedTargetAcrossEpochs"
    ) "computer-helper dedicated-window binding"
    if ($Record.windowBinding.application -cne "google-chrome" -or
        $Record.windowBinding.dedicatedTargetRef -cne $dedicatedTargetRef -or
        $Record.windowBinding.dedicatedProcessRef -cne $dedicatedProcessRef) {
        throw "The dedicated-window summary differs from the exact normalized Chrome epochs."
    }
    Assert-IntegerRange $Record.windowBinding.baselineChromeWindowCount 1 1000 `
        "computer-helper baseline Chrome window count"
    Assert-Hex $Record.windowBinding.baselineTargetSetSha256 64 `
        "computer-helper baseline Chrome target-set digest"
    Assert-ExactBoolean $Record.windowBinding.dedicatedAbsentBeforeCreation $true `
        "computer-helper dedicated target absence before creation"
    Assert-ExactBoolean $Record.windowBinding.dedicatedCreatedAsOnlyNewChromeWindow $true `
        "computer-helper sole-new dedicated target creation"
    Assert-ExactBoolean $Record.windowBinding.sameDedicatedTargetAcrossEpochs $true `
        "computer-helper dedicated target equality"

    Assert-ExactKeys $Record.initialState @(
        "capturedFromFreshHelperFrames", "developerMode", "fullAccess", "savedToken", "candidateCard"
    ) "computer-helper live initial state"
    Assert-ExactBoolean $Record.initialState.capturedFromFreshHelperFrames $true `
        "computer-helper fresh-frame initial-state capture"
    foreach ($entry in @(
        @("developerMode", 1, $Operator.initialState.developerMode.value),
        @("fullAccess", 4, $Operator.initialState.fullAccess.value)
    )) {
        $name = [string]$entry[0]
        $epochIndex = [int]$entry[1]
        $state = $Record.initialState.$name
        Assert-ExactKeys $state @("value", "epochRef", "frameRef", "operatorDecisionRef", "capturedBeforeMutation") `
            "computer-helper live $name state"
        Assert-ToggleValueV2 $state.value "computer-helper live $name value"
        if ($state.value -cne [string]$entry[2] -or
            $state.epochRef -cne $Record.windowEpochs[$epochIndex].epochRef) {
            throw "The operator and live helper initial $name state do not match the exact fresh-frame epoch."
        }
        Assert-Hex $state.frameRef 64 "computer-helper live $name frame"
        Assert-Hex $state.operatorDecisionRef 64 "computer-helper live $name operator decision"
        Assert-ExactBoolean $state.capturedBeforeMutation $true "computer-helper live $name timing"
    }
    Assert-ExactKeys $Record.initialState.savedToken @(
        "configured", "epochRef", "frameRef", "operatorDecisionRef", "capturedBeforeMutation"
    ) "computer-helper live saved-token state"
    Assert-ExactBoolean $Record.initialState.savedToken.configured $false `
        "computer-helper live saved-token configured state"
    if ($Record.initialState.savedToken.epochRef -cne $Record.windowEpochs[4].epochRef -or
        $Operator.initialState.savedTokenConfigured -ne $Record.initialState.savedToken.configured) {
        throw "The operator and live helper initial saved-token state do not match the popup frame."
    }
    Assert-Hex $Record.initialState.savedToken.frameRef 64 "computer-helper live saved-token frame"
    Assert-Hex $Record.initialState.savedToken.operatorDecisionRef 64 `
        "computer-helper live saved-token operator decision"
    Assert-ExactBoolean $Record.initialState.savedToken.capturedBeforeMutation $true `
        "computer-helper live saved-token timing"
    if ($Record.initialState.fullAccess.frameRef -ceq $Record.initialState.savedToken.frameRef) {
        throw "Full Access and saved-token initial-state receipts must use independently fresh popup frames."
    }
    Assert-ExactKeys $Record.initialState.candidateCard @(
        "present", "epochRef", "frameRef", "operatorDecisionRef", "verifiedFromFreshLiveUi"
    ) "computer-helper initial candidate card"
    Assert-ExactBoolean $Record.initialState.candidateCard.present $false `
        "computer-helper initial candidate-card absence"
    Assert-ExactBoolean $Record.initialState.candidateCard.verifiedFromFreshLiveUi $true `
        "computer-helper initial candidate-card fresh live-UI verification"
    Assert-Hex $Record.initialState.candidateCard.frameRef 64 `
        "computer-helper initial candidate-card frame"
    Assert-Hex $Record.initialState.candidateCard.operatorDecisionRef 64 `
        "computer-helper initial candidate-card operator decision"
    if ($Record.initialState.candidateCard.epochRef -cne $Record.windowEpochs[1].epochRef -or
        $Operator.initialState.candidateExtensionPresent -ne $Record.initialState.candidateCard.present) {
        throw "The operator and live helper initial candidate-card state do not match the exact fresh-frame epoch."
    }

    $allowedMethods = @("computer.observe", "computer.click", "computer.typeText", "computer.key", "computer.status")
    if ($Record.actions.Count -ne $script:ComputerHelperActionNames.Count) {
        throw "The computer-helper record must contain the exact action sequence."
    }
    $lastActionTime = $startedAt
    $lastDispatchTime = $startedAt
    $decisionRefs = @{}
    $observedFirstCoveredSequence = 0
    for ($index = 0; $index -lt $Record.actions.Count; $index += 1) {
        $action = $Record.actions[$index]
        Assert-ExactKeys $action @(
            "sequence", "name", "atUtc", "source", "epochRef", "methods", "preFrameRef",
            "postFrameRef", "normalizedParamsSha256", "responseSha256", "httpStatus",
            "resultVerified", "postconditionVerified", "dispatchedAtUtc", "operatorDecisionRef",
            "riskRef", "approvalRef"
        ) "computer-helper action"
        Assert-IntegerRange $action.sequence ($index + 1) ($index + 1) "computer-helper action sequence"
        $expectedEpoch = $Record.windowEpochs[$script:ComputerHelperActionEpochIndexes[$index]]
        $expectedFinalMethod = if (@(4, ($script:ComputerHelperActionNames.Count - 1)) -ccontains $index) {
            "computer.status"
        }
        else { "computer.observe" }
        if ($action.name -cne $script:ComputerHelperActionNames[$index] -or
            $action.source -cne $script:ComputerHelperActionSource -or
            $action.epochRef -cne $expectedEpoch.epochRef -or
            $action.methods.Count -lt 3 -or
            $action.methods[0] -cne "computer.observe" -or
            $action.methods[$action.methods.Count - 1] -cne $expectedFinalMethod -or
            @($action.methods | Where-Object {
                $_ -isnot [string] -or $allowedMethods -cnotcontains $_
            }).Count -ne 0) {
            throw "The computer-helper action sequence, epoch, source, or method envelope is invalid."
        }
        $requiredMutator = $script:ComputerHelperActionMutators[$index]
        if ($requiredMutator -ceq "conditional-developer-mode") {
            $requiredMutator = if ($Operator.initialState.developerMode.changedByRun) {
                "computer.click"
            }
            else { "none" }
        }
        elseif ($requiredMutator -ceq "conditional-full-access") {
            $requiredMutator = if ($Operator.initialState.fullAccess.changedByRun) {
                "computer.click"
            }
            else { "none" }
        }
        $actualMutators = @($action.methods | Where-Object {
            $_ -cne "computer.observe" -and $_ -cne "computer.status"
        })
        if ($requiredMutator -ceq "none") {
            if ($actualMutators.Count -ne 0) {
                throw "An observation-only computer-helper checkpoint performed an unexpected mutation."
            }
        }
        elseif ($actualMutators -cnotcontains $requiredMutator) {
            throw "The computer-helper action omits its required native-input mutator."
        }
        if ($index -eq 4 -and
            ($actualMutators -cnotcontains "computer.typeText" -or
             $actualMutators -cnotcontains "computer.click")) {
            throw "The native picker action must type the exact directory and invoke Select Folder."
        }
        if ($index -eq 24 -and
            @($actualMutators | Where-Object { $_ -ceq "computer.click" }).Count -lt 3) {
            throw "Cleanup must switch to chrome://extensions before the exact card removal and confirmation."
        }
        foreach ($name in @(
            "preFrameRef", "postFrameRef", "normalizedParamsSha256", "responseSha256", "operatorDecisionRef"
        )) {
            Assert-Hex $action.$name 64 "computer-helper action $name"
        }
        if ($decisionRefs.ContainsKey($action.operatorDecisionRef)) {
            throw "A computer-helper action replayed an earlier operator decision."
        }
        $decisionRefs[$action.operatorDecisionRef] = $true
        if ($action.preFrameRef -ceq $action.postFrameRef) {
            throw "A computer-helper action reused a stale pre-action frame."
        }
        $actionTime = Assert-UtcTimestamp $action.atUtc "computer-helper action time"
        if ($actionTime -lt $lastActionTime -or $actionTime -gt $finishedAt) {
            throw "Computer-helper action timestamps are not ordered within the run."
        }
        $lastActionTime = $actionTime
        $dispatchTime = Assert-UtcTimestamp $action.dispatchedAtUtc "computer-helper action dispatch time"
        if ($dispatchTime -lt $lastDispatchTime -or $dispatchTime -lt $startedAt -or
            $dispatchTime -gt $actionTime) {
            throw "Computer-helper action dispatch timestamps are reordered or outside the action interval."
        }
        $lastDispatchTime = $dispatchTime
        Assert-IntegerRange $action.httpStatus 200 200 "computer-helper action HTTP status"
        Assert-ExactBoolean $action.resultVerified $true "computer-helper action result"
        Assert-ExactBoolean $action.postconditionVerified $true "computer-helper action postcondition"
        $expectedRisk = $script:ComputerHelperActionRiskRefs[$index]
        if ($expectedRisk -ceq "conditional-developer-mode") {
            $expectedRisk = if ($Operator.initialState.developerMode.changedByRun) {
                "developerModeChange"
            }
            else { "none" }
        }
        if ($action.riskRef -cne $expectedRisk) {
            throw "The computer-helper action does not reference its exact risk checkpoint."
        }
        if ($expectedRisk -ceq "none") {
            if ($action.approvalRef -cne "none") {
                throw "A non-covered action must not claim scoped approval consumption."
            }
        }
        else {
            if ($action.approvalRef -cne $approval.approvalId -or $dispatchTime -le $approvalConfirmedAt) {
                throw "A covered action is missing the exact pre-dispatch scoped approval."
            }
            if ($observedFirstCoveredSequence -eq 0) {
                $observedFirstCoveredSequence = $index + 1
                if ($approval.firstCoveredActionSequence -ne ($index + 1) -or
                    $approval.firstCoveredActionDispatchedAtUtc -cne $action.dispatchedAtUtc) {
                    throw "The first covered action summary does not match the canonical action chain."
                }
            }
        }
    }
    if ($observedFirstCoveredSequence -eq 0) {
        throw "The helper action chain never consumed the scoped action-time approval."
    }

    Assert-ExactKeys $Record.browserAction @(
        "source", "apiMatrixRecordSha256", "targetBindingSha256", "methodSequence",
        "requestResponseSha256", "resultText", "resultVerified"
    ) "computer-helper browser action"
    if ($Record.browserAction.source -cne "local-browser-bridge-api" -or
        $Record.browserAction.apiMatrixRecordSha256 -cne $ExpectedApiMatrixSha256 -or
        ($Record.browserAction.methodSequence | ConvertTo-Json -Compress) -cne
            ($script:ComputerHelperBrowserMethods | ConvertTo-Json -Compress) -or
        $Record.browserAction.resultText -cne "Hello, Bridge Matrix. blue selected.") {
        throw "The deterministic browser API action is missing, reordered, or cross-run."
    }
    foreach ($name in @("apiMatrixRecordSha256", "targetBindingSha256", "requestResponseSha256")) {
        Assert-Hex $Record.browserAction.$name 64 "computer-helper browser action $name"
    }
    Assert-ExactBoolean $Record.browserAction.resultVerified $true "deterministic browser API result"

    Assert-ExactKeys $Record.handback @("stop", "cancel") "computer-helper handback matrix"
    Assert-ExactPropertyOrder $Record.handback @("stop", "cancel") "computer-helper handback matrix"
    Assert-HandbackCaseV2 $Record.handback.stop "Stop" "in-page-stop" "released_by_user" $true
    Assert-HandbackCaseV2 $Record.handback.cancel "Cancel" "chrome-native-cancel" "canceled_by_user" $true
    if (($Record.handback | ConvertTo-Json -Depth 12 -Compress) -cne
        ($Operator.handback | ConvertTo-Json -Depth 12 -Compress)) {
        throw "Operator handback assertions do not match the live computer-helper/API record."
    }

    if ($Record.screenshots.Count -ne $script:ExpectedScreenshotsV2.Count) {
        throw "The computer-helper record must bind exactly six raw exact-window screenshots."
    }
    for ($index = 0; $index -lt $script:ExpectedScreenshotsV2.Count; $index += 1) {
        $purpose = @($script:ExpectedScreenshotsV2.Keys)[$index]
        $shot = $Record.screenshots[$index]
        Assert-ExactKeys $shot @(
            "sequence", "purpose", "source", "epochRef", "shareRef", "frameRef", "rawImage",
            "endpoint", "bytes", "sha256", "width", "height", "exactWindowFrame",
            "shareFrameFresh", "rawImageRetained"
        ) "computer-helper screenshot"
        $expectedEpoch = $Record.windowEpochs[$script:ComputerHelperScreenshotEpochIndexes[$index]]
        $rawName = [IO.Path]::GetFileNameWithoutExtension(
            $script:ExpectedScreenshotsV2[$purpose]
        ) + ".raw.png"
        Assert-IntegerRange $shot.sequence ($index + 1) ($index + 1) "computer-helper screenshot sequence"
        if ($shot.purpose -cne $purpose -or
            $shot.source -cne $script:ComputerHelperActionSource -or
            $shot.epochRef -cne $expectedEpoch.epochRef -or
            $shot.shareRef -cne $expectedEpoch.shareRef -or
            $shot.rawImage -cne $rawName -or
            $shot.endpoint -cne "/api/computer/screenshot") {
            throw "The computer-helper screenshot identity, source, or share epoch is invalid."
        }
        foreach ($name in @("frameRef", "sha256")) {
            Assert-Hex $shot.$name 64 "computer-helper screenshot $name"
        }
        Assert-IntegerRange $shot.bytes 1001 100MB "computer-helper screenshot bytes"
        Assert-IntegerRange $shot.width 120 8192 "computer-helper screenshot width"
        Assert-IntegerRange $shot.height 32 8192 "computer-helper screenshot height"
        if ([int64]$shot.width * [int64]$shot.height -gt 50MB) {
            throw "The computer-helper screenshot dimensions exceed the pixel limit."
        }
        Assert-ExactBoolean $shot.exactWindowFrame $true "computer-helper exact-window screenshot"
        Assert-ExactBoolean $shot.shareFrameFresh $true "computer-helper fresh share frame"
        Assert-ExactBoolean $shot.rawImageRetained $false "computer-helper raw screenshot retention"
    }

    Assert-ExactKeys $Record.cleanup @(
        "allSharesStopped", "helperTerminationDisposition", "helperExitCode",
        "helperDisconnectedAfterTermination", "helperChildrenRemaining", "helperListenersRemaining",
        "exactHelperImageProcessesRemaining", "serverTerminationDisposition", "serverExitCode",
        "serverListenersRemaining", "canonicalPortListenersRemaining", "candidateExtensionRemoved",
        "candidateCardAbsentAfterRemoval", "candidateCardAbsenceVerifiedFromFreshLiveUi",
        "savedTokenCleared", "developerModeRestored",
        "fullAccessRestored", "testWindowClosed", "helperExecutableUnchanged",
        "serverExecutableUnchanged", "extensionPayloadUnchanged", "rawIdentifiersCleared"
    ) "computer-helper cleanup"
    foreach ($name in @(
        "allSharesStopped", "helperDisconnectedAfterTermination", "candidateExtensionRemoved",
        "candidateCardAbsentAfterRemoval", "candidateCardAbsenceVerifiedFromFreshLiveUi",
        "savedTokenCleared", "developerModeRestored", "fullAccessRestored", "testWindowClosed",
        "helperExecutableUnchanged", "serverExecutableUnchanged", "extensionPayloadUnchanged",
        "rawIdentifiersCleared"
    )) {
        Assert-ExactBoolean $Record.cleanup.$name $true "computer-helper cleanup $name"
    }
    if ($Record.cleanup.helperTerminationDisposition -cne "owner-forced-exact-supervisor" -or
        $Record.cleanup.serverTerminationDisposition -cne "owner-forced-exact-process") {
        throw "Computer-helper cleanup did not use the exact ownership-bounded forced dispositions."
    }
    if (($Record.cleanup.helperExitCode -isnot [int] -and
        $Record.cleanup.helperExitCode -isnot [long]) -or
        [int64]$Record.cleanup.helperExitCode -eq 0) {
        throw "Computer-helper cleanup falsely claims a graceful or invalid helper exit."
    }
    if ($Record.cleanup.serverExitCode -isnot [int] -and
        $Record.cleanup.serverExitCode -isnot [long]) {
        throw "Computer-helper cleanup server exit code must be an integer."
    }
    foreach ($name in @(
        "helperChildrenRemaining", "helperListenersRemaining", "exactHelperImageProcessesRemaining",
        "serverListenersRemaining", "canonicalPortListenersRemaining"
    )) {
        Assert-IntegerRange $Record.cleanup.$name 0 0 "computer-helper cleanup $name"
    }

    Assert-ExactKeys $Record.privacy @(
        "rawWindowIdsRetained", "rawFrameIdsRetained", "rawShareIdsRetained",
        "rawSessionIdsRetained", "rawTabIdsRetained", "credentialRetained",
        "apiResponseBodiesRetained", "opaqueReferenceMapDiscarded"
    ) "computer-helper privacy"
    foreach ($name in @(
        "rawWindowIdsRetained", "rawFrameIdsRetained", "rawShareIdsRetained",
        "rawSessionIdsRetained", "rawTabIdsRetained", "credentialRetained",
        "apiResponseBodiesRetained"
    )) {
        Assert-ExactBoolean $Record.privacy.$name $false "computer-helper privacy $name"
    }
    Assert-ExactBoolean $Record.privacy.opaqueReferenceMapDiscarded $true "computer-helper opaque map disposal"

    if ($Operator.computerHelperChain.rawScreenshotCount -ne $Record.screenshots.Count -or
        $Operator.computerHelperChain.ownerForcedSupervisorTermination -ne
            ($Record.cleanup.helperTerminationDisposition -ceq "owner-forced-exact-supervisor") -or
        $Operator.computerHelperChain.helperDisconnectedAfterTermination -ne
            $Record.cleanup.helperDisconnectedAfterTermination -or
        $Operator.computerHelperChain.topologyProtocolBound -ne
            $Record.helper.topology.protocolRoundTrip -or
        $Operator.computerHelperChain.noHelperChildrenOrListenersRemain -ne
            (($Record.cleanup.helperChildrenRemaining + $Record.cleanup.helperListenersRemaining +
              $Record.cleanup.exactHelperImageProcessesRemaining + $Record.cleanup.canonicalPortListenersRemaining) -eq 0)) {
        throw "Operator helper assertions do not match the machine helper record."
    }
}

function Assert-HelperScreenshotsMatchSidecarsV2 {
    param([object]$HelperRecord, [object[]]$Sidecars)
    if ($HelperRecord.screenshots.Count -ne $Sidecars.Count) {
        throw "Computer-helper screenshots do not match the finalized sidecar count."
    }
    for ($index = 0; $index -lt $Sidecars.Count; $index += 1) {
        $helperShot = $HelperRecord.screenshots[$index]
        $sidecar = $Sidecars[$index]
        if ($helperShot.purpose -cne $sidecar.purpose -or
            $helperShot.rawImage -cne $sidecar.sourceCapture.name -or
            $helperShot.endpoint -cne $sidecar.sourceCapture.endpoint -or
            [int64]$helperShot.bytes -ne [int64]$sidecar.sourceCapture.bytes -or
            $helperShot.sha256 -cne $sidecar.sourceCapture.sha256 -or
            [int64]$helperShot.width -ne [int64]$sidecar.sourceCapture.width -or
            [int64]$helperShot.height -ne [int64]$sidecar.sourceCapture.height) {
            throw "A sanitized screenshot sidecar does not bind its exact helper-captured raw PNG."
        }
    }
}

function Assert-ApprovalMatchesHelperV3 {
    param(
        [object]$ApprovalRecord,
        [string]$ApprovalRecordSha256,
        [object]$HelperRecord,
        [object]$Operator
    )
    $helperApproval = $HelperRecord.scopedActionApproval
    if ($helperApproval.recordSha256 -cne $ApprovalRecordSha256 -or
        $helperApproval.approvalId -cne $ApprovalRecord.approvalId -or
        $helperApproval.scopeSha256 -cne $ApprovalRecord.request.scopeSha256 -or
        $helperApproval.executorSessionRef -cne $HelperRecord.operatorExchange.executorSessionRef -or
        $helperApproval.approvalConfirmedAtUtc -cne $ApprovalRecord.response.confirmedAtUtc -or
        $helperApproval.approvalExpiresAtUtc -cne $ApprovalRecord.request.expiresAtUtc -or
        $helperApproval.approvalChallengeFrameRef -cne $ApprovalRecord.request.challengeFrameRef -or
        $helperApproval.preDispatchFrameRef -cne $ApprovalRecord.consumption.preDispatchFrameRef -or
        $helperApproval.preDispatchDecisionRef -cne $ApprovalRecord.consumption.preDispatchDecisionRef -or
        $helperApproval.preDispatchVerifiedAtUtc -cne $ApprovalRecord.consumption.preDispatchVerifiedAtUtc -or
        $helperApproval.consumedBeforeFirstCoveredAction -ne $ApprovalRecord.consumption.consumedBeforeFirstCoveredAction -or
        $helperApproval.consumedBeforeExpiry -ne $ApprovalRecord.consumption.consumedBeforeExpiry -or
        $helperApproval.freshStateRevalidatedAfterApproval -ne
            $ApprovalRecord.consumption.freshStateRevalidatedAfterApproval -or
        $helperApproval.scopeUnchangedThroughRun -ne $ApprovalRecord.consumption.scopeUnchangedThroughRun) {
        throw "Computer-helper approval facts do not match the exact retained scoped approval record."
    }
    if ($ApprovalRecord.response.orchestratorSessionRef -ceq
            $HelperRecord.operatorExchange.executorSessionRef -or
        $ApprovalRecord.response.orchestratorSessionRef -ceq
            $HelperRecord.operatorExchange.reviewerSessionRef) {
        throw "Scoped approval must use a user-facing orchestrator session distinct from both executor and reviewer."
    }
    if ($Operator.consentCheckpoints.scopedActionTimeApproval.recordSha256 -cne $ApprovalRecordSha256 -or
        $Operator.consentCheckpoints.scopedActionTimeApproval.approvalId -cne $ApprovalRecord.approvalId -or
        $Operator.consentCheckpoints.scopedActionTimeApproval.consumedBeforeExpiry -ne
            $ApprovalRecord.consumption.consumedBeforeExpiry -or
        $Operator.consentCheckpoints.scopedActionTimeApproval.freshStateRevalidatedAfterApproval -ne
            $ApprovalRecord.consumption.freshStateRevalidatedAfterApproval -or
        $Operator.consentCheckpoints.scopedActionTimeApproval.singleCandidateRun -ne
            $ApprovalRecord.response.singleCandidateRun) {
        throw "Operator approval summary does not match the exact retained scoped approval record."
    }
}

function Assert-ReviewMatchesHelperAndSidecarsV3 {
    param(
        [object]$ReviewRecord,
        [string]$ReviewRecordSha256,
        [object]$HelperRecord,
        [object]$Operator,
        [object[]]$Sidecars
    )
    if ($ReviewRecord.executorSessionRef -cne $HelperRecord.operatorExchange.executorSessionRef -or
        $ReviewRecord.reviewerSessionRef -cne $HelperRecord.operatorExchange.reviewerSessionRef -or
        $ReviewRecord.independentSessionBoundary -ne $HelperRecord.operatorExchange.independentSessionBoundary) {
        throw "Independent review sessions do not match the exact helper operator exchange."
    }
    $helperFinishedAt = Assert-UtcTimestamp $HelperRecord.run.finishedAtUtc "computer-helper finish time"
    $reviewedAt = Assert-UtcTimestamp $ReviewRecord.reviewedAtUtc "independent review time"
    if ($reviewedAt -lt $helperFinishedAt) {
        throw "Independent review predates completion of the exact helper capture chain."
    }
    if ($Operator.independentVisualReview.recordSha256 -cne $ReviewRecordSha256 -or
        $Operator.independentVisualReview.executorSessionRef -cne $ReviewRecord.executorSessionRef -or
        $Operator.independentVisualReview.reviewerSessionRef -cne $ReviewRecord.reviewerSessionRef -or
        $Operator.independentVisualReview.independentSessionBoundary -ne $ReviewRecord.independentSessionBoundary -or
        ($Operator.independentVisualReview | ConvertTo-Json -Depth 8 -Compress) -cne
            (([ordered]@{
                recordSha256 = $ReviewRecordSha256
                executorSessionRef = $ReviewRecord.executorSessionRef
                reviewerSessionRef = $ReviewRecord.reviewerSessionRef
                independentSessionBoundary = $ReviewRecord.independentSessionBoundary
                reviewedCropCount = $ReviewRecord.aggregate.reviewedCropCount
                everySanitizedCropOpenedByReviewer = $ReviewRecord.aggregate.everySanitizedCropOpenedByReviewer
                allImageDigestsMatched = $ReviewRecord.aggregate.allImageDigestsMatched
                requiredVisibleStateConfirmedByReviewer = $ReviewRecord.aggregate.requiredVisibleStateConfirmedByReviewer
                noSensitivePixelsObservedByReviewer = $ReviewRecord.aggregate.noSensitivePixelsObservedByReviewer
                noUncertaintyReported = $ReviewRecord.aggregate.noUncertaintyReported
                visualJudgmentNotPixelSafetyProof = $ReviewRecord.aggregate.visualJudgmentNotPixelSafetyProof
            }) | ConvertTo-Json -Depth 8 -Compress)) {
        throw "Operator independent-review summary does not match the exact retained review record."
    }
    if ($Sidecars.Count -ne $ReviewRecord.entries.Count) {
        throw "Independent review entries do not match the screenshot sidecar count."
    }
    for ($index = 0; $index -lt $Sidecars.Count; $index += 1) {
        $entry = $ReviewRecord.entries[$index]
        $sidecar = $Sidecars[$index]
        $expectedEntryRef = Get-TextSha256 (
            "$ReviewRecordSha256`n$($entry.sequence)`n$($entry.purpose)`n$($entry.sha256)"
        )
        if ($sidecar.reviewRecordSha256 -cne $ReviewRecordSha256 -or
            $sidecar.reviewEntryRef -cne $expectedEntryRef -or
            $sidecar.purpose -cne $entry.purpose -or $sidecar.image.name -cne $entry.image -or
            $sidecar.image.sha256 -cne $entry.sha256 -or
            [int64]$sidecar.image.width -ne [int64]$entry.width -or
            [int64]$sidecar.image.height -ne [int64]$entry.height) {
            throw "A screenshot sidecar is not bound to its exact independent review entry."
        }
    }
}

function Assert-RetainedEvidenceDirectoryV2 {
    param(
        [string]$PreflightPath,
        [string]$PostflightPath,
        [string]$MatrixPath,
        [string]$HelperPath,
        [string]$ApprovalPath,
        [string]$ReviewPath,
        [string]$ExternalPreflightPath,
        [string]$ExternalPostflightPath,
        [string]$OperatorPath,
        [string[]]$SidecarPaths,
        [string]$OutputPath
    )
    $directory = [IO.Path]::GetDirectoryName($OutputPath)
    if ([IO.Path]::GetFileName($OutputPath) -cne "browser-acceptance.json") {
        throw "v0.12.52 final evidence must use the canonical browser-acceptance.json filename."
    }
    $fixedInputs = [ordered]@{
        "candidate-preflight.json" = $PreflightPath
        "candidate-postflight.json" = $PostflightPath
        "browser-api-matrix.json" = $MatrixPath
        "browser-computer-helper-chain.json" = $HelperPath
        "scoped-action-approval.json" = $ApprovalPath
        "independent-visual-review.json" = $ReviewPath
        "external-surface-preflight.json" = $ExternalPreflightPath
        "external-surface-postflight.json" = $ExternalPostflightPath
        "operator-results.json" = $OperatorPath
    }
    $expectedNames = @($fixedInputs.Keys)
    foreach ($entry in $fixedInputs.GetEnumerator()) {
        if ([IO.Path]::GetFileName($entry.Value) -cne $entry.Key -or
            -not [String]::Equals(
                [IO.Path]::GetDirectoryName($entry.Value), $directory,
                [StringComparison]::OrdinalIgnoreCase
            )) {
            throw "v0.12.52 retained evidence inputs must use canonical names in one directory."
        }
    }
    foreach ($sidecarPath in $SidecarPaths) {
        if (-not [String]::Equals(
            [IO.Path]::GetDirectoryName($sidecarPath), $directory,
            [StringComparison]::OrdinalIgnoreCase
        )) {
            throw "v0.12.52 screenshot sidecars must be in the retained evidence directory."
        }
        $sidecarName = [IO.Path]::GetFileName($sidecarPath)
        $imageName = [IO.Path]::GetFileNameWithoutExtension($sidecarName) + ".png"
        $imagePath = [IO.Path]::Combine($directory, $imageName)
        if (-not [IO.File]::Exists($imagePath)) {
            throw "v0.12.52 retained screenshot image is missing."
        }
        $image = [IO.FileInfo]::new($imagePath)
        if (($image.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "v0.12.52 retained screenshot image must not be a reparse point."
        }
        $expectedNames += $sidecarName
        $expectedNames += $imageName
    }
    if ($expectedNames.Count -ne 21 -or ($expectedNames | Select-Object -Unique).Count -ne 21) {
        throw "v0.12.52 retained evidence allowlist must contain exactly 21 unique inputs."
    }
    $actualNames = @()
    foreach ($item in [IO.DirectoryInfo]::new($directory).GetFileSystemInfos()) {
        if ($item -isnot [IO.FileInfo] -or
            ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "v0.12.52 retained evidence directory contains a directory, link, or unsupported entry."
        }
        $actualNames += $item.Name
    }
    $expectedSorted = @($expectedNames | Sort-Object)
    $actualSorted = @($actualNames | Sort-Object)
    if (($expectedSorted -join "`n") -cne ($actualSorted -join "`n")) {
        throw "v0.12.52 retained evidence directory contains a missing or unexpected input."
    }
}

function New-OperatorResultsV3 {
    param(
        [object]$Candidate,
        [object]$CandidateBinding,
        [object]$ReleaseBinding,
        [object]$Helper,
        [object]$Approval,
        [string]$ApprovalSha256,
        [object]$Review,
        [string]$ReviewSha256,
        [object]$ExternalSurfaces,
        [string]$ObservedBrowserVersion
    )
    if (-not [regex]::IsMatch(
            $ObservedBrowserVersion,
            '^(1(?:4[0-9]|[5-9][0-9])|[2-9][0-9]{2})\.[0-9]{1,5}\.[0-9]{1,5}\.[0-9]{1,5}$'
        )) {
        throw "BrowserVersion must be a supported canonical four-part Google Chrome version."
    }
    $approvalId = [string]$Approval.approvalId
    $developerChanged = [string]$Helper.initialState.developerMode.value -ceq "disabled"
    $fullAccessChanged = [string]$Helper.initialState.fullAccess.value -ceq "disabled"
    $riskRefs = @($Helper.actions | ForEach-Object { [string]$_.riskRef })
    foreach ($requiredRisk in @(
        "installCandidate", "fullAccessUse", "acceptanceTokenSave",
        "clearSavedTokenInitiate", "clearSavedTokenConfirm", "extensionDisposition"
    )) {
        if ($riskRefs -cnotcontains $requiredRisk) {
            throw "The helper action chain is missing the required $requiredRisk approval consumption."
        }
    }
    if ($developerChanged -and $riskRefs -cnotcontains "developerModeChange") {
        throw "The helper changed Developer Mode without the required scoped approval reference."
    }
    $ExactAllowlistedActionChain = @($Helper.actions).Count -eq `
        $script:ComputerHelperActionNames.Count
    if ($ExactAllowlistedActionChain) {
        for ($ActionIndex = 0; $ActionIndex -lt $script:ComputerHelperActionNames.Count; $ActionIndex += 1) {
            $Action = $Helper.actions[$ActionIndex]
            $Epoch = $Helper.windowEpochs[$script:ComputerHelperActionEpochIndexes[$ActionIndex]]
            if ($Action.name -cne $script:ComputerHelperActionNames[$ActionIndex] -or
                $Action.source -cne $script:ComputerHelperActionSource -or
                $Action.epochRef -cne $Epoch.epochRef -or $Epoch.selectedExactly -ne $true) {
                $ExactAllowlistedActionChain = $false
                break
            }
        }
    }
    $ExactCandidateExtensionLifecycle = (
        -not [bool]$Helper.initialState.candidateCard.present -and
        [bool]$Helper.cleanup.candidateExtensionRemoved -and
        [bool]$Helper.cleanup.candidateCardAbsentAfterRemoval -and
        [bool]$Helper.cleanup.candidateCardAbsenceVerifiedFromFreshLiveUi
    )

    $captures = @()
    foreach ($purpose in $script:ExpectedScreenshotsV2.Keys) {
        $captures += [ordered]@{
            purpose = $purpose
            image = $script:ExpectedScreenshotsV2[$purpose]
            captureSurface = "local-browser-bridge-computer-helper"
            requiredVisibleState = $script:RequiredVisibleStatesV2[$purpose]
        }
    }
    $approvalCheckpoint = { param([bool]$Performed) [ordered]@{
        performed = $Performed
        approvalRef = $approvalId
    }}

    return [ordered]@{
        schemaVersion = 3
        evidenceType = "stock-user-chrome-operator-observations"
        releaseCandidateBinding = Copy-JsonObject $ReleaseBinding
        candidateBinding = [pscustomobject]$CandidateBinding
        environment = [ordered]@{
            platform = "windows-x86_64"
            browserProduct = "Google Chrome"
            browserVersion = $ObservedBrowserVersion
            stockUserChrome = $true
            existingUserSession = $true
            dedicatedTemporaryWindow = $true
            browserLaunchFlagsUsed = $false
            directCdpUsed = $false
            automationTestProfileUsed = $false
            localDemoOnly = $true
        }
        actionSurfaces = [ordered]@{
            bridgeApiMatrix = "local-browser-bridge-api"
            computerHelperApi = "local-browser-bridge-computer-api"
            orchestrationAndConsent = `
                "user-orchestrator-secured-ssh-exported-file-review"
            dedicatedWindowCreation = "local-browser-bridge-computer-helper"
            chromeExtensionsPage = "local-browser-bridge-computer-helper"
            nativeLoadUnpackedPicker = "local-browser-bridge-computer-helper"
            extensionPopup = "local-browser-bridge-computer-helper"
            chromeDebuggerNotice = "local-browser-bridge-computer-helper"
            browserApiResult = "local-browser-bridge-computer-helper"
            computerShareAction = "local-browser-bridge-computer-helper"
            acceptanceScreenshots = "local-browser-bridge-computer-helper"
            debuggerOwnerDuringBridgeLease = "local-browser-bridge-extension"
            competingDebuggerAttachmentAllowed = $false
            chromeMcpUsedDuringBridgeLease = $false
            chromeMcpReleaseEvidenceClaimed = $false
        }
        computerHelperChain = [ordered]@{
            candidateBoundExecutableStarted = $true
            connectedThroughLoopbackServer = [bool]$Helper.helper.connectedThroughLoopbackServer
            serverApiOnly = [bool]$Helper.helper.serverApiOnly
            exactChromeWindowSelected = [bool]$Helper.windowBinding.sameDedicatedTargetAcrossEpochs
            chromeExtensionsLoadCompleted = [bool]$Helper.actions[5].postconditionVerified
            browserApiActionCompleted = [bool]$Helper.browserAction.resultVerified
            exactWindowShareActionCompleted = [bool]$Helper.actions[11].postconditionVerified
            freshFramesBoundToComputerAction = $Helper.actions[11].preFrameRef -cne $Helper.actions[11].postFrameRef
            screenshotEndpoint = "/api/computer/screenshot"
            rawScreenshotCount = @($Helper.screenshots).Count
            topologyProtocolBound = [bool]$Helper.helper.topology.protocolRoundTrip
            ownerForcedSupervisorTermination = $Helper.cleanup.helperTerminationDisposition -ceq "owner-forced-exact-supervisor"
            helperDisconnectedAfterTermination = [bool]$Helper.cleanup.helperDisconnectedAfterTermination
            noHelperChildrenOrListenersRemain = (
                $Helper.cleanup.helperChildrenRemaining + $Helper.cleanup.helperListenersRemaining +
                $Helper.cleanup.exactHelperImageProcessesRemaining + $Helper.cleanup.canonicalPortListenersRemaining
            ) -eq 0
            helperStopped = [bool]$Helper.cleanup.helperDisconnectedAfterTermination
        }
        consentCheckpoints = [ordered]@{
            scopedActionTimeApproval = [ordered]@{
                recordSha256 = $ApprovalSha256
                approvalId = $approvalId
                obtainedAtActionTime = $true
                consumedBeforeFirstCoveredAction = [bool]$Approval.consumption.consumedBeforeFirstCoveredAction
                consumedBeforeExpiry = [bool]$Approval.consumption.consumedBeforeExpiry
                freshStateRevalidatedAfterApproval = `
                    [bool]$Approval.consumption.freshStateRevalidatedAfterApproval
                scopeUnchangedThroughRun = [bool]$Approval.consumption.scopeUnchangedThroughRun
                singleCandidateRun = [bool]$Approval.response.singleCandidateRun
            }
            installCandidate = & $approvalCheckpoint $true
            developerModeChange = & $approvalCheckpoint $developerChanged
            fullAccessUse = & $approvalCheckpoint $true
            acceptanceTokenSave = & $approvalCheckpoint $true
            clearSavedToken = & $approvalCheckpoint $true
            removeTestOwnedExtension = & $approvalCheckpoint $true
            restoreCapturedBrowserSettings = & $approvalCheckpoint $true
            failureRollback = & $approvalCheckpoint $false
        }
        initialState = [ordered]@{
            capturedBeforeRelevantMutation = [bool]$Helper.initialState.capturedFromFreshHelperFrames
            candidateExtensionPresent = [bool]$Helper.initialState.candidateCard.present
            developerMode = [ordered]@{
                value = [string]$Helper.initialState.developerMode.value
                changedByRun = $developerChanged
            }
            fullAccess = [ordered]@{
                value = [string]$Helper.initialState.fullAccess.value
                changedByRun = $fullAccessChanged
            }
            savedTokenConfigured = [bool]$Helper.initialState.savedToken.configured
        }
        extension = [ordered]@{
            cardCount = 1
            version = [string]$Candidate.version
            enabled = $true
            loadErrors = 0
            loadedVia = "chrome://extensions-load-unpacked"
            loadedDirectoryByteMatchesCandidateZip = [bool]$Helper.extensionPayload.verifiedAfterLoad
            popupConnected = [bool]$Helper.actions[8].postconditionVerified
            debuggerLeaseActiveAtFirstCapture = [bool]$Helper.screenshots[0].shareFrameFresh
            nativeDebuggerUseIndicatorSeen = [bool]$Helper.actions[5].postconditionVerified
        }
        handback = Copy-JsonObject $Helper.handback
        screenshotCaptures = $captures
        independentVisualReview = [ordered]@{
            recordSha256 = $ReviewSha256
            executorSessionRef = [string]$Review.executorSessionRef
            reviewerSessionRef = [string]$Review.reviewerSessionRef
            independentSessionBoundary = [bool]$Review.independentSessionBoundary
            reviewedCropCount = [int]$Review.aggregate.reviewedCropCount
            everySanitizedCropOpenedByReviewer = [bool]$Review.aggregate.everySanitizedCropOpenedByReviewer
            allImageDigestsMatched = [bool]$Review.aggregate.allImageDigestsMatched
            requiredVisibleStateConfirmedByReviewer = [bool]$Review.aggregate.requiredVisibleStateConfirmedByReviewer
            noSensitivePixelsObservedByReviewer = [bool]$Review.aggregate.noSensitivePixelsObservedByReviewer
            noUncertaintyReported = [bool]$Review.aggregate.noUncertaintyReported
            visualJudgmentNotPixelSafetyProof = [bool]$Review.aggregate.visualJudgmentNotPixelSafetyProof
        }
        restoration = [ordered]@{
            candidateExtensionPresence = [ordered]@{
                finalPresent = -not [bool]$Helper.cleanup.candidateCardAbsentAfterRemoval
                matchesInitial = (
                    (-not [bool]$Helper.cleanup.candidateCardAbsentAfterRemoval) -eq
                    [bool]$Helper.initialState.candidateCard.present
                )
                verifiedFromLiveUi = [bool]$Helper.cleanup.candidateCardAbsenceVerifiedFromFreshLiveUi
            }
            developerMode = [ordered]@{
                finalValue = [string]$Helper.initialState.developerMode.value
                matchesInitial = [bool]$Helper.cleanup.developerModeRestored
                verifiedFromLiveUi = [bool]$Helper.cleanup.developerModeRestored
            }
            fullAccess = [ordered]@{
                finalValue = [string]$Helper.initialState.fullAccess.value
                matchesInitial = [bool]$Helper.cleanup.fullAccessRestored
                verifiedFromLiveUi = [bool]$Helper.cleanup.fullAccessRestored
            }
            savedToken = [ordered]@{
                finalConfigured = -not [bool]$Helper.cleanup.savedTokenCleared
                matchesInitial = ((-not [bool]$Helper.cleanup.savedTokenCleared) -eq [bool]$Helper.initialState.savedToken.configured)
                verifiedFromLiveUi = [bool]$Helper.cleanup.savedTokenCleared
            }
        }
        cleanup = [ordered]@{
            controlReleased = [bool]$Helper.cleanup.allSharesStopped
            testTabsClosed = $true
            testWindowClosed = [bool]$Helper.cleanup.testWindowClosed
            popupDisconnected = [bool]$Helper.cleanup.savedTokenCleared
            serverStopped = $Helper.cleanup.serverListenersRemaining -eq 0
            portReleased = $Helper.cleanup.canonicalPortListenersRemaining -eq 0
            acceptanceCredentialClearedFromShell = [bool]$Helper.privacy.credentialRetained -eq $false
            savedTokenClear = [ordered]@{
                trustedPopupClick = [bool]$Helper.actions[21].resultVerified
                confirmationDialogShown = [bool]$Helper.actions[21].postconditionVerified
                confirmationAcceptedByExecutor = [bool]$Helper.actions[22].resultVerified
                approvalRef = $approvalId
                popupStateVerifiedAfterClear = [bool]$Helper.cleanup.savedTokenCleared
                tokenConfigured = -not [bool]$Helper.cleanup.savedTokenCleared
                clearButtonDisabled = [bool]$Helper.cleanup.savedTokenCleared
            }
            externalSurfacePreflightAttestationSha256 = [string]$ExternalSurfaces.PreflightSha256
            externalSurfacePostflightAttestationSha256 = [string]$ExternalSurfaces.PostflightSha256
            chromeMcpDisposition = [string]$ExternalSurfaces.ChromeMcpDisposition
            computerUseDisposition = [string]$ExternalSurfaces.ComputerUseDisposition
            reviewerInputDisposition = [string]$ExternalSurfaces.ReviewerInputDisposition
            computerHelperStopped = [bool]$Helper.cleanup.helperDisconnectedAfterTermination
            rawScreenshotScratchDeleted = @($Helper.screenshots | Where-Object { $_.rawImageRetained -ne $false }).Count -eq 0
            pendingReviewRecordsDeleted = $true
            extractedExtensionInventoryVerifiedBeforeDeletion = [bool]$Helper.extensionPayload.verifiedAfterCleanup
            extractedExtensionDirectoryDeleted = $true
            extensionDisposition = "removed"
            unrelatedTargetMutationCommandsIssued = -not $ExactAllowlistedActionChain
            unrelatedExtensionMutationCommandsIssued = `
                -not ($ExactAllowlistedActionChain -and $ExactCandidateExtensionLifecycle)
        }
        retainedEvidence = [ordered]@{
            scope = "acceptance-evidence-directory-only"
            exactAllowlistVerified = $true
            inputFileCount = 21
            finalFileCount = 22
            rawScreenshotsPresent = $false
            pendingReviewRecordsPresent = $false
            chromeMcpTranscriptPresent = $false
            computerUseTranscriptPresent = $false
            secretCredentialPresent = $false
            filesystemLocationsOrBrowserIdsPresent = $false
            externalToolAndPlatformLogsScope = "not-asserted"
        }
    }
}

function Invoke-BuildOperator {
    foreach ($item in @(
        @($PreflightRecord, "PreflightRecord"), @($ComputerHelperRecord, "ComputerHelperRecord"),
        @($ScopedApprovalRecord, "ScopedApprovalRecord"), @($IndependentReviewRecord, "IndependentReviewRecord"),
        @($ExternalSurfacePreflightAttestation, "ExternalSurfacePreflightAttestation"),
        @($ExternalSurfacePostflightAttestation, "ExternalSurfacePostflightAttestation"),
        @($BrowserVersion, "BrowserVersion"), @($OutputRecord, "OutputRecord")
    )) { Assert-RequiredArgument $item[0] $item[1] }
    $preflightPath = Resolve-RequiredFile $PreflightRecord "PreflightRecord"
    $helperPath = Resolve-RequiredFile $ComputerHelperRecord "ComputerHelperRecord"
    $approvalPath = Resolve-RequiredFile $ScopedApprovalRecord "ScopedApprovalRecord"
    $reviewPath = Resolve-RequiredFile $IndependentReviewRecord "IndependentReviewRecord"
    $externalSurfacePreflightPath = Resolve-RequiredFile `
        $ExternalSurfacePreflightAttestation "ExternalSurfacePreflightAttestation"
    $externalSurfacePostflightPath = Resolve-RequiredFile `
        $ExternalSurfacePostflightAttestation "ExternalSurfacePostflightAttestation"
    $outputPath = Resolve-NewOutputFile $OutputRecord
    $preflightRead = Read-JsonWithSha256 $preflightPath "PreflightRecord"
    $preflight = $preflightRead.Value
    $candidate = Assert-CandidatePreflight $preflight
    if ($candidate.version -cne $script:OperatorV2Version) {
        throw "BuildOperator is registered only for the v0.12.52 autonomous evidence protocol."
    }
    $binding = Get-CandidateBindingDomain $preflight $preflightRead.Sha256
    $release = Assert-ReleaseCandidateBinding $preflight.releaseCandidateBinding $candidate `
        "preflight releaseCandidateBinding"
    $approvalRead = Read-JsonWithSha256 $approvalPath "ScopedApprovalRecord"
    $approval = Assert-ScopedApprovalRecordV3 $approvalRead.Value `
        $binding $release $candidate
    $reviewRead = Read-JsonWithSha256 $reviewPath "IndependentReviewRecord"
    $review = Assert-IndependentReviewRecordV3 $reviewRead.Value `
        $binding $release $candidate
    $helperRead = Read-JsonWithSha256 $helperPath "ComputerHelperRecord"
    $helper = $helperRead.Value
    $externalSurfacePreflightRead = Read-JsonWithSha256 `
        $externalSurfacePreflightPath "ExternalSurfacePreflightAttestation"
    $externalSurfacePostflightRead = Read-JsonWithSha256 `
        $externalSurfacePostflightPath "ExternalSurfacePostflightAttestation"
    $externalSurfacePreflight = Assert-ExternalSurfaceAttestationV3 `
        $externalSurfacePreflightRead.Value $externalSurfacePreflightRead.Sha256 $release "preflight"
    $externalSurfacePostflight = Assert-ExternalSurfaceAttestationV3 `
        $externalSurfacePostflightRead.Value $externalSurfacePostflightRead.Sha256 $release "postflight"
    $externalSurfaces = Assert-ExternalSurfaceAttestationPairV3 `
        $externalSurfacePreflight $externalSurfacePostflight $helper $approval $review
    $operator = New-OperatorResultsV3 $candidate $binding $release $helper $approval `
        $approvalRead.Sha256 $review $reviewRead.Sha256 $externalSurfaces $BrowserVersion
    Assert-OperatorResultsV2 $operator $candidate.version $binding $release $candidate
    Assert-ComputerHelperRecordV2 $helper $binding $release $candidate $operator `
        ([string]$helper.run.apiMatrixRecordSha256)
    Assert-ApprovalMatchesHelperV3 $approval $approvalRead.Sha256 $helper $operator
    if ($review.executorSessionRef -cne $helper.operatorExchange.executorSessionRef -or
        $review.reviewerSessionRef -cne $helper.operatorExchange.reviewerSessionRef) {
        throw "Independent review sessions do not match the helper operator exchange."
    }
    $serialized = $operator | ConvertTo-Json -Depth 30 -Compress
    Assert-SafeSerializedRecord $serialized @(Read-DenyValues $DenyValuesFile)
    Write-NewJson $outputPath $operator
    Write-Output "Candidate-bound operator results were mechanically derived from validated evidence."
}

function Invoke-Finalize {
    Assert-RequiredArgument $PreflightRecord "PreflightRecord"
    Assert-RequiredArgument $PostflightRecord "PostflightRecord"
    Assert-RequiredArgument $ApiMatrixRecord "ApiMatrixRecord"
    Assert-RequiredArgument $OperatorResults "OperatorResults"
    Assert-RequiredArgument $ScreenshotRecords "ScreenshotRecords"
    Assert-RequiredArgument $OutputRecord "OutputRecord"
    $preflightPath = Resolve-RequiredFile $PreflightRecord "PreflightRecord"
    $postflightPath = Resolve-RequiredFile $PostflightRecord "PostflightRecord"
    $matrixPath = Resolve-RequiredFile $ApiMatrixRecord "ApiMatrixRecord"
    $operatorPath = Resolve-RequiredFile $OperatorResults "OperatorResults"
    $outputPath = Resolve-NewOutputFile $OutputRecord
    $preflightRead = Read-JsonWithSha256 $preflightPath "PreflightRecord"
    $preflight = $preflightRead.Value
    $preflightCandidate = Assert-CandidatePreflight $preflight
    $preflightSha256 = $preflightRead.Sha256
    $candidateBinding = Get-CandidateBindingDomain $preflight $preflightSha256
    $postflightRead = Read-JsonWithSha256 $postflightPath "PostflightRecord"
    $postflight = $postflightRead.Value
    $candidate = Assert-CandidatePostflight $postflight
    if ($preflightSha256 -cne $postflight.preflightRecordSha256 -or
        $preflight.runNonce -cne $postflight.runNonce) {
        throw "Candidate postflight does not bind the supplied preflight record."
    }
    if (($preflightCandidate | ConvertTo-Json -Depth 20 -Compress) -cne
        ($candidate | ConvertTo-Json -Depth 20 -Compress)) {
        throw "Candidate preflight and postflight bindings differ."
    }
    $releaseCandidateBinding = $null
    if ($candidate.version -ceq $script:OperatorV2Version) {
        $releaseCandidateBinding = Assert-ReleaseCandidateBinding `
            $preflight.releaseCandidateBinding $candidate "preflight releaseCandidateBinding"
        if (($releaseCandidateBinding | ConvertTo-Json -Depth 10 -Compress) -cne
            ($postflight.releaseCandidateBinding | ConvertTo-Json -Depth 10 -Compress)) {
            throw "Candidate preflight and postflight releaseCandidateBinding objects differ."
        }
    }
    Assert-CandidateBindingDomain $postflight.candidateBinding $candidateBinding "candidate postflight candidateBinding"
    $matrixRead = Read-JsonWithSha256 $matrixPath "ApiMatrixRecord"
    $matrix = $matrixRead.Value
    Assert-ApiMatrixRecord $matrix $candidate.version $candidateBinding
    $operatorRead = Read-JsonWithSha256 $operatorPath "OperatorResults"
    $operator = $operatorRead.Value
    Assert-OperatorResults $operator $candidate.version $candidateBinding $releaseCandidateBinding $candidate
    $helperPath = $null
    $helperRecord = $null
    if ($candidate.version -ceq $script:OperatorV2Version) {
        Assert-RequiredArgument $ComputerHelperRecord "ComputerHelperRecord"
        Assert-RequiredArgument $ScopedApprovalRecord "ScopedApprovalRecord"
        Assert-RequiredArgument $IndependentReviewRecord "IndependentReviewRecord"
        Assert-RequiredArgument $ExternalSurfacePreflightAttestation `
            "ExternalSurfacePreflightAttestation"
        Assert-RequiredArgument $ExternalSurfacePostflightAttestation `
            "ExternalSurfacePostflightAttestation"
        $helperPath = Resolve-RequiredFile $ComputerHelperRecord "ComputerHelperRecord"
        $approvalPath = Resolve-RequiredFile $ScopedApprovalRecord "ScopedApprovalRecord"
        $reviewPath = Resolve-RequiredFile $IndependentReviewRecord "IndependentReviewRecord"
        $externalSurfacePreflightPath = Resolve-RequiredFile `
            $ExternalSurfacePreflightAttestation "ExternalSurfacePreflightAttestation"
        $externalSurfacePostflightPath = Resolve-RequiredFile `
            $ExternalSurfacePostflightAttestation "ExternalSurfacePostflightAttestation"
        $approvalRead = Read-JsonWithSha256 $approvalPath "ScopedApprovalRecord"
        $approvalRecord = Assert-ScopedApprovalRecordV3 `
            $approvalRead.Value $candidateBinding `
            $releaseCandidateBinding $candidate
        $reviewRead = Read-JsonWithSha256 $reviewPath "IndependentReviewRecord"
        $reviewRecord = Assert-IndependentReviewRecordV3 `
            $reviewRead.Value $candidateBinding `
            $releaseCandidateBinding $candidate
        $helperRead = Read-JsonWithSha256 $helperPath "ComputerHelperRecord"
        $helperRecord = $helperRead.Value
        Assert-ComputerHelperRecordV2 `
            $helperRecord $candidateBinding $releaseCandidateBinding $candidate $operator $matrixRead.Sha256
        $externalSurfacePreflightRead = Read-JsonWithSha256 `
            $externalSurfacePreflightPath "ExternalSurfacePreflightAttestation"
        $externalSurfacePostflightRead = Read-JsonWithSha256 `
            $externalSurfacePostflightPath "ExternalSurfacePostflightAttestation"
        $externalSurfacePreflight = Assert-ExternalSurfaceAttestationV3 `
            $externalSurfacePreflightRead.Value $externalSurfacePreflightRead.Sha256 `
            $releaseCandidateBinding "preflight"
        $externalSurfacePostflight = Assert-ExternalSurfaceAttestationV3 `
            $externalSurfacePostflightRead.Value $externalSurfacePostflightRead.Sha256 `
            $releaseCandidateBinding "postflight"
        $externalSurfaces = Assert-ExternalSurfaceAttestationPairV3 `
            $externalSurfacePreflight $externalSurfacePostflight `
            $helperRecord $approvalRecord $reviewRecord
        if ($operator.cleanup.externalSurfacePreflightAttestationSha256 -cne `
                $externalSurfaces.PreflightSha256 -or
            $operator.cleanup.externalSurfacePostflightAttestationSha256 -cne `
                $externalSurfaces.PostflightSha256) {
            throw "Operator cleanup does not bind the exact external-surface attestation pair."
        }
    }
    $screenshots = @(
        Assert-ScreenshotRecords $ScreenshotRecords $candidateBinding $releaseCandidateBinding $candidate $candidate.version
    )
    if ($candidate.version -ceq $script:OperatorV2Version) {
        Assert-HelperScreenshotsMatchSidecarsV2 $helperRecord $screenshots
        Assert-ApprovalMatchesHelperV3 $approvalRecord $approvalRead.Sha256 $helperRecord $operator
        Assert-ReviewMatchesHelperAndSidecarsV3 `
            $reviewRecord $reviewRead.Sha256 $helperRecord $operator $screenshots
    }
    $candidateSummary = [ordered]@{
        version = $candidate.version
        finalSha = $candidate.finalSha
        preflightRecordSha256 = $preflightSha256
        postflightRecordSha256 = $postflightRead.Sha256
        checksumManifestSha256 = $candidate.checksumManifest.sha256
        serverName = $candidate.server.name
        serverSha256 = $candidate.server.sha256
        extensionZipName = $candidate.extension.name
        extensionZipSha256 = $candidate.extension.sha256
        combinedExtensionPayloadSha256 = $candidate.extension.combinedPayloadSha256
        minimumChromeVersion = $candidate.extension.minimumChromeVersion
        manifestPermissions = @($candidate.extension.permissions)
        manifestHostPermissions = @($candidate.extension.hostPermissions)
    }
    if ($candidate.version -ceq $script:OperatorV2Version) {
        $candidateSummary.computerHelperName = $candidate.computerHelper.name
        $candidateSummary.computerHelperSha256 = $candidate.computerHelper.sha256
    }
    if ($candidate.version -ceq $script:OperatorV2Version) {
        $resolvedSidecars = @()
        foreach ($path in $ScreenshotRecords) {
            $resolvedSidecars += Resolve-RequiredFile $path "ScreenshotRecords entry"
        }
        Assert-RetainedEvidenceDirectoryV2 `
            $preflightPath $postflightPath $matrixPath $helperPath $approvalPath $reviewPath `
            $externalSurfacePreflightPath $externalSurfacePostflightPath `
            $operatorPath $resolvedSidecars $outputPath
        $record = [ordered]@{
            schemaVersion = 3
            evidenceType = "stock-user-chrome-acceptance"
            recordedAtUtc = [DateTime]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
            passed = $true
            releaseCandidateBinding = $releaseCandidateBinding
            candidateBinding = $candidateBinding
            candidate = $candidateSummary
            environment = $operator.environment
            actionSurfaces = $operator.actionSurfaces
            computerHelperChain = $operator.computerHelperChain
            consentCheckpoints = $operator.consentCheckpoints
            initialState = $operator.initialState
            extension = $operator.extension
            handback = $helperRecord.handback
            apiMatrixRecordSha256 = $matrixRead.Sha256
            apiMatrix = $matrix
            computerHelperRecordSha256 = $helperRead.Sha256
            computerHelper = $helperRecord
            scopedApprovalRecordSha256 = $approvalRead.Sha256
            scopedActionApproval = $approvalRecord
            independentReviewRecordSha256 = $reviewRead.Sha256
            independentVisualReview = $reviewRecord
            operatorRecordSha256 = $operatorRead.Sha256
            screenshotCaptures = $operator.screenshotCaptures
            screenshots = $screenshots
            restoration = $operator.restoration
            cleanup = $operator.cleanup
            retainedEvidence = $operator.retainedEvidence
            privacy = [ordered]@{
                allowlistedSchemaOnly = $true
                retainedEvidenceScope = "acceptance-evidence-directory-only"
                rawApiResponsesPresentInRetainedEvidence = $false
                chromeMcpTranscriptPresentInRetainedEvidence = $false
                computerUseTranscriptPresentInRetainedEvidence = $false
                filesystemLocationsPresentInRetainedEvidence = $false
                browserAccountDetailsPresentInRetainedEvidence = $false
                externalToolAndPlatformLogsScope = "not-asserted"
                automaticPixelRedactionClaimed = $false
                independentAgentScreenshotReviewRequired = $true
            }
        }
    }
    else {
        $record = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-acceptance"
            recordedAtUtc = [DateTime]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
            passed = $true
            candidateBinding = $candidateBinding
            candidate = $candidateSummary
            environment = $operator.environment
            extension = $operator.extension
            apiMatrixRecordSha256 = $matrixRead.Sha256
            apiMatrix = $matrix
            handback = $operator.handback
            screenshots = $screenshots
            cleanup = $operator.cleanup
            privacy = [ordered]@{
                allowlistedSchemaOnly = $true
                rawApiResponsesRetained = $false
                chromeMcpTranscriptRetained = $false
                filesystemLocationsRetained = $false
                browserAccountOrProfileStateRetained = $false
                automaticPixelRedactionClaimed = $false
                manualScreenshotReviewRequired = $true
            }
        }
    }
    $serialized = $record | ConvertTo-Json -Depth 30 -Compress
    Assert-SafeSerializedRecord $serialized @(Read-DenyValues $DenyValuesFile)
    Write-NewJson $outputPath $record
    Write-Output "Allowlisted stock-user-Chrome acceptance record was written."
}

function Invoke-InitializeOperator {
    Assert-RequiredArgument $PreflightRecord "PreflightRecord"
    Assert-RequiredArgument $OutputRecord "OutputRecord"
    $preflightPath = Resolve-RequiredFile $PreflightRecord "PreflightRecord"
    $outputPath = Resolve-NewOutputFile $OutputRecord
    $preflight = Read-Json $preflightPath "PreflightRecord"
    [void](Assert-CandidatePreflight $preflight)
    $binding = Get-CandidateBindingDomain $preflight (Get-Sha256 $preflightPath)
    $templateVersion = [string]$preflight.candidate.version
    if ($templateVersion -ceq $script:OperatorV2Version) {
        throw "v0.12.52 operator results must be created with BuildOperator; editable checklists are not accepted."
    }
    if (@("0.12.2", $script:OperatorV2Version) -cnotcontains $templateVersion) {
        throw "No stock-user-Chrome operator template is registered for candidate version $templateVersion."
    }
    $templatePath = [IO.Path]::GetFullPath([IO.Path]::Combine(
        $PSScriptRoot, "..", "evidence", "v$templateVersion", "browser", "operator-results.template.json"
    ))
    $operator = Read-Json $templatePath "operator template"
    Assert-ExactKeys $operator @(
        "schemaVersion", "evidenceType", "candidateBinding", "environment", "extension", "handback", "cleanup"
    ) "operator template"
    Assert-IntegerRange $operator.schemaVersion 1 1 "operator template schemaVersion"
    $operator.candidateBinding = [pscustomobject]$binding
    Write-NewJson $outputPath $operator
    Write-Output "Candidate-bound operator checklist was initialized."
}

function New-PassingHandbackCaseV2SelfTest {
    param([string]$Trigger, [string]$Reason)
    return [ordered]@{
        trigger = $Trigger
        operatorSurface = "local-browser-bridge-computer-helper"
        statusPollMethod = "browser.control.status"
        statusPolledAfterTrigger = $true
        reducedStatus = [ordered]@{
            active = $false; humanPaused = $true; reason = $Reason; revocationPending = $false
        }
        controlStartRefusal = [ordered]@{
            httpStatus = 423; errorCode = "HUMAN_CONTROL_PAUSED"; taxonomyState = "needs_user"
            taxonomyAction = "handback"; retriable = $false
        }
        tabMutationRefusal = [ordered]@{
            httpStatus = 423; errorCode = "HUMAN_CONTROL_PAUSED"; taxonomyState = "needs_user"
            taxonomyAction = "handback"; retriable = $false
        }
        indicatorsRemoved = $true
        resume = [ordered]@{
            trustedPopupClick = $true
            operatorSurface = "local-browser-bridge-computer-helper"
            statusPollMethod = "browser.control.status"
            statusPolledAfterResume = $true
            reducedStatus = [ordered]@{
                active = $false; humanPaused = $false; revocationPending = $false
            }
            postResumeStartSucceeded = $true
            activeStatusPolled = $true
            activeStatus = [ordered]@{
                active = $true; humanPaused = $false; revocationPending = $false
            }
        }
    }
}

function New-PassingOperatorV2SelfTest {
    param([object]$Binding, [object]$ReleaseBinding)
    $templatePath = [IO.Path]::GetFullPath([IO.Path]::Combine(
        $PSScriptRoot, "..", "evidence", "v0.12.52", "browser", "operator-results.template.json"
    ))
    $operator = Read-Json $templatePath "v0.12.52 operator self-test template"
    $operator.releaseCandidateBinding = Copy-JsonObject $ReleaseBinding
    $operator.candidateBinding = [pscustomobject]$Binding
    $operator.environment.browserVersion = "151.0.7390.0"
    foreach ($name in @(
        "stockUserChrome", "existingUserSession", "dedicatedTemporaryWindow", "localDemoOnly"
    )) {
        $operator.environment.$name = $true
    }
    foreach ($name in @("browserLaunchFlagsUsed", "directCdpUsed", "automationTestProfileUsed")) {
        $operator.environment.$name = $false
    }
    foreach ($name in @(
        "dedicatedWindowCreation", "chromeExtensionsPage", "nativeLoadUnpackedPicker",
        "extensionPopup", "chromeDebuggerNotice", "browserApiResult",
        "computerShareAction", "acceptanceScreenshots"
    )) {
        $operator.actionSurfaces.$name = "local-browser-bridge-computer-helper"
    }
    $operator.actionSurfaces.orchestrationAndConsent = `
        "user-orchestrator-secured-ssh-exported-file-review"
    $operator.actionSurfaces.competingDebuggerAttachmentAllowed = $false
    $operator.actionSurfaces.chromeMcpUsedDuringBridgeLease = $false
    $operator.actionSurfaces.chromeMcpReleaseEvidenceClaimed = $false

    foreach ($name in @(
        "candidateBoundExecutableStarted", "connectedThroughLoopbackServer", "serverApiOnly",
        "exactChromeWindowSelected", "chromeExtensionsLoadCompleted", "browserApiActionCompleted",
        "exactWindowShareActionCompleted", "freshFramesBoundToComputerAction",
        "topologyProtocolBound", "ownerForcedSupervisorTermination", "helperDisconnectedAfterTermination",
        "noHelperChildrenOrListenersRemain", "helperStopped"
    )) {
        $operator.computerHelperChain.$name = $true
    }
    $operator.computerHelperChain.rawScreenshotCount = 6

    $operator.handback.stop = New-PassingHandbackCaseV2SelfTest "in-page-stop" "released_by_user"
    $operator.handback.cancel = New-PassingHandbackCaseV2SelfTest "chrome-native-cancel" "canceled_by_user"

    $approvalId = [String]::new([char]"a", 64)
    $operator.consentCheckpoints.scopedActionTimeApproval.recordSha256 = [String]::new([char]"b", 64)
    $operator.consentCheckpoints.scopedActionTimeApproval.approvalId = $approvalId
    foreach ($name in @(
        "obtainedAtActionTime", "consumedBeforeFirstCoveredAction", "consumedBeforeExpiry",
        "freshStateRevalidatedAfterApproval", "scopeUnchangedThroughRun", "singleCandidateRun"
    )) { $operator.consentCheckpoints.scopedActionTimeApproval.$name = $true }
    foreach ($name in @(
        "installCandidate", "developerModeChange", "fullAccessUse", "acceptanceTokenSave",
        "clearSavedToken", "removeTestOwnedExtension", "restoreCapturedBrowserSettings"
    )) {
        $operator.consentCheckpoints.$name.performed = $true
        $operator.consentCheckpoints.$name.approvalRef = $approvalId
    }
    $operator.consentCheckpoints.failureRollback.performed = $false
    $operator.consentCheckpoints.failureRollback.approvalRef = $approvalId

    $operator.initialState.capturedBeforeRelevantMutation = $true
    $operator.initialState.candidateExtensionPresent = $false
    $operator.initialState.developerMode.value = "disabled"
    $operator.initialState.developerMode.changedByRun = $true
    $operator.initialState.fullAccess.value = "enabled"
    $operator.initialState.fullAccess.changedByRun = $false
    $operator.initialState.savedTokenConfigured = $false

    $operator.extension.cardCount = 1
    $operator.extension.enabled = $true
    $operator.extension.loadErrors = 0
    foreach ($name in @(
        "loadedDirectoryByteMatchesCandidateZip", "popupConnected",
        "debuggerLeaseActiveAtFirstCapture", "nativeDebuggerUseIndicatorSeen"
    )) {
        $operator.extension.$name = $true
    }
    foreach ($capture in $operator.screenshotCaptures) {
        $capture.captureSurface = "local-browser-bridge-computer-helper"
    }

    $operator.independentVisualReview.recordSha256 = [String]::new([char]"c", 64)
    $operator.independentVisualReview.executorSessionRef = [String]::new([char]"d", 64)
    $operator.independentVisualReview.reviewerSessionRef = [String]::new([char]"e", 64)
    foreach ($name in @(
        "independentSessionBoundary", "everySanitizedCropOpenedByReviewer", "allImageDigestsMatched",
        "requiredVisibleStateConfirmedByReviewer", "noSensitivePixelsObservedByReviewer",
        "noUncertaintyReported", "visualJudgmentNotPixelSafetyProof"
    )) { $operator.independentVisualReview.$name = $true }
    $operator.independentVisualReview.reviewedCropCount = 6

    $operator.restoration.candidateExtensionPresence.finalPresent = $false
    $operator.restoration.candidateExtensionPresence.matchesInitial = $true
    $operator.restoration.candidateExtensionPresence.verifiedFromLiveUi = $true
    $operator.restoration.developerMode.finalValue = "disabled"
    $operator.restoration.fullAccess.finalValue = "enabled"
    foreach ($name in @("developerMode", "fullAccess")) {
        $operator.restoration.$name.matchesInitial = $true
        $operator.restoration.$name.verifiedFromLiveUi = $true
    }
    $operator.restoration.savedToken.finalConfigured = $false
    $operator.restoration.savedToken.matchesInitial = $true
    $operator.restoration.savedToken.verifiedFromLiveUi = $true

    foreach ($name in @(
        "controlReleased", "testTabsClosed", "testWindowClosed", "popupDisconnected",
        "serverStopped", "portReleased", "acceptanceCredentialClearedFromShell",
        "computerHelperStopped", "rawScreenshotScratchDeleted", "pendingReviewRecordsDeleted",
        "extractedExtensionInventoryVerifiedBeforeDeletion", "extractedExtensionDirectoryDeleted"
    )) {
        $operator.cleanup.$name = $true
    }
    $operator.cleanup.externalSurfacePreflightAttestationSha256 = [String]::new([char]"8", 64)
    $operator.cleanup.externalSurfacePostflightAttestationSha256 = [String]::new([char]"9", 64)
    $operator.cleanup.chromeMcpDisposition = "never-used-through-independent-review"
    $operator.cleanup.computerUseDisposition = "not-resumed-through-independent-review"
    $operator.cleanup.reviewerInputDisposition = "exported-digest-bound-files-only"
    foreach ($name in @(
        "trustedPopupClick", "confirmationDialogShown", "confirmationAcceptedByExecutor",
        "popupStateVerifiedAfterClear", "clearButtonDisabled"
    )) {
        $operator.cleanup.savedTokenClear.$name = $true
    }
    $operator.cleanup.savedTokenClear.tokenConfigured = $false
    $operator.cleanup.savedTokenClear.approvalRef = $approvalId
    $operator.cleanup.extensionDisposition = "removed"
    $operator.cleanup.unrelatedTargetMutationCommandsIssued = $false
    $operator.cleanup.unrelatedExtensionMutationCommandsIssued = $false

    $operator.retainedEvidence.exactAllowlistVerified = $true
    $operator.retainedEvidence.inputFileCount = 21
    $operator.retainedEvidence.finalFileCount = 22
    foreach ($name in @(
        "rawScreenshotsPresent", "pendingReviewRecordsPresent", "chromeMcpTranscriptPresent",
        "computerUseTranscriptPresent", "secretCredentialPresent",
        "filesystemLocationsOrBrowserIdsPresent"
    )) {
        $operator.retainedEvidence.$name = $false
    }
    return $operator
}

function Copy-JsonObject {
    param([object]$Value)
    return ConvertFrom-JsonPreservingStrings ($Value | ConvertTo-Json -Depth 30 -Compress)
}

function New-PassingReleaseCandidateBindingV3SelfTest {
    param([object]$Candidate)
    $assetNames = @($Candidate.checksumManifest.canonicalNamesInOrder) + "SHA256SUMS.txt"
    $assetHashes = @(
        $Candidate.server.sha256,
        $Candidate.computerHelper.sha256,
        [String]::new([char]"c", 64),
        $Candidate.extension.sha256,
        $Candidate.checksumManifest.sha256
    )
    $assets = @()
    for ($index = 0; $index -lt $assetNames.Count; $index += 1) {
        $assets += [ordered]@{ file = $assetNames[$index]; bytes = 1000; sha256 = $assetHashes[$index] }
    }
    return [ordered]@{
        schemaVersion = 3
        version = $Candidate.version
        releaseTag = "v$($Candidate.version)"
        repository = "flrngel/local-browser-bridge"
        sourceSha = $Candidate.finalSha
        workflowRunId = "123456789"
        workflowRunAttempt = "1"
        workflowEvent = "workflow_dispatch"
        workflowRef = "refs/heads/main"
        workflowPath = ".github/workflows/deploy.yml"
        artifactId = "987654321"
        artifactName = "release-candidate"
        artifactZipBytes = 5000
        artifactZipSha256 = [String]::new([char]"d", 64)
        checksumManifestSha256 = $Candidate.checksumManifest.sha256
        attestationInvocationUri = "https://github.com/flrngel/local-browser-bridge/actions/runs/123456789/attempts/1"
        attestedAssetCount = 5
        githubHostedRunner = $true
        assets = $assets
    }
}

function New-PassingScopedApprovalV3SelfTest {
    param(
        [object]$Binding,
        [object]$ReleaseBinding,
        [DateTimeOffset]$ConfirmedAt,
        [string]$OrchestratorSessionRef
    )
    return [ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-scoped-action-approval"
        releaseCandidateBinding = Copy-JsonObject $ReleaseBinding
        candidateBinding = [pscustomobject]$Binding
        approvalId = [String]::new([char]"a", 64)
        request = [ordered]@{
            createdAtUtc = Format-CanonicalUtc ($ConfirmedAt.AddSeconds(-1))
            expiresAtUtc = Format-CanonicalUtc ($ConfirmedAt.AddMinutes(10))
            challengeFrameRef = [String]::new([char]"1", 64)
            scopeSha256 = [String]::new([char]"b", 64)
            coveredActions = @($script:CoveredApprovalActions)
            loopbackOnly = $true
            dedicatedWindowOnly = $true
            restoreCapturedState = $true
            noUnrelatedExtensionMutation = $true
        }
        response = [ordered]@{
            approvedBy = "user"
            deliveredBy = "user-via-orchestrator"
            orchestratorSessionRef = $OrchestratorSessionRef
            confirmationMode = "batched-action-time"
            confirmedAtUtc = Format-CanonicalUtc $ConfirmedAt
            requestSha256 = [String]::new([char]"c", 64)
            singleCandidateRun = $true
        }
        consumption = [ordered]@{
            consumedBeforeFirstCoveredAction = $true
            consumedBeforeExpiry = $true
            preDispatchFrameRef = [String]::new([char]"2", 64)
            preDispatchDecisionRef = [String]::new([char]"3", 64)
            preDispatchVerifiedAtUtc = Format-CanonicalUtc ($ConfirmedAt.AddSeconds(1))
            freshStateRevalidatedAfterApproval = $true
            scopeUnchangedThroughRun = $true
            replayed = $false
            cleanupAuthoritySurvivesFailure = $true
        }
    }
}

function New-PassingIndependentReviewV3SelfTest {
    param(
        [object]$Binding,
        [object]$ReleaseBinding,
        [string]$ExecutorSessionRef,
        [string]$ReviewerSessionRef,
        [string]$ImageSha256,
        [DateTimeOffset]$ReviewedAt
    )
    $entries = @()
    $index = 0
    foreach ($purpose in $script:ExpectedScreenshotsV2.Keys) {
        $entries += [ordered]@{
            sequence = $index + 1
            purpose = $purpose
            image = $script:ExpectedScreenshotsV2[$purpose]
            sha256 = $ImageSha256
            width = 120
            height = 32
            requiredVisibleStateSha256 = Get-TextSha256 $script:RequiredVisibleStatesV2[$purpose]
            digestMatched = $true
            requiredStateVerdict = "pass"
            sensitivePixelsObserved = $false
            uncertain = $false
        }
        $index += 1
    }
    return [ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-independent-visual-review"
        releaseCandidateBinding = Copy-JsonObject $ReleaseBinding
        candidateBinding = [pscustomobject]$Binding
        executorSessionRef = $ExecutorSessionRef
        reviewerSessionRef = $ReviewerSessionRef
        independentSessionBoundary = $true
        requestSha256 = [String]::new([char]"f", 64)
        reviewedAtUtc = Format-CanonicalUtc $ReviewedAt
        entries = $entries
        aggregate = [ordered]@{
            reviewedCropCount = 6
            everySanitizedCropOpenedByReviewer = $true
            allImageDigestsMatched = $true
            requiredVisibleStateConfirmedByReviewer = $true
            noSensitivePixelsObservedByReviewer = $true
            noUncertaintyReported = $true
            visualJudgmentNotPixelSafetyProof = $true
        }
    }
}

function New-PassingComputerHelperRecordV2SelfTest {
    param(
        [object]$Binding,
        [object]$ReleaseBinding,
        [object]$Candidate,
        [string]$ApiMatrixSha256,
        [object]$ApprovalRecord,
        [string]$ApprovalRecordSha256,
        [string]$ExecutorSessionRef,
        [string]$ReviewerSessionRef
    )
    $approvalTime = Assert-UtcTimestamp $ApprovalRecord.response.confirmedAtUtc `
        "self-test approval confirmation"
    $start = $approvalTime.AddSeconds(-2)
    $helperProcessRef = [String]::new([char]"e", 64)
    $serverProcessRef = [String]::new([char]"f", 64)
    $lifecycle = @()
    $connections = @(
        "disconnected", "disconnected", "disconnected", "connected",
        "connected", "connected", "disconnected", "disconnected"
    )
    for ($index = 0; $index -lt $script:ComputerHelperLifecycleEvents.Count; $index += 1) {
        $isServer = @(0, 7) -ccontains $index
        $isHelper = @(2, 4, 5) -ccontains $index
        $lifecycle += [ordered]@{
            sequence = $index + 1
            name = $script:ComputerHelperLifecycleEvents[$index]
            atUtc = Format-CanonicalUtc ($start.AddSeconds($index + 1))
            source = $script:ComputerHelperActionSource
            connectionState = $connections[$index]
            processRef = if ($isServer) { $serverProcessRef } elseif ($isHelper) { $helperProcessRef } else { "not-applicable" }
            executableSha256 = if ($isServer) { $Candidate.server.sha256 } elseif ($isHelper) { $Candidate.computerHelper.sha256 } else { "not-applicable" }
            exitCode = if (@(5, 7) -ccontains $index) { 1 } else { -1 }
            resultVerified = $true
        }
    }

    $epochs = @()
    $dedicatedTargetRef = [String]::new([char]"a", 64)
    $dedicatedProcessRef = [String]::new([char]"b", 64)
    for ($index = 0; $index -lt $script:ComputerHelperEpochNames.Count; $index += 1) {
        $targetRef = if (@(1, 3, 5, 7, 9, 11, 12) -ccontains $index) {
            $dedicatedTargetRef
        }
        else {
            [String]::new([char]"0", 60) + ('{0:x4}' -f (200 + $index))
        }
        $epochs += [ordered]@{
            sequence = $index + 1
            name = $script:ComputerHelperEpochNames[$index]
            surface = $script:ComputerHelperEpochSurfaces[$index]
            application = "google-chrome"
            processRef = $dedicatedProcessRef
            epochRef = [String]::new([char]"0", 60) + ('{0:x4}' -f (100 + $index))
            targetRef = $targetRef
            shareRef = [String]::new([char]"0", 60) + ('{0:x4}' -f (300 + $index))
            selectedExactly = $true
            shareStartSequence = ($index * 10) + 1
            shareStartSucceeded = $true
            firstFreshFrameRef = [String]::new([char]"0", 60) + ('{0:x4}' -f (400 + $index))
            lastFreshFrameRef = [String]::new([char]"0", 60) + ('{0:x4}' -f (500 + $index))
            freshObservationCount = 2
            shareStopSequence = ($index * 10) + 9
            shareStopSucceeded = $true
            rawIdentifiersRetained = $false
        }
    }

    $actions = @()
    for ($index = 0; $index -lt $script:ComputerHelperActionNames.Count; $index += 1) {
        $mutator = $script:ComputerHelperActionMutators[$index]
        if ($mutator -ceq "conditional-developer-mode") { $mutator = "computer.click" }
        elseif ($mutator -ceq "conditional-full-access") { $mutator = "none" }
        $methods = if ($mutator -ceq "none") {
            @("computer.observe", "computer.observe", "computer.observe")
        }
        else { @("computer.observe", $mutator, "computer.observe") }
        if ($index -eq 4) {
            $methods = @(
                "computer.observe", "computer.key", "computer.typeText",
                "computer.click", "computer.status"
            )
        }
        elseif ($index -eq 24) {
            $methods = @(
                "computer.observe", "computer.click", "computer.click",
                "computer.click", "computer.observe"
            )
        }
        elseif ($index -eq ($script:ComputerHelperActionNames.Count - 1)) {
            $methods = @("computer.observe", "computer.key", "computer.status")
        }
        $riskRef = $script:ComputerHelperActionRiskRefs[$index]
        if ($riskRef -ceq "conditional-developer-mode") {
            $riskRef = "developerModeChange"
        }
        $preDigit = [char](97 + ($index % 6))
        $postDigit = [char](49 + ($index % 6))
        $actions += [ordered]@{
            sequence = $index + 1
            name = $script:ComputerHelperActionNames[$index]
            atUtc = Format-CanonicalUtc ($start.AddSeconds(20 + $index))
            source = $script:ComputerHelperActionSource
            epochRef = $epochs[$script:ComputerHelperActionEpochIndexes[$index]].epochRef
            methods = $methods
            preFrameRef = [String]::new($preDigit, 62) + ('{0:x2}' -f $index)
            postFrameRef = [String]::new($postDigit, 62) + ('{0:x2}' -f $index)
            normalizedParamsSha256 = [String]::new([char]"d", 62) + ('{0:x2}' -f $index)
            responseSha256 = [String]::new([char]"e", 62) + ('{0:x2}' -f $index)
            httpStatus = 200
            resultVerified = $true
            postconditionVerified = $true
            dispatchedAtUtc = Format-CanonicalUtc ($start.AddSeconds(19 + $index))
            operatorDecisionRef = [String]::new([char]"0", 60) + ('{0:x4}' -f (700 + $index))
            riskRef = $riskRef
            approvalRef = if ($riskRef -ceq "none") { "none" } else { $ApprovalRecord.approvalId }
        }
    }

    $shots = @()
    $shotIndex = 0
    foreach ($purpose in $script:ExpectedScreenshotsV2.Keys) {
        $rawName = [IO.Path]::GetFileNameWithoutExtension(
            $script:ExpectedScreenshotsV2[$purpose]
        ) + ".raw.png"
        $epoch = $epochs[$script:ComputerHelperScreenshotEpochIndexes[$shotIndex]]
        $shots += [ordered]@{
            sequence = $shotIndex + 1
            purpose = $purpose
            source = $script:ComputerHelperActionSource
            epochRef = $epoch.epochRef
            shareRef = $epoch.shareRef
            frameRef = [String]::new([char]"f", 62) + ('{0:x2}' -f $shotIndex)
            rawImage = $rawName
            endpoint = "/api/computer/screenshot"
            bytes = 2000
            sha256 = [String]::new([char]"9", 64)
            width = 320
            height = 180
            exactWindowFrame = $true
            shareFrameFresh = $true
            rawImageRetained = $false
        }
        $shotIndex += 1
    }

    $recorderPath = [IO.Path]::Combine($PSScriptRoot, "record-computer-helper-chain.ps1")
    return [ordered]@{
        schemaVersion = 2
        evidenceType = "stock-user-chrome-computer-helper-chain"
        version = $script:OperatorV2Version
        releaseCandidateBinding = Copy-JsonObject $ReleaseBinding
        candidateBinding = $Binding
        passed = $true
        recordedBy = [ordered]@{
            name = "record-computer-helper-chain.ps1"
            sha256 = Get-Sha256 $recorderPath
            source = "candidate-final-sha-blob"
        }
        run = [ordered]@{
            runNonce = $Binding.runNonce
            preflightRecordSha256 = $Binding.preflightRecordSha256
            apiMatrixRecordSha256 = $ApiMatrixSha256
            startedAtUtc = Format-CanonicalUtc $start
            finishedAtUtc = Format-CanonicalUtc ($start.AddSeconds(60))
        }
        operatorExchange = [ordered]@{
            protocolVersion = 1
            executorSessionRef = $ExecutorSessionRef
            reviewerSessionRef = $ReviewerSessionRef
            independentSessionBoundary = $true
            requestCount = 101
            statusDecisionCount = 1
            freshFrameDecisionCount = 99
            allRequestsCreateOnce = $true
            allResponsesCreateOnce = $true
            everyFrameDependentDecisionBoundToFreshFrame = $true
            everyStatusDecisionBoundToFreshStatus = $true
            responseChainSha256 = [String]::new([char]"7", 64)
            scratchDeleted = $true
        }
        scopedActionApproval = [ordered]@{
            recordSha256 = $ApprovalRecordSha256
            approvalId = $ApprovalRecord.approvalId
            scopeSha256 = $ApprovalRecord.request.scopeSha256
            executorSessionRef = $ExecutorSessionRef
            approvalConfirmedAtUtc = $ApprovalRecord.response.confirmedAtUtc
            approvalExpiresAtUtc = $ApprovalRecord.request.expiresAtUtc
            approvalChallengeFrameRef = $ApprovalRecord.request.challengeFrameRef
            preDispatchFrameRef = $ApprovalRecord.consumption.preDispatchFrameRef
            preDispatchDecisionRef = $ApprovalRecord.consumption.preDispatchDecisionRef
            preDispatchVerifiedAtUtc = $ApprovalRecord.consumption.preDispatchVerifiedAtUtc
            firstCoveredActionSequence = 3
            firstCoveredActionDispatchedAtUtc = $actions[2].dispatchedAtUtc
            consumedBeforeFirstCoveredAction = $true
            consumedBeforeExpiry = $true
            freshStateRevalidatedAfterApproval = $true
            scopeUnchangedThroughRun = $true
        }
        server = [ordered]@{
            executableName = $Candidate.server.name
            sha256 = $Candidate.server.sha256
            processRef = $serverProcessRef
            soleListener = "127.0.0.1:17373"
            updateCheckDisabled = $true
        }
        helper = [ordered]@{
            executableName = $Candidate.computerHelper.name
            sha256 = $Candidate.computerHelper.sha256
            connectedThroughLoopbackServer = $true
            serverApiOnly = $true
            processRef = $helperProcessRef
            sessionBindingSha256 = [String]::new([char]"8", 64)
            rawSessionIdentifierRetained = $false
            topology = [ordered]@{
                exactImageDirectChildCount = 1
                exactImageMatched = $true
                directChildOfLaunchedSupervisor = $true
                interactiveSessionMatched = $true
                stableConsecutivePolls = 2
                helloStateMatched = $true
                protocolRoundTrip = $true
                roundTripMethod = "computer.status"
                supervisorProcessRef = [String]::new([char]"7", 64)
                workerProcessRef = [String]::new([char]"6", 64)
            }
        }
        extensionPayload = [ordered]@{
            fileCount = 11
            combinedPayloadSha256 = $Binding.extractedPayloadSha256
            verifiedBeforeLoad = $true
            verifiedAfterLoad = $true
            verifiedAfterCleanup = $true
        }
        initialState = [ordered]@{
            capturedFromFreshHelperFrames = $true
            developerMode = [ordered]@{
                value = "disabled"
                epochRef = $epochs[1].epochRef
                frameRef = [String]::new([char]"1", 64)
                operatorDecisionRef = [String]::new([char]"a", 64)
                capturedBeforeMutation = $true
            }
            fullAccess = [ordered]@{
                value = "enabled"
                epochRef = $epochs[4].epochRef
                frameRef = [String]::new([char]"2", 64)
                operatorDecisionRef = [String]::new([char]"b", 64)
                capturedBeforeMutation = $true
            }
            savedToken = [ordered]@{
                configured = $false
                epochRef = $epochs[4].epochRef
                frameRef = [String]::new([char]"3", 64)
                operatorDecisionRef = [String]::new([char]"c", 64)
                capturedBeforeMutation = $true
            }
            candidateCard = [ordered]@{
                present = $false
                epochRef = $epochs[1].epochRef
                frameRef = [String]::new([char]"5", 64)
                operatorDecisionRef = [String]::new([char]"d", 64)
                verifiedFromFreshLiveUi = $true
            }
        }
        windowBinding = [ordered]@{
            application = "google-chrome"
            baselineChromeWindowCount = 1
            baselineTargetSetSha256 = [String]::new([char]"4", 64)
            dedicatedTargetRef = $dedicatedTargetRef
            dedicatedProcessRef = $dedicatedProcessRef
            dedicatedAbsentBeforeCreation = $true
            dedicatedCreatedAsOnlyNewChromeWindow = $true
            sameDedicatedTargetAcrossEpochs = $true
        }
        lifecycle = $lifecycle
        windowEpochs = $epochs
        actions = $actions
        browserAction = [ordered]@{
            source = "local-browser-bridge-api"
            apiMatrixRecordSha256 = $ApiMatrixSha256
            targetBindingSha256 = [String]::new([char]"7", 64)
            methodSequence = $script:ComputerHelperBrowserMethods
            requestResponseSha256 = [String]::new([char]"6", 64)
            resultText = "Hello, Bridge Matrix. blue selected."
            resultVerified = $true
        }
        handback = [ordered]@{
            stop = New-PassingHandbackCaseV2SelfTest "in-page-stop" "released_by_user"
            cancel = New-PassingHandbackCaseV2SelfTest "chrome-native-cancel" "canceled_by_user"
        }
        screenshots = $shots
        cleanup = [ordered]@{
            allSharesStopped = $true
            helperTerminationDisposition = "owner-forced-exact-supervisor"
            helperExitCode = 1
            helperDisconnectedAfterTermination = $true
            helperChildrenRemaining = 0
            helperListenersRemaining = 0
            exactHelperImageProcessesRemaining = 0
            serverTerminationDisposition = "owner-forced-exact-process"
            serverExitCode = 1
            serverListenersRemaining = 0
            canonicalPortListenersRemaining = 0
            candidateExtensionRemoved = $true
            candidateCardAbsentAfterRemoval = $true
            candidateCardAbsenceVerifiedFromFreshLiveUi = $true
            savedTokenCleared = $true
            developerModeRestored = $true
            fullAccessRestored = $true
            testWindowClosed = $true
            helperExecutableUnchanged = $true
            serverExecutableUnchanged = $true
            extensionPayloadUnchanged = $true
            rawIdentifiersCleared = $true
        }
        privacy = [ordered]@{
            rawWindowIdsRetained = $false
            rawFrameIdsRetained = $false
            rawShareIdsRetained = $false
            rawSessionIdsRetained = $false
            rawTabIdsRetained = $false
            credentialRetained = $false
            apiResponseBodiesRetained = $false
            opaqueReferenceMapDiscarded = $true
        }
    }
}
function Assert-OperatorV2SelfTestRejected {
    param([object]$Operator, [object]$Binding, [string]$FailureMessage)
    $rejected = $false
    try {
        Assert-OperatorResultsV2 $Operator $script:OperatorV2Version $Binding `
            $script:SelfTestReleaseCandidateBinding $script:SelfTestCandidateV2
    }
    catch { $rejected = $true }
    if (-not $rejected) { throw $FailureMessage }
}

function Assert-HelperV2SelfTestRejected {
    param(
        [object]$Record,
        [object]$Binding,
        [object]$Candidate,
        [object]$Operator,
        [string]$MatrixSha256,
        [string]$FailureMessage
    )
    $rejected = $false
    try {
        Assert-ComputerHelperRecordV2 `
            $Record $Binding $Operator.releaseCandidateBinding $Candidate $Operator $MatrixSha256
    }
    catch { $rejected = $true }
    if (-not $rejected) { throw $FailureMessage }
}

function Invoke-SelfTest {
    $root = [IO.Path]::Combine([IO.Path]::GetTempPath(), "lbb-browser-record-" + [Guid]::NewGuid().ToString("N"))
    [IO.Directory]::CreateDirectory($root) | Out-Null
    try {
        $booleanIntegerRejected = $false
        try { Assert-IntegerRange $true 1 1 "self-test integer" }
        catch { $booleanIntegerRejected = $true }
        if (-not $booleanIntegerRejected) {
            throw "Evidence finalizer accepted a boolean as an integer."
        }
        $undersizedScreenshotRejected = $false
        try { Assert-IntegerRange 1 120 8192 "self-test screenshot width" }
        catch { $undersizedScreenshotRejected = $true }
        if (-not $undersizedScreenshotRejected) {
            throw "Evidence finalizer accepted an undersized screenshot."
        }
        $canonicalTimestamp = "2026-08-24T00:00:00.0000000Z"
        $parsedCanonicalTimestamp = Assert-UtcTimestamp $canonicalTimestamp "self-test timestamp"
        if ($parsedCanonicalTimestamp.Offset -ne [TimeSpan]::Zero -or
            (Format-CanonicalUtc $parsedCanonicalTimestamp) -cne $canonicalTimestamp) {
            throw "Evidence finalizer rejected a canonical Z-suffixed timestamp."
        }
        $offsetTimestampRejected = $false
        try {
            [void](Assert-UtcTimestamp "2026-08-24T00:00:00.0000000+00:00" `
                "self-test offset timestamp")
        }
        catch { $offsetTimestampRejected = $true }
        if (-not $offsetTimestampRejected) {
            throw "Evidence finalizer accepted a noncanonical +00:00 timestamp."
        }
        $decodedPathRejected = $false
        try {
            Assert-SafeSerializedRecord `
                '{"note":"C:\\Users\\Alice\\private.txt"}' @()
        }
        catch { $decodedPathRejected = $true }
        if (-not $decodedPathRejected) {
            throw "Evidence finalizer accepted a decoded Windows home path hidden by JSON escaping."
        }
        $version = "0.12.2"
        $hash = [String]::new([char]"a", 64)
        $runNonce = [String]::new([char]"d", 64)
        $inventory = @()
        foreach ($name in $script:ExtensionFiles) {
            $inventory += [ordered]@{ name = $name; bytes = 1; sha256 = $hash }
        }
        $preflight = [ordered]@{
            schemaVersion = 1; evidenceType = "stock-user-chrome-candidate-binding"; phase = "preflight"
            recordedAtUtc = [DateTime]::UtcNow.ToString("o"); passed = $true; runNonce = $runNonce
            candidate = [ordered]@{
                version = $version; finalSha = [String]::new([char]"b", 40); gitClean = $true
                checksumManifest = [ordered]@{
                    name = "SHA256SUMS.txt"; bytes = 400; sha256 = $hash; externallySuppliedSha256 = $hash
                    canonicalEntryCount = 4
                    canonicalNamesInOrder = @(
                        "local-browser-bridge-v$version-windows-x86_64.exe",
                        "local-computer-helper-v$version-windows-x86_64.exe",
                        "local-browser-bridge-v$version-macos-universal.tar.gz",
                        "local-browser-bridge-extension-v$version.zip"
                    )
                }
                server = [ordered]@{ name = "local-browser-bridge-v$version-windows-x86_64.exe"; bytes = 1000; sha256 = $hash }
                extension = [ordered]@{
                    name = "local-browser-bridge-extension-v$version.zip"; bytes = 1000; sha256 = $hash
                    manifestVersion = $version; libraryVersion = $version; minimumChromeVersion = "140"
                    permissions = $script:Permissions; hostPermissions = $script:HostPermissions; archiveInventory = $inventory
                    extractedPayloadInventory = $inventory; checkoutPayloadInventory = $inventory
                    combinedPayloadSha256 = $hash
                }
            }
        }
        $preflightPath = [IO.Path]::Combine($root, "preflight.json")
        [IO.File]::WriteAllText($preflightPath, (($preflight | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)
        $preflightBinding = Get-CandidateBindingDomain $preflight (Get-Sha256 $preflightPath)

        $checksumManifestV2 = Copy-JsonObject $preflight.candidate.checksumManifest
        $checksumManifestV2.canonicalNamesInOrder = @(
            "local-browser-bridge-v$($script:OperatorV2Version)-windows-x86_64.exe",
            "local-computer-helper-v$($script:OperatorV2Version)-windows-x86_64.exe",
            "local-browser-bridge-v$($script:OperatorV2Version)-macos-universal.tar.gz",
            "local-browser-bridge-extension-v$($script:OperatorV2Version).zip"
        )
        $extensionV2 = Copy-JsonObject $preflight.candidate.extension
        $extensionV2.name = "local-browser-bridge-extension-v$($script:OperatorV2Version).zip"
        $extensionV2.manifestVersion = $script:OperatorV2Version
        $extensionV2.libraryVersion = $script:OperatorV2Version
        $candidateV2 = [ordered]@{
            version = $script:OperatorV2Version
            finalSha = $preflight.candidate.finalSha
            gitClean = $true
            checksumManifest = $checksumManifestV2
            server = [ordered]@{
                name = "local-browser-bridge-v$($script:OperatorV2Version)-windows-x86_64.exe"
                bytes = 1000
                sha256 = $hash
            }
            computerHelper = [ordered]@{
                name = "local-computer-helper-v$($script:OperatorV2Version)-windows-x86_64.exe"
                bytes = 1000
                sha256 = $hash
            }
            extension = $extensionV2
        }
        $releaseCandidateV2 = New-PassingReleaseCandidateBindingV3SelfTest $candidateV2
        $preflightV2 = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-candidate-binding"
            phase = "preflight"
            recordedAtUtc = [DateTime]::UtcNow.ToString("o")
            passed = $true
            runNonce = $preflight.runNonce
            releaseCandidateBinding = $releaseCandidateV2
            candidate = $candidateV2
        }
        $script:SelfTestReleaseCandidateBinding = $releaseCandidateV2
        $script:SelfTestCandidateV2 = $candidateV2
        $preflightV2Path = [IO.Path]::Combine($root, "preflight-v2.json")
        [IO.File]::WriteAllText($preflightV2Path, (($preflightV2 | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)
        $preflightV2Binding = Get-CandidateBindingDomain $preflightV2 (Get-Sha256 $preflightV2Path)
        $initializedV2Path = [IO.Path]::Combine($root, "operator-v2-initialized.json")
        $editableCurrentRejected = $false
        try {
            & $PSCommandPath -Mode InitializeOperator -PreflightRecord $preflightV2Path `
                -OutputRecord $initializedV2Path | Out-Null
        }
        catch { $editableCurrentRejected = $true }
        if (-not $editableCurrentRejected -or [IO.File]::Exists($initializedV2Path)) {
            throw "Evidence finalizer allowed an editable current-schema operator checklist."
        }

        $operatorV2 = New-PassingOperatorV2SelfTest $preflightV2Binding $releaseCandidateV2
        Assert-OperatorResults $operatorV2 $script:OperatorV2Version $preflightV2Binding `
            $releaseCandidateV2 $candidateV2

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.releaseCandidateBinding.workflowRunAttempt = "2"
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted an operator record bound to another workflow attempt."

        $badV2 = Copy-JsonObject $operatorV2
        Add-Member -InputObject $badV2.consentCheckpoints.installCandidate `
            -NotePropertyName actionTimeHumanConfirmed -NotePropertyValue $true
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted an obsolete per-action human-confirmation field."

        $badV2 = Copy-JsonObject $operatorV2
        Add-Member -InputObject $badV2 -NotePropertyName humanVisualReview `
            -NotePropertyValue ([pscustomobject]@{ confirmed = $true })
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted an obsolete human visual-review object."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.consentCheckpoints.installCandidate.approvalRef = [String]::new([char]"f", 64)
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a checkpoint bound to another approval."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.consentCheckpoints.developerModeChange.approvalRef = [String]::new([char]"f", 64)
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a Developer Mode change without action-time consent."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.consentCheckpoints.removeTestOwnedExtension.approvalRef = [String]::new([char]"f", 64)
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted test-copy removal without ownership consent."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.consentCheckpoints.installCandidate.approvalRef = [String]::new([char]"f", 64)
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted candidate installation without ownership consent."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.independentVisualReview.reviewerSessionRef = $badV2.independentVisualReview.executorSessionRef
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted same-session visual review. Evidence finalizer accepted a same-session independent review."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.restoration.developerMode.finalValue = "enabled"
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a Developer Mode restoration mismatch."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.restoration.fullAccess.finalValue = "disabled"
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a Full Access restoration mismatch."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.initialState.candidateExtensionPresent = $true
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted an existing candidate extension identity."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.initialState.savedTokenConfigured = $true
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted an initially configured saved token."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.restoration.savedToken.finalConfigured = $true
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a configured final saved token."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.restoration.candidateExtensionPresence.finalPresent = $true
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a retained test-owned extension identity."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.cleanup.extensionDisposition = "kept-single-enabled-identity"
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a non-removal disposition for the test-owned identity."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.screenshotCaptures[0].captureSurface = "google-chrome-mcp-tab-viewport"
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a browser-chrome screenshot from Chrome MCP."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.screenshotCaptures[1].captureSurface = "windows-computer-use-app-share"
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a screenshot surface inconsistent with the declared capture surface."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.computerHelperChain.chromeExtensionsLoadCompleted = $false
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted an incomplete packaged computer-helper action chain."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.actionSurfaces.chromeMcpUsedDuringBridgeLease = $true
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted Chrome MCP use during an active bridge debugger lease."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.actionSurfaces.chromeMcpReleaseEvidenceClaimed = $true
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a Chrome MCP release-evidence claim."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.actionSurfaces.competingDebuggerAttachmentAllowed = $true
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a competing debugger attachment during the bridge lease."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.cleanup.savedTokenClear.approvalRef = [String]::new([char]"f", 64)
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted clear-token confirmation bound to another approval."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.cleanup.savedTokenClear.confirmationAcceptedByExecutor = $false
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted an incomplete clear-token confirmation."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.consentCheckpoints.restoreCapturedBrowserSettings.performed = $false
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a restoration mismatch under scoped approval."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.cleanup.rawScreenshotScratchDeleted = $false
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted retained raw screenshot scratch data."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.cleanup.pendingReviewRecordsDeleted = $false
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted retained pending-review records."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.cleanup.extractedExtensionInventoryVerifiedBeforeDeletion = $false
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted unverified extracted-extension cleanup."

        $badV2 = Copy-JsonObject $operatorV2
        $badV2.cleanup.extractedExtensionDirectoryDeleted = $false
        Assert-OperatorV2SelfTestRejected $badV2 $preflightV2Binding `
            "Evidence finalizer accepted a retained extracted-extension directory."

        $retainedV2Root = [IO.Path]::Combine($root, "retained-v2")
        [IO.Directory]::CreateDirectory($retainedV2Root) | Out-Null
        $retainedPreflight = [IO.Path]::Combine($retainedV2Root, "candidate-preflight.json")
        $retainedPostflight = [IO.Path]::Combine($retainedV2Root, "candidate-postflight.json")
        $retainedMatrix = [IO.Path]::Combine($retainedV2Root, "browser-api-matrix.json")
        $retainedHelper = [IO.Path]::Combine($retainedV2Root, "browser-computer-helper-chain.json")
        $retainedApproval = [IO.Path]::Combine($retainedV2Root, "scoped-action-approval.json")
        $retainedReview = [IO.Path]::Combine($retainedV2Root, "independent-visual-review.json")
        $retainedExternalPreflight = [IO.Path]::Combine(
            $retainedV2Root, "external-surface-preflight.json"
        )
        $retainedExternalPostflight = [IO.Path]::Combine(
            $retainedV2Root, "external-surface-postflight.json"
        )
        $retainedOperator = [IO.Path]::Combine($retainedV2Root, "operator-results.json")
        foreach ($path in @(
            $retainedPreflight, $retainedPostflight, $retainedMatrix, $retainedHelper,
            $retainedApproval, $retainedReview, $retainedExternalPreflight,
            $retainedExternalPostflight, $retainedOperator
        )) {
            [IO.File]::WriteAllText($path, "{}`n", $script:Utf8NoBom)
        }
        $retainedSidecars = @()
        foreach ($purpose in $script:ExpectedScreenshotsV2.Keys) {
            $imageName = $script:ExpectedScreenshotsV2[$purpose]
            $imagePath = [IO.Path]::Combine($retainedV2Root, $imageName)
            $sidecarPath = [IO.Path]::Combine(
                $retainedV2Root, ([IO.Path]::GetFileNameWithoutExtension($imageName) + ".json")
            )
            [IO.File]::WriteAllBytes($imagePath, [byte[]](1))
            [IO.File]::WriteAllText($sidecarPath, "{}`n", $script:Utf8NoBom)
            $retainedSidecars += $sidecarPath
        }
        $retainedOutput = [IO.Path]::Combine($retainedV2Root, "browser-acceptance.json")
        Assert-RetainedEvidenceDirectoryV2 `
            $retainedPreflight $retainedPostflight $retainedMatrix $retainedHelper $retainedApproval `
            $retainedReview $retainedExternalPreflight $retainedExternalPostflight `
            $retainedOperator $retainedSidecars $retainedOutput
        $unexpectedRetainedPath = [IO.Path]::Combine($retainedV2Root, "raw-screenshot.png")
        [IO.File]::WriteAllBytes($unexpectedRetainedPath, [byte[]](1))
        $unexpectedRetainedRejected = $false
        try {
            Assert-RetainedEvidenceDirectoryV2 `
                $retainedPreflight $retainedPostflight $retainedMatrix $retainedHelper $retainedApproval `
                $retainedReview $retainedExternalPreflight $retainedExternalPostflight `
                $retainedOperator $retainedSidecars $retainedOutput
        }
        catch { $unexpectedRetainedRejected = $true }
        if (-not $unexpectedRetainedRejected) {
            throw "Evidence finalizer accepted an unexpected retained-evidence file."
        }

        $postflight = [ordered]@{
            schemaVersion = 1; evidenceType = "stock-user-chrome-candidate-binding"; phase = "postflight"
            recordedAtUtc = [DateTime]::UtcNow.ToString("o"); passed = $true; runNonce = $runNonce
            candidate = [ordered]@{
                version = $version; finalSha = [String]::new([char]"b", 40); gitClean = $true
                checksumManifest = [ordered]@{
                    name = "SHA256SUMS.txt"; bytes = 400; sha256 = $hash; externallySuppliedSha256 = $hash
                    canonicalEntryCount = 4
                    canonicalNamesInOrder = @(
                        "local-browser-bridge-v$version-windows-x86_64.exe",
                        "local-computer-helper-v$version-windows-x86_64.exe",
                        "local-browser-bridge-v$version-macos-universal.tar.gz",
                        "local-browser-bridge-extension-v$version.zip"
                    )
                }
                server = [ordered]@{ name = "local-browser-bridge-v$version-windows-x86_64.exe"; bytes = 1000; sha256 = $hash }
                extension = [ordered]@{
                    name = "local-browser-bridge-extension-v$version.zip"; bytes = 1000; sha256 = $hash
                    manifestVersion = $version; libraryVersion = $version; minimumChromeVersion = "140"
                    permissions = $script:Permissions; hostPermissions = $script:HostPermissions; archiveInventory = $inventory
                    extractedPayloadInventory = $inventory; checkoutPayloadInventory = $inventory
                    combinedPayloadSha256 = $hash
                }
            }
            candidateBinding = $preflightBinding
            preflightRecordSha256 = Get-Sha256 $preflightPath
            unchanged = [ordered]@{ checkoutHead = $true; checkoutClean = $true; checksumManifest = $true; serverExecutable = $true; extensionZip = $true; extractedPayload = $true }
        }
        $postflightPath = [IO.Path]::Combine($root, "postflight.json")
        [IO.File]::WriteAllText($postflightPath, (($postflight | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)

        $operatorPath = [IO.Path]::Combine($root, "operator.json")
        & $PSCommandPath -Mode InitializeOperator -PreflightRecord $preflightPath -OutputRecord $operatorPath | Out-Null
        $operator = Read-Json $operatorPath "initialized operator checklist"
        $operator.environment.browserVersion = "151.0.7390.0"
        foreach ($name in @("stockUserChrome", "chromeMcpUsed", "manualChromeExtensionsLoad", "developerMode", "dedicatedWindowBoundToOwnedTarget", "localDemoOnly")) { $operator.environment.$name = $true }
        foreach ($name in @("browserLaunchFlagsUsed", "cdpInstallUsed", "automationTestProfileUsed")) { $operator.environment.$name = $false }
        $operator.extension.cardCount = 1
        $operator.extension.idPatternValid = $true
        $operator.extension.enabled = $true
        $operator.extension.loadErrors = 0
        $operator.extension.loadedDirectoryByteMatchesCandidateZip = $true
        $operator.extension.permissionsUiReviewed = $true
        $operator.extension.hostAccessUiReviewed = $true
        foreach ($name in @("popupConnected", "nativeDebuggerWarningSeen", "pageControlPillSeen", "pageStopButtonSeen")) { $operator.extension.$name = $true }
        foreach ($caseName in @("stop", "cancel")) {
            $case = $operator.handback.$caseName
            $case.statusPolledAfterTrigger = $true
            $case.reducedStatus.active = $false
            $case.reducedStatus.humanPaused = $true
            $case.reducedStatus.revocationPending = $false
            foreach ($refusalName in @("controlStartRefusal", "tabMutationRefusal")) {
                $case.$refusalName.httpStatus = 423
                $case.$refusalName.retriable = $false
            }
            $case.indicatorsRemoved = $true
            $case.resume.trustedPopupClick = $true
            $case.resume.statusPolledAfterResume = $true
            $case.resume.reducedStatus.active = $false
            $case.resume.reducedStatus.humanPaused = $false
            $case.resume.reducedStatus.revocationPending = $false
            $case.resume.postResumeStartSucceeded = $true
            $case.resume.activeStatusPolled = $true
            $case.resume.activeStatus.active = $true
            $case.resume.activeStatus.humanPaused = $false
            $case.resume.activeStatus.revocationPending = $false
        }
        foreach ($name in @("controlReleased", "testTabsClosed", "testWindowClosed", "popupDisconnected", "serverStopped", "portReleased", "acceptanceCredentialClearedFromShell", "chromeMcpReleased", "developerModeRestored")) { $operator.cleanup.$name = $true }
        $operator.cleanup.savedTokenClear.trustedPopupClick = $true
        $operator.cleanup.savedTokenClear.popupStateVerifiedAfterClear = $true
        $operator.cleanup.savedTokenClear.tokenConfigured = $false
        $operator.cleanup.savedTokenClear.clearButtonDisabled = $true
        foreach ($name in @("unrelatedTabsOrWindowsChanged", "unrelatedExtensionsChanged", "rawBrowserDataRetained")) { $operator.cleanup.$name = $false }
        $operator.cleanup.extensionDisposition = "kept-single-enabled-identity"
        [IO.File]::WriteAllText($operatorPath, (($operator | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)

        $matrixMethods = @()
        foreach ($method in $script:Methods) {
            $matrixMethods += [ordered]@{
                name = $method; passed = $true; stage = $script:MethodStages[$method]
                commandInvoked = $true; resultVerified = $true; postconditionVerified = $true
                screenshot = $script:MethodScreenshots[$method]
                machineProof = "machine-command-result-postcondition"
            }
        }
        $matrix = [ordered]@{
            schemaVersion = 1; evidenceType = "stock-user-chrome-api-matrix"; version = $version
            target = "loopback-demo"; candidateBinding = $preflightBinding
            passed = $true; methodCount = $script:Methods.Count; methods = $matrixMethods
            assertions = [ordered]@{
                serverVersionMatched = $true; extensionVersionMatched = $true; browserFloorMet = $true
                realExtensionConnected = $true; fullAccessEnabled = $true; capabilitiesComplete = $true; freshCommandIdentity = $true
                freshObservationAfterPageMutation = $true; dynamicTargetDiscovery = $true
                testOwnedTabsOnly = $true; topLayerControlUiIntegrity = $true
                dialogLifecycle = $true; cleanupComplete = $true
            }
        }
        $matrixPath = [IO.Path]::Combine($root, "api-matrix.json")
        [IO.File]::WriteAllText($matrixPath, (($matrix | ConvertTo-Json -Depth 12) + "`n"), $script:Utf8NoBom)

        $png = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAHgAAAAgCAYAAADZubxIAAAATElEQVR42u3RMQ0AAAzDsPLH23/jUfkIgThtT7vFBMACLMACLMACLMCABViABViABViAAQuwAAuwAAuwAAswYAEWYAEWYAEWYMBa6gF5KNhYb/AmigAAAABJRU5ErkJggg==")
        $sidecars = @()
        foreach ($purpose in $script:ExpectedScreenshots.Keys) {
            $imageName = $script:ExpectedScreenshots[$purpose]
            $imagePath = [IO.Path]::Combine($root, $imageName)
            [IO.File]::WriteAllBytes($imagePath, $png)
            $sidecar = [ordered]@{
                schemaVersion = 1; evidenceType = "stock-user-chrome-screenshot"; purpose = $purpose
                candidateBinding = $preflightBinding
                image = [ordered]@{ name = $imageName; bytes = $png.Length; sha256 = Get-Sha256 $imagePath; width = 120; height = 32 }
                cropApplied = $true; metadataStrippedByDecodeAndReencode = $true; forbiddenMetadataChunksPresent = $false
                ocrAvailable = $false; ocrDenylistChecked = $false; ocrDenylistMatches = 0
                manualVisualReviewConfirmed = $true; automaticPixelRedactionPerformed = $false; unknownPixelSafetyClaimed = $false
                reviewStatement = $script:ReviewStatement
            }
            $sidecarPath = [IO.Path]::Combine($root, ([IO.Path]::GetFileNameWithoutExtension($imageName) + ".json"))
            [IO.File]::WriteAllText($sidecarPath, (($sidecar | ConvertTo-Json -Depth 12) + "`n"), $script:Utf8NoBom)
            $sidecars += $sidecarPath
        }
        $outputPath = [IO.Path]::Combine($root, "browser-acceptance.json")
        & $PSCommandPath -Mode Finalize -PreflightRecord $preflightPath -PostflightRecord $postflightPath -ApiMatrixRecord $matrixPath -OperatorResults $operatorPath `
            -ScreenshotRecords $sidecars -OutputRecord $outputPath | Out-Null
        if (-not [IO.File]::Exists($outputPath)) { throw "Evidence finalizer self-test failed." }
        $persisted = [IO.File]::ReadAllText($outputPath, $script:Utf8NoBom)
        if ($persisted.Contains($root) -or $persisted.Contains("REPLACE_WITH")) {
            throw "Evidence finalizer self-test retained a path or placeholder."
        }

        # Exercise the complete v0.12.52 Finalize path with its exact 21-file
        # retained-input inventory, not only the schema-v3 relation validators.
        $finalizeV2Root = [IO.Path]::Combine($root, "finalize-v2")
        [IO.Directory]::CreateDirectory($finalizeV2Root) | Out-Null
        $finalizeV2PreflightPath = [IO.Path]::Combine($finalizeV2Root, "candidate-preflight.json")
        [IO.File]::Copy($preflightV2Path, $finalizeV2PreflightPath)
        $finalizeV2Binding = Get-CandidateBindingDomain $preflightV2 (Get-Sha256 $finalizeV2PreflightPath)

        $postflightV2 = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-candidate-binding"
            phase = "postflight"
            recordedAtUtc = [DateTime]::UtcNow.ToString("o")
            passed = $true
            runNonce = $preflightV2.runNonce
            releaseCandidateBinding = Copy-JsonObject $releaseCandidateV2
            candidate = Copy-JsonObject $preflightV2.candidate
            candidateBinding = [pscustomobject]$finalizeV2Binding
            preflightRecordSha256 = Get-Sha256 $finalizeV2PreflightPath
            unchanged = [ordered]@{
                checkoutHead = $true
                checkoutClean = $true
                checksumManifest = $true
                serverExecutable = $true
                computerHelperExecutable = $true
                extensionZip = $true
                extractedPayload = $true
            }
        }
        $finalizeV2PostflightPath = [IO.Path]::Combine($finalizeV2Root, "candidate-postflight.json")
        [IO.File]::WriteAllText(
            $finalizeV2PostflightPath, (($postflightV2 | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom
        )

        $matrixV2 = Copy-JsonObject $matrix
        $matrixV2.version = $script:OperatorV2Version
        $matrixV2.candidateBinding = [pscustomobject]$finalizeV2Binding
        foreach ($item in $matrixV2.methods) {
            $item.screenshot = $script:MethodScreenshotsV2[$item.name]
        }
        $finalizeV2MatrixPath = [IO.Path]::Combine($finalizeV2Root, "browser-api-matrix.json")
        [IO.File]::WriteAllText(
            $finalizeV2MatrixPath, (($matrixV2 | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom
        )

        # Keep the complete helper/reviewer/postflight fixture in the past so
        # exact postflight ordering does not depend on self-test execution speed.
        $approvalConfirmedAt = [DateTimeOffset]::UtcNow.AddMinutes(-2)
        $executorRef = [String]::new([char]"d", 64)
        $reviewerRef = [String]::new([char]"e", 64)
        $orchestratorRef = [String]::new([char]"6", 64)
        $approvalV3 = New-PassingScopedApprovalV3SelfTest `
            $finalizeV2Binding $releaseCandidateV2 $approvalConfirmedAt $orchestratorRef
        $finalizeV2ApprovalPath = [IO.Path]::Combine(
            $finalizeV2Root, "scoped-action-approval.json"
        )
        [IO.File]::WriteAllText(
            $finalizeV2ApprovalPath, (($approvalV3 | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom
        )
        $helperV2 = New-PassingComputerHelperRecordV2SelfTest `
            $finalizeV2Binding $releaseCandidateV2 $preflightV2.candidate `
            (Get-Sha256 $finalizeV2MatrixPath) $approvalV3 (Get-Sha256 $finalizeV2ApprovalPath) `
            $executorRef $reviewerRef
        $finalizeV2HelperPath = [IO.Path]::Combine(
            $finalizeV2Root, "browser-computer-helper-chain.json"
        )
        [IO.File]::WriteAllText(
            $finalizeV2HelperPath, (($helperV2 | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom
        )

        foreach ($purpose in $script:ExpectedScreenshotsV2.Keys) {
            [IO.File]::WriteAllBytes(
                [IO.Path]::Combine($finalizeV2Root, $script:ExpectedScreenshotsV2[$purpose]), $png
            )
        }
        $pngSha = Get-Sha256 ([IO.Path]::Combine(
            $finalizeV2Root, $script:ExpectedScreenshotsV2["extension-loaded"]
        ))
        $reviewV3 = New-PassingIndependentReviewV3SelfTest `
            $finalizeV2Binding $releaseCandidateV2 $executorRef $reviewerRef $pngSha `
            $approvalConfirmedAt.AddSeconds(61)
        $finalizeV2ReviewPath = [IO.Path]::Combine(
            $finalizeV2Root, "independent-visual-review.json"
        )
        [IO.File]::WriteAllText(
            $finalizeV2ReviewPath, (($reviewV3 | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom
        )
        $reviewSha = Get-Sha256 $finalizeV2ReviewPath
        $releaseCandidateV2Sha = Get-TextSha256 (
            $releaseCandidateV2 | ConvertTo-Json -Depth 20 -Compress
        )
        $externalSurfacePreflightV3 = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-external-surface-attestation"
            phase = "preflight"
            releaseCandidateBinding = Copy-JsonObject $releaseCandidateV2
            releaseCandidateBindingSha256 = $releaseCandidateV2Sha
            orchestrationSurface = "user-orchestrator-secured-ssh-exported-file-review"
            chromeMcpState = "not-used-before-candidate-execution"
            computerUseState = "released-before-candidate-execution"
            reviewerInputState = "review-not-started"
            attestorKind = "orchestrator-agent"
            attestorSessionRef = $orchestratorRef
            attestedAtUtc = Format-CanonicalUtc (
                (Assert-UtcTimestamp $helperV2.run.startedAtUtc "self-test helper start").AddSeconds(-1)
            )
        }
        $externalSurfacePostflightV3 = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-external-surface-attestation"
            phase = "postflight"
            releaseCandidateBinding = Copy-JsonObject $releaseCandidateV2
            releaseCandidateBindingSha256 = $releaseCandidateV2Sha
            orchestrationSurface = "user-orchestrator-secured-ssh-exported-file-review"
            chromeMcpState = "never-used-through-independent-review"
            computerUseState = "not-resumed-through-independent-review"
            reviewerInputState = "exported-digest-bound-files-only"
            attestorKind = "orchestrator-agent"
            attestorSessionRef = $orchestratorRef
            attestedAtUtc = Format-CanonicalUtc (
                (Assert-UtcTimestamp $reviewV3.reviewedAtUtc "self-test review time").AddSeconds(1)
            )
        }
        $finalizeV2ExternalSurfacePreflightPath = [IO.Path]::Combine(
            $finalizeV2Root, "external-surface-preflight.json"
        )
        $finalizeV2ExternalSurfacePostflightPath = [IO.Path]::Combine(
            $finalizeV2Root, "external-surface-postflight.json"
        )
        [IO.File]::WriteAllText(
            $finalizeV2ExternalSurfacePreflightPath,
            (($externalSurfacePreflightV3 | ConvertTo-Json -Depth 30) + "`n"),
            $script:Utf8NoBom
        )
        [IO.File]::WriteAllText(
            $finalizeV2ExternalSurfacePostflightPath,
            (($externalSurfacePostflightV3 | ConvertTo-Json -Depth 30) + "`n"),
            $script:Utf8NoBom
        )
        $externalPreflightValidated = Assert-ExternalSurfaceAttestationV3 `
            $externalSurfacePreflightV3 ([String]::new([char]"1", 64)) `
            $releaseCandidateV2 "preflight"
        $externalPostflightValidated = Assert-ExternalSurfaceAttestationV3 `
            $externalSurfacePostflightV3 ([String]::new([char]"2", 64)) `
            $releaseCandidateV2 "postflight"
        [void](Assert-ExternalSurfaceAttestationPairV3 `
            $externalPreflightValidated $externalPostflightValidated `
            $helperV2 $approvalV3 $reviewV3)
        foreach ($externalMutation in @(
            "wrong-session", "wrong-candidate-digest", "wrong-phase", "stale-preflight",
            "postflight-before-review", "postflight-expired"
        )) {
            $badExternalPreflight = Copy-JsonObject $externalSurfacePreflightV3
            $badExternalPostflight = Copy-JsonObject $externalSurfacePostflightV3
            switch ($externalMutation) {
                "wrong-session" {
                    $badExternalPostflight.attestorSessionRef = [String]::new([char]"7", 64)
                }
                "wrong-candidate-digest" {
                    $badExternalPreflight.releaseCandidateBindingSha256 = `
                        [String]::new([char]"7", 64)
                }
                "wrong-phase" { $badExternalPostflight.phase = "preflight" }
                "stale-preflight" {
                    $badExternalPreflight.attestedAtUtc = Format-CanonicalUtc (
                        (Assert-UtcTimestamp $helperV2.run.startedAtUtc `
                            "self-test helper start").AddMinutes(-31)
                    )
                }
                "postflight-before-review" {
                    $badExternalPostflight.attestedAtUtc = Format-CanonicalUtc (
                        (Assert-UtcTimestamp $reviewV3.reviewedAtUtc `
                            "self-test review time").AddSeconds(-1)
                    )
                }
                "postflight-expired" {
                    $badExternalPostflight.attestedAtUtc = Format-CanonicalUtc (
                        (Assert-UtcTimestamp $reviewV3.reviewedAtUtc `
                            "self-test review time").AddMinutes(31)
                    )
                }
            }
            $badExternalRejected = $false
            try {
                $badExternalPreflightValidated = Assert-ExternalSurfaceAttestationV3 `
                    $badExternalPreflight ([String]::new([char]"3", 64)) `
                    $releaseCandidateV2 "preflight"
                $badExternalPostflightValidated = Assert-ExternalSurfaceAttestationV3 `
                    $badExternalPostflight ([String]::new([char]"4", 64)) `
                    $releaseCandidateV2 "postflight"
                [void](Assert-ExternalSurfaceAttestationPairV3 `
                    $badExternalPreflightValidated $badExternalPostflightValidated `
                    $helperV2 $approvalV3 $reviewV3)
            }
            catch { $badExternalRejected = $true }
            if (-not $badExternalRejected) {
                throw "Evidence finalizer accepted a $externalMutation external-surface attestation pair."
            }
        }
        $finalizeV2Sidecars = @()
        $reviewIndex = 0
        foreach ($purpose in $script:ExpectedScreenshotsV2.Keys) {
            $imageName = $script:ExpectedScreenshotsV2[$purpose]
            $imagePathV2 = [IO.Path]::Combine($finalizeV2Root, $imageName)
            $reviewEntry = $reviewV3.entries[$reviewIndex]
            $sidecarV2 = [ordered]@{
                schemaVersion = 1; evidenceType = "stock-user-chrome-screenshot"; purpose = $purpose
                releaseCandidateBinding = Copy-JsonObject $releaseCandidateV2
                candidateBinding = $finalizeV2Binding
                sourceCapture = [ordered]@{
                    name = [IO.Path]::GetFileNameWithoutExtension($imageName) + ".raw.png"
                    endpoint = "/api/computer/screenshot"; bytes = 2000
                    sha256 = [String]::new([char]"9", 64); width = 320; height = 180
                }
                image = [ordered]@{ name = $imageName; bytes = $png.Length; sha256 = Get-Sha256 $imagePathV2; width = 120; height = 32 }
                cropApplied = $true; metadataStrippedByDecodeAndReencode = $true; forbiddenMetadataChunksPresent = $false
                automatedTextInspectionPerformed = $false
                independentVisualReviewRequired = $true; independentVisualReviewCompleted = $true
                reviewRecordSha256 = $reviewSha
                reviewEntryRef = Get-TextSha256 "$reviewSha`n$($reviewEntry.sequence)`n$($reviewEntry.purpose)`n$($reviewEntry.sha256)"
                automaticPixelRedactionPerformed = $false; unknownPixelSafetyClaimed = $false
                reviewStatement = $script:ReviewStatementV2
            }
            $sidecarPathV2 = [IO.Path]::Combine(
                $finalizeV2Root, ([IO.Path]::GetFileNameWithoutExtension($imageName) + ".json")
            )
            [IO.File]::WriteAllText(
                $sidecarPathV2, (($sidecarV2 | ConvertTo-Json -Depth 12) + "`n"), $script:Utf8NoBom
            )
            $finalizeV2Sidecars += $sidecarPathV2
            $reviewIndex += 1
        }
        $finalizeV2OperatorPath = [IO.Path]::Combine($finalizeV2Root, "operator-results.json")
        & $PSCommandPath -Mode BuildOperator -PreflightRecord $finalizeV2PreflightPath `
            -ComputerHelperRecord $finalizeV2HelperPath -ScopedApprovalRecord $finalizeV2ApprovalPath `
            -IndependentReviewRecord $finalizeV2ReviewPath `
            -ExternalSurfacePreflightAttestation $finalizeV2ExternalSurfacePreflightPath `
            -ExternalSurfacePostflightAttestation $finalizeV2ExternalSurfacePostflightPath `
            -BrowserVersion "151.0.7390.0" `
            -OutputRecord $finalizeV2OperatorPath | Out-Null
        $operatorV2Finalize = Read-Json $finalizeV2OperatorPath "mechanically built operator results"
        $finalizeV2OutputPath = [IO.Path]::Combine($finalizeV2Root, "browser-acceptance.json")
        & $PSCommandPath -Mode Finalize -PreflightRecord $finalizeV2PreflightPath `
            -PostflightRecord $finalizeV2PostflightPath -ApiMatrixRecord $finalizeV2MatrixPath `
            -ComputerHelperRecord $finalizeV2HelperPath -OperatorResults $finalizeV2OperatorPath `
            -ScopedApprovalRecord $finalizeV2ApprovalPath -IndependentReviewRecord $finalizeV2ReviewPath `
            -ExternalSurfacePreflightAttestation $finalizeV2ExternalSurfacePreflightPath `
            -ExternalSurfacePostflightAttestation $finalizeV2ExternalSurfacePostflightPath `
            -ScreenshotRecords $finalizeV2Sidecars `
            -OutputRecord $finalizeV2OutputPath | Out-Null
        $persistedV2 = Read-Json $finalizeV2OutputPath "finalized v0.12.52 self-test record"
        if ($persistedV2.schemaVersion -ne 3 -or $persistedV2.candidate.version -cne $script:OperatorV2Version -or
            $persistedV2.passed -ne $true -or
            @(Get-ChildItem -LiteralPath $finalizeV2Root -Force).Count -ne 22) {
            throw "Evidence finalizer complete v0.12.52 self-test failed."
        }

        $replayedHelperV2 = Copy-JsonObject $helperV2
        $replayedHelperV2.candidateBinding.runNonce = [String]::new([char]"7", 64)
        $helperReplayRejected = $false
        try {
            Assert-ComputerHelperRecordV2 `
                $replayedHelperV2 $finalizeV2Binding $releaseCandidateV2 `
                $preflightV2.candidate $operatorV2Finalize `
                (Get-Sha256 $finalizeV2MatrixPath)
        }
        catch { $helperReplayRejected = $true }
        if (-not $helperReplayRejected) {
            throw "Evidence finalizer accepted a computer-helper record replayed from another run."
        }

        $matrixShaV2 = Get-Sha256 $finalizeV2MatrixPath

        foreach ($requestCountDelta in @(-1, 1)) {
            $badHelperV2 = Copy-JsonObject $helperV2
            $badHelperV2.operatorExchange.requestCount += $requestCountDelta
            Assert-HelperV2SelfTestRejected `
                $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
                $operatorV2Finalize $matrixShaV2 `
                "Evidence finalizer accepted an operator request count missing or adding a request outside status, fresh-frame, and scoped-approval decisions."
        }

        $badApprovalV3 = Copy-JsonObject $approvalV3
        $badApprovalV3.candidateBinding.runNonce = [String]::new([char]"1", 64)
        $badApprovalRejected = $false
        try {
            [void](Assert-ScopedApprovalRecordV3 `
                $badApprovalV3 $finalizeV2Binding $releaseCandidateV2 $preflightV2.candidate)
        }
        catch { $badApprovalRejected = $true }
        if (-not $badApprovalRejected) {
            throw "Evidence finalizer accepted an approval replayed from another candidate run. Evidence finalizer accepted a replayed scoped approval."
        }

        $badApprovalV3 = Copy-JsonObject $approvalV3
        $badApprovalV3.response.deliveredBy = "executor"
        $badApprovalRejected = $false
        try {
            [void](Assert-ScopedApprovalRecordV3 `
                $badApprovalV3 $finalizeV2Binding $releaseCandidateV2 $preflightV2.candidate)
        }
        catch { $badApprovalRejected = $true }
        if (-not $badApprovalRejected) {
            throw "Evidence finalizer accepted approval not delivered by the user-facing orchestrator."
        }

        foreach ($collision in @(
            [pscustomobject]@{
                Ref = $executorRef
                Message = "Evidence finalizer accepted an approval delivered through the executor session."
            },
            [pscustomobject]@{
                Ref = $reviewerRef
                Message = "Evidence finalizer accepted an approval delivered through the reviewer session."
            }
        )) {
            $badApprovalV3 = Copy-JsonObject $approvalV3
            $badApprovalV3.response.orchestratorSessionRef = [string]$collision.Ref
            $badApprovalCrossRejected = $false
            try {
                Assert-ApprovalMatchesHelperV3 `
                    $badApprovalV3 (Get-Sha256 $finalizeV2ApprovalPath) $helperV2 $operatorV2Finalize
            }
            catch { $badApprovalCrossRejected = $true }
            if (-not $badApprovalCrossRejected) {
                throw "Evidence finalizer accepted an orchestrator session colliding with executor or reviewer. Evidence finalizer accepted an approval delivered through the executor session. Evidence finalizer accepted an approval delivered through the reviewer session."
            }
        }

        $badApprovalV3 = Copy-JsonObject $approvalV3
        $swap = $badApprovalV3.request.coveredActions[0]
        $badApprovalV3.request.coveredActions[0] = $badApprovalV3.request.coveredActions[1]
        $badApprovalV3.request.coveredActions[1] = $swap
        $badApprovalRejected = $false
        try {
            [void](Assert-ScopedApprovalRecordV3 `
                $badApprovalV3 $finalizeV2Binding $releaseCandidateV2 $preflightV2.candidate)
        }
        catch { $badApprovalRejected = $true }
        if (-not $badApprovalRejected) {
            throw "Evidence finalizer accepted a reordered approval scope."
        }

        foreach ($reviewMutation in @("same-session", "reordered", "uncertain", "sensitive")) {
            $badReviewV3 = Copy-JsonObject $reviewV3
            switch ($reviewMutation) {
                "same-session" { $badReviewV3.reviewerSessionRef = $badReviewV3.executorSessionRef }
                "reordered" {
                    $entrySwap = $badReviewV3.entries[0]
                    $badReviewV3.entries[0] = $badReviewV3.entries[1]
                    $badReviewV3.entries[1] = $entrySwap
                }
                "uncertain" { $badReviewV3.entries[0].uncertain = $true }
                "sensitive" { $badReviewV3.entries[0].sensitivePixelsObserved = $true }
            }
            $badReviewRejected = $false
            try {
                [void](Assert-IndependentReviewRecordV3 `
                    $badReviewV3 $finalizeV2Binding $releaseCandidateV2 $preflightV2.candidate)
            }
            catch { $badReviewRejected = $true }
            if (-not $badReviewRejected) {
                throw "Evidence finalizer accepted a $reviewMutation independent-review record. Evidence finalizer accepted reordered independent review entries. Evidence finalizer accepted an uncertain independent review. Evidence finalizer accepted sensitive pixels in independent review."
            }
        }

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.actions[2].dispatchedAtUtc = $badHelperV2.scopedActionApproval.approvalConfirmedAtUtc
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted approval at or after the first covered action dispatch. Evidence finalizer accepted a scoped approval at or after the first covered action."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.scopedActionApproval.approvalExpiresAtUtc = Format-CanonicalUtc (
            $approvalConfirmedAt.AddSeconds(1)
        )
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted an approval expired before the first covered action."

        $badApprovalV3 = Copy-JsonObject $approvalV3
        $badApprovalV3.consumption.consumedBeforeExpiry = $false
        $expiredConsumptionRejected = $false
        try {
            [void](Assert-ScopedApprovalRecordV3 `
                $badApprovalV3 $finalizeV2Binding $releaseCandidateV2 $preflightV2.candidate)
        }
        catch { $expiredConsumptionRejected = $true }
        if (-not $expiredConsumptionRejected) {
            throw "Evidence finalizer accepted a false approval-before-expiry claim."
        }

        $badApprovalV3 = Copy-JsonObject $approvalV3
        $badApprovalV3.consumption.preDispatchFrameRef = `
            $badApprovalV3.request.challengeFrameRef
        $reusedApprovalFrameRejected = $false
        try {
            [void](Assert-ScopedApprovalRecordV3 `
                $badApprovalV3 $finalizeV2Binding $releaseCandidateV2 $preflightV2.candidate)
        }
        catch { $reusedApprovalFrameRejected = $true }
        if (-not $reusedApprovalFrameRejected) {
            throw "Evidence finalizer accepted reuse of the approval challenge frame for pre-dispatch revalidation."
        }

        $badApprovalV3 = Copy-JsonObject $approvalV3
        $badApprovalV3.consumption.freshStateRevalidatedAfterApproval = $false
        $changedApprovalStateRejected = $false
        try {
            [void](Assert-ScopedApprovalRecordV3 `
                $badApprovalV3 $finalizeV2Binding $releaseCandidateV2 $preflightV2.candidate)
        }
        catch { $changedApprovalStateRejected = $true }
        if (-not $changedApprovalStateRejected) {
            throw "Evidence finalizer accepted changed state after scoped approval."
        }

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.scopedActionApproval.preDispatchVerifiedAtUtc = `
            $badHelperV2.scopedActionApproval.firstCoveredActionDispatchedAtUtc
        $badHelperV2.scopedActionApproval.preDispatchFrameRef = `
            $badHelperV2.scopedActionApproval.approvalChallengeFrameRef
        Assert-HelperV2SelfTestRejected `
            $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted an aged or reused-frame pre-dispatch approval revalidation."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.actions[1].operatorDecisionRef = $badHelperV2.actions[0].operatorDecisionRef
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a replayed operator decision. Evidence finalizer accepted a reused operator decision reference."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.actions[1].dispatchedAtUtc = $badHelperV2.run.startedAtUtc
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted reordered or duplicated action timing."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.actions[0].source = "human-on-stock-chrome"
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a human-authored helper action source."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.run.apiMatrixRecordSha256 = [String]::new([char]"5", 64)
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a helper record from another API-matrix run."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.lifecycle[1].connectionState = "connected"
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a helper already connected before the run."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.cleanup.helperDisconnectedAfterTermination = $false
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a helper without a verified disconnect."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.cleanup.helperExitCode = 0
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a graceful-zero claim for forced helper termination."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.windowEpochs[2].shareStopSucceeded = $false
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted an open or failed helper share epoch."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.actions[11].postFrameRef = $badHelperV2.actions[11].preFrameRef
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a stale computer-action frame."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.actions[11].methods = @("computer.observe", "computer.observe", "computer.observe")
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a missing computer.click proof action."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.actions[11].httpStatus = 500
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a failed helper API response."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.screenshots[0].source = "windows-computer-use-app-share"
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a non-product screenshot source."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.extensionPayload.combinedPayloadSha256 = [String]::new([char]"4", 64)
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a different unpacked extension payload."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.initialState.developerMode.value = "enabled"
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted operator initial state not bound to the fresh helper frame."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.actions[4].methods = @("computer.observe", "computer.typeText", "computer.observe")
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a picker action without a closed-target status proof."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.windowEpochs[3].targetRef = [String]::new([char]"5", 64)
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a different dedicated Chrome window in a later epoch."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.windowEpochs[5].application = "another-application"
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a non-Chrome product-control epoch."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.windowBinding.dedicatedCreatedAsOnlyNewChromeWindow = $false
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a dedicated window without a sole-new status delta."

        $badHelperV2 = Copy-JsonObject $helperV2
        $badHelperV2.cleanup.helperExecutableUnchanged = $false
        Assert-HelperV2SelfTestRejected $badHelperV2 $finalizeV2Binding $preflightV2.candidate `
            $operatorV2Finalize $matrixShaV2 `
            "Evidence finalizer accepted a mutated helper executable."

        $unboundPostflight = ConvertFrom-JsonPreservingStrings `
            ($postflight | ConvertTo-Json -Depth 30 -Compress)
        $unboundPostflight.preflightRecordSha256 = [String]::new([char]"c", 64)
        $unboundPostflightPath = [IO.Path]::Combine($root, "unbound-postflight.json")
        [IO.File]::WriteAllText($unboundPostflightPath, (($unboundPostflight | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)
        $unboundOutputPath = [IO.Path]::Combine($root, "unbound-acceptance.json")
        $unboundRejected = $false
        try {
            & $PSCommandPath -Mode Finalize -PreflightRecord $preflightPath -PostflightRecord $unboundPostflightPath `
                -ApiMatrixRecord $matrixPath -OperatorResults $operatorPath -ScreenshotRecords $sidecars `
                -OutputRecord $unboundOutputPath | Out-Null
        }
        catch { $unboundRejected = $true }
        if (-not $unboundRejected -or [IO.File]::Exists($unboundOutputPath)) {
            throw "Evidence finalizer accepted a postflight that did not bind the supplied preflight."
        }

        $otherRunNonce = [String]::new([char]"9", 64)
        $otherMatrix = ConvertFrom-JsonPreservingStrings `
            ($matrix | ConvertTo-Json -Depth 30 -Compress)
        $otherMatrix.candidateBinding.runNonce = $otherRunNonce
        $otherMatrixPath = [IO.Path]::Combine($root, "other-run-matrix.json")
        [IO.File]::WriteAllText($otherMatrixPath, (($otherMatrix | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)
        $otherMatrixOutput = [IO.Path]::Combine($root, "other-run-matrix-output.json")
        $otherMatrixRejected = $false
        try {
            & $PSCommandPath -Mode Finalize -PreflightRecord $preflightPath -PostflightRecord $postflightPath `
                -ApiMatrixRecord $otherMatrixPath -OperatorResults $operatorPath -ScreenshotRecords $sidecars `
                -OutputRecord $otherMatrixOutput | Out-Null
        }
        catch { $otherMatrixRejected = $true }
        if (-not $otherMatrixRejected -or [IO.File]::Exists($otherMatrixOutput)) {
            throw "Evidence finalizer accepted an API matrix from another candidate run."
        }

        $otherOperator = ConvertFrom-JsonPreservingStrings `
            ($operator | ConvertTo-Json -Depth 30 -Compress)
        $otherOperator.candidateBinding.preflightRecordSha256 = [String]::new([char]"8", 64)
        $otherOperatorPath = [IO.Path]::Combine($root, "other-run-operator.json")
        [IO.File]::WriteAllText($otherOperatorPath, (($otherOperator | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)
        $otherOperatorOutput = [IO.Path]::Combine($root, "other-run-operator-output.json")
        $otherOperatorRejected = $false
        try {
            & $PSCommandPath -Mode Finalize -PreflightRecord $preflightPath -PostflightRecord $postflightPath `
                -ApiMatrixRecord $matrixPath -OperatorResults $otherOperatorPath -ScreenshotRecords $sidecars `
                -OutputRecord $otherOperatorOutput | Out-Null
        }
        catch { $otherOperatorRejected = $true }
        if (-not $otherOperatorRejected -or [IO.File]::Exists($otherOperatorOutput)) {
            throw "Evidence finalizer accepted operator results from another candidate run."
        }

        $otherSidecar = Read-Json $sidecars[0] "self-test screenshot sidecar"
        $otherSidecar.candidateBinding.extensionZipSha256 = [String]::new([char]"7", 64)
        $otherSidecarDirectory = [IO.Path]::Combine($root, "other-run-screenshot")
        [IO.Directory]::CreateDirectory($otherSidecarDirectory) | Out-Null
        $otherSidecarPath = [IO.Path]::Combine($otherSidecarDirectory, "browser-01-extensions-card.json")
        [IO.File]::Copy(
            [IO.Path]::Combine($root, "browser-01-extensions-card.png"),
            [IO.Path]::Combine($otherSidecarDirectory, "browser-01-extensions-card.png")
        )
        [IO.File]::WriteAllText($otherSidecarPath, (($otherSidecar | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)
        $otherSidecars = @($sidecars)
        $otherSidecars[0] = $otherSidecarPath
        $otherScreenshotOutput = [IO.Path]::Combine($root, "other-run-screenshot-output.json")
        $otherScreenshotRejected = $false
        try {
            & $PSCommandPath -Mode Finalize -PreflightRecord $preflightPath -PostflightRecord $postflightPath `
                -ApiMatrixRecord $matrixPath -OperatorResults $operatorPath -ScreenshotRecords $otherSidecars `
                -OutputRecord $otherScreenshotOutput | Out-Null
        }
        catch { $otherScreenshotRejected = $true }
        if (-not $otherScreenshotRejected -or [IO.File]::Exists($otherScreenshotOutput)) {
            throw "Evidence finalizer accepted a screenshot from another candidate run."
        }

        $missingPostcondition = ConvertFrom-JsonPreservingStrings `
            ($matrix | ConvertTo-Json -Depth 30 -Compress)
        $missingPostcondition.methods[0].postconditionVerified = $false
        $missingPostconditionPath = [IO.Path]::Combine($root, "matrix-missing-postcondition.json")
        [IO.File]::WriteAllText($missingPostconditionPath, (($missingPostcondition | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)
        $missingPostconditionOutput = [IO.Path]::Combine($root, "matrix-missing-postcondition-output.json")
        $missingPostconditionRejected = $false
        try {
            & $PSCommandPath -Mode Finalize -PreflightRecord $preflightPath -PostflightRecord $postflightPath `
                -ApiMatrixRecord $missingPostconditionPath -OperatorResults $operatorPath -ScreenshotRecords $sidecars `
                -OutputRecord $missingPostconditionOutput | Out-Null
        }
        catch { $missingPostconditionRejected = $true }
        if (-not $missingPostconditionRejected -or [IO.File]::Exists($missingPostconditionOutput)) {
            throw "Evidence finalizer accepted a matrix method without a verified postcondition."
        }

        $wrongScreenshot = ConvertFrom-JsonPreservingStrings `
            ($matrix | ConvertTo-Json -Depth 30 -Compress)
        $wrongScreenshot.methods[0].screenshot = "N/A"
        $wrongScreenshotPath = [IO.Path]::Combine($root, "matrix-wrong-screenshot.json")
        [IO.File]::WriteAllText($wrongScreenshotPath, (($wrongScreenshot | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8NoBom)
        $wrongScreenshotOutput = [IO.Path]::Combine($root, "matrix-wrong-screenshot-output.json")
        $wrongScreenshotRejected = $false
        try {
            & $PSCommandPath -Mode Finalize -PreflightRecord $preflightPath -PostflightRecord $postflightPath `
                -ApiMatrixRecord $wrongScreenshotPath -OperatorResults $operatorPath -ScreenshotRecords $sidecars `
                -OutputRecord $wrongScreenshotOutput | Out-Null
        }
        catch { $wrongScreenshotRejected = $true }
        if (-not $wrongScreenshotRejected -or [IO.File]::Exists($wrongScreenshotOutput)) {
            throw "Evidence finalizer accepted a noncanonical matrix screenshot mapping."
        }
        Write-Output "Browser evidence finalizer self-test passed."
    }
    finally {
        if ([IO.Directory]::Exists($root)) { [IO.Directory]::Delete($root, $true) }
    }
}

switch ($Mode) {
    "InitializeOperator" { Invoke-InitializeOperator }
    "BuildOperator" { Invoke-BuildOperator }
    "Finalize" { Invoke-Finalize }
    "SelfTest" { Invoke-SelfTest }
}
