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

    [ValidateRange(0, 30)]
    [int]$FutureToleranceSeconds = 5,

    [ValidateRange(25, 1000)]
    [int]$PollMilliseconds = 100
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$script:ProductVersion = "0.12.68"
$script:ForegroundGateMode = "automatic-stable-external-foreground"
$script:MarkerSchemaVersion = 3
$script:HandoffSchemaVersion = 2
$script:MaximumMarkerBytes = 16384
$script:Utf8StrictNoBom = [Text.UTF8Encoding]::new($false, $true)
$script:RequestFields = @(
    "schemaVersion",
    "productVersion",
    "kind",
    "status",
    "requestId",
    "publishedAtUtc",
    "timeoutSeconds",
    "mode",
    "operatorActionRequired",
    "action",
    "globalInputUsed",
    "focusChangedByRunner",
    "cursorChangedByRunner",
    "syntheticInputUsed",
    "requiredStableSamples",
    "notificationOnly",
    "acceptedAsAuthority",
    "rawWindowHandlesRecorded",
    "rawProcessIdentifiersRecorded",
    "rawCursorCoordinatesRecorded",
    "pathsRecorded",
    "secretsRecorded"
)
$script:ReceivedFields = @(
    "schemaVersion",
    "productVersion",
    "kind",
    "status",
    "requestId",
    "receivedAtUtc",
    "mode",
    "operatorActionRequired",
    "action",
    "clickAttemptsObserved",
    "stableSamplesObserved",
    "stableSamplesRequired",
    "nativeSampleSeqlockMatched",
    "ownerIdentityStable",
    "focusRootMatched",
    "fixtureProcessExcluded",
    "interactiveSessionMatched",
    "cursorStable",
    "inputDesktopStable",
    "globalInputUsed",
    "focusChangedByRunner",
    "cursorChangedByRunner",
    "syntheticInputUsed",
    "notificationOnly",
    "acceptedAsAuthority",
    "rawWindowHandlesRecorded",
    "rawProcessIdentifiersRecorded",
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

function Assert-ExactMarkerFieldOccurrences {
    param([string]$Json, [string[]]$Fields, [string]$Label)
    foreach ($field in $Fields) {
        $propertyPattern = '"' + [regex]::Escape($field) + '"\s*:'
        if ([regex]::Matches($Json, $propertyPattern).Count -ne 1) {
            throw "$Label must contain each canonical field exactly once."
        }
    }
}

function Assert-CommonAutomaticMarkerSemantics {
    param([object]$Marker, [string]$Label)
    Assert-ExactIntegerRange $Marker.schemaVersion $script:MarkerSchemaVersion $script:MarkerSchemaVersion "$Label schemaVersion"
    if ($Marker.productVersion -isnot [string] -or $Marker.productVersion -cne $script:ProductVersion) {
        throw "$Label productVersion must be $($script:ProductVersion)."
    }
    if ($Marker.kind -isnot [string] -or $Marker.kind -cne "foreground-baseline") {
        throw "$Label kind is invalid."
    }
    if ($Marker.requestId -isnot [string] -or $Marker.requestId -cnotmatch '^[0-9a-f]{32}$') {
        throw "$Label requestId is invalid."
    }
    if ($Marker.mode -isnot [string] -or $Marker.mode -cne $script:ForegroundGateMode) {
        throw "$Label mode is invalid."
    }
    Assert-ExactBoolean $Marker.operatorActionRequired $false "$Label operatorActionRequired"
    if ($Marker.action -isnot [string] -or $Marker.action -cne "none") {
        throw "$Label action must be none."
    }
    Assert-ExactBoolean $Marker.globalInputUsed $false "$Label globalInputUsed"
    Assert-ExactBoolean $Marker.focusChangedByRunner $false "$Label focusChangedByRunner"
    Assert-ExactBoolean $Marker.cursorChangedByRunner $false "$Label cursorChangedByRunner"
    Assert-ExactBoolean $Marker.syntheticInputUsed $false "$Label syntheticInputUsed"
    Assert-ExactBoolean $Marker.notificationOnly $true "$Label notificationOnly"
    Assert-ExactBoolean $Marker.acceptedAsAuthority $false "$Label acceptedAsAuthority"
    Assert-ExactBoolean $Marker.rawWindowHandlesRecorded $false "$Label rawWindowHandlesRecorded"
    Assert-ExactBoolean $Marker.rawProcessIdentifiersRecorded $false "$Label rawProcessIdentifiersRecorded"
    Assert-ExactBoolean $Marker.rawCursorCoordinatesRecorded $false "$Label rawCursorCoordinatesRecorded"
    Assert-ExactBoolean $Marker.pathsRecorded $false "$Label pathsRecorded"
    Assert-ExactBoolean $Marker.secretsRecorded $false "$Label secretsRecorded"
}

function Assert-ExactRequestMarkerSchema {
    param([object]$Marker, [string]$Json)
    $label = "foreground-baseline request marker"
    Assert-ExactPropertyOrder $Marker $script:RequestFields $label
    Assert-ExactMarkerFieldOccurrences $Json $script:RequestFields $label
    Assert-CommonAutomaticMarkerSemantics $Marker $label
    if ($Marker.status -isnot [string] -or $Marker.status -cne "automatic") {
        throw "The foreground-baseline request marker status is invalid."
    }
    $null = ConvertFrom-CanonicalUtcTimestamp $Marker.publishedAtUtc "request marker publishedAtUtc"
    Assert-ExactIntegerRange $Marker.timeoutSeconds 15 300 "request marker timeoutSeconds"
    Assert-ExactIntegerRange $Marker.requiredStableSamples 3 3 "request marker requiredStableSamples"
}

function Assert-ExactReceivedMarkerSchema {
    param([object]$Marker, [string]$Json)
    $label = "foreground-baseline received marker"
    Assert-ExactPropertyOrder $Marker $script:ReceivedFields $label
    Assert-ExactMarkerFieldOccurrences $Json $script:ReceivedFields $label
    Assert-CommonAutomaticMarkerSemantics $Marker $label
    if ($Marker.status -isnot [string] -or $Marker.status -cne "ready") {
        throw "The foreground-baseline received marker status is invalid."
    }
    $null = ConvertFrom-CanonicalUtcTimestamp $Marker.receivedAtUtc "received marker receivedAtUtc"
    Assert-ExactIntegerRange $Marker.clickAttemptsObserved 0 0 "received marker clickAttemptsObserved"
    Assert-ExactIntegerRange $Marker.stableSamplesRequired 3 3 "received marker stableSamplesRequired"
    Assert-ExactIntegerRange $Marker.stableSamplesObserved 3 3 "received marker stableSamplesObserved"
    foreach ($field in @(
        "nativeSampleSeqlockMatched", "ownerIdentityStable", "focusRootMatched",
        "fixtureProcessExcluded", "interactiveSessionMatched", "cursorStable", "inputDesktopStable"
    )) {
        Assert-ExactBoolean $Marker.$field $true "received marker $field"
    }
}

function ConvertFrom-ExactMarkerJson {
    param([string]$Json, [ValidateSet("Request", "Received")][string]$MarkerType)
    $label = if ($MarkerType -ceq "Request") {
        "foreground-baseline request marker"
    }
    else { "foreground-baseline received marker" }
    if ([String]::IsNullOrWhiteSpace($Json) -or $Json.Length -gt $script:MaximumMarkerBytes) {
        throw "The $label has an invalid size."
    }
    if ($Json.IndexOf([char]0) -ge 0) {
        throw "The $label contains a NUL character."
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
        throw "The $label is not one complete JSON object."
    }
    if ($MarkerType -ceq "Request") {
        Assert-ExactRequestMarkerSchema $marker $Json
    }
    else {
        Assert-ExactReceivedMarkerSchema $marker $Json
    }
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

function Read-AtomicForegroundMarker {
    param(
        [string]$MarkerPath,
        [string]$OperatorDirectory,
        [ValidateSet("Request", "Received")][string]$MarkerType
    )

    $label = if ($MarkerType -ceq "Request") {
        "foreground-baseline request marker"
    }
    else { "foreground-baseline received marker" }

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
            throw "The $label must be an ordinary file."
        }
        $stream = [IO.FileStream]::new(
            $MarkerPath,
            [IO.FileMode]::Open,
            [IO.FileAccess]::Read,
            [IO.FileShare]::Read
        )
        $length = $stream.Length
        if ($length -le 0 -or $length -gt $script:MaximumMarkerBytes) {
            throw "The $label has an invalid size."
        }
        $bytes = [byte[]]::new([int]$length)
        $offset = 0
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -le 0) {
                throw "The $label ended before its observed length."
            }
            $offset += $read
        }
        if ($stream.ReadByte() -ne -1 -or $stream.Length -ne $length) {
            throw "The $label changed while it was read."
        }
        if ($bytes.Length -ge 3 -and
            $bytes[0] -eq 0xEF -and
            $bytes[1] -eq 0xBB -and
            $bytes[2] -eq 0xBF) {
            throw "The $label must be UTF-8 without a BOM."
        }
        $json = $script:Utf8StrictNoBom.GetString($bytes)
        $marker = ConvertFrom-ExactMarkerJson $json $MarkerType
        $markerInfo.Refresh()
        if (-not $markerInfo.Exists -or $markerInfo.Length -ne $length) {
            throw "The $label changed while it was read."
        }
        return [pscustomobject]([ordered]@{
            marker = $marker
            json = $json
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

function Assert-FreshRequestBinding {
    param(
        [object]$RequestMarker,
        [DateTimeOffset]$ExpectedRunnerStart,
        [DateTimeOffset]$Now,
        [int]$AllowedFutureSeconds
    )
    $published = ConvertFrom-CanonicalUtcTimestamp $RequestMarker.publishedAtUtc "request marker publishedAtUtc"
    if ($published.UtcDateTime.Ticks -lt $ExpectedRunnerStart.UtcDateTime.Ticks) {
        throw "The request marker predates the bound runner instance."
    }
    if ($published -gt $Now.AddSeconds($AllowedFutureSeconds)) {
        throw "The request marker publication time is unacceptably far in the future."
    }
    $deadline = $published.AddSeconds([int]$RequestMarker.timeoutSeconds)
    if ($Now -ge $deadline) {
        throw "The automatic foreground-baseline request has expired."
    }
    return [pscustomobject]([ordered]@{
        published = $published
        deadline = $deadline
    })
}

function Assert-ReceivedMarkerBinding {
    param(
        [object]$RequestMarker,
        [object]$ReceivedMarker,
        [DateTimeOffset]$Published,
        [DateTimeOffset]$Deadline,
        [DateTimeOffset]$Observed,
        [int]$AllowedFutureSeconds
    )
    if ([string]$ReceivedMarker.requestId -cne [string]$RequestMarker.requestId) {
        throw "The received marker does not match the automatic foreground-baseline request ID."
    }
    if ([string]$ReceivedMarker.mode -cne [string]$RequestMarker.mode -or
        [int]$ReceivedMarker.stableSamplesRequired -ne [int]$RequestMarker.requiredStableSamples) {
        throw "The received marker does not match the automatic foreground-baseline request semantics."
    }
    $received = ConvertFrom-CanonicalUtcTimestamp $ReceivedMarker.receivedAtUtc "received marker receivedAtUtc"
    if ($received -lt $Published) {
        throw "The received marker predates its matching request."
    }
    if ($received -gt $Deadline) {
        throw "The received marker was published after the automatic foreground-baseline deadline."
    }
    if ($received -gt $Observed.AddSeconds($AllowedFutureSeconds)) {
        throw "The received marker time is unacceptably far in the future."
    }
    return $received
}

function New-SanitizedHandoff {
    param(
        [object]$RequestMarker,
        [object]$ReceivedMarker,
        [DateTimeOffset]$Published,
        [DateTimeOffset]$Received,
        [DateTimeOffset]$Observed,
        [DateTimeOffset]$Deadline
    )
    return [pscustomobject]([ordered]@{
        schemaVersion = $script:HandoffSchemaVersion
        productVersion = $script:ProductVersion
        kind = "foreground-baseline-ready-handoff"
        status = "automatic-ready"
        requestId = [string]$RequestMarker.requestId
        publishedAtUtc = ConvertTo-CanonicalUtcString $Published
        receivedAtUtc = ConvertTo-CanonicalUtcString $Received
        observedAtUtc = ConvertTo-CanonicalUtcString $Observed
        deadlineAtUtc = ConvertTo-CanonicalUtcString $Deadline
        mode = $script:ForegroundGateMode
        operatorActionRequired = $false
        action = "none"
        clickAttemptsObserved = [int]$ReceivedMarker.clickAttemptsObserved
        stableSamplesObserved = [int]$ReceivedMarker.stableSamplesObserved
        stableSamplesRequired = [int]$ReceivedMarker.stableSamplesRequired
        nativeSampleSeqlockMatched = [bool]$ReceivedMarker.nativeSampleSeqlockMatched
        ownerIdentityStable = [bool]$ReceivedMarker.ownerIdentityStable
        focusRootMatched = [bool]$ReceivedMarker.focusRootMatched
        fixtureProcessExcluded = [bool]$ReceivedMarker.fixtureProcessExcluded
        interactiveSessionMatched = [bool]$ReceivedMarker.interactiveSessionMatched
        cursorStable = [bool]$ReceivedMarker.cursorStable
        inputDesktopStable = [bool]$ReceivedMarker.inputDesktopStable
        globalInputUsed = $false
        focusChangedByRunner = $false
        cursorChangedByRunner = $false
        syntheticInputUsed = $false
        notificationOnly = $true
        acceptedAsAuthority = $false
        runnerIdentityMatched = $true
        requestFresh = $true
        receivedBeforeDeadline = $true
        rawWindowHandlesRecorded = $false
        rawProcessIdentifiersRecorded = $false
        rawCursorCoordinatesRecorded = $false
        pathsRecorded = $false
        secretsRecorded = $false
    })
}

function Wait-ForegroundBaselineHandoff {
    param(
        [int]$ExpectedRunnerProcessId,
        [DateTimeOffset]$ExpectedRunnerStart,
        [scriptblock]$RunnerStateReader,
        [scriptblock]$RequestReader,
        [object[]]$RequestReaderArguments = @(),
        [scriptblock]$ReceivedReader,
        [object[]]$ReceivedReaderArguments = @(),
        [scriptblock]$ClockReader,
        [scriptblock]$Sleeper,
        [int]$TimeoutMilliseconds,
        [int]$AllowedFutureSeconds,
        [int]$PollIntervalMilliseconds
    )
    $timeoutWatch = [Diagnostics.Stopwatch]::StartNew()
    $requestMarker = $null
    $published = [DateTimeOffset]::MinValue
    $deadline = [DateTimeOffset]::MinValue
    try {
        do {
            $beforeRunner = & $RunnerStateReader $ExpectedRunnerProcessId
            Assert-BoundRunnerState $beforeRunner $ExpectedRunnerStart
            if ($null -eq $requestMarker) {
                $atomicRequest = & $RequestReader @RequestReaderArguments
                if ($null -ne $atomicRequest) {
                    $requestMarker = $atomicRequest.marker
                    $now = ConvertTo-UtcTimestamp (& $ClockReader) "clock reader"
                    $requestBinding = Assert-FreshRequestBinding `
                        -RequestMarker $requestMarker `
                        -ExpectedRunnerStart $ExpectedRunnerStart `
                        -Now $now `
                        -AllowedFutureSeconds $AllowedFutureSeconds
                    $published = $requestBinding.published
                    $deadline = $requestBinding.deadline
                }
            }
            if ($null -ne $requestMarker) {
                $now = ConvertTo-UtcTimestamp (& $ClockReader) "clock reader"
                $atomicReceived = & $ReceivedReader @ReceivedReaderArguments
                if ($null -ne $atomicReceived) {
                    $received = Assert-ReceivedMarkerBinding `
                        -RequestMarker $requestMarker `
                        -ReceivedMarker $atomicReceived.marker `
                        -Published $published `
                        -Deadline $deadline `
                        -Observed $now `
                        -AllowedFutureSeconds $AllowedFutureSeconds
                    $afterRunner = & $RunnerStateReader $ExpectedRunnerProcessId
                    Assert-BoundRunnerState $afterRunner $ExpectedRunnerStart
                    return New-SanitizedHandoff `
                        -RequestMarker $requestMarker `
                        -ReceivedMarker $atomicReceived.marker `
                        -Published $published `
                        -Received $received `
                        -Observed $now `
                        -Deadline $deadline
                }
                if ($now -ge $deadline) {
                    throw "The automatic foreground-baseline request expired before a matching ready marker arrived."
                }
            }
            if ($PollIntervalMilliseconds -gt 0) {
                & $Sleeper $PollIntervalMilliseconds
            }
        } while ($timeoutWatch.ElapsedMilliseconds -lt $TimeoutMilliseconds)
    }
    finally {
        $timeoutWatch.Stop()
    }
    throw "Timed out waiting for matching automatic foreground-baseline request and ready markers."
}

function New-SelfTestRequestMarker {
    param([DateTimeOffset]$Published, [int]$TimeoutSeconds)
    $record = [ordered]@{
        schemaVersion = 3
        productVersion = "0.12.68"
        kind = "foreground-baseline"
        status = "automatic"
        requestId = "0123456789abcdef0123456789abcdef"
        publishedAtUtc = ConvertTo-CanonicalUtcString $Published
        timeoutSeconds = $TimeoutSeconds
        mode = $script:ForegroundGateMode
        operatorActionRequired = $false
        action = "none"
        globalInputUsed = $false
        focusChangedByRunner = $false
        cursorChangedByRunner = $false
        syntheticInputUsed = $false
        requiredStableSamples = 3
        notificationOnly = $true
        acceptedAsAuthority = $false
        rawWindowHandlesRecorded = $false
        rawProcessIdentifiersRecorded = $false
        rawCursorCoordinatesRecorded = $false
        pathsRecorded = $false
        secretsRecorded = $false
    }
    $json = $record | ConvertTo-Json -Depth 8
    return [pscustomobject]([ordered]@{
        marker = ConvertFrom-ExactMarkerJson $json "Request"
        json = $json
    })
}

function New-SelfTestReceivedMarker {
    param(
        [DateTimeOffset]$Received,
        [string]$RequestId = "0123456789abcdef0123456789abcdef"
    )
    $record = [ordered]@{
        schemaVersion = 3
        productVersion = "0.12.68"
        kind = "foreground-baseline"
        status = "ready"
        requestId = $RequestId
        receivedAtUtc = ConvertTo-CanonicalUtcString $Received
        mode = $script:ForegroundGateMode
        operatorActionRequired = $false
        action = "none"
        clickAttemptsObserved = 0
        stableSamplesObserved = 3
        stableSamplesRequired = 3
        nativeSampleSeqlockMatched = $true
        ownerIdentityStable = $true
        focusRootMatched = $true
        fixtureProcessExcluded = $true
        interactiveSessionMatched = $true
        cursorStable = $true
        inputDesktopStable = $true
        globalInputUsed = $false
        focusChangedByRunner = $false
        cursorChangedByRunner = $false
        syntheticInputUsed = $false
        notificationOnly = $true
        acceptedAsAuthority = $false
        rawWindowHandlesRecorded = $false
        rawProcessIdentifiersRecorded = $false
        rawCursorCoordinatesRecorded = $false
        pathsRecorded = $false
        secretsRecorded = $false
    }
    $json = $record | ConvertTo-Json -Depth 8
    return [pscustomobject]([ordered]@{
        marker = ConvertFrom-ExactMarkerJson $json "Received"
        json = $json
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

    $validRequest = New-SelfTestRequestMarker $now.AddSeconds(-40) 300
    $validReceived = New-SelfTestReceivedMarker $now.AddSeconds(-1)
    $validCounters = [pscustomobject]@{ requestReads = 0; receivedReads = 0 }
    $validRequestReader = {
        $validCounters.requestReads += 1
        return $validRequest
    }.GetNewClosure()
    $validReceivedReader = {
        $validCounters.receivedReads += 1
        return $validReceived
    }.GetNewClosure()
    $validEmissions = @(
        Wait-ForegroundBaselineHandoff `
            -ExpectedRunnerProcessId 17 `
            -ExpectedRunnerStart $runnerStart `
            -RunnerStateReader $runnerReader `
            -RequestReader $validRequestReader `
            -ReceivedReader $validReceivedReader `
            -ClockReader $clockReader `
            -Sleeper $noSleep `
            -TimeoutMilliseconds 1000 `
            -AllowedFutureSeconds 2 `
            -PollIntervalMilliseconds 0
    )
    Assert-Condition ($validEmissions.Count -eq 1) "The watcher core emitted more than one handoff."
    $validResult = $validEmissions[0]
    Assert-Condition ($validResult.schemaVersion -eq $script:HandoffSchemaVersion -and
        $validResult.status -ceq "automatic-ready" -and
        $validResult.mode -ceq $script:ForegroundGateMode -and
        $validResult.operatorActionRequired -eq $false -and
        $validResult.action -ceq "none" -and
        $validResult.clickAttemptsObserved -eq 0 -and
        $validResult.stableSamplesObserved -eq 3 -and
        $validResult.nativeSampleSeqlockMatched -eq $true -and
        $validResult.ownerIdentityStable -eq $true -and
        $validResult.focusRootMatched -eq $true -and
        $validResult.fixtureProcessExcluded -eq $true -and
        $validResult.interactiveSessionMatched -eq $true -and
        $validResult.cursorStable -eq $true -and
        $validResult.inputDesktopStable -eq $true -and
        $validResult.globalInputUsed -eq $false -and
        $validResult.focusChangedByRunner -eq $false -and
        $validResult.cursorChangedByRunner -eq $false -and
        $validResult.syntheticInputUsed -eq $false -and
        $validCounters.requestReads -eq 1 -and
        $validCounters.receivedReads -eq 1) "The valid automatic foreground-baseline handoff failed its self-test."
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
        "lbb-foreground-baseline-watcher-self-test-$([Guid]::NewGuid().ToString('N'))"
    )
    $liveStyleOperatorDirectory = [IO.Path]::Combine($liveStyleRoot, "operator")
    $liveStyleRequestPath = [IO.Path]::Combine($liveStyleOperatorDirectory, "foreground-arm-request.json")
    $liveStyleReceivedPath = [IO.Path]::Combine($liveStyleOperatorDirectory, "foreground-arm-received.json")
    Assert-Condition (-not [IO.Directory]::Exists($liveStyleRoot)) "The live-style self-test path unexpectedly exists."
    $liveStyleMarkerReader = {
        param([string]$boundMarkerPath, [string]$boundOperatorDirectory, [string]$boundMarkerType)
        return Read-AtomicForegroundMarker $boundMarkerPath $boundOperatorDirectory $boundMarkerType
    }
    $liveStyleMissingRequest = & $liveStyleMarkerReader $liveStyleRequestPath $liveStyleOperatorDirectory "Request"
    $liveStyleMissingReceived = & $liveStyleMarkerReader $liveStyleReceivedPath $liveStyleOperatorDirectory "Received"
    Assert-Condition ($null -eq $liveStyleMissingRequest -and
        $null -eq $liveStyleMissingReceived -and
        -not [IO.Directory]::Exists($liveStyleRoot)) "The zero-write live-style marker callback failed its self-test."

    $manualRequest = New-SelfTestRequestMarker $now.AddSeconds(-1) 300
    $manualRequest.marker.operatorActionRequired = $true
    $manualRequestJson = $manualRequest.marker | ConvertTo-Json -Depth 8
    Assert-SelfTestFailure `
        -Operation { $null = ConvertFrom-ExactMarkerJson $manualRequestJson "Request" } `
        -ExpectedText "operatorActionRequired must be False" `
        -Label "manual-action request"

    $inputReceived = New-SelfTestReceivedMarker $now.AddSeconds(-1)
    $inputReceived.marker.clickAttemptsObserved = 1
    $inputReceivedJson = $inputReceived.marker | ConvertTo-Json -Depth 8
    Assert-SelfTestFailure `
        -Operation { $null = ConvertFrom-ExactMarkerJson $inputReceivedJson "Received" } `
        -ExpectedText "clickAttemptsObserved must be an integer from 0 through 0" `
        -Label "received input attempt"

    $extraSampleReceived = New-SelfTestReceivedMarker $now.AddSeconds(-1)
    $extraSampleReceived.marker.stableSamplesObserved = 4
    $extraSampleReceivedJson = $extraSampleReceived.marker | ConvertTo-Json -Depth 8
    Assert-SelfTestFailure `
        -Operation { $null = ConvertFrom-ExactMarkerJson $extraSampleReceivedJson "Received" } `
        -ExpectedText "stableSamplesObserved must be an integer from 3 through 3" `
        -Label "producer-unreachable extra stable sample"

    $wrongSessionReceived = New-SelfTestReceivedMarker $now.AddSeconds(-1)
    $wrongSessionReceived.marker.interactiveSessionMatched = $false
    $wrongSessionReceivedJson = $wrongSessionReceived.marker | ConvertTo-Json -Depth 8
    Assert-SelfTestFailure `
        -Operation { $null = ConvertFrom-ExactMarkerJson $wrongSessionReceivedJson "Received" } `
        -ExpectedText "interactiveSessionMatched must be True" `
        -Label "wrong interactive session"

    $unstableReceived = New-SelfTestReceivedMarker $now.AddSeconds(-1)
    $unstableReceived.marker.ownerIdentityStable = $false
    $unstableReceivedJson = $unstableReceived.marker | ConvertTo-Json -Depth 8
    Assert-SelfTestFailure `
        -Operation { $null = ConvertFrom-ExactMarkerJson $unstableReceivedJson "Received" } `
        -ExpectedText "ownerIdentityStable must be True" `
        -Label "unstable owner identity"

    Assert-SelfTestFailure `
        -Operation { $null = ConvertFrom-ExactMarkerJson '{not-json' "Request" } `
        -ExpectedText "not one complete JSON object" `
        -Label "malformed marker"

    # Keep these operation blocks in the script scope. PowerShell 7 runs a
    # GetNewClosure() block in a dynamic module that cannot resolve the
    # watcher functions when this script is invoked with the call operator.
    $futureRequest = New-SelfTestRequestMarker $now.AddSeconds(10) 300
    Assert-SelfTestFailure `
        -Operation {
            $null = Assert-FreshRequestBinding $futureRequest.marker $runnerStart $now 2
        } `
        -ExpectedText "unacceptably far in the future" `
        -Label "future marker"

    $expiredRequest = New-SelfTestRequestMarker $now.AddSeconds(-20) 15
    Assert-SelfTestFailure `
        -Operation {
            $null = Assert-FreshRequestBinding $expiredRequest.marker $runnerStart $now 2
        } `
        -ExpectedText "request has expired" `
        -Label "expired marker"

    $mismatchedReceived = New-SelfTestReceivedMarker `
        $now.AddSeconds(-1) `
        "fedcba9876543210fedcba9876543210"
    Assert-SelfTestFailure `
        -Operation {
            $null = Assert-ReceivedMarkerBinding `
                $validRequest.marker `
                $mismatchedReceived.marker `
                $now.AddSeconds(-40) `
                $now.AddSeconds(260) `
                $now `
                2
        } `
        -ExpectedText "does not match the automatic foreground-baseline request ID" `
        -Label "mismatched received marker"

    $shortRequest = New-SelfTestRequestMarker $now.AddSeconds(-20) 15
    $lateReceived = New-SelfTestReceivedMarker $now.AddSeconds(-1)
    Assert-SelfTestFailure `
        -Operation {
            $null = Assert-ReceivedMarkerBinding `
                $shortRequest.marker `
                $lateReceived.marker `
                $now.AddSeconds(-20) `
                $now.AddSeconds(-5) `
                $now `
                2
        } `
        -ExpectedText "after the automatic foreground-baseline deadline" `
        -Label "late received marker"

    $deadRunnerReader = { param($ignoredProcessId) return [pscustomobject]@{ alive = $false; startTimeUtc = $null } }
    Assert-SelfTestFailure `
        -Operation {
            $null = Wait-ForegroundBaselineHandoff `
                -ExpectedRunnerProcessId 17 `
                -ExpectedRunnerStart $runnerStart `
                -RunnerStateReader $deadRunnerReader `
                -RequestReader ({ return $validRequest }.GetNewClosure()) `
                -ReceivedReader ({ return $validReceived }.GetNewClosure()) `
                -ClockReader $clockReader `
                -Sleeper $noSleep `
                -TimeoutMilliseconds 1000 `
                -AllowedFutureSeconds 2 `
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

    Write-Output "Windows automatic foreground-baseline handoff watcher self-test passed."
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
$requestMarkerPath = [IO.Path]::Combine($operatorDirectory, "foreground-arm-request.json")
$receivedMarkerPath = [IO.Path]::Combine($operatorDirectory, "foreground-arm-received.json")
$runnerReader = { param($processId) return Get-RunnerState $processId }
$markerReader = {
    param([string]$boundMarkerPath, [string]$boundOperatorDirectory, [string]$boundMarkerType)
    return Read-AtomicForegroundMarker $boundMarkerPath $boundOperatorDirectory $boundMarkerType
}
$clockReader = { return [DateTimeOffset]::UtcNow }
$sleeper = { param($milliseconds) Start-Sleep -Milliseconds $milliseconds }

$handoff = Wait-ForegroundBaselineHandoff `
    -ExpectedRunnerProcessId $RunnerProcessId `
    -ExpectedRunnerStart $expectedRunnerStart `
    -RunnerStateReader $runnerReader `
    -RequestReader $markerReader `
    -RequestReaderArguments @($requestMarkerPath, $operatorDirectory, "Request") `
    -ReceivedReader $markerReader `
    -ReceivedReaderArguments @($receivedMarkerPath, $operatorDirectory, "Received") `
    -ClockReader $clockReader `
    -Sleeper $sleeper `
    -TimeoutMilliseconds ($WaitTimeoutSeconds * 1000) `
    -AllowedFutureSeconds $FutureToleranceSeconds `
    -PollIntervalMilliseconds $PollMilliseconds

Write-Output ($handoff | ConvertTo-Json -Depth 8 -Compress)
