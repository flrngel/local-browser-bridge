#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Sanitize", "BindReview", "SelfTest")]
    [string]$Mode,

    [string]$InputImage,
    [string]$OutputImage,
    [string]$OutputRecord,
    [string]$PreflightRecord,
    [string]$PendingRecord,
    [string]$ReviewedImage,
    [string]$IndependentReviewRecord,

    [ValidateSet(
        "extensions-card", "extension-details", "popup-connected", "native-debugger-warning", "page-control-pill", "action-result",
        "stop-after", "stop-paused-popup", "cancel-after", "cancel-paused-popup",
        "resume-active", "extension-loaded", "api-action-result", "computer-share-action",
        "stop-paused", "cancel-paused", "post-handback-resume"
    )]
    [string]$Purpose,

    [int]$CropX = -1,
    [int]$CropY = -1,
    [int]$CropWidth = -1,
    [int]$CropHeight = -1,
    [string]$DenyValuesFile,
    [switch]$LegacyV0122ReviewConfirmed
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$script:Utf8NoBom = [Text.UTF8Encoding]::new($false, $true)
$script:MaxInputBytes = 20MB
$script:MaxDimension = 8192
$script:MaxPixels = 50MB
$script:MinOutputWidth = 120
$script:MinOutputHeight = 32
$script:ForbiddenPngChunks = @("tEXt", "zTXt", "iTXt", "eXIf", "iCCP", "tIME")

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
$script:LegacyReviewStatement = "A human reviewed this tight crop; OCR is supplemental and unknown sensitive pixels are not automatically redacted."
$script:PendingReviewStatement = "Sanitization completed; independent visual review is pending and no automated text inspection or pixel-redaction safety proof is claimed."
$script:CompletedReviewStatement = "A separate agent reviewed this exact digest-bound crop; no sensitive pixels were observed, but visual judgment is not a pixel-safety proof."
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
$script:RequiredVisibleStatesV2 = [ordered]@{
    "extension-loaded" = "stock Chrome chrome://extensions shows exactly one enabled unpacked Local Browser Bridge v0.12.33 card with no load errors and Chrome's debugger-use indicator during the active bridge lease"
    "api-action-result" = "the loopback demo visibly shows Hello, Bridge Matrix. blue selected. after the browser API action"
    "computer-share-action" = "the exact shared Chrome window visibly shows the post-click demo state and synthetic session pointer from a fresh helper frame"
    "stop-paused" = "the trusted extension popup visibly shows the human pause and Resume remote control after the in-page Stop handback"
    "cancel-paused" = "the trusted extension popup visibly shows the human pause and Resume remote control after Chrome's browser-owned Cancel handback"
    "post-handback-resume" = "the exact demo visibly shows the restored Chrome debugger-use indicator and page control pill after both trusted-popup recovery cycles"
}

function Assert-RequiredArgument {
    param([string]$Value, [string]$Name)
    if ([String]::IsNullOrWhiteSpace($Value)) {
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
    $directory = if ([IO.Directory]::Exists([IO.Path]::GetFullPath($Path))) {
        [IO.DirectoryInfo]::new([IO.Path]::GetFullPath($Path))
    }
    else { [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($Path))) }
    while ($null -ne $directory) {
        if ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "$Label must not traverse a reparse-point directory."
        }
        $directory = $directory.Parent
    }
}

function Resolve-NewOutputFile {
    param([string]$Path, [string]$Label, [string]$RequiredExtension)
    $resolved = [IO.Path]::GetFullPath($Path)
    if ([IO.Path]::GetExtension($resolved) -cne $RequiredExtension) {
        throw "$Label must use the $RequiredExtension extension."
    }
    if ([IO.File]::Exists($resolved) -or [IO.Directory]::Exists($resolved)) {
        throw "$Label already exists; evidence is never overwritten."
    }
    $parent = [IO.Path]::GetDirectoryName($resolved)
    if ([String]::IsNullOrWhiteSpace($parent) -or -not [IO.Directory]::Exists($parent)) {
        throw "$Label parent directory must already exist."
    }
    $parentInfo = [IO.DirectoryInfo]::new($parent)
    if (($parentInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "$Label parent directory must not be a reparse point."
    }
    Assert-NoReparseAncestorChain $parent "$Label parent"
    return $resolved
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
    $hasher = [Security.Cryptography.SHA256]::Create()
    $digest = $null
    try {
        $digest = $hasher.ComputeHash($bytes)
        return ([BitConverter]::ToString($digest)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        if ($null -ne $digest) { [Array]::Clear($digest, 0, $digest.Length) }
        [Array]::Clear($bytes, 0, $bytes.Length)
        $hasher.Dispose()
    }
}

function Get-BytesSha256 {
    param([byte[]]$Bytes)
    $hasher = [Security.Cryptography.SHA256]::Create()
    $digest = $null
    try {
        $digest = $hasher.ComputeHash($Bytes)
        return ([BitConverter]::ToString($digest)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        if ($null -ne $digest) { [Array]::Clear($digest, 0, $digest.Length) }
        $hasher.Dispose()
    }
}

function Read-StableBytes {
    param([string]$Path, [int64]$MaximumBytes, [string]$Label)
    $item = [IO.FileInfo]::new([IO.Path]::GetFullPath($Path))
    if (-not $item.Exists -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
        $item.Length -le 0 -or $item.Length -gt $MaximumBytes -or $item.Length -gt [int]::MaxValue) {
        throw "$Label is not an ordinary bounded file."
    }
    Assert-NoReparseAncestorChain $item.FullName $Label
    $stream = [IO.File]::Open(
        $item.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None
    )
    try {
        if ($stream.Length -ne $item.Length) { throw "$Label changed before its stable read." }
        $bytes = New-Object byte[] ([int]$stream.Length)
        $offset = 0
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -le 0) { throw "$Label ended during its stable read." }
            $offset += $read
        }
        return ,$bytes
    }
    finally { $stream.Dispose() }
}

function Assert-CanonicalUtcTimestamp {
    param([object]$Value, [string]$Label)
    if ($Value -isnot [string] -or
        -not [regex]::IsMatch([string]$Value, '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{7}Z$')) {
        throw "$Label must be a canonical UTC timestamp."
    }
    $parsed = [DateTime]::MinValue
    if (-not [DateTime]::TryParseExact(
        [string]$Value, "o", [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::RoundtripKind, [ref]$parsed
    ) -or $parsed.Kind -ne [DateTimeKind]::Utc) {
        throw "$Label is not a valid UTC timestamp."
    }
}

function Assert-ReleaseCandidateBindingBasic {
    param([object]$Binding, [object]$Candidate)
    $fields = @(
        "schemaVersion", "version", "releaseTag", "repository", "sourceSha",
        "workflowRunId", "workflowRunAttempt", "workflowEvent", "workflowRef", "workflowPath",
        "artifactId", "artifactName",
        "artifactZipBytes", "artifactZipSha256", "checksumManifestSha256",
        "attestationInvocationUri", "attestedAssetCount", "githubHostedRunner", "assets"
    )
    Assert-ExactKeys $Binding $fields "preflight releaseCandidateBinding"
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
        $Binding.attestedAssetCount -ne 5 -or $Binding.githubHostedRunner -ne $true -or
        @($Binding.assets).Count -ne 5) {
        throw "Preflight releaseCandidateBinding is invalid."
    }
    foreach ($asset in @($Binding.assets)) {
        Assert-ExactKeys $asset @("file", "bytes", "sha256") "preflight release asset"
        if ($asset.bytes -isnot [ValueType] -or [int64]$asset.bytes -le 0 -or
            [string]$asset.sha256 -cnotmatch '^[0-9a-f]{64}$') {
            throw "Preflight releaseCandidateBinding contains an invalid asset."
        }
    }
}

function Get-CandidateBindingFromPreflight {
    param([string]$Path)
    $resolved = Resolve-RequiredFile $Path "PreflightRecord"
    $item = [IO.FileInfo]::new($resolved)
    if ($item.Length -le 0 -or $item.Length -gt 1MB) {
        throw "PreflightRecord has an invalid size."
    }
    $bytes = Read-StableBytes $resolved 1MB "PreflightRecord"
    $preflightSha256 = Get-BytesSha256 $bytes
    try {
        $record = ConvertFrom-JsonPreservingStrings ($script:Utf8NoBom.GetString($bytes))
        $actual = @($record.PSObject.Properties.Name)
        $expected = @("schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed", "runNonce", "candidate")
        if ([string]$record.candidate.version -ceq "0.12.33") {
            $expected = @(
                "schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed",
                "runNonce", "releaseCandidateBinding", "candidate"
            )
        }
        if (($actual -join "`n") -cne ($expected -join "`n") -or
            $record.schemaVersion -ne 1 -or
            $record.evidenceType -cne "stock-user-chrome-candidate-binding" -or
            $record.phase -cne "preflight" -or $record.passed -ne $true -or
            [string]$record.runNonce -cnotmatch '^[0-9a-f]{64}$' -or
            [string]$record.candidate.finalSha -cnotmatch '^[0-9a-f]{40}$' -or
            @("0.12.2", "0.12.33") -cnotcontains [string]$record.candidate.version) {
            throw "PreflightRecord identity is invalid."
        }
        foreach ($value in @(
            [string]$record.candidate.checksumManifest.sha256,
            [string]$record.candidate.server.sha256,
            $(if ([string]$record.candidate.version -ceq "0.12.33") { [string]$record.candidate.computerHelper.sha256 } else { [string]$record.candidate.server.sha256 }),
            [string]$record.candidate.extension.sha256,
            [string]$record.candidate.extension.combinedPayloadSha256
        )) {
            if ($value -cnotmatch '^[0-9a-f]{64}$') {
                throw "PreflightRecord candidate hash is invalid."
            }
        }
        $script:CandidateVersionFromPreflight = [string]$record.candidate.version
        if ($script:CandidateVersionFromPreflight -ceq "0.12.33") {
            Assert-ReleaseCandidateBindingBasic $record.releaseCandidateBinding $record.candidate
            $script:ReleaseCandidateBindingFromPreflight = $record.releaseCandidateBinding
        }
        else { $script:ReleaseCandidateBindingFromPreflight = $null }
        $binding = [ordered]@{
            runNonce = [string]$record.runNonce
            preflightRecordSha256 = $preflightSha256
            finalSha = [string]$record.candidate.finalSha
            checksumManifestSha256 = [string]$record.candidate.checksumManifest.sha256
            serverSha256 = [string]$record.candidate.server.sha256
        }
        if ($script:CandidateVersionFromPreflight -ceq "0.12.33") {
            $binding.computerHelperSha256 = [string]$record.candidate.computerHelper.sha256
        }
        $binding.extensionZipSha256 = [string]$record.candidate.extension.sha256
        $binding.extractedPayloadSha256 = [string]$record.candidate.extension.combinedPayloadSha256
        return $binding
    }
    finally {
        [Array]::Clear($bytes, 0, $bytes.Length)
    }
}

function Read-DenyValues {
    param([string]$Path)
    if ([String]::IsNullOrWhiteSpace($Path)) {
        return @()
    }
    $resolved = Resolve-RequiredFile $Path "DenyValuesFile"
    $bytes = [IO.File]::ReadAllBytes($resolved)
    if ($bytes.Length -le 0 -or $bytes.Length -gt 65536) {
        throw "DenyValuesFile has an invalid size."
    }
    $text = $script:Utf8NoBom.GetString($bytes)
    $values = @($text -split "`r?`n" | Where-Object { -not [String]::IsNullOrWhiteSpace($_) })
    if ($values.Count -eq 0) {
        throw "DenyValuesFile must contain at least one non-empty value."
    }
    foreach ($value in $values) {
        if ($value.Length -lt 4 -or $value.Length -gt 512) {
            throw "DenyValuesFile entries must contain 4 through 512 characters."
        }
    }
    return $values
}

function Test-ForbiddenText {
    param(
        [string]$Text,
        [string[]]$ExactValues,
        [switch]$AllowCanonicalBindingHex,
        [switch]$AllowCanonicalEvidenceType
    )
    foreach ($value in $ExactValues) {
        if ($Text.IndexOf($value, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
            return $true
        }
    }
    $textToScan = if ($AllowCanonicalBindingHex) {
        [regex]::Replace($Text, '"(?:[0-9a-f]{40}|[0-9a-f]{64})"', '""', [Text.RegularExpressions.RegexOptions]::CultureInvariant)
    }
    else { $Text }
    if ($AllowCanonicalEvidenceType) {
        $textToScan = $textToScan.Replace('"stock-user-chrome-screenshot-review-pending"', '""')
    }
    foreach ($pattern in @(
        '(?i)\b(?:authorization|bearer|access[_ -]?token|api[_ -]?key|client[_ -]?secret)\b',
        '(?i)[a-z]:\\users\\',
        '(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b',
        '(?i)(?<![a-p])[a-p]{32}(?![a-p])'
    )) {
        if ([regex]::IsMatch($textToScan, $pattern, [Text.RegularExpressions.RegexOptions]::CultureInvariant)) {
            return $true
        }
    }
    foreach ($match in [regex]::Matches($textToScan, '(?<![A-Za-z0-9_-])[A-Za-z0-9_-]{40,}(?![A-Za-z0-9_-])')) {
        return $true
    }
    foreach ($match in [regex]::Matches($textToScan, '(?<![0-9])(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?![0-9])')) {
        if ($match.Value -cne "127.0.0.1" -and $match.Value -cne "0.0.0.0") {
            return $true
        }
    }
    return $false
}

function Get-PngChunkTypesFromBytes {
    param([byte[]]$Bytes)
    $signature = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    if ($Bytes.Length -lt 33) {
        throw "PNG is too small."
    }
    for ($index = 0; $index -lt $signature.Length; $index += 1) {
        if ($Bytes[$index] -ne $signature[$index]) {
            throw "Screenshot must be a PNG."
        }
    }
    $types = @()
    $offset = 8
    $sawEnd = $false
    while ($offset -lt $Bytes.Length) {
        if ($Bytes.Length - $offset -lt 12) {
            throw "PNG chunk framing is invalid."
        }
        $length = ([uint32]$Bytes[$offset] -shl 24) -bor
            ([uint32]$Bytes[$offset + 1] -shl 16) -bor
            ([uint32]$Bytes[$offset + 2] -shl 8) -bor
            [uint32]$Bytes[$offset + 3]
        if ($length -gt $script:MaxInputBytes -or ([uint64]$offset + 12 + $length) -gt $Bytes.Length) {
            throw "PNG chunk length is invalid."
        }
        $type = [Text.Encoding]::ASCII.GetString($Bytes, $offset + 4, 4)
        if (-not [regex]::IsMatch($type, '^[A-Za-z]{4}$')) {
            throw "PNG chunk type is invalid."
        }
        $types += $type
        $offset += 12 + [int]$length
        if ($type -ceq "IEND") {
            if ($length -ne 0 -or $offset -ne $Bytes.Length) {
                throw "PNG has trailing data after IEND."
            }
            $sawEnd = $true
            break
        }
    }
    if (-not $sawEnd -or $types[0] -cne "IHDR") {
        throw "PNG does not have canonical framing."
    }
    return $types
}

function Write-NewJson {
    param([string]$TemporaryPath, [object]$Value)
    $json = $Value | ConvertTo-Json -Depth 12
    $bytes = $script:Utf8NoBom.GetBytes("$json`n")
    $stream = [IO.File]::Open(
        $TemporaryPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
    )
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    }
    finally {
        $stream.Dispose()
        [Array]::Clear($bytes, 0, $bytes.Length)
    }
}

function Assert-ExactKeys {
    param([object]$Object, [string[]]$Expected, [string]$Label)
    if ($null -eq $Object) { throw "$Label must be an object." }
    $actual = @(
        if ($Object -is [Collections.IDictionary]) {
            foreach ($key in $Object.Keys) { [string]$key }
        }
        else {
            $Object.PSObject.Properties.Name
        }
    )
    if (($actual -join "`n") -cne ($Expected -join "`n")) {
        throw "$Label contains missing, unexpected, or reordered fields."
    }
}

function Test-ExactJsonInteger([object]$Value) {
    return $Value -is [int] -or $Value -is [long]
}

function Assert-CandidateBindingEqual {
    param([object]$Actual, [object]$Expected)
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
    Assert-ExactKeys $Actual $fields "pending screenshot candidateBinding"
    Assert-ExactKeys $Expected $fields "preflight candidateBinding"
    foreach ($name in $fields) {
        if ($Actual.$name -isnot [string] -or $Actual.$name -cne $Expected.$name) {
            throw "Pending screenshot does not bind the supplied candidate preflight."
        }
    }
}

function Read-StrictJsonWithDigest {
    param([string]$Path, [string]$Label)
    $bytes = Read-StableBytes $Path 2MB $Label
    try {
        try { $value = ConvertFrom-JsonPreservingStrings ($script:Utf8NoBom.GetString($bytes)) }
        catch { throw "$Label is not strict UTF-8 JSON." }
        return [pscustomobject]@{ Value = $value; Sha256 = Get-BytesSha256 $bytes }
    }
    catch { throw "$Label is not strict UTF-8 JSON." }
    finally { [Array]::Clear($bytes, 0, $bytes.Length) }
}

function Invoke-Sanitize {
    foreach ($item in @(
        @($InputImage, "InputImage"), @($OutputImage, "OutputImage"),
        @($OutputRecord, "OutputRecord"), @($PreflightRecord, "PreflightRecord"), @($Purpose, "Purpose")
    )) {
        Assert-RequiredArgument $item[0] $item[1]
    }
    $inputPath = Resolve-RequiredFile $InputImage "InputImage"
    $inputInfo = [IO.FileInfo]::new($inputPath)
    if ($inputInfo.Length -le 0 -or $inputInfo.Length -gt $script:MaxInputBytes) {
        throw "InputImage has an invalid size."
    }
    $outputPath = Resolve-NewOutputFile $OutputImage "OutputImage" ".png"
    $recordPath = Resolve-NewOutputFile $OutputRecord "OutputRecord" ".json"
    if ($outputPath -ceq $recordPath -or $inputPath -ceq $outputPath -or $inputPath -ceq $recordPath) {
        throw "Input, output image, and output record must be distinct files."
    }
    $denyValues = @(Read-DenyValues $DenyValuesFile)
    $candidateBinding = Get-CandidateBindingFromPreflight $PreflightRecord
    $legacyOnePhase = $script:CandidateVersionFromPreflight -ceq "0.12.2"
    $expectedScreenshots = if ($legacyOnePhase) {
        $script:ExpectedScreenshots
    }
    else { $script:ExpectedScreenshotsV2 }
    $sourceCapture = $null
    if (-not $expectedScreenshots.Contains($Purpose)) {
        throw "Purpose is not one of the canonical screenshot purposes."
    }
    if ($legacyOnePhase -and -not $LegacyV0122ReviewConfirmed) {
        throw "LegacyV0122ReviewConfirmed is mandatory for v0.12.2 compatibility."
    }
    if (-not $legacyOnePhase -and $LegacyV0122ReviewConfirmed) {
        throw "LegacyV0122ReviewConfirmed is forbidden for the v0.12.33 independent-review protocol."
    }
    $cropValues = @($CropX, $CropY, $CropWidth, $CropHeight)
    $hasCrop = @($cropValues | Where-Object { $_ -ne -1 }).Count -gt 0
    if ($hasCrop -and @($cropValues | Where-Object { $_ -eq -1 }).Count -gt 0) {
        throw "CropX, CropY, CropWidth, and CropHeight must be supplied together."
    }
    if ($hasCrop -and ($CropX -lt 0 -or $CropY -lt 0 -or $CropWidth -le 0 -or $CropHeight -le 0)) {
        throw "Crop coordinates and dimensions are invalid."
    }

    Add-Type -AssemblyName System.Drawing -ErrorAction Stop
    $temporaryImage = "$outputPath.new"
    $temporaryRecord = "$recordPath.new"
    if ([IO.File]::Exists($temporaryImage) -or [IO.File]::Exists($temporaryRecord)) {
        throw "A stale temporary evidence file exists."
    }
    $inputBytes = $null
    $sanitizedBytes = $null
    try {
        $inputBytes = Read-StableBytes $inputPath $script:MaxInputBytes "InputImage"
        $inputSha256 = Get-BytesSha256 $inputBytes
        $inputStream = [IO.MemoryStream]::new($inputBytes, $false)
        try {
            $source = [Drawing.Image]::FromStream($inputStream, $true, $true)
            try {
                if ($source.RawFormat.Guid -ne [Drawing.Imaging.ImageFormat]::Png.Guid) {
                    throw "InputImage must decode as PNG."
                }
                if ($source.Width -le 0 -or $source.Height -le 0 -or $source.Width -gt $script:MaxDimension -or $source.Height -gt $script:MaxDimension -or ([int64]$source.Width * $source.Height) -gt $script:MaxPixels) {
                    throw "InputImage dimensions exceed the evidence limit."
                }
                if (-not $legacyOnePhase) {
                    $expectedRawName = [IO.Path]::GetFileNameWithoutExtension(
                        $expectedScreenshots[$Purpose]
                    ) + ".raw.png"
                    if ([IO.Path]::GetFileName($inputPath) -cne $expectedRawName) {
                        throw "v0.12.33 InputImage must use the canonical raw helper-capture filename."
                    }
                    $sourceCapture = [ordered]@{
                        name = $expectedRawName
                        endpoint = "/api/computer/screenshot"
                        bytes = $inputBytes.Length
                        sha256 = $inputSha256
                        width = [int64]$source.Width
                        height = [int64]$source.Height
                    }
                }
                $left = if ($hasCrop) { $CropX } else { 0 }
                $top = if ($hasCrop) { $CropY } else { 0 }
                $width = if ($hasCrop) { $CropWidth } else { $source.Width }
                $height = if ($hasCrop) { $CropHeight } else { $source.Height }
                if ($left + $width -gt $source.Width -or $top + $height -gt $source.Height) {
                    throw "Crop rectangle exceeds InputImage bounds."
                }
                if ($width -lt $script:MinOutputWidth -or $height -lt $script:MinOutputHeight) {
                    throw "OutputImage is too small to prove the named visible UI state."
                }
                if ($width -gt $script:MaxDimension -or $height -gt $script:MaxDimension -or ([int64]$width * $height) -gt $script:MaxPixels) {
                    throw "OutputImage dimensions exceed the evidence limit."
                }
                $bitmap = [Drawing.Bitmap]::new($width, $height, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
                try {
                    $graphics = [Drawing.Graphics]::FromImage($bitmap)
                    try {
                        $graphics.Clear([Drawing.Color]::White)
                        $destination = [Drawing.Rectangle]::new(0, 0, $width, $height)
                        $graphics.DrawImage($source, $destination, $left, $top, $width, $height, [Drawing.GraphicsUnit]::Pixel)
                    }
                    finally { $graphics.Dispose() }
                    $imageStream = [IO.File]::Open(
                        $temporaryImage, [IO.FileMode]::CreateNew,
                        [IO.FileAccess]::Write, [IO.FileShare]::None
                    )
                    try {
                        $bitmap.Save($imageStream, [Drawing.Imaging.ImageFormat]::Png)
                        $imageStream.Flush($true)
                    }
                    finally { $imageStream.Dispose() }
                }
                finally { $bitmap.Dispose() }
            }
            finally { $source.Dispose() }
        }
        finally { $inputStream.Dispose() }

        $sanitizedBytes = Read-StableBytes $temporaryImage $script:MaxInputBytes "sanitized PNG"
        $sanitizedSha256 = Get-BytesSha256 $sanitizedBytes
        $chunkTypes = @(Get-PngChunkTypesFromBytes $sanitizedBytes)
        foreach ($forbidden in $script:ForbiddenPngChunks) {
            if ($chunkTypes -ccontains $forbidden) {
                throw "Sanitized PNG retained a forbidden metadata chunk."
            }
        }

        if ($legacyOnePhase) {
            $record = [ordered]@{
                schemaVersion = 1
                evidenceType = "stock-user-chrome-screenshot"
                purpose = $Purpose
                candidateBinding = $candidateBinding
                image = [ordered]@{
                    name = [IO.Path]::GetFileName($outputPath); bytes = $sanitizedBytes.Length
                    sha256 = $sanitizedSha256; width = $width; height = $height
                }
                cropApplied = $hasCrop
                metadataStrippedByDecodeAndReencode = $true
                forbiddenMetadataChunksPresent = $false
                ocrAvailable = $false
                ocrDenylistChecked = $false
                ocrDenylistMatches = 0
                manualVisualReviewConfirmed = $true
                automaticPixelRedactionPerformed = $false
                unknownPixelSafetyClaimed = $false
                reviewStatement = $script:LegacyReviewStatement
            }
        }
        else {
            $record = [ordered]@{
                schemaVersion = 1
                evidenceType = "stock-user-chrome-screenshot-review-pending"
                purpose = $Purpose
                releaseCandidateBinding = $script:ReleaseCandidateBindingFromPreflight
                candidateBinding = $candidateBinding
                sourceCapture = $sourceCapture
                image = [ordered]@{
                    name = [IO.Path]::GetFileName($outputPath); bytes = $sanitizedBytes.Length
                    sha256 = $sanitizedSha256; width = $width; height = $height
                }
                cropApplied = $hasCrop
                metadataStrippedByDecodeAndReencode = $true
                forbiddenMetadataChunksPresent = $false
                automatedTextInspectionPerformed = $false
                independentVisualReviewRequired = $true
                independentVisualReviewCompleted = $false
                automaticPixelRedactionPerformed = $false
                unknownPixelSafetyClaimed = $false
                reviewStatement = $script:PendingReviewStatement
            }
        }
        $serialized = $record | ConvertTo-Json -Depth 12 -Compress
        if (Test-ForbiddenText $serialized $denyValues -AllowCanonicalBindingHex -AllowCanonicalEvidenceType) {
            throw "Screenshot evidence record failed its text safety check."
        }
        Write-NewJson $temporaryRecord $record
        [IO.File]::Move($temporaryImage, $outputPath)
        try {
            [IO.File]::Move($temporaryRecord, $recordPath)
        }
        catch {
            [IO.File]::Delete($outputPath)
            throw
        }
        if ($legacyOnePhase) {
            Write-Output "Legacy v0.12.2 screenshot was sanitized with its required review confirmation."
        }
        else {
            Write-Output "Screenshot was sanitized with an exact digest-bound independent review pending."
        }
    }
    finally {
        if ($null -ne $inputBytes) { [Array]::Clear($inputBytes, 0, $inputBytes.Length) }
        if ($null -ne $sanitizedBytes) { [Array]::Clear($sanitizedBytes, 0, $sanitizedBytes.Length) }
        foreach ($temporary in @($temporaryImage, $temporaryRecord)) {
            if ([IO.File]::Exists($temporary)) {
                [IO.File]::Delete($temporary)
            }
        }
    }
}

function Invoke-BindReview {
    foreach ($item in @(
        @($PendingRecord, "PendingRecord"), @($ReviewedImage, "ReviewedImage"),
        @($OutputRecord, "OutputRecord"), @($PreflightRecord, "PreflightRecord"),
        @($IndependentReviewRecord, "IndependentReviewRecord")
    )) {
        Assert-RequiredArgument $item[0] $item[1]
    }
    $pendingPath = Resolve-RequiredFile $PendingRecord "PendingRecord"
    $imagePath = Resolve-RequiredFile $ReviewedImage "ReviewedImage"
    $reviewPath = Resolve-RequiredFile $IndependentReviewRecord "IndependentReviewRecord"
    $recordPath = Resolve-NewOutputFile $OutputRecord "OutputRecord" ".json"
    if ($pendingPath -ceq $recordPath -or $imagePath -ceq $recordPath -or
        -not [String]::Equals(
            [IO.Path]::GetDirectoryName($imagePath), [IO.Path]::GetDirectoryName($recordPath),
            [StringComparison]::OrdinalIgnoreCase
        )) {
        throw "The finalized sidecar must be a new file beside the reviewed sanitized PNG."
    }
    $denyValues = @(Read-DenyValues $DenyValuesFile)
    $candidateBinding = Get-CandidateBindingFromPreflight $PreflightRecord
    if ($script:CandidateVersionFromPreflight -cne "0.12.33") {
        throw "BindReview is available only for the v0.12.33 independent-review protocol."
    }
    $pendingStable = Read-StrictJsonWithDigest $pendingPath "PendingRecord"
    $pending = $pendingStable.Value
    $fields = @(
        "schemaVersion", "evidenceType", "purpose", "releaseCandidateBinding", "candidateBinding", "sourceCapture", "image", "cropApplied",
        "metadataStrippedByDecodeAndReencode", "forbiddenMetadataChunksPresent",
        "automatedTextInspectionPerformed", "independentVisualReviewRequired", "independentVisualReviewCompleted",
        "automaticPixelRedactionPerformed", "unknownPixelSafetyClaimed", "reviewStatement"
    )
    Assert-ExactKeys $pending $fields "pending screenshot record"
    if ($pending.schemaVersion -ne 1 -or
        $pending.evidenceType -cne "stock-user-chrome-screenshot-review-pending" -or
        $pending.purpose -isnot [string] -or -not $script:ExpectedScreenshotsV2.Contains($pending.purpose)) {
        throw "PendingRecord identity is invalid."
    }
    if (($pending.releaseCandidateBinding | ConvertTo-Json -Depth 10 -Compress) -cne
        ($script:ReleaseCandidateBindingFromPreflight | ConvertTo-Json -Depth 10 -Compress)) {
        throw "Pending screenshot does not bind the exact release candidate attempt."
    }
    Assert-CandidateBindingEqual $pending.candidateBinding $candidateBinding
    Assert-ExactKeys $pending.sourceCapture @(
        "name", "endpoint", "bytes", "sha256", "width", "height"
    ) "pending source capture"
    $expectedRawName = [IO.Path]::GetFileNameWithoutExtension(
        $script:ExpectedScreenshotsV2[$pending.purpose]
    ) + ".raw.png"
    if ($pending.sourceCapture.name -cne $expectedRawName -or
        $pending.sourceCapture.endpoint -cne "/api/computer/screenshot" -or
        $pending.sourceCapture.bytes -isnot [ValueType] -or
        [int64]$pending.sourceCapture.bytes -le 1000 -or
        [int64]$pending.sourceCapture.bytes -gt $script:MaxInputBytes -or
        $pending.sourceCapture.sha256 -isnot [string] -or
        -not [regex]::IsMatch($pending.sourceCapture.sha256, '^[0-9a-f]{64}$') -or
        $pending.sourceCapture.width -isnot [ValueType] -or
        $pending.sourceCapture.height -isnot [ValueType] -or
        [int64]$pending.sourceCapture.width -lt $script:MinOutputWidth -or
        [int64]$pending.sourceCapture.height -lt $script:MinOutputHeight -or
        [int64]$pending.sourceCapture.width -gt $script:MaxDimension -or
        [int64]$pending.sourceCapture.height -gt $script:MaxDimension -or
        ([int64]$pending.sourceCapture.width * [int64]$pending.sourceCapture.height) -gt $script:MaxPixels) {
        throw "PendingRecord source capture is invalid."
    }
    Assert-ExactKeys $pending.image @("name", "bytes", "sha256", "width", "height") "pending screenshot image"
    $expectedImageName = $script:ExpectedScreenshotsV2[$pending.purpose]
    $expectedSidecarName = [IO.Path]::GetFileNameWithoutExtension($expectedImageName) + ".json"
    if ([IO.Path]::GetFileName($imagePath) -cne $expectedImageName -or
        [IO.Path]::GetFileName($recordPath) -cne $expectedSidecarName -or
        $pending.image.name -cne $expectedImageName -or
        $pending.image.bytes -isnot [ValueType] -or [int64]$pending.image.bytes -le 0 -or
        [int64]$pending.image.bytes -gt $script:MaxInputBytes -or
        $pending.image.sha256 -isnot [string] -or
        -not [regex]::IsMatch($pending.image.sha256, '^[0-9a-f]{64}$') -or
        $pending.image.width -isnot [ValueType] -or $pending.image.height -isnot [ValueType] -or
        [int64]$pending.image.width -lt $script:MinOutputWidth -or
        [int64]$pending.image.height -lt $script:MinOutputHeight -or
        [int64]$pending.image.width -gt $script:MaxDimension -or
        [int64]$pending.image.height -gt $script:MaxDimension -or
        ([int64]$pending.image.width * [int64]$pending.image.height) -gt $script:MaxPixels) {
        throw "PendingRecord image identity or dimensions are invalid."
    }
    foreach ($name in @("cropApplied", "metadataStrippedByDecodeAndReencode")) {
        if ($pending.$name -isnot [bool] -or $pending.$name -ne $true) {
            throw "PendingRecord $name must be true."
        }
    }
    foreach ($name in @(
        "forbiddenMetadataChunksPresent", "automatedTextInspectionPerformed", "independentVisualReviewCompleted",
        "automaticPixelRedactionPerformed", "unknownPixelSafetyClaimed"
    )) {
        if ($pending.$name -isnot [bool] -or $pending.$name -ne $false) {
            throw "PendingRecord $name must be false."
        }
    }
    if ($pending.independentVisualReviewRequired -ne $true -or
        $pending.reviewStatement -cne $script:PendingReviewStatement) {
        throw "PendingRecord sanitization or pending-review state is invalid."
    }
    $reviewedImageBytes = Read-StableBytes $imagePath $script:MaxInputBytes "ReviewedImage"
    try {
        if ($reviewedImageBytes.Length -ne [int64]$pending.image.bytes -or
            (Get-BytesSha256 $reviewedImageBytes) -cne $pending.image.sha256) {
            throw "The sanitized PNG changed after its pending review record was created."
        }
        $chunkTypes = @(Get-PngChunkTypesFromBytes $reviewedImageBytes)
        foreach ($forbidden in $script:ForbiddenPngChunks) {
            if ($chunkTypes -ccontains $forbidden) {
                throw "The reviewed sanitized PNG contains forbidden metadata."
            }
        }
        Add-Type -AssemblyName System.Drawing -ErrorAction Stop
        $imageStream = [IO.MemoryStream]::new($reviewedImageBytes, $false)
        try {
            $decoded = [Drawing.Image]::FromStream($imageStream, $true, $true)
            try {
                if ($decoded.RawFormat.Guid -ne [Drawing.Imaging.ImageFormat]::Png.Guid -or
                    $decoded.Width -ne [int]$pending.image.width -or $decoded.Height -ne [int]$pending.image.height) {
                    throw "The reviewed sanitized PNG dimensions do not match its pending record."
                }
            }
            finally { $decoded.Dispose() }
        }
        finally { $imageStream.Dispose() }
    }
    finally {
        [Array]::Clear($reviewedImageBytes, 0, $reviewedImageBytes.Length)
    }

    $reviewStable = Read-StrictJsonWithDigest $reviewPath "IndependentReviewRecord"
    $review = $reviewStable.Value
    Assert-ExactKeys $review @(
        "schemaVersion", "evidenceType", "releaseCandidateBinding", "candidateBinding",
        "executorSessionRef", "reviewerSessionRef", "independentSessionBoundary",
        "requestSha256", "reviewedAtUtc", "entries", "aggregate"
    ) "independent visual review"
    if ($review.schemaVersion -ne 1 -or
        $review.evidenceType -cne "stock-user-chrome-independent-visual-review" -or
        $review.independentSessionBoundary -isnot [bool] -or
        $review.independentSessionBoundary -ne $true) {
        throw "IndependentReviewRecord identity or session-boundary proof is invalid."
    }
    foreach ($name in @("executorSessionRef", "reviewerSessionRef", "requestSha256")) {
        if ($review.$name -isnot [string] -or
            -not [regex]::IsMatch([string]$review.$name, '^[0-9a-f]{64}$')) {
            throw "IndependentReviewRecord $name is invalid."
        }
    }
    if ($review.executorSessionRef -ceq $review.reviewerSessionRef) {
        throw "IndependentReviewRecord reused the executor as the reviewer."
    }
    Assert-CanonicalUtcTimestamp $review.reviewedAtUtc "IndependentReviewRecord reviewedAtUtc"
    if (($review.releaseCandidateBinding | ConvertTo-Json -Depth 12 -Compress) -cne
        ($script:ReleaseCandidateBindingFromPreflight | ConvertTo-Json -Depth 12 -Compress)) {
        throw "IndependentReviewRecord does not bind the exact release candidate attempt."
    }
    Assert-CandidateBindingEqual $review.candidateBinding $candidateBinding
    Assert-ExactKeys $review.aggregate @(
        "reviewedCropCount", "everySanitizedCropOpenedByReviewer", "allImageDigestsMatched",
        "requiredVisibleStateConfirmedByReviewer", "noSensitivePixelsObservedByReviewer",
        "noUncertaintyReported", "visualJudgmentNotPixelSafetyProof"
    ) "independent visual review aggregate"
    if (-not (Test-ExactJsonInteger $review.aggregate.reviewedCropCount) -or
        [int64]$review.aggregate.reviewedCropCount -ne 6) {
        throw "IndependentReviewRecord must contain six reviewed crops."
    }
    foreach ($name in @(
        "everySanitizedCropOpenedByReviewer", "allImageDigestsMatched",
        "requiredVisibleStateConfirmedByReviewer", "noSensitivePixelsObservedByReviewer",
        "noUncertaintyReported", "visualJudgmentNotPixelSafetyProof"
    )) {
        if ($review.aggregate.$name -isnot [bool] -or $review.aggregate.$name -ne $true) {
            throw "IndependentReviewRecord aggregate $name must be true."
        }
    }
    if ($review.entries.Count -ne 6) {
        throw "IndependentReviewRecord entry count is invalid."
    }
    $reviewEntry = $null
    for ($index = 0; $index -lt 6; $index += 1) {
        $entry = $review.entries[$index]
        Assert-ExactKeys $entry @(
            "sequence", "purpose", "image", "sha256", "width", "height",
            "requiredVisibleStateSha256", "digestMatched", "requiredStateVerdict",
            "sensitivePixelsObserved", "uncertain"
        ) "independent visual review entry"
        $expectedPurpose = @($script:ExpectedScreenshotsV2.Keys)[$index]
        $expectedImage = $script:ExpectedScreenshotsV2[$expectedPurpose]
        $expectedCriterionSha = Get-TextSha256 $script:RequiredVisibleStatesV2[$expectedPurpose]
        if (-not (Test-ExactJsonInteger $entry.sequence) -or
            -not (Test-ExactJsonInteger $entry.width) -or
            -not (Test-ExactJsonInteger $entry.height) -or
            [int64]$entry.sequence -ne ($index + 1) -or $entry.purpose -cne $expectedPurpose -or
            $entry.image -cne $expectedImage -or $entry.requiredVisibleStateSha256 -cne $expectedCriterionSha -or
            $entry.sha256 -isnot [string] -or -not [regex]::IsMatch($entry.sha256, '^[0-9a-f]{64}$') -or
            $entry.digestMatched -isnot [bool] -or
            $entry.sensitivePixelsObserved -isnot [bool] -or $entry.uncertain -isnot [bool] -or
            $entry.digestMatched -ne $true -or $entry.requiredStateVerdict -cne "pass" -or
            $entry.sensitivePixelsObserved -ne $false -or $entry.uncertain -ne $false) {
            throw "IndependentReviewRecord contains a mismatched, failed, sensitive, uncertain, or reordered entry."
        }
        if ($entry.purpose -ceq $pending.purpose) { $reviewEntry = $entry }
    }
    if ($null -eq $reviewEntry -or $reviewEntry.image -cne $pending.image.name -or
        $reviewEntry.sha256 -cne $pending.image.sha256 -or
        [int64]$reviewEntry.width -ne [int64]$pending.image.width -or
        [int64]$reviewEntry.height -ne [int64]$pending.image.height) {
        throw "IndependentReviewRecord does not bind this exact sanitized PNG."
    }
    $reviewRecordSha256 = $reviewStable.Sha256
    $reviewEntryRef = Get-TextSha256 (
        "$reviewRecordSha256`n$($reviewEntry.sequence)`n$($reviewEntry.purpose)`n$($reviewEntry.sha256)"
    )

    $record = [ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-screenshot"
        purpose = [string]$pending.purpose
        releaseCandidateBinding = $script:ReleaseCandidateBindingFromPreflight
        candidateBinding = $candidateBinding
        sourceCapture = [ordered]@{
            name = [string]$pending.sourceCapture.name
            endpoint = [string]$pending.sourceCapture.endpoint
            bytes = [int64]$pending.sourceCapture.bytes
            sha256 = [string]$pending.sourceCapture.sha256
            width = [int64]$pending.sourceCapture.width
            height = [int64]$pending.sourceCapture.height
        }
        image = [ordered]@{
            name = [string]$pending.image.name
            bytes = [int64]$pending.image.bytes
            sha256 = [string]$pending.image.sha256
            width = [int64]$pending.image.width
            height = [int64]$pending.image.height
        }
        cropApplied = $true
        metadataStrippedByDecodeAndReencode = $true
        forbiddenMetadataChunksPresent = $false
        automatedTextInspectionPerformed = $false
        independentVisualReviewRequired = $true
        independentVisualReviewCompleted = $true
        reviewRecordSha256 = $reviewRecordSha256
        reviewEntryRef = $reviewEntryRef
        automaticPixelRedactionPerformed = $false
        unknownPixelSafetyClaimed = $false
        reviewStatement = $script:CompletedReviewStatement
    }
    $serialized = $record | ConvertTo-Json -Depth 12 -Compress
    if (Test-ForbiddenText $serialized $denyValues -AllowCanonicalBindingHex) {
        throw "Final screenshot review attestation failed its text safety check."
    }
    $temporaryRecord = "$recordPath.new"
    if ([IO.File]::Exists($temporaryRecord)) { throw "A stale temporary review attestation exists." }
    try {
        Write-NewJson $temporaryRecord $record
        [IO.File]::Move($temporaryRecord, $recordPath)
    }
    finally {
        if ([IO.File]::Exists($temporaryRecord)) { [IO.File]::Delete($temporaryRecord) }
    }
    Write-Output "Independent digest-bound review was bound; the reviewed PNG hash is unchanged."
}

function Invoke-SelfTest {
    Add-Type -AssemblyName System.Drawing -ErrorAction Stop
    $orderedKeyProbe = [ordered]@{ first = 1; second = 2 }
    Assert-ExactKeys $orderedKeyProbe @("first", "second") "ordered dictionary probe"
    foreach ($invalidProbe in @(
        [ordered]@{ second = 2; first = 1 },
        [ordered]@{ first = 1; second = 2; third = 3 }
    )) {
        $invalidKeysRejected = $false
        try { Assert-ExactKeys $invalidProbe @("first", "second") "invalid dictionary probe" }
        catch { $invalidKeysRejected = $true }
        if (-not $invalidKeysRejected) {
            throw "Screenshot sanitizer exact ordered dictionary key self-test failed."
        }
    }
    $root = [IO.Path]::Combine([IO.Path]::GetTempPath(), "lbb-browser-shot-" + [Guid]::NewGuid().ToString("N"))
    [IO.Directory]::CreateDirectory($root) | Out-Null
    try {
        $inputPath = [IO.Path]::Combine($root, "browser-02-api-action-result.raw.png")
        $outputPath = [IO.Path]::Combine($root, "browser-02-api-action-result.png")
        $pendingPath = [IO.Path]::Combine($root, "browser-02-api-action-result.pending.json")
        $recordPath = [IO.Path]::Combine($root, "browser-02-api-action-result.json")
        $bitmap = [Drawing.Bitmap]::new(320, 180)
        try {
            for ($y = 0; $y -lt $bitmap.Height; $y += 1) {
                for ($x = 0; $x -lt $bitmap.Width; $x += 1) {
                    $red = ($x * 73 + $y * 151 + (($x * $y) % 251)) -band 255
                    $green = ($x * 197 + $y * 43 + (($x + $y) * 29)) -band 255
                    $blue = (($x -bxor $y) * 89 + $x * 17 + $y * 61) -band 255
                    $bitmap.SetPixel($x, $y, [Drawing.Color]::FromArgb(255, $red, $green, $blue))
                }
            }
            $bitmap.Save($inputPath, [Drawing.Imaging.ImageFormat]::Png)
        }
        finally { $bitmap.Dispose() }
        if ([IO.FileInfo]::new($inputPath).Length -le 1000) {
            throw "Screenshot sanitizer self-test raw fixture does not satisfy the production byte floor."
        }
        $bindingHash = [String]::new([char]"0", 64)
        $preflightPath = [IO.Path]::Combine($root, "candidate-preflight.json")
        $preflight = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-candidate-binding"
            phase = "preflight"
            recordedAtUtc = [DateTime]::UtcNow.ToString("o")
            passed = $true
            runNonce = $bindingHash
            releaseCandidateBinding = [ordered]@{
                schemaVersion = 3; version = "0.12.33"; releaseTag = "v0.12.33"
                repository = "flrngel/local-browser-bridge"; sourceSha = [String]::new([char]"0", 40)
                workflowRunId = "1"; workflowRunAttempt = "1"; workflowEvent = "workflow_dispatch"
                workflowRef = "refs/heads/main"; workflowPath = ".github/workflows/deploy.yml"
                artifactId = "1"; artifactName = "release-candidate"
                artifactZipBytes = 1; artifactZipSha256 = $bindingHash; checksumManifestSha256 = $bindingHash
                attestationInvocationUri = "https://github.com/flrngel/local-browser-bridge/actions/runs/1/attempts/1"
                attestedAssetCount = 5; githubHostedRunner = $true
                assets = @(1..5 | ForEach-Object {
                    [ordered]@{ file = "asset-$_.bin"; bytes = 1; sha256 = $bindingHash }
                })
            }
            candidate = [ordered]@{
                version = "0.12.33"
                finalSha = [String]::new([char]"0", 40)
                checksumManifest = [ordered]@{ sha256 = $bindingHash }
                server = [ordered]@{ sha256 = $bindingHash }
                computerHelper = [ordered]@{ sha256 = $bindingHash }
                extension = [ordered]@{ sha256 = $bindingHash; combinedPayloadSha256 = $bindingHash }
            }
        }
        [IO.File]::WriteAllText($preflightPath, (($preflight | ConvertTo-Json -Depth 8) + "`n"), $script:Utf8NoBom)
        $preAttestationRejected = $false
        try {
            & $PSCommandPath -Mode Sanitize -InputImage $inputPath -OutputImage $outputPath `
                -OutputRecord $pendingPath -PreflightRecord $preflightPath -Purpose api-action-result `
                -CropX 4 -CropY 4 -CropWidth 240 -CropHeight 120 -LegacyV0122ReviewConfirmed | Out-Null
        }
        catch { $preAttestationRejected = $true }
        if (-not $preAttestationRejected -or [IO.File]::Exists($outputPath) -or [IO.File]::Exists($pendingPath)) {
            throw "Screenshot sanitizer accepted review confirmation before creating the sanitized crop."
        }
        & $PSCommandPath -Mode Sanitize -InputImage $inputPath -OutputImage $outputPath `
            -OutputRecord $pendingPath -PreflightRecord $preflightPath -Purpose api-action-result -CropX 4 -CropY 4 `
            -CropWidth 240 -CropHeight 120 | Out-Null
        if (-not [IO.File]::Exists($outputPath) -or -not [IO.File]::Exists($pendingPath) -or
            [IO.File]::Exists($recordPath)) {
            throw "Screenshot sanitizer self-test failed."
        }
        $pending = ConvertFrom-JsonPreservingStrings `
            ([IO.File]::ReadAllText($pendingPath, $script:Utf8NoBom))
        if ($pending.evidenceType -cne "stock-user-chrome-screenshot-review-pending" -or
            $pending.independentVisualReviewRequired -ne $true -or
            $pending.independentVisualReviewCompleted -ne $false -or
            $null -ne $pending.PSObject.Properties["manualVisualReviewConfirmed"] -or
            $pending.reviewStatement -cne $script:PendingReviewStatement) {
            throw "Screenshot sanitizer pre-review record is invalid."
        }
        $missingIndependentReviewRejected = $false
        try {
            & $PSCommandPath -Mode BindReview -PendingRecord $pendingPath -ReviewedImage $outputPath `
                -OutputRecord $recordPath -PreflightRecord $preflightPath | Out-Null
        }
        catch { $missingIndependentReviewRejected = $true }
        if (-not $missingIndependentReviewRejected -or [IO.File]::Exists($recordPath)) {
            throw "Screenshot review binding succeeded without an independent review record."
        }

        $executorRef = [String]::new([char]"1", 64)
        $reviewerRef = [String]::new([char]"2", 64)
        $reviewEntries = @()
        $sequence = 0
        foreach ($reviewPurpose in @($script:ExpectedScreenshotsV2.Keys)) {
            $sequence += 1
            $reviewImage = $script:ExpectedScreenshotsV2[$reviewPurpose]
            $entrySha = if ($reviewPurpose -ceq "api-action-result") {
                [string]$pending.image.sha256
            }
            else { [String]::new([char](48 + $sequence), 64) }
            $entryWidth = if ($reviewPurpose -ceq "api-action-result") { [int64]$pending.image.width } else { 240 }
            $entryHeight = if ($reviewPurpose -ceq "api-action-result") { [int64]$pending.image.height } else { 120 }
            $reviewEntries += [ordered]@{
                sequence = $sequence
                purpose = $reviewPurpose
                image = $reviewImage
                sha256 = $entrySha
                width = $entryWidth
                height = $entryHeight
                requiredVisibleStateSha256 = Get-TextSha256 $script:RequiredVisibleStatesV2[$reviewPurpose]
                digestMatched = $true
                requiredStateVerdict = "pass"
                sensitivePixelsObserved = $false
                uncertain = $false
            }
        }
        $review = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-independent-visual-review"
            releaseCandidateBinding = $pending.releaseCandidateBinding
            candidateBinding = $pending.candidateBinding
            executorSessionRef = $executorRef
            reviewerSessionRef = $reviewerRef
            independentSessionBoundary = $true
            requestSha256 = [String]::new([char]"3", 64)
            reviewedAtUtc = [DateTime]::UtcNow.ToString("o")
            entries = $reviewEntries
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
        $reviewPath = [IO.Path]::Combine($root, "independent-visual-review.json")
        foreach ($negative in @(
            "same-session", "reordered", "uncertain", "sensitive", "digest",
            "string-session-boundary", "string-sequence", "string-entry-bool",
            "string-aggregate-count", "string-aggregate-bool", "legacy-human-field"
        )) {
            $invalidReview = ConvertFrom-JsonPreservingStrings ($review | ConvertTo-Json -Depth 12)
            switch ($negative) {
                "same-session" { $invalidReview.reviewerSessionRef = $invalidReview.executorSessionRef }
                "reordered" { $invalidReview.entries[0].sequence = 2 }
                "uncertain" { $invalidReview.entries[1].uncertain = $true }
                "sensitive" { $invalidReview.entries[1].sensitivePixelsObserved = $true }
                "digest" { $invalidReview.entries[1].sha256 = [String]::new([char]"f", 64) }
                "string-session-boundary" { $invalidReview.independentSessionBoundary = "True" }
                "string-sequence" { $invalidReview.entries[0].sequence = "1" }
                "string-entry-bool" { $invalidReview.entries[0].digestMatched = "True" }
                "string-aggregate-count" { $invalidReview.aggregate.reviewedCropCount = "6" }
                "string-aggregate-bool" {
                    $invalidReview.aggregate.noUncertaintyReported = "True"
                }
                "legacy-human-field" {
                    $invalidReview | Add-Member -NotePropertyName humanVisualReview -NotePropertyValue $true
                }
            }
            $invalidReviewPath = [IO.Path]::Combine($root, "invalid-review-$negative.json")
            [IO.File]::WriteAllText(
                $invalidReviewPath, (($invalidReview | ConvertTo-Json -Depth 12) + "`n"), $script:Utf8NoBom
            )
            $invalidRejected = $false
            try {
                & $PSCommandPath -Mode BindReview -PendingRecord $pendingPath -ReviewedImage $outputPath `
                    -IndependentReviewRecord $invalidReviewPath -OutputRecord $recordPath `
                    -PreflightRecord $preflightPath | Out-Null
            }
            catch { $invalidRejected = $true }
            if (-not $invalidRejected -or [IO.File]::Exists($recordPath)) {
                throw "Screenshot review binding accepted the $negative negative fixture."
            }
        }
        [IO.File]::WriteAllText($reviewPath, (($review | ConvertTo-Json -Depth 12) + "`n"), $script:Utf8NoBom)
        & $PSCommandPath -Mode BindReview -PendingRecord $pendingPath -ReviewedImage $outputPath `
            -IndependentReviewRecord $reviewPath -OutputRecord $recordPath -PreflightRecord $preflightPath | Out-Null
        if (-not [IO.File]::Exists($recordPath)) {
            throw "Screenshot independent review binding was not created."
        }
        $record = ConvertFrom-JsonPreservingStrings `
            ([IO.File]::ReadAllText($recordPath, $script:Utf8NoBom))
        if ($record.image.width -ne 240 -or $record.image.height -ne 120 -or
            $record.sourceCapture.name -cne "browser-02-api-action-result.raw.png" -or
            $record.sourceCapture.endpoint -cne "/api/computer/screenshot" -or
            $record.sourceCapture.sha256 -cne (Get-Sha256 $inputPath) -or
            $record.independentVisualReviewRequired -ne $true -or
            $record.independentVisualReviewCompleted -ne $true -or
            $record.reviewRecordSha256 -cne (Get-Sha256 $reviewPath) -or
            $null -ne $record.PSObject.Properties["manualVisualReviewConfirmed"] -or
            $record.automaticPixelRedactionPerformed -ne $false -or
            $record.reviewStatement -cne $script:CompletedReviewStatement -or
            $record.image.sha256 -cne $pending.image.sha256 -or
            $record.candidateBinding.preflightRecordSha256 -cne (Get-Sha256 $preflightPath)) {
            throw "Screenshot sanitizer self-test record is invalid."
        }
        if ([IO.File]::ReadAllText($recordPath, $script:Utf8NoBom).Contains($root)) {
            throw "Screenshot sanitizer persisted a filesystem path."
        }

        $legacyPreflightPath = [IO.Path]::Combine($root, "candidate-preflight-v0.12.2.json")
        $legacyPreflight = ConvertFrom-JsonPreservingStrings ($preflight | ConvertTo-Json -Depth 8)
        $legacyPreflight.candidate.version = "0.12.2"
        $legacyPreflight.PSObject.Properties.Remove("releaseCandidateBinding")
        [IO.File]::WriteAllText(
            $legacyPreflightPath, (($legacyPreflight | ConvertTo-Json -Depth 8) + "`n"), $script:Utf8NoBom
        )
        $legacyOutputPath = [IO.Path]::Combine($root, "browser-06-action-result.png")
        $legacyRecordPath = [IO.Path]::Combine($root, "browser-06-action-result.json")
        $legacyReviewRequired = $false
        try {
            & $PSCommandPath -Mode Sanitize -InputImage $inputPath -OutputImage $legacyOutputPath `
                -OutputRecord $legacyRecordPath -PreflightRecord $legacyPreflightPath -Purpose action-result `
                -CropX 4 -CropY 4 -CropWidth 240 -CropHeight 120 | Out-Null
        }
        catch { $legacyReviewRequired = $true }
        if (-not $legacyReviewRequired -or [IO.File]::Exists($legacyOutputPath) -or
            [IO.File]::Exists($legacyRecordPath)) {
            throw "Legacy v0.12.2 screenshot sanitization did not require its historical review switch."
        }
        & $PSCommandPath -Mode Sanitize -InputImage $inputPath -OutputImage $legacyOutputPath `
            -OutputRecord $legacyRecordPath -PreflightRecord $legacyPreflightPath -Purpose action-result `
            -CropX 4 -CropY 4 -CropWidth 240 -CropHeight 120 -LegacyV0122ReviewConfirmed | Out-Null
        $legacyRecord = ConvertFrom-JsonPreservingStrings `
            ([IO.File]::ReadAllText($legacyRecordPath, $script:Utf8NoBom))
        if ($legacyRecord.evidenceType -cne "stock-user-chrome-screenshot" -or
            $legacyRecord.manualVisualReviewConfirmed -ne $true -or
            $legacyRecord.reviewStatement -cne $script:LegacyReviewStatement) {
            throw "Legacy v0.12.2 screenshot sanitization compatibility failed."
        }
        $canonicalHash = '"' + ("0123456789abcdef" * 4) + '"'
        $canonicalPendingType = '"stock-user-chrome-screenshot-review-pending"'
        $canonicalPendingLookalike = '"stock-user-chrome-screenshot-review-pending-extra"'
        $tokenShape = '"' + [String]::new([char]"A", 43) + '"'
        if (-not (Test-ForbiddenText $canonicalHash @()) -or
            (Test-ForbiddenText $canonicalHash @() -AllowCanonicalBindingHex) -or
            -not (Test-ForbiddenText $canonicalPendingType @()) -or
            (Test-ForbiddenText $canonicalPendingType @() -AllowCanonicalEvidenceType) -or
            -not (Test-ForbiddenText $canonicalPendingLookalike @() -AllowCanonicalEvidenceType) -or
            -not (Test-ForbiddenText $tokenShape @()) -or
            -not (Test-ForbiddenText $tokenShape @() -AllowCanonicalBindingHex -AllowCanonicalEvidenceType)) {
            throw "Screenshot sanitizer secret/hash distinction self-test failed."
        }
        Write-Output "Browser screenshot sanitizer self-test passed."
    }
    finally {
        if ([IO.Directory]::Exists($root)) {
            [IO.Directory]::Delete($root, $true)
        }
    }
}

switch ($Mode) {
    "Sanitize" { Invoke-Sanitize }
    "BindReview" { Invoke-BindReview }
    "SelfTest" { Invoke-SelfTest }
}
