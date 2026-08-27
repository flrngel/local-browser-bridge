[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+(?:[-.][0-9A-Za-z.-]+)?$')]
    [string]$Version,

    [string]$ServerPath = "dist/local-browser-bridge-v$Version-windows-x86_64.exe",
    [string]$HelperPath = "dist/local-computer-helper-v$Version-windows-x86_64.exe",

    [string]$ChecksumManifestPath = "",

    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-WindowsSdkTool {
    param([Parameter(Mandatory = $true)][string]$Name)

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    $kitsRoot = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
    $candidate = Get-ChildItem -LiteralPath $kitsRoot -Filter $Name -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } |
        Sort-Object FullName |
        Select-Object -Last 1
    if ($null -eq $candidate) {
        throw "Could not locate $Name in PATH or the Windows SDK."
    }
    return $candidate.FullName
}

function Resolve-DumpBin {
    $command = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
        throw 'Could not locate vswhere.exe to resolve dumpbin.exe.'
    }
    $installation = (& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -Last 1)
    if ([string]::IsNullOrWhiteSpace($installation)) {
        throw 'Could not locate a Visual C++ toolchain containing dumpbin.exe.'
    }
    $candidate = Get-ChildItem -LiteralPath "$installation\VC\Tools\MSVC" -Filter dumpbin.exe -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\bin\\Hostx64\\x64\\dumpbin\.exe$' } |
        Sort-Object FullName |
        Select-Object -Last 1
    if ($null -eq $candidate) {
        throw 'Could not locate the x64 dumpbin.exe tool.'
    }
    return $candidate.FullName
}

function Assert-PeX64 {
    param([Parameter(Mandatory = $true)][string]$Path)

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 0x40 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
        throw "$Path is not a PE executable with an MZ header."
    }
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    if ($peOffset -lt 0 -or $peOffset + 6 -gt $bytes.Length) {
        throw "$Path has an invalid PE header offset."
    }
    if ([BitConverter]::ToUInt32($bytes, $peOffset) -ne 0x00004550) {
        throw "$Path has an invalid PE signature."
    }
    if ([BitConverter]::ToUInt16($bytes, $peOffset + 4) -ne 0x8664) {
        throw "$Path is not an x86_64 PE executable."
    }
}

function Assert-PeSubsystem {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][uint16]$ExpectedSubsystem
    )

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    $optionalHeader = $peOffset + 24
    if ($optionalHeader + 70 -gt $bytes.Length -or
        [BitConverter]::ToUInt16($bytes, $optionalHeader) -ne 0x20b) {
        throw "$Path does not contain a valid PE32+ optional header."
    }
    $actual = [BitConverter]::ToUInt16($bytes, $optionalHeader + 68)
    if ($actual -ne $ExpectedSubsystem) {
        throw "$Path uses PE subsystem $actual, expected $ExpectedSubsystem."
    }
}

function Get-FileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)

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

function Write-ChecksumManifest {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][object[]]$Entries
    )

    $resolved = [IO.Path]::GetFullPath($Path)
    $parent = [IO.Path]::GetDirectoryName($resolved)
    if ([string]::IsNullOrWhiteSpace($parent) -or -not [IO.Directory]::Exists($parent)) {
        throw "The checksum manifest parent directory does not exist."
    }
    $lines = @($Entries | ForEach-Object {
        "$($_.Sha256)  $([IO.Path]::GetFileName($_.Path))"
    })
    if ($lines.Count -ne $Entries.Count) {
        throw "The checksum manifest inventory is incomplete."
    }

    $stream = [IO.File]::Open(
        $resolved, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
    )
    $writer = $null
    try {
        $writer = [IO.StreamWriter]::new($stream, [Text.Encoding]::ASCII)
        foreach ($line in $lines) { $writer.WriteLine($line) }
        $writer.Flush()
    }
    finally {
        if ($null -ne $writer) { $writer.Dispose() } else { $stream.Dispose() }
    }
}

if ($SelfTest) {
    $selfTestRoot = Join-Path ([IO.Path]::GetTempPath()) "lbb-windows-artifact-self-test-$([guid]::NewGuid().ToString('N'))"
    [IO.Directory]::CreateDirectory($selfTestRoot) | Out-Null
    $probePath = Join-Path $selfTestRoot "probe.bin"
    $manifestPath = Join-Path $selfTestRoot "SHA256SUMS.txt"
    try {
        [IO.File]::WriteAllBytes($probePath, [byte[]](0x61, 0x62, 0x63))
        $expectedSha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        $actualSha256 = Get-FileSha256 $probePath
        if ($actualSha256 -cne $expectedSha256) {
            throw "The artifact verifier SHA-256 helper failed its canonical test vector."
        }
        $entry = [pscustomobject]@{ Path = $probePath; Sha256 = $actualSha256 }
        Write-ChecksumManifest -Path $manifestPath -Entries @($entry)
        $expectedManifest = "$expectedSha256  probe.bin$([Environment]::NewLine)"
        if ([IO.File]::ReadAllText($manifestPath, [Text.Encoding]::ASCII) -cne $expectedManifest) {
            throw "The artifact verifier wrote a noncanonical checksum manifest."
        }
        $overwriteRefused = $false
        try {
            Write-ChecksumManifest -Path $manifestPath -Entries @($entry)
        }
        catch [IO.IOException] {
            $overwriteRefused = $true
        }
        if (-not $overwriteRefused) {
            throw "The artifact verifier checksum writer did not refuse an existing manifest."
        }
        Write-Output "Windows artifact verifier self-test passed."
        exit 0
    }
    finally {
        if ([IO.File]::Exists($manifestPath)) { [IO.File]::Delete($manifestPath) }
        if ([IO.File]::Exists($probePath)) { [IO.File]::Delete($probePath) }
        if ([IO.Directory]::Exists($selfTestRoot)) { [IO.Directory]::Delete($selfTestRoot, $false) }
    }
}

function Assert-Manifest {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$MtPath
    )

    $manifestPath = Join-Path ([IO.Path]::GetTempPath()) "lbb-$([guid]::NewGuid().ToString('N')).manifest"
    try {
        & $MtPath -nologo "-inputresource:$Path;#1" "-out:$manifestPath"
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
            throw "$Path does not contain an extractable RT_MANIFEST resource."
        }
        $manifest = Get-Content -Raw -LiteralPath $manifestPath
        foreach ($required in @(
            'level="asInvoker"',
            'uiAccess="false"',
            '>PerMonitorV2</dpiAwareness>',
            '>true</longPathAware>',
            '{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}'
        )) {
            if (-not $manifest.Contains($required)) {
                throw "$Path manifest is missing required declaration: $required"
            }
        }
    }
    finally {
        Remove-Item -LiteralPath $manifestPath -Force -ErrorAction SilentlyContinue
    }
}

function Assert-StaticCrt {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$DumpBinPath
    )

    $imports = (& $DumpBinPath /nologo /dependents $Path 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin failed while inspecting $Path."
    }
    if ($imports -match '(?im)^\s*(?:VCRUNTIME|MSVCP|api-ms-win-crt)[^\s]*\.dll\s*$') {
        throw "$Path still imports a separately distributed Visual C++ runtime."
    }
}

function Assert-HelperInputSurface {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$DumpBinPath
    )

    $imports = (& $DumpBinPath /nologo /imports $Path 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin failed while inspecting the helper input API surface."
    }
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $ascii = [System.Text.Encoding]::ASCII.GetString($bytes)
    $utf16 = [System.Text.Encoding]::Unicode.GetString($bytes)
    $reports = @($imports, $ascii, $utf16)

    foreach ($api in @(
        'SendInput',
        'SetCursorPos',
        'SetPhysicalCursorPos',
        'mouse_event',
        'keybd_event',
        'SetForegroundWindow',
        'AttachThreadInput',
        'AllowSetForegroundWindow',
        'LockSetForegroundWindow',
        'SwitchToThisWindow'
    )) {
        $pattern = '(?m)(^|[^A-Za-z0-9_])_?' + [Regex]::Escape($api) + '([^A-Za-z0-9_]|$)'
        if ($reports.Where({ $_ -match $pattern }).Count -ne 0) {
            throw "Windows helper contains forbidden global input API: $api"
        }
    }

    foreach ($api in @(
        'PostMessageW',
        'GetCursorPos',
        'OpenInputDesktop',
        'GetUserObjectInformationW',
        'RegisterRawInputDevices',
        'SetWindowsHookExW'
    )) {
        $pattern = '(?m)(^|[^A-Za-z0-9_])_?' + [Regex]::Escape($api) + '([^A-Za-z0-9_]|$)'
        if ($reports.Where({ $_ -match $pattern }).Count -eq 0) {
            throw "Windows helper is missing required target-route or invariant API: $api"
        }
    }
}

function Assert-VersionResource {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedVersion,
        [Parameter(Mandatory = $true)][string]$ExpectedDescription,
        [Parameter(Mandatory = $true)][string]$ExpectedOriginalFilename
    )

    $versionInfo = (Get-Item -LiteralPath $Path).VersionInfo
    $expected = [ordered]@{
        FileVersion = $ExpectedVersion
        ProductVersion = $ExpectedVersion
        ProductName = 'Local Browser Bridge'
        CompanyName = 'Local Browser Bridge contributors'
        FileDescription = $ExpectedDescription
        InternalName = [IO.Path]::GetFileNameWithoutExtension($ExpectedOriginalFilename)
        OriginalFilename = $ExpectedOriginalFilename
    }
    foreach ($field in $expected.Keys) {
        $actual = [string]$versionInfo.$field
        if ($actual -cne $expected[$field]) {
            throw "$Path has unexpected VERSIONINFO $field metadata: '$actual' (expected '$($expected[$field])')."
        }
    }

    $coreVersion = ($ExpectedVersion -split '[-+]', 2)[0]
    $expectedFixedVersion = @($coreVersion.Split('.') | ForEach-Object { [int]$_ }) + @(0)
    if ($expectedFixedVersion.Count -ne 4) {
        throw "Expected version '$ExpectedVersion' cannot be represented as a four-part Windows fixed version."
    }
    $actualFileFixedVersion = @(
        $versionInfo.FileMajorPart,
        $versionInfo.FileMinorPart,
        $versionInfo.FileBuildPart,
        $versionInfo.FilePrivatePart
    )
    $actualProductFixedVersion = @(
        $versionInfo.ProductMajorPart,
        $versionInfo.ProductMinorPart,
        $versionInfo.ProductBuildPart,
        $versionInfo.ProductPrivatePart
    )
    $expectedFixed = $expectedFixedVersion -join '.'
    $actualFixedVersions = @(
        ($actualFileFixedVersion -join '.'),
        ($actualProductFixedVersion -join '.')
    )
    foreach ($actualFixed in $actualFixedVersions) {
        if ($actualFixed -cne $expectedFixed) {
            throw "$Path has unexpected fixed VERSIONINFO metadata: '$actualFixed' (expected '$expectedFixed')."
        }
    }
}

$mt = Resolve-WindowsSdkTool -Name 'mt.exe'
$dumpbin = Resolve-DumpBin
$resolvedHelperPath = (Resolve-Path -LiteralPath $HelperPath).Path
Assert-PeX64 -Path $resolvedHelperPath
Assert-PeSubsystem -Path $resolvedHelperPath -ExpectedSubsystem 3
Assert-HelperInputSurface -Path $resolvedHelperPath -DumpBinPath $dumpbin
$artifacts = @(
    [pscustomobject]@{
        Path = $ServerPath
        ExpectedVersion = "local-browser-bridge-desktop $Version"
        Description = 'Local Browser Bridge Desktop Host'
        OriginalFilename = 'local-browser-bridge-desktop.exe'
        Subsystem = 2
    },
    [pscustomobject]@{
        Path = $HelperPath
        ExpectedVersion = "local-computer-helper $Version"
        Description = 'Local Browser Bridge Computer Helper'
        OriginalFilename = 'local-computer-helper.exe'
        Subsystem = 3
    }
)

$results = foreach ($artifact in $artifacts) {
    $resolved = (Resolve-Path -LiteralPath $artifact.Path).Path
    Assert-PeX64 -Path $resolved
    Assert-PeSubsystem -Path $resolved -ExpectedSubsystem $artifact.Subsystem
    $reportedVersion = (& $resolved --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $reportedVersion -ne $artifact.ExpectedVersion) {
        throw "$resolved reported an unexpected version: $reportedVersion"
    }
    $licenseReport = (& $resolved --licenses | Out-String)
    if ($LASTEXITCODE -ne 0 -or
        -not $licenseReport.Contains('Local Browser Bridge third-party licenses') -or
        -not $licenseReport.Contains('MIT License') -or
        -not $licenseReport.Contains('Apache License') -or
        $licenseReport.Contains('option-ext') -or
        $licenseReport.Contains('Mozilla Public License') -or
        $licenseReport.Contains('/Users/') -or
        $licenseReport.Contains('\Users\')) {
        throw "$resolved does not expose the expected sanitized project and dependency licenses."
    }
    Assert-Manifest -Path $resolved -MtPath $mt
    Assert-VersionResource -Path $resolved -ExpectedVersion $Version -ExpectedDescription $artifact.Description -ExpectedOriginalFilename $artifact.OriginalFilename
    Assert-StaticCrt -Path $resolved -DumpBinPath $dumpbin
    [pscustomobject]@{
        Path = $resolved
        Version = $reportedVersion
        Sha256 = Get-FileSha256 $resolved
        Manifest = 'asInvoker; uiAccess=false; PerMonitorV2; longPathAware'
        VersionResource = "$($artifact.Description); FileVersion=$Version; ProductVersion=$Version"
        Runtime = 'static CRT'
        Licenses = '--licenses embedded'
    }
}

if (-not [string]::IsNullOrWhiteSpace($ChecksumManifestPath)) {
    Write-ChecksumManifest -Path $ChecksumManifestPath -Entries $results
}
$results | Format-Table -AutoSize
