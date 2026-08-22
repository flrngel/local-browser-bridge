#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Sanitize", "AttestReview", "SelfTest")]
    [string]$Mode,

    [string]$InputImage,
    [string]$OutputImage,
    [string]$OutputRecord,
    [string]$PreflightRecord,
    [string]$PendingRecord,
    [string]$ReviewedImage,

    [ValidateSet(
        "extensions-card", "extension-details", "popup-connected", "native-debugger-warning", "page-control-pill", "action-result",
        "stop-after", "stop-paused-popup", "cancel-after", "cancel-paused-popup",
        "resume-active", "extension-loaded", "api-action-result", "computer-share-action"
    )]
    [string]$Purpose,

    [int]$CropX = -1,
    [int]$CropY = -1,
    [int]$CropWidth = -1,
    [int]$CropHeight = -1,
    [string]$DenyValuesFile,
    [switch]$ManualVisualReviewConfirmed
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
$script:LegacyReviewStatement = "A human reviewed this tight crop; OCR is supplemental and unknown sensitive pixels are not automatically redacted."
$script:PendingReviewStatement = "Sanitization completed; a human has not yet reviewed this crop, and OCR is supplemental."
$script:CompletedReviewStatement = "A human reviewed this tight crop after sanitization; OCR is supplemental and unknown sensitive pixels are not automatically redacted."
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
    return $resolved
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
    return $resolved
}

function Get-Sha256 {
    param([string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-CandidateBindingFromPreflight {
    param([string]$Path)
    $resolved = Resolve-RequiredFile $Path "PreflightRecord"
    $item = [IO.FileInfo]::new($resolved)
    if ($item.Length -le 0 -or $item.Length -gt 1MB) {
        throw "PreflightRecord has an invalid size."
    }
    $bytes = [IO.File]::ReadAllBytes($resolved)
    try {
        $record = $script:Utf8NoBom.GetString($bytes) | ConvertFrom-Json
        $actual = @($record.PSObject.Properties.Name)
        $expected = @("schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed", "runNonce", "candidate")
        if (($actual -join "`n") -cne ($expected -join "`n") -or
            $record.schemaVersion -ne 1 -or
            $record.evidenceType -cne "stock-user-chrome-candidate-binding" -or
            $record.phase -cne "preflight" -or $record.passed -ne $true -or
            [string]$record.runNonce -cnotmatch '^[0-9a-f]{64}$' -or
            [string]$record.candidate.finalSha -cnotmatch '^[0-9a-f]{40}$' -or
            @("0.12.2", "0.12.3") -cnotcontains [string]$record.candidate.version) {
            throw "PreflightRecord identity is invalid."
        }
        foreach ($value in @(
            [string]$record.candidate.checksumManifest.sha256,
            [string]$record.candidate.server.sha256,
            $(if ([string]$record.candidate.version -ceq "0.12.3") { [string]$record.candidate.computerHelper.sha256 } else { [string]$record.candidate.server.sha256 }),
            [string]$record.candidate.extension.sha256,
            [string]$record.candidate.extension.combinedPayloadSha256
        )) {
            if ($value -cnotmatch '^[0-9a-f]{64}$') {
                throw "PreflightRecord candidate hash is invalid."
            }
        }
        $script:CandidateVersionFromPreflight = [string]$record.candidate.version
        $binding = [ordered]@{
            runNonce = [string]$record.runNonce
            preflightRecordSha256 = Get-Sha256 $resolved
            finalSha = [string]$record.candidate.finalSha
            checksumManifestSha256 = [string]$record.candidate.checksumManifest.sha256
            serverSha256 = [string]$record.candidate.server.sha256
        }
        if ($script:CandidateVersionFromPreflight -ceq "0.12.3") {
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

function Get-PngChunkTypes {
    param([string]$Path)
    $bytes = [IO.File]::ReadAllBytes($Path)
    $signature = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    if ($bytes.Length -lt 33) {
        throw "PNG is too small."
    }
    for ($index = 0; $index -lt $signature.Length; $index += 1) {
        if ($bytes[$index] -ne $signature[$index]) {
            throw "Screenshot must be a PNG."
        }
    }
    $types = @()
    $offset = 8
    $sawEnd = $false
    while ($offset -lt $bytes.Length) {
        if ($bytes.Length - $offset -lt 12) {
            throw "PNG chunk framing is invalid."
        }
        $length = ([uint32]$bytes[$offset] -shl 24) -bor
            ([uint32]$bytes[$offset + 1] -shl 16) -bor
            ([uint32]$bytes[$offset + 2] -shl 8) -bor
            [uint32]$bytes[$offset + 3]
        if ($length -gt $script:MaxInputBytes -or ([uint64]$offset + 12 + $length) -gt $bytes.Length) {
            throw "PNG chunk length is invalid."
        }
        $type = [Text.Encoding]::ASCII.GetString($bytes, $offset + 4, 4)
        if (-not [regex]::IsMatch($type, '^[A-Za-z]{4}$')) {
            throw "PNG chunk type is invalid."
        }
        $types += $type
        $offset += 12 + [int]$length
        if ($type -ceq "IEND") {
            if ($length -ne 0 -or $offset -ne $bytes.Length) {
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
    [IO.File]::WriteAllText($TemporaryPath, "$json`n", $script:Utf8NoBom)
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

function Read-StrictJson {
    param([string]$Path, [string]$Label)
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -le 0 -or $bytes.Length -gt 2MB) {
        throw "$Label has an invalid size."
    }
    try { return $script:Utf8NoBom.GetString($bytes) | ConvertFrom-Json }
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
    if ($legacyOnePhase -and -not $ManualVisualReviewConfirmed) {
        throw "ManualVisualReviewConfirmed is mandatory for v0.12.2 compatibility."
    }
    if (-not $legacyOnePhase -and $ManualVisualReviewConfirmed) {
        throw "ManualVisualReviewConfirmed is valid only for AttestReview after a human has inspected the sanitized PNG."
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
    try {
        $inputStream = [IO.File]::Open($inputPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
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
                        throw "v0.12.3 InputImage must use the canonical raw helper-capture filename."
                    }
                    $sourceCapture = [ordered]@{
                        name = $expectedRawName
                        endpoint = "/api/computer/screenshot"
                        bytes = $inputInfo.Length
                        sha256 = Get-Sha256 $inputPath
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
                    $bitmap.Save($temporaryImage, [Drawing.Imaging.ImageFormat]::Png)
                }
                finally { $bitmap.Dispose() }
            }
            finally { $source.Dispose() }
        }
        finally { $inputStream.Dispose() }

        $temporaryInfo = [IO.FileInfo]::new($temporaryImage)
        if ($temporaryInfo.Length -le 0 -or $temporaryInfo.Length -gt $script:MaxInputBytes) {
            throw "Sanitized PNG has an invalid size."
        }
        $chunkTypes = @(Get-PngChunkTypes $temporaryImage)
        foreach ($forbidden in $script:ForbiddenPngChunks) {
            if ($chunkTypes -ccontains $forbidden) {
                throw "Sanitized PNG retained a forbidden metadata chunk."
            }
        }

        $ocrCommand = Get-Command tesseract.exe, tesseract -ErrorAction SilentlyContinue | Select-Object -First 1
        $ocrAvailable = $null -ne $ocrCommand
        $ocrChecked = $false
        if ($ocrAvailable) {
            $ocrLines = & $ocrCommand.Source $temporaryImage stdout --psm 6 2>$null
            if ($LASTEXITCODE -ne 0) {
                throw "Optional OCR was available but failed."
            }
            $ocrChecked = $true
            $ocrText = @($ocrLines) -join "`n"
            if (Test-ForbiddenText $ocrText $denyValues) {
                throw "OCR found denylisted or sensitive-looking text; no screenshot was retained."
            }
        }

        $record = [ordered]@{
            schemaVersion = 1
            evidenceType = if ($legacyOnePhase) { "stock-user-chrome-screenshot" } else { "stock-user-chrome-screenshot-review-pending" }
            purpose = $Purpose
            candidateBinding = $candidateBinding
            image = [ordered]@{
                name = [IO.Path]::GetFileName($outputPath)
                bytes = ([IO.FileInfo]::new($temporaryImage)).Length
                sha256 = Get-Sha256 $temporaryImage
                width = $width
                height = $height
            }
            cropApplied = $hasCrop
            metadataStrippedByDecodeAndReencode = $true
            forbiddenMetadataChunksPresent = $false
            ocrAvailable = $ocrAvailable
            ocrDenylistChecked = $ocrChecked
            ocrDenylistMatches = 0
            manualVisualReviewConfirmed = $legacyOnePhase
            automaticPixelRedactionPerformed = $false
            unknownPixelSafetyClaimed = $false
            reviewStatement = if ($legacyOnePhase) { $script:LegacyReviewStatement } else { $script:PendingReviewStatement }
        }
        if (-not $legacyOnePhase) {
            $record.Insert(4, "sourceCapture", $sourceCapture)
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
            Write-Output "Screenshot was sanitized with review pending; pause and obtain human review before AttestReview."
        }
    }
    finally {
        foreach ($temporary in @($temporaryImage, $temporaryRecord)) {
            if ([IO.File]::Exists($temporary)) {
                [IO.File]::Delete($temporary)
            }
        }
    }
}

function Invoke-AttestReview {
    foreach ($item in @(
        @($PendingRecord, "PendingRecord"), @($ReviewedImage, "ReviewedImage"),
        @($OutputRecord, "OutputRecord"), @($PreflightRecord, "PreflightRecord")
    )) {
        Assert-RequiredArgument $item[0] $item[1]
    }
    if (-not $ManualVisualReviewConfirmed) {
        throw "AttestReview requires action-time confirmation after a human inspected the sanitized PNG."
    }
    $pendingPath = Resolve-RequiredFile $PendingRecord "PendingRecord"
    $imagePath = Resolve-RequiredFile $ReviewedImage "ReviewedImage"
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
    if ($script:CandidateVersionFromPreflight -cne "0.12.3") {
        throw "AttestReview is available only for the v0.12.3 two-phase screenshot protocol."
    }
    $pending = Read-StrictJson $pendingPath "PendingRecord"
    $fields = @(
        "schemaVersion", "evidenceType", "purpose", "candidateBinding", "sourceCapture", "image", "cropApplied",
        "metadataStrippedByDecodeAndReencode", "forbiddenMetadataChunksPresent", "ocrAvailable",
        "ocrDenylistChecked", "ocrDenylistMatches", "manualVisualReviewConfirmed",
        "automaticPixelRedactionPerformed", "unknownPixelSafetyClaimed", "reviewStatement"
    )
    Assert-ExactKeys $pending $fields "pending screenshot record"
    if ($pending.schemaVersion -ne 1 -or
        $pending.evidenceType -cne "stock-user-chrome-screenshot-review-pending" -or
        $pending.purpose -isnot [string] -or -not $script:ExpectedScreenshotsV2.Contains($pending.purpose)) {
        throw "PendingRecord identity is invalid."
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
        "forbiddenMetadataChunksPresent", "manualVisualReviewConfirmed",
        "automaticPixelRedactionPerformed", "unknownPixelSafetyClaimed"
    )) {
        if ($pending.$name -isnot [bool] -or $pending.$name -ne $false) {
            throw "PendingRecord $name must be false."
        }
    }
    if ($pending.ocrAvailable -isnot [bool] -or $pending.ocrDenylistChecked -isnot [bool] -or
        $pending.ocrDenylistChecked -ne $pending.ocrAvailable -or
        $pending.ocrDenylistMatches -ne 0 -or $pending.reviewStatement -cne $script:PendingReviewStatement) {
        throw "PendingRecord sanitization or pending-review state is invalid."
    }
    $imageInfo = [IO.FileInfo]::new($imagePath)
    if ($imageInfo.Length -ne [int64]$pending.image.bytes -or
        (Get-Sha256 $imagePath) -cne $pending.image.sha256) {
        throw "The sanitized PNG changed after its pending review record was created."
    }
    $chunkTypes = @(Get-PngChunkTypes $imagePath)
    foreach ($forbidden in $script:ForbiddenPngChunks) {
        if ($chunkTypes -ccontains $forbidden) {
            throw "The reviewed sanitized PNG contains forbidden metadata."
        }
    }
    Add-Type -AssemblyName System.Drawing -ErrorAction Stop
    $imageStream = [IO.File]::Open($imagePath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
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

    $record = [ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-screenshot"
        purpose = [string]$pending.purpose
        candidateBinding = $candidateBinding
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
        ocrAvailable = [bool]$pending.ocrAvailable
        ocrDenylistChecked = [bool]$pending.ocrDenylistChecked
        ocrDenylistMatches = 0
        manualVisualReviewConfirmed = $true
        automaticPixelRedactionPerformed = $false
        unknownPixelSafetyClaimed = $false
        reviewStatement = $script:CompletedReviewStatement
    }
    $record.Insert(4, "sourceCapture", [ordered]@{
        name = [string]$pending.sourceCapture.name
        endpoint = [string]$pending.sourceCapture.endpoint
        bytes = [int64]$pending.sourceCapture.bytes
        sha256 = [string]$pending.sourceCapture.sha256
        width = [int64]$pending.sourceCapture.width
        height = [int64]$pending.sourceCapture.height
    })
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
    Write-Output "Human review was attested after sanitization; the reviewed PNG hash is unchanged."
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
            candidate = [ordered]@{
                version = "0.12.3"
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
                -CropX 4 -CropY 4 -CropWidth 240 -CropHeight 120 -ManualVisualReviewConfirmed | Out-Null
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
        $pending = [IO.File]::ReadAllText($pendingPath, $script:Utf8NoBom) | ConvertFrom-Json
        if ($pending.evidenceType -cne "stock-user-chrome-screenshot-review-pending" -or
            $pending.manualVisualReviewConfirmed -ne $false -or
            $pending.reviewStatement -cne $script:PendingReviewStatement) {
            throw "Screenshot sanitizer pre-review record is invalid."
        }
        $missingHumanReviewRejected = $false
        try {
            & $PSCommandPath -Mode AttestReview -PendingRecord $pendingPath -ReviewedImage $outputPath `
                -OutputRecord $recordPath -PreflightRecord $preflightPath | Out-Null
        }
        catch { $missingHumanReviewRejected = $true }
        if (-not $missingHumanReviewRejected -or [IO.File]::Exists($recordPath)) {
            throw "Screenshot review attestation succeeded without later human confirmation."
        }
        & $PSCommandPath -Mode AttestReview -PendingRecord $pendingPath -ReviewedImage $outputPath `
            -OutputRecord $recordPath -PreflightRecord $preflightPath -ManualVisualReviewConfirmed | Out-Null
        if (-not [IO.File]::Exists($recordPath)) {
            throw "Screenshot post-sanitization review attestation was not created."
        }
        $record = [IO.File]::ReadAllText($recordPath, $script:Utf8NoBom) | ConvertFrom-Json
        if ($record.image.width -ne 240 -or $record.image.height -ne 120 -or
            $record.sourceCapture.name -cne "browser-02-api-action-result.raw.png" -or
            $record.sourceCapture.endpoint -cne "/api/computer/screenshot" -or
            $record.sourceCapture.sha256 -cne (Get-Sha256 $inputPath) -or
            $record.manualVisualReviewConfirmed -ne $true -or
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
        $legacyPreflight = $preflight | ConvertTo-Json -Depth 8 | ConvertFrom-Json
        $legacyPreflight.candidate.version = "0.12.2"
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
            -CropX 4 -CropY 4 -CropWidth 240 -CropHeight 120 -ManualVisualReviewConfirmed | Out-Null
        $legacyRecord = [IO.File]::ReadAllText($legacyRecordPath, $script:Utf8NoBom) | ConvertFrom-Json
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
    "AttestReview" { Invoke-AttestReview }
    "SelfTest" { Invoke-SelfTest }
}
