#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("InitializeOperator", "Finalize", "SelfTest")]
    [string]$Mode,
    [string]$PreflightRecord,
    [string]$PostflightRecord,
    [string]$ApiMatrixRecord,
    [string]$OperatorResults,
    [string[]]$ScreenshotRecords,
    [string]$DenyValuesFile,
    [string]$OutputRecord
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$script:Utf8NoBom = [Text.UTF8Encoding]::new($false, $true)
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
$script:ReviewStatement = "A human reviewed this tight crop; OCR is supplemental and unknown sensitive pixels are not automatically redacted."

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
    return $resolved
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
    return $resolved
}

function Read-Json {
    param([string]$Path, [string]$Label)
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -le 0 -or $bytes.Length -gt 2MB) {
        throw "$Label has an invalid size."
    }
    try {
        return $script:Utf8NoBom.GetString($bytes) | ConvertFrom-Json
    }
    catch {
        throw "$Label is not strict UTF-8 JSON."
    }
}

function Get-Sha256 {
    param([string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-ExactKeys {
    param([object]$Object, [string[]]$Expected, [string]$Label)
    if ($null -eq $Object) {
        throw "$Label must be an object."
    }
    $actual = @($Object.PSObject.Properties.Name | Sort-Object)
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
    $actual = @($Object.PSObject.Properties.Name)
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
    Assert-ExactKeys $candidate @("version", "finalSha", "gitClean", "checksumManifest", "server", "extension") "candidate binding"
    if ($candidate.version -isnot [string] -or -not [regex]::IsMatch($candidate.version, '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')) {
        throw "Candidate version must be stable SemVer."
    }
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

function Assert-CandidatePreflight {
    param([object]$Record)
    Assert-ExactKeys $Record @(
        "schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed", "runNonce", "candidate"
    ) "candidate preflight record"
    Assert-IntegerRange $Record.schemaVersion 1 1 "candidate preflight schemaVersion"
    if ($Record.evidenceType -cne "stock-user-chrome-candidate-binding" -or $Record.phase -cne "preflight") {
        throw "Candidate preflight identity is invalid."
    }
    Assert-ExactBoolean $Record.passed $true "candidate preflight passed"
    Assert-Hex $Record.runNonce 64 "candidate preflight run nonce"
    return Assert-CandidateBinding $Record.candidate
}

function Assert-CandidatePostflight {
    param([object]$Record)
    Assert-ExactKeys $Record @(
        "schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed", "runNonce", "candidate",
        "candidateBinding", "preflightRecordSha256", "unchanged"
    ) "candidate postflight record"
    Assert-IntegerRange $Record.schemaVersion 1 1 "candidate postflight schemaVersion"
    if ($Record.evidenceType -cne "stock-user-chrome-candidate-binding" -or $Record.phase -cne "postflight") {
        throw "Candidate postflight identity is invalid."
    }
    Assert-ExactBoolean $Record.passed $true "candidate postflight passed"
    Assert-Hex $Record.runNonce 64 "candidate postflight run nonce"
    Assert-Hex $Record.preflightRecordSha256 64 "preflight record SHA-256"
    Assert-ExactKeys $Record.unchanged @(
        "checkoutHead", "checkoutClean", "checksumManifest", "serverExecutable", "extensionZip", "extractedPayload"
    ) "candidate unchanged assertions"
    foreach ($property in $Record.unchanged.PSObject.Properties) {
        Assert-ExactBoolean $property.Value $true "candidate unchanged $($property.Name)"
    }
    return Assert-CandidateBinding $Record.candidate
}

function Get-CandidateBindingDomain {
    param([object]$Preflight, [string]$PreflightSha256)
    return [ordered]@{
        runNonce = [string]$Preflight.runNonce
        preflightRecordSha256 = $PreflightSha256
        finalSha = [string]$Preflight.candidate.finalSha
        checksumManifestSha256 = [string]$Preflight.candidate.checksumManifest.sha256
        serverSha256 = [string]$Preflight.candidate.server.sha256
        extensionZipSha256 = [string]$Preflight.candidate.extension.sha256
        extractedPayloadSha256 = [string]$Preflight.candidate.extension.combinedPayloadSha256
    }
}

function Assert-CandidateBindingDomain {
    param([object]$Binding, [object]$Expected, [string]$Label)
    $fields = @(
        "runNonce", "preflightRecordSha256", "finalSha", "checksumManifestSha256",
        "serverSha256", "extensionZipSha256", "extractedPayloadSha256"
    )
    Assert-ExactKeys $Binding $fields $Label
    Assert-ExactPropertyOrder $Binding $fields $Label
    Assert-Hex $Binding.runNonce 64 "$Label run nonce"
    Assert-Hex $Binding.preflightRecordSha256 64 "$Label preflight record SHA-256"
    Assert-Hex $Binding.finalSha 40 "$Label FINAL_SHA"
    foreach ($name in @("checksumManifestSha256", "serverSha256", "extensionZipSha256", "extractedPayloadSha256")) {
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
        if ($item.screenshot -cne $script:MethodScreenshots[$item.name]) {
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

function Assert-OperatorResults {
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

function Get-PngFacts {
    param([string]$Path)
    $bytes = [IO.File]::ReadAllBytes($Path)
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
    return [pscustomobject]@{ Width = [long]$width; Height = [long]$height }
}

function Assert-ScreenshotRecords {
    param([string[]]$Paths, [object]$ExpectedBinding)
    if ($Paths.Count -ne $script:ExpectedScreenshots.Count) {
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
        Assert-ExactKeys $record $screenshotFields "screenshot record"
        Assert-ExactPropertyOrder $record $screenshotFields "screenshot record"
        Assert-IntegerRange $record.schemaVersion 1 1 "screenshot schemaVersion"
        if ($record.evidenceType -cne "stock-user-chrome-screenshot" -or -not $script:ExpectedScreenshots.Contains($record.purpose) -or $safe.Contains($record.purpose)) {
            throw "Screenshot purpose is invalid or duplicated."
        }
        Assert-CandidateBindingDomain $record.candidateBinding $ExpectedBinding "screenshot candidateBinding"
        Assert-ExactKeys $record.image @("name", "bytes", "sha256", "width", "height") "screenshot image"
        $expectedName = $script:ExpectedScreenshots[$record.purpose]
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
        foreach ($name in @("cropApplied", "metadataStrippedByDecodeAndReencode", "manualVisualReviewConfirmed")) {
            Assert-ExactBoolean $record.$name $true "screenshot $name"
        }
        foreach ($name in @("forbiddenMetadataChunksPresent", "automaticPixelRedactionPerformed", "unknownPixelSafetyClaimed")) {
            Assert-ExactBoolean $record.$name $false "screenshot $name"
        }
        Assert-IntegerRange $record.ocrDenylistMatches 0 0 "screenshot OCR denylist matches"
        if ($record.ocrAvailable -isnot [bool] -or $record.ocrDenylistChecked -isnot [bool] -or $record.ocrDenylistChecked -ne $record.ocrAvailable -or $record.reviewStatement -cne $script:ReviewStatement) {
            throw "Screenshot review contract is invalid."
        }
        $imagePath = Resolve-RequiredFile ([IO.Path]::Combine([IO.Path]::GetDirectoryName($recordPath), $record.image.name)) "sanitized screenshot"
        $pngFacts = Get-PngFacts $imagePath
        if ($pngFacts.Width -ne [long]$record.image.width -or $pngFacts.Height -ne [long]$record.image.height) {
            throw "Sanitized screenshot dimensions do not match their sidecar."
        }
        if (([IO.FileInfo]::new($imagePath)).Length -ne [int64]$record.image.bytes -or (Get-Sha256 $imagePath) -cne $record.image.sha256) {
            throw "Sanitized screenshot bytes do not match their sidecar."
        }
        $safe[$record.purpose] = $record
    }
    foreach ($purpose in $script:ExpectedScreenshots.Keys) {
        if (-not $safe.Contains($purpose)) {
            throw "A required screenshot purpose is missing."
        }
    }
    $ordered = @()
    foreach ($purpose in $script:ExpectedScreenshots.Keys) {
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
        [IO.File]::WriteAllText($temporary, "$json`n", $script:Utf8NoBom)
        [IO.File]::Move($temporary, $Path)
    }
    catch {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
        throw
    }
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
    $preflight = Read-Json $preflightPath "PreflightRecord"
    $preflightCandidate = Assert-CandidatePreflight $preflight
    $preflightSha256 = Get-Sha256 $preflightPath
    $candidateBinding = Get-CandidateBindingDomain $preflight $preflightSha256
    $postflight = Read-Json $postflightPath "PostflightRecord"
    $candidate = Assert-CandidatePostflight $postflight
    if ($preflightSha256 -cne $postflight.preflightRecordSha256 -or
        $preflight.runNonce -cne $postflight.runNonce) {
        throw "Candidate postflight does not bind the supplied preflight record."
    }
    if (($preflightCandidate | ConvertTo-Json -Depth 20 -Compress) -cne
        ($candidate | ConvertTo-Json -Depth 20 -Compress)) {
        throw "Candidate preflight and postflight bindings differ."
    }
    Assert-CandidateBindingDomain $postflight.candidateBinding $candidateBinding "candidate postflight candidateBinding"
    $matrix = Read-Json $matrixPath "ApiMatrixRecord"
    Assert-ApiMatrixRecord $matrix $candidate.version $candidateBinding
    $operator = Read-Json $operatorPath "OperatorResults"
    Assert-OperatorResults $operator $candidate.version $candidateBinding
    $screenshots = @(Assert-ScreenshotRecords $ScreenshotRecords $candidateBinding)
    $record = [ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-acceptance"
        recordedAtUtc = [DateTime]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        passed = $true
        candidateBinding = $candidateBinding
        candidate = [ordered]@{
            version = $candidate.version
            finalSha = $candidate.finalSha
            preflightRecordSha256 = $preflightSha256
            postflightRecordSha256 = Get-Sha256 $postflightPath
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
        environment = $operator.environment
        extension = $operator.extension
        apiMatrixRecordSha256 = Get-Sha256 $matrixPath
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
    $templatePath = [IO.Path]::GetFullPath([IO.Path]::Combine(
        $PSScriptRoot, "..", "evidence", "v0.12.2", "browser", "operator-results.template.json"
    ))
    $operator = Read-Json $templatePath "operator template"
    Assert-ExactKeys $operator @(
        "schemaVersion", "evidenceType", "candidateBinding", "environment", "extension", "handback", "cleanup"
    ) "operator template"
    $operator.candidateBinding = [pscustomobject]$binding
    Write-NewJson $outputPath $operator
    Write-Output "Candidate-bound operator checklist was initialized."
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

        $unboundPostflight = $postflight | ConvertTo-Json -Depth 30 -Compress | ConvertFrom-Json
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
        $otherMatrix = $matrix | ConvertTo-Json -Depth 30 -Compress | ConvertFrom-Json
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

        $otherOperator = $operator | ConvertTo-Json -Depth 30 -Compress | ConvertFrom-Json
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

        $missingPostcondition = $matrix | ConvertTo-Json -Depth 30 -Compress | ConvertFrom-Json
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

        $wrongScreenshot = $matrix | ConvertTo-Json -Depth 30 -Compress | ConvertFrom-Json
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
    "Finalize" { Invoke-Finalize }
    "SelfTest" { Invoke-SelfTest }
}
