#requires -Version 5.1

<#
.SYNOPSIS
Runs the real-extension browser API acceptance matrix against the loopback demo.

.DESCRIPTION
Run this only after the frozen candidate server is listening, the exact
unpacked extension is connected in stock Google Chrome with Full Access, and
the operator has opened exactly one dedicated loopback `/demo` tab through the
Chrome control connector. `ConfirmPreopenedDemoIsTestOwned` is an explicit
ownership attestation for API actions. The runner deliberately leaves that
demo tab open for the later visual Stop/Cancel evidence flow; it closes only
the secondary bridge-created blank tab used by the tab-lifecycle checks.
The bearer is accepted only from the current process's LBB_TOKEN environment
variable and is removed from that process in the final cleanup boundary.

The output is a deliberately reduced, allowlisted JSON record. It contains no
raw response, browser content, identifier, URL, coordinate, credential, path,
or terminal data. Candidate-file binding belongs to the separate frozen-
candidate preflight; this driver independently checks the API-reported server
and extension version before it touches Chrome.
#>

[CmdletBinding()]
param(
    [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+$')]
    [string]$Version = "",

    [ValidateRange(1, 65535)]
    [int]$Port = 17373,

    [string]$OutputPath = "",

    [switch]$ConfirmPreopenedDemoIsTestOwned,

    [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$ActionMethods = @(
    "status",
    "browser.control.start",
    "browser.control.status",
    "browser.control.stop",
    "tabs.list",
    "tabs.activate",
    "tabs.new",
    "tabs.close",
    "page.observe",
    "page.navigate",
    "page.back",
    "page.forward",
    "page.reload",
    "page.click",
    "page.fill",
    "page.select",
    "page.key",
    "page.scroll",
    "page.clickAt",
    "page.typeText",
    "page.evaluate",
    "page.waitFor",
    "page.hover",
    "page.batch",
    "page.handleDialog"
)

$MethodStages = [ordered]@{
    "status"                 = "preflight"
    "browser.control.start"  = "control"
    "browser.control.status" = "control"
    "browser.control.stop"   = "cleanup"
    "tabs.list"              = "tab-lifecycle"
    "tabs.activate"          = "tab-lifecycle"
    "tabs.new"               = "tab-lifecycle"
    "tabs.close"             = "cleanup"
    "page.observe"           = "freshness"
    "page.navigate"          = "navigation"
    "page.back"              = "navigation"
    "page.forward"           = "navigation"
    "page.reload"            = "navigation"
    "page.click"             = "interaction"
    "page.fill"              = "interaction"
    "page.select"            = "interaction"
    "page.key"               = "interaction"
    "page.scroll"            = "interaction"
    "page.clickAt"            = "interaction"
    "page.typeText"          = "interaction"
    "page.evaluate"          = "inspection"
    "page.waitFor"           = "inspection"
    "page.hover"             = "interaction"
    "page.batch"             = "interaction"
    "page.handleDialog"      = "dialog"
}

$AssertionNames = @(
    "serverVersionMatched",
    "extensionVersionMatched",
    "browserFloorMet",
    "realExtensionConnected",
    "fullAccessEnabled",
    "capabilitiesComplete",
    "freshCommandIdentity",
    "freshObservationAfterPageMutation",
    "dynamicTargetDiscovery",
    "testOwnedTabsOnly",
    "dialogLifecycle",
    "cleanupComplete"
)

$MethodPassed = [ordered]@{}
foreach ($method in $ActionMethods) {
    $MethodPassed[$method] = $false
}

$AggregateAssertions = [ordered]@{}
foreach ($name in $AssertionNames) {
    $AggregateAssertions[$name] = $false
}

$script:AcceptanceToken = $null
$script:CommandUri = $null
$script:HealthUri = $null
$script:SeenCallIds = New-Object 'System.Collections.Generic.HashSet[string]'
$script:CommandCallCount = 0
$script:OwnedTabs = New-Object 'System.Collections.Generic.HashSet[long]'
$script:ClosableTabs = New-Object 'System.Collections.Generic.HashSet[long]'
$script:CurrentObservation = $null
$script:LastGeneration = $null
$script:PageMutationCount = 0
$script:ObservedPageMutationCount = 0
$script:ControlMayBeActive = $false
$script:ActiveStage = "preflight"

function Assert-Acceptance {
    param(
        [Parameter(Mandatory = $true)][bool]$Condition,
        [Parameter(Mandatory = $true)][string]$Assertion
    )
    if (-not $Condition) {
        throw [InvalidOperationException]::new("Acceptance assertion failed: $Assertion")
    }
}

function Assert-ExactPropertyOrder {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string[]]$Expected,
        [Parameter(Mandatory = $true)][string]$Boundary
    )
    $actual = @($Value.PSObject.Properties | ForEach-Object { $_.Name })
    Assert-Acceptance ($actual.Count -eq $Expected.Count) "$Boundary-property-count"
    for ($index = 0; $index -lt $Expected.Count; $index += 1) {
        Assert-Acceptance ($actual[$index] -ceq $Expected[$index]) "$Boundary-property-order"
    }
}

function Get-ReducedEvidenceRecord {
    param([Parameter(Mandatory = $true)][bool]$Passed)

    $methods = @()
    foreach ($method in $ActionMethods) {
        $methods += [pscustomobject][ordered]@{
            name   = $method
            passed = [bool]$MethodPassed[$method]
            stage  = [string]$MethodStages[$method]
        }
    }

    $assertions = [ordered]@{}
    foreach ($name in $AssertionNames) {
        $assertions[$name] = [bool]$AggregateAssertions[$name]
    }

    return [pscustomobject][ordered]@{
        schemaVersion = 1
        evidenceType  = "stock-user-chrome-api-matrix"
        version       = $Version
        target        = "loopback-demo"
        passed        = $Passed
        methodCount   = 25
        methods       = $methods
        assertions    = [pscustomobject]$assertions
    }
}

function Assert-ReducedEvidenceRecord {
    param([Parameter(Mandatory = $true)]$Record)

    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "evidenceType", "version", "target", "passed",
        "methodCount", "methods", "assertions"
    ) "record"
    Assert-Acceptance ($Record.schemaVersion -eq 1) "record-schema"
    Assert-Acceptance ($Record.evidenceType -ceq "stock-user-chrome-api-matrix") "record-type"
    Assert-Acceptance ($Record.version -match '^[0-9]+\.[0-9]+\.[0-9]+$') "record-version"
    Assert-Acceptance ($Record.target -ceq "loopback-demo") "record-target"
    Assert-Acceptance ($Record.passed -is [bool]) "record-passed-type"
    Assert-Acceptance ($Record.methodCount -eq 25) "record-method-count"

    $methods = @($Record.methods)
    Assert-Acceptance ($methods.Count -eq $ActionMethods.Count) "record-method-array"
    $derivedPass = $true
    for ($index = 0; $index -lt $ActionMethods.Count; $index += 1) {
        $entry = $methods[$index]
        Assert-ExactPropertyOrder $entry @("name", "passed", "stage") "record-method"
        Assert-Acceptance ($entry.name -ceq $ActionMethods[$index]) "record-method-order"
        Assert-Acceptance ($entry.passed -is [bool]) "record-method-passed-type"
        Assert-Acceptance ($entry.stage -ceq $MethodStages[$entry.name]) "record-method-stage"
        if (-not $entry.passed) {
            $derivedPass = $false
        }
    }

    Assert-ExactPropertyOrder $Record.assertions $AssertionNames "record-assertions"
    foreach ($name in $AssertionNames) {
        Assert-Acceptance ($Record.assertions.$name -is [bool]) "record-assertion-type"
        if (-not $Record.assertions.$name) {
            $derivedPass = $false
        }
    }
    Assert-Acceptance ($Record.passed -eq $derivedPass) "record-derived-pass"

    $json = $Record | ConvertTo-Json -Depth 8 -Compress
    foreach ($forbiddenKey in @(
        "token", "path", "url", "ref", "callId", "tabId", "windowId",
        "sessionId", "controllerId", "connectionId", "x", "y", "params",
        "body", "text", "value", "stdout", "stderr", "terminal", "profile"
    )) {
        Assert-Acceptance ($json -notmatch ('"' + [Regex]::Escape($forbiddenKey) + '"\s*:')) "record-forbidden-key"
    }
    foreach ($forbiddenValue in @(
        'https?://', 'about:', '[A-Za-z]:\\', '\\\\', '/Users/', '/home/',
        'file:', '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}',
        '"[A-Za-z0-9_-]{43}"'
    )) {
        Assert-Acceptance ($json -notmatch $forbiddenValue) "record-forbidden-value"
    }
    return $json
}

function Copy-JsonObject {
    param([Parameter(Mandatory = $true)]$Value)
    return (($Value | ConvertTo-Json -Depth 8 -Compress) | ConvertFrom-Json)
}

function Test-RejectedEvidence {
    param([Parameter(Mandatory = $true)]$Value)
    try {
        [void](Assert-ReducedEvidenceRecord $Value)
        return $false
    }
    catch {
        return $true
    }
}

function Invoke-RecordSelfTest {
    foreach ($method in $ActionMethods) {
        $MethodPassed[$method] = $true
    }
    foreach ($name in $AssertionNames) {
        $AggregateAssertions[$name] = $true
    }
    $record = Get-ReducedEvidenceRecord $true
    [void](Assert-ReducedEvidenceRecord $record)

    $withToken = Copy-JsonObject $record
    $withToken | Add-Member -NotePropertyName "token" -NotePropertyValue ("A" * 43)
    Assert-Acceptance (Test-RejectedEvidence $withToken) "self-test-token-rejected"

    $withPath = Copy-JsonObject $record
    $withPath | Add-Member -NotePropertyName "path" -NotePropertyValue "C:\acceptance\raw.json"
    Assert-Acceptance (Test-RejectedEvidence $withPath) "self-test-path-rejected"

    $withIdentifier = Copy-JsonObject $record
    $withIdentifier.methods[0] | Add-Member -NotePropertyName "callId" -NotePropertyValue ([Guid]::NewGuid().ToString())
    Assert-Acceptance (Test-RejectedEvidence $withIdentifier) "self-test-identifier-rejected"

    $withUrl = Copy-JsonObject $record
    $withUrl.target = "http://127.0.0.1/"
    Assert-Acceptance (Test-RejectedEvidence $withUrl) "self-test-url-rejected"

    Write-Output "Browser API acceptance self-test passed."
}

if ($SelfTest) {
    if ([String]::IsNullOrWhiteSpace($Version)) {
        $Version = "0.0.0"
    }
    Invoke-RecordSelfTest
    exit 0
}

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "The browser API acceptance driver can run only on Windows."
}
if ([String]::IsNullOrWhiteSpace($Version) -or $Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
    throw "Version must be an explicit stable semantic version."
}
if ([String]::IsNullOrWhiteSpace($OutputPath)) {
    throw "OutputPath must name a brand-new reduced evidence file."
}
if (-not $ConfirmPreopenedDemoIsTestOwned) {
    throw "ConfirmPreopenedDemoIsTestOwned is required before the runner can own or act on the dedicated demo tab."
}

$resolvedOutput = [IO.Path]::GetFullPath($OutputPath)
$outputParent = [IO.Path]::GetDirectoryName($resolvedOutput)
if ([String]::IsNullOrWhiteSpace($outputParent) -or -not [IO.Directory]::Exists($outputParent)) {
    throw "OutputPath must have a pre-existing parent directory."
}
if ([IO.File]::Exists($resolvedOutput) -or [IO.Directory]::Exists($resolvedOutput)) {
    throw "OutputPath already exists; acceptance evidence is append-never."
}

$script:CommandUri = "http://127.0.0.1:$Port/api/v1/command"
$script:HealthUri = "http://127.0.0.1:$Port/health"
$demoUrl = "http://127.0.0.1:$Port/demo"
$historyUrl = "$demoUrl`?step=2"

function Test-BridgeToken {
    param([string]$Value)
    if ([String]::IsNullOrEmpty($Value) -or $Value.Length -ne 43 -or $Value -notmatch '^[A-Za-z0-9_-]{43}$') {
        return $false
    }
    try {
        $base64 = $Value.Replace('-', '+').Replace('_', '/') + "="
        $bytes = [Convert]::FromBase64String($base64)
        return $bytes.Length -eq 32 -and @($bytes | Select-Object -Unique).Count -ge 16
    }
    catch {
        return $false
    }
}

function Get-FreshCommandCallId {
    $callId = [Guid]::NewGuid().ToString("N")
    Assert-Acceptance ($script:SeenCallIds.Add($callId)) "fresh-command-identity"
    $script:CommandCallCount += 1
    return $callId
}

function Assert-OwnedCommandTarget {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params
    )
    $requiresOwnedTab = $Method.StartsWith("page.") -or $Method -in @(
        "browser.control.start", "tabs.activate", "tabs.close"
    )
    if (-not $requiresOwnedTab) {
        return
    }
    Assert-Acceptance ($null -ne $Params.tabId) "owned-target-present"
    $tabId = [long]$Params.tabId
    Assert-Acceptance ($script:OwnedTabs.Contains($tabId)) "owned-target-only"
}

function Invoke-BridgeCommand {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params
    )

    Assert-Acceptance ($ActionMethods -ccontains $Method) "advertised-method-only"
    Assert-OwnedCommandTarget $Method $Params
    $script:ActiveStage = $Method
    $callId = Get-FreshCommandCallId
    $requestJson = [ordered]@{
        method = $Method
        params = $Params
        callId = $callId
    } | ConvertTo-Json -Depth 12 -Compress

    try {
        $response = Invoke-WebRequest `
            -UseBasicParsing `
            -Uri $script:CommandUri `
            -Method Post `
            -Headers @{ Authorization = "Bearer $script:AcceptanceToken" } `
            -ContentType "application/json" `
            -Body $requestJson `
            -TimeoutSec 25
    }
    catch {
        throw [InvalidOperationException]::new("Bridge command transport failed for $Method.")
    }

    Assert-Acceptance ([int]$response.StatusCode -eq 200) "command-http-ok"
    try {
        $body = $response.Content | ConvertFrom-Json
    }
    catch {
        throw [InvalidOperationException]::new("Bridge command returned invalid JSON for $Method.")
    }
    Assert-Acceptance ($body.ok -eq $true) "command-result-ok"
    Assert-Acceptance ($body.callId -ceq $callId) "command-identity-bound"
    Assert-Acceptance ($body.PSObject.Properties.Name -notcontains "replayed") "command-not-replayed"
    return $body
}

function Invoke-Health {
    $script:ActiveStage = "preflight"
    try {
        $response = Invoke-WebRequest -UseBasicParsing -Uri $script:HealthUri -Method Get -TimeoutSec 10
        Assert-Acceptance ([int]$response.StatusCode -eq 200) "health-http-ok"
        return ($response.Content | ConvertFrom-Json)
    }
    catch {
        throw [InvalidOperationException]::new("Candidate health preflight failed.")
    }
}

function Confirm-MethodPassed {
    param([Parameter(Mandatory = $true)][string]$Method)
    Assert-Acceptance ($ActionMethods -ccontains $Method) "known-method-mark"
    $MethodPassed[$Method] = $true
}

function Get-FreshObservation {
    param([Parameter(Mandatory = $true)][long]$TabId)

    $response = Invoke-BridgeCommand "page.observe" ([ordered]@{ tabId = $TabId })
    $snapshot = $response.result.snapshot
    $generation = [string]$snapshot.generation
    Assert-Acceptance (-not [String]::IsNullOrWhiteSpace($generation)) "observation-generation-present"
    Assert-Acceptance ($generation -match '^[a-z0-9-]{1,64}$') "observation-generation-canonical"
    if ($null -ne $script:LastGeneration) {
        Assert-Acceptance ($generation -cne $script:LastGeneration) "observation-generation-fresh"
    }
    Assert-Acceptance ([long]$response.state.targetTabId -eq $TabId) "observation-target-bound"
    Assert-Acceptance ($response.result.control.active -eq $true) "observation-control-active"

    $elements = @($snapshot.elements)
    Assert-Acceptance ($elements.Count -gt 0) "observation-elements-present"
    foreach ($element in $elements) {
        $reference = [string]$element.ref
        Assert-Acceptance ($reference.StartsWith("$generation.")) "observation-ref-generation-bound"
    }

    $script:LastGeneration = $generation
    $script:CurrentObservation = $snapshot
    Confirm-MethodPassed "page.observe"
    return $snapshot
}

function Register-PageMutation {
    $script:PageMutationCount += 1
}

function Get-PageMutationObservation {
    param([Parameter(Mandatory = $true)][long]$TabId)
    $snapshot = Get-FreshObservation $TabId
    $script:ObservedPageMutationCount += 1
    return $snapshot
}

function Get-ObservedElement {
    param(
        [Parameter(Mandatory = $true)]$Observation,
        [Parameter(Mandatory = $true)][string]$Role,
        [Parameter(Mandatory = $true)][string]$Name
    )
    $elementMatches = @($Observation.elements | Where-Object {
        [string]$_.role -ceq $Role -and [string]$_.name -ceq $Name
    })
    Assert-Acceptance ($elementMatches.Count -eq 1) "unique-observed-element"
    Assert-Acceptance ($elementMatches[0].disabled -ne $true) "observed-element-enabled"
    return $elementMatches[0]
}

function Wait-ForDemoState {
    param(
        [Parameter(Mandatory = $true)][long]$TabId,
        [Parameter(Mandatory = $true)][bool]$HistoryStep
    )
    $params = [ordered]@{
        tabId = $TabId
        mutationQuietMs = 250
        timeoutMs = 12000
    }
    if ($HistoryStep) {
        $params.text = "?step=2"
    }
    else {
        $params.textGone = "?step=2"
    }
    $response = Invoke-BridgeCommand "page.waitFor" $params
    Assert-Acceptance ($response.result.satisfied -eq $true) "demo-state-settled"
    Confirm-MethodPassed "page.waitFor"
}

function Assert-DedicatedDemoTab {
    param(
        [Parameter(Mandatory = $true)][long]$TabId,
        [Parameter(Mandatory = $true)][string]$ExpectedUrl
    )
    $listed = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
    $ownedDemoTabs = @($listed.result.tabs | Where-Object {
        [long]$_.id -eq $TabId -and [string]$_.url -ceq $ExpectedUrl
    })
    Assert-Acceptance ($ownedDemoTabs.Count -eq 1) "dynamic-demo-tab-discovered"
    Assert-Acceptance ($script:OwnedTabs.Contains($TabId)) "dynamic-demo-tab-owned"
    Confirm-MethodPassed "tabs.list"
}

function Write-NewEvidenceFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Json
    )
    $encoding = New-Object System.Text.UTF8Encoding($false)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $writer = New-Object IO.StreamWriter($stream, $encoding)
        try {
            $writer.WriteLine($Json)
            $writer.Flush()
        }
        finally {
            $writer.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}

$mainFailed = $false
$failedStage = "preflight"
$targetTabId = $null

try {
    $script:AcceptanceToken = [Environment]::GetEnvironmentVariable("LBB_TOKEN", "Process")
    Assert-Acceptance (Test-BridgeToken $script:AcceptanceToken) "process-token-canonical"

    $health = Invoke-Health
    Assert-Acceptance ($health.ok -eq $true) "health-ok"
    Assert-Acceptance ([string]$health.version -ceq $Version) "server-version-match"
    Assert-Acceptance ($health.extensionConnected -eq $true) "health-extension-connected"
    $AggregateAssertions["serverVersionMatched"] = $true

    $status = Invoke-BridgeCommand "status" ([ordered]@{})
    Assert-Acceptance ($status.result.connected -eq $true) "status-bridge-connected"
    Assert-Acceptance ($status.result.enabled -eq $true) "status-extension-enabled"
    Assert-Acceptance ($status.result.fullAccess -eq $true) "status-full-access"
    Assert-Acceptance ($status.state.connected -eq $true) "state-extension-connected"
    Assert-Acceptance ($status.state.extension.compatible -eq $true) "state-extension-compatible"
    Assert-Acceptance ([string]$status.state.extension.version -ceq $Version) "extension-version-match"
    Assert-Acceptance ([string]$status.state.extension.browser -ceq "Google Chrome") "stock-google-chrome"
    Assert-Acceptance ([string]$status.state.extension.mode -ceq "full-access") "extension-mode-full-access"
    Assert-Acceptance ($null -eq $status.state.pendingDialog) "no-preexisting-dialog"
    Assert-Acceptance ($status.state.browserControl.active -ne $true) "no-preexisting-control"

    $capabilities = @($status.state.extension.capabilities)
    Assert-Acceptance ($capabilities.Count -eq $ActionMethods.Count) "capability-count"
    Assert-Acceptance (@($capabilities | Select-Object -Unique).Count -eq $ActionMethods.Count) "capability-unique"
    foreach ($method in $ActionMethods) {
        Assert-Acceptance ($capabilities -ccontains $method) "capability-complete"
    }
    Confirm-MethodPassed "status"
    $AggregateAssertions["extensionVersionMatched"] = $true
    $AggregateAssertions["realExtensionConnected"] = $true
    $AggregateAssertions["fullAccessEnabled"] = $true
    $AggregateAssertions["capabilitiesComplete"] = $true

    $baseline = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
    $demoTabs = @($baseline.result.tabs | Where-Object {
        [string]$_.url -ceq $demoUrl
    })
    Assert-Acceptance ($demoTabs.Count -eq 1) "single-preopened-demo-tab"
    $targetTabId = [long]$demoTabs[0].id
    Assert-Acceptance ($targetTabId -gt 0) "preopened-demo-tab-valid"
    [void]$script:OwnedTabs.Add($targetTabId)
    Confirm-MethodPassed "tabs.list"
    $AggregateAssertions["dynamicTargetDiscovery"] = $true

    $baselineIds = New-Object 'System.Collections.Generic.HashSet[long]'
    foreach ($tab in @($baseline.result.tabs)) {
        [void]$baselineIds.Add([long]$tab.id)
    }

    $activated = Invoke-BridgeCommand "tabs.activate" ([ordered]@{ tabId = $targetTabId })
    Assert-Acceptance ($activated.result.active -eq $true) "demo-tab-active"
    Assert-Acceptance ([long]$activated.result.tabId -eq $targetTabId) "activation-target-bound"
    Confirm-MethodPassed "tabs.activate"

    $script:ControlMayBeActive = $true
    $started = Invoke-BridgeCommand "browser.control.start" ([ordered]@{
        tabId = $targetTabId
        ttlMs = 300000
    })
    Assert-Acceptance ($started.result.active -eq $true) "control-started"
    Assert-Acceptance ([long]$started.result.tabId -eq $targetTabId) "control-target-bound"
    Confirm-MethodPassed "browser.control.start"

    $control = Invoke-BridgeCommand "browser.control.status" ([ordered]@{})
    Assert-Acceptance ($control.result.active -eq $true) "control-status-active"
    Assert-Acceptance ([long]$control.result.tabId -eq $targetTabId) "control-status-target"
    Confirm-MethodPassed "browser.control.status"

    $created = Invoke-BridgeCommand "tabs.new" ([ordered]@{})
    Assert-Acceptance ($created.result.bridgeCreated -eq $true) "bridge-tab-created"
    $blankTabId = [long]$created.result.tabId
    Assert-Acceptance ($blankTabId -gt 0) "created-tab-id-valid"
    Assert-Acceptance ($blankTabId -ne $targetTabId) "created-tab-distinct"
    [void]$script:OwnedTabs.Add($blankTabId)
    [void]$script:ClosableTabs.Add($blankTabId)

    $afterCreate = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
    $newTabs = @($afterCreate.result.tabs | Where-Object {
        -not $baselineIds.Contains([long]$_.id)
    })
    Assert-Acceptance ($newTabs.Count -eq 1) "single-new-tab"
    Assert-Acceptance ([long]$newTabs[0].id -eq $blankTabId) "created-tab-reconciled"
    Assert-Acceptance ([string]$newTabs[0].url -ceq "about:blank") "created-tab-about-blank"
    Confirm-MethodPassed "tabs.new"

    # Activation itself is a tab API check. Its automatic post-action page
    # observation cannot inject the control indicator into top-level
    # about:blank and may release the demo lease. The runner never starts or
    # renews control on this blank tab; it closes it and explicitly reacquires
    # the pre-opened HTTP demo below.
    $blankActivated = Invoke-BridgeCommand "tabs.activate" ([ordered]@{ tabId = $blankTabId })
    Assert-Acceptance ($blankActivated.result.active -eq $true) "blank-tab-active"
    Assert-Acceptance ([long]$blankActivated.result.tabId -eq $blankTabId) "blank-activation-target-bound"

    $blankClosed = Invoke-BridgeCommand "tabs.close" ([ordered]@{ tabId = $blankTabId })
    Assert-Acceptance ($blankClosed.result.closed -eq $true) "blank-tab-closed"
    [void]$script:ClosableTabs.Remove($blankTabId)
    [void]$script:OwnedTabs.Remove($blankTabId)
    Confirm-MethodPassed "tabs.close"

    $afterBlankClose = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
    Assert-Acceptance (@($afterBlankClose.result.tabs | Where-Object {
        [long]$_.id -eq $blankTabId
    }).Count -eq 0) "blank-close-reconciled"

    $demoReactivated = Invoke-BridgeCommand "tabs.activate" ([ordered]@{ tabId = $targetTabId })
    Assert-Acceptance ($demoReactivated.result.active -eq $true) "demo-tab-reactivated"
    Assert-Acceptance ([long]$demoReactivated.result.tabId -eq $targetTabId) "demo-reactivation-target-bound"

    $reacquired = Invoke-BridgeCommand "browser.control.start" ([ordered]@{
        tabId = $targetTabId
        ttlMs = 300000
    })
    Assert-Acceptance ($reacquired.result.active -eq $true) "demo-control-reacquired"
    Assert-Acceptance ([long]$reacquired.result.tabId -eq $targetTabId) "reacquired-control-target-bound"

    $reacquiredStatus = Invoke-BridgeCommand "browser.control.status" ([ordered]@{})
    Assert-Acceptance ($reacquiredStatus.result.active -eq $true) "reacquired-control-active"
    Assert-Acceptance ([long]$reacquiredStatus.result.tabId -eq $targetTabId) "reacquired-control-target"

    Wait-ForDemoState $targetTabId $false
    $observation = Get-FreshObservation $targetTabId
    Assert-DedicatedDemoTab $targetTabId $demoUrl

    $navigated = Invoke-BridgeCommand "page.navigate" ([ordered]@{
        tabId = $targetTabId
        url = $demoUrl
    })
    Assert-Acceptance ([long]$navigated.result.tabId -eq $targetTabId) "navigate-target-bound"
    Register-PageMutation
    Wait-ForDemoState $targetTabId $false
    $observation = Get-PageMutationObservation $targetTabId
    Assert-DedicatedDemoTab $targetTabId $demoUrl
    Confirm-MethodPassed "page.navigate"

    $browserVersion = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = 'Number((navigator.userAgent.match(/Chrome\/([0-9]+)/) || [])[1] || 0)'
    })
    Assert-Acceptance ([string]$browserVersion.result.type -ceq "number") "browser-version-type"
    Assert-Acceptance ([int]$browserVersion.result.value -ge 140) "browser-version-floor"
    $observation = Get-FreshObservation $targetTabId
    $AggregateAssertions["browserFloorMet"] = $true

    $secondNavigation = Invoke-BridgeCommand "page.navigate" ([ordered]@{
        tabId = $targetTabId
        url = $historyUrl
    })
    Assert-Acceptance ([long]$secondNavigation.result.tabId -eq $targetTabId) "history-navigate-target-bound"
    Register-PageMutation
    Wait-ForDemoState $targetTabId $true
    $observation = Get-PageMutationObservation $targetTabId
    # Tab inventory is intentionally privacy-reduced by the extension and
    # strips query strings. The page-owned route status above is the proof of
    # `?step=2`; inventory proves only the canonical loopback demo URL.
    Assert-DedicatedDemoTab $targetTabId $demoUrl

    $back = Invoke-BridgeCommand "page.back" ([ordered]@{ tabId = $targetTabId })
    Assert-Acceptance ([string]$back.result.navigated -ceq "back") "history-back-completed"
    Register-PageMutation
    Wait-ForDemoState $targetTabId $false
    $observation = Get-PageMutationObservation $targetTabId
    Assert-DedicatedDemoTab $targetTabId $demoUrl
    Confirm-MethodPassed "page.back"

    $forward = Invoke-BridgeCommand "page.forward" ([ordered]@{ tabId = $targetTabId })
    Assert-Acceptance ([string]$forward.result.navigated -ceq "forward") "history-forward-completed"
    Register-PageMutation
    Wait-ForDemoState $targetTabId $true
    $observation = Get-PageMutationObservation $targetTabId
    Assert-DedicatedDemoTab $targetTabId $demoUrl
    Confirm-MethodPassed "page.forward"

    $reload = Invoke-BridgeCommand "page.reload" ([ordered]@{ tabId = $targetTabId })
    Assert-Acceptance ($reload.result.reloaded -eq $true) "reload-completed"
    Register-PageMutation
    Wait-ForDemoState $targetTabId $true
    $observation = Get-PageMutationObservation $targetTabId
    Assert-DedicatedDemoTab $targetTabId $demoUrl
    Confirm-MethodPassed "page.reload"

    $nameField = Get-ObservedElement $observation "textbox" "Display name"
    $filled = Invoke-BridgeCommand "page.fill" ([ordered]@{
        tabId = $targetTabId
        ref = [string]$nameField.ref
        generation = [string]$observation.generation
        text = "Bridge Matrix"
    })
    Assert-Acceptance ($filled.result.filled -eq $true) "fill-completed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Confirm-MethodPassed "page.fill"

    $colorField = Get-ObservedElement $observation "select" "Favorite color"
    $selected = Invoke-BridgeCommand "page.select" ([ordered]@{
        tabId = $targetTabId
        ref = [string]$colorField.ref
        generation = [string]$observation.generation
        value = "blue"
    })
    Assert-Acceptance ([string]$selected.result.selected -ceq "blue") "select-completed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Confirm-MethodPassed "page.select"

    $submitButton = Get-ObservedElement $observation "button" "Show greeting"
    $clicked = Invoke-BridgeCommand "page.click" ([ordered]@{
        tabId = $targetTabId
        ref = [string]$submitButton.ref
        generation = [string]$observation.generation
        button = "left"
        clickCount = 1
    })
    Assert-Acceptance ($clicked.result.clicked -eq $true -and $clicked.result.trusted -eq $true) "click-completed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Assert-Acceptance ([string]$observation.bodyText -like "*Hello, Bridge Matrix. blue selected.*") "greeting-rendered"
    Confirm-MethodPassed "page.click"

    $greeting = Invoke-BridgeCommand "page.waitFor" ([ordered]@{
        tabId = $targetTabId
        text = "Hello, Bridge Matrix. blue selected."
        timeoutMs = 5000
    })
    Assert-Acceptance ($greeting.result.satisfied -eq $true) "greeting-wait-satisfied"

    $keyboardField = Get-ObservedElement $observation "textbox" "Keyboard target"
    $focused = Invoke-BridgeCommand "page.click" ([ordered]@{
        tabId = $targetTabId
        ref = [string]$keyboardField.ref
        generation = [string]$observation.generation
        button = "left"
        clickCount = 1
    })
    Assert-Acceptance ($focused.result.clicked -eq $true) "keyboard-focus-clicked"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId

    $keyed = Invoke-BridgeCommand "page.key" ([ordered]@{
        tabId = $targetTabId
        generation = [string]$observation.generation
        key = "End"
    })
    Assert-Acceptance ([string]$keyed.result.pressed -ceq "End") "key-completed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Assert-Acceptance ([string]$observation.bodyText -like "*key:End*") "key-observed"
    Confirm-MethodPassed "page.key"

    $typed = Invoke-BridgeCommand "page.typeText" ([ordered]@{
        tabId = $targetTabId
        generation = [string]$observation.generation
        text = "matrix-input"
    })
    Assert-Acceptance ($typed.result.typed -eq $true) "type-text-completed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Assert-Acceptance ([string]$observation.bodyText -like "*input:matrix-input*") "type-text-observed"
    Confirm-MethodPassed "page.typeText"

    $coordinateButton = Get-ObservedElement $observation "button" "Coordinate target"
    $hovered = Invoke-BridgeCommand "page.hover" ([ordered]@{
        tabId = $targetTabId
        ref = [string]$coordinateButton.ref
        generation = [string]$observation.generation
    })
    Assert-Acceptance ($hovered.result.hovered -eq $true -and $hovered.result.trusted -eq $true) "hover-completed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Confirm-MethodPassed "page.hover"

    $coordinateButton = Get-ObservedElement $observation "button" "Coordinate target"
    $pointX = [double]$coordinateButton.bounds.x + ([double]$coordinateButton.bounds.width / 2.0)
    $pointY = [double]$coordinateButton.bounds.y + ([double]$coordinateButton.bounds.height / 2.0)
    Assert-Acceptance (-not [double]::IsNaN($pointX) -and -not [double]::IsInfinity($pointX)) "point-x-finite"
    Assert-Acceptance (-not [double]::IsNaN($pointY) -and -not [double]::IsInfinity($pointY)) "point-y-finite"
    $pointClicked = Invoke-BridgeCommand "page.clickAt" ([ordered]@{
        tabId = $targetTabId
        generation = [string]$observation.generation
        x = $pointX
        y = $pointY
        button = "left"
        clickCount = 1
    })
    Assert-Acceptance ($pointClicked.result.clicked -eq $true -and $pointClicked.result.trusted -eq $true) "point-click-completed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Assert-Acceptance ([string]$observation.bodyText -like "*coordinate:true*") "point-click-observed"
    Confirm-MethodPassed "page.clickAt"

    $evaluated = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = 'document.body.dataset.lastAction === "coordinate:true"'
    })
    Assert-Acceptance ([string]$evaluated.result.type -ceq "boolean") "evaluation-type"
    Assert-Acceptance ($evaluated.result.value -eq $true) "evaluation-value"
    $observation = Get-FreshObservation $targetTabId

    $dialogScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = 'setTimeout(function () { confirm("bridge acceptance dialog"); }, 100); "scheduled"'
    })
    Assert-Acceptance ([string]$dialogScheduled.result.type -ceq "string") "dialog-scheduled"

    $dialogSeen = $false
    for ($attempt = 0; $attempt -lt 40; $attempt += 1) {
        $dialogStatus = Invoke-BridgeCommand "status" ([ordered]@{})
        if ($null -ne $dialogStatus.state.pendingDialog) {
            $dialogSeen = $true
            break
        }
        Start-Sleep -Milliseconds 125
    }
    Assert-Acceptance $dialogSeen "dialog-opened"

    $handled = Invoke-BridgeCommand "page.handleDialog" ([ordered]@{
        tabId = $targetTabId
        accept = $false
    })
    Assert-Acceptance ($handled.result.handled -eq $true) "dialog-handled"
    Assert-Acceptance ($handled.result.accept -eq $false) "dialog-dismissed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Assert-Acceptance ($null -eq $handled.state.pendingDialog) "dialog-state-cleared"
    Confirm-MethodPassed "page.evaluate"
    Confirm-MethodPassed "page.handleDialog"
    $AggregateAssertions["dialogLifecycle"] = $true

    $scrolled = Invoke-BridgeCommand "page.scroll" ([ordered]@{
        tabId = $targetTabId
        generation = [string]$observation.generation
        deltaX = 0
        deltaY = 1600
    })
    Assert-Acceptance ($scrolled.result.snapshotInvalidated -eq $true) "scroll-invalidated-snapshot"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Assert-Acceptance ([double]$observation.scroll.y -gt 0) "scroll-observed"
    Confirm-MethodPassed "page.scroll"

    $batch = Invoke-BridgeCommand "page.batch" ([ordered]@{
        tabId = $targetTabId
        generation = [string]$observation.generation
        actions = @(
            [ordered]@{
                method = "page.scroll"
                deltaX = 0
                deltaY = -200
            }
        )
    })
    Assert-Acceptance ([int]$batch.result.completed -eq 1) "batch-completed"
    Assert-Acceptance ([int]$batch.result.total -eq 1) "batch-total"
    Assert-Acceptance (@($batch.result.perStep).Count -eq 1) "batch-step-count"
    Assert-Acceptance ($batch.result.perStep[0].ok -eq $true) "batch-step-ok"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Confirm-MethodPassed "page.batch"

    Assert-Acceptance ($script:PageMutationCount -eq $script:ObservedPageMutationCount) "all-page-mutations-reobserved"
    $AggregateAssertions["freshObservationAfterPageMutation"] = $true
    $AggregateAssertions["testOwnedTabsOnly"] = $true

    $stopped = Invoke-BridgeCommand "browser.control.stop" ([ordered]@{})
    Assert-Acceptance ($stopped.result.active -ne $true) "control-stopped"
    $script:ControlMayBeActive = $false
    Confirm-MethodPassed "browser.control.stop"

    $afterStop = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
    Assert-Acceptance (@($afterStop.result.tabs | Where-Object {
        [long]$_.id -eq $targetTabId -and [string]$_.url -ceq $demoUrl
    }).Count -eq 1) "demo-tab-retained-for-visual-evidence"
    [void]$script:OwnedTabs.Remove($targetTabId)

    Assert-Acceptance ($script:SeenCallIds.Count -eq $script:CommandCallCount) "all-command-identities-unique"
    $AggregateAssertions["freshCommandIdentity"] = $true
}
catch {
    $mainFailed = $true
    $failedStage = $script:ActiveStage
}
finally {
    $cleanupSucceeded = $true
    if ($script:ControlMayBeActive -and $null -ne $script:AcceptanceToken) {
        try {
            $cleanupControl = Invoke-BridgeCommand "browser.control.stop" ([ordered]@{})
            if ($cleanupControl.result.active -eq $true) {
                $cleanupSucceeded = $false
            }
            else {
                $script:ControlMayBeActive = $false
            }
        }
        catch {
            $cleanupSucceeded = $false
        }
    }

    foreach ($ownedTabId in @($script:ClosableTabs)) {
        try {
            $cleanupClose = Invoke-BridgeCommand "tabs.close" ([ordered]@{ tabId = [long]$ownedTabId })
            if ($cleanupClose.result.closed -eq $true) {
                [void]$script:ClosableTabs.Remove([long]$ownedTabId)
                [void]$script:OwnedTabs.Remove([long]$ownedTabId)
            }
            else {
                $cleanupSucceeded = $false
            }
        }
        catch {
            $cleanupSucceeded = $false
        }
    }

    if ($script:ControlMayBeActive -or $script:ClosableTabs.Count -ne 0) {
        $cleanupSucceeded = $false
    }
    $AggregateAssertions["cleanupComplete"] = $cleanupSucceeded

    [Environment]::SetEnvironmentVariable("LBB_TOKEN", $null, "Process")
    Remove-Item Env:LBB_TOKEN -ErrorAction SilentlyContinue
    $script:AcceptanceToken = $null
}

$allMethodsPassed = $true
foreach ($method in $ActionMethods) {
    if (-not $MethodPassed[$method]) {
        $allMethodsPassed = $false
    }
}
$allAssertionsPassed = $true
foreach ($name in $AssertionNames) {
    if (-not $AggregateAssertions[$name]) {
        $allAssertionsPassed = $false
    }
}
$overallPassed = -not $mainFailed -and $allMethodsPassed -and $allAssertionsPassed
$record = Get-ReducedEvidenceRecord $overallPassed
$recordJson = Assert-ReducedEvidenceRecord $record
Write-NewEvidenceFile $resolvedOutput $recordJson

if (-not $overallPassed) {
    if ($ActionMethods -notcontains $failedStage -and $failedStage -cne "preflight") {
        $failedStage = "preflight"
    }
    throw "Browser API acceptance failed at the $failedStage stage; the reduced evidence record contains no raw failure data."
}

Write-Output "Browser API acceptance passed."
