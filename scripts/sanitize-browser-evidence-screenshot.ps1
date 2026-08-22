#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Sanitize", "SelfTest")]
    [string]$Mode,

    [string]$InputImage,
    [string]$OutputImage,
    [string]$OutputRecord,
    [string]$PreflightRecord,

    [ValidateSet(
        "extensions-card", "extension-details", "popup-connected", "native-debugger-warning", "page-control-pill", "action-result",
        "stop-after", "stop-paused-popup", "cancel-after", "cancel-paused-popup",
        "resume-active"
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
            [string]$record.candidate.finalSha -cnotmatch '^[0-9a-f]{40}$') {
            throw "PreflightRecord identity is invalid."
        }
        foreach ($value in @(
            [string]$record.candidate.checksumManifest.sha256,
            [string]$record.candidate.server.sha256,
            [string]$record.candidate.extension.sha256,
            [string]$record.candidate.extension.combinedPayloadSha256
        )) {
            if ($value -cnotmatch '^[0-9a-f]{64}$') {
                throw "PreflightRecord candidate hash is invalid."
            }
        }
        return [ordered]@{
            runNonce = [string]$record.runNonce
            preflightRecordSha256 = Get-Sha256 $resolved
            finalSha = [string]$record.candidate.finalSha
            checksumManifestSha256 = [string]$record.candidate.checksumManifest.sha256
            serverSha256 = [string]$record.candidate.server.sha256
            extensionZipSha256 = [string]$record.candidate.extension.sha256
            extractedPayloadSha256 = [string]$record.candidate.extension.combinedPayloadSha256
        }
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
        [switch]$AllowCanonicalBindingHex
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

function Invoke-Sanitize {
    foreach ($item in @(
        @($InputImage, "InputImage"), @($OutputImage, "OutputImage"),
        @($OutputRecord, "OutputRecord"), @($PreflightRecord, "PreflightRecord"), @($Purpose, "Purpose")
    )) {
        Assert-RequiredArgument $item[0] $item[1]
    }
    if (-not $ManualVisualReviewConfirmed) {
        throw "ManualVisualReviewConfirmed is mandatory; automation cannot identify every sensitive pixel."
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
            evidenceType = "stock-user-chrome-screenshot"
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
            manualVisualReviewConfirmed = $true
            automaticPixelRedactionPerformed = $false
            unknownPixelSafetyClaimed = $false
            reviewStatement = "A human reviewed this tight crop; OCR is supplemental and unknown sensitive pixels are not automatically redacted."
        }
        $serialized = $record | ConvertTo-Json -Depth 12 -Compress
        if (Test-ForbiddenText $serialized $denyValues -AllowCanonicalBindingHex) {
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
        Write-Output "Screenshot was metadata-stripped and validated; manual review remains the privacy authority."
    }
    finally {
        foreach ($temporary in @($temporaryImage, $temporaryRecord)) {
            if ([IO.File]::Exists($temporary)) {
                [IO.File]::Delete($temporary)
            }
        }
    }
}

function Invoke-SelfTest {
    Add-Type -AssemblyName System.Drawing -ErrorAction Stop
    $root = [IO.Path]::Combine([IO.Path]::GetTempPath(), "lbb-browser-shot-" + [Guid]::NewGuid().ToString("N"))
    [IO.Directory]::CreateDirectory($root) | Out-Null
    try {
        $inputPath = [IO.Path]::Combine($root, "input.png")
        $outputPath = [IO.Path]::Combine($root, "page-control-pill.png")
        $recordPath = [IO.Path]::Combine($root, "page-control-pill.json")
        $bitmap = [Drawing.Bitmap]::new(320, 180)
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.Clear([Drawing.Color]::FromArgb(255, 20, 90, 160))
            }
            finally { $graphics.Dispose() }
            $bitmap.Save($inputPath, [Drawing.Imaging.ImageFormat]::Png)
        }
        finally { $bitmap.Dispose() }
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
                finalSha = [String]::new([char]"0", 40)
                checksumManifest = [ordered]@{ sha256 = $bindingHash }
                server = [ordered]@{ sha256 = $bindingHash }
                extension = [ordered]@{ sha256 = $bindingHash; combinedPayloadSha256 = $bindingHash }
            }
        }
        [IO.File]::WriteAllText($preflightPath, (($preflight | ConvertTo-Json -Depth 8) + "`n"), $script:Utf8NoBom)
        & $PSCommandPath -Mode Sanitize -InputImage $inputPath -OutputImage $outputPath `
            -OutputRecord $recordPath -PreflightRecord $preflightPath -Purpose page-control-pill -CropX 4 -CropY 4 `
            -CropWidth 240 -CropHeight 120 -ManualVisualReviewConfirmed | Out-Null
        if (-not [IO.File]::Exists($outputPath) -or -not [IO.File]::Exists($recordPath)) {
            throw "Screenshot sanitizer self-test failed."
        }
        $record = [IO.File]::ReadAllText($recordPath, $script:Utf8NoBom) | ConvertFrom-Json
        if ($record.image.width -ne 240 -or $record.image.height -ne 120 -or
            $record.manualVisualReviewConfirmed -ne $true -or
            $record.automaticPixelRedactionPerformed -ne $false -or
            $record.candidateBinding.preflightRecordSha256 -cne (Get-Sha256 $preflightPath)) {
            throw "Screenshot sanitizer self-test record is invalid."
        }
        if ([IO.File]::ReadAllText($recordPath, $script:Utf8NoBom).Contains($root)) {
            throw "Screenshot sanitizer persisted a filesystem path."
        }
        $canonicalHash = '"' + ("0123456789abcdef" * 4) + '"'
        $tokenShape = '"' + [String]::new([char]"A", 43) + '"'
        if (-not (Test-ForbiddenText $canonicalHash @()) -or
            (Test-ForbiddenText $canonicalHash @() -AllowCanonicalBindingHex) -or
            -not (Test-ForbiddenText $tokenShape @()) -or
            -not (Test-ForbiddenText $tokenShape @() -AllowCanonicalBindingHex)) {
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
    "SelfTest" { Invoke-SelfTest }
}
