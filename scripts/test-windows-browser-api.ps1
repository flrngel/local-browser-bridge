#requires -Version 5.1

<#
.SYNOPSIS
Runs the real-extension browser API acceptance matrix against the loopback demo.

.DESCRIPTION
Run this only after the frozen candidate server is listening and the exact
unpacked extension is connected in stock Google Chrome with Full Access. The
runner creates its dedicated loopback `/demo` tab with policy-checked
`tabs.new { url }`, exercises only tabs that it created, and leaves the demo
tab open for the later visual Stop/Cancel evidence flow. It closes the
secondary bridge-created blank tab used by the tab-lifecycle checks.
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

    [string]$PreflightRecord = "",

    [string]$OutputPath = "",

    [switch]$PassThruOwnedTarget,

    [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

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

$MethodScreenshots = [ordered]@{
    "status"                 = "browser-03-popup-connected.png"
    "browser.control.start"  = "browser-04-native-debugger-warning.png"
    "browser.control.status" = "N/A"
    "browser.control.stop"   = "N/A"
    "tabs.list"              = "N/A"
    "tabs.activate"          = "N/A"
    "tabs.new"               = "N/A"
    "tabs.close"             = "N/A"
    "page.observe"           = "N/A"
    "page.navigate"          = "N/A"
    "page.back"              = "N/A"
    "page.forward"           = "N/A"
    "page.reload"            = "N/A"
    "page.click"             = "browser-06-action-result.png"
    "page.fill"              = "browser-06-action-result.png"
    "page.select"            = "browser-06-action-result.png"
    "page.key"               = "N/A"
    "page.scroll"            = "N/A"
    "page.clickAt"            = "N/A"
    "page.typeText"          = "N/A"
    "page.evaluate"          = "browser-05-page-control-pill.png"
    "page.waitFor"           = "N/A"
    "page.hover"             = "N/A"
    "page.batch"             = "N/A"
    "page.handleDialog"      = "N/A"
}
$MethodScreenshotsV2 = [ordered]@{
    "status"                 = "N/A"
    "browser.control.start"  = "browser-01-extension-loaded.png"
    "browser.control.status" = "N/A"
    "browser.control.stop"   = "N/A"
    "tabs.list"              = "N/A"
    "tabs.activate"          = "N/A"
    "tabs.new"               = "N/A"
    "tabs.close"             = "N/A"
    "page.observe"           = "N/A"
    "page.navigate"          = "N/A"
    "page.back"              = "N/A"
    "page.forward"           = "N/A"
    "page.reload"            = "N/A"
    "page.click"             = "browser-02-api-action-result.png"
    "page.fill"              = "browser-02-api-action-result.png"
    "page.select"            = "browser-02-api-action-result.png"
    "page.key"               = "N/A"
    "page.scroll"            = "N/A"
    "page.clickAt"           = "N/A"
    "page.typeText"          = "N/A"
    "page.evaluate"          = "browser-02-api-action-result.png"
    "page.waitFor"           = "N/A"
    "page.hover"             = "N/A"
    "page.batch"             = "N/A"
    "page.handleDialog"      = "N/A"
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
    "topLayerControlUiIntegrity",
    "dialogLifecycle",
    "cleanupComplete"
)

$MethodPassed = [ordered]@{}
$MethodCommandInvoked = [ordered]@{}
$MethodResultVerified = [ordered]@{}
$MethodPostconditionVerified = [ordered]@{}
foreach ($method in $ActionMethods) {
    $MethodPassed[$method] = $false
    $MethodCommandInvoked[$method] = $false
    $MethodResultVerified[$method] = $false
    $MethodPostconditionVerified[$method] = $false
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
$script:CandidateBinding = $null
$script:OwnedTargetHandoff = $null

function Assert-Acceptance {
    param(
        [Parameter(Mandatory = $true)][bool]$Condition,
        [Parameter(Mandatory = $true)][string]$Assertion
    )
    if (-not $Condition) {
        throw [InvalidOperationException]::new("Acceptance assertion failed: $Assertion")
    }
}

function Assert-NoReparseAncestorChain {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Boundary
    )
    $full = [IO.Path]::GetFullPath($Path)
    $directory = if ([IO.Directory]::Exists($full)) {
        [IO.DirectoryInfo]::new($full)
    }
    else {
        [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName($full))
    }
    while ($null -ne $directory) {
        Assert-Acceptance (
            ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0
        ) "${Boundary}-no-reparse-ancestor"
        $directory = $directory.Parent
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

function Get-BytesSha256 {
    param([Parameter(Mandatory = $true)][byte[]]$Bytes)
    $hasher = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($hasher.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
    }
}

function Assert-ReleaseCandidateBinding {
    param(
        [Parameter(Mandatory = $true)]$Binding,
        [Parameter(Mandatory = $true)]$Candidate,
        [Parameter(Mandatory = $true)][string]$Boundary
    )
    $fields = @(
        "schemaVersion", "version", "releaseTag", "repository", "sourceSha",
        "workflowRunId", "workflowRunAttempt", "workflowEvent", "workflowRef", "workflowPath",
        "artifactId", "artifactName",
        "artifactZipBytes", "artifactZipSha256", "checksumManifestSha256",
        "attestationInvocationUri", "attestedAssetCount", "githubHostedRunner", "assets"
    )
    Assert-ExactPropertyOrder $Binding $fields $Boundary
    Assert-Acceptance ($Binding.schemaVersion -eq 3) "$Boundary-schema-version"
    Assert-Acceptance ($Binding.version -ceq $Version) "$Boundary-version"
    Assert-Acceptance ($Binding.releaseTag -ceq "v$Version") "$Boundary-release-tag"
    Assert-Acceptance ($Binding.repository -ceq "flrngel/local-browser-bridge") "$Boundary-repository"
    Assert-Acceptance ($Binding.sourceSha -ceq $Candidate.finalSha) "$Boundary-source"
    foreach ($name in @("workflowRunId", "workflowRunAttempt", "artifactId")) {
        Assert-Acceptance ([string]$Binding.$name -cmatch '^[1-9][0-9]*$') "$Boundary-$name"
    }
    Assert-Acceptance ($Binding.workflowEvent -ceq "workflow_dispatch") "$Boundary-workflow-event"
    Assert-Acceptance ($Binding.workflowRef -ceq "refs/heads/main") "$Boundary-workflow-ref"
    Assert-Acceptance ($Binding.workflowPath -ceq ".github/workflows/deploy.yml") "$Boundary-workflow-path"
    Assert-Acceptance ($Binding.artifactName -ceq "release-candidate") "$Boundary-artifact-name"
    Assert-Acceptance ($Binding.artifactZipBytes -is [ValueType] -and
        [int64]$Binding.artifactZipBytes -gt 0) "$Boundary-artifact-size"
    Assert-Acceptance ([string]$Binding.artifactZipSha256 -cmatch '^[0-9a-f]{64}$') "$Boundary-artifact-hash"
    Assert-Acceptance ($Binding.checksumManifestSha256 -ceq $Candidate.checksumManifest.sha256) `
        "$Boundary-manifest-hash"
    $invocationUri = "https://github.com/flrngel/local-browser-bridge/actions/runs/$($Binding.workflowRunId)/attempts/$($Binding.workflowRunAttempt)"
    Assert-Acceptance ($Binding.attestationInvocationUri -ceq $invocationUri) "$Boundary-invocation"
    Assert-Acceptance ($Binding.attestedAssetCount -eq 5 -and
        $Binding.githubHostedRunner -eq $true) "$Boundary-attestation"
    $expectedNames = @($Candidate.checksumManifest.canonicalNamesInOrder) + "SHA256SUMS.txt"
    $assets = @($Binding.assets)
    Assert-Acceptance ($assets.Count -eq 5) "$Boundary-asset-count"
    for ($index = 0; $index -lt $assets.Count; $index += 1) {
        Assert-ExactPropertyOrder $assets[$index] @("file", "bytes", "sha256") "$Boundary-asset"
        Assert-Acceptance ($assets[$index].file -ceq $expectedNames[$index]) "$Boundary-asset-name"
        Assert-Acceptance ($assets[$index].bytes -is [ValueType] -and
            [int64]$assets[$index].bytes -gt 0) "$Boundary-asset-size"
        Assert-Acceptance ([string]$assets[$index].sha256 -cmatch '^[0-9a-f]{64}$') "$Boundary-asset-hash"
    }
    Assert-Acceptance ($assets[0].sha256 -ceq $Candidate.server.sha256) "$Boundary-server-asset"
    Assert-Acceptance ($assets[1].sha256 -ceq $Candidate.computerHelper.sha256) "$Boundary-helper-asset"
    Assert-Acceptance ($assets[3].sha256 -ceq $Candidate.extension.sha256) "$Boundary-extension-asset"
    Assert-Acceptance ($assets[4].sha256 -ceq $Candidate.checksumManifest.sha256) "$Boundary-manifest-asset"
}

function Get-CandidateBindingFromPreflight {
    param([Parameter(Mandatory = $true)][string]$Path)
    $resolved = [IO.Path]::GetFullPath($Path)
    Assert-Acceptance ([IO.File]::Exists($resolved)) "preflight-record-exists"
    $item = [IO.FileInfo]::new($resolved)
    Assert-Acceptance (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) "preflight-record-not-reparse-point"
    Assert-NoReparseAncestorChain $resolved "preflight-record"
    Assert-Acceptance ($item.Length -gt 0 -and $item.Length -le 1MB) "preflight-record-size"
    $bytes = [IO.File]::ReadAllBytes($resolved)
    try {
        $utf8 = [Text.UTF8Encoding]::new($false, $true)
        $record = ConvertFrom-JsonPreservingStrings ($utf8.GetString($bytes))
        $preflightFields = @(
            "schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed", "runNonce", "candidate"
        )
        if ($Version -ceq "0.12.30") {
            $preflightFields = @(
                "schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed",
                "runNonce", "releaseCandidateBinding", "candidate"
            )
        }
        Assert-ExactPropertyOrder $record $preflightFields "preflight-record"
        Assert-Acceptance ($record.schemaVersion -eq 1 -and
            $record.evidenceType -ceq "stock-user-chrome-candidate-binding" -and
            $record.phase -ceq "preflight" -and $record.passed -eq $true) "preflight-record-identity"
        Assert-Acceptance ([string]$record.runNonce -cmatch '^[0-9a-f]{64}$') "preflight-run-nonce"
        Assert-Acceptance ([string]$record.candidate.version -ceq $Version) "preflight-version"
        Assert-Acceptance ([string]$record.candidate.finalSha -cmatch '^[0-9a-f]{40}$') "preflight-final-sha"
        foreach ($value in @(
            [string]$record.candidate.checksumManifest.sha256,
            [string]$record.candidate.server.sha256,
            $(if ($Version -ceq "0.12.30") { [string]$record.candidate.computerHelper.sha256 } else { [string]$record.candidate.server.sha256 }),
            [string]$record.candidate.extension.sha256,
            [string]$record.candidate.extension.combinedPayloadSha256
        )) {
            Assert-Acceptance ($value -cmatch '^[0-9a-f]{64}$') "preflight-candidate-hash"
        }
        if ($Version -ceq "0.12.30") {
            Assert-ReleaseCandidateBinding $record.releaseCandidateBinding $record.candidate `
                "preflight-release-candidate-binding"
            $script:ReleaseCandidateBinding = $record.releaseCandidateBinding
        }
        $binding = [ordered]@{
            runNonce = [string]$record.runNonce
            preflightRecordSha256 = Get-BytesSha256 $bytes
            finalSha = [string]$record.candidate.finalSha
            checksumManifestSha256 = [string]$record.candidate.checksumManifest.sha256
            serverSha256 = [string]$record.candidate.server.sha256
        }
        if ($Version -ceq "0.12.30") {
            $binding.computerHelperSha256 = [string]$record.candidate.computerHelper.sha256
        }
        $binding.extensionZipSha256 = [string]$record.candidate.extension.sha256
        $binding.extractedPayloadSha256 = [string]$record.candidate.extension.combinedPayloadSha256
        return [pscustomobject]$binding
    }
    finally {
        [Array]::Clear($bytes, 0, $bytes.Length)
    }
}

function Get-ReducedEvidenceRecord {
    param([Parameter(Mandatory = $true)][bool]$Passed)

    $methods = @()
    foreach ($method in $ActionMethods) {
        $methods += [pscustomobject][ordered]@{
            name                      = $method
            passed                    = [bool]$MethodPassed[$method]
            stage                     = [string]$MethodStages[$method]
            commandInvoked            = [bool]$MethodCommandInvoked[$method]
            resultVerified            = [bool]$MethodResultVerified[$method]
            postconditionVerified     = [bool]$MethodPostconditionVerified[$method]
            screenshot                = [string]$MethodScreenshots[$method]
            machineProof              = "machine-command-result-postcondition"
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
        candidateBinding = $script:CandidateBinding
        passed        = $Passed
        methodCount   = 25
        methods       = $methods
        assertions    = [pscustomobject]$assertions
    }
}

function Assert-ReducedEvidenceRecord {
    param([Parameter(Mandatory = $true)]$Record)

    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "evidenceType", "version", "target", "candidateBinding", "passed",
        "methodCount", "methods", "assertions"
    ) "record"
    Assert-Acceptance ($Record.schemaVersion -eq 1) "record-schema"
    Assert-Acceptance ($Record.evidenceType -ceq "stock-user-chrome-api-matrix") "record-type"
    Assert-Acceptance ($Record.version -match '^[0-9]+\.[0-9]+\.[0-9]+$') "record-version"
    Assert-Acceptance ($Record.target -ceq "loopback-demo") "record-target"
    $bindingFields = @(
        "runNonce", "preflightRecordSha256", "finalSha", "checksumManifestSha256",
        "serverSha256", "extensionZipSha256", "extractedPayloadSha256"
    )
    if ($Version -ceq "0.12.30") {
        $bindingFields = @(
            "runNonce", "preflightRecordSha256", "finalSha", "checksumManifestSha256",
            "serverSha256", "computerHelperSha256", "extensionZipSha256", "extractedPayloadSha256"
        )
    }
    Assert-ExactPropertyOrder $Record.candidateBinding $bindingFields "record-candidate-binding"
    Assert-Acceptance ([string]$Record.candidateBinding.runNonce -cmatch '^[0-9a-f]{64}$') "record-run-nonce"
    Assert-Acceptance ([string]$Record.candidateBinding.finalSha -cmatch '^[0-9a-f]{40}$') "record-final-sha"
    foreach ($name in @($bindingFields | Where-Object { $_ -like "*Sha256" })) {
        Assert-Acceptance ([string]$Record.candidateBinding.$name -cmatch '^[0-9a-f]{64}$') "record-candidate-hash"
    }
    Assert-Acceptance (($Record.candidateBinding | ConvertTo-Json -Depth 5 -Compress) -ceq
        ($script:CandidateBinding | ConvertTo-Json -Depth 5 -Compress)) "record-candidate-binding-exact"
    Assert-Acceptance ($Record.passed -is [bool]) "record-passed-type"
    Assert-Acceptance ($Record.methodCount -eq 25) "record-method-count"

    $methods = @($Record.methods)
    Assert-Acceptance ($methods.Count -eq $ActionMethods.Count) "record-method-array"
    $derivedPass = $true
    for ($index = 0; $index -lt $ActionMethods.Count; $index += 1) {
        $entry = $methods[$index]
        Assert-ExactPropertyOrder $entry @(
            "name", "passed", "stage", "commandInvoked", "resultVerified",
            "postconditionVerified", "screenshot", "machineProof"
        ) "record-method"
        Assert-Acceptance ($entry.name -ceq $ActionMethods[$index]) "record-method-order"
        Assert-Acceptance ($entry.passed -is [bool]) "record-method-passed-type"
        Assert-Acceptance ($entry.stage -ceq $MethodStages[$entry.name]) "record-method-stage"
        foreach ($proof in @("commandInvoked", "resultVerified", "postconditionVerified")) {
            Assert-Acceptance ($entry.$proof -is [bool]) "record-method-proof-type"
        }
        Assert-Acceptance ($entry.screenshot -ceq $MethodScreenshots[$entry.name]) "record-method-screenshot"
        Assert-Acceptance ($entry.machineProof -ceq "machine-command-result-postcondition") "record-method-machine-proof"
        if (-not $entry.passed -or -not $entry.commandInvoked -or
            -not $entry.resultVerified -or -not $entry.postconditionVerified) {
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
    return ConvertFrom-JsonPreservingStrings ($Value | ConvertTo-Json -Depth 8 -Compress)
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
    $selfTestBinding = [ordered]@{
        runNonce = [String]::new([char]"a", 64)
        preflightRecordSha256 = [String]::new([char]"b", 64)
        finalSha = [String]::new([char]"c", 40)
        checksumManifestSha256 = [String]::new([char]"d", 64)
        serverSha256 = [String]::new([char]"e", 64)
    }
    if ($Version -ceq "0.12.30") {
        $selfTestBinding.computerHelperSha256 = [String]::new([char]"1", 64)
    }
    $selfTestBinding.extensionZipSha256 = [String]::new([char]"f", 64)
    $selfTestBinding.extractedPayloadSha256 = [String]::new([char]"0", 64)
    $script:CandidateBinding = [pscustomobject]$selfTestBinding
    if ($Version -ceq "0.12.30") {
        $selfTestCandidate = [pscustomobject][ordered]@{
            version = $Version
            finalSha = $selfTestBinding.finalSha
            checksumManifest = [pscustomobject][ordered]@{
                sha256 = $selfTestBinding.checksumManifestSha256
                canonicalNamesInOrder = @(
                    "local-browser-bridge-v$Version-windows-x86_64.exe",
                    "local-computer-helper-v$Version-windows-x86_64.exe",
                    "local-browser-bridge-v$Version-macos-universal.tar.gz",
                    "local-browser-bridge-extension-v$Version.zip"
                )
            }
            server = [pscustomobject][ordered]@{ sha256 = $selfTestBinding.serverSha256 }
            computerHelper = [pscustomobject][ordered]@{ sha256 = $selfTestBinding.computerHelperSha256 }
            extension = [pscustomobject][ordered]@{ sha256 = $selfTestBinding.extensionZipSha256 }
        }
        $assetNames = @($selfTestCandidate.checksumManifest.canonicalNamesInOrder) + "SHA256SUMS.txt"
        $assetHashes = @(
            $selfTestBinding.serverSha256, $selfTestBinding.computerHelperSha256,
            [String]::new([char]"2", 64), $selfTestBinding.extensionZipSha256,
            $selfTestBinding.checksumManifestSha256
        )
        $assets = @()
        for ($index = 0; $index -lt 5; $index += 1) {
            $assets += [pscustomobject][ordered]@{
                file = $assetNames[$index]; bytes = 1000; sha256 = $assetHashes[$index]
            }
        }
        $releaseBinding = [pscustomobject][ordered]@{
            schemaVersion = 3
            version = $Version
            releaseTag = "v$Version"
            repository = "flrngel/local-browser-bridge"
            sourceSha = $selfTestBinding.finalSha
            workflowRunId = "123"
            workflowRunAttempt = "1"
            workflowEvent = "workflow_dispatch"
            workflowRef = "refs/heads/main"
            workflowPath = ".github/workflows/deploy.yml"
            artifactId = "456"
            artifactName = "release-candidate"
            artifactZipBytes = 5000
            artifactZipSha256 = [String]::new([char]"4", 64)
            checksumManifestSha256 = $selfTestBinding.checksumManifestSha256
            attestationInvocationUri = "https://github.com/flrngel/local-browser-bridge/actions/runs/123/attempts/1"
            attestedAssetCount = 5
            githubHostedRunner = $true
            assets = $assets
        }
        Assert-ReleaseCandidateBinding $releaseBinding $selfTestCandidate "self-test-release-binding"
        $script:ReleaseCandidateBinding = $releaseBinding
        $mismatchedRelease = Copy-JsonObject $releaseBinding
        $mismatchedRelease.workflowRunAttempt = "2"
        $mismatchRejected = $false
        try {
            Assert-ReleaseCandidateBinding $mismatchedRelease $selfTestCandidate `
                "self-test-mismatched-release-binding"
        }
        catch { $mismatchRejected = $true }
        Assert-Acceptance $mismatchRejected "self-test-release-attempt-mismatch-rejected"
    }
    foreach ($method in $ActionMethods) {
        $MethodPassed[$method] = $true
        $MethodCommandInvoked[$method] = $true
        $MethodResultVerified[$method] = $true
        $MethodPostconditionVerified[$method] = $true
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

    $withOtherCandidate = Copy-JsonObject $record
    $withOtherCandidate.candidateBinding.preflightRecordSha256 = [String]::new([char]"9", 64)
    Assert-Acceptance (Test-RejectedEvidence $withOtherCandidate) "self-test-other-candidate-rejected"

    $withoutPostcondition = Copy-JsonObject $record
    $withoutPostcondition.methods[0].postconditionVerified = $false
    Assert-Acceptance (Test-RejectedEvidence $withoutPostcondition) "self-test-postcondition-rejected"

    $withScreenshotPath = Copy-JsonObject $record
    $withScreenshotPath.methods[0].screenshot = "C:\acceptance\raw.png"
    Assert-Acceptance (Test-RejectedEvidence $withScreenshotPath) "self-test-screenshot-path-rejected"

    Write-Output "Browser API acceptance self-test passed."
}

if ($SelfTest) {
    if ([String]::IsNullOrWhiteSpace($Version)) {
        $Version = "0.12.30"
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
if ($Version -ceq "0.12.30") {
    $MethodScreenshots = $MethodScreenshotsV2
}
if ([String]::IsNullOrWhiteSpace($PreflightRecord)) {
    throw "PreflightRecord must name the exact candidate preflight record."
}
if (-not $PassThruOwnedTarget) {
    throw "PassThruOwnedTarget is required so the exact bridge-owned tab stays in memory for visual handoff."
}
$script:CandidateBinding = Get-CandidateBindingFromPreflight $PreflightRecord
if ([String]::IsNullOrWhiteSpace($OutputPath)) {
    throw "OutputPath must name a brand-new reduced evidence file."
}
$resolvedOutput = [IO.Path]::GetFullPath($OutputPath)
$outputParent = [IO.Path]::GetDirectoryName($resolvedOutput)
if ([String]::IsNullOrWhiteSpace($outputParent) -or -not [IO.Directory]::Exists($outputParent)) {
    throw "OutputPath must have a pre-existing parent directory."
}
Assert-NoReparseAncestorChain $outputParent "output-parent"
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
    $MethodCommandInvoked[$Method] = $true
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
        $body = ConvertFrom-JsonPreservingStrings $response.Content
    }
    catch {
        throw [InvalidOperationException]::new("Bridge command returned invalid JSON for $Method.")
    }
    Assert-Acceptance ($body.ok -eq $true) "command-result-ok"
    Assert-Acceptance ($body.callId -ceq $callId) "command-identity-bound"
    Assert-Acceptance ($body.PSObject.Properties.Name -notcontains "replayed") "command-not-replayed"
    $MethodResultVerified[$Method] = $true
    return $body
}

function Invoke-BridgeCommandExpectError {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params,
        [Parameter(Mandatory = $true)][int]$ExpectedHttpStatus,
        [Parameter(Mandatory = $true)][string]$ExpectedErrorCode,
        [Parameter(Mandatory = $true)][string]$ExpectedTaxonomyCode,
        [Parameter(Mandatory = $true)][string]$ExpectedRecoveryHint
    )

    Assert-Acceptance ($ActionMethods -ccontains $Method) "advertised-error-method-only"
    Assert-OwnedCommandTarget $Method $Params
    $MethodCommandInvoked[$Method] = $true
    $script:ActiveStage = $Method
    $callId = Get-FreshCommandCallId
    $requestJson = [ordered]@{
        method = $Method
        params = $Params
        callId = $callId
    } | ConvertTo-Json -Depth 12 -Compress

    $statusCode = 0
    $responseText = $null
    try {
        $unexpected = Invoke-WebRequest `
            -UseBasicParsing `
            -Uri $script:CommandUri `
            -Method Post `
            -Headers @{ Authorization = "Bearer $script:AcceptanceToken" } `
            -ContentType "application/json" `
            -Body $requestJson `
            -TimeoutSec 25
        [void]$unexpected
        throw [InvalidOperationException]::new("Bridge command unexpectedly succeeded for the required refusal.")
    }
    catch {
        $webResponse = $_.Exception.Response
        if ($null -eq $webResponse) {
            throw [InvalidOperationException]::new("Bridge refusal transport failed for $Method.")
        }
        $statusCode = [int]$webResponse.StatusCode
        if ($null -ne $_.ErrorDetails -and -not [String]::IsNullOrWhiteSpace($_.ErrorDetails.Message)) {
            $responseText = [string]$_.ErrorDetails.Message
        }
        elseif ($webResponse.PSObject.Properties.Name -contains "Content" -and
            $null -ne $webResponse.Content -and
            $webResponse.Content.PSObject.Methods.Name -contains "ReadAsStringAsync") {
            $responseText = $webResponse.Content.ReadAsStringAsync().GetAwaiter().GetResult()
        }
        elseif ($webResponse.PSObject.Methods.Name -contains "GetResponseStream") {
            $stream = $webResponse.GetResponseStream()
            try {
                $reader = New-Object IO.StreamReader($stream)
                try { $responseText = $reader.ReadToEnd() }
                finally { $reader.Dispose() }
            }
            finally { if ($null -ne $stream) { $stream.Dispose() } }
        }
    }
    Assert-Acceptance ($statusCode -eq $ExpectedHttpStatus) "expected-refusal-http-status"
    Assert-Acceptance (-not [String]::IsNullOrWhiteSpace($responseText) -and $responseText.Length -le 256KB) "expected-refusal-body-bounded"
    try {
        $body = ConvertFrom-JsonPreservingStrings $responseText
    }
    catch {
        throw [InvalidOperationException]::new("Bridge refusal returned invalid JSON for $Method.")
    }
    $topLevel = @($body.PSObject.Properties.Name)
    Assert-Acceptance ($topLevel.Count -eq 4 -and
        $topLevel -ccontains "ok" -and $topLevel -ccontains "error" -and
        $topLevel -ccontains "taxonomy" -and $topLevel -ccontains "callId") "expected-refusal-allowlisted-envelope"
    Assert-Acceptance ($body.ok -eq $false) "expected-refusal-result"
    Assert-Acceptance ([string]$body.callId -ceq $callId) "expected-refusal-command-identity"
    Assert-Acceptance ([string]$body.error.code -ceq $ExpectedErrorCode) "expected-refusal-error-code"
    Assert-Acceptance ([string]$body.taxonomy.code -ceq $ExpectedTaxonomyCode) "expected-refusal-taxonomy"
    Assert-Acceptance ($body.taxonomy.retriable -eq $true) "expected-refusal-retriable"
    Assert-Acceptance ([string]$body.taxonomy.recoveryHint -ceq $ExpectedRecoveryHint) "expected-refusal-recovery-hint"
    $responseText = $null
    $body = $null
    return [pscustomobject]@{
        httpStatus = $statusCode
        errorCode = $ExpectedErrorCode
        taxonomyCode = $ExpectedTaxonomyCode
        recoveryHint = $ExpectedRecoveryHint
    }
}

function Invoke-Health {
    $script:ActiveStage = "preflight"
    try {
        $response = Invoke-WebRequest -UseBasicParsing -Uri $script:HealthUri -Method Get -TimeoutSec 10
        Assert-Acceptance ([int]$response.StatusCode -eq 200) "health-http-ok"
        return ConvertFrom-JsonPreservingStrings $response.Content
    }
    catch {
        throw [InvalidOperationException]::new("Candidate health preflight failed.")
    }
}

function Confirm-MethodPassed {
    param([Parameter(Mandatory = $true)][string]$Method)
    Assert-Acceptance ($ActionMethods -ccontains $Method) "known-method-mark"
    Assert-Acceptance ($MethodCommandInvoked[$Method] -eq $true) "method-command-was-invoked"
    Assert-Acceptance ($MethodResultVerified[$Method] -eq $true) "method-result-was-verified"
    $MethodPostconditionVerified[$Method] = $true
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

    $viewportWidth = [double]$snapshot.viewport.width
    $viewportHeight = [double]$snapshot.viewport.height
    $deviceScale = [double]$snapshot.viewport.devicePixelRatio
    foreach ($dimension in @($viewportWidth, $viewportHeight, $deviceScale)) {
        Assert-Acceptance (-not [double]::IsNaN($dimension) -and -not [double]::IsInfinity($dimension) -and $dimension -gt 0) "observation-viewport-finite"
    }
    Assert-Acceptance ($viewportWidth -le 8192 -and $viewportHeight -le 8192 -and $deviceScale -le 16) "observation-viewport-bounded"

    # The raw data URL and its binding are validated only in memory. Neither
    # enters the reduced record, errors, stdout, or any temporary file.
    $screenshotData = [string]$response.result.screenshot
    $screenshotMatch = [regex]::Match($screenshotData, '^data:image/(?<format>png|jpeg);base64,(?<payload>[A-Za-z0-9+/]+={0,2})$')
    Assert-Acceptance $screenshotMatch.Success "observation-screenshot-data-url"
    Assert-Acceptance ($screenshotMatch.Groups["payload"].Value.Length -le 12MB) "observation-screenshot-encoding-bounded"
    $screenshotBytes = [Convert]::FromBase64String($screenshotMatch.Groups["payload"].Value)
    Assert-Acceptance ($screenshotBytes.Length -gt 0 -and $screenshotBytes.Length -le 8MB) "observation-screenshot-bytes-bounded"

    $published = $response.state.observation
    Assert-Acceptance ([string]$published.generation -ceq $generation) "observation-state-generation-bound"
    $screenshotHash = Get-BytesSha256 $screenshotBytes
    Assert-Acceptance ([string]$published.contentHash -ceq $screenshotHash) "observation-screenshot-hash-bound"
    $screenshotWidth = [long]$published.screenshotWidth
    $screenshotHeight = [long]$published.screenshotHeight
    Assert-Acceptance ($screenshotWidth -gt 0 -and $screenshotWidth -le 8192) "observation-screenshot-width"
    Assert-Acceptance ($screenshotHeight -gt 0 -and $screenshotHeight -le 8192) "observation-screenshot-height"
    Assert-Acceptance ($screenshotWidth * $screenshotHeight -le 50MB) "observation-screenshot-pixels"
    $screenshotBinding = [string]$published.screenshotUrl
    $bindingIdentity = [Text.Encoding]::UTF8.GetBytes("$TabId`:$generation")
    $expectedBinding = [Convert]::ToBase64String($bindingIdentity).TrimEnd('=').Replace('+', '-').Replace('/', '_')
    $expectedBindingPattern = '^/api/screenshot\?id=[0-9a-f]{32}&binding=browser-tab-generation\.' + [regex]::Escape($expectedBinding) + '$'
    Assert-Acceptance ([regex]::IsMatch($screenshotBinding, $expectedBindingPattern,
        [Text.RegularExpressions.RegexOptions]::CultureInvariant)) "observation-screenshot-generation-binding"
    $screenshotBytes = $null
    $screenshotData = $null
    $screenshotBinding = $null
    $bindingIdentity = $null
    $expectedBinding = $null

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

function Get-VisibleObservedElement {
    param(
        [Parameter(Mandatory = $true)][long]$TabId,
        [Parameter(Mandatory = $true)]$Observation,
        [Parameter(Mandatory = $true)][string]$Role,
        [Parameter(Mandatory = $true)][string]$Name
    )
    $current = $Observation
    for ($attempt = 0; $attempt -lt 5; $attempt += 1) {
        $element = Get-ObservedElement $current $Role $Name
        if ($element.inViewport -eq $true) {
            return [pscustomobject]@{ Observation = $current; Element = $element }
        }
        $viewportHeight = [double]$current.viewport.height
        Assert-Acceptance ($viewportHeight -gt 0) "viewport-height-valid"
        $targetCenter = [double]$element.bounds.y + ([double]$element.bounds.height / 2.0)
        $deltaY = [Math]::Max(-5000.0, [Math]::Min(5000.0, [Math]::Round($targetCenter - ($viewportHeight / 2.0))))
        if ([Math]::Abs($deltaY) -lt 1) {
            $deltaY = if ($targetCenter -lt 0) { -1 } else { 1 }
        }
        $scrolled = Invoke-BridgeCommand "page.scroll" ([ordered]@{
            tabId = $TabId
            generation = [string]$current.generation
            deltaX = 0
            deltaY = $deltaY
        })
        Assert-Acceptance ($scrolled.result.snapshotInvalidated -eq $true) "target-scroll-invalidated-snapshot"
        Register-PageMutation
        $current = Get-PageMutationObservation $TabId
        Confirm-MethodPassed "page.scroll"
    }
    throw [InvalidOperationException]::new("Acceptance target did not become visible after bounded scrolling.")
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

function Wait-ForReleasedControl {
    param(
        [Parameter(Mandatory = $true)][string]$ExpectedReason,
        [long]$DeadlineEpochMilliseconds = 0,
        [switch]$ReturnRevocationAt
    )
    $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $deadline = if ($DeadlineEpochMilliseconds -gt 0) {
        $DeadlineEpochMilliseconds
    }
    else {
        $now + 15000
    }
    Assert-Acceptance ($deadline -gt $now -and $deadline -le $now + 30000) "released-control-deadline-bounded"
    do {
        $reply = Invoke-BridgeCommand "browser.control.status" ([ordered]@{})
        $control = $reply.result
        $revocation = $control.revocation
        if ($control.active -eq $false -and $control.humanPaused -eq $false -and
            $control.revocationPending -eq $false -and $null -eq $control.humanPause -and
            $null -ne $revocation -and [string]$revocation.reason -ceq $ExpectedReason -and
            $revocation.requiresExplicitStart -eq $true) {
            if ($ReturnRevocationAt) {
                $revokedAt = [long]$revocation.at
                $observedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                Assert-Acceptance ($revokedAt -gt 0 -and $revokedAt -le $observedAt) "released-control-revocation-time"
                return $revokedAt
            }
            return
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() -lt $deadline)
    throw [InvalidOperationException]::new("Browser control did not reach the required released state.")
}

function Wait-UntilEpochMilliseconds {
    param([Parameter(Mandatory = $true)][long]$Target)
    $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    Assert-Acceptance ($Target -ge $now - 30000 -and $Target -le $now + 30000) "fixture-deadline-bounded"
    while (($remaining = $Target - [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) -gt 0) {
        Start-Sleep -Milliseconds ([Math]::Min(100, [int]$remaining))
    }
}

function Assert-FixtureTiming {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $attackAt = [long]$Value.attackAt
    $cleanupAt = [long]$Value.cleanupAt
    Assert-Acceptance ($Value.scheduled -eq $true) "$Label-scheduled"
    Assert-Acceptance ($attackAt -gt $now -and $attackAt -le $now + 15000) "$Label-attack-time"
    Assert-Acceptance ($cleanupAt -ge $attackAt + 7000 -and $cleanupAt -le $attackAt + 12000) "$Label-cleanup-time"
    return [pscustomobject]@{ AttackAt = $attackAt; CleanupAt = $cleanupAt }
}

function Assert-ControlActiveBeforeFixture {
    param(
        [Parameter(Mandatory = $true)][long]$AttackAt,
        [Parameter(Mandatory = $true)][string]$Label
    )
    Wait-UntilEpochMilliseconds ($AttackAt - 1500)
    $status = Invoke-BridgeCommand "browser.control.status" ([ordered]@{})
    $checkedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    Assert-Acceptance ($checkedAt -lt $AttackAt) "$Label-pre-attack-check-timely"
    Assert-Acceptance ($status.result.active -eq $true) "$Label-pre-attack-active"
    Assert-Acceptance ($status.result.humanPaused -eq $false) "$Label-pre-attack-unpaused"
    Assert-Acceptance ($status.result.revocationPending -eq $false) "$Label-pre-attack-clean"
}

function Complete-ControlUiRevocationFixture {
    param(
        [Parameter(Mandatory = $true)][long]$TabId,
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $timing = Assert-FixtureTiming $Value $Label
    Register-PageMutation
    Assert-ControlActiveBeforeFixture $timing.AttackAt $Label
    Wait-UntilEpochMilliseconds ($timing.AttackAt + 50)
    $revokedAt = Wait-ForReleasedControl "control_ui_hidden" `
        -DeadlineEpochMilliseconds ($timing.AttackAt + 3000) -ReturnRevocationAt
    Assert-Acceptance ($revokedAt -ge $timing.AttackAt -and
        $revokedAt -le $timing.AttackAt + 3000) "$Label-bounded-revocation"
    $script:ControlMayBeActive = $false
    Wait-UntilEpochMilliseconds ($timing.CleanupAt + 250)
    $fresh = Restart-DemoControlAfterFixture $TabId $Label
    return [pscustomobject]@{
        Observation = $fresh
        AttackAt = $timing.AttackAt
        CleanupAt = $timing.CleanupAt
        RevokedAt = $revokedAt
    }
}

function Get-PublicControlHostIdentity {
    param([Parameter(Mandatory = $true)][long]$TabId)
    $reply = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $TabId
        expression = 'document.querySelector(''[aria-label="Local Browser Bridge browser control"][popover]'')?.id || ""'
    })
    Assert-Acceptance ([string]$reply.result.type -ceq "string") "control-host-identity-type"
    $identity = [string]$reply.result.value
    Assert-Acceptance ([regex]::IsMatch(
        $identity,
        '^__local_browser_bridge_control_[0-9a-f]{32}__$',
        [Text.RegularExpressions.RegexOptions]::CultureInvariant
    )) "control-host-identity-shape"
    return $identity
}

function Restart-DemoControlAfterFixture {
    param(
        [Parameter(Mandatory = $true)][long]$TabId,
        [Parameter(Mandatory = $true)][string]$Label
    )
    # A granted restart must always be cleaned up even if a later response
    # assertion fails, so conservatively assume authority before the call.
    $script:ControlMayBeActive = $true
    $restart = Invoke-BridgeCommand "browser.control.start" ([ordered]@{
        tabId = $TabId
        ttlMs = 300000
    })
    Assert-Acceptance ($restart.result.active -eq $true) "$Label-explicit-restart"
    Assert-Acceptance ([long]$restart.result.tabId -eq $TabId) "$Label-restart-target-bound"
    $status = Invoke-BridgeCommand "browser.control.status" ([ordered]@{})
    Assert-Acceptance ($status.result.active -eq $true) "$Label-restart-status-active"
    Assert-Acceptance ($status.result.humanPaused -eq $false) "$Label-restart-status-unpaused"
    Assert-Acceptance ($null -eq $status.result.humanPause) "$Label-restart-status-no-human-pause"
    Assert-Acceptance ($status.result.revocationPending -eq $false) "$Label-restart-status-clean"
    return Get-PageMutationObservation $TabId
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

    $createdDemo = Invoke-BridgeCommand "tabs.new" ([ordered]@{ url = $demoUrl })
    $targetTabId = [long]$createdDemo.result.tabId
    $targetGroupId = [long]$createdDemo.result.groupId
    # Track a valid bridge-returned ID before any later response/postcondition
    # assertion can fail, so failure cleanup never loses an already-created tab.
    if ($createdDemo.result.bridgeCreated -eq $true -and $targetTabId -gt 0) {
        [void]$script:OwnedTabs.Add($targetTabId)
        [void]$script:ClosableTabs.Add($targetTabId)
    }
    Assert-Acceptance ($createdDemo.result.bridgeCreated -eq $true) "bridge-demo-created"
    Assert-Acceptance ($targetTabId -gt 0) "created-demo-tab-id-valid"
    Assert-Acceptance ($targetGroupId -ge 0) "created-demo-group-id-valid"

    $demoReconciled = $false
    $demoDeadline = [DateTime]::UtcNow.AddSeconds(12)
    do {
        $afterDemoCreate = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
        $newDemoTabs = @($afterDemoCreate.result.tabs | Where-Object {
            [long]$_.id -eq $targetTabId
        })
        Assert-Acceptance ($newDemoTabs.Count -le 1) "single-created-demo-tab"
        if ($newDemoTabs.Count -eq 1) {
            Assert-Acceptance ([long]$newDemoTabs[0].id -eq $targetTabId) "created-demo-tab-reconciled"
            if ([string]$newDemoTabs[0].url -ceq $demoUrl -and
                [string]$newDemoTabs[0].title -ceq "Bridge Demo Target") {
                $demoReconciled = $true
                break
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $demoDeadline)
    Assert-Acceptance $demoReconciled "created-demo-url-reconciled"
    Confirm-MethodPassed "tabs.new"
    Confirm-MethodPassed "tabs.list"
    $AggregateAssertions["dynamicTargetDiscovery"] = $true

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
    Assert-Acceptance ($control.result.humanPaused -eq $false) "control-status-unpaused"
    Assert-Acceptance ($null -eq $control.result.humanPause) "control-status-no-human-pause"
    Assert-Acceptance ($control.result.revocationPending -eq $false) "control-status-clean"
    Confirm-MethodPassed "browser.control.status"

    $createdBlank = Invoke-BridgeCommand "tabs.new" ([ordered]@{})
    $blankTabId = [long]$createdBlank.result.tabId
    if ($createdBlank.result.bridgeCreated -eq $true -and $blankTabId -gt 0) {
        [void]$script:OwnedTabs.Add($blankTabId)
        [void]$script:ClosableTabs.Add($blankTabId)
    }
    Assert-Acceptance ($createdBlank.result.bridgeCreated -eq $true) "bridge-blank-created"
    Assert-Acceptance ($blankTabId -gt 0) "created-tab-id-valid"
    Assert-Acceptance ($blankTabId -ne $targetTabId) "created-tab-distinct"

    $afterCreate = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
    $newTabs = @($afterCreate.result.tabs | Where-Object {
        [long]$_.id -eq $blankTabId
    })
    Assert-Acceptance ($newTabs.Count -eq 1) "single-new-tab"
    Assert-Acceptance ([long]$newTabs[0].id -eq $blankTabId) "created-tab-reconciled"
    Assert-Acceptance ([string]$newTabs[0].url -ceq "about:blank") "created-tab-about-blank"
    # Activation itself is a tab API check. Its automatic post-action page
    # observation cannot inject the control indicator into top-level
    # about:blank and may release the demo lease. The runner never starts or
    # renews control on this blank tab; it closes it and explicitly reacquires
    # the bridge-created HTTP demo below.
    $blankActivated = Invoke-BridgeCommand "tabs.activate" ([ordered]@{ tabId = $blankTabId })
    Assert-Acceptance ($blankActivated.result.active -eq $true) "blank-tab-active"
    Assert-Acceptance ([long]$blankActivated.result.tabId -eq $blankTabId) "blank-activation-target-bound"

    $blankClosed = Invoke-BridgeCommand "tabs.close" ([ordered]@{ tabId = $blankTabId })
    Assert-Acceptance ($blankClosed.result.closed -eq $true) "blank-tab-closed"

    $afterBlankClose = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
    Assert-Acceptance (@($afterBlankClose.result.tabs | Where-Object {
        [long]$_.id -eq $blankTabId
    }).Count -eq 0) "blank-close-reconciled"
    [void]$script:ClosableTabs.Remove($blankTabId)
    [void]$script:OwnedTabs.Remove($blankTabId)
    Confirm-MethodPassed "tabs.close"

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
    Assert-Acceptance ($reacquiredStatus.result.humanPaused -eq $false) "reacquired-control-unpaused"
    Assert-Acceptance ($null -eq $reacquiredStatus.result.humanPause) "reacquired-control-no-human-pause"
    Assert-Acceptance ($reacquiredStatus.result.revocationPending -eq $false) "reacquired-control-clean"

    Wait-ForDemoState $targetTabId $false
    $observation = Get-FreshObservation $targetTabId
    Assert-DedicatedDemoTab $targetTabId $demoUrl
    $initialControlHostIdentity = Get-PublicControlHostIdentity $targetTabId

    $navigated = Invoke-BridgeCommand "page.navigate" ([ordered]@{
        tabId = $targetTabId
        url = $demoUrl
    })
    Assert-Acceptance ([long]$navigated.result.tabId -eq $targetTabId) "navigate-target-bound"
    Register-PageMutation
    Wait-ForDemoState $targetTabId $false
    $observation = Get-PageMutationObservation $targetTabId
    Assert-DedicatedDemoTab $targetTabId $demoUrl
    $navigatedControlHostIdentity = Get-PublicControlHostIdentity $targetTabId
    Assert-Acceptance ($navigatedControlHostIdentity -cne $initialControlHostIdentity) "navigate-control-host-rotated"
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
    $historyControlHostIdentity = Get-PublicControlHostIdentity $targetTabId
    Assert-Acceptance ($historyControlHostIdentity -cne $navigatedControlHostIdentity) "history-control-host-rotated"

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

    $preReloadControlHostIdentity = Get-PublicControlHostIdentity $targetTabId

    $reload = Invoke-BridgeCommand "page.reload" ([ordered]@{ tabId = $targetTabId })
    Assert-Acceptance ($reload.result.reloaded -eq $true) "reload-completed"
    Register-PageMutation
    Wait-ForDemoState $targetTabId $true
    $observation = Get-PageMutationObservation $targetTabId
    Assert-DedicatedDemoTab $targetTabId $demoUrl
    $reloadedControlHostIdentity = Get-PublicControlHostIdentity $targetTabId
    Assert-Acceptance ($reloadedControlHostIdentity -cne $preReloadControlHostIdentity) "reload-control-host-rotated"
    Confirm-MethodPassed "page.reload"

    # `/demo.css` installs hostile generic popover, pseudo-element, and
    # backdrop rules before this document's control host is created. The
    # extension must neutralize those page-author declarations. Keep the raw
    # computed styles in page memory and return only fixed booleans; the later
    # tight screenshot is the independent proof that real pixels are visible.
    $hostileStyleState = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const host = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  if (!host) {
    return {
      hostPaintSafe: false,
      beforeSuppressed: false,
      afterSuppressed: false,
      backdropExposed: false,
      backdropSafe: false,
      earlyStopCaptureListenersArmed: false
    };
  }
  const hostStyle = getComputedStyle(host);
  const before = getComputedStyle(host, '::before');
  const after = getComputedStyle(host, '::after');
  const pseudoSuppressed = (style) => style.content === 'none' && style.display === 'none';
  let backdropExposed = false;
  let backdropSafe = true;
  try {
    const backdrop = getComputedStyle(host, '::backdrop');
    backdropExposed = Boolean(backdrop
      && typeof backdrop.backgroundColor === 'string'
      && backdrop.backgroundColor.length > 0);
    if (backdropExposed) {
      const transparent = backdrop.backgroundColor === 'transparent'
        || backdrop.backgroundColor === 'rgba(0, 0, 0, 0)';
      const noneIfExposed = (value) => value === undefined || value === '' || value === 'none';
      backdropSafe = transparent
        && backdrop.backgroundImage === 'none'
        && Number(backdrop.opacity) === 1
        && backdrop.filter === 'none'
        && noneIfExposed(backdrop.backdropFilter)
        && noneIfExposed(backdrop.webkitBackdropFilter)
        && noneIfExposed(backdrop.maskImage)
        && noneIfExposed(backdrop.webkitMaskImage)
        && noneIfExposed(backdrop.clipPath)
        && noneIfExposed(backdrop.transform)
        && backdrop.pointerEvents === 'none';
    }
  } catch {
    backdropExposed = false;
    backdropSafe = true;
  }
  return {
    hostPaintSafe: host.isConnected
      && host.matches(':popover-open')
      && Number(hostStyle.opacity) === 1
      && hostStyle.filter === 'none'
      && hostStyle.maskImage === 'none'
      && hostStyle.transform === 'none',
    beforeSuppressed: pseudoSuppressed(before),
    afterSuppressed: pseudoSuppressed(after),
    backdropExposed,
    backdropSafe,
    earlyStopCaptureListenersArmed:
      document.documentElement.dataset.hostileStopCaptureListeners === 'armed'
  };
})()
'@
    })
    Assert-Acceptance ([string]$hostileStyleState.result.type -ceq "object") "hostile-style-result-type"
    Assert-Acceptance ($hostileStyleState.result.value.hostPaintSafe -eq $true) "hostile-host-paint-safe"
    Assert-Acceptance ($hostileStyleState.result.value.beforeSuppressed -eq $true) "hostile-before-suppressed"
    Assert-Acceptance ($hostileStyleState.result.value.afterSuppressed -eq $true) "hostile-after-suppressed"
    Assert-Acceptance ($hostileStyleState.result.value.backdropSafe -eq $true) "hostile-backdrop-safe-if-exposed"
    Assert-Acceptance ($hostileStyleState.result.value.earlyStopCaptureListenersArmed -eq $true) "hostile-stop-capture-listeners-armed"

    # Every controlled evaluation and its automatic observation cause the
    # bridge itself to hide/show and re-top the genuine host. Those trusted
    # root top-layer events must not be confused with a later page loss. Two
    # complete command/auto-observe cycles prove the exact direct-root host
    # remains active before any hostile fixture mutates it.
    for ($cleanRootAttempt = 0; $cleanRootAttempt -lt 2; $cleanRootAttempt += 1) {
        $cleanRootShow = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
            tabId = $targetTabId
            expression = @'
(() => {
  const host = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  const root = document.documentElement;
  return {
    hostDirect: Boolean(host && host.parentNode === root && host.parentElement === root),
    hostOpen: host?.matches(':popover-open') === true,
    hostAccessible: Boolean(host && !host.hidden && !host.inert && host.getAttribute('aria-hidden') === 'false'),
    rootAccessible: Boolean(root && !root.hidden && !root.inert && root.getAttribute('aria-hidden') !== 'true')
  };
})()
'@
        })
        Assert-Acceptance ([string]$cleanRootShow.result.type -ceq "object") "clean-root-event-result-type"
        foreach ($name in @("hostDirect", "hostOpen", "hostAccessible", "rootAccessible")) {
            Assert-Acceptance ($cleanRootShow.result.value.$name -eq $true) "clean-root-event-$name"
        }
        $cleanRootStatus = Invoke-BridgeCommand "browser.control.status" ([ordered]@{})
        Assert-Acceptance ($cleanRootStatus.result.active -eq $true) "clean-root-event-control-active"
        Assert-Acceptance ($cleanRootStatus.result.humanPaused -eq $false) "clean-root-event-control-unpaused"
        Assert-Acceptance ($cleanRootStatus.result.revocationPending -eq $false) "clean-root-event-control-clean"
    }
    $observation = Get-FreshObservation $targetTabId

    # A top-layer stack is per document. A benign same-process child popover
    # may appear in Chrome's flattened DOM-domain result, but it paints inside
    # its iframe and must not falsely revoke the top-document control UI. The
    # second command enters through the full browser-process reuse gate while
    # the child popover is still open, then closes only that fixture.
    $childPopoverOpened = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const frame = document.getElementById('top-layer-child-frame');
  const child = frame?.contentDocument?.getElementById('child-popover');
  if (!child || typeof child.showPopover !== 'function') return { available: false, open: false };
  if (child.matches(':popover-open')) child.hidePopover();
  child.showPopover();
  return { available: true, open: child.matches(':popover-open') };
})()
'@
    })
    Assert-Acceptance ([string]$childPopoverOpened.result.type -ceq "object") "child-popover-result-type"
    Assert-Acceptance ($childPopoverOpened.result.value.available -eq $true) "child-popover-available"
    Assert-Acceptance ($childPopoverOpened.result.value.open -eq $true) "child-popover-open"
    $childPopoverSurvived = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const child = document.getElementById('top-layer-child-frame')?.contentDocument?.getElementById('child-popover');
  const survived = child?.matches(':popover-open') === true;
  if (survived) child.hidePopover();
  return survived;
})()
'@
    })
    Assert-Acceptance ([string]$childPopoverSurvived.result.type -ceq "boolean") "child-popover-survival-type"
    Assert-Acceptance ($childPopoverSurvived.result.value -eq $true) "child-popover-did-not-falsely-revoke"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId

    # Exercise the same document-scoping rule beyond the product's former
    # 32-frame map bound. Each same-origin srcdoc owns an open manual popover,
    # but none belongs to the controlled top-level document. Keep all of them
    # open across a second gated command, then remove only this test fixture.
    # Counts and frame state stay in memory and never enter reduced evidence.
    $childPopoverSwarmOpened = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(async () => {
  const prior = document.getElementById('top-layer-child-swarm');
  if (prior) prior.remove();
  const container = document.createElement('div');
  container.id = 'top-layer-child-swarm';
  container.style.cssText = 'position:absolute;left:0;top:0;width:1px;height:1px;overflow:hidden;pointer-events:none';
  const count = 33;
  const loaded = [];
  for (let index = 0; index < count; index += 1) {
    const frame = document.createElement('iframe');
    frame.title = `Benign child top-layer fixture ${index + 1}`;
    frame.style.cssText = 'width:1px;height:1px;border:0';
    frame.srcdoc = `<!doctype html><div id="child" popover="manual">${index + 1}</div>`;
    loaded.push(new Promise((resolve) => {
      frame.addEventListener('load', () => resolve(true), { once: true });
      frame.addEventListener('error', () => resolve(false), { once: true });
    }));
    container.append(frame);
  }
  document.body.append(container);
  const settled = await Promise.race([
    Promise.all(loaded),
    new Promise((resolve) => setTimeout(() => resolve([]), 5000))
  ]);
  let openCount = 0;
  for (const frame of container.querySelectorAll('iframe')) {
    const child = frame.contentDocument?.getElementById('child');
    if (!child || typeof child.showPopover !== 'function') continue;
    child.showPopover();
    if (child.matches(':popover-open')) openCount += 1;
  }
  return {
    available: Array.isArray(settled) && settled.length === count,
    count,
    openCount,
    aboveLegacyBound: count > 32
  };
})()
'@
    })
    Assert-Acceptance ([string]$childPopoverSwarmOpened.result.type -ceq "object") "child-popover-swarm-result-type"
    Assert-Acceptance ($childPopoverSwarmOpened.result.value.available -eq $true) "child-popover-swarm-loaded"
    Assert-Acceptance ([int]$childPopoverSwarmOpened.result.value.count -eq 33) "child-popover-swarm-count"
    Assert-Acceptance ([int]$childPopoverSwarmOpened.result.value.openCount -eq 33) "child-popover-swarm-open"
    Assert-Acceptance ($childPopoverSwarmOpened.result.value.aboveLegacyBound -eq $true) "child-popover-swarm-above-legacy-bound"

    $childPopoverSwarmSurvived = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const container = document.getElementById('top-layer-child-swarm');
  const frames = [...(container?.querySelectorAll('iframe') || [])];
  const openCount = frames.filter((frame) => (
    frame.contentDocument?.getElementById('child')?.matches(':popover-open') === true
  )).length;
  for (const frame of frames) {
    const child = frame.contentDocument?.getElementById('child');
    if (child?.matches(':popover-open')) child.hidePopover();
  }
  container?.remove();
  return { survived: frames.length === 33 && openCount === 33, removed: !container?.isConnected };
})()
'@
    })
    Assert-Acceptance ([string]$childPopoverSwarmSurvived.result.type -ceq "object") "child-popover-swarm-survival-type"
    Assert-Acceptance ($childPopoverSwarmSurvived.result.value.survived -eq $true) "child-popover-swarm-did-not-falsely-revoke"
    Assert-Acceptance ($childPopoverSwarmSurvived.result.value.removed -eq $true) "child-popover-swarm-removed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId

    # A page-owned, opaque, pointer-events:none manual popover opened after the
    # bridge wins its document's top-layer order without winning ordinary hit
    # testing. The next controlled evaluation can succeed only after the
    # extension re-tops and the browser-process stack/point proofs bind the
    # exact closed-shadow host. The passive fixture stays open below the host
    # for the later tight visual crop.
    $fixtureOpened = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const bridge = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  const fixture = document.getElementById('top-layer-control-fixture');
  if (!bridge || !fixture || !bridge.matches(':popover-open')) return { fixtureOpen: false, passive: false };
  fixture.classList.add('pass-through');
  if (fixture.matches(':popover-open')) fixture.hidePopover();
  fixture.showPopover();
  return {
    fixtureOpen: fixture.matches(':popover-open'),
    passive: getComputedStyle(fixture).pointerEvents === 'none'
  };
})()
'@
    })
    Assert-Acceptance ([string]$fixtureOpened.result.type -ceq "object") "top-layer-fixture-result-type"
    Assert-Acceptance ($fixtureOpened.result.value.fixtureOpen -eq $true) "top-layer-fixture-open"
    Assert-Acceptance ($fixtureOpened.result.value.passive -eq $true) "page-popover-passive"

    $fixtureRetopped = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const bridge = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  const fixture = document.getElementById('top-layer-control-fixture');
  if (!bridge || !fixture) return { fixtureOpen: false, controlRetopped: false };
  const rect = bridge.getBoundingClientRect();
  const hit = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
  const controlRetopped = hit === bridge || bridge.contains(hit);
  return { fixtureOpen: fixture.matches(':popover-open'), controlRetopped };
})()
'@
    })
    Assert-Acceptance ([string]$fixtureRetopped.result.type -ceq "object") "top-layer-retop-result-type"
    Assert-Acceptance ($fixtureRetopped.result.value.fixtureOpen -eq $true) "top-layer-fixture-still-open"
    Assert-Acceptance ($fixtureRetopped.result.value.controlRetopped -eq $true) "control-ui-retopped"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId

    $visibleTarget = Get-VisibleObservedElement $targetTabId $observation "textbox" "Display name"
    $observation = $visibleTarget.Observation
    $nameField = $visibleTarget.Element
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

    $visibleTarget = Get-VisibleObservedElement $targetTabId $observation "select" "Favorite color"
    $observation = $visibleTarget.Observation
    $colorField = $visibleTarget.Element
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

    $visibleTarget = Get-VisibleObservedElement $targetTabId $observation "button" "Show greeting"
    $observation = $visibleTarget.Observation
    $submitButton = $visibleTarget.Element
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

    $visibleTarget = Get-VisibleObservedElement $targetTabId $observation "textbox" "Keyboard target"
    $observation = $visibleTarget.Observation
    $keyboardField = $visibleTarget.Element
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

    $visibleTarget = Get-VisibleObservedElement $targetTabId $observation "button" "Coordinate target"
    $observation = $visibleTarget.Observation
    $coordinateButton = $visibleTarget.Element
    $hovered = Invoke-BridgeCommand "page.hover" ([ordered]@{
        tabId = $targetTabId
        ref = [string]$coordinateButton.ref
        generation = [string]$observation.generation
    })
    Assert-Acceptance ($hovered.result.hovered -eq $true -and $hovered.result.trusted -eq $true) "hover-completed"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Confirm-MethodPassed "page.hover"

    $visibleTarget = Get-VisibleObservedElement $targetTabId $observation "button" "Coordinate target"
    $observation = $visibleTarget.Observation
    $coordinateButton = $visibleTarget.Element
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

    # A page-created display:contents wrapper can copy the randomized public
    # host ID, but it is not the retained bridge host object. Mutations below
    # that fake must invalidate an already published snapshot. The first click
    # therefore has to fail with exact STALE_SNAPSHOT; only an explicit fresh
    # observation may authorize the same local button afterward.
    $snapshotExclusionScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const host = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  const form = document.getElementById('demo-form');
  if (!host || !form || globalThis.__lbbSnapshotExclusionAttack) {
    return { scheduled: false, attackAt: 0, cleanupAt: 0, duplicateId: false, displayContents: false };
  }
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 8000;
  const originalParent = form.parentNode;
  const originalNext = form.nextSibling;
  const originalAction = form.getAttribute('action');
  const fake = document.createElement('div');
  fake.id = host.id;
  fake.style.display = 'contents';
  fake.setAttribute('data-local-fixture', 'snapshot-exclusion');
  document.body.append(fake);
  const state = {
    scheduled: true,
    attackAt,
    cleanupAt,
    actualAttackAt: 0,
    attackApplied: false,
    fakeRemoved: false,
    formRestored: false,
    actionRestored: false
  };
  globalThis.__lbbSnapshotExclusionAttack = state;
  setTimeout(() => {
    state.actualAttackAt = Date.now();
    fake.append(form);
    form.setAttribute('action', '/demo?snapshot-exclusion=mutated');
    state.attackApplied = form.parentNode === fake
      && form.getAttribute('action') === '/demo?snapshot-exclusion=mutated';
    setTimeout(() => {
      const before = originalNext && originalNext.parentNode === originalParent ? originalNext : null;
      originalParent.insertBefore(form, before);
      if (originalAction === null) form.removeAttribute('action');
      else form.setAttribute('action', originalAction);
      fake.remove();
      state.fakeRemoved = !fake.isConnected;
      state.formRestored = form.parentNode === originalParent;
      state.actionRestored = form.getAttribute('action') === originalAction;
      state.cleanupAtActual = Date.now();
    }, 8000);
  }, 7000);
  return {
    scheduled: true,
    attackAt,
    cleanupAt,
    duplicateId: fake.id === host.id,
    displayContents: getComputedStyle(fake).display === 'contents'
  };
})()
'@
    })
    Assert-Acceptance ([string]$snapshotExclusionScheduled.result.type -ceq "object") "snapshot-exclusion-schedule-type"
    $snapshotExclusionTiming = Assert-FixtureTiming $snapshotExclusionScheduled.result.value "snapshot-exclusion"
    Assert-Acceptance ($snapshotExclusionScheduled.result.value.duplicateId -eq $true) "snapshot-exclusion-duplicate-id"
    Assert-Acceptance ($snapshotExclusionScheduled.result.value.displayContents -eq $true) "snapshot-exclusion-display-contents"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    $staleSubmit = Get-VisibleObservedElement $targetTabId $observation "button" "Show greeting"
    $observation = $staleSubmit.Observation
    Assert-ControlActiveBeforeFixture $snapshotExclusionTiming.AttackAt "snapshot-exclusion"
    # Do not probe or mutate any object outside the duplicate-ID fake here: an
    # unrelated mutation would invalidate the snapshot and let this negative
    # pass even if exact-object exclusion were broken. The later cleanup query
    # proves the scheduled attack actually ran at the bounded timestamp.
    Wait-UntilEpochMilliseconds ($snapshotExclusionTiming.AttackAt + 250)
    $staleSnapshotRefusal = Invoke-BridgeCommandExpectError `
        -Method "page.click" `
        -Params ([ordered]@{
            tabId = $targetTabId
            ref = [string]$staleSubmit.Element.ref
            generation = [string]$observation.generation
            button = "left"
            clickCount = 1
        }) `
        -ExpectedHttpStatus 409 `
        -ExpectedErrorCode "STALE_SNAPSHOT" `
        -ExpectedTaxonomyCode "stale_snapshot" `
        -ExpectedRecoveryHint "reobserve"
    Assert-Acceptance ($staleSnapshotRefusal.httpStatus -eq 409 -and
        $staleSnapshotRefusal.errorCode -ceq "STALE_SNAPSHOT") "snapshot-exclusion-stale-refusal"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    $freshSubmit = Get-VisibleObservedElement $targetTabId $observation "button" "Show greeting"
    $observation = $freshSubmit.Observation
    $freshSnapshotClick = Invoke-BridgeCommand "page.click" ([ordered]@{
        tabId = $targetTabId
        ref = [string]$freshSubmit.Element.ref
        generation = [string]$observation.generation
        button = "left"
        clickCount = 1
    })
    Assert-Acceptance ($freshSnapshotClick.result.clicked -eq $true -and
        $freshSnapshotClick.result.trusted -eq $true) "snapshot-exclusion-fresh-click"
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    Assert-Acceptance ([string]$observation.bodyText -like "*Hello, Bridge Matrix. blue selected.*") "snapshot-exclusion-fresh-action-observed"
    Wait-UntilEpochMilliseconds ($snapshotExclusionTiming.CleanupAt + 250)
    Register-PageMutation
    $observation = Get-PageMutationObservation $targetTabId
    $snapshotExclusionCleanup = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const state = globalThis.__lbbSnapshotExclusionAttack;
  delete globalThis.__lbbSnapshotExclusionAttack;
  if (!state) return null;
  return {
    attackApplied: state.attackApplied === true,
    actualAttackAt: state.actualAttackAt,
    cleanupAtActual: state.cleanupAtActual,
    fakeRemoved: state.fakeRemoved === true,
    formRestored: state.formRestored === true,
    actionRestored: state.actionRestored === true
  };
})()
'@
    })
    Assert-Acceptance ([string]$snapshotExclusionCleanup.result.type -ceq "object") "snapshot-exclusion-cleanup-type"
    foreach ($name in @("attackApplied", "fakeRemoved", "formRestored", "actionRestored")) {
        Assert-Acceptance ($snapshotExclusionCleanup.result.value.$name -eq $true) "snapshot-exclusion-$name"
    }
    $snapshotAttackActual = [long]$snapshotExclusionCleanup.result.value.actualAttackAt
    $snapshotCleanupActual = [long]$snapshotExclusionCleanup.result.value.cleanupAtActual
    Assert-Acceptance ($snapshotAttackActual -ge $snapshotExclusionTiming.AttackAt -and
        $snapshotAttackActual -le $snapshotExclusionTiming.AttackAt + 1000) "snapshot-exclusion-attack-timing"
    Assert-Acceptance ($snapshotCleanupActual -ge $snapshotAttackActual + 7500 -and
        $snapshotCleanupActual -le $snapshotAttackActual + 9500) "snapshot-exclusion-cleanup-timing"

    # A later pointer-active page popover must revoke. Its seven-second delay
    # exceeds the automatic post-evaluate observation/capture bound, ensuring
    # no post-action acknowledgement accidentally re-tops the bridge after the
    # fixture opens. Poll only browser.control.status: status does not repaint
    # or reuse authority, so the exact release is attributable to the control-
    # UI safety path rather than a probing page action.
    $lateOccluderScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const bridge = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  const occluder = document.getElementById('top-layer-late-occluder');
  const label = occluder?.querySelector('strong');
  if (!bridge || !occluder || !label) return { scheduled: false, attackAt: 0, cleanupAt: 0, pointerActive: false };
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 8000;
  label.textContent = 'Hit-testable page occluder';
  setTimeout(() => {
    if (occluder.matches(':popover-open')) occluder.hidePopover();
    occluder.showPopover();
    const rect = bridge.getBoundingClientRect();
    const hit = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
    label.textContent = hit === occluder || occluder.contains(hit)
      ? 'Hit-testable page occluder verified'
      : 'Page occluder hit-test failed';
    setTimeout(() => { if (occluder.matches(':popover-open')) occluder.hidePopover(); }, 8000);
  }, 7000);
  return {
    scheduled: true,
    attackAt,
    cleanupAt,
    pointerActive: getComputedStyle(occluder).pointerEvents !== 'none'
  };
})()
'@
    })
    Assert-Acceptance ([string]$lateOccluderScheduled.result.type -ceq "object") "late-occluder-schedule-type"
    $lateTiming = Assert-FixtureTiming $lateOccluderScheduled.result.value "late-occluder"
    Assert-Acceptance ($lateOccluderScheduled.result.value.pointerActive -eq $true) "late-occluder-pointer-active"
    Register-PageMutation
    Assert-ControlActiveBeforeFixture $lateTiming.AttackAt "late-occluder"
    Wait-UntilEpochMilliseconds ($lateTiming.AttackAt + 50)
    $lateRevokedAt = Wait-ForReleasedControl "control_ui_hidden" `
        -DeadlineEpochMilliseconds ($lateTiming.AttackAt + 3000) -ReturnRevocationAt
    Assert-Acceptance ($lateRevokedAt -ge $lateTiming.AttackAt -and $lateRevokedAt -le $lateTiming.AttackAt + 3000) "late-occluder-bounded-revocation"
    $script:ControlMayBeActive = $false
    Wait-UntilEpochMilliseconds ($lateTiming.CleanupAt + 250)
    $observation = Restart-DemoControlAfterFixture $targetTabId "occlusion"

    # An opaque pointer-events:none popover reopens itself on every animation
    # frame. Ordinary DOM hit tests skip it. The 500 ms watchdog must still
    # fail closed through the browser-process stack and ignore-pointer-events
    # point proofs within one interval plus a 2.5 second scheduling margin—
    # safely before the independent ten-second heartbeat.
    $perpetualScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const occluder = document.getElementById('top-layer-perpetual-occluder');
  if (!occluder) return { scheduled: false, attackAt: 0, cleanupAt: 0, passive: false };
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 8000;
  const state = { active: false, frame: 0 };
  globalThis.__lbbPerpetualTopLayerAttack = state;
  const reopen = () => {
    if (!state.active) return;
    try {
      if (occluder.matches(':popover-open')) occluder.hidePopover();
      occluder.showPopover();
    } finally {
      state.frame = requestAnimationFrame(reopen);
    }
  };
  setTimeout(() => {
    state.active = true;
    reopen();
    setTimeout(() => {
      state.active = false;
      cancelAnimationFrame(state.frame);
      if (occluder.matches(':popover-open')) occluder.hidePopover();
      delete globalThis.__lbbPerpetualTopLayerAttack;
    }, 8000);
  }, 7000);
  return {
    scheduled: true,
    attackAt,
    cleanupAt,
    passive: getComputedStyle(occluder).pointerEvents === 'none'
  };
})()
'@
    })
    Assert-Acceptance ([string]$perpetualScheduled.result.type -ceq "object") "perpetual-occluder-schedule-type"
    $perpetualTiming = Assert-FixtureTiming $perpetualScheduled.result.value "perpetual-occluder"
    Assert-Acceptance ($perpetualScheduled.result.value.passive -eq $true) "perpetual-occluder-passive"
    Register-PageMutation
    Assert-ControlActiveBeforeFixture $perpetualTiming.AttackAt "perpetual-occluder"
    Wait-UntilEpochMilliseconds ($perpetualTiming.AttackAt + 50)
    $perpetualRevokedAt = Wait-ForReleasedControl "control_ui_hidden" `
        -DeadlineEpochMilliseconds ($perpetualTiming.AttackAt + 3000) -ReturnRevocationAt
    Assert-Acceptance ($perpetualRevokedAt -ge $perpetualTiming.AttackAt -and $perpetualRevokedAt -le $perpetualTiming.AttackAt + 3000) "perpetual-watchdog-revocation-bound"
    $script:ControlMayBeActive = $false
    Wait-UntilEpochMilliseconds ($perpetualTiming.CleanupAt + 250)
    $observation = Restart-DemoControlAfterFixture $targetTabId "perpetual-occluder"

    # Five stable browser hit points are necessary but not sufficient: a
    # narrow opaque strip can cover unsampled pixels while leaving every point
    # on the bridge host. This pointer-events:none popover occupies only the
    # top edge of the pill and re-tops every animation frame. The controlled-
    # root top-layer tail proof must still revoke it within the watchdog bound.
    $sparseOccluderScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const host = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  if (!host) return {
    scheduled: false, attackAt: 0, cleanupAt: 0, passive: false,
    opaque: false, sparse: false, stablePointRowsAvoided: false
  };
  const rect = host.getBoundingClientRect();
  if (rect.width < 40 || rect.height < 16) return {
    scheduled: false, attackAt: 0, cleanupAt: 0, passive: false,
    opaque: false, sparse: false, stablePointRowsAvoided: false
  };
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 8000;
  const stripHeight = Math.max(2, Math.min(4, Math.floor(rect.height / 8)));
  const strip = document.createElement('div');
  strip.className = 'top-layer-sparse-occluder';
  strip.setAttribute('popover', 'manual');
  strip.setAttribute('aria-hidden', 'true');
  strip.style.left = `${Math.round(rect.left + 1)}px`;
  strip.style.top = `${Math.round(rect.top + 1)}px`;
  strip.style.width = `${Math.max(8, Math.floor(rect.width - 2))}px`;
  strip.style.height = `${stripHeight}px`;
  document.documentElement.append(strip);
  const state = { active: false, frame: 0, strip };
  globalThis.__lbbSparseTopLayerAttack = state;
  const reopen = () => {
    if (!state.active || !strip.isConnected) return;
    try {
      if (strip.matches(':popover-open')) strip.hidePopover();
      strip.showPopover();
    } finally {
      state.frame = requestAnimationFrame(reopen);
    }
  };
  setTimeout(() => {
    state.active = true;
    reopen();
    setTimeout(() => {
      state.active = false;
      cancelAnimationFrame(state.frame);
      if (strip.matches(':popover-open')) strip.hidePopover();
      strip.remove();
      delete globalThis.__lbbSparseTopLayerAttack;
    }, 8000);
  }, 7000);
  const style = getComputedStyle(strip);
  const stripBottom = rect.top + 1 + stripHeight;
  return {
    scheduled: true,
    attackAt,
    cleanupAt,
    passive: style.pointerEvents === 'none',
    opaque: ['rgb(0, 0, 0)', 'rgba(0, 0, 0, 1)'].includes(style.backgroundColor),
    sparse: stripHeight < rect.height / 4,
    stablePointRowsAvoided: stripBottom < rect.top + (rect.height * 0.25)
  };
})()
'@
    })
    Assert-Acceptance ([string]$sparseOccluderScheduled.result.type -ceq "object") "sparse-occluder-schedule-type"
    $sparseTiming = Assert-FixtureTiming $sparseOccluderScheduled.result.value "sparse-occluder"
    Assert-Acceptance ($sparseOccluderScheduled.result.value.passive -eq $true) "sparse-occluder-passive"
    Assert-Acceptance ($sparseOccluderScheduled.result.value.opaque -eq $true) "sparse-occluder-opaque"
    Assert-Acceptance ($sparseOccluderScheduled.result.value.sparse -eq $true) "sparse-occluder-narrow-strip"
    Assert-Acceptance ($sparseOccluderScheduled.result.value.stablePointRowsAvoided -eq $true) "sparse-occluder-avoids-stable-point-rows"
    Register-PageMutation
    Assert-ControlActiveBeforeFixture $sparseTiming.AttackAt "sparse-occluder"
    Wait-UntilEpochMilliseconds ($sparseTiming.AttackAt + 50)
    $sparseRevokedAt = Wait-ForReleasedControl "control_ui_hidden" `
        -DeadlineEpochMilliseconds ($sparseTiming.AttackAt + 3000) -ReturnRevocationAt
    Assert-Acceptance ($sparseRevokedAt -ge $sparseTiming.AttackAt -and $sparseRevokedAt -le $sparseTiming.AttackAt + 3000) "sparse-occluder-bounded-revocation"
    $script:ControlMayBeActive = $false
    Wait-UntilEpochMilliseconds ($sparseTiming.CleanupAt + 250)
    $observation = Restart-DemoControlAfterFixture $targetTabId "sparse-occluder"

    # A page can read and rename the public host ID, but it cannot read the
    # random marker inside the closed shadow root. This fixture steals the old
    # public ID for a fake opaque passive popover and re-tops it every frame.
    # Browser-process verification must bind the top element to the unique
    # closed-shadow marker ancestry and revoke rather than accepting the fake.
    $fakeHostScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const real = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  if (!real || !/^__local_browser_bridge_control_[0-9a-f]{32}__$/.test(real.id)) {
    return { scheduled: false, attackAt: 0, cleanupAt: 0, passive: false };
  }
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 8000;
  const originalId = real.id;
  const state = { active: false, frame: 0, fake: null };
  globalThis.__lbbFakeControlHostAttack = state;
  const reopen = () => {
    if (!state.active || !state.fake) return;
    try {
      if (state.fake.matches(':popover-open')) state.fake.hidePopover();
      state.fake.showPopover();
    } finally {
      state.frame = requestAnimationFrame(reopen);
    }
  };
  setTimeout(() => {
    real.id = `${originalId}-renamed`;
    const fake = document.createElement('div');
    fake.id = originalId;
    fake.className = 'top-layer-host-forgery';
    fake.setAttribute('popover', 'manual');
    fake.setAttribute('aria-hidden', 'false');
    fake.setAttribute('aria-label', 'Local Browser Bridge browser control');
    fake.textContent = 'Forged page control host';
    document.documentElement.append(fake);
    state.fake = fake;
    state.active = true;
    reopen();
    setTimeout(() => {
      state.active = false;
      cancelAnimationFrame(state.frame);
      if (fake.matches(':popover-open')) fake.hidePopover();
      fake.remove();
      if (real.isConnected) real.id = originalId;
      delete globalThis.__lbbFakeControlHostAttack;
    }, 8000);
  }, 7000);
  return { scheduled: true, attackAt, cleanupAt, passive: true };
})()
'@
    })
    Assert-Acceptance ([string]$fakeHostScheduled.result.type -ceq "object") "fake-host-schedule-type"
    $fakeHostTiming = Assert-FixtureTiming $fakeHostScheduled.result.value "fake-host"
    Assert-Acceptance ($fakeHostScheduled.result.value.passive -eq $true) "fake-host-passive"
    Register-PageMutation
    Assert-ControlActiveBeforeFixture $fakeHostTiming.AttackAt "fake-host"
    Wait-UntilEpochMilliseconds ($fakeHostTiming.AttackAt + 50)
    $fakeHostRevokedAt = Wait-ForReleasedControl "control_ui_hidden" `
        -DeadlineEpochMilliseconds ($fakeHostTiming.AttackAt + 3000) -ReturnRevocationAt
    Assert-Acceptance ($fakeHostRevokedAt -ge $fakeHostTiming.AttackAt -and $fakeHostRevokedAt -le $fakeHostTiming.AttackAt + 3000) "fake-host-bounded-revocation"
    $script:ControlMayBeActive = $false
    Wait-UntilEpochMilliseconds ($fakeHostTiming.CleanupAt + 250)
    $observation = Restart-DemoControlAfterFixture $targetTabId "fake-host"

    # Keep the genuine host ID unchanged while a second page object copies it.
    # Public identity and mutation-exclusion shortcuts are insufficient: the
    # opaque passive duplicate re-tops every frame, but only the genuine exact
    # object owns the secret closed-shadow marker and is eligible for bridge-
    # owned mutation exclusion. The duplicate must revoke, then remove itself.
    $duplicateIdScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const real = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  if (!real || !/^__local_browser_bridge_control_[0-9a-f]{32}__$/.test(real.id)
      || globalThis.__lbbDuplicateIdTopLayerAttack) {
    return { scheduled: false, attackAt: 0, cleanupAt: 0, duplicateId: false, passive: false };
  }
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 8000;
  const fake = document.createElement('div');
  fake.id = real.id;
  fake.className = 'top-layer-host-forgery';
  fake.setAttribute('popover', 'manual');
  fake.setAttribute('aria-hidden', 'false');
  fake.setAttribute('aria-label', 'Local Browser Bridge browser control');
  fake.textContent = 'Duplicate-ID page object';
  document.documentElement.append(fake);
  const state = { active: false, frame: 0, fakeRemoved: false };
  globalThis.__lbbDuplicateIdTopLayerAttack = state;
  const reopen = () => {
    if (!state.active || !fake.isConnected) return;
    try {
      if (fake.matches(':popover-open')) fake.hidePopover();
      fake.showPopover();
    } finally {
      state.frame = requestAnimationFrame(reopen);
    }
  };
  setTimeout(() => {
    state.active = true;
    reopen();
    setTimeout(() => {
      state.active = false;
      cancelAnimationFrame(state.frame);
      if (fake.matches(':popover-open')) fake.hidePopover();
      fake.remove();
      state.fakeRemoved = !fake.isConnected;
      state.cleanupAtActual = Date.now();
    }, 8000);
  }, 7000);
  return {
    scheduled: true,
    attackAt,
    cleanupAt,
    duplicateId: fake.id === real.id,
    passive: getComputedStyle(fake).pointerEvents === 'none'
  };
})()
'@
    })
    Assert-Acceptance ([string]$duplicateIdScheduled.result.type -ceq "object") "duplicate-id-schedule-type"
    Assert-Acceptance ($duplicateIdScheduled.result.value.duplicateId -eq $true) "duplicate-id-simultaneous"
    Assert-Acceptance ($duplicateIdScheduled.result.value.passive -eq $true) "duplicate-id-passive"
    $duplicateIdResult = Complete-ControlUiRevocationFixture `
        -TabId $targetTabId -Value $duplicateIdScheduled.result.value -Label "duplicate-id"
    $observation = $duplicateIdResult.Observation
    $duplicateIdCleanup = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const state = globalThis.__lbbDuplicateIdTopLayerAttack;
  delete globalThis.__lbbDuplicateIdTopLayerAttack;
  return state ? { fakeRemoved: state.fakeRemoved === true, cleanupAtActual: state.cleanupAtActual } : null;
})()
'@
    })
    Assert-Acceptance ([string]$duplicateIdCleanup.result.type -ceq "object") "duplicate-id-cleanup-type"
    Assert-Acceptance ($duplicateIdCleanup.result.value.fakeRemoved -eq $true) "duplicate-id-cleanup"
    $duplicateIdCleanupAt = [long]$duplicateIdCleanup.result.value.cleanupAtActual
    Assert-Acceptance ($duplicateIdCleanupAt -ge $duplicateIdResult.AttackAt + 7500 -and
        $duplicateIdCleanupAt -le $duplicateIdResult.AttackAt + 9500) "duplicate-id-cleanup-timing"

    # Chrome view-transition pseudo-elements paint above the ordinary top
    # layer. The local fixture holds that layer for eight seconds. The content
    # watchdog must refuse the active transition and release within the same
    # bounded margin; renderer/top-layer checks alone are insufficient.
    $viewTransitionScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  if (typeof document.startViewTransition !== 'function') {
    return { scheduled: false, attackAt: 0, cleanupAt: 0, supported: false };
  }
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 10000;
  const marker = document.getElementById('stop-guard-fixture');
  const original = marker.textContent;
  setTimeout(() => {
    marker.textContent = 'Long view transition active';
    const transition = document.startViewTransition(() => {
      document.documentElement.classList.toggle('view-transition-acceptance-state');
    });
    transition.finished.finally(() => {
      marker.textContent = original;
      document.documentElement.classList.remove('view-transition-acceptance-state');
    });
  }, 7000);
  return { scheduled: true, attackAt, cleanupAt, supported: true };
})()
'@
    })
    Assert-Acceptance ([string]$viewTransitionScheduled.result.type -ceq "object") "view-transition-schedule-type"
    $viewTransitionTiming = Assert-FixtureTiming $viewTransitionScheduled.result.value "view-transition"
    Assert-Acceptance ($viewTransitionScheduled.result.value.supported -eq $true) "view-transition-supported"
    Register-PageMutation
    Assert-ControlActiveBeforeFixture $viewTransitionTiming.AttackAt "view-transition"
    Wait-UntilEpochMilliseconds ($viewTransitionTiming.AttackAt + 50)
    $viewTransitionRevokedAt = Wait-ForReleasedControl "control_ui_hidden" `
        -DeadlineEpochMilliseconds ($viewTransitionTiming.AttackAt + 3000) -ReturnRevocationAt
    Assert-Acceptance ($viewTransitionRevokedAt -ge $viewTransitionTiming.AttackAt -and $viewTransitionRevokedAt -le $viewTransitionTiming.AttackAt + 3000) "view-transition-bounded-revocation"
    $script:ControlMayBeActive = $false
    Wait-UntilEpochMilliseconds ($viewTransitionTiming.CleanupAt + 250)
    $observation = Restart-DemoControlAfterFixture $targetTabId "view-transition"

    # Accessibility is part of the visible handback contract. Repeatedly set
    # aria-hidden and inert after each bridge repair opportunity; the watchdog
    # must revoke, the fixture restores the exact prior values, and recovery is
    # again an explicit start plus fresh observation.
    $accessibilityScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const host = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  if (!host) return { scheduled: false, attackAt: 0, cleanupAt: 0, supported: false };
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 8000;
  const originalAriaHidden = host.getAttribute('aria-hidden');
  const originalHidden = host.getAttribute('hidden');
  const originalInert = host.inert;
  const state = { active: false, frame: 0, restored: false };
  globalThis.__lbbHostAccessibilityAttack = state;
  const suppress = () => {
    if (!state.active) return;
    host.setAttribute('aria-hidden', 'true');
    host.setAttribute('hidden', '');
    host.inert = true;
    state.frame = requestAnimationFrame(suppress);
  };
  setTimeout(() => {
    state.active = true;
    suppress();
    setTimeout(() => {
      state.active = false;
      cancelAnimationFrame(state.frame);
      if (originalAriaHidden === null) host.removeAttribute('aria-hidden');
      else host.setAttribute('aria-hidden', originalAriaHidden);
      if (originalHidden === null) host.removeAttribute('hidden');
      else host.setAttribute('hidden', originalHidden);
      host.inert = originalInert;
      state.restored = !host.isConnected
        || (host.getAttribute('aria-hidden') === originalAriaHidden
          && host.getAttribute('hidden') === originalHidden
          && host.inert === originalInert);
      state.cleanupAtActual = Date.now();
    }, 8000);
  }, 7000);
  return { scheduled: true, attackAt, cleanupAt, supported: 'inert' in host };
})()
'@
    })
    Assert-Acceptance ([string]$accessibilityScheduled.result.type -ceq "object") "accessibility-schedule-type"
    $accessibilityTiming = Assert-FixtureTiming $accessibilityScheduled.result.value "accessibility"
    Assert-Acceptance ($accessibilityScheduled.result.value.supported -eq $true) "accessibility-fixture-supported"
    Register-PageMutation
    Assert-ControlActiveBeforeFixture $accessibilityTiming.AttackAt "accessibility"
    Wait-UntilEpochMilliseconds ($accessibilityTiming.AttackAt + 50)
    $accessibilityRevokedAt = Wait-ForReleasedControl "control_ui_hidden" `
        -DeadlineEpochMilliseconds ($accessibilityTiming.AttackAt + 3000) -ReturnRevocationAt
    Assert-Acceptance ($accessibilityRevokedAt -ge $accessibilityTiming.AttackAt -and $accessibilityRevokedAt -le $accessibilityTiming.AttackAt + 3000) "accessibility-bounded-revocation"
    $script:ControlMayBeActive = $false
    Wait-UntilEpochMilliseconds ($accessibilityTiming.CleanupAt + 250)
    $observation = Restart-DemoControlAfterFixture $targetTabId "accessibility"
    $accessibilityCleanup = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const state = globalThis.__lbbHostAccessibilityAttack;
  delete globalThis.__lbbHostAccessibilityAttack;
  return state ? { restored: state.restored === true, cleanupAtActual: state.cleanupAtActual } : null;
})()
'@
    })
    Assert-Acceptance ([string]$accessibilityCleanup.result.type -ceq "object") "accessibility-cleanup-type"
    Assert-Acceptance ($accessibilityCleanup.result.value.restored -eq $true) "accessibility-cleanup-restored"
    $accessibilityCleanupAt = [long]$accessibilityCleanup.result.value.cleanupAtActual
    Assert-Acceptance ($accessibilityCleanupAt -ge $accessibilityTiming.AttackAt + 7500 -and
        $accessibilityCleanupAt -le $accessibilityTiming.AttackAt + 9500) "accessibility-cleanup-timing"

    # The controlled root is independently security-critical. Repeatedly hide,
    # inert, and aria-hide document.documentElement after every repair chance.
    # The browser proof must revoke even though the genuine host object and its
    # own attributes remain untouched, then the fixture must restore the exact
    # root attribute strings it observed before an explicit restart.
    $rootAccessibilityScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const root = document.documentElement;
  const host = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  if (!root || !host || globalThis.__lbbRootAccessibilityAttack) {
    return { scheduled: false, attackAt: 0, cleanupAt: 0, supported: false };
  }
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 8000;
  const originalAriaHidden = root.getAttribute('aria-hidden');
  const originalHidden = root.getAttribute('hidden');
  const originalInert = root.getAttribute('inert');
  const state = {
    active: false,
    frame: 0,
    attackApplied: false,
    restored: false,
    cleanupAtActual: 0
  };
  globalThis.__lbbRootAccessibilityAttack = state;
  const suppress = () => {
    if (!state.active) return;
    root.setAttribute('aria-hidden', 'true');
    root.setAttribute('hidden', '');
    root.setAttribute('inert', '');
    state.attackApplied = root.getAttribute('aria-hidden') === 'true'
      && root.hasAttribute('hidden')
      && root.hasAttribute('inert');
    state.frame = requestAnimationFrame(suppress);
  };
  setTimeout(() => {
    state.active = true;
    suppress();
    setTimeout(() => {
      state.active = false;
      cancelAnimationFrame(state.frame);
      const restore = (name, value) => {
        if (value === null) root.removeAttribute(name);
        else root.setAttribute(name, value);
      };
      restore('aria-hidden', originalAriaHidden);
      restore('hidden', originalHidden);
      restore('inert', originalInert);
      state.restored = root.getAttribute('aria-hidden') === originalAriaHidden
        && root.getAttribute('hidden') === originalHidden
        && root.getAttribute('inert') === originalInert;
      state.cleanupAtActual = Date.now();
    }, 8000);
  }, 7000);
  return {
    scheduled: true,
    attackAt,
    cleanupAt,
    supported: 'inert' in root,
    hostInitiallyDirect: host.parentNode === root && host.parentElement === root
  };
})()
'@
    })
    Assert-Acceptance ([string]$rootAccessibilityScheduled.result.type -ceq "object") "root-accessibility-schedule-type"
    Assert-Acceptance ($rootAccessibilityScheduled.result.value.supported -eq $true) "root-accessibility-fixture-supported"
    Assert-Acceptance ($rootAccessibilityScheduled.result.value.hostInitiallyDirect -eq $true) "root-accessibility-host-initially-direct"
    $rootAccessibilityResult = Complete-ControlUiRevocationFixture `
        -TabId $targetTabId -Value $rootAccessibilityScheduled.result.value -Label "root-accessibility"
    $observation = $rootAccessibilityResult.Observation
    $rootAccessibilityCleanup = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const state = globalThis.__lbbRootAccessibilityAttack;
  delete globalThis.__lbbRootAccessibilityAttack;
  const root = document.documentElement;
  const host = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  return state ? {
    attackApplied: state.attackApplied === true,
    restored: state.restored === true,
    cleanupAtActual: state.cleanupAtActual,
    freshHostDirect: Boolean(host && host.parentNode === root && host.parentElement === root),
    freshRootAccessible: Boolean(root && !root.hidden && !root.inert && root.getAttribute('aria-hidden') !== 'true')
  } : null;
})()
'@
    })
    Assert-Acceptance ([string]$rootAccessibilityCleanup.result.type -ceq "object") "root-accessibility-cleanup-type"
    foreach ($name in @("attackApplied", "restored", "freshHostDirect", "freshRootAccessible")) {
        Assert-Acceptance ($rootAccessibilityCleanup.result.value.$name -eq $true) "root-accessibility-$name"
    }
    $rootAccessibilityCleanupAt = [long]$rootAccessibilityCleanup.result.value.cleanupAtActual
    Assert-Acceptance ($rootAccessibilityCleanupAt -ge $rootAccessibilityResult.AttackAt + 7500 -and
        $rootAccessibilityCleanupAt -le $rootAccessibilityResult.AttackAt + 9500) "root-accessibility-cleanup-timing"

    # Reparenting the exact host through ordinary light DOM, an open shadow
    # root, or a closed shadow root must never be mistaken for a valid direct
    # child of document.documentElement. Each wrapper exists only inside the
    # owned localhost fixture, restores or retires the old host exactly, and
    # is removed before explicit control restart creates a verified root host.
    foreach ($wrapperMode in @("light", "open", "closed")) {
        $wrapperExpression = @'
(() => {
  const mode = '__WRAPPER_MODE__';
  const root = document.documentElement;
  const host = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  const stateKey = `__lbbDirectParentAttack_${mode}`;
  if (!root || !host || host.parentNode !== root || globalThis[stateKey]) {
    return { scheduled: false, attackAt: 0, cleanupAt: 0, mode, hostInitiallyDirect: false };
  }
  const attackAt = Date.now() + 7000;
  const cleanupAt = attackAt + 8000;
  const originalNext = host.nextSibling;
  const state = {
    mode,
    host,
    wrapper: null,
    attackApplied: false,
    publicIdentityCopied: false,
    wrapperRemoved: false,
    hostRestoredOrRetired: false,
    cleanupAtActual: 0
  };
  globalThis[stateKey] = state;
  setTimeout(() => {
    const wrapper = document.createElement('div');
    wrapper.id = host.id;
    wrapper.setAttribute('popover', 'manual');
    wrapper.setAttribute('aria-hidden', 'false');
    wrapper.setAttribute('aria-label', 'Local Browser Bridge browser control');
    wrapper.style.display = 'contents';
    wrapper.setAttribute('data-local-fixture', `direct-parent-${mode}`);
    root.append(wrapper);
    let insertionParent = wrapper;
    if (mode === 'open' || mode === 'closed') {
      insertionParent = wrapper.attachShadow({ mode });
    }
    insertionParent.append(host);
    state.wrapper = wrapper;
    state.publicIdentityCopied = wrapper.id === host.id
      && wrapper.getAttribute('aria-label') === host.getAttribute('aria-label');
    state.attackApplied = host.parentNode === insertionParent
      && host.parentNode !== root
      && wrapper.isConnected;
    setTimeout(() => {
      const oldHostRetired = !host.isConnected;
      if (!oldHostRetired) {
        const before = originalNext && originalNext.parentNode === root ? originalNext : null;
        root.insertBefore(host, before);
      }
      wrapper.remove();
      state.wrapperRemoved = !wrapper.isConnected;
      state.hostRestoredOrRetired = oldHostRetired
        || (host.parentNode === root && host.parentElement === root);
      state.cleanupAtActual = Date.now();
    }, 8000);
  }, 7000);
  return { scheduled: true, attackAt, cleanupAt, mode, hostInitiallyDirect: true };
})()
'@
        $wrapperScheduled = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
            tabId = $targetTabId
            expression = $wrapperExpression.Replace("__WRAPPER_MODE__", $wrapperMode)
        })
        Assert-Acceptance ([string]$wrapperScheduled.result.type -ceq "object") "direct-parent-$wrapperMode-schedule-type"
        Assert-Acceptance ([string]$wrapperScheduled.result.value.mode -ceq $wrapperMode) "direct-parent-$wrapperMode-mode"
        Assert-Acceptance ($wrapperScheduled.result.value.hostInitiallyDirect -eq $true) "direct-parent-$wrapperMode-host-initially-direct"
        $wrapperResult = Complete-ControlUiRevocationFixture `
            -TabId $targetTabId -Value $wrapperScheduled.result.value -Label "direct-parent-$wrapperMode"
        $observation = $wrapperResult.Observation
        $wrapperCleanupExpression = @'
(() => {
  const mode = '__WRAPPER_MODE__';
  const stateKey = `__lbbDirectParentAttack_${mode}`;
  const state = globalThis[stateKey];
  delete globalThis[stateKey];
  const root = document.documentElement;
  const current = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  return state ? {
    mode: state.mode,
    attackApplied: state.attackApplied === true,
    publicIdentityCopied: state.publicIdentityCopied === true,
    wrapperRemoved: state.wrapperRemoved === true,
    hostRestoredOrRetired: state.hostRestoredOrRetired === true,
    cleanupAtActual: state.cleanupAtActual,
    freshHostDirect: Boolean(current && current.parentNode === root && current.parentElement === root),
    noWrapperSubstitution: Boolean(current && current !== state.wrapper)
  } : null;
})()
'@
        $wrapperCleanup = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
            tabId = $targetTabId
            expression = $wrapperCleanupExpression.Replace("__WRAPPER_MODE__", $wrapperMode)
        })
        Assert-Acceptance ([string]$wrapperCleanup.result.type -ceq "object") "direct-parent-$wrapperMode-cleanup-type"
        Assert-Acceptance ([string]$wrapperCleanup.result.value.mode -ceq $wrapperMode) "direct-parent-$wrapperMode-cleanup-mode"
        foreach ($name in @("attackApplied", "publicIdentityCopied", "wrapperRemoved", "hostRestoredOrRetired", "freshHostDirect", "noWrapperSubstitution")) {
            Assert-Acceptance ($wrapperCleanup.result.value.$name -eq $true) "direct-parent-$wrapperMode-$name"
        }
        $wrapperCleanupAt = [long]$wrapperCleanup.result.value.cleanupAtActual
        Assert-Acceptance ($wrapperCleanupAt -ge $wrapperResult.AttackAt + 7500 -and
            $wrapperCleanupAt -le $wrapperResult.AttackAt + 9500) "direct-parent-$wrapperMode-cleanup-timing"
    }

    # Exercise the former mutation-depth deadline gap through the real command
    # and automatic-observation path. The page arms an open-transition listener
    # on the genuine host. During the bridge-owned capture/show cycle it queues
    # an opaque passive top-layer cover and immediately stalls its renderer for
    # five seconds. The service worker must timestamp revocation before that
    # stall ends and within the independent three-second dirty deadline.
    $preMutationStallGeneration = [string]$observation.generation
    $preMutationStallStatus = Invoke-BridgeCommand "browser.control.status" ([ordered]@{})
    Assert-Acceptance ($preMutationStallStatus.result.active -eq $true) "mutation-stall-pre-attack-active"
    Assert-Acceptance ($preMutationStallStatus.result.humanPaused -eq $false) "mutation-stall-pre-attack-unpaused"
    Assert-Acceptance ($preMutationStallStatus.result.revocationPending -eq $false) "mutation-stall-pre-attack-clean"
    $mutationStallArmed = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const host = document.querySelector('[aria-label="Local Browser Bridge browser control"][popover]');
  if (!host || globalThis.__lbbMutationDepthStallAttack) {
    return { armed: false, passive: false, opaque: false, stallMs: 0 };
  }
  const occluder = document.createElement('div');
  occluder.setAttribute('popover', 'manual');
  occluder.setAttribute('aria-hidden', 'true');
  occluder.setAttribute('data-local-fixture', 'mutation-depth-stall');
  for (const [name, value] of Object.entries({
    position: 'fixed', inset: '0px', width: '100vw', height: '100vh',
    margin: '0px', padding: '0px', border: '0px', background: 'rgb(0, 0, 0)',
    pointerEvents: 'none'
  })) {
    occluder.style.setProperty(name.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`), value, 'important');
  }
  document.documentElement.append(occluder);
  const stallMs = 5000;
  const state = {
    host,
    occluder,
    triggered: false,
    listenerRemoved: false,
    attackApplied: false,
    fullViewport: false,
    attackAt: 0,
    stallEndedAt: 0,
    cleanupAtActual: 0,
    restored: false
  };
  globalThis.__lbbMutationDepthStallAttack = state;
  const onBeforeToggle = (event) => {
    if (state.triggered || event.newState !== 'open') return;
    state.triggered = true;
    host.removeEventListener('beforetoggle', onBeforeToggle);
    state.listenerRemoved = true;
    queueMicrotask(() => {
      state.attackAt = Date.now();
      try {
        occluder.showPopover();
        const rect = occluder.getBoundingClientRect();
        state.fullViewport = rect.left <= 0 && rect.top <= 0
          && rect.right >= innerWidth && rect.bottom >= innerHeight;
        state.attackApplied = occluder.matches(':popover-open')
          && getComputedStyle(occluder).pointerEvents === 'none';
        const stallStarted = performance.now();
        while (performance.now() - stallStarted < stallMs) {
          // Deliberately block only this owned localhost renderer.
        }
      } finally {
        state.stallEndedAt = Date.now();
        if (occluder.matches(':popover-open')) occluder.hidePopover();
        occluder.remove();
        state.restored = !occluder.isConnected;
        state.cleanupAtActual = Date.now();
      }
    });
  };
  host.addEventListener('beforetoggle', onBeforeToggle);
  const style = getComputedStyle(occluder);
  return {
    armed: true,
    passive: style.pointerEvents === 'none',
    opaque: ['rgb(0, 0, 0)', 'rgba(0, 0, 0, 1)'].includes(style.backgroundColor),
    stallMs
  };
})()
'@
    })
    Assert-Acceptance ([string]$mutationStallArmed.result.type -ceq "object") "mutation-stall-arm-type"
    Assert-Acceptance ($mutationStallArmed.result.value.armed -eq $true) "mutation-stall-armed"
    Assert-Acceptance ($mutationStallArmed.result.value.passive -eq $true) "mutation-stall-passive"
    Assert-Acceptance ($mutationStallArmed.result.value.opaque -eq $true) "mutation-stall-opaque"
    Assert-Acceptance ([int]$mutationStallArmed.result.value.stallMs -eq 5000) "mutation-stall-duration-configured"
    Register-PageMutation
    $mutationStallRevokedAt = Wait-ForReleasedControl "control_ui_hidden" -ReturnRevocationAt
    $script:ControlMayBeActive = $false
    $observation = Restart-DemoControlAfterFixture $targetTabId "mutation-stall"
    Assert-Acceptance ([string]$observation.generation -cne $preMutationStallGeneration) "mutation-stall-fresh-generation"
    $mutationStallCleanup = Invoke-BridgeCommand "page.evaluate" ([ordered]@{
        tabId = $targetTabId
        expression = @'
(() => {
  const state = globalThis.__lbbMutationDepthStallAttack;
  delete globalThis.__lbbMutationDepthStallAttack;
  return state ? {
    triggered: state.triggered === true,
    listenerRemoved: state.listenerRemoved === true,
    attackApplied: state.attackApplied === true,
    fullViewport: state.fullViewport === true,
    attackAt: state.attackAt,
    stallEndedAt: state.stallEndedAt,
    cleanupAtActual: state.cleanupAtActual,
    restored: state.restored === true
  } : null;
})()
'@
    })
    Assert-Acceptance ([string]$mutationStallCleanup.result.type -ceq "object") "mutation-stall-cleanup-type"
    foreach ($name in @("triggered", "listenerRemoved", "attackApplied", "fullViewport", "restored")) {
        Assert-Acceptance ($mutationStallCleanup.result.value.$name -eq $true) "mutation-stall-$name"
    }
    $mutationStallAttackAt = [long]$mutationStallCleanup.result.value.attackAt
    $mutationStallEndedAt = [long]$mutationStallCleanup.result.value.stallEndedAt
    $mutationStallCleanupAt = [long]$mutationStallCleanup.result.value.cleanupAtActual
    Assert-Acceptance ($mutationStallAttackAt -gt 0 -and
        $mutationStallAttackAt -lt $mutationStallRevokedAt) "mutation-stall-authoritative-order"
    Assert-Acceptance ($mutationStallRevokedAt -le $mutationStallAttackAt + 4000 -and
        $mutationStallRevokedAt -lt $mutationStallEndedAt) "mutation-stall-independent-deadline"
    Assert-Acceptance ($mutationStallEndedAt -ge $mutationStallAttackAt + 4800 -and
        $mutationStallEndedAt -le $mutationStallAttackAt + 6500) "mutation-stall-renderer-duration"
    Assert-Acceptance ($mutationStallCleanupAt -ge $mutationStallEndedAt -and
        $mutationStallCleanupAt -le $mutationStallEndedAt + 500) "mutation-stall-cleanup-timing"

    # Four real screenshot observations plus deliberate gaps span more than
    # two watchdog intervals. Intentional capture hiding must not be mistaken
    # for page suppression, and restoration must pass the browser-process
    # marker, point, and document-scoped top-layer proofs every time.
    $captureRace = [Diagnostics.Stopwatch]::StartNew()
    for ($captureAttempt = 0; $captureAttempt -lt 4; $captureAttempt += 1) {
        $observation = Get-FreshObservation $targetTabId
        Start-Sleep -Milliseconds 300
    }
    $captureRace.Stop()
    Assert-Acceptance ($captureRace.ElapsedMilliseconds -ge 1000) "capture-watchdog-overlap-duration"
    $afterCaptureRace = Invoke-BridgeCommand "browser.control.status" ([ordered]@{})
    Assert-Acceptance ($afterCaptureRace.result.active -eq $true) "capture-watchdog-control-active"
    Assert-Acceptance ($afterCaptureRace.result.humanPaused -eq $false) "capture-watchdog-control-unpaused"
    Assert-Acceptance ($afterCaptureRace.result.revocationPending -eq $false) "capture-watchdog-control-clean"
    $AggregateAssertions["topLayerControlUiIntegrity"] = $true

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
    $visibleTarget = Get-VisibleObservedElement $targetTabId $observation "button" "Bottom action"
    $observation = $visibleTarget.Observation
    Assert-Acceptance ($visibleTarget.Element.inViewport -eq $true) "bottom-marker-visible"
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
    Wait-ForReleasedControl "released_by_client"
    $script:ControlMayBeActive = $false
    Confirm-MethodPassed "browser.control.stop"

    $afterStop = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
    Assert-Acceptance (@($afterStop.result.tabs | Where-Object {
        [long]$_.id -eq $targetTabId -and [string]$_.url -ceq $demoUrl
    }).Count -eq 1) "demo-tab-retained-for-visual-evidence"

    # The visual handoff must never rediscover a public-title/URL lookalike.
    # Re-activate the exact bridge-returned ID so Chrome focuses its containing
    # window, prove that exact ID is active, and return it only through the
    # caller's in-memory pipeline (never the retained matrix record).
    $handoffActivation = Invoke-BridgeCommand "tabs.activate" ([ordered]@{ tabId = $targetTabId })
    Assert-Acceptance ($handoffActivation.result.active -eq $true -and
        [long]$handoffActivation.result.tabId -eq $targetTabId) "owned-handoff-activation"
    $handoffInventory = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
    Assert-Acceptance ([long]$handoffInventory.result.activeTabId -eq $targetTabId) "owned-handoff-active-id"
    Assert-Acceptance (@($handoffInventory.result.tabs | Where-Object {
        [long]$_.id -eq $targetTabId -and $_.active -eq $true
    }).Count -eq 1) "owned-handoff-single-active-target"
    $script:OwnedTargetHandoff = [pscustomobject][ordered]@{
        runNonce = [string]$script:CandidateBinding.runNonce
        tabId = $targetTabId
        groupId = $targetGroupId
        focusedByExactOwnedTabActivation = $true
    }

    Assert-Acceptance ($script:SeenCallIds.Count -eq $script:CommandCallCount) "all-command-identities-unique"
    $AggregateAssertions["freshCommandIdentity"] = $true
    # This is the final no-throw handoff point. Until every matrix assertion
    # above succeeds, the demo remains owned and the failure cleanup closes it.
    [void]$script:ClosableTabs.Remove($targetTabId)
    [void]$script:OwnedTabs.Remove($targetTabId)
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
                Wait-ForReleasedControl "released_by_client"
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
                $cleanupInventory = Invoke-BridgeCommand "tabs.list" ([ordered]@{})
                $stillPresent = @($cleanupInventory.result.tabs | Where-Object {
                    [long]$_.id -eq [long]$ownedTabId
                }).Count -ne 0
                if ($stillPresent) {
                    $cleanupSucceeded = $false
                }
                else {
                    [void]$script:ClosableTabs.Remove([long]$ownedTabId)
                    [void]$script:OwnedTabs.Remove([long]$ownedTabId)
                }
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

Assert-Acceptance ($null -ne $script:OwnedTargetHandoff) "owned-target-handoff-present"
Write-Output ($script:OwnedTargetHandoff | ConvertTo-Json -Depth 3 -Compress)
