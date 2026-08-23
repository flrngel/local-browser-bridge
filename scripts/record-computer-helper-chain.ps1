#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Run", "SelfTest")]
    [string]$Mode,
    [string]$PreflightRecord,
    [string]$ApiMatrixRunner,
    [string]$ApiMatrixRecord,
    [string]$ServerExecutable,
    [string]$HelperExecutable,
    [string]$ExtensionDirectory,
    [string]$RawScreenshotDirectory,
    [string]$OutputRecord,
    [ValidateRange(1, 65535)]
    [int]$Port = 17373
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$script:Utf8NoBom = [Text.UTF8Encoding]::new($false, $true)
$script:Version = "0.12.10"
$script:Source = "local-browser-bridge-computer-helper-via-loopback-api"
$script:Screenshots = [ordered]@{
    "extension-loaded" = "browser-01-extension-loaded.raw.png"
    "api-action-result" = "browser-02-api-action-result.raw.png"
    "computer-share-action" = "browser-03-computer-share-action.raw.png"
}
$script:ExtensionFiles = @(
    "background.js", "content.js", "dom-core.js", "frame-agent.js", "lib.js",
    "manifest.json", "popup.css", "popup.html", "popup.js", "stop-guard.js", "LICENSE"
)
$script:EpochNames = @(
    "existing-chrome-bootstrap", "dedicated-chrome-extensions", "native-load-picker",
    "dedicated-chrome-installed", "extension-popup-setup", "dedicated-chrome-demo",
    "extension-popup-cleanup", "cleanup-chrome-extensions", "dedicated-chrome-close"
)
$script:EpochSurfaces = @(
    "chrome-window", "chrome-window", "native-file-picker", "chrome-window",
    "extension-popup", "chrome-window", "extension-popup", "chrome-window", "chrome-window"
)
$script:ActionNames = @(
    "dedicated-window-created", "chrome-extensions-navigated", "developer-mode-ready",
    "load-unpacked-clicked", "native-picker-completed", "candidate-card-verified",
    "extension-popup-opened", "full-access-ready", "popup-token-saved",
    "extension-proof-revealed", "browser-api-result-revealed", "computer-demo-clicked", "cleanup-popup-opened",
    "token-clear-initiated", "token-clear-confirmed", "full-access-restored",
    "test-card-removed", "developer-mode-restored", "test-window-closed"
)

function Resolve-OrdinaryFile {
    param([string]$Path, [string]$Label)
    if ([String]::IsNullOrWhiteSpace($Path)) { throw "$Label is required." }
    $full = [IO.Path]::GetFullPath($Path)
    if (-not [IO.File]::Exists($full)) { throw "$Label does not exist." }
    if ([IO.FileInfo]::new($full).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "$Label must not be a reparse point."
    }
    return $full
}

function Resolve-OrdinaryDirectory {
    param([string]$Path, [string]$Label)
    if ([String]::IsNullOrWhiteSpace($Path)) { throw "$Label is required." }
    $full = [IO.Path]::GetFullPath($Path)
    if (-not [IO.Directory]::Exists($full)) { throw "$Label does not exist." }
    if ([IO.DirectoryInfo]::new($full).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "$Label must not be a reparse point."
    }
    return $full
}

function Resolve-NewJson {
    param([string]$Path)
    if ([String]::IsNullOrWhiteSpace($Path)) { throw "OutputRecord is required." }
    $full = [IO.Path]::GetFullPath($Path)
    if ([IO.Path]::GetExtension($full) -cne ".json" -or
        [IO.File]::Exists($full) -or [IO.Directory]::Exists($full)) {
        throw "OutputRecord must be a new JSON file."
    }
    $parent = [IO.Path]::GetDirectoryName($full)
    if (-not [IO.Directory]::Exists($parent) -or
        ([IO.DirectoryInfo]::new($parent).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "OutputRecord parent must be an existing ordinary directory."
    }
    return $full
}

function Read-Json {
    param([string]$Path, [string]$Label)
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -le 0 -or $bytes.Length -gt 4MB) { throw "$Label has an invalid size." }
    try { return $script:Utf8NoBom.GetString($bytes) | ConvertFrom-Json }
    catch { throw "$Label is not strict UTF-8 JSON." }
    finally { [Array]::Clear($bytes, 0, $bytes.Length) }
}

function Get-Sha256 {
    param([string]$Path)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { return ([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace("-", "").ToLowerInvariant() }
        finally { $hasher.Dispose() }
    }
    finally { $stream.Dispose() }
}

function Get-ExactExtensionPayloadDigest {
    param([string]$DirectoryPath, [object[]]$ExpectedInventory, [string]$ExpectedCombinedSha256)
    $entries = @([IO.DirectoryInfo]::new($DirectoryPath).GetFileSystemInfos())
    $actualNames = @($entries | ForEach-Object { $_.Name } | Sort-Object)
    $expectedNames = @($script:ExtensionFiles | Sort-Object)
    if ($entries.Count -ne $script:ExtensionFiles.Count -or
        ($actualNames -join "`n") -cne ($expectedNames -join "`n")) {
        throw "ExtensionDirectory does not contain the exact eleven-file candidate inventory."
    }
    if ($ExpectedInventory.Count -ne $script:ExtensionFiles.Count) {
        throw "PreflightRecord does not contain the exact extracted extension inventory."
    }
    $canonical = ""
    for ($index = 0; $index -lt $script:ExtensionFiles.Count; $index += 1) {
        $name = $script:ExtensionFiles[$index]
        $entry = $entries | Where-Object { $_.Name -ceq $name } | Select-Object -First 1
        if ($entry -isnot [IO.FileInfo] -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            $entry.Length -le 0 -or $entry.Length -gt 5MB) {
            throw "ExtensionDirectory contains a linked, non-file, empty, or oversized candidate entry."
        }
        $expected = $ExpectedInventory[$index]
        $sha256 = Get-Sha256 $entry.FullName
        if ($expected.name -cne $name -or [int64]$expected.bytes -ne $entry.Length -or
            $expected.sha256 -cne $sha256) {
            throw "ExtensionDirectory differs from the exact preflight payload."
        }
        $canonical += "$sha256  $name`n"
    }
    $combined = Get-TextSha256 $canonical
    if ($combined -cne $ExpectedCombinedSha256) {
        throw "ExtensionDirectory combined payload digest differs from preflight."
    }
    return $combined
}

function Assert-ExactKeys {
    param([object]$Object, [string[]]$Expected, [string]$Label)
    $actual = @($Object.PSObject.Properties.Name)
    if (($actual -join "`n") -cne ($Expected -join "`n")) {
        throw "$Label contains missing, unexpected, or reordered fields."
    }
}

function Assert-Hex {
    param([object]$Value, [int]$Length, [string]$Label)
    if ($Value -isnot [string] -or -not [regex]::IsMatch($Value, "^[0-9a-f]{$Length}$")) {
        throw "$Label must be lowercase hexadecimal."
    }
}

function Get-PngDimensions {
    param([string]$Path)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $header = New-Object byte[] 24
        if ($stream.Read($header, 0, $header.Length) -ne $header.Length -or
            ([BitConverter]::ToString($header, 0, 8)) -cne "89-50-4E-47-0D-0A-1A-0A" -or
            [Text.Encoding]::ASCII.GetString($header, 12, 4) -cne "IHDR") {
            throw "Raw helper screenshot is not a canonical PNG."
        }
        $width = ([uint32]$header[16] -shl 24) -bor ([uint32]$header[17] -shl 16) -bor
            ([uint32]$header[18] -shl 8) -bor [uint32]$header[19]
        $height = ([uint32]$header[20] -shl 24) -bor ([uint32]$header[21] -shl 16) -bor
            ([uint32]$header[22] -shl 8) -bor [uint32]$header[23]
        if ($width -lt 120 -or $height -lt 32 -or $width -gt 8192 -or $height -gt 8192 -or
            ([uint64]$width * [uint64]$height) -gt 50MB) {
            throw "Raw helper screenshot dimensions are invalid."
        }
        return [pscustomobject]@{ Width = [int64]$width; Height = [int64]$height }
    }
    finally { $stream.Dispose() }
}

function Remove-CanonicalRawScreenshots {
    param([string]$DirectoryPath)
    if (-not [IO.Directory]::Exists($DirectoryPath)) { return }
    $directory = [IO.DirectoryInfo]::new([IO.Path]::GetFullPath($DirectoryPath))
    if ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Raw screenshot cleanup refused a reparse-point directory."
    }
    $errors = New-Object Collections.Generic.List[string]
    foreach ($name in $script:Screenshots.Values) {
        $path = [IO.Path]::Combine($directory.FullName, $name)
        try {
            if ([IO.Directory]::Exists($path)) {
                throw "the canonical raw screenshot path is a directory"
            }
            if ([IO.File]::Exists($path)) {
                $item = [IO.FileInfo]::new($path)
                if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                    throw "the canonical raw screenshot is a reparse point"
                }
                [IO.File]::Delete($path)
            }
        }
        catch { $errors.Add("${name}: $($_.Exception.Message)") }
    }
    if ($errors.Count -ne 0) {
        throw "Canonical raw screenshot cleanup was incomplete: $($errors -join '; ')"
    }
}

function Get-CandidateBinding {
    param([object]$Preflight, [string]$PreflightSha256)
    if ($Preflight.phase -cne "preflight" -or $Preflight.passed -ne $true -or
        $Preflight.candidate.version -cne $script:Version) {
        throw "PreflightRecord is not a passing v0.12.10 preflight."
    }
    foreach ($value in @(
        $Preflight.runNonce, $PreflightSha256, $Preflight.candidate.checksumManifest.sha256,
        $Preflight.candidate.server.sha256, $Preflight.candidate.computerHelper.sha256,
        $Preflight.candidate.extension.sha256, $Preflight.candidate.extension.combinedPayloadSha256
    )) { Assert-Hex $value 64 "candidate binding digest" }
    Assert-Hex $Preflight.candidate.finalSha 40 "candidate FINAL_SHA"
    return [ordered]@{
        runNonce = [string]$Preflight.runNonce
        preflightRecordSha256 = $PreflightSha256
        finalSha = [string]$Preflight.candidate.finalSha
        checksumManifestSha256 = [string]$Preflight.candidate.checksumManifest.sha256
        serverSha256 = [string]$Preflight.candidate.server.sha256
        computerHelperSha256 = [string]$Preflight.candidate.computerHelper.sha256
        extensionZipSha256 = [string]$Preflight.candidate.extension.sha256
        extractedPayloadSha256 = [string]$Preflight.candidate.extension.combinedPayloadSha256
    }
}

function Get-TextSha256 {
    param([string]$Value)
    $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { return ([BitConverter]::ToString($hasher.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant() }
        finally { $hasher.Dispose() }
    }
    finally { [Array]::Clear($bytes, 0, $bytes.Length) }
}

function Get-OpaqueRef {
    param([string]$Domain, [string]$RawValue)
    if ([String]::IsNullOrWhiteSpace($RawValue)) { throw "A raw value was missing for $Domain binding." }
    return Get-TextSha256 "$($script:Binding.runNonce)`n$Domain`n$RawValue"
}

function Read-ExactReceipt {
    param([string]$Instruction, [string]$Receipt)
    Write-Host $Instruction
    if ((Read-Host "Type $Receipt") -cne $Receipt) { throw "Required human receipt was not supplied." }
}

function Invoke-LoopbackJson {
    param([string]$Path, [string]$Method, [object]$Body)
    $parameters = @{
        UseBasicParsing = $true
        Uri = "http://127.0.0.1:$Port$Path"
        Method = $Method
        Headers = @{ Authorization = "Bearer $($script:Token)" }
        TimeoutSec = 25
    }
    if ($null -ne $Body) {
        $parameters.ContentType = "application/json"
        $parameters.Body = $Body | ConvertTo-Json -Depth 20 -Compress
    }
    $response = Invoke-WebRequest @parameters
    if ([int]$response.StatusCode -ne 200) { throw "Loopback request failed." }
    return [pscustomobject]@{
        Status = [int]$response.StatusCode
        Body = $response.Content | ConvertFrom-Json
        Digest = Get-TextSha256 ([string]$response.Content)
    }
}

function Get-BridgeState {
    return (Invoke-LoopbackJson "/api/state" "Get" $null).Body.state
}

function Wait-BridgeState {
    param([scriptblock]$Predicate, [string]$Label, [int]$Seconds = 20)
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if ($null -ne $script:ServerProcess -and $script:ServerProcess.HasExited) {
            throw "The candidate server exited while waiting for $Label."
        }
        $state = Get-BridgeState
        if (& $Predicate $state) { return $state }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Label."
}

function Invoke-ComputerCommand {
    param([string]$Method, [hashtable]$Params)
    $script:CommandSequence += 1
    $response = Invoke-LoopbackJson "/api/v1/command" "Post" ([ordered]@{
        method = $Method
        params = $Params
        callId = "helper-chain-" + [Guid]::NewGuid().ToString("N")
    })
    if ($null -ne $response.Body.error) {
        throw "Loopback computer command $Method returned a structured error."
    }
    return $response
}

function Get-NormalizedObservationApplication {
    param([object]$Observation)
    if ($null -eq $Observation -or [int64]$Observation.pid -le 0) {
        throw "The computer observation has no valid process identity."
    }
    $app = ([string]$Observation.appName).Trim().ToLowerInvariant()
    $leaf = [IO.Path]::GetFileNameWithoutExtension($app)
    if ($app -in @("google chrome", "google chrome.exe", "chrome", "chrome.exe") -or
        $leaf -ceq "chrome") {
        return "google-chrome"
    }
    throw "The computer observation is not stock Google Chrome UI."
}

function Test-ExactObservationIdentity {
    param([object]$Observation, [string]$WindowId, [int64]$ExpectedPid)
    try {
        return $null -ne $Observation -and
            [string]$Observation.windowId -ceq $WindowId -and
            [int64]$Observation.pid -eq $ExpectedPid -and
            (Get-NormalizedObservationApplication $Observation) -ceq "google-chrome" -and
            -not [String]::IsNullOrWhiteSpace([string]$Observation.frameId)
    }
    catch { return $false }
}

function Test-ExactSharedFrame {
    param(
        [object]$Observation,
        [string]$WindowId,
        [int64]$ExpectedPid,
        [string]$ShareId,
        [int64]$PreviousSourceSequence
    )
    try {
        if (-not (Test-ExactObservationIdentity $Observation $WindowId $ExpectedPid) -or
            $null -eq $Observation.PSObject.Properties["shareId"] -or
            $null -eq $Observation.PSObject.Properties["sourceSequence"] -or
            $null -eq $Observation.PSObject.Properties["share"] -or
            [string]$Observation.shareId -cne $ShareId -or
            [int64]$Observation.sourceSequence -le $PreviousSourceSequence -or
            $Observation.share.active -ne $true -or [string]$Observation.share.id -cne $ShareId) {
            return $false
        }
        return $true
    }
    catch { return $false }
}

function Get-FreshObservation {
    param(
        [string]$WindowId,
        [int64]$ExpectedPid,
        [string]$PreviousFrameId
    )
    $response = Invoke-ComputerCommand "computer.observe" @{ windowId = $WindowId }
    $observation = $response.Body.state.computerObservation
    if (-not (Test-ExactObservationIdentity $observation $WindowId $ExpectedPid) -or
        (-not [String]::IsNullOrWhiteSpace($PreviousFrameId) -and
            [string]$observation.frameId -ceq $PreviousFrameId)) {
        throw "computer.observe did not return a new frame for the exact stock-Chrome window and process."
    }
    return [pscustomobject]@{ Observation = $observation; Response = $response }
}

function Get-FreshSharedFrame {
    param([object]$Context, [string]$Label, [int]$Seconds = 20)
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        $state = Get-BridgeState
        $observation = $state.computerObservation
        if (Test-ExactSharedFrame $observation $Context.WindowId $Context.Pid `
                $Context.RawShareId $Context.LastSourceSequence) {
            $Context.Observation = $observation
            $Context.ObservationCount += 1
            $Context.LastSourceSequence = [int64]$observation.sourceSequence
            $Context.LastFrameRef = Get-OpaqueRef "frame" ([string]$observation.frameId)
            return $observation
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for a new exact-bound streamed helper frame for $Label."
}

function Get-NormalizedApplication {
    param([object]$Window)
    if ($null -eq $Window -or [string]$Window.id -eq "" -or [int64]$Window.pid -le 0 -or
        $Window.minimized -eq $true) {
        throw "The selected helper window has an invalid identity or is minimized."
    }
    $app = ([string]$Window.appName).Trim().ToLowerInvariant()
    $leaf = [IO.Path]::GetFileNameWithoutExtension($app)
    if ($app -in @("google chrome", "google chrome.exe", "chrome", "chrome.exe") -or
        $leaf -ceq "chrome") {
        return "google-chrome"
    }
    throw "The selected helper window is not stock Google Chrome UI."
}

function Get-ExactChromeWindow {
    param([string]$WindowId, [int64]$ExpectedPid = 0)
    $status = Invoke-ComputerCommand "computer.status" @{}
    $matches = @($status.Body.result.windows | Where-Object {
        [string]$_.id -ceq $WindowId -and ($ExpectedPid -eq 0 -or [int64]$_.pid -eq $ExpectedPid)
    })
    if ($matches.Count -ne 1 -or (Get-NormalizedApplication $matches[0]) -cne "google-chrome") {
        throw "The exact owned stock-Chrome window identity is no longer available."
    }
    return [pscustomobject]@{
        WindowId = [string]$matches[0].id
        Pid = [int64]$matches[0].pid
        Application = "google-chrome"
        StatusDigest = $status.Digest
    }
}

function Select-ExactWindow {
    param([string]$Instruction)
    $status = Invoke-ComputerCommand "computer.status" @{}
    $windows = @($status.Body.result.windows)
    if ($windows.Count -eq 0) { throw "The helper reported no selectable windows." }
    Write-Host $Instruction
    for ($index = 0; $index -lt $windows.Count; $index += 1) {
        Write-Host "[$index] $($windows[$index].title) ($($windows[$index].appName))"
    }
    $selection = 0
    if (-not [int]::TryParse((Read-Host "Exact window index"), [ref]$selection) -or
        $selection -lt 0 -or $selection -ge $windows.Count) {
        throw "Exact-window selection was invalid."
    }
    $selected = $windows[$selection]
    return [pscustomobject]@{
        WindowId = [string]$selected.id
        Pid = [int64]$selected.pid
        Application = Get-NormalizedApplication $selected
        StatusDigest = $status.Digest
    }
}

function Get-BaselineChromeWindowBinding {
    $status = Invoke-ComputerCommand "computer.status" @{}
    $refs = New-Object Collections.Generic.List[string]
    foreach ($window in @($status.Body.result.windows)) {
        try {
            if ((Get-NormalizedApplication $window) -ceq "google-chrome") {
                $refs.Add((Get-OpaqueRef "target" "$([string]$window.id)|$([int64]$window.pid)"))
            }
        }
        catch { }
    }
    $ordered = @($refs | Sort-Object -Unique)
    $binding = [pscustomobject]@{
        Count = $ordered.Count
        Digest = Get-TextSha256 ($ordered -join [Environment]::NewLine)
        TargetRefs = $ordered
        StatusDigest = $status.Digest
    }
    $refs.Clear()
    return $binding
}

function Bind-NewDedicatedChromeWindow {
    if ($null -eq $script:BaselineChrome) {
        throw "The baseline Chrome-window set was not captured."
    }
    $status = Invoke-ComputerCommand "computer.status" @{}
    $chrome = New-Object Collections.Generic.List[object]
    foreach ($window in @($status.Body.result.windows)) {
        try {
            if ((Get-NormalizedApplication $window) -ceq "google-chrome") {
                $chrome.Add([pscustomobject]@{
                    Window = $window
                    TargetRef = Get-OpaqueRef "target" "$([string]$window.id)|$([int64]$window.pid)"
                })
            }
        }
        catch { }
    }
    $newChrome = @($chrome | Where-Object { $script:BaselineChrome.TargetRefs -cnotcontains $_.TargetRef })
    $missingBaseline = @($script:BaselineChrome.TargetRefs | Where-Object {
        $chrome.TargetRef -cnotcontains $_
    })
    if ($chrome.Count -ne ($script:BaselineChrome.Count + 1) -or
        $newChrome.Count -ne 1 -or $missingBaseline.Count -ne 0) {
        throw "Control+N did not produce exactly one new stock-Chrome window over the unchanged baseline."
    }
    $selected = $newChrome[0].Window
    $script:DedicatedWindowId = [string]$selected.id
    $script:DedicatedWindowPid = [int64]$selected.pid
    $script:DedicatedTargetRef = [string]$newChrome[0].TargetRef
    $script:DedicatedProcessRef = Get-OpaqueRef "process" ([string]$script:DedicatedWindowPid)
    $script:DedicatedAbsentBeforeCreation = $true
    $script:DedicatedCreatedAsOnlyNewChromeWindow = $true
    $chrome.Clear()
    return [pscustomobject]@{
        WindowId = $script:DedicatedWindowId
        Pid = $script:DedicatedWindowPid
        Application = "google-chrome"
        StatusDigest = $status.Digest
    }
}

function Start-RecordedEpoch {
    param([string]$Name, [string]$Surface, [string]$Instruction)
    if ($script:Epochs.Count -ge $script:EpochNames.Count -or
        $Name -cne $script:EpochNames[$script:Epochs.Count] -or
        $Surface -cne $script:EpochSurfaces[$script:Epochs.Count]) {
        throw "The requested helper epoch is out of canonical order."
    }
    if ($Name -in @(
        "dedicated-chrome-extensions", "dedicated-chrome-installed", "dedicated-chrome-demo",
        "cleanup-chrome-extensions", "dedicated-chrome-close"
    )) {
        Write-Host $Instruction
        $selected = Get-ExactChromeWindow $script:DedicatedWindowId $script:DedicatedWindowPid
    }
    else {
        $selected = Select-ExactWindow $Instruction
    }
    $windowId = $selected.WindowId
    $targetRef = Get-OpaqueRef "target" "$windowId|$($selected.Pid)"
    $processRef = Get-OpaqueRef "process" ([string]$selected.Pid)
    if ($Name -ceq "dedicated-chrome-extensions") {
        if (-not $script:DedicatedCreatedAsOnlyNewChromeWindow -or
            $script:BaselineChrome.TargetRefs -contains $targetRef -or
            $targetRef -cne $script:DedicatedTargetRef -or
            $processRef -cne $script:DedicatedProcessRef) {
            throw "The dedicated Chrome epoch is not the sole helper-created post-baseline window."
        }
    }
    elseif ($Name -in @(
        "dedicated-chrome-installed", "dedicated-chrome-demo",
        "cleanup-chrome-extensions", "dedicated-chrome-close"
    )) {
        if ($targetRef -cne $script:DedicatedTargetRef -or
            $processRef -cne $script:DedicatedProcessRef) {
            throw "A later dedicated Chrome epoch selected a different native window or process."
        }
    }
    elseif ($Name -in @("native-load-picker", "extension-popup-setup", "extension-popup-cleanup")) {
        if ($processRef -cne $script:DedicatedProcessRef) {
            throw "The native picker/popup is not process-linked to the exact dedicated Chrome window."
        }
    }
    if ($Surface -ceq "extension-popup") {
        $script:LastOwnedPopupWindowId = $windowId
        $script:LastOwnedPopupWindowPid = $selected.Pid
    }
    $startSequence = $script:CommandSequence + 1
    $start = Invoke-ComputerCommand "computer.share.start" @{ windowId = $windowId; fps = 4 }
    $rawShareId = [string]$start.Body.result.id
    if ($start.Body.result.active -ne $true -or [String]::IsNullOrWhiteSpace($rawShareId)) {
        throw "computer.share.start did not return an exact active share identity."
    }
    $state = Wait-BridgeState {
        param($candidate)
        Test-ExactSharedFrame $candidate.computerObservation $windowId $selected.Pid $rawShareId 0
    } "the first exact-bound streamed frame in $Name"
    $observation = $state.computerObservation
    $context = [pscustomobject]@{
        Name = $Name; Surface = $Surface; WindowId = $windowId; Pid = $selected.Pid
        Application = $selected.Application; ProcessRef = $processRef
        EpochRef = Get-OpaqueRef "epoch-$Name" "$windowId|$rawShareId"
        TargetRef = $targetRef; ShareRef = Get-OpaqueRef "share" $rawShareId
        StartSequence = $startSequence
        RawShareId = $rawShareId; LastSourceSequence = [int64]$observation.sourceSequence
        FirstFrameRef = Get-OpaqueRef "frame" ([string]$observation.frameId)
        LastFrameRef = $null; ObservationCount = 1; Observation = $observation
    }
    $script:ActiveEpoch = $context
    $rawShareId = $null
    return $context
}

function Stop-RecordedEpoch {
    param([object]$Context, [switch]$TargetClosed)
    if (-not $TargetClosed) {
        [void](Get-FreshSharedFrame $Context "the final frame before $($Context.Name) teardown")
    }
    elseif ([String]::IsNullOrWhiteSpace([string]$Context.LastFrameRef)) {
        throw "The closed-target epoch lacks a final fresh frame."
    }
    $stopSequence = $script:CommandSequence + 1
    $stopped = Invoke-ComputerCommand "computer.share.stop" @{}
    if ($stopped.Body.result.active -ne $false) { throw "computer.share.stop did not return active:false." }
    [void](Wait-BridgeState { param($state) $state.computer.share.active -ne $true } "share teardown")
    $script:Epochs += [ordered]@{
        sequence = $script:Epochs.Count + 1; name = $Context.Name; surface = $Context.Surface
        application = $Context.Application; processRef = $Context.ProcessRef
        epochRef = $Context.EpochRef; targetRef = $Context.TargetRef; shareRef = $Context.ShareRef
        selectedExactly = $true; shareStartSequence = $Context.StartSequence
        shareStartSucceeded = $true; firstFreshFrameRef = $Context.FirstFrameRef
        lastFreshFrameRef = $Context.LastFrameRef; freshObservationCount = $Context.ObservationCount
        shareStopSequence = $stopSequence; shareStopSucceeded = $true; rawIdentifiersRetained = $false
    }
    $Context.WindowId = $null
    $Context.Observation = $null
    $Context.RawShareId = $null
    $Context.LastSourceSequence = 0
    $script:ActiveEpoch = $null
}

function Read-ClickParams {
    param([object]$Observation, [string]$Label)
    $x = 0.0; $y = 0.0
    if (-not [double]::TryParse((Read-Host "$Label x in captured-image pixels"),
            [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$x) -or
        -not [double]::TryParse((Read-Host "$Label y in captured-image pixels"),
            [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$y) -or
        $x -lt 0 -or $y -lt 0 -or $x -ge [double]$Observation.imageWidth -or $y -ge [double]$Observation.imageHeight) {
        throw "Click coordinates were outside the fresh exact-window frame."
    }
    return [ordered]@{ x = $x; y = $y; coordinateSpace = "image"; button = "left"; clickCount = 1; durationMs = 80 }
}

function Read-LiveToggleState {
    param([object]$Context, [string]$Label)
    $fresh = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
    $Context.Observation = $fresh.Observation
    $Context.ObservationCount += 1
    $frameRef = Get-OpaqueRef "frame" ([string]$fresh.Observation.frameId)
    $Context.LastFrameRef = $frameRef
    $value = (Read-Host "$Label in this fresh exact-window helper frame (enabled/disabled)").Trim().ToLowerInvariant()
    if ($value -cne "enabled" -and $value -cne "disabled") {
        throw "$Label live state must be entered exactly as enabled or disabled."
    }
    Read-ExactReceipt "Confirm that $Label=$value was read from this fresh helper frame before mutation." "VERIFIED:live-$Label-$value"
    return [ordered]@{
        value = $value
        epochRef = $Context.EpochRef
        frameRef = $frameRef
        capturedBeforeMutation = $true
    }
}

function Read-LiveSavedTokenState {
    param([object]$Context)
    $fresh = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
    $Context.Observation = $fresh.Observation
    $Context.ObservationCount += 1
    $frameRef = Get-OpaqueRef "frame" ([string]$fresh.Observation.frameId)
    $Context.LastFrameRef = $frameRef
    Read-ExactReceipt "Verify the fresh exact-popup helper frame says the saved token is Not configured." "VERIFIED:live-saved-token-unconfigured"
    return [ordered]@{
        configured = $false
        epochRef = $Context.EpochRef
        frameRef = $frameRef
        capturedBeforeMutation = $true
    }
}

function Invoke-RecordedAction {
    param(
        [object]$Context,
        [string]$Name,
        [object[]]$Steps,
        [string]$ConsentRef,
        [string]$VerificationInstruction,
        [switch]$ExpectTargetClosed
    )
    if ($script:Actions.Count -ge $script:ActionNames.Count -or
        $Name -cne $script:ActionNames[$script:Actions.Count]) {
        throw "The helper action is out of canonical order."
    }
    if ($ConsentRef -cne "none") {
        Read-ExactReceipt "Confirm the action-time $ConsentRef checkpoint for $Name." "CONSENT:${ConsentRef}:$Name"
    }
    $pre = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
    $Context.Observation = $pre.Observation
    $Context.ObservationCount += 1
    $preFrameRef = Get-OpaqueRef "frame" ([string]$pre.Observation.frameId)
    $methods = New-Object Collections.Generic.List[string]
    $methods.Add("computer.observe")
    $parameterDigests = New-Object Collections.Generic.List[string]
    $responseDigests = New-Object Collections.Generic.List[string]
    $responseDigests.Add($pre.Response.Digest)
    if ($Steps.Count -eq 0) {
        $middle = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
        $Context.Observation = $middle.Observation
        $Context.ObservationCount += 1
        $methods.Add("computer.observe")
        $responseDigests.Add($middle.Response.Digest)
    }
    $lastOpenFrameRef = $preFrameRef
    for ($stepIndex = 0; $stepIndex -lt $Steps.Count; $stepIndex += 1) {
        $step = $Steps[$stepIndex]
        $stepFrameId = [string]$Context.Observation.frameId
        $lastOpenFrameRef = Get-OpaqueRef "frame" $stepFrameId
        $params = [ordered]@{ frameId = [string]$Context.Observation.frameId }
        switch ($step.kind) {
            "click" {
                $click = Read-ClickParams $Context.Observation ([string]$step.label)
                foreach ($entry in $click.GetEnumerator()) { $params[$entry.Key] = $entry.Value }
                $method = "computer.click"
            }
            "key" { $params.key = [string]$step.value; $method = "computer.key" }
            "typeText" { $params.text = [string]$step.value; $method = "computer.typeText" }
            default { throw "The helper action plan contains an unsupported native step." }
        }
        $parameterDigests.Add((Get-TextSha256 ($params | ConvertTo-Json -Depth 8 -Compress)))
        $result = Invoke-ComputerCommand $method $params
        $methods.Add($method)
        $responseDigests.Add($result.Digest)
        $isTerminalCloseStep = $ExpectTargetClosed -and $stepIndex -eq ($Steps.Count - 1)
        if (-not $isTerminalCloseStep) {
            # Never accept response-state observation as post-action authority. The
            # server may retain an older public frame when its own follow-up observe
            # fails, so require a new explicit exact-bound observation here.
            $next = Get-FreshObservation $Context.WindowId $Context.Pid $stepFrameId
            $Context.Observation = $next.Observation
            $Context.ObservationCount += 1
            $methods.Add("computer.observe")
            $responseDigests.Add($next.Response.Digest)
        }
        $params.Clear()
        $result = $null
    }
    if ($ExpectTargetClosed) {
        $status = Invoke-ComputerCommand "computer.status" @{}
        $methods.Add("computer.status")
        $responseDigests.Add($status.Digest)
        if (@($status.Body.result.windows | Where-Object {
            [string]$_.id -ceq $Context.WindowId -and [int64]$_.pid -eq $Context.Pid
        }).Count -ne 0) {
            throw "The exact dedicated Chrome window remained after its close action."
        }
        $postFrameRef = Get-OpaqueRef "closed-frame" "$lastOpenFrameRef|$($status.Digest)"
        $Context.LastFrameRef = $lastOpenFrameRef
    }
    else {
        $post = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
        $Context.Observation = $post.Observation
        $Context.ObservationCount += 1
        $methods.Add("computer.observe")
        $responseDigests.Add($post.Response.Digest)
        $postFrameRef = Get-OpaqueRef "frame" ([string]$post.Observation.frameId)
        $Context.LastFrameRef = $postFrameRef
    }
    if ($preFrameRef -ceq $postFrameRef) { throw "The helper action did not obtain a fresh post-action frame." }
    Read-ExactReceipt $VerificationInstruction "VERIFIED:$Name"
    $script:Actions += [ordered]@{
        sequence = $script:Actions.Count + 1; name = $Name
        atUtc = [DateTimeOffset]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        source = $script:Source; epochRef = $Context.EpochRef; methods = @($methods)
        preFrameRef = $preFrameRef; postFrameRef = $postFrameRef
        normalizedParamsSha256 = Get-TextSha256 (@($parameterDigests) -join "`n")
        responseSha256 = Get-TextSha256 (@($responseDigests) -join "`n")
        httpStatus = 200; resultVerified = $true; postconditionVerified = $true; consentRef = $ConsentRef
    }
    $parameterDigests.Clear(); $responseDigests.Clear(); $methods.Clear()
}

function Save-RecordedScreenshot {
    param([object]$Context, [string]$Purpose)
    $expectedPurpose = @($script:Screenshots.Keys)[$script:ScreenshotRecords.Count]
    if ($Purpose -cne $expectedPurpose) { throw "Screenshot capture is out of canonical order." }
    $streamed = Get-FreshSharedFrame $Context "the $Purpose screenshot"
    $relative = [string]$streamed.screenshotUrl
    if (-not $relative.StartsWith("/api/computer/screenshot?id=", [StringComparison]::Ordinal) -or
        $relative.Contains("://")) { throw "The helper returned an invalid screenshot endpoint." }
    $rawName = $script:Screenshots[$Purpose]
    $rawPath = [IO.Path]::Combine($script:RawDirectory, $rawName)
    if ([IO.File]::Exists($rawPath) -or [IO.Directory]::Exists($rawPath)) { throw "A raw screenshot already exists." }
    Invoke-WebRequest -UseBasicParsing -Uri ("http://127.0.0.1:$Port" + $relative) -Method Get `
        -Headers @{ Authorization = "Bearer $($script:Token)" } -OutFile $rawPath -TimeoutSec 25 | Out-Null
    $facts = Get-PngDimensions $rawPath
    $frameRef = Get-OpaqueRef "frame" ([string]$streamed.frameId)
    $Context.LastFrameRef = $frameRef
    $script:ScreenshotRecords += [ordered]@{
        sequence = $script:ScreenshotRecords.Count + 1; purpose = $Purpose; source = $script:Source
        epochRef = $Context.EpochRef; shareRef = $Context.ShareRef
        frameRef = $frameRef
        rawImage = $rawName; endpoint = "/api/computer/screenshot"
        bytes = ([IO.FileInfo]::new($rawPath)).Length; sha256 = Get-Sha256 $rawPath
        width = $facts.Width; height = $facts.Height; exactWindowFrame = $true
        shareFrameFresh = $true; rawImageRetained = $false
    }
    $relative = $null
}

function Invoke-BrowserCommand {
    param([string]$Method, [hashtable]$Params)
    $response = Invoke-LoopbackJson "/api/v1/command" "Post" ([ordered]@{
        method = $Method
        params = $Params
        callId = "helper-browser-" + [Guid]::NewGuid().ToString("N")
    })
    if ($null -ne $response.Body.error) {
        throw "Browser command $Method failed during helper evidence."
    }
    return $response
}

function Show-DeterministicGreeting {
    param([object]$OwnedTarget)
    if ($OwnedTarget.runNonce -cne $script:Binding.runNonce -or
        $OwnedTarget.tabId -isnot [ValueType] -or
        $OwnedTarget.groupId -isnot [ValueType] -or
        $OwnedTarget.focusedByExactOwnedTabActivation -ne $true) {
        throw "The browser API matrix handoff is not bound to this exact run and focused target."
    }
    $tabId = [long]$OwnedTarget.tabId
    $groupId = [long]$OwnedTarget.groupId
    $methods = New-Object Collections.Generic.List[string]
    $digests = New-Object Collections.Generic.List[string]

    $started = Invoke-BrowserCommand "browser.control.start" @{ tabId = $tabId; ttlMs = 900000 }
    $methods.Add("browser.control.start")
    $digests.Add($started.Digest)
    if ($started.Body.result.active -ne $true) {
        throw "The exact matrix-owned demo lease did not start."
    }

    $observed = Invoke-BrowserCommand "page.observe" @{ tabId = $tabId }
    $methods.Add("page.observe")
    $digests.Add($observed.Digest)
    $observation = $observed.Body.result.snapshot
    foreach ($spec in @(
        @("textbox", "Display name", "page.fill", "Bridge Matrix"),
        @("select", "Favorite color", "page.select", "blue"),
        @("button", "Show greeting", "page.click", "")
    )) {
        $matches = @($observation.elements | Where-Object {
            $_.role -eq $spec[0] -and $_.name -eq $spec[1]
        })
        if ($matches.Count -ne 1) {
            throw "The deterministic demo target was not uniquely observed."
        }
        $params = @{
            tabId = $tabId
            ref = [string]$matches[0].ref
            generation = [string]$observation.generation
        }
        if ($spec[2] -eq "page.fill") { $params.text = $spec[3] }
        elseif ($spec[2] -eq "page.select") { $params.value = $spec[3] }
        else { $params.button = "left"; $params.clickCount = 1 }
        $mutated = Invoke-BrowserCommand $spec[2] $params
        $methods.Add($spec[2])
        $digests.Add($mutated.Digest)
        $observed = Invoke-BrowserCommand "page.observe" @{ tabId = $tabId }
        $methods.Add("page.observe")
        $digests.Add($observed.Digest)
        $observation = $observed.Body.result.snapshot
        $params.Clear()
    }
    if ([string]$observation.bodyText -notlike "*Hello, Bridge Matrix. blue selected.*") {
        throw "The deterministic greeting was not rendered."
    }
    $visible = Invoke-BrowserCommand "page.evaluate" @{
        tabId = $tabId
        expression = "(() => { const e=document.getElementById('result'); if(!e)return false; e.scrollIntoView({block:'center'}); return e.textContent==='Hello, Bridge Matrix. blue selected.'; })()"
    }
    $methods.Add("page.evaluate")
    $digests.Add($visible.Digest)
    if ($visible.Body.result.value -ne $true) {
        throw "The deterministic greeting was not brought into view."
    }
    $record = [ordered]@{
        source = "local-browser-bridge-api"
        apiMatrixRecordSha256 = Get-Sha256 $script:MatrixOutputPath
        targetBindingSha256 = Get-OpaqueRef "browser-target" "$tabId|$groupId"
        methodSequence = @($methods)
        requestResponseSha256 = Get-TextSha256 (@($digests) -join [Environment]::NewLine)
        resultText = "Hello, Bridge Matrix. blue selected."
        resultVerified = $true
    }
    $methods.Clear()
    $digests.Clear()
    $tabId = 0
    $groupId = 0
    return $record
}

function Add-LifecycleEvent {
    param([string]$Name, [string]$ConnectionState, [string]$ProcessRef, [string]$ExecutableSha256, [int]$ExitCode)
    $script:Lifecycle += [ordered]@{
        sequence = $script:Lifecycle.Count + 1; name = $Name
        atUtc = [DateTimeOffset]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        source = $script:Source; connectionState = $ConnectionState; processRef = $ProcessRef
        executableSha256 = $ExecutableSha256; exitCode = $ExitCode; resultVerified = $true
    }
}

function Wait-ServerReady {
    param([string]$ExpectedVersion, [int]$Seconds = 25)
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if ($null -ne $script:ServerProcess -and $script:ServerProcess.HasExited) {
            throw "The exact candidate server exited before readiness."
        }
        try {
            $health = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$Port/health" `
                -Method Get -TimeoutSec 2
            $body = $health.Content | ConvertFrom-Json
            if ([int]$health.StatusCode -eq 200 -and $body.ok -eq $true -and
                $body.version -ceq $ExpectedVersion) {
                return Get-BridgeState
            }
        }
        catch {
            if ([DateTime]::UtcNow -ge $deadline) { throw }
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for the exact candidate server readiness/version proof."
}

function Get-ProcessFamilyIds {
    param([int]$RootProcessId)
    $processes = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop)
    $ids = @($RootProcessId)
    do {
        $before = $ids.Count
        foreach ($process in $processes) {
            if ($ids -contains [int]$process.ParentProcessId -and
                $ids -notcontains [int]$process.ProcessId) {
                $ids += [int]$process.ProcessId
            }
        }
    } while ($ids.Count -ne $before)
    return @($ids | Sort-Object -Unique)
}

function Get-ListenerCountForProcesses {
    param([int[]]$ProcessIds)
    if ($ProcessIds.Count -eq 0) { return 0 }
    return @(Get-NetTCPConnection -State Listen -ErrorAction Stop | Where-Object {
        $ProcessIds -contains [int]$_.OwningProcess
    }).Count
}

function Set-MutationDisposition {
    param([hashtable]$State, [string]$Name, [string]$Disposition)
    if (-not $State.ContainsKey($Name) -or
        $Disposition -notin @("not_attempted", "outcome_unknown", "verified_applied", "restored")) {
        throw "The mutation disposition transition is invalid."
    }
    $current = [string]$State[$Name]
    $allowed = switch ($current) {
        "not_attempted" { @("outcome_unknown") }
        "outcome_unknown" { @("verified_applied", "restored") }
        "verified_applied" { @("outcome_unknown", "restored") }
        "restored" { @() }
        default { throw "The current mutation disposition is invalid." }
    }
    if ($allowed -cnotcontains $Disposition) {
        throw "The mutation disposition transition $current -> $Disposition is not allowed."
    }
    $State[$Name] = $Disposition
}

function Test-UnresolvedMutation {
    param([hashtable]$State, [string]$Name)
    return $State[$Name] -in @("outcome_unknown", "verified_applied")
}

function Get-UnresolvedMutationNames {
    param([hashtable]$State)
    return @($State.Keys | Where-Object { Test-UnresolvedMutation $State $_ } | Sort-Object)
}

function Invoke-UnrecordedNativeSteps {
    param(
        [string]$WindowInstruction,
        [object[]]$Steps,
        [string]$Receipt,
        [string]$ExpectedWindowId,
        [int64]$ExpectedPid = 0,
        [switch]$ExpectTargetClosed
    )
    if (-not [String]::IsNullOrWhiteSpace($Receipt)) {
        Read-ExactReceipt "Authorize and verify this ownership-bounded rollback action." $Receipt
    }
    if ([String]::IsNullOrWhiteSpace($ExpectedWindowId)) {
        $selected = Select-ExactWindow $WindowInstruction
    }
    else {
        Write-Host $WindowInstruction
        $selected = Get-ExactChromeWindow $ExpectedWindowId $ExpectedPid
    }
    $windowId = [string]$selected.WindowId
    [void](Get-ExactChromeWindow $windowId $selected.Pid)
    $observation = (Get-FreshObservation $windowId $selected.Pid $null).Observation
    for ($stepIndex = 0; $stepIndex -lt $Steps.Count; $stepIndex += 1) {
        $step = $Steps[$stepIndex]
        $stepFrameId = [string]$observation.frameId
        $params = [ordered]@{ frameId = [string]$observation.frameId }
        switch ($step.kind) {
            "click" {
                $click = Read-ClickParams $observation ([string]$step.label)
                foreach ($entry in $click.GetEnumerator()) { $params[$entry.Key] = $entry.Value }
                $method = "computer.click"
            }
            "key" { $params.key = [string]$step.value; $method = "computer.key" }
            "typeText" { $params.text = [string]$step.value; $method = "computer.typeText" }
            default { throw "Rollback contains an unsupported native step." }
        }
        [void](Invoke-ComputerCommand $method $params)
        $params.Clear()
        if (-not ($ExpectTargetClosed -and $stepIndex -eq ($Steps.Count - 1))) {
            [void](Get-ExactChromeWindow $windowId $selected.Pid)
            $observation = (Get-FreshObservation $windowId $selected.Pid $stepFrameId).Observation
        }
    }
    if ($ExpectTargetClosed) {
        $status = Invoke-ComputerCommand "computer.status" @{}
        if (@($status.Body.result.windows | Where-Object {
            [string]$_.id -ceq $windowId -and [int64]$_.pid -eq $selected.Pid
        }).Count -ne 0) {
            throw "The exact rollback-owned Chrome window remained open."
        }
    }
    $windowId = $null
    $observation = $null
}

function Read-RollbackToggleState {
    param([string]$WindowId, [int64]$ExpectedPid, [string]$Label)
    [void](Get-ExactChromeWindow $WindowId $ExpectedPid)
    $fresh = Get-FreshObservation $WindowId $ExpectedPid $null
    $value = (Read-Host "$Label in this fresh exact-owned helper frame (enabled/disabled)").Trim().ToLowerInvariant()
    if ($value -notin @("enabled", "disabled")) {
        throw "$Label rollback state was not reduced to enabled or disabled."
    }
    Read-ExactReceipt "Confirm the current $Label state was read from this fresh exact-owned frame." `
        "VERIFIED:rollback-live-$Label-$value"
    return $value
}

function Read-RollbackSavedTokenState {
    param([string]$WindowId, [int64]$ExpectedPid)
    [void](Get-ExactChromeWindow $WindowId $ExpectedPid)
    [void](Get-FreshObservation $WindowId $ExpectedPid $null)
    $value = (Read-Host "Saved-token state in this fresh exact-owned popup frame (configured/unconfigured)").Trim().ToLowerInvariant()
    if ($value -notin @("configured", "unconfigured")) {
        throw "Saved-token rollback state was not reduced to configured or unconfigured."
    }
    Read-ExactReceipt "Confirm the current saved-token state was read from this fresh exact-owned popup frame." `
        "VERIFIED:rollback-live-saved-token-$value"
    return $value
}

function Read-RollbackCandidateCardState {
    param([string]$WindowId, [int64]$ExpectedPid)
    [void](Get-ExactChromeWindow $WindowId $ExpectedPid)
    [void](Get-FreshObservation $WindowId $ExpectedPid $null)
    $value = (Read-Host "Exact v0.12.10 test-owned candidate card in this fresh chrome://extensions frame (present/absent)").Trim().ToLowerInvariant()
    if ($value -notin @("present", "absent")) {
        throw "Candidate-card rollback state was not reduced to present or absent."
    }
    Read-ExactReceipt "Confirm candidate-card presence was read from this fresh exact-owned Chrome frame." `
        "VERIFIED:rollback-live-candidate-card-$value"
    return $value
}

function Invoke-BestEffortUiRollback {
    param(
        [hashtable]$State,
        [object]$InitialDeveloperMode,
        [object]$InitialFullAccess,
        [object]$InitialSavedToken
    )
    $errors = New-Object Collections.Generic.List[string]
    try {
        if ($null -ne $script:ActiveEpoch) {
            $stopped = Invoke-ComputerCommand "computer.share.stop" @{}
            if ($stopped.Body.result.active -ne $false) { throw "Active helper share did not stop." }
            $script:ActiveEpoch = $null
        }
    }
    catch { $errors.Add("share: $($_.Exception.Message)") }

    if ((Test-UnresolvedMutation $State "SavedToken") -or
        (Test-UnresolvedMutation $State "FullAccess")) {
        try {
            if ([String]::IsNullOrWhiteSpace($script:LastOwnedPopupWindowId) -or
                $script:LastOwnedPopupWindowPid -le 0) {
                throw "No exact candidate-popup identity was bound before failure; popup state was not touched."
            }
            [void](Get-ExactChromeWindow $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid)
            if (Test-UnresolvedMutation $State "SavedToken") {
                if ($null -eq $InitialSavedToken -or $InitialSavedToken.configured -ne $false) {
                    throw "The live initial saved-token state is unavailable or was not unconfigured."
                }
                $tokenState = Read-RollbackSavedTokenState $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                if ($tokenState -ceq "configured") {
                    Invoke-UnrecordedNativeSteps `
                        "Use only the exact bound candidate popup for rollback token clearing." `
                        @([ordered]@{ kind="click"; label="Clear saved token button" }) `
                        "CONSENT:clearSavedTokenInitiate:rollback" `
                        $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                    Invoke-UnrecordedNativeSteps `
                        "Use only the same exact bound candidate popup for rollback confirmation." `
                        @([ordered]@{ kind="click"; label="affirmative clear-token confirmation" }) `
                        "CONSENT:clearSavedTokenConfirm:rollback" `
                        $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                    $tokenState = Read-RollbackSavedTokenState $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                }
                if ($tokenState -cne "unconfigured") { throw "Saved-token rollback was not verified." }
                Set-MutationDisposition $State "SavedToken" "restored"
            }
            if (Test-UnresolvedMutation $State "FullAccess") {
                if ($null -eq $InitialFullAccess -or
                    $InitialFullAccess.value -notin @("enabled", "disabled")) {
                    throw "The live initial Full Access state is unavailable."
                }
                $fullAccessState = Read-RollbackToggleState `
                    $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid "FullAccess"
                if ($fullAccessState -cne $InitialFullAccess.value) {
                    Invoke-UnrecordedNativeSteps `
                        "Use only the exact bound candidate popup for Full Access restoration." `
                        @([ordered]@{ kind="click"; label="Full Access toggle to the live-captured initial state" }) `
                        "CONSENT:fullAccessUse:rollback" `
                        $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                    $fullAccessState = Read-RollbackToggleState `
                        $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid "FullAccess"
                }
                if ($fullAccessState -cne $InitialFullAccess.value) { throw "Full Access rollback was not verified." }
                Set-MutationDisposition $State "FullAccess" "restored"
            }
        }
        catch { $errors.Add("popup: $($_.Exception.Message)") }
    }

    if ((Test-UnresolvedMutation $State "CandidateExtension") -or
        (Test-UnresolvedMutation $State "DeveloperMode") -or
        (Test-UnresolvedMutation $State "DedicatedWindow")) {
        try {
            if ([String]::IsNullOrWhiteSpace($script:DedicatedWindowId) -or
                $script:DedicatedWindowPid -le 0 -or
                -not $script:DedicatedAbsentBeforeCreation -or
                -not $script:DedicatedCreatedAsOnlyNewChromeWindow -or
                (Get-OpaqueRef "target" "$($script:DedicatedWindowId)|$($script:DedicatedWindowPid)") -cne $script:DedicatedTargetRef) {
                throw "The exact sole-new dedicated Chrome identity was not proved; Chrome state was not touched."
            }
            [void](Get-ExactChromeWindow $script:DedicatedWindowId $script:DedicatedWindowPid)
            Invoke-UnrecordedNativeSteps `
                "Use only the exact bound test-owned dedicated Chrome window for rollback." @(
                    [ordered]@{ kind="key"; value="Control+L" },
                    [ordered]@{ kind="typeText"; value="chrome://extensions" },
                    [ordered]@{ kind="key"; value="Enter" }
                ) "VERIFIED:rollback-chrome-extensions" `
                $script:DedicatedWindowId $script:DedicatedWindowPid
            if (Test-UnresolvedMutation $State "CandidateExtension") {
                $cardState = Read-RollbackCandidateCardState $script:DedicatedWindowId $script:DedicatedWindowPid
                if ($cardState -ceq "present") {
                    Invoke-UnrecordedNativeSteps `
                        "Use only the exact bound test-owned chrome://extensions window." @(
                            [ordered]@{ kind="click"; label="Remove on the exact v0.12.10 test-owned candidate card" },
                            [ordered]@{ kind="click"; label="confirm removal of that exact candidate card" }
                        ) "CONSENT:extensionDisposition:rollback" `
                        $script:DedicatedWindowId $script:DedicatedWindowPid
                    $cardState = Read-RollbackCandidateCardState $script:DedicatedWindowId $script:DedicatedWindowPid
                }
                if ($cardState -cne "absent") { throw "Candidate-card rollback was not verified." }
                Set-MutationDisposition $State "CandidateExtension" "restored"
            }
            if (Test-UnresolvedMutation $State "DeveloperMode") {
                if ($null -eq $InitialDeveloperMode -or
                    $InitialDeveloperMode.value -notin @("enabled", "disabled")) {
                    throw "The live initial Developer Mode state is unavailable."
                }
                $developerState = Read-RollbackToggleState `
                    $script:DedicatedWindowId $script:DedicatedWindowPid "DeveloperMode"
                if ($developerState -cne $InitialDeveloperMode.value) {
                    Invoke-UnrecordedNativeSteps `
                        "Use only the exact bound test-owned chrome://extensions window." `
                        @([ordered]@{ kind="click"; label="Developer Mode toggle to the live-captured initial state" }) `
                        "CONSENT:developerModeChange:rollback" `
                        $script:DedicatedWindowId $script:DedicatedWindowPid
                    $developerState = Read-RollbackToggleState `
                        $script:DedicatedWindowId $script:DedicatedWindowPid "DeveloperMode"
                }
                if ($developerState -cne $InitialDeveloperMode.value) { throw "Developer Mode rollback was not verified." }
                Set-MutationDisposition $State "DeveloperMode" "restored"
            }
            if (Test-UnresolvedMutation $State "DedicatedWindow") {
                $otherUnresolved = @(Get-UnresolvedMutationNames $State | Where-Object { $_ -cne "DedicatedWindow" })
                if ($otherUnresolved.Count -ne 0) {
                    throw "The exact dedicated window was intentionally left open because other rollback state is unresolved: $($otherUnresolved -join ', ')."
                }
                Invoke-UnrecordedNativeSteps `
                    "Close only the exact sole-new test-owned dedicated Chrome window." `
                    @([ordered]@{ kind="key"; value="Control+Shift+W" }) `
                    "VERIFIED:rollback-test-window-closed" `
                    $script:DedicatedWindowId $script:DedicatedWindowPid -ExpectTargetClosed
                Set-MutationDisposition $State "DedicatedWindow" "restored"
            }
        }
        catch { $errors.Add("chrome: $($_.Exception.Message)") }
    }
    return @($errors)
}

function Invoke-Run {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "The live computer-helper chain recorder runs only on Windows."
    }
    if ($Port -ne 17373) {
        throw "The v0.12.10 acceptance recorder requires the canonical 127.0.0.1:17373 endpoint."
    }
    $preflightPath = Resolve-OrdinaryFile $PreflightRecord "PreflightRecord"
    $runnerPath = Resolve-OrdinaryFile $ApiMatrixRunner "ApiMatrixRunner"
    $serverPath = Resolve-OrdinaryFile $ServerExecutable "ServerExecutable"
    $helperPath = Resolve-OrdinaryFile $HelperExecutable "HelperExecutable"
    $extensionDirectoryPath = Resolve-OrdinaryDirectory $ExtensionDirectory "ExtensionDirectory"
    $script:RawDirectory = Resolve-OrdinaryDirectory $RawScreenshotDirectory "RawScreenshotDirectory"
    if ([IO.DirectoryInfo]::new($script:RawDirectory).GetFileSystemInfos().Count -ne 0) {
        throw "RawScreenshotDirectory must begin empty."
    }
    $script:MatrixOutputPath = Resolve-NewJson $ApiMatrixRecord
    $outputPath = Resolve-NewJson $OutputRecord
    if ($script:MatrixOutputPath -ceq $outputPath) {
        throw "ApiMatrixRecord and OutputRecord must be distinct new files."
    }

    $preflight = Read-Json $preflightPath "PreflightRecord"
    $script:Binding = Get-CandidateBinding $preflight (Get-Sha256 $preflightPath)
    if ([IO.Path]::GetFileName($serverPath) -cne $preflight.candidate.server.name -or
        (Get-Sha256 $serverPath) -cne $preflight.candidate.server.sha256 -or
        [IO.Path]::GetFileName($helperPath) -cne $preflight.candidate.computerHelper.name -or
        (Get-Sha256 $helperPath) -cne $preflight.candidate.computerHelper.sha256) {
        throw "The live harness executables do not match the exact preflight candidate."
    }
    $payloadInventory = @($preflight.candidate.extension.extractedPayloadInventory)
    [void](Get-ExactExtensionPayloadDigest $extensionDirectoryPath $payloadInventory $preflight.candidate.extension.combinedPayloadSha256)

    $credential = New-Object byte[] 32
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    try { $rng.GetBytes($credential) } finally { $rng.Dispose() }
    $script:Token = [Convert]::ToBase64String($credential).TrimEnd('=').Replace('+', '-').Replace('/', '_')

    $script:Lifecycle = @()
    $script:Epochs = @()
    $script:Actions = @()
    $script:ScreenshotRecords = @()
    $script:CommandSequence = 0
    $script:ServerProcess = $null
    $script:ActiveEpoch = $null
    $script:BaselineChrome = $null
    $script:DedicatedWindowId = $null
    $script:DedicatedWindowPid = 0
    $script:DedicatedTargetRef = $null
    $script:DedicatedProcessRef = $null
    $script:DedicatedAbsentBeforeCreation = $false
    $script:DedicatedCreatedAsOnlyNewChromeWindow = $false
    $script:LastOwnedPopupWindowId = $null
    $script:LastOwnedPopupWindowPid = 0
    $helperProcess = $null
    $helperFamilyIds = @()
    $serverProcessRef = $null
    $helperProcessRef = $null
    $sessionBinding = $null
    $ownedTarget = $null
    $browserAction = $null
    $capturedDeveloperMode = $null
    $capturedFullAccess = $null
    $capturedSavedToken = $null
    $record = $null
    $primaryFailure = $null
    $cleanupErrors = New-Object Collections.Generic.List[string]
    $startedAt = [DateTimeOffset]::UtcNow
    $mutation = @{
        DedicatedWindow = "not_attempted"
        DeveloperMode = "not_attempted"
        CandidateExtension = "not_attempted"
        FullAccess = "not_attempted"
        SavedToken = "not_attempted"
    }
    $tokenWasPresent = Test-Path Env:LBB_TOKEN
    $portWasPresent = Test-Path Env:LBB_PORT
    $previousToken = [Environment]::GetEnvironmentVariable("LBB_TOKEN", "Process")
    $previousPort = [Environment]::GetEnvironmentVariable("LBB_PORT", "Process")

    try {
        if (@(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue).Count -ne 0) {
            throw "The acceptance port was not free before server start."
        }
        [Environment]::SetEnvironmentVariable("LBB_TOKEN", $script:Token, "Process")
        [Environment]::SetEnvironmentVariable("LBB_PORT", [string]$Port, "Process")
        $script:ServerProcess = Start-Process -FilePath $serverPath -ArgumentList @("--no-update-check") -WorkingDirectory ([IO.Path]::GetDirectoryName($serverPath)) -PassThru
        $serverProcessRef = Get-OpaqueRef "server-process" "$($script:ServerProcess.Id)|$($script:ServerProcess.StartTime.ToUniversalTime().Ticks)"
        $initialState = Wait-ServerReady $script:Version
        $listeners = @(Get-NetTCPConnection -State Listen -ErrorAction Stop | Where-Object {
            [int]$_.OwningProcess -eq $script:ServerProcess.Id
        })
        if ($listeners.Count -ne 1 -or $listeners[0].LocalAddress -cne "127.0.0.1" -or [int]$listeners[0].LocalPort -ne $Port) {
            throw "The candidate server did not own the sole expected IPv4 loopback listener."
        }
        Add-LifecycleEvent "server-ready" "disconnected" $serverProcessRef $preflight.candidate.server.sha256 -1
        if ($initialState.computerConnected -eq $true) {
            throw "A computer helper was already connected before this run."
        }
        Add-LifecycleEvent "computer-disconnected-before-helper-start" "disconnected" "not-applicable" "not-applicable" -1

        $helperProcess = Start-Process -FilePath $helperPath -WorkingDirectory ([IO.Path]::GetDirectoryName($helperPath)) -PassThru
        $helperProcessRef = Get-OpaqueRef "helper-process" "$($helperProcess.Id)|$($helperProcess.StartTime.ToUniversalTime().Ticks)"
        Add-LifecycleEvent "helper-started" "disconnected" $helperProcessRef $preflight.candidate.computerHelper.sha256 -1
        $connected = Wait-BridgeState {
            param($state)
            $state.computerConnected -eq $true -and $null -ne $state.computer
        } "candidate helper connection"
        $helperFamilyIds = @(Get-ProcessFamilyIds $helperProcess.Id)
        if ((Get-ListenerCountForProcesses $helperFamilyIds) -ne 0) {
            throw "The candidate helper family unexpectedly owned a TCP listener."
        }
        $sessionBinding = Get-OpaqueRef "helper-session" ([string]$connected.computer.sessionId)
        Add-LifecycleEvent "computer-connected" "connected" "not-applicable" "not-applicable" -1
        $script:BaselineChrome = Get-BaselineChromeWindowBinding

        $epoch = Start-RecordedEpoch $script:EpochNames[0] $script:EpochSurfaces[0] "Select the existing stock Chrome window."
        Set-MutationDisposition $mutation "DedicatedWindow" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[0] @([ordered]@{ kind="key"; value="Control+N" }) "none" "Verify exactly one new stock Chrome window appeared without launch flags."
        [void](Bind-NewDedicatedChromeWindow)
        Set-MutationDisposition $mutation "DedicatedWindow" "verified_applied"
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[1] $script:EpochSurfaces[1] "Select only the new dedicated stock Chrome window."
        Invoke-RecordedAction $epoch $script:ActionNames[1] @(
            [ordered]@{ kind="key"; value="Control+L" },
            [ordered]@{ kind="typeText"; value="chrome://extensions" },
            [ordered]@{ kind="key"; value="Enter" }
        ) "none" "Verify chrome://extensions is visible in the dedicated window and no candidate card exists."
        $capturedDeveloperMode = Read-LiveToggleState $epoch "DeveloperMode"
        $developerSteps = if ($capturedDeveloperMode.value -ceq "disabled") {
            Set-MutationDisposition $mutation "DeveloperMode" "outcome_unknown"
            @([ordered]@{ kind="click"; label="Developer Mode toggle" })
        } else { @() }
        $developerConsent = if ($capturedDeveloperMode.value -ceq "disabled") { "developerModeChange" } else { "none" }
        Invoke-RecordedAction $epoch $script:ActionNames[2] $developerSteps $developerConsent "Verify the live-captured initial Developer Mode value was $($capturedDeveloperMode.value) and the required test state is enabled."
        if ($developerSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "DeveloperMode" "verified_applied"
        }
        [void](Get-ExactExtensionPayloadDigest $extensionDirectoryPath $payloadInventory $preflight.candidate.extension.combinedPayloadSha256)
        Invoke-RecordedAction $epoch $script:ActionNames[3] @([ordered]@{ kind="click"; label="Load unpacked button" }) "installCandidate" "Verify Chrome's native Load unpacked picker opened."
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[2] $script:EpochSurfaces[2] "Select only Chrome's native Load unpacked file picker."
        Set-MutationDisposition $mutation "CandidateExtension" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[4] @(
            [ordered]@{ kind="key"; value="Control+L" },
            [ordered]@{ kind="typeText"; value=$extensionDirectoryPath },
            [ordered]@{ kind="key"; value="Enter" },
            [ordered]@{ kind="click"; label="Select Folder button in Chrome's native picker" }
        ) "installCandidate" "Verify the helper explicitly invoked Select Folder for the exact new test-owned directory and the native picker closed." -ExpectTargetClosed
        Stop-RecordedEpoch $epoch -TargetClosed
        [void](Get-ExactExtensionPayloadDigest $extensionDirectoryPath $payloadInventory $preflight.candidate.extension.combinedPayloadSha256)

        $epoch = Start-RecordedEpoch $script:EpochNames[3] $script:EpochSurfaces[3] "Reselect the dedicated stock Chrome extensions window."
        Invoke-RecordedAction $epoch $script:ActionNames[5] @() "none" "Verify exactly one enabled unpacked Local Browser Bridge v0.12.10 card, no duplicate, and no load error."
        Set-MutationDisposition $mutation "CandidateExtension" "verified_applied"
        Invoke-RecordedAction $epoch $script:ActionNames[6] @(
            [ordered]@{ kind="click"; label="Chrome Extensions menu button" },
            [ordered]@{ kind="click"; label="Local Browser Bridge entry in the Extensions menu" }
        ) "none" "Verify the Local Browser Bridge extension-owned popup opened."
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[4] $script:EpochSurfaces[4] "Select only the Local Browser Bridge extension-owned popup."
        $capturedFullAccess = Read-LiveToggleState $epoch "FullAccess"
        $capturedSavedToken = Read-LiveSavedTokenState $epoch
        $fullAccessSteps = if ($capturedFullAccess.value -ceq "disabled") {
            Set-MutationDisposition $mutation "FullAccess" "outcome_unknown"
            @([ordered]@{ kind="click"; label="Full Access toggle" })
        } else { @() }
        Invoke-RecordedAction $epoch $script:ActionNames[7] $fullAccessSteps "fullAccessUse" "Verify Full Access is enabled only for this bounded acceptance run."
        if ($fullAccessSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "FullAccess" "verified_applied"
        }
        Set-MutationDisposition $mutation "SavedToken" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[8] @(
            [ordered]@{ kind="click"; label="popup token field" },
            [ordered]@{ kind="typeText"; value=$script:Token },
            [ordered]@{ kind="click"; label="popup Connect button" }
        ) "acceptanceTokenSave" "Verify v0.12.10 is connected and the credential field is empty."
        Set-MutationDisposition $mutation "SavedToken" "verified_applied"
        Stop-RecordedEpoch $epoch

        [Environment]::SetEnvironmentVariable("LBB_TOKEN", $script:Token, "Process")
        $handoffOutput = @(& $runnerPath -Version $script:Version -Port $Port -PreflightRecord $preflightPath -OutputPath $script:MatrixOutputPath -PassThruOwnedTarget)
        if ($handoffOutput.Count -ne 1 -or $handoffOutput[0] -isnot [string]) {
            throw "The browser API matrix did not return exactly one in-memory owned-target JSON value."
        }
        try { $ownedTarget = [string]$handoffOutput[0] | ConvertFrom-Json }
        catch { throw "The browser API matrix owned-target handoff was not JSON." }
        [Environment]::SetEnvironmentVariable("LBB_TOKEN", $script:Token, "Process")
        $browserAction = Show-DeterministicGreeting $ownedTarget

        $epoch = Start-RecordedEpoch $script:EpochNames[5] $script:EpochSurfaces[5] "Select the dedicated Chrome window containing chrome://extensions and the matrix-owned demo."
        Invoke-RecordedAction $epoch $script:ActionNames[9] @([ordered]@{ kind="click"; label="the exact chrome://extensions tab" }) "none" "Verify exactly one enabled unpacked Local Browser Bridge v0.12.10 card, no error, and Chrome's native debugger-use indicator while the exact bridge lease is active."
        Save-RecordedScreenshot $epoch "extension-loaded"
        Invoke-RecordedAction $epoch $script:ActionNames[10] @([ordered]@{ kind="click"; label="the exact matrix-owned loopback demo tab" }) "none" "Verify the exact visible result is Hello, Bridge Matrix. blue selected."
        Save-RecordedScreenshot $epoch "api-action-result"
        Invoke-RecordedAction $epoch $script:ActionNames[11] @([ordered]@{ kind="click"; label="Coordinate target button on the exact loopback demo" }) "none" "Verify the visible Action log says coordinate:true and the synthetic session pointer is present."
        Save-RecordedScreenshot $epoch "computer-share-action"
        Invoke-RecordedAction $epoch $script:ActionNames[12] @(
            [ordered]@{ kind="click"; label="Chrome Extensions menu button" },
            [ordered]@{ kind="click"; label="Local Browser Bridge entry in the Extensions menu" }
        ) "none" "Verify the cleanup Local Browser Bridge popup opened."
        $stoppedLease = Invoke-BrowserCommand "browser.control.stop" @{}
        if ($stoppedLease.Body.result.active -eq $true) {
            throw "The browser lease remained active before credential cleanup."
        }
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[6] $script:EpochSurfaces[6] "Select only the Local Browser Bridge cleanup popup."
        Set-MutationDisposition $mutation "SavedToken" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[13] @([ordered]@{ kind="click"; label="Clear saved token button" }) "clearSavedTokenInitiate" "Verify the clear-token confirmation dialog appeared."
        Invoke-RecordedAction $epoch $script:ActionNames[14] @([ordered]@{ kind="click"; label="affirmative clear-token confirmation button" }) "clearSavedTokenConfirm" "Verify Not configured and the disabled Clear saved token button."
        Set-MutationDisposition $mutation "SavedToken" "restored"
        $restoreFullSteps = if ($capturedFullAccess.value -ceq "disabled") {
            @([ordered]@{ kind="click"; label="Full Access toggle back to disabled" })
        } else { @() }
        if ($restoreFullSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "FullAccess" "outcome_unknown"
        }
        Invoke-RecordedAction $epoch $script:ActionNames[15] $restoreFullSteps "fullAccessUse" "Verify Full Access exactly equals its live-captured $($capturedFullAccess.value) value."
        if ($restoreFullSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "FullAccess" "restored"
        }
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[7] $script:EpochSurfaces[7] "Select only the dedicated test-owned stock Chrome window."
        Set-MutationDisposition $mutation "CandidateExtension" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[16] @(
            [ordered]@{ kind="click"; label="the exact existing chrome://extensions tab" },
            [ordered]@{ kind="click"; label="Remove on the exact v0.12.10 test-owned candidate card" },
            [ordered]@{ kind="click"; label="confirm removal of that exact candidate card" }
        ) "extensionDisposition" "Verify the helper switched to the protected chrome://extensions tab before removing only the new test-owned v0.12.10 card."
        Set-MutationDisposition $mutation "CandidateExtension" "restored"
        $restoreDeveloperSteps = if ($capturedDeveloperMode.value -ceq "disabled") {
            @([ordered]@{ kind="click"; label="Developer Mode toggle back to disabled" })
        } else { @() }
        $restoreDeveloperConsent = if ($capturedDeveloperMode.value -ceq "disabled") { "developerModeChange" } else { "none" }
        if ($restoreDeveloperSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "DeveloperMode" "outcome_unknown"
        }
        Invoke-RecordedAction $epoch $script:ActionNames[17] $restoreDeveloperSteps $restoreDeveloperConsent "Verify Developer Mode exactly equals its live-captured $($capturedDeveloperMode.value) value."
        if ($restoreDeveloperSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "DeveloperMode" "restored"
        }
        Stop-RecordedEpoch $epoch
        [void](Get-ExactExtensionPayloadDigest $extensionDirectoryPath $payloadInventory $preflight.candidate.extension.combinedPayloadSha256)

        $epoch = Start-RecordedEpoch $script:EpochNames[8] $script:EpochSurfaces[8] "Reselect only the dedicated test-owned stock Chrome window for closure."
        Set-MutationDisposition $mutation "DedicatedWindow" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[18] @([ordered]@{ kind="key"; value="Control+Shift+W" }) "none" "Verify only the dedicated test-owned Chrome window closed." -ExpectTargetClosed
        Stop-RecordedEpoch $epoch -TargetClosed
        Set-MutationDisposition $mutation "DedicatedWindow" "restored"

        Add-LifecycleEvent "helper-owner-forced-termination-requested" "connected" $helperProcessRef $preflight.candidate.computerHelper.sha256 -1
        $helperFamilyIds = @(Get-ProcessFamilyIds $helperProcess.Id)
        Stop-Process -Id $helperProcess.Id -Force -ErrorAction Stop
        if (-not $helperProcess.WaitForExit(10000)) {
            throw "The exact candidate helper supervisor did not terminate within ten seconds."
        }
        $helperExitCode = [int]$helperProcess.ExitCode
        if ($helperExitCode -eq 0) {
            throw "The owner-forced helper termination unexpectedly reported a graceful zero exit."
        }
        Add-LifecycleEvent "helper-owner-forced-terminated" "connected" $helperProcessRef $preflight.candidate.computerHelper.sha256 $helperExitCode
        [void](Wait-BridgeState { param($state) $state.computerConnected -ne $true } "helper disconnection after exact supervisor termination")
        Add-LifecycleEvent "computer-disconnected-after-helper-termination" "disconnected" "not-applicable" "not-applicable" -1
        if (@(Get-Process -Id $helperFamilyIds -ErrorAction SilentlyContinue).Count -ne 0 -or
            (Get-ListenerCountForProcesses $helperFamilyIds) -ne 0) {
            throw "The exact helper supervisor family retained a process or listener."
        }
        if ((Get-Sha256 $helperPath) -cne $preflight.candidate.computerHelper.sha256) {
            throw "The candidate helper executable changed during the run."
        }

        Stop-Process -Id $script:ServerProcess.Id -Force -ErrorAction Stop
        if (-not $script:ServerProcess.WaitForExit(10000)) {
            throw "The exact candidate server did not terminate within ten seconds."
        }
        $serverExitCode = [int]$script:ServerProcess.ExitCode
        if (@(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object {
            [int]$_.OwningProcess -eq $script:ServerProcess.Id
        }).Count -ne 0) {
            throw "The exact candidate server retained a listener after termination."
        }
        if ((Get-Sha256 $serverPath) -cne $preflight.candidate.server.sha256) {
            throw "The candidate server executable changed during the run."
        }
        Add-LifecycleEvent "server-owner-forced-terminated" "disconnected" $serverProcessRef $preflight.candidate.server.sha256 $serverExitCode

        if ($script:Lifecycle.Count -ne 8 -or $script:Epochs.Count -ne 9 -or
            $script:Actions.Count -ne 19 -or $script:ScreenshotRecords.Count -ne 3) {
            throw "The live helper chain is incomplete."
        }
        $matrix = Read-Json $script:MatrixOutputPath "ApiMatrixRecord"
        if ($matrix.version -cne $script:Version -or $matrix.passed -ne $true -or
            $matrix.candidateBinding.runNonce -cne $script:Binding.runNonce -or
            $matrix.candidateBinding.computerHelperSha256 -cne $script:Binding.computerHelperSha256) {
            throw "The API matrix did not pass against the exact candidate/helper/run."
        }
        $finishedAt = [DateTimeOffset]::UtcNow
        $record = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-computer-helper-chain"
            version = $script:Version
            candidateBinding = $script:Binding
            passed = $true
            recordedBy = [ordered]@{
                name = [IO.Path]::GetFileName($PSCommandPath)
                sha256 = Get-Sha256 $PSCommandPath
                source = "candidate-final-sha-blob"
            }
            run = [ordered]@{
                runNonce = $script:Binding.runNonce
                preflightRecordSha256 = $script:Binding.preflightRecordSha256
                apiMatrixRecordSha256 = Get-Sha256 $script:MatrixOutputPath
                startedAtUtc = $startedAt.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
                finishedAtUtc = $finishedAt.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
            }
            server = [ordered]@{
                executableName = $preflight.candidate.server.name
                sha256 = $preflight.candidate.server.sha256
                processRef = $serverProcessRef
                soleListener = "127.0.0.1:17373"
                updateCheckDisabled = $true
            }
            helper = [ordered]@{
                executableName = $preflight.candidate.computerHelper.name
                sha256 = $preflight.candidate.computerHelper.sha256
                connectedThroughLoopbackServer = $true
                serverApiOnly = $true
                processRef = $helperProcessRef
                sessionBindingSha256 = $sessionBinding
                rawSessionIdentifierRetained = $false
            }
            extensionPayload = [ordered]@{
                fileCount = 11
                combinedPayloadSha256 = $script:Binding.extractedPayloadSha256
                verifiedBeforeLoad = $true
                verifiedAfterLoad = $true
                verifiedAfterCleanup = $true
            }
            initialState = [ordered]@{
                capturedFromFreshHelperFrames = $true
                developerMode = $capturedDeveloperMode
                fullAccess = $capturedFullAccess
                savedToken = $capturedSavedToken
            }
            windowBinding = [ordered]@{
                application = "google-chrome"
                baselineChromeWindowCount = $script:BaselineChrome.Count
                baselineTargetSetSha256 = $script:BaselineChrome.Digest
                dedicatedTargetRef = $script:DedicatedTargetRef
                dedicatedProcessRef = $script:DedicatedProcessRef
                dedicatedAbsentBeforeCreation = $script:DedicatedAbsentBeforeCreation
                dedicatedCreatedAsOnlyNewChromeWindow = $script:DedicatedCreatedAsOnlyNewChromeWindow
                sameDedicatedTargetAcrossEpochs = $true
            }
            lifecycle = $script:Lifecycle
            windowEpochs = $script:Epochs
            actions = $script:Actions
            browserAction = $browserAction
            screenshots = $script:ScreenshotRecords
            cleanup = [ordered]@{
                allSharesStopped = $true
                helperTerminationDisposition = "owner-forced-exact-supervisor"
                helperExitCode = $helperExitCode
                helperDisconnectedAfterTermination = $true
                helperChildrenRemaining = 0
                helperListenersRemaining = 0
                serverTerminationDisposition = "owner-forced-exact-process"
                serverExitCode = $serverExitCode
                serverListenersRemaining = 0
                candidateExtensionRemoved = $true
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
        $serialized = $record | ConvertTo-Json -Depth 30 -Compress
        if ([regex]::IsMatch($serialized, '(?i)"(?:windowId|frameId|shareId|sessionId|tabId|token|authorization|bearer)"\s*:')) {
            throw "The live helper record retained a raw identifier or secret field."
        }
    }
    catch {
        $primaryFailure = $_
    }
    finally {
        $possibleUiMutation = @(Get-UnresolvedMutationNames $mutation).Count -ne 0
        if ($null -ne $primaryFailure -and $possibleUiMutation) {
            if ($null -ne $helperProcess -and -not $helperProcess.HasExited -and
                $null -ne $script:ServerProcess -and -not $script:ServerProcess.HasExited) {
                try {
                    $state = Get-BridgeState
                    if ($state.computerConnected -eq $true) {
                        foreach ($rollbackError in @(
                            Invoke-BestEffortUiRollback $mutation $capturedDeveloperMode $capturedFullAccess $capturedSavedToken
                        )) {
                            $cleanupErrors.Add($rollbackError)
                        }
                        $unresolved = @(Get-UnresolvedMutationNames $mutation)
                        if ($unresolved.Count -ne 0) {
                            $cleanupErrors.Add("UI rollback left exact mutation dispositions unresolved ($($unresolved -join ', ')); inspect only the dedicated test Chrome window, candidate card, Developer Mode, Full Access, and saved-token state.")
                        }
                    }
                    else {
                        $cleanupErrors.Add("UI rollback was unavailable because the exact packaged helper was disconnected; inspect the dedicated test Chrome window, candidate card, Developer Mode, Full Access, and saved-token state.")
                    }
                }
                catch { $cleanupErrors.Add("rollback: $($_.Exception.Message)") }
            }
            else {
                $cleanupErrors.Add("UI rollback was unavailable because the exact helper/server transport was not alive; inspect the dedicated test Chrome window, candidate card, Developer Mode, Full Access, and saved-token state.")
            }
        }
        if ($null -ne $helperProcess -and -not $helperProcess.HasExited) {
            try {
                $failureFamily = @(Get-ProcessFamilyIds $helperProcess.Id)
                Stop-Process -Id $helperProcess.Id -Force -ErrorAction Stop
                if (-not $helperProcess.WaitForExit(10000)) { throw "helper termination timeout" }
                if (@(Get-Process -Id $failureFamily -ErrorAction SilentlyContinue).Count -ne 0) { throw "helper child remained" }
            }
            catch { $cleanupErrors.Add("helper: $($_.Exception.Message)") }
        }
        if ($null -ne $script:ServerProcess -and -not $script:ServerProcess.HasExited) {
            try {
                Stop-Process -Id $script:ServerProcess.Id -Force -ErrorAction Stop
                if (-not $script:ServerProcess.WaitForExit(10000)) { throw "server termination timeout" }
            }
            catch { $cleanupErrors.Add("server: $($_.Exception.Message)") }
        }
        try {
            if ($tokenWasPresent) { [Environment]::SetEnvironmentVariable("LBB_TOKEN", $previousToken, "Process") }
            else { [Environment]::SetEnvironmentVariable("LBB_TOKEN", $null, "Process") }
            if ($portWasPresent) { [Environment]::SetEnvironmentVariable("LBB_PORT", $previousPort, "Process") }
            else { [Environment]::SetEnvironmentVariable("LBB_PORT", $null, "Process") }
        }
        catch { $cleanupErrors.Add("environment: $($_.Exception.Message)") }

        $script:Token = $null
        if ($null -ne $credential) { [Array]::Clear($credential, 0, $credential.Length) }
        $ownedTarget = $null
        $sessionBinding = $null
        $script:ActiveEpoch = $null

        if ($null -ne $primaryFailure) {
            try {
                $rawEntries = @([IO.DirectoryInfo]::new($script:RawDirectory).GetFileSystemInfos())
                $allowedRawNames = @($script:Screenshots.Values)
                if (@($rawEntries | Where-Object {
                    $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
                    $allowedRawNames -cnotcontains $_.Name
                }).Count -ne 0) {
                    throw "raw screenshot scratch contains an unexpected or linked entry"
                }
                foreach ($entry in $rawEntries) { [IO.File]::Delete($entry.FullName) }
                foreach ($path in @($script:MatrixOutputPath, $outputPath)) {
                    if ([IO.File]::Exists($path)) {
                        $file = [IO.FileInfo]::new($path)
                        if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                            throw "failure artifact is a reparse point"
                        }
                        [IO.File]::Delete($path)
                    }
                }
            }
            catch { $cleanupErrors.Add("artifacts: $($_.Exception.Message)") }
        }
    }

    if ($null -ne $primaryFailure) {
        $suffix = if ($cleanupErrors.Count -eq 0) { "Failure rollback completed." }
        else { "Failure rollback was incomplete: " + ($cleanupErrors -join "; ") }
        throw "$($primaryFailure.Exception.Message) $suffix"
    }
    if ($cleanupErrors.Count -ne 0) {
        throw "The acceptance run completed but shell cleanup failed: $($cleanupErrors -join '; ')"
    }
    if ($null -eq $record) {
        throw "The acceptance run produced no final machine record."
    }

    $temporary = "$outputPath.new"
    try {
        [IO.File]::WriteAllText($temporary, (($record | ConvertTo-Json -Depth 30) + [Environment]::NewLine), $script:Utf8NoBom)
        [IO.File]::Move($temporary, $outputPath)
    }
    catch {
        $finalWriteFailure = $_
        try {
            Remove-CanonicalRawScreenshots $script:RawDirectory
        }
        catch {
            throw "$($finalWriteFailure.Exception.Message) Final-record failure cleanup was incomplete: $($_.Exception.Message)"
        }
        throw "$($finalWriteFailure.Exception.Message) Canonical raw screenshot cleanup completed."
    }
    finally {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
    }
    Write-Output "Live candidate-bound computer-helper chain record created."
}
function Invoke-SelfTest {
    $bad = [ordered]@{ windowId = 42 } | ConvertTo-Json -Compress
    if (-not [regex]::IsMatch($bad, '(?i)"(?:windowId|frameId|shareId|sessionId|tabId|token|authorization|bearer)"\s*:')) {
        throw "Computer-helper recorder raw-identifier self-test failed."
    }
    $hexRejected = $false
    try { Assert-Hex "not-a-digest" 64 "self-test digest" } catch { $hexRejected = $true }
    if (-not $hexRejected) { throw "Computer-helper recorder digest self-test failed." }
    if ($script:Screenshots.Count -ne 3 -or $script:EpochNames.Count -ne 9 -or
        $script:ActionNames.Count -ne 19 -or
        @($script:Screenshots.Values | Select-Object -Unique).Count -ne 3) {
        throw "Computer-helper recorder canonical sequence self-test failed."
    }
    $mutationSelfTest = @{
        DedicatedWindow = "not_attempted"; DeveloperMode = "not_attempted"
        CandidateExtension = "not_attempted"; FullAccess = "not_attempted"; SavedToken = "not_attempted"
    }
    Set-MutationDisposition $mutationSelfTest "DedicatedWindow" "outcome_unknown"
    if (-not (Test-UnresolvedMutation $mutationSelfTest "DedicatedWindow")) {
        throw "Computer-helper recorder unresolved-mutation self-test failed."
    }
    Set-MutationDisposition $mutationSelfTest "DedicatedWindow" "verified_applied"
    Set-MutationDisposition $mutationSelfTest "DedicatedWindow" "restored"
    if (Test-UnresolvedMutation $mutationSelfTest "DedicatedWindow") {
        throw "Computer-helper recorder restored-mutation self-test failed."
    }
    $invalidDispositionRejected = $false
    try { Set-MutationDisposition $mutationSelfTest "DedicatedWindow" "assumed_restored" }
    catch { $invalidDispositionRejected = $true }
    if (-not $invalidDispositionRejected) {
        throw "Computer-helper recorder invalid-mutation-disposition self-test failed."
    }
    $invalidJumpRejected = $false
    try { Set-MutationDisposition $mutationSelfTest "DeveloperMode" "restored" }
    catch { $invalidJumpRejected = $true }
    if (-not $invalidJumpRejected) {
        throw "Computer-helper recorder invalid-mutation-transition self-test failed."
    }
    $goodSharedFrame = [pscustomobject]@{
        windowId = "window-1"; pid = 42; appName = "chrome.exe"; frameId = "frame-2"
        shareId = "share-1"; sourceSequence = 2
        share = [pscustomobject]@{ active = $true; id = "share-1" }
    }
    if (-not (Test-ExactObservationIdentity $goodSharedFrame "window-1" 42) -or
        -not (Test-ExactSharedFrame $goodSharedFrame "window-1" 42 "share-1" 1) -or
        (Test-ExactSharedFrame $goodSharedFrame "window-1" 42 "share-1" 2)) {
        throw "Computer-helper recorder exact shared-frame self-test failed."
    }
    $wrongPidFrame = $goodSharedFrame | ConvertTo-Json -Depth 5 -Compress | ConvertFrom-Json
    $wrongPidFrame.pid = 43
    if (Test-ExactObservationIdentity $wrongPidFrame "window-1" 42) {
        throw "Computer-helper recorder accepted an observation from a reused HWND/wrong PID."
    }
    $wrongAppFrame = $goodSharedFrame | ConvertTo-Json -Depth 5 -Compress | ConvertFrom-Json
    $wrongAppFrame.appName = "notepad.exe"
    if (Test-ExactObservationIdentity $wrongAppFrame "window-1" 42) {
        throw "Computer-helper recorder accepted an observation from a non-Chrome application."
    }
    $source = [IO.File]::ReadAllText($PSCommandPath, $script:Utf8NoBom)
    if ($source.Contains('[string]$' + 'JournalRecord') -or -not $source.Contains("Invoke-ComputerCommand") -or
        -not $source.Contains("computer.share.start") -or -not $source.Contains("computer.share.stop") -or
        $source.Contains("computer." + "helper.shutdown") -or
        -not $source.Contains('Stop-Process -Id $helperProcess.Id -Force') -or
        -not $source.Contains('ArgumentList @("--no-update-check")') -or
        -not $source.Contains("Get-ExactExtensionPayloadDigest") -or
        -not $source.Contains("Invoke-BestEffortUiRollback") -or
        -not $source.Contains("Read-LiveToggleState") -or
        -not $source.Contains("Read-LiveSavedTokenState") -or
        -not $source.Contains("[void](Bind-NewDedicatedChromeWindow)") -or
        -not $source.Contains("Control+N did not produce exactly one new stock-Chrome window") -or
        -not $source.Contains("Test-ExactSharedFrame") -or
        -not $source.Contains('Get-FreshSharedFrame $Context "the $Purpose screenshot"') -or
        -not $source.Contains("Never accept response-state observation as post-action authority") -or
        -not $source.Contains("Get-ExactChromeWindow `$ExpectedWindowId `$ExpectedPid") -or
        -not $source.Contains('Set-MutationDisposition $mutation "DedicatedWindow" "outcome_unknown"') -or
        $source.Contains("MayBe" + "Changed") -or $source.Contains("May" + "Exist") -or
        -not $source.Contains("Select Folder button in Chrome's native picker") -or
        -not $source.Contains("the exact existing chrome://extensions tab") -or
        -not $source.Contains("UI rollback was unavailable because the exact helper/server transport was not alive") -or
        -not $source.Contains("Remove-CanonicalRawScreenshots `$script:RawDirectory") -or
        -not $source.Contains("PassThruOwnedTarget") -or
        -not $source.Contains("ConvertFrom-Json")) {
        throw "Computer-helper recorder live-execution self-test failed."
    }
    Write-Output "Computer-helper chain recorder self-test passed."
}

switch ($Mode) {
    "Run" { Invoke-Run }
    "SelfTest" { Invoke-SelfTest }
}
