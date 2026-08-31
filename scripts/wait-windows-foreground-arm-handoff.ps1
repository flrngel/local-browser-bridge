#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Watch", "SelfTest")]
    [string]$Mode,

    [string]$EvidenceDirectory,

    [int]$RunnerProcessId = 0,

    [string]$RunnerStartedAtUtc,

    [ValidateRange(1, 300)]
    [int]$WaitTimeoutSeconds = 300,

    [ValidateRange(1, 300)]
    [int]$MaximumMarkerAgeSeconds = 30,

    [ValidateRange(0, 30)]
    [int]$FutureToleranceSeconds = 5,

    [ValidateRange(0, 30)]
    [int]$FileTimestampToleranceSeconds = 5,

    [ValidateRange(25, 1000)]
    [int]$PollMilliseconds = 100
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$script:ProductVersion = "0.12.66"
$script:ForegroundSentinelWindowTitle = "LBB Foreground Sentinel"
$script:MarkerSchemaVersion = 2
$script:MaximumMarkerBytes = 16384
$script:Utf8StrictNoBom = [Text.UTF8Encoding]::new($false, $true)
$script:MarkerFields = @(
    "schemaVersion",
    "productVersion",
    "kind",
    "status",
    "requestId",
    "publishedAtUtc",
    "timeoutSeconds",
    "operatorActionRequired",
    "preferredRelaySurface",
    "fallbackRelaySurface",
    "expectedVisibleWindowTitle",
    "expectedVisibleButtonText",
    "expectedAccessibleName",
    "action",
    "stopUiAfterAction",
    "requiresSeparateAuthorization",
    "markerGrantsAuthorization",
    "markerGrantsConsent",
    "externalOneShotConsentRequired",
    "visualConfirmationRequired",
    "maximumClickAttempts",
    "retryOnUnknownOutcome",
    "instruction",
    "requestDelivered",
    "buttonEnabled",
    "nativeTopologyMatched",
    "inputStateAtPublication",
    "notificationOnly",
    "acceptedAsAuthority",
    "rawWindowHandlesRecorded",
    "rawCursorCoordinatesRecorded",
    "pathsRecorded",
    "secretsRecorded"
)

function Assert-Condition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Assert-ExactBoolean {
    param([object]$Actual, [bool]$Expected, [string]$Label)
    if ($Actual -isnot [bool] -or $Actual -ne $Expected) {
        throw "$Label must be $Expected."
    }
}

function Assert-ExactIntegerRange {
    param([object]$Actual, [int64]$Minimum, [int64]$Maximum, [string]$Label)
    $integerTypes = @(
        [TypeCode]::SByte,
        [TypeCode]::Byte,
        [TypeCode]::Int16,
        [TypeCode]::UInt16,
        [TypeCode]::Int32,
        [TypeCode]::UInt32,
        [TypeCode]::Int64,
        [TypeCode]::UInt64
    )
    if ($null -eq $Actual -or
        $Actual -is [bool] -or
        $Actual -isnot [ValueType] -or
        $integerTypes -notcontains [Type]::GetTypeCode($Actual.GetType()) -or
        [decimal]$Actual -lt $Minimum -or
        [decimal]$Actual -gt $Maximum) {
        throw "$Label must be an integer from $Minimum through $Maximum."
    }
}

function Assert-ExactPropertyOrder {
    param([object]$Object, [string[]]$Expected, [string]$Label)
    if ($null -eq $Object -or $Object -is [Array]) {
        throw "$Label must be one object."
    }
    $actual = @($Object.PSObject.Properties.Name)
    if (($actual -join "`n") -cne ($Expected -join "`n")) {
        throw "$Label fields are not in canonical order."
    }
}

function ConvertFrom-CanonicalUtcTimestamp {
    param([object]$Value, [string]$Label)
    $parsed = [DateTimeOffset]::MinValue
    if ($Value -isnot [string] -or
        $Value -cnotmatch '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{7}Z$' -or
        -not [DateTimeOffset]::TryParseExact(
            $Value,
            "o",
            [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind,
            [ref]$parsed
        ) -or
        $parsed.Offset -ne [TimeSpan]::Zero) {
        throw "$Label must be a canonical UTC round-trip timestamp."
    }
    return $parsed
}

function ConvertTo-UtcTimestamp {
    param([object]$Value, [string]$Label)
    if ($Value -is [DateTimeOffset]) {
        return ([DateTimeOffset]$Value).ToUniversalTime()
    }
    if ($Value -is [DateTime]) {
        return [DateTimeOffset]::new(([DateTime]$Value).ToUniversalTime())
    }
    throw "$Label did not return a timestamp."
}

function ConvertTo-CanonicalUtcString {
    param([DateTimeOffset]$Value)
    return $Value.UtcDateTime.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
}

function Assert-ExactMarkerSchema {
    param([object]$Marker, [string]$Json)

    Assert-ExactPropertyOrder $Marker $script:MarkerFields "foreground-arm request marker"
    foreach ($field in $script:MarkerFields) {
        $propertyPattern = '"' + [regex]::Escape($field) + '"\s*:'
        if ([regex]::Matches($Json, $propertyPattern).Count -ne 1) {
            throw "The foreground-arm request marker must contain each canonical field exactly once."
        }
    }

    Assert-ExactIntegerRange $Marker.schemaVersion $script:MarkerSchemaVersion $script:MarkerSchemaVersion "marker schemaVersion"
    if ($Marker.productVersion -isnot [string] -or $Marker.productVersion -cne $script:ProductVersion) {
        throw "The marker productVersion must be $($script:ProductVersion)."
    }
    if ($Marker.kind -isnot [string] -or $Marker.kind -cne "foreground-arm") {
        throw "The marker kind is invalid."
    }
    if ($Marker.requestId -isnot [string] -or $Marker.requestId -cnotmatch '^[0-9a-f]{32}$') {
        throw "The marker requestId is invalid."
    }
    $null = ConvertFrom-CanonicalUtcTimestamp $Marker.publishedAtUtc "marker publishedAtUtc"
    Assert-ExactIntegerRange $Marker.timeoutSeconds 15 300 "marker timeoutSeconds"

    if ($Marker.preferredRelaySurface -isnot [string] -or
        $Marker.preferredRelaySurface -cne "windows-computer-use-app-share" -or
        $Marker.fallbackRelaySurface -isnot [string] -or
        $Marker.fallbackRelaySurface -cne "human-on-windows-session") {
        throw "The marker relay surfaces are invalid."
    }
    if ($Marker.expectedAccessibleName -isnot [string] -or
        $Marker.expectedAccessibleName -cne "Click to arm Windows acceptance") {
        throw "The marker accessible target is invalid."
    }
    if ($Marker.expectedVisibleWindowTitle -isnot [string] -or
        $Marker.expectedVisibleWindowTitle -cne $script:ForegroundSentinelWindowTitle) {
        throw "The marker stable foreground-sentinel window title is invalid."
    }

    Assert-ExactBoolean $Marker.stopUiAfterAction $true "marker stopUiAfterAction"
    Assert-ExactBoolean $Marker.requiresSeparateAuthorization $true "marker requiresSeparateAuthorization"
    Assert-ExactBoolean $Marker.markerGrantsAuthorization $false "marker markerGrantsAuthorization"
    Assert-ExactBoolean $Marker.markerGrantsConsent $false "marker markerGrantsConsent"
    Assert-ExactBoolean $Marker.externalOneShotConsentRequired $true "marker externalOneShotConsentRequired"
    Assert-ExactBoolean $Marker.visualConfirmationRequired $true "marker visualConfirmationRequired"
    Assert-ExactBoolean $Marker.retryOnUnknownOutcome $false "marker retryOnUnknownOutcome"
    Assert-ExactBoolean $Marker.requestDelivered $true "marker requestDelivered"
    Assert-ExactBoolean $Marker.buttonEnabled $true "marker buttonEnabled"
    Assert-ExactBoolean $Marker.nativeTopologyMatched $true "marker nativeTopologyMatched"
    Assert-ExactBoolean $Marker.notificationOnly $true "marker notificationOnly"
    Assert-ExactBoolean $Marker.acceptedAsAuthority $false "marker acceptedAsAuthority"
    Assert-ExactBoolean $Marker.rawWindowHandlesRecorded $false "marker rawWindowHandlesRecorded"
    Assert-ExactBoolean $Marker.rawCursorCoordinatesRecorded $false "marker rawCursorCoordinatesRecorded"
    Assert-ExactBoolean $Marker.pathsRecorded $false "marker pathsRecorded"
    Assert-ExactBoolean $Marker.secretsRecorded $false "marker secretsRecorded"

    if ($Marker.status -ceq "action-required") {
        Assert-ExactBoolean $Marker.operatorActionRequired $true "action-required operatorActionRequired"
        Assert-ExactIntegerRange $Marker.maximumClickAttempts 1 1 "action-required maximumClickAttempts"
        if ($Marker.inputStateAtPublication -isnot [string] -or
            $Marker.inputStateAtPublication -cne "not-started" -or
            $Marker.expectedVisibleButtonText -isnot [string] -or
            $Marker.expectedVisibleButtonText -cne "CLICK TO ARM" -or
            $Marker.action -isnot [string] -or
            $Marker.action -cne "single-left-click" -or
            $Marker.instruction -isnot [string] -or
            $Marker.instruction -cne "Use a separately authorized Windows Computer Use app share to visually confirm this exact window and button, click it once, then stop all UI use. If it already says ARMED or the outcome is uncertain, do not click or retry.") {
            throw "The action-required marker semantics are invalid."
        }
    }
    elseif ($Marker.status -ceq "already-armed") {
        Assert-ExactBoolean $Marker.operatorActionRequired $false "already-armed operatorActionRequired"
        Assert-ExactIntegerRange $Marker.maximumClickAttempts 0 0 "already-armed maximumClickAttempts"
        if ($Marker.inputStateAtPublication -isnot [string] -or
            $Marker.inputStateAtPublication -cne "already-acknowledged" -or
            $Marker.expectedVisibleButtonText -isnot [string] -or
            $Marker.expectedVisibleButtonText -cne "ARMED - DO NOT USE THIS SESSION" -or
            $Marker.action -isnot [string] -or
            $Marker.action -cne "none" -or
            $Marker.instruction -isnot [string] -or
            $Marker.instruction -cne "Do not click; stop all UI use because the foreground arm is already acknowledged.") {
            throw "The already-armed marker semantics are invalid."
        }
    }
    else {
        throw "The marker status is invalid."
    }
}

function ConvertFrom-ExactMarkerJson {
    param([string]$Json)
    if ([String]::IsNullOrWhiteSpace($Json) -or $Json.Length -gt $script:MaximumMarkerBytes) {
        throw "The foreground-arm request marker has an invalid size."
    }
    if ($Json.IndexOf([char]0) -ge 0) {
        throw "The foreground-arm request marker contains a NUL character."
    }
    $marker = $null
    try {
        $conversionArguments = @{
            InputObject = $Json
            ErrorAction = "Stop"
        }
        $convertFromJson = Get-Command ConvertFrom-Json -CommandType Cmdlet -ErrorAction Stop
        if ($convertFromJson.Parameters.ContainsKey("DateKind")) {
            $conversionArguments["DateKind"] = "String"
        }
        $marker = ConvertFrom-Json @conversionArguments
    }
    catch {
        throw "The foreground-arm request marker is not one complete JSON object."
    }
    Assert-ExactMarkerSchema $marker $Json
    return $marker
}

function Resolve-OrdinaryEvidenceDirectory {
    param([string]$Path)
    if ([String]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathRooted($Path)) {
        throw "EvidenceDirectory must be an absolute path to the exact fresh evidence directory."
    }
    $resolved = [IO.Path]::GetFullPath($Path)
    if (-not [IO.Directory]::Exists($resolved)) {
        throw "The exact evidence directory does not exist."
    }
    $current = [IO.DirectoryInfo]::new($resolved)
    while ($null -ne $current) {
        if (($current.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "The exact evidence directory path must not traverse a reparse point."
        }
        $current = $current.Parent
    }
    return $resolved.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
}

function Read-AtomicRequestMarker {
    param([string]$MarkerPath, [string]$OperatorDirectory)

    if (-not [IO.Directory]::Exists($OperatorDirectory)) {
        return $null
    }
    $operatorInfo = [IO.DirectoryInfo]::new($OperatorDirectory)
    if (($operatorInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "The operator-marker directory must not be a reparse point."
    }
    if (-not [IO.File]::Exists($MarkerPath)) {
        return $null
    }

    $stream = $null
    $bytes = $null
    try {
        $markerInfo = [IO.FileInfo]::new($MarkerPath)
        if (($markerInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "The foreground-arm request marker must be an ordinary file."
        }
        $stream = [IO.FileStream]::new(
            $MarkerPath,
            [IO.FileMode]::Open,
            [IO.FileAccess]::Read,
            [IO.FileShare]::Read
        )
        $length = $stream.Length
        if ($length -le 0 -or $length -gt $script:MaximumMarkerBytes) {
            throw "The foreground-arm request marker has an invalid size."
        }
        $bytes = [byte[]]::new([int]$length)
        $offset = 0
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -le 0) {
                throw "The foreground-arm request marker ended before its observed length."
            }
            $offset += $read
        }
        if ($stream.ReadByte() -ne -1 -or $stream.Length -ne $length) {
            throw "The foreground-arm request marker changed while it was read."
        }
        if ($bytes.Length -ge 3 -and
            $bytes[0] -eq 0xEF -and
            $bytes[1] -eq 0xBB -and
            $bytes[2] -eq 0xBF) {
            throw "The foreground-arm request marker must be UTF-8 without a BOM."
        }
        $json = $script:Utf8StrictNoBom.GetString($bytes)
        $marker = ConvertFrom-ExactMarkerJson $json
        $markerInfo.Refresh()
        if (-not $markerInfo.Exists -or $markerInfo.Length -ne $length) {
            throw "The foreground-arm request marker changed while it was read."
        }
        return [pscustomobject]([ordered]@{
            marker = $marker
            json = $json
            lastWriteTimeUtc = $markerInfo.LastWriteTimeUtc
        })
    }
    catch [IO.FileNotFoundException] {
        return $null
    }
    catch [IO.DirectoryNotFoundException] {
        return $null
    }
    finally {
        if ($null -ne $stream) {
            $stream.Dispose()
        }
        if ($null -ne $bytes) {
            [Array]::Clear($bytes, 0, $bytes.Length)
        }
    }
}

function Get-RunnerState {
    param([int]$ProcessId)
    $process = $null
    try {
        $process = Get-Process -Id $ProcessId -ErrorAction Stop
        if ($null -eq $process -or $process.HasExited) {
            return [pscustomobject]@{ alive = $false; startTimeUtc = $null }
        }
        return [pscustomobject]@{
            alive = $true
            startTimeUtc = $process.StartTime.ToUniversalTime()
        }
    }
    catch {
        return [pscustomobject]@{ alive = $false; startTimeUtc = $null }
    }
    finally {
        if ($null -ne $process) {
            $process.Dispose()
        }
    }
}

function Assert-BoundRunnerState {
    param([object]$RunnerState, [DateTimeOffset]$ExpectedStartTime)
    if ($null -eq $RunnerState -or $RunnerState.alive -ne $true) {
        throw "The bound Windows acceptance runner is not alive."
    }
    $actualStart = ConvertTo-UtcTimestamp $RunnerState.startTimeUtc "runner state"
    if ($actualStart.UtcDateTime.Ticks -ne $ExpectedStartTime.UtcDateTime.Ticks) {
        throw "The live runner PID does not match the exact expected start time."
    }
}

function Assert-FreshMarkerBinding {
    param(
        [object]$AtomicMarker,
        [DateTimeOffset]$ExpectedRunnerStart,
        [DateTimeOffset]$Now,
        [int]$MaximumAgeSeconds,
        [int]$AllowedFutureSeconds,
        [int]$AllowedFileTimestampDifferenceSeconds
    )
    $marker = $AtomicMarker.marker
    $published = ConvertFrom-CanonicalUtcTimestamp $marker.publishedAtUtc "marker publishedAtUtc"
    $lastWrite = ConvertTo-UtcTimestamp $AtomicMarker.lastWriteTimeUtc "marker file timestamp"
    if ($published.UtcDateTime.Ticks -lt $ExpectedRunnerStart.UtcDateTime.Ticks -or
        $lastWrite.UtcDateTime.Ticks -lt $ExpectedRunnerStart.UtcDateTime.Ticks) {
        throw "The marker predates the bound runner instance."
    }
    if ($published -gt $Now.AddSeconds($AllowedFutureSeconds)) {
        throw "The marker publication time is unacceptably far in the future."
    }
    if ($Now -ge $published.AddSeconds([int]$marker.timeoutSeconds)) {
        throw "The marker has expired."
    }
    if (($Now - $published).TotalSeconds -gt $MaximumAgeSeconds) {
        throw "The marker is stale for immediate operator handoff."
    }
    if ([Math]::Abs(($lastWrite - $published).TotalSeconds) -gt $AllowedFileTimestampDifferenceSeconds) {
        throw "The marker file timestamp does not match its publication time."
    }
    return $published
}

function New-SanitizedHandoff {
    param([object]$Marker, [DateTimeOffset]$Published, [DateTimeOffset]$Observed)
    return [pscustomobject]([ordered]@{
        schemaVersion = 1
        productVersion = $script:ProductVersion
        kind = "foreground-arm-visual-handoff"
        status = [string]$Marker.status
        requestId = [string]$Marker.requestId
        publishedAtUtc = ConvertTo-CanonicalUtcString $Published
        observedAtUtc = ConvertTo-CanonicalUtcString $Observed
        expiresAtUtc = ConvertTo-CanonicalUtcString ($Published.AddSeconds([int]$Marker.timeoutSeconds))
        operatorActionRequired = [bool]$Marker.operatorActionRequired
        preferredRelaySurface = [string]$Marker.preferredRelaySurface
        fallbackRelaySurface = [string]$Marker.fallbackRelaySurface
        expectedVisibleWindowTitle = [string]$Marker.expectedVisibleWindowTitle
        expectedVisibleButtonText = [string]$Marker.expectedVisibleButtonText
        expectedAccessibleName = [string]$Marker.expectedAccessibleName
        action = [string]$Marker.action
        stopUiAfterAction = $true
        requiresSeparateAuthorization = $true
        markerGrantsAuthorization = $false
        markerGrantsConsent = $false
        externalOneShotConsentRequired = $true
        externalAuthorizationVerifiedByWatcher = $false
        visualConfirmationRequired = $true
        maximumClickAttempts = [int]$Marker.maximumClickAttempts
        retryOnUnknownOutcome = $false
        instruction = [string]$Marker.instruction
        notificationOnly = $true
        acceptedAsAuthority = $false
        runnerIdentityMatched = $true
        markerFresh = $true
        rawWindowHandlesRecorded = $false
        rawCursorCoordinatesRecorded = $false
        processIdentifiersRecorded = $false
        pathsRecorded = $false
        secretsRecorded = $false
    })
}

function Wait-ForegroundArmHandoff {
    param(
        [int]$ExpectedRunnerProcessId,
        [DateTimeOffset]$ExpectedRunnerStart,
        [scriptblock]$RunnerStateReader,
        [scriptblock]$MarkerReader,
        [object[]]$MarkerReaderArguments = @(),
        [scriptblock]$ClockReader,
        [scriptblock]$Sleeper,
        [int]$TimeoutMilliseconds,
        [int]$MaximumAgeSeconds,
        [int]$AllowedFutureSeconds,
        [int]$AllowedFileTimestampDifferenceSeconds,
        [int]$PollIntervalMilliseconds
    )
    $timeoutWatch = [Diagnostics.Stopwatch]::StartNew()
    try {
        do {
            $beforeRunner = & $RunnerStateReader $ExpectedRunnerProcessId
            Assert-BoundRunnerState $beforeRunner $ExpectedRunnerStart
            $atomicMarker = & $MarkerReader @MarkerReaderArguments
            if ($null -ne $atomicMarker) {
                $now = ConvertTo-UtcTimestamp (& $ClockReader) "clock reader"
                $published = Assert-FreshMarkerBinding `
                    -AtomicMarker $atomicMarker `
                    -ExpectedRunnerStart $ExpectedRunnerStart `
                    -Now $now `
                    -MaximumAgeSeconds $MaximumAgeSeconds `
                    -AllowedFutureSeconds $AllowedFutureSeconds `
                    -AllowedFileTimestampDifferenceSeconds $AllowedFileTimestampDifferenceSeconds
                $afterRunner = & $RunnerStateReader $ExpectedRunnerProcessId
                Assert-BoundRunnerState $afterRunner $ExpectedRunnerStart
                return New-SanitizedHandoff $atomicMarker.marker $published $now
            }
            if ($PollIntervalMilliseconds -gt 0) {
                & $Sleeper $PollIntervalMilliseconds
            }
        } while ($timeoutWatch.ElapsedMilliseconds -lt $TimeoutMilliseconds)
    }
    finally {
        $timeoutWatch.Stop()
    }
    throw "Timed out waiting for the fresh foreground-arm request marker."
}

function New-SelfTestMarker {
    param([DateTimeOffset]$Published, [ValidateSet("action-required", "already-armed")][string]$Status, [int]$TimeoutSeconds)
    $actionRequired = $Status -ceq "action-required"
    $record = [ordered]@{
        schemaVersion = 2
        productVersion = "0.12.66"
        kind = "foreground-arm"
        status = $Status
        requestId = "0123456789abcdef0123456789abcdef"
        publishedAtUtc = ConvertTo-CanonicalUtcString $Published
        timeoutSeconds = $TimeoutSeconds
        operatorActionRequired = $actionRequired
        preferredRelaySurface = "windows-computer-use-app-share"
        fallbackRelaySurface = "human-on-windows-session"
        expectedVisibleWindowTitle = $script:ForegroundSentinelWindowTitle
        expectedVisibleButtonText = if ($actionRequired) { "CLICK TO ARM" } else { "ARMED - DO NOT USE THIS SESSION" }
        expectedAccessibleName = "Click to arm Windows acceptance"
        action = if ($actionRequired) { "single-left-click" } else { "none" }
        stopUiAfterAction = $true
        requiresSeparateAuthorization = $true
        markerGrantsAuthorization = $false
        markerGrantsConsent = $false
        externalOneShotConsentRequired = $true
        visualConfirmationRequired = $true
        maximumClickAttempts = if ($actionRequired) { 1 } else { 0 }
        retryOnUnknownOutcome = $false
        instruction = if ($actionRequired) { "Use a separately authorized Windows Computer Use app share to visually confirm this exact window and button, click it once, then stop all UI use. If it already says ARMED or the outcome is uncertain, do not click or retry." } else { "Do not click; stop all UI use because the foreground arm is already acknowledged." }
        requestDelivered = $true
        buttonEnabled = $true
        nativeTopologyMatched = $true
        inputStateAtPublication = if ($actionRequired) { "not-started" } else { "already-acknowledged" }
        notificationOnly = $true
        acceptedAsAuthority = $false
        rawWindowHandlesRecorded = $false
        rawCursorCoordinatesRecorded = $false
        pathsRecorded = $false
        secretsRecorded = $false
    }
    $json = $record | ConvertTo-Json -Depth 8
    return [pscustomobject]([ordered]@{
        marker = ConvertFrom-ExactMarkerJson $json
        json = $json
        lastWriteTimeUtc = $Published.UtcDateTime
    })
}

function Assert-SelfTestFailure {
    param([scriptblock]$Operation, [string]$ExpectedText, [string]$Label)
    $failure = $null
    try {
        $null = & $Operation
    }
    catch {
        $failure = $_.Exception.Message
    }
    if ([String]::IsNullOrWhiteSpace($failure) -or
        $failure.IndexOf($ExpectedText, [StringComparison]::Ordinal) -lt 0) {
        throw "$Label did not fail closed with the expected error."
    }
}

function Invoke-SelfTest {
    $now = [DateTimeOffset]::ParseExact(
        "2026-08-23T12:00:00.0000000Z",
        "o",
        [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::RoundtripKind
    )
    $runnerStart = $now.AddSeconds(-120)
    $runnerState = [pscustomobject]@{ alive = $true; startTimeUtc = $runnerStart.UtcDateTime }
    $runnerReader = { param($ignoredProcessId) return $runnerState }.GetNewClosure()
    $clockReader = { return $now }.GetNewClosure()
    $noSleep = { param($ignoredMilliseconds) }.GetNewClosure()

    $validAtomic = New-SelfTestMarker $now.AddSeconds(-1) "action-required" 300
    $validCounter = [pscustomobject]@{ reads = 0 }
    $validReader = {
        $validCounter.reads += 1
        return $validAtomic
    }.GetNewClosure()
    $validEmissions = @(
        Wait-ForegroundArmHandoff `
            -ExpectedRunnerProcessId 17 `
            -ExpectedRunnerStart $runnerStart `
            -RunnerStateReader $runnerReader `
            -MarkerReader $validReader `
            -ClockReader $clockReader `
            -Sleeper $noSleep `
            -TimeoutMilliseconds 1000 `
            -MaximumAgeSeconds 30 `
            -AllowedFutureSeconds 2 `
            -AllowedFileTimestampDifferenceSeconds 2 `
            -PollIntervalMilliseconds 0
    )
    Assert-Condition ($validEmissions.Count -eq 1) "The watcher core emitted more than one handoff."
    $validResult = $validEmissions[0]
    Assert-Condition ($validResult.status -ceq "action-required" -and
        $validResult.action -ceq "single-left-click" -and
        $validResult.maximumClickAttempts -eq 1 -and
        $script:ForegroundSentinelWindowTitle -ceq "LBB Foreground Sentinel" -and
        $validResult.expectedVisibleWindowTitle -ceq $script:ForegroundSentinelWindowTitle -and
        $validResult.externalAuthorizationVerifiedByWatcher -eq $false -and
        $validCounter.reads -eq 1) "The valid action-required handoff failed its self-test."
    $singleEmission = @($validResult | ConvertTo-Json -Depth 8 -Compress)
    Assert-Condition ($singleEmission.Count -eq 1 -and
        $singleEmission[0].IndexOf("`n", [StringComparison]::Ordinal) -lt 0 -and
        $singleEmission[0].IndexOf("`r", [StringComparison]::Ordinal) -lt 0) "The watcher did not produce one sanitized JSON emission."

    # Exercise the exact live callback shape without writing any evidence.
    # GetNewClosure() is deliberately absent: under Windows PowerShell 5.1 it
    # creates a dynamic module that cannot resolve this script's reader
    # function. Explicit callback arguments preserve script-scope function
    # resolution without moving either bound path into global state.
    $liveStyleRoot = [IO.Path]::Combine(
        [IO.Path]::GetTempPath(),
        "lbb-foreground-arm-watcher-self-test-$([Guid]::NewGuid().ToString('N'))"
    )
    $liveStyleOperatorDirectory = [IO.Path]::Combine($liveStyleRoot, "operator")
    $liveStyleMarkerPath = [IO.Path]::Combine($liveStyleOperatorDirectory, "foreground-arm-request.json")
    Assert-Condition (-not [IO.Directory]::Exists($liveStyleRoot)) "The live-style self-test path unexpectedly exists."
    $liveStyleMarkerReader = {
        param([string]$boundMarkerPath, [string]$boundOperatorDirectory)
        return Read-AtomicRequestMarker $boundMarkerPath $boundOperatorDirectory
    }
    $liveStyleMissingMarker = & $liveStyleMarkerReader $liveStyleMarkerPath $liveStyleOperatorDirectory
    Assert-Condition ($null -eq $liveStyleMissingMarker -and
        -not [IO.Directory]::Exists($liveStyleRoot)) "The zero-write live-style marker callback failed its self-test."

    $alreadyAtomic = New-SelfTestMarker $now.AddSeconds(-1) "already-armed" 300
    $alreadyResult = Wait-ForegroundArmHandoff `
        -ExpectedRunnerProcessId 17 `
        -ExpectedRunnerStart $runnerStart `
        -RunnerStateReader $runnerReader `
        -MarkerReader ({ return $alreadyAtomic }.GetNewClosure()) `
        -ClockReader $clockReader `
        -Sleeper $noSleep `
        -TimeoutMilliseconds 1000 `
        -MaximumAgeSeconds 30 `
        -AllowedFutureSeconds 2 `
        -AllowedFileTimestampDifferenceSeconds 2 `
        -PollIntervalMilliseconds 0
    Assert-Condition ($alreadyResult.status -ceq "already-armed" -and
        $alreadyResult.operatorActionRequired -eq $false -and
        $alreadyResult.action -ceq "none" -and
        $alreadyResult.expectedVisibleWindowTitle -ceq $script:ForegroundSentinelWindowTitle -and
        $alreadyResult.expectedVisibleWindowTitle -ceq $validResult.expectedVisibleWindowTitle -and
        $alreadyResult.maximumClickAttempts -eq 0) "The already-armed handoff failed its self-test."

    $unstableTitleAtomic = New-SelfTestMarker $now.AddSeconds(-1) "action-required" 300
    $unstableTitleAtomic.marker.expectedVisibleWindowTitle = "LBB Windows Acceptance - ACTION REQUIRED"
    $unstableTitleJson = $unstableTitleAtomic.marker | ConvertTo-Json -Depth 8
    Assert-SelfTestFailure `
        -Operation { $null = ConvertFrom-ExactMarkerJson $unstableTitleJson } `
        -ExpectedText "stable foreground-sentinel window title is invalid" `
        -Label "state-mutating window title"

    Assert-SelfTestFailure `
        -Operation { $null = ConvertFrom-ExactMarkerJson '{not-json' } `
        -ExpectedText "not one complete JSON object" `
        -Label "malformed marker"

    # Keep these operation blocks in the script scope. PowerShell 7 runs a
    # GetNewClosure() block in a dynamic module that cannot resolve the
    # watcher functions when this script is invoked with the call operator.
    $staleAtomic = New-SelfTestMarker $now.AddSeconds(-40) "action-required" 300
    Assert-SelfTestFailure `
        -Operation {
            $null = Assert-FreshMarkerBinding $staleAtomic $runnerStart $now 30 2 2
        } `
        -ExpectedText "stale for immediate operator handoff" `
        -Label "stale marker"

    $futureAtomic = New-SelfTestMarker $now.AddSeconds(10) "action-required" 300
    Assert-SelfTestFailure `
        -Operation {
            $null = Assert-FreshMarkerBinding $futureAtomic $runnerStart $now 30 2 2
        } `
        -ExpectedText "unacceptably far in the future" `
        -Label "future marker"

    $expiredAtomic = New-SelfTestMarker $now.AddSeconds(-20) "action-required" 15
    Assert-SelfTestFailure `
        -Operation {
            $null = Assert-FreshMarkerBinding $expiredAtomic $runnerStart $now 60 2 2
        } `
        -ExpectedText "marker has expired" `
        -Label "expired marker"

    $deadRunnerReader = { param($ignoredProcessId) return [pscustomobject]@{ alive = $false; startTimeUtc = $null } }
    Assert-SelfTestFailure `
        -Operation {
            $null = Wait-ForegroundArmHandoff `
                -ExpectedRunnerProcessId 17 `
                -ExpectedRunnerStart $runnerStart `
                -RunnerStateReader $deadRunnerReader `
                -MarkerReader ({ return $validAtomic }.GetNewClosure()) `
                -ClockReader $clockReader `
                -Sleeper $noSleep `
                -TimeoutMilliseconds 1000 `
                -MaximumAgeSeconds 30 `
                -AllowedFutureSeconds 2 `
                -AllowedFileTimestampDifferenceSeconds 2 `
                -PollIntervalMilliseconds 0
        } `
        -ExpectedText "runner is not alive" `
        -Label "dead runner"

    $wrongStartReader = {
        param($ignoredProcessId)
        return [pscustomobject]@{ alive = $true; startTimeUtc = $runnerStart.AddTicks(1).UtcDateTime }
    }.GetNewClosure()
    Assert-SelfTestFailure `
        -Operation { $null = Assert-BoundRunnerState (& $wrongStartReader 17) $runnerStart } `
        -ExpectedText "does not match the exact expected start time" `
        -Label "runner start-time mismatch"

    Write-Output "Windows foreground-arm handoff watcher self-test passed."
}

if ($Mode -ceq "SelfTest") {
    Invoke-SelfTest
    return
}

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "Watch mode can run only on Windows."
}
if ($RunnerProcessId -le 0) {
    throw "RunnerProcessId must identify the live Windows acceptance runner."
}
$resolvedEvidenceDirectory = Resolve-OrdinaryEvidenceDirectory $EvidenceDirectory
$expectedRunnerStart = ConvertFrom-CanonicalUtcTimestamp $RunnerStartedAtUtc "RunnerStartedAtUtc"
$operatorDirectory = [IO.Path]::Combine($resolvedEvidenceDirectory, "operator")
$markerPath = [IO.Path]::Combine($operatorDirectory, "foreground-arm-request.json")
$runnerReader = { param($processId) return Get-RunnerState $processId }
$markerReader = {
    param([string]$boundMarkerPath, [string]$boundOperatorDirectory)
    return Read-AtomicRequestMarker $boundMarkerPath $boundOperatorDirectory
}
$clockReader = { return [DateTimeOffset]::UtcNow }
$sleeper = { param($milliseconds) Start-Sleep -Milliseconds $milliseconds }

$handoff = Wait-ForegroundArmHandoff `
    -ExpectedRunnerProcessId $RunnerProcessId `
    -ExpectedRunnerStart $expectedRunnerStart `
    -RunnerStateReader $runnerReader `
    -MarkerReader $markerReader `
    -MarkerReaderArguments @($markerPath, $operatorDirectory) `
    -ClockReader $clockReader `
    -Sleeper $sleeper `
    -TimeoutMilliseconds ($WaitTimeoutSeconds * 1000) `
    -MaximumAgeSeconds $MaximumMarkerAgeSeconds `
    -AllowedFutureSeconds $FutureToleranceSeconds `
    -AllowedFileTimestampDifferenceSeconds $FileTimestampToleranceSeconds `
    -PollIntervalMilliseconds $PollMilliseconds

Write-Output ($handoff | ConvertTo-Json -Depth 8 -Compress)
