#requires -Version 5.1

[CmdletBinding(DefaultParameterSetName = "Run")]
param(
    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+$')]
    [string]$Version,

    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$ServerPath,

    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$HelperPath,

    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$ChecksumManifest,

    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidatePattern('^[0-9A-Fa-f]{64}$')]
    [string]$ChecksumManifestSha256,

    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$CandidateBindingPath,

    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$FixturePath,

    [Parameter(ParameterSetName = "Run", Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$EvidenceDirectory,

    [string]$Token,

    [ValidateSet("Smoke", "Capture", "Recovery", "Semantic", "Keyboard", "Pixel", "Cancellation", "All")]
    [string[]]$Suite = @("Smoke"),

    [ValidateRange(0, 65535)]
    [int]$Port = 0,

    [ValidateRange(10, 180)]
    [int]$TimeoutSeconds = 45,

    [ValidateRange(15, 300)]
    [int]$ForegroundArmTimeoutSeconds = 300,

    [switch]$ShowOccluder,

    [Parameter(ParameterSetName = "SelfTest", Mandatory = $true)]
    [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "The Windows acceptance runner can run only on Windows."
}
if (-not $SelfTest -and (
        $PSVersionTable.PSEdition -cne "Desktop" -or
        $PSVersionTable.PSVersion.Major -ne 5 -or
        $PSVersionTable.PSVersion.Minor -lt 1
    )) {
    throw "Live Windows acceptance requires Windows PowerShell 5.1 (Desktop edition); PowerShell 7 remains a parser and runner-self-test surface only."
}

Add-Type -AssemblyName System.Net.Http -ErrorAction Stop

function Resolve-RequiredFile {
    param([string]$Path, [string]$Label)
    $resolved = [IO.Path]::GetFullPath($Path)
    if (-not [IO.File]::Exists($resolved)) {
        throw "$Label does not exist."
    }
    $downloadZone = Get-Item -LiteralPath $resolved -Stream "Zone.Identifier" -ErrorAction SilentlyContinue
    if ($null -ne $downloadZone) {
        throw "$Label carries Windows download-zone metadata. Inspect the file and resolve any Windows warning manually; this runner never bypasses it."
    }
    return $resolved
}

function Test-BridgeToken {
    param([string]$Value)
    if ([String]::IsNullOrEmpty($Value) -or $Value.Length -ne 43 -or $Value -notmatch '^[A-Za-z0-9_-]{43}$') {
        return $false
    }
    try {
        $base64 = $Value.Replace('-', '+').Replace('_', '/') + "="
        $bytes = [Convert]::FromBase64String($base64)
        if ($bytes.Length -ne 32) {
            return $false
        }
        return (@($bytes | Select-Object -Unique).Count -ge 16)
    }
    catch {
        return $false
    }
}

function Get-BoundedReportedVersion {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Path
    $startInfo.Arguments = "--version"
    $startInfo.WorkingDirectory = [IO.Path]::GetDirectoryName($Path)
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) {
            throw "$Label did not start for its bounded version query."
        }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(5000)) {
            $process.Kill()
            $process.WaitForExit()
            throw "$Label did not finish its bounded version query."
        }
        $reported = $stdout.GetAwaiter().GetResult().Trim()
        $errorText = $stderr.GetAwaiter().GetResult().Trim()
        if ($process.ExitCode -ne 0 -or -not [String]::IsNullOrEmpty($errorText)) {
            throw "$Label failed its bounded version query."
        }
        return $reported
    }
    finally {
        $process.Dispose()
    }
}

function Read-ExactCandidateChecksums {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string[]]$ExpectedNames
    )
    $entries = [Collections.Generic.Dictionary[string, string]]::new([StringComparer]::Ordinal)
    $lines = [IO.File]::ReadAllLines($Path)
    if ($lines.Count -ne $ExpectedNames.Count) {
        throw "ChecksumManifest must contain exactly the four canonical release assets."
    }
    foreach ($line in $lines) {
        $match = [Text.RegularExpressions.Regex]::Match(
            $line,
            '^(?<hash>[0-9A-Fa-f]{64})[\x20\t]+[*]?(?<name>[^\\/:]+)$',
            [Text.RegularExpressions.RegexOptions]::CultureInvariant
        )
        if (-not $match.Success) {
            throw "ChecksumManifest contains a malformed entry."
        }
        $name = $match.Groups['name'].Value
        if ($name -cnotin $ExpectedNames -or $entries.ContainsKey($name)) {
            throw "ChecksumManifest contains an unexpected or duplicate asset entry."
        }
        $entries.Add($name, $match.Groups['hash'].Value.ToLowerInvariant())
    }
    foreach ($name in $ExpectedNames) {
        if (-not $entries.ContainsKey($name)) {
            throw "ChecksumManifest is missing a canonical release asset entry."
        }
    }
    return ,$entries
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

function Assert-ExactJsonProperties {
    param(
        [object]$Value,
        [string[]]$Expected,
        [string]$Label
    )
    if ($null -eq $Value) {
        throw "$Label is absent."
    }
    $actual = @($Value.PSObject.Properties | ForEach-Object { $_.Name })
    if ($actual.Count -ne $Expected.Count) {
        throw "$Label does not contain the exact property count."
    }
    for ($index = 0; $index -lt $Expected.Count; $index++) {
        if ($actual[$index] -cne $Expected[$index]) {
            throw "$Label property order or spelling is not canonical."
        }
    }
}

function Read-ExactReleaseCandidateBinding {
    param(
        [string]$Path,
        [string]$ExpectedVersion,
        [string]$ExpectedManifestSha256,
        [Collections.Generic.Dictionary[string, string]]$ExpectedChecksums,
        [string[]]$ExpectedAssetNames
    )
    if ([IO.Path]::GetFileName($Path) -cne "candidate-binding.json") {
        throw "CandidateBindingPath must use the canonical candidate-binding.json filename."
    }
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 2 -or $bytes.Length -gt 262144 -or
        ($bytes.Length -ge 3 -and $bytes[0] -eq 0xef -and $bytes[1] -eq 0xbb -and $bytes[2] -eq 0xbf)) {
        throw "CandidateBindingPath has an invalid size or encoding marker."
    }
    try {
        $utf8 = [Text.UTF8Encoding]::new($false, $true)
        $binding = $utf8.GetString($bytes) | ConvertFrom-Json
    }
    catch {
        throw "CandidateBindingPath is not strict UTF-8 JSON."
    }
    finally {
        [Array]::Clear($bytes, 0, $bytes.Length)
    }
    $bindingFields = @(
        "schemaVersion", "productVersion", "repository", "tag", "sourceSha",
        "tagObjectSha", "workflowRunId", "workflowRunAttempt", "artifactId",
        "artifactName", "artifactZipBytes", "artifactZipSha256",
        "checksumManifestSha256", "attestationInvocationUri", "attestedAssetCount",
        "githubHostedRunner", "assets", "passed"
    )
    Assert-ExactJsonProperties $binding $bindingFields "release candidate binding"
    if (($binding.schemaVersion -isnot [int] -and $binding.schemaVersion -isnot [long]) -or
        [int64]$binding.schemaVersion -ne 1 -or
        $binding.productVersion -cne $ExpectedVersion -or
        $binding.repository -cne "flrngel/local-browser-bridge" -or
        $binding.tag -cne "v$ExpectedVersion" -or
        [string]$binding.sourceSha -cnotmatch '^[0-9a-f]{40}$' -or
        [string]$binding.tagObjectSha -cnotmatch '^[0-9a-f]{40}$' -or
        [string]$binding.workflowRunId -cnotmatch '^[1-9][0-9]*$' -or
        [string]$binding.workflowRunAttempt -cnotmatch '^[1-9][0-9]*$' -or
        [string]$binding.artifactId -cnotmatch '^[1-9][0-9]*$' -or
        $binding.artifactName -cne "release-candidate" -or
        ($binding.artifactZipBytes -isnot [int] -and $binding.artifactZipBytes -isnot [long]) -or
        [int64]$binding.artifactZipBytes -lt 1 -or [int64]$binding.artifactZipBytes -gt 536870912 -or
        [string]$binding.artifactZipSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $binding.checksumManifestSha256 -cne $ExpectedManifestSha256 -or
        $binding.attestationInvocationUri -cne (
            "https://github.com/flrngel/local-browser-bridge/actions/runs/" +
            [string]$binding.workflowRunId + "/attempts/" + [string]$binding.workflowRunAttempt
        ) -or
        ($binding.attestedAssetCount -isnot [int] -and $binding.attestedAssetCount -isnot [long]) -or
        [int64]$binding.attestedAssetCount -ne 5 -or
        $binding.githubHostedRunner -ne $true -or $binding.passed -ne $true) {
        throw "CandidateBindingPath does not bind the exact frozen workflow candidate."
    }
    $expectedFiles = @($ExpectedAssetNames) + "SHA256SUMS.txt"
    $assets = @($binding.assets)
    if ($assets.Count -ne $expectedFiles.Count) {
        throw "CandidateBindingPath does not contain the exact five-file asset inventory."
    }
    $normalizedAssets = New-Object Collections.Generic.List[object]
    for ($index = 0; $index -lt $expectedFiles.Count; $index++) {
        $asset = $assets[$index]
        Assert-ExactJsonProperties $asset @("file", "bytes", "sha256") "release candidate asset"
        $expectedSha256 = if ($index -lt $ExpectedAssetNames.Count) {
            $ExpectedChecksums[$ExpectedAssetNames[$index]]
        }
        else {
            $ExpectedManifestSha256
        }
        if ($asset.file -cne $expectedFiles[$index] -or
            ($asset.bytes -isnot [int] -and $asset.bytes -isnot [long]) -or
            [int64]$asset.bytes -lt 1 -or [int64]$asset.bytes -gt 536870912 -or
            $asset.sha256 -cne $expectedSha256) {
            throw "CandidateBindingPath contains a mismatched asset fact."
        }
        $normalizedAssets.Add([ordered]@{
            file = [string]$asset.file
            bytes = [int64]$asset.bytes
            sha256 = [string]$asset.sha256
        })
    }
    return [ordered]@{
        productVersion = [string]$binding.productVersion
        repository = [string]$binding.repository
        tag = [string]$binding.tag
        sourceSha = [string]$binding.sourceSha
        tagObjectSha = [string]$binding.tagObjectSha
        workflowRunId = [string]$binding.workflowRunId
        workflowRunAttempt = [string]$binding.workflowRunAttempt
        artifactId = [string]$binding.artifactId
        artifactName = [string]$binding.artifactName
        artifactZipBytes = [int64]$binding.artifactZipBytes
        artifactZipSha256 = [string]$binding.artifactZipSha256
        checksumManifestSha256 = [string]$binding.checksumManifestSha256
        attestationInvocationUri = [string]$binding.attestationInvocationUri
        attestedAssetCount = [int64]$binding.attestedAssetCount
        githubHostedRunner = $true
        assets = $normalizedAssets.ToArray()
    }
}

function Get-VerifiedCandidateArtifact {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedName,
        [Parameter(Mandatory = $true)][string]$ExpectedSha256,
        [Parameter(Mandatory = $true)][string]$ExpectedReportedVersion,
        [Parameter(Mandatory = $true)][string]$Label
    )
    if ([IO.Path]::GetFileName($Path) -cne $ExpectedName) {
        throw "$Label must use the canonical frozen-candidate filename."
    }
    $actualSha256 = Get-FileSha256 $Path
    if ($actualSha256 -cne $ExpectedSha256) {
        throw "$Label does not match its exact ChecksumManifest entry."
    }
    $versionInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo($Path)
    if ($versionInfo.FileVersion -cne $Version -or $versionInfo.ProductVersion -cne $Version) {
        throw "$Label VERSIONINFO does not match Version $Version."
    }
    $reportedVersion = Get-BoundedReportedVersion -Path $Path -Label $Label
    if ($reportedVersion -cne $ExpectedReportedVersion) {
        throw "$Label --version output does not match Version $Version."
    }
    return [ordered]@{
        name = $ExpectedName
        bytes = ([IO.FileInfo]::new($Path)).Length
        sha256 = $actualSha256
        checksumManifestMatched = $true
        fileVersion = $versionInfo.FileVersion
        productVersion = $versionInfo.ProductVersion
        reportedVersion = $reportedVersion
        versionMatched = $true
        pathRecorded = $false
    }
}

if (-not $SelfTest) {
    $environmentToken = [Environment]::GetEnvironmentVariable("LBB_TOKEN", "Process")
    # Consume the inherited secret immediately even when an in-memory -Token was
    # supplied. This keeps the fixture and every other non-bridge child from
    # inheriting a stale or duplicate bearer token.
    [Environment]::SetEnvironmentVariable("LBB_TOKEN", $null, "Process")
    if ([String]::IsNullOrWhiteSpace($Token)) {
        $Token = $environmentToken
    }
    $environmentToken = $null
    if (-not (Test-BridgeToken $Token)) {
        throw "Token must be a canonical, high-entropy 43-character Local Browser Bridge token."
    }

    $resolvedServer = Resolve-RequiredFile $ServerPath "ServerPath"
    $resolvedHelper = Resolve-RequiredFile $HelperPath "HelperPath"
    $resolvedChecksumManifest = Resolve-RequiredFile $ChecksumManifest "ChecksumManifest"
    $resolvedCandidateBinding = Resolve-RequiredFile $CandidateBindingPath "CandidateBindingPath"
    $resolvedFixture = Resolve-RequiredFile $FixturePath "FixturePath"
    $evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)

    if ([IO.Path]::GetFileName($resolvedChecksumManifest) -cne "SHA256SUMS.txt") {
        throw "ChecksumManifest must use the canonical SHA256SUMS.txt filename."
    }
    $manifestSha256 = Get-FileSha256 $resolvedChecksumManifest
    if ($manifestSha256 -cne $ChecksumManifestSha256.ToLowerInvariant()) {
        throw "ChecksumManifest does not match the externally recorded frozen-candidate SHA-256."
    }
    $expectedServerName = "local-browser-bridge-v$Version-windows-x86_64.exe"
    $expectedHelperName = "local-computer-helper-v$Version-windows-x86_64.exe"
    $expectedCandidateNames = @(
        $expectedServerName,
        $expectedHelperName,
        "local-browser-bridge-v$Version-macos-universal.tar.gz",
        "local-browser-bridge-extension-v$Version.zip"
    )
    $candidateChecksums = Read-ExactCandidateChecksums -Path $resolvedChecksumManifest -ExpectedNames $expectedCandidateNames
    $releaseCandidateBinding = Read-ExactReleaseCandidateBinding `
        -Path $resolvedCandidateBinding `
        -ExpectedVersion $Version `
        -ExpectedManifestSha256 $manifestSha256 `
        -ExpectedChecksums $candidateChecksums `
        -ExpectedAssetNames $expectedCandidateNames
    $candidateBinding = [ordered]@{
        version = $Version
        checksumManifestMatched = $true
        exactAssetSetMatched = $true
        checksumManifest = [ordered]@{
            name = "SHA256SUMS.txt"
            bytes = ([IO.FileInfo]::new($resolvedChecksumManifest)).Length
            sha256 = $manifestSha256
            expectedSha256 = $ChecksumManifestSha256.ToLowerInvariant()
            exactEntryCount = $candidateChecksums.Count
            pathRecorded = $false
        }
        server = Get-VerifiedCandidateArtifact `
            -Path $resolvedServer `
            -ExpectedName $expectedServerName `
            -ExpectedSha256 $candidateChecksums[$expectedServerName] `
            -ExpectedReportedVersion "local-browser-bridge $Version" `
            -Label "ServerPath"
        helper = Get-VerifiedCandidateArtifact `
            -Path $resolvedHelper `
            -ExpectedName $expectedHelperName `
            -ExpectedSha256 $candidateChecksums[$expectedHelperName] `
            -ExpectedReportedVersion "local-computer-helper $Version" `
            -Label "HelperPath"
    }
    if ([int64]$releaseCandidateBinding.assets[0].bytes -ne [int64]$candidateBinding.server.bytes -or
        [int64]$releaseCandidateBinding.assets[1].bytes -ne [int64]$candidateBinding.helper.bytes -or
        [int64]$releaseCandidateBinding.assets[4].bytes -ne [int64]$candidateBinding.checksumManifest.bytes) {
        throw "CandidateBindingPath byte facts do not match the supplied Windows candidate files."
    }

    if ([IO.Directory]::Exists($evidenceRoot)) {
        if (@([IO.Directory]::EnumerateFileSystemEntries($evidenceRoot)).Count -ne 0) {
            throw "EvidenceDirectory must be new or empty; existing evidence is never overwritten."
        }
    }
    else {
        [IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null
    }

    $fixtureEvidence = [IO.Path]::Combine($evidenceRoot, "fixture")
    $stepEvidence = [IO.Path]::Combine($evidenceRoot, "steps")
    $screenshotEvidence = [IO.Path]::Combine($evidenceRoot, "screenshots")
    $operatorEvidence = [IO.Path]::Combine($evidenceRoot, "operator")
    [IO.Directory]::CreateDirectory($fixtureEvidence) | Out-Null
    [IO.Directory]::CreateDirectory($stepEvidence) | Out-Null
    [IO.Directory]::CreateDirectory($screenshotEvidence) | Out-Null
    [IO.Directory]::CreateDirectory($operatorEvidence) | Out-Null
}

$probeSource = @'
using System;
using System.Collections;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace LbbWindowsAcceptance
{
    public sealed class ProbeSnapshot
    {
        public long ForegroundHwnd;
        public long FocusHwnd;
        public int CursorX;
        public int CursorY;
        public bool CursorAvailable;
        public string InputDesktop;
    }

    public static class NativeProbe
    {
        private const uint DESKTOP_READOBJECTS = 0x0001;
        private const int UOI_NAME = 2;
        private const uint TH32CS_SNAPPROCESS = 0x00000002;
        private const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x00001000;
        private const int ERROR_INVALID_PARAMETER = 87;
        private const int ERROR_NO_MORE_FILES = 18;
        private const uint SYNCHRONIZE = 0x00100000;
        private const uint WAIT_OBJECT_0 = 0x00000000;
        private const uint WAIT_TIMEOUT = 0x00000102;
        private static readonly IntPtr InvalidHandle = new IntPtr(-1);

        [StructLayout(LayoutKind.Sequential)]
        private struct POINT
        {
            internal int X;
            internal int Y;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct GUITHREADINFO
        {
            internal int cbSize;
            internal uint flags;
            internal IntPtr hwndActive;
            internal IntPtr hwndFocus;
            internal IntPtr hwndCapture;
            internal IntPtr hwndMenuOwner;
            internal IntPtr hwndMoveSize;
            internal IntPtr hwndCaret;
            internal RECT rcCaret;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct RECT
        {
            internal int Left;
            internal int Top;
            internal int Right;
            internal int Bottom;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct PROCESSENTRY32
        {
            internal uint Size;
            internal uint Usage;
            internal uint ProcessId;
            internal IntPtr DefaultHeapId;
            internal uint ModuleId;
            internal uint Threads;
            internal uint ParentProcessId;
            internal int BasePriority;
            internal uint Flags;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)]
            internal string ExeFile;
        }

        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr window, IntPtr processId);

        [DllImport("user32.dll", EntryPoint = "GetWindowThreadProcessId")]
        private static extern uint GetWindowThreadProcessIdWithOwner(IntPtr window, out uint processId);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsWindow(IntPtr window);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsChild(IntPtr parent, IntPtr child);

        [DllImport("user32.dll")]
        private static extern IntPtr GetAncestor(IntPtr window, uint flags);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetGUIThreadInfo(uint threadId, ref GUITHREADINFO info);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetCursorPos(out POINT point);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint desiredAccess);

        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetUserObjectInformation(IntPtr handle, int index, StringBuilder info, uint length, out uint needed);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseDesktop(IntPtr desktop);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint processId);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool Process32First(IntPtr snapshot, ref PROCESSENTRY32 entry);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool Process32Next(IntPtr snapshot, ref PROCESSENTRY32 entry);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr OpenProcess(uint desiredAccess, [MarshalAs(UnmanagedType.Bool)] bool inheritHandle, uint processId);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool QueryFullProcessImageName(IntPtr process, uint flags, StringBuilder imagePath, ref uint size);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool ProcessIdToSessionId(uint processId, out uint sessionId);

        [DllImport("kernel32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseHandle(IntPtr value);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr OpenEvent(uint desiredAccess, [MarshalAs(UnmanagedType.Bool)] bool inheritHandle, string name);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

        public static bool HasInteractiveInputDesktop()
        {
            IntPtr desktop = OpenInputDesktop(0, false, DESKTOP_READOBJECTS);
            if (desktop == IntPtr.Zero)
            {
                return false;
            }
            CloseDesktop(desktop);
            return true;
        }

        public static ProbeSnapshot Capture()
        {
            ProbeSnapshot output = new ProbeSnapshot();
            IntPtr foreground = GetForegroundWindow();
            output.ForegroundHwnd = foreground.ToInt64();
            GUITHREADINFO info = new GUITHREADINFO();
            info.cbSize = Marshal.SizeOf(typeof(GUITHREADINFO));
            uint threadId = GetWindowThreadProcessId(foreground, IntPtr.Zero);
            if (threadId != 0 && GetGUIThreadInfo(threadId, ref info))
            {
                output.FocusHwnd = info.hwndFocus.ToInt64();
            }
            POINT cursor;
            if (GetCursorPos(out cursor))
            {
                output.CursorX = cursor.X;
                output.CursorY = cursor.Y;
                output.CursorAvailable = true;
            }
            output.InputDesktop = ReadInputDesktopName();
            return output;
        }

        public static bool ValidateFixtureArmTopology(long sentinelHwnd, long armButtonHwnd, int expectedProcessId)
        {
            if (sentinelHwnd <= 0 || armButtonHwnd <= 0 || expectedProcessId <= 0)
            {
                return false;
            }
            IntPtr sentinel = new IntPtr(sentinelHwnd);
            IntPtr button = new IntPtr(armButtonHwnd);
            if (!IsWindow(sentinel) || !IsWindow(button) || !IsChild(sentinel, button))
            {
                return false;
            }
            const uint GA_ROOT = 2;
            if (GetAncestor(sentinel, GA_ROOT) != sentinel || GetAncestor(button, GA_ROOT) != sentinel)
            {
                return false;
            }
            uint sentinelProcessId;
            uint buttonProcessId;
            uint sentinelThreadId = GetWindowThreadProcessIdWithOwner(sentinel, out sentinelProcessId);
            uint buttonThreadId = GetWindowThreadProcessIdWithOwner(button, out buttonProcessId);
            return sentinelThreadId != 0 &&
                buttonThreadId == sentinelThreadId &&
                sentinelProcessId == (uint)expectedProcessId &&
                buttonProcessId == (uint)expectedProcessId;
        }

        public static int[] GetDirectChildProcessIds(int parentProcessId, string expectedImagePath)
        {
            if (parentProcessId <= 0 || String.IsNullOrWhiteSpace(expectedImagePath))
            {
                throw new ArgumentException("A positive parent PID and exact helper image path are required");
            }
            string expectedPath = Path.GetFullPath(expectedImagePath);
            string expectedName = Path.GetFileName(expectedPath);
            List<int> output = new List<int>();
            IntPtr snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if (snapshot == InvalidHandle)
            {
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not inspect runner-owned helper descendants");
            }
            try
            {
                PROCESSENTRY32 entry = new PROCESSENTRY32();
                entry.Size = (uint)Marshal.SizeOf(typeof(PROCESSENTRY32));
                if (!Process32First(snapshot, ref entry))
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not enumerate runner-owned helper descendants");
                }
                do
                {
                    if (entry.ParentProcessId == (uint)parentProcessId &&
                        String.Equals(entry.ExeFile, expectedName, StringComparison.OrdinalIgnoreCase))
                    {
                        string childPath = ReadProcessImagePath(entry.ProcessId);
                        if (childPath != null && String.Equals(Path.GetFullPath(childPath), expectedPath, StringComparison.OrdinalIgnoreCase))
                        {
                            output.Add((int)entry.ProcessId);
                        }
                    }
                    entry.Size = (uint)Marshal.SizeOf(typeof(PROCESSENTRY32));
                }
                while (Process32Next(snapshot, ref entry));
                int enumerationError = Marshal.GetLastWin32Error();
                if (enumerationError != ERROR_NO_MORE_FILES)
                {
                    throw new Win32Exception(enumerationError, "Could not finish enumerating runner-owned helper descendants");
                }
                return output.ToArray();
            }
            finally
            {
                CloseHandle(snapshot);
            }
        }

        public static int GetProcessSessionId(int processId)
        {
            uint processSessionId;
            if (processId <= 0 || !ProcessIdToSessionId((uint)processId, out processSessionId))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not inspect the helper worker session");
            }
            return checked((int)processSessionId);
        }

        private static string ReadProcessImagePath(uint processId)
        {
            IntPtr process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, processId);
            if (process == IntPtr.Zero)
            {
                int error = Marshal.GetLastWin32Error();
                if (error == ERROR_INVALID_PARAMETER)
                {
                    return null;
                }
                throw new Win32Exception(error, "Could not open a runner-owned helper descendant for image verification");
            }
            try
            {
                uint size = 32768;
                StringBuilder imagePath = new StringBuilder((int)size);
                if (!QueryFullProcessImageName(process, 0, imagePath, ref size))
                {
                    int error = Marshal.GetLastWin32Error();
                    if (error == ERROR_INVALID_PARAMETER)
                    {
                        return null;
                    }
                    throw new Win32Exception(error, "Could not verify a runner-owned helper descendant image");
                }
                return imagePath.ToString();
            }
            finally
            {
                CloseHandle(process);
            }
        }

        public static int GetKernelEventState(string name)
        {
            IntPtr eventHandle = OpenEvent(SYNCHRONIZE, false, name);
            if (eventHandle == IntPtr.Zero)
            {
                return 0;
            }
            try
            {
                uint result = WaitForSingleObject(eventHandle, 0);
                if (result == WAIT_OBJECT_0) { return 2; }
                if (result == WAIT_TIMEOUT) { return 1; }
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not inspect the local one-shot recovery event");
            }
            finally
            {
                CloseHandle(eventHandle);
            }
        }

        private static string ReadInputDesktopName()
        {
            IntPtr desktop = OpenInputDesktop(0, false, DESKTOP_READOBJECTS);
            if (desktop == IntPtr.Zero)
            {
                return "unavailable";
            }
            try
            {
                uint needed;
                GetUserObjectInformation(desktop, UOI_NAME, null, 0, out needed);
                if (needed == 0)
                {
                    return "unavailable";
                }
                StringBuilder buffer = new StringBuilder((int)(needed / 2) + 1);
                if (!GetUserObjectInformation(desktop, UOI_NAME, buffer, needed, out needed))
                {
                    return "unavailable";
                }
                return buffer.ToString();
            }
            finally
            {
                CloseDesktop(desktop);
            }
        }
    }

    public sealed class OwnedProcessJob : IDisposable
    {
        private const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
        private const int JobObjectBasicAccountingInformation = 1;
        private const int JobObjectExtendedLimitInformation = 9;
        private const uint CREATE_SUSPENDED = 0x00000004;
        private const uint CREATE_UNICODE_ENVIRONMENT = 0x00000400;
        private const uint CREATE_NO_WINDOW = 0x08000000;
        private const uint EXTENDED_STARTUPINFO_PRESENT = 0x00080000;
        private const uint STARTF_USESTDHANDLES = 0x00000100;
        private const uint PROC_THREAD_ATTRIBUTE_HANDLE_LIST = 0x00020002;
        private const uint GENERIC_READ = 0x80000000;
        private const uint GENERIC_WRITE = 0x40000000;
        private const uint FILE_SHARE_READ = 0x00000001;
        private const uint FILE_SHARE_WRITE = 0x00000002;
        private const uint OPEN_EXISTING = 3;
        private const int ERROR_INSUFFICIENT_BUFFER = 122;
        private const uint WAIT_OBJECT_0 = 0x00000000;
        private const uint WAIT_TIMEOUT = 0x00000102;
        private const uint WAIT_FAILED = 0xFFFFFFFF;
        private const uint PROCESS_TERMINATION_WAIT_MS = 5000;
        private static readonly IntPtr InvalidHandle = new IntPtr(-1);
        private IntPtr handle;

        [StructLayout(LayoutKind.Sequential)]
        private struct SECURITY_ATTRIBUTES
        {
            internal int Length;
            internal IntPtr SecurityDescriptor;
            [MarshalAs(UnmanagedType.Bool)]
            internal bool InheritHandle;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct STARTUPINFO
        {
            internal int cb;
            internal string Reserved;
            internal string Desktop;
            internal string Title;
            internal int X;
            internal int Y;
            internal int XSize;
            internal int YSize;
            internal int XCountChars;
            internal int YCountChars;
            internal int FillAttribute;
            internal uint Flags;
            internal short ShowWindow;
            internal short Reserved2Count;
            internal IntPtr Reserved2;
            internal IntPtr StdInput;
            internal IntPtr StdOutput;
            internal IntPtr StdError;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct STARTUPINFOEX
        {
            internal STARTUPINFO StartupInfo;
            internal IntPtr AttributeList;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct PROCESS_INFORMATION
        {
            internal IntPtr Process;
            internal IntPtr Thread;
            internal int ProcessId;
            internal int ThreadId;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JOBOBJECT_BASIC_LIMIT_INFORMATION
        {
            internal long PerProcessUserTimeLimit;
            internal long PerJobUserTimeLimit;
            internal uint LimitFlags;
            internal UIntPtr MinimumWorkingSetSize;
            internal UIntPtr MaximumWorkingSetSize;
            internal uint ActiveProcessLimit;
            internal UIntPtr Affinity;
            internal uint PriorityClass;
            internal uint SchedulingClass;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct IO_COUNTERS
        {
            internal ulong ReadOperationCount;
            internal ulong WriteOperationCount;
            internal ulong OtherOperationCount;
            internal ulong ReadTransferCount;
            internal ulong WriteTransferCount;
            internal ulong OtherTransferCount;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION
        {
            internal JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
            internal IO_COUNTERS IoInfo;
            internal UIntPtr ProcessMemoryLimit;
            internal UIntPtr JobMemoryLimit;
            internal UIntPtr PeakProcessMemoryUsed;
            internal UIntPtr PeakJobMemoryUsed;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JOBOBJECT_BASIC_ACCOUNTING_INFORMATION
        {
            internal long TotalUserTime;
            internal long TotalKernelTime;
            internal long ThisPeriodTotalUserTime;
            internal long ThisPeriodTotalKernelTime;
            internal uint TotalPageFaultCount;
            internal uint TotalProcesses;
            internal uint ActiveProcesses;
            internal uint TotalTerminatedProcesses;
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateJobObject(IntPtr attributes, string name);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetInformationJobObject(IntPtr job, int informationClass, ref JOBOBJECT_EXTENDED_LIMIT_INFORMATION information, int length);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool QueryInformationJobObject(IntPtr job, int informationClass, ref JOBOBJECT_BASIC_ACCOUNTING_INFORMATION information, int length, IntPtr returnLength);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool TerminateJobObject(IntPtr job, uint exitCode);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CreateProcess(
            string applicationName,
            StringBuilder commandLine,
            IntPtr processAttributes,
            IntPtr threadAttributes,
            [MarshalAs(UnmanagedType.Bool)] bool inheritHandles,
            uint creationFlags,
            IntPtr environment,
            string currentDirectory,
            ref STARTUPINFOEX startupInfo,
            out PROCESS_INFORMATION processInformation);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool InitializeProcThreadAttributeList(
            IntPtr attributeList,
            int attributeCount,
            int flags,
            ref UIntPtr size);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool UpdateProcThreadAttribute(
            IntPtr attributeList,
            uint flags,
            UIntPtr attribute,
            IntPtr value,
            UIntPtr size,
            IntPtr previousValue,
            IntPtr returnSize);

        [DllImport("kernel32.dll")]
        private static extern void DeleteProcThreadAttributeList(IntPtr attributeList);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateFile(
            string fileName,
            uint desiredAccess,
            uint shareMode,
            ref SECURITY_ATTRIBUTES securityAttributes,
            uint creationDisposition,
            uint flagsAndAttributes,
            IntPtr templateFile);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint ResumeThread(IntPtr thread);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool TerminateProcess(IntPtr process, uint exitCode);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

        [DllImport("kernel32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseHandle(IntPtr value);

        public OwnedProcessJob()
        {
            handle = CreateJobObject(IntPtr.Zero, null);
            if (handle == IntPtr.Zero)
            {
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not create the private acceptance-test Job Object");
            }
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if (!SetInformationJobObject(handle, JobObjectExtendedLimitInformation, ref limits, Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION))))
            {
                int error = Marshal.GetLastWin32Error();
                CloseHandle(handle);
                handle = IntPtr.Zero;
                throw new Win32Exception(error, "Could not configure private process-tree cleanup");
            }
        }

        public int StartProcess(string applicationName, string commandLine, string currentDirectory, IDictionary overrides)
        {
            EnsureOpen();
            IntPtr environment = IntPtr.Zero;
            IntPtr nullHandle = IntPtr.Zero;
            IntPtr inheritedHandleList = IntPtr.Zero;
            IntPtr attributeList = IntPtr.Zero;
            bool attributeListInitialized = false;
            PROCESS_INFORMATION process = new PROCESS_INFORMATION();
            bool suspendedProcessNeedsTermination = false;
            bool processAssignedToJob = false;
            Exception primaryFailure = null;
            try
            {
                environment = BuildEnvironment(overrides);
                SECURITY_ATTRIBUTES security = new SECURITY_ATTRIBUTES();
                security.Length = Marshal.SizeOf(typeof(SECURITY_ATTRIBUTES));
                security.InheritHandle = true;
                nullHandle = CreateFile("NUL", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, ref security, OPEN_EXISTING, 0, IntPtr.Zero);
                if (nullHandle == InvalidHandle)
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not open the null device for sanitized child output");
                }

                UIntPtr attributeListSize = UIntPtr.Zero;
                bool sizingUnexpectedlySucceeded = InitializeProcThreadAttributeList(IntPtr.Zero, 1, 0, ref attributeListSize);
                int sizingError = Marshal.GetLastWin32Error();
                ulong attributeBytes = attributeListSize.ToUInt64();
                if (sizingUnexpectedlySucceeded || sizingError != ERROR_INSUFFICIENT_BUFFER || attributeBytes == 0 || attributeBytes > Int32.MaxValue)
                {
                    throw new Win32Exception(sizingError, "Could not size the restricted child handle list");
                }
                attributeList = Marshal.AllocHGlobal(checked((int)attributeBytes));
                if (!InitializeProcThreadAttributeList(attributeList, 1, 0, ref attributeListSize))
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not initialize the restricted child handle list");
                }
                attributeListInitialized = true;

                inheritedHandleList = Marshal.AllocHGlobal(IntPtr.Size);
                Marshal.WriteIntPtr(inheritedHandleList, nullHandle);
                if (!UpdateProcThreadAttribute(
                    attributeList,
                    0,
                    new UIntPtr(PROC_THREAD_ATTRIBUTE_HANDLE_LIST),
                    inheritedHandleList,
                    new UIntPtr((uint)IntPtr.Size),
                    IntPtr.Zero,
                    IntPtr.Zero))
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not restrict child handle inheritance to the null device");
                }

                STARTUPINFOEX startup = new STARTUPINFOEX();
                startup.StartupInfo.cb = Marshal.SizeOf(typeof(STARTUPINFOEX));
                startup.StartupInfo.Flags = STARTF_USESTDHANDLES;
                startup.StartupInfo.StdInput = nullHandle;
                startup.StartupInfo.StdOutput = nullHandle;
                startup.StartupInfo.StdError = nullHandle;
                startup.AttributeList = attributeList;
                // CreateProcess requires inheritHandles=true for HANDLE_LIST,
                // but the extended attribute restricts inheritance to NUL.
                bool created = CreateProcess(
                    applicationName,
                    new StringBuilder(commandLine),
                    IntPtr.Zero,
                    IntPtr.Zero,
                    true,
                    CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                    environment,
                    currentDirectory,
                    ref startup,
                    out process);
                if (!created)
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "A required acceptance-test process did not start");
                }
                suspendedProcessNeedsTermination = true;
                if (!AssignProcessToJobObject(handle, process.Process))
                {
                    int error = Marshal.GetLastWin32Error();
                    throw new Win32Exception(error, "A child could not be assigned to the private acceptance-test Job Object");
                }
                processAssignedToJob = true;
                if (ResumeThread(process.Thread) == UInt32.MaxValue)
                {
                    int error = Marshal.GetLastWin32Error();
                    throw new Win32Exception(error, "A Job-owned child could not be resumed");
                }
                suspendedProcessNeedsTermination = false;
                return process.ProcessId;
            }
            catch (Exception error)
            {
                primaryFailure = error;
                throw;
            }
            finally
            {
                Exception terminationFailure = null;
                if (suspendedProcessNeedsTermination && process.Process != IntPtr.Zero)
                {
                    terminationFailure = TerminateSuspendedProcess(process.Process, processAssignedToJob);
                }
                if (process.Thread != IntPtr.Zero) { CloseHandle(process.Thread); }
                if (process.Process != IntPtr.Zero) { CloseHandle(process.Process); }
                if (attributeListInitialized) { DeleteProcThreadAttributeList(attributeList); }
                if (attributeList != IntPtr.Zero) { Marshal.FreeHGlobal(attributeList); }
                if (inheritedHandleList != IntPtr.Zero) { Marshal.FreeHGlobal(inheritedHandleList); }
                if (nullHandle != IntPtr.Zero && nullHandle != InvalidHandle) { CloseHandle(nullHandle); }
                if (environment != IntPtr.Zero) { Marshal.FreeHGlobal(environment); }
                if (terminationFailure != null)
                {
                    if (primaryFailure != null)
                    {
                        throw new AggregateException(
                            "A required child launch failed and its suspended-process cleanup also failed",
                            new Exception[] { primaryFailure, terminationFailure });
                    }
                    throw terminationFailure;
                }
            }
        }

        private static Exception TerminateSuspendedProcess(IntPtr process, bool assignedToJob)
        {
            bool terminationRequested = TerminateProcess(process, 1);
            int terminationError = Marshal.GetLastWin32Error();
            uint wait = WaitForSingleObject(process, terminationRequested ? PROCESS_TERMINATION_WAIT_MS : 0);
            if (wait == WAIT_OBJECT_0)
            {
                return null;
            }
            string ownership = assignedToJob ? "Job-owned" : "unassigned";
            if (!terminationRequested)
            {
                return new Win32Exception(
                    terminationError,
                    ownership + " suspended child could not be terminated during failed launch cleanup");
            }
            if (wait == WAIT_TIMEOUT)
            {
                return new TimeoutException(
                    ownership + " suspended child did not exit within the bounded failed-launch cleanup window");
            }
            int waitError = wait == WAIT_FAILED ? Marshal.GetLastWin32Error() : unchecked((int)wait);
            return new Win32Exception(
                waitError,
                ownership + " suspended child termination could not be verified during failed launch cleanup");
        }

        public uint ActiveProcessCount
        {
            get
            {
                EnsureOpen();
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION accounting = new JOBOBJECT_BASIC_ACCOUNTING_INFORMATION();
                if (!QueryInformationJobObject(handle, JobObjectBasicAccountingInformation, ref accounting, Marshal.SizeOf(typeof(JOBOBJECT_BASIC_ACCOUNTING_INFORMATION)), IntPtr.Zero))
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not query private process-tree cleanup state");
                }
                return accounting.ActiveProcesses;
            }
        }

        public void Terminate()
        {
            EnsureOpen();
            if (!TerminateJobObject(handle, 1))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not terminate the private acceptance-test process tree");
            }
        }

        public void Dispose()
        {
            if (handle != IntPtr.Zero)
            {
                CloseHandle(handle);
                handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~OwnedProcessJob()
        {
            Dispose();
        }

        private void EnsureOpen()
        {
            if (handle == IntPtr.Zero)
            {
                throw new ObjectDisposedException("OwnedProcessJob");
            }
        }

        private static IntPtr BuildEnvironment(IDictionary overrides)
        {
            SortedDictionary<string, string> environment = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (DictionaryEntry item in Environment.GetEnvironmentVariables())
            {
                string key = item.Key as string;
                string value = item.Value as string;
                if (!String.IsNullOrEmpty(key) && key.IndexOf('=') < 0 && key.IndexOf('\0') < 0 && value != null && value.IndexOf('\0') < 0)
                {
                    environment[key] = value;
                }
            }
            if (overrides != null)
            {
                foreach (DictionaryEntry item in overrides)
                {
                    string key = item.Key as string;
                    string value = item.Value as string;
                    if (String.IsNullOrEmpty(key) || key.IndexOf('=') >= 0 || key.IndexOf('\0') >= 0 || value == null || value.IndexOf('\0') >= 0)
                    {
                        throw new ArgumentException("A child environment override was invalid");
                    }
                    environment[key] = value;
                }
            }
            StringBuilder block = new StringBuilder();
            foreach (KeyValuePair<string, string> item in environment)
            {
                block.Append(item.Key).Append('=').Append(item.Value).Append('\0');
            }
            block.Append('\0');
            return Marshal.StringToHGlobalUni(block.ToString());
        }
    }
}
'@

$probeNamespace = "LbbWindowsAcceptance_" + [Guid]::NewGuid().ToString("N")
$probeSource = $probeSource.Replace("namespace LbbWindowsAcceptance", "namespace $probeNamespace")
Add-Type -TypeDefinition $probeSource -Language CSharp
$script:nativeProbeType = ("$probeNamespace.NativeProbe" -as [type])
$script:ownedJobType = ("$probeNamespace.OwnedProcessJob" -as [type])
if ($null -eq $script:nativeProbeType -or $null -eq $script:ownedJobType) {
    throw "The isolated Windows acceptance probe types did not load."
}

function Wait-Condition {
    param(
        [scriptblock]$Probe,
        [string]$Description,
        [ValidateRange(1, 180000)]
        [int]$TimeoutMilliseconds = ($TimeoutSeconds * 1000),
        [ValidateRange(0, 10000)]
        [int]$PollMilliseconds = 150
    )
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $value = & $Probe
        if ($null -ne $value -and $value -ne $false) {
            return $value
        }
        if ($PollMilliseconds -gt 0) {
            Start-Sleep -Milliseconds $PollMilliseconds
        }
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Description."
}

function Wait-ForFixtureProof {
    param(
        [scriptblock]$FixturePredicate,
        [string]$Description,
        [scriptblock]$StateReader = { Get-FixtureState },
        [ValidateRange(1, 180000)]
        [int]$TimeoutMilliseconds = ($TimeoutSeconds * 1000),
        [ValidateRange(0, 10000)]
        [int]$PollMilliseconds = 150
    )
    # Keep this bounded loop independent from Wait-Condition. Windows
    # PowerShell uses dynamic variable lookup for invoked scriptblocks, so a
    # nested wrapper with the same predicate parameter name can resolve to
    # itself and recurse until the engine's call-depth limit.
    $timeoutWatch = [Diagnostics.Stopwatch]::StartNew()
    do {
        $state = & $StateReader
        if (& $FixturePredicate $state) {
            $timeoutWatch.Stop()
            return $state
        }
        if ($PollMilliseconds -gt 0) {
            Start-Sleep -Milliseconds $PollMilliseconds
        }
    } while ($timeoutWatch.ElapsedMilliseconds -lt $TimeoutMilliseconds)
    $timeoutWatch.Stop()
    throw "Timed out waiting for $Description."
}

function Write-NewOperatorMarker {
    param(
        [string]$Directory,
        [ValidateSet("foreground-arm-request.json", "foreground-arm-received.json")]
        [string]$FileName,
        [object]$Value
    )
    if (-not [IO.Directory]::Exists($Directory)) {
        throw "The operator-marker directory does not exist."
    }
    $finalPath = [IO.Path]::Combine($Directory, $FileName)
    if ([IO.File]::Exists($finalPath)) {
        throw "Operator markers are create-once for this runner and cannot be overwritten by it."
    }
    $temporaryPath = [IO.Path]::Combine(
        $Directory,
        "." + $FileName + "." + [Guid]::NewGuid().ToString("N") + ".tmp"
    )
    $stream = $null
    try {
        $json = $Value | ConvertTo-Json -Depth 20
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes($json + [Environment]::NewLine)
        $stream = [IO.FileStream]::new(
            $temporaryPath,
            [IO.FileMode]::CreateNew,
            [IO.FileAccess]::Write,
            [IO.FileShare]::None
        )
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
        $stream.Dispose()
        $stream = $null
        # The temporary file and destination share a directory, so observers
        # see either no marker or one complete runner-create-once JSON document.
        [IO.File]::Move($temporaryPath, $finalPath)
    }
    finally {
        if ($null -ne $stream) {
            $stream.Dispose()
        }
        if ([IO.File]::Exists($temporaryPath)) {
            [IO.File]::Delete($temporaryPath)
        }
    }
    return $finalPath
}

function New-ForegroundArmRequestMarker {
    param(
        [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+$')]
        [string]$ProductVersion,
        [ValidatePattern('^[0-9a-f]{32}$')]
        [string]$RequestId,
        [ValidateSet("not-started", "already-acknowledged")]
        [string]$InputStateAtPublication,
        [ValidateRange(15, 300)]
        [int]$TimeoutSeconds,
        [bool]$RequestDelivered,
        [bool]$ButtonEnabled,
        [bool]$NativeTopologyMatched
    )
    if (-not $RequestDelivered -or -not $ButtonEnabled -or -not $NativeTopologyMatched) {
        throw "An operator request marker requires a completed request-delivery proof."
    }
    $operatorActionRequired = $InputStateAtPublication -ceq "not-started"
    return [ordered]@{
        schemaVersion = 2
        productVersion = $ProductVersion
        kind = "foreground-arm"
        status = if ($operatorActionRequired) { "action-required" } else { "already-armed" }
        requestId = $RequestId
        publishedAtUtc = [DateTime]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        timeoutSeconds = $TimeoutSeconds
        operatorActionRequired = $operatorActionRequired
        preferredRelaySurface = "windows-computer-use-app-share"
        fallbackRelaySurface = "human-on-windows-session"
        expectedVisibleWindowTitle = if ($operatorActionRequired) { "LBB Windows Acceptance - ACTION REQUIRED" } else { "LBB Windows Acceptance - ARMED" }
        expectedVisibleButtonText = if ($operatorActionRequired) { "CLICK TO ARM" } else { "ARMED - DO NOT USE THIS SESSION" }
        expectedAccessibleName = "Click to arm Windows acceptance"
        action = if ($operatorActionRequired) { "single-left-click" } else { "none" }
        stopUiAfterAction = $true
        requiresSeparateAuthorization = $true
        markerGrantsAuthorization = $false
        markerGrantsConsent = $false
        externalOneShotConsentRequired = $true
        visualConfirmationRequired = $true
        maximumClickAttempts = if ($operatorActionRequired) { 1 } else { 0 }
        retryOnUnknownOutcome = $false
        instruction = if ($operatorActionRequired) { "Use a separately authorized Windows Computer Use app share to visually confirm this exact window and button, click it once, then stop all UI use. If it already says ARMED or the outcome is uncertain, do not click or retry." } else { "Do not click; stop all UI use because the foreground arm is already acknowledged." }
        requestDelivered = $true
        buttonEnabled = $true
        nativeTopologyMatched = $true
        inputStateAtPublication = $InputStateAtPublication
        notificationOnly = $true
        acceptedAsAuthority = $false
        rawWindowHandlesRecorded = $false
        rawCursorCoordinatesRecorded = $false
        pathsRecorded = $false
        secretsRecorded = $false
    }
}

function New-ForegroundArmReceivedMarker {
    param(
        [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+$')]
        [string]$ProductVersion,
        [ValidatePattern('^[0-9a-f]{32}$')]
        [string]$RequestId,
        [object]$Proof
    )
    $exactClickCountsMatched = (
        $Proof.completed -eq $true -and
        [int]$Proof.fixtureRequestCount -eq 1 -and
        [int]$Proof.fixtureAcknowledgementCount -eq 1 -and
        [int]$Proof.fixtureLeftMouseDownCount -eq 1 -and
        [int]$Proof.fixtureLeftMouseUpCount -eq 1
    )
    $nativeGatesMatched = (
        $Proof.nativeTopologyMatched -eq $true -and
        $Proof.foregroundMatched -eq $true -and
        $Proof.focusMatched -eq $true -and
        $Proof.cursorAvailable -eq $true -and
        $Proof.cursorStable -eq $true -and
        $Proof.inputDesktopAvailable -eq $true -and
        $Proof.inputDesktopStable -eq $true -and
        [int]$Proof.stableSamplesObserved -ge [int]$Proof.stableSamplesRequired
    )
    if (-not $exactClickCountsMatched -or -not $nativeGatesMatched) {
        throw "An operator received marker requires the complete click and stable-native-sample proof."
    }
    return [ordered]@{
        schemaVersion = 2
        productVersion = $ProductVersion
        kind = "foreground-arm"
        status = "received"
        requestId = $RequestId
        receivedAtUtc = [DateTime]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        exactClickCountsMatched = $true
        stableSamplesObserved = [int]$Proof.stableSamplesObserved
        stableSamplesRequired = [int]$Proof.stableSamplesRequired
        nativeTopologyMatched = $true
        foregroundMatched = $true
        focusMatched = $true
        cursorStable = $true
        inputDesktopStable = $true
        notificationOnly = $true
        acceptedAsAuthority = $false
        rawWindowHandlesRecorded = $false
        rawCursorCoordinatesRecorded = $false
        pathsRecorded = $false
        secretsRecorded = $false
    }
}

$script:foregroundArmProof = $null

function Test-ForegroundArmRequestDeliveryState {
    param(
        [object]$State,
        [ValidateRange(1, 2147483647)]
        [int]$RequestedGeneration,
        [Int64]$ArmButtonHwnd
    )
    if ($null -eq $State -or $ArmButtonHwnd -le 0) {
        return $false
    }
    $requestIdentityMatched = (
        [int]$State.foregroundArmRequestedGeneration -eq $RequestedGeneration -and
        [int]$State.foregroundArmRequestCount -eq 1 -and
        $State.foregroundArmButtonEnabled -eq $true -and
        [Int64]$State.armButtonHwnd -eq $ArmButtonHwnd
    )
    $inputNotStarted = (
        [int]$State.foregroundArmAcknowledgedGeneration -eq 0 -and
        [int]$State.foregroundArmAcknowledgementCount -eq 0 -and
        [int]$State.foregroundArmLeftMouseDownCount -eq 0 -and
        [int]$State.foregroundArmLeftMouseUpCount -eq 0
    )
    $inputAlreadyComplete = (
        [int]$State.foregroundArmAcknowledgedGeneration -eq $RequestedGeneration -and
        [int]$State.foregroundArmAcknowledgementCount -eq 1 -and
        [int]$State.foregroundArmLeftMouseDownCount -eq 1 -and
        [int]$State.foregroundArmLeftMouseUpCount -eq 1
    )
    return $requestIdentityMatched -and ($inputNotStarted -or $inputAlreadyComplete)
}

function Wait-ForStableForegroundArm {
    param(
        [ValidateRange(1, 2147483647)]
        [int]$RequestedGeneration,
        [Int64]$SentinelHwnd,
        [Int64]$ArmButtonHwnd,
        [ValidateRange(1, 2147483647)]
        [int]$ExpectedFixtureProcessId,
        [scriptblock]$StateReader = { Get-FixtureStateSnapshot },
        [scriptblock]$NativeReader = { $script:nativeProbeType::Capture() },
        [scriptblock]$TopologyReader = { param($sentinel, $button, $processId) $script:nativeProbeType::ValidateFixtureArmTopology($sentinel, $button, $processId) },
        [ValidateRange(1, 300)]
        [int]$RequestDeliveryTimeoutSeconds = 10,
        [Int64]$AfterPublicationGeneration = 0,
        [ValidateRange(3, 10)]
        [int]$RequiredStableSamples = 3,
        [ValidateRange(1, 300000)]
        [int]$TimeoutMilliseconds = ($ForegroundArmTimeoutSeconds * 1000),
        [ValidateRange(0, 10000)]
        [int]$PollMilliseconds = 150
    )
    if ($SentinelHwnd -le 0 -or $ArmButtonHwnd -le 0) {
        throw "Positive fixture-owned sentinel and arm-button HWNDs are required."
    }
    if ($AfterPublicationGeneration -lt 0) {
        throw "The foreground-arm publication boundary must be non-negative."
    }
    $timeoutWatch = [Diagnostics.Stopwatch]::StartNew()
    $previousSignature = $null
    $stableSamples = 0
    $lastAcceptedPublicationGeneration = $AfterPublicationGeneration
    do {
        $state = & $StateReader
        $native = & $NativeReader
        if ($null -eq $state) {
            $state = [pscustomobject]@{
                foregroundArmRequestedGeneration = 0
                foregroundArmAcknowledgedGeneration = 0
                foregroundArmRequestCount = 0
                foregroundArmAcknowledgementCount = 0
                foregroundArmLeftMouseDownCount = 0
                foregroundArmLeftMouseUpCount = 0
                foregroundArmButtonEnabled = $false
                armButtonHwnd = 0
                statePublicationGeneration = 0
            }
        }
        $statePublicationGeneration = [Int64]$state.statePublicationGeneration
        $statePublicationAdvanced = $statePublicationGeneration -gt $lastAcceptedPublicationGeneration
        $requestMatched = [int]$state.foregroundArmRequestedGeneration -eq $RequestedGeneration
        $acknowledgementMatched = [int]$state.foregroundArmAcknowledgedGeneration -eq $RequestedGeneration
        $requestCountMatched = [int]$state.foregroundArmRequestCount -eq 1
        $acknowledgementCountMatched = [int]$state.foregroundArmAcknowledgementCount -eq 1
        $leftMouseDownCountMatched = [int]$state.foregroundArmLeftMouseDownCount -eq 1
        $leftMouseUpCountMatched = [int]$state.foregroundArmLeftMouseUpCount -eq 1
        $armButtonEnabled = $state.foregroundArmButtonEnabled -eq $true
        $buttonIdentityMatched = [Int64]$state.armButtonHwnd -eq $ArmButtonHwnd
        $nativeTopologyMatched = (& $TopologyReader $SentinelHwnd $ArmButtonHwnd $ExpectedFixtureProcessId) -eq $true
        $foregroundMatched = [Int64]$native.ForegroundHwnd -eq $SentinelHwnd
        $focusMatched = [Int64]$native.FocusHwnd -eq $ArmButtonHwnd
        $cursorAvailable = $native.CursorAvailable -eq $true
        $inputDesktopAvailable = -not [String]::IsNullOrWhiteSpace([string]$native.InputDesktop) -and [string]$native.InputDesktop -cne "unavailable"
        $armAndNativeMatched = ($requestMatched -and
            $acknowledgementMatched -and
            $requestCountMatched -and
            $acknowledgementCountMatched -and
            $leftMouseDownCountMatched -and
            $leftMouseUpCountMatched -and
            $armButtonEnabled -and
            $buttonIdentityMatched -and
            $nativeTopologyMatched -and
            $foregroundMatched -and
            $focusMatched -and
            $cursorAvailable -and
            $inputDesktopAvailable)
        $signature = [String]::Join("|", @(
                [string]$native.ForegroundHwnd,
                [string]$native.FocusHwnd,
                [string]$native.CursorX,
                [string]$native.CursorY,
                [string]$native.InputDesktop
            ))
        if ($armAndNativeMatched -and $statePublicationAdvanced) {
            $lastAcceptedPublicationGeneration = $statePublicationGeneration
            if ($null -ne $previousSignature -and $signature -ceq $previousSignature) {
                $stableSamples++
            }
            else {
                $previousSignature = $signature
                $stableSamples = 1
            }
        }
        elseif ($armAndNativeMatched -and
            $statePublicationGeneration -eq $lastAcceptedPublicationGeneration -and
            $null -ne $previousSignature -and
            $signature -ceq $previousSignature) {
            # A repeated read of the same valid publication is neutral: it
            # cannot advance the proof, but ordinary polling faster than the
            # fixture writer must not erase already accepted fresh samples.
        }
        else {
            $previousSignature = $null
            $stableSamples = 0
        }
        $proof = [ordered]@{
            requestedGeneration = $RequestedGeneration
            requestPosted = $true
            acknowledgementMode = "fresh-foreground-focused-left-mouse-down-up"
            nativeForegroundAndFocusRequired = $true
            fixtureRequestedGeneration = [int]$state.foregroundArmRequestedGeneration
            fixtureAcknowledgedGeneration = [int]$state.foregroundArmAcknowledgedGeneration
            fixtureRequestMatched = $requestMatched
            fixtureAcknowledgementMatched = $acknowledgementMatched
            fixtureRequestCount = [int]$state.foregroundArmRequestCount
            fixtureAcknowledgementCount = [int]$state.foregroundArmAcknowledgementCount
            fixtureLeftMouseDownCount = [int]$state.foregroundArmLeftMouseDownCount
            fixtureLeftMouseUpCount = [int]$state.foregroundArmLeftMouseUpCount
            requestDeliveryPublicationGeneration = $AfterPublicationGeneration
            fixtureStatePublicationGeneration = $statePublicationGeneration
            statePublicationAdvanced = $statePublicationAdvanced
            armAndNativeMatched = $armAndNativeMatched
            requestCountMatched = $requestCountMatched
            acknowledgementCountMatched = $acknowledgementCountMatched
            leftMouseDownCountMatched = $leftMouseDownCountMatched
            leftMouseUpCountMatched = $leftMouseUpCountMatched
            requestDelivered = $true
            requestDeliveryTimeoutSeconds = $RequestDeliveryTimeoutSeconds
            armButtonEnabled = $armButtonEnabled
            armButtonIdentityMatched = $buttonIdentityMatched
            nativeTopologyMatched = $nativeTopologyMatched
            foregroundMatched = $foregroundMatched
            foregroundStable = $stableSamples -ge $RequiredStableSamples
            focusMatched = $focusMatched
            focusStable = $stableSamples -ge $RequiredStableSamples
            cursorAvailable = $cursorAvailable
            cursorStable = $stableSamples -ge $RequiredStableSamples
            inputDesktopAvailable = $inputDesktopAvailable
            inputDesktopStable = $stableSamples -ge $RequiredStableSamples
            stableSamplesRequired = $RequiredStableSamples
            stableSamplesObserved = $stableSamples
            stablePublicationSamplesObserved = $stableSamples
            timeoutSeconds = [Math]::Ceiling($TimeoutMilliseconds / 1000.0)
            completed = $stableSamples -ge $RequiredStableSamples
            baselineContinuityMatched = $false
            rawWindowHandlesRecorded = $false
            rawCursorCoordinatesRecorded = $false
        }
        $script:foregroundArmProof = $proof
        if ($stableSamples -ge $RequiredStableSamples) {
            $timeoutWatch.Stop()
            return [pscustomobject]@{
                fixtureState = $state
                nativeSample = $native
                proof = $proof
            }
        }
        if ($PollMilliseconds -gt 0) {
            Start-Sleep -Milliseconds $PollMilliseconds
        }
    } while ($timeoutWatch.ElapsedMilliseconds -lt $TimeoutMilliseconds)
    $timeoutWatch.Stop()
    throw "Timed out waiting for a fresh foreground-arm click and $RequiredStableSamples stable native samples."
}

if ($SelfTest) {
    $selfTestHost = Get-Process -Id $PID
    $selfTestHostPath = $selfTestHost.Path
    $selfTestSessionId = $selfTestHost.SessionId
    if ($script:nativeProbeType::GetProcessSessionId($PID) -ne $selfTestSessionId) {
        throw "The native process-session probe did not identify the PowerShell runner session."
    }

    $selfTestJob = $script:ownedJobType::new()
    $selfTestChild = $null
    $selfTestTerminated = $false
    try {
        if ($selfTestJob.ActiveProcessCount -ne 0) {
            throw "The new private Job unexpectedly owned a process before self-test launch."
        }
        $selfTestCommandLine = '"' + $selfTestHostPath + '" -NoLogo -NoProfile -NonInteractive -Command "Start-Sleep -Seconds 30"'
        $selfTestChildPid = $selfTestJob.StartProcess(
            $selfTestHostPath,
            $selfTestCommandLine,
            [IO.Path]::GetDirectoryName($selfTestHostPath),
            @{}
        )
        $selfTestChild = [Diagnostics.Process]::GetProcessById($selfTestChildPid)
        $selfTestDeadline = [DateTime]::UtcNow.AddSeconds(5)
        $selfTestChildren = @()
        do {
            $selfTestChildren = @($script:nativeProbeType::GetDirectChildProcessIds($PID, $selfTestHostPath))
            if ($selfTestChildren -contains $selfTestChildPid) {
                break
            }
            Start-Sleep -Milliseconds 50
        } while ([DateTime]::UtcNow -lt $selfTestDeadline)

        if ($selfTestChildren.Count -ne 1 -or $selfTestChildren[0] -ne $selfTestChildPid) {
            throw "The native exact-parent/image probe did not identify exactly the Job-owned self-test child."
        }
        if ($script:nativeProbeType::GetProcessSessionId($selfTestChildPid) -ne $selfTestSessionId) {
            throw "The Job-owned self-test child was outside the PowerShell runner session."
        }
        $nonMatchingImage = [IO.Path]::Combine([IO.Path]::GetDirectoryName($selfTestHostPath), "lbb-self-test-nonmatch.exe")
        if (@($script:nativeProbeType::GetDirectChildProcessIds($PID, $nonMatchingImage)).Count -ne 0) {
            throw "The native exact-image probe accepted a nonmatching executable path."
        }
        $selfTestActiveProcessCount = $selfTestJob.ActiveProcessCount
        if ($selfTestActiveProcessCount -lt 1) {
            throw "The private Job did not account for the resumed self-test process tree (active count $selfTestActiveProcessCount)."
        }

        $selfTestJob.Terminate()
        $selfTestTerminated = $true
        if (-not $selfTestChild.WaitForExit(5000)) {
            throw "The private Job did not terminate its resumed self-test child."
        }
        $selfTestAccountingDeadline = [DateTime]::UtcNow.AddSeconds(2)
        while ($selfTestJob.ActiveProcessCount -ne 0 -and [DateTime]::UtcNow -lt $selfTestAccountingDeadline) {
            Start-Sleep -Milliseconds 50
        }
        if ($selfTestJob.ActiveProcessCount -ne 0) {
            throw "The private Job retained a live process after bounded termination."
        }
    }
    finally {
        if (-not $selfTestTerminated) {
            try {
                if ($selfTestJob.ActiveProcessCount -gt 0) {
                    $selfTestJob.Terminate()
                }
            }
            catch {
                # Preserve the primary self-test failure; Dispose still closes
                # the kill-on-close Job handle below.
            }
        }
        if ($null -ne $selfTestChild) {
            $selfTestChild.Dispose()
        }
        $selfTestJob.Dispose()
    }

    $fixtureWaitProbe = [ordered]@{
        stateReads = 0
        predicateCalls = 0
    }
    $fixtureWaitResult = Wait-ForFixtureProof `
        -FixturePredicate {
            param($state)
            $fixtureWaitProbe.predicateCalls++
            return $state.ready -eq $true
        } `
        -Description "the synthetic false-false-true fixture state" `
        -StateReader {
            $fixtureWaitProbe.stateReads++
            return [pscustomobject]@{
                sequence = $fixtureWaitProbe.stateReads
                ready = $fixtureWaitProbe.stateReads -eq 3
            }
        } `
        -TimeoutMilliseconds 1000 `
        -PollMilliseconds 1
    if ($fixtureWaitResult.sequence -ne 3 -or
        $fixtureWaitProbe.stateReads -ne 3 -or
        $fixtureWaitProbe.predicateCalls -ne 3) {
        throw "The fixture wait did not evaluate the synthetic false-false-true sequence exactly once per state."
    }

    $fixtureTimeoutWatch = [Diagnostics.Stopwatch]::StartNew()
    $fixtureTimeoutFailure = $null
    try {
        $null = Wait-ForFixtureProof `
            -FixturePredicate { param($state) return $state.ready -eq $true } `
            -Description "the bounded synthetic fixture timeout" `
            -StateReader { return [pscustomobject]@{ ready = $false } } `
            -TimeoutMilliseconds 25 `
            -PollMilliseconds 1
    }
    catch {
        $fixtureTimeoutFailure = $_.Exception.Message
    }
    finally {
        $fixtureTimeoutWatch.Stop()
    }
    if ($fixtureTimeoutFailure -cne "Timed out waiting for the bounded synthetic fixture timeout." -or
        $fixtureTimeoutWatch.ElapsedMilliseconds -gt 2000) {
        throw "The fixture wait did not preserve its bounded timeout contract."
    }

    $fixtureProbeFailure = $null
    try {
        $null = Wait-ForFixtureProof `
            -FixturePredicate { param($state) throw "synthetic-fixture-predicate-failure" } `
            -Description "the synthetic fixture predicate failure" `
            -StateReader { return [pscustomobject]@{ ready = $false } } `
            -TimeoutMilliseconds 1000 `
            -PollMilliseconds 1
    }
    catch {
        $fixtureProbeFailure = $_.Exception.Message
    }
    if ($fixtureProbeFailure -cne "synthetic-fixture-predicate-failure") {
        throw "The fixture wait did not propagate its predicate failure unchanged."
    }

    $armGeneration = 73
    $armSentinelHwnd = 101
    $armButtonHwnd = 102
    foreach ($deliveryCase in @(
        [pscustomobject]@{ name = "not started"; expected = $true; requested = $armGeneration; acknowledged = 0; requestCount = 1; acknowledgementCount = 0; leftDownCount = 0; leftUpCount = 0; enabled = $true; buttonHwnd = $armButtonHwnd },
        [pscustomobject]@{ name = "already complete"; expected = $true; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; enabled = $true; buttonHwnd = $armButtonHwnd },
        [pscustomobject]@{ name = "partial mouse down"; expected = $false; requested = $armGeneration; acknowledged = 0; requestCount = 1; acknowledgementCount = 0; leftDownCount = 1; leftUpCount = 0; enabled = $true; buttonHwnd = $armButtonHwnd },
        [pscustomobject]@{ name = "stale request"; expected = $false; requested = $armGeneration - 1; acknowledged = 0; requestCount = 1; acknowledgementCount = 0; leftDownCount = 0; leftUpCount = 0; enabled = $true; buttonHwnd = $armButtonHwnd },
        [pscustomobject]@{ name = "duplicate request"; expected = $false; requested = $armGeneration; acknowledged = 0; requestCount = 2; acknowledgementCount = 0; leftDownCount = 0; leftUpCount = 0; enabled = $true; buttonHwnd = $armButtonHwnd },
        [pscustomobject]@{ name = "duplicate input edges"; expected = $false; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 2; leftUpCount = 2; enabled = $true; buttonHwnd = $armButtonHwnd },
        [pscustomobject]@{ name = "button disabled"; expected = $false; requested = $armGeneration; acknowledged = 0; requestCount = 1; acknowledgementCount = 0; leftDownCount = 0; leftUpCount = 0; enabled = $false; buttonHwnd = $armButtonHwnd },
        [pscustomobject]@{ name = "wrong button"; expected = $false; requested = $armGeneration; acknowledged = 0; requestCount = 1; acknowledgementCount = 0; leftDownCount = 0; leftUpCount = 0; enabled = $true; buttonHwnd = $armButtonHwnd + 1 }
    )) {
        $deliveryState = [pscustomobject]@{
            foregroundArmRequestedGeneration = $deliveryCase.requested
            foregroundArmAcknowledgedGeneration = $deliveryCase.acknowledged
            foregroundArmRequestCount = $deliveryCase.requestCount
            foregroundArmAcknowledgementCount = $deliveryCase.acknowledgementCount
            foregroundArmLeftMouseDownCount = $deliveryCase.leftDownCount
            foregroundArmLeftMouseUpCount = $deliveryCase.leftUpCount
            foregroundArmButtonEnabled = $deliveryCase.enabled
            armButtonHwnd = $deliveryCase.buttonHwnd
        }
        $deliveryAccepted = Test-ForegroundArmRequestDeliveryState $deliveryState $armGeneration $armButtonHwnd
        if ($deliveryAccepted -ne $deliveryCase.expected) {
            throw "The foreground-arm request-delivery predicate failed its synthetic $($deliveryCase.name) case."
        }
    }
    $armProbe = [ordered]@{ stateReads = 0; nativeReads = 0 }
    $armStableResult = Wait-ForStableForegroundArm `
        -RequestedGeneration $armGeneration `
        -SentinelHwnd $armSentinelHwnd `
        -ArmButtonHwnd $armButtonHwnd `
        -ExpectedFixtureProcessId 103 `
        -AfterPublicationGeneration 10 `
        -TopologyReader { return $true } `
        -StateReader {
            $armProbe.stateReads = [int]$armProbe.stateReads + 1
            $requested = if ($armProbe.stateReads -eq 1) { 0 } else { $armGeneration }
            $acknowledged = if ($armProbe.stateReads -eq 1) { 0 } elseif ($armProbe.stateReads -eq 2) { $armGeneration - 1 } else { $armGeneration }
            $publicationSequence = @(11, 12, 13, 14, 14, 15, 15, 16)
            return [pscustomobject]@{
                foregroundArmRequestedGeneration = $requested
                foregroundArmAcknowledgedGeneration = $acknowledged
                foregroundArmRequestCount = if ($requested -eq $armGeneration) { 1 } else { 0 }
                foregroundArmAcknowledgementCount = if ($acknowledged -gt 0) { 1 } else { 0 }
                foregroundArmLeftMouseDownCount = if ($acknowledged -gt 0) { 1 } else { 0 }
                foregroundArmLeftMouseUpCount = if ($acknowledged -gt 0) { 1 } else { 0 }
                foregroundArmButtonEnabled = $true
                armButtonHwnd = $armButtonHwnd
                statePublicationGeneration = $publicationSequence[$armProbe.stateReads - 1]
            }
        } `
        -NativeReader {
            $armProbe.nativeReads = [int]$armProbe.nativeReads + 1
            return [pscustomobject]@{
                ForegroundHwnd = if ($armProbe.nativeReads -eq 1) { $armSentinelHwnd + 1 } else { $armSentinelHwnd }
                FocusHwnd = if ($armProbe.nativeReads -eq 2) { $armButtonHwnd + 1 } else { $armButtonHwnd }
                CursorX = if ($armProbe.nativeReads -le 3) { 40 } else { 41 }
                CursorY = 50
                CursorAvailable = $true
                InputDesktop = "Default"
            }
        } `
        -RequiredStableSamples 3 `
        -TimeoutMilliseconds 1000 `
        -PollMilliseconds 1
    if ($armProbe.stateReads -ne 8 -or
        $armProbe.nativeReads -ne 8 -or
        $armStableResult.proof.completed -ne $true -or
        $armStableResult.proof.stableSamplesObserved -ne 3 -or
        $armStableResult.proof.stablePublicationSamplesObserved -ne 3 -or
        $armStableResult.proof.fixtureStatePublicationGeneration -ne 16 -or
        $armStableResult.proof.fixtureRequestMatched -ne $true -or
        $armStableResult.proof.fixtureAcknowledgementMatched -ne $true) {
        throw "The foreground-arm wait accepted a stale, unstable, or incomplete synthetic sequence."
    }

    foreach ($armTimeoutCase in @(
        [pscustomobject]@{ name = "stale request"; requested = $armGeneration - 1; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "stale acknowledgement"; requested = $armGeneration; acknowledged = $armGeneration - 1; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "missing click"; requested = $armGeneration; acknowledged = 0; requestCount = 1; acknowledgementCount = 0; leftDownCount = 0; leftUpCount = 0; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "duplicate request"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 2; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "duplicate acknowledgement"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 2; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "duplicate left mouse down"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 2; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "duplicate left mouse up"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 2; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "wrong button identity"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd + 1; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "button disabled"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "native topology mismatch"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $false; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "foreground mismatch"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd + 1; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "focus mismatch"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd + 1; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "cursor unavailable"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $false; inputDesktop = "Default"; churnCursor = $false },
        [pscustomobject]@{ name = "input desktop unavailable"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "unavailable"; churnCursor = $false },
        [pscustomobject]@{ name = "perpetual signature churn"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $true },
        [pscustomobject]@{ name = "stale valid publication"; requested = $armGeneration; acknowledged = $armGeneration; requestCount = 1; acknowledgementCount = 1; leftDownCount = 1; leftUpCount = 1; buttonHwnd = $armButtonHwnd; topologyMatched = $true; foregroundHwnd = $armSentinelHwnd; focusHwnd = $armButtonHwnd; cursorAvailable = $true; inputDesktop = "Default"; churnCursor = $false }
    )) {
        $armTimeoutFailure = $null
        $armTimeoutProbe = [ordered]@{ nativeReads = 0; stateReads = 0 }
        $armTimeoutWatch = [Diagnostics.Stopwatch]::StartNew()
        try {
            $null = Wait-ForStableForegroundArm `
                -RequestedGeneration $armGeneration `
                -SentinelHwnd $armSentinelHwnd `
                -ArmButtonHwnd $armButtonHwnd `
                -ExpectedFixtureProcessId 103 `
                -AfterPublicationGeneration 10 `
                -TopologyReader { return $armTimeoutCase.topologyMatched } `
                -StateReader {
                    $armTimeoutProbe.stateReads = [int]$armTimeoutProbe.stateReads + 1
                    return [pscustomobject]@{
                        foregroundArmRequestedGeneration = $armTimeoutCase.requested
                        foregroundArmAcknowledgedGeneration = $armTimeoutCase.acknowledged
                        foregroundArmRequestCount = $armTimeoutCase.requestCount
                        foregroundArmAcknowledgementCount = $armTimeoutCase.acknowledgementCount
                        foregroundArmLeftMouseDownCount = $armTimeoutCase.leftDownCount
                        foregroundArmLeftMouseUpCount = $armTimeoutCase.leftUpCount
                        foregroundArmButtonEnabled = $armTimeoutCase.name -cne "button disabled"
                        armButtonHwnd = $armTimeoutCase.buttonHwnd
                        statePublicationGeneration = if ($armTimeoutCase.name -ceq "stale valid publication") { 10 } else { 10 + $armTimeoutProbe.stateReads }
                    }
                } `
                -NativeReader {
                    $armTimeoutProbe.nativeReads = [int]$armTimeoutProbe.nativeReads + 1
                    return [pscustomobject]@{
                        ForegroundHwnd = $armTimeoutCase.foregroundHwnd
                        FocusHwnd = $armTimeoutCase.focusHwnd
                        CursorX = if ($armTimeoutCase.churnCursor) { 40 + $armTimeoutProbe.nativeReads } else { 41 }
                        CursorY = 50
                        CursorAvailable = $armTimeoutCase.cursorAvailable
                        InputDesktop = $armTimeoutCase.inputDesktop
                    }
                } `
                -RequiredStableSamples 3 `
                -TimeoutMilliseconds 25 `
                -PollMilliseconds 1
        }
        catch {
            $armTimeoutFailure = $_.Exception.Message
        }
        finally {
            $armTimeoutWatch.Stop()
        }
        if ($armTimeoutFailure -cne "Timed out waiting for a fresh foreground-arm click and 3 stable native samples." -or
            $armTimeoutWatch.ElapsedMilliseconds -gt 2000 -or
            $script:foregroundArmProof.completed -ne $false) {
            throw "The foreground-arm wait did not fail closed for the synthetic $($armTimeoutCase.name) case."
        }
    }

    $operatorMarkerSelfTestRoot = [IO.Path]::Combine(
        [IO.Path]::GetTempPath(),
        "lbb-operator-marker-self-test-" + [Guid]::NewGuid().ToString("N")
    )
    [IO.Directory]::CreateDirectory($operatorMarkerSelfTestRoot) | Out-Null
    try {
        $operatorMarkerSelfTestRequestId = "0123456789abcdef0123456789abcdef"
        $operatorRequestMarker = New-ForegroundArmRequestMarker `
            -ProductVersion "0.12.14" `
            -RequestId $operatorMarkerSelfTestRequestId `
            -InputStateAtPublication "not-started" `
            -TimeoutSeconds 120 `
            -RequestDelivered $true `
            -ButtonEnabled $true `
            -NativeTopologyMatched $true
        $operatorRequestPath = Write-NewOperatorMarker `
            -Directory $operatorMarkerSelfTestRoot `
            -FileName "foreground-arm-request.json" `
            -Value $operatorRequestMarker
        $operatorRequestBytesBefore = [IO.File]::ReadAllBytes($operatorRequestPath)
        $operatorRequestJson = [Text.Encoding]::UTF8.GetString($operatorRequestBytesBefore)
        $operatorRequestRecord = $operatorRequestJson | ConvertFrom-Json
        $expectedRequestMarkerProperties = @(
            "schemaVersion", "productVersion", "kind", "status", "requestId", "publishedAtUtc",
            "timeoutSeconds", "operatorActionRequired", "preferredRelaySurface", "fallbackRelaySurface",
            "expectedVisibleWindowTitle", "expectedVisibleButtonText", "expectedAccessibleName", "action",
            "stopUiAfterAction", "requiresSeparateAuthorization", "markerGrantsAuthorization",
            "markerGrantsConsent", "externalOneShotConsentRequired", "visualConfirmationRequired",
            "maximumClickAttempts", "retryOnUnknownOutcome", "instruction", "requestDelivered",
            "buttonEnabled", "nativeTopologyMatched", "inputStateAtPublication", "notificationOnly", "acceptedAsAuthority",
            "rawWindowHandlesRecorded", "rawCursorCoordinatesRecorded", "pathsRecorded",
            "secretsRecorded"
        )
        if ((@($operatorRequestRecord.PSObject.Properties.Name) -join "|") -cne ($expectedRequestMarkerProperties -join "|") -or
            $operatorRequestRecord.schemaVersion -ne 2 -or
            $operatorRequestRecord.productVersion -cne "0.12.14" -or
            $operatorRequestRecord.status -cne "action-required" -or
            $operatorRequestRecord.requestId -cne $operatorMarkerSelfTestRequestId -or
            $operatorRequestRecord.operatorActionRequired -ne $true -or
            $operatorRequestRecord.preferredRelaySurface -cne "windows-computer-use-app-share" -or
            $operatorRequestRecord.fallbackRelaySurface -cne "human-on-windows-session" -or
            $operatorRequestRecord.expectedVisibleWindowTitle -cne "LBB Windows Acceptance - ACTION REQUIRED" -or
            $operatorRequestRecord.expectedVisibleButtonText -cne "CLICK TO ARM" -or
            $operatorRequestRecord.expectedAccessibleName -cne "Click to arm Windows acceptance" -or
            $operatorRequestRecord.action -cne "single-left-click" -or
            $operatorRequestRecord.stopUiAfterAction -ne $true -or
            $operatorRequestRecord.requiresSeparateAuthorization -ne $true -or
            $operatorRequestRecord.markerGrantsAuthorization -ne $false -or
            $operatorRequestRecord.markerGrantsConsent -ne $false -or
            $operatorRequestRecord.externalOneShotConsentRequired -ne $true -or
            $operatorRequestRecord.visualConfirmationRequired -ne $true -or
            $operatorRequestRecord.maximumClickAttempts -ne 1 -or
            $operatorRequestRecord.retryOnUnknownOutcome -ne $false -or
            $operatorRequestRecord.notificationOnly -ne $true -or
            $operatorRequestRecord.acceptedAsAuthority -ne $false) {
            throw "The foreground-arm request marker failed its exact-schema self-test."
        }
        foreach ($forbiddenMarkerField in @('"token"', '"pid"', '"hwnd"', '"cursorX"', '"cursorY"', '"path"')) {
            if ($operatorRequestJson.IndexOf($forbiddenMarkerField, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                throw "The foreground-arm request marker retained a forbidden raw or secret-bearing field."
            }
        }
        $duplicateMarkerFailure = $null
        try {
            $null = Write-NewOperatorMarker `
                -Directory $operatorMarkerSelfTestRoot `
                -FileName "foreground-arm-request.json" `
                -Value $operatorRequestMarker
        }
        catch {
            $duplicateMarkerFailure = $_.Exception.Message
        }
        $operatorRequestBytesAfter = [IO.File]::ReadAllBytes($operatorRequestPath)
        if ($duplicateMarkerFailure -cne "Operator markers are create-once for this runner and cannot be overwritten by it." -or
            [Convert]::ToBase64String($operatorRequestBytesBefore) -cne [Convert]::ToBase64String($operatorRequestBytesAfter) -or
            @([IO.Directory]::EnumerateFiles($operatorMarkerSelfTestRoot, "*.tmp")).Count -ne 0) {
            throw "The operator marker writer failed its atomic create-once self-test."
        }

        $alreadyArmedMarker = New-ForegroundArmRequestMarker `
            -ProductVersion "0.12.14" `
            -RequestId $operatorMarkerSelfTestRequestId `
            -InputStateAtPublication "already-acknowledged" `
            -TimeoutSeconds 120 `
            -RequestDelivered $true `
            -ButtonEnabled $true `
            -NativeTopologyMatched $true
        if ($alreadyArmedMarker.status -cne "already-armed" -or
            $alreadyArmedMarker.operatorActionRequired -ne $false -or
            $alreadyArmedMarker.expectedVisibleWindowTitle -cne "LBB Windows Acceptance - ARMED" -or
            $alreadyArmedMarker.expectedVisibleButtonText -cne "ARMED - DO NOT USE THIS SESSION" -or
            $alreadyArmedMarker.action -cne "none" -or
            $alreadyArmedMarker.maximumClickAttempts -ne 0 -or
            $alreadyArmedMarker.retryOnUnknownOutcome -ne $false) {
            throw "The foreground-arm request marker did not suppress a duplicate-click prompt after an early valid acknowledgement."
        }

        $operatorReceivedProof = [pscustomobject]@{
            completed = $true
            fixtureRequestCount = 1
            fixtureAcknowledgementCount = 1
            fixtureLeftMouseDownCount = 1
            fixtureLeftMouseUpCount = 1
            nativeTopologyMatched = $true
            foregroundMatched = $true
            focusMatched = $true
            cursorAvailable = $true
            cursorStable = $true
            inputDesktopAvailable = $true
            inputDesktopStable = $true
            stableSamplesObserved = 3
            stableSamplesRequired = 3
        }
        $operatorReceivedMarker = New-ForegroundArmReceivedMarker `
            -ProductVersion "0.12.14" `
            -RequestId $operatorMarkerSelfTestRequestId `
            -Proof $operatorReceivedProof
        $operatorReceivedPath = Write-NewOperatorMarker `
            -Directory $operatorMarkerSelfTestRoot `
            -FileName "foreground-arm-received.json" `
            -Value $operatorReceivedMarker
        $operatorReceivedRecord = [IO.File]::ReadAllText($operatorReceivedPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
        $expectedReceivedMarkerProperties = @(
            "schemaVersion", "productVersion", "kind", "status", "requestId", "receivedAtUtc",
            "exactClickCountsMatched", "stableSamplesObserved", "stableSamplesRequired", "nativeTopologyMatched",
            "foregroundMatched", "focusMatched", "cursorStable", "inputDesktopStable", "notificationOnly",
            "acceptedAsAuthority", "rawWindowHandlesRecorded", "rawCursorCoordinatesRecorded", "pathsRecorded",
            "secretsRecorded"
        )
        if ((@($operatorReceivedRecord.PSObject.Properties.Name) -join "|") -cne ($expectedReceivedMarkerProperties -join "|") -or
            $operatorReceivedRecord.status -cne "received" -or
            $operatorReceivedRecord.schemaVersion -ne 2 -or
            $operatorReceivedRecord.productVersion -cne "0.12.14" -or
            $operatorReceivedRecord.requestId -cne $operatorRequestRecord.requestId -or
            $operatorReceivedRecord.exactClickCountsMatched -ne $true -or
            $operatorReceivedRecord.stableSamplesObserved -ne 3 -or
            $operatorReceivedRecord.notificationOnly -ne $true -or
            $operatorReceivedRecord.acceptedAsAuthority -ne $false) {
            throw "The foreground-arm received marker failed its exact-proof self-test."
        }
        $operatorReceivedProof.fixtureLeftMouseUpCount = 2
        $incompleteReceivedMarkerFailure = $null
        try {
            $null = New-ForegroundArmReceivedMarker `
                -ProductVersion "0.12.14" `
                -RequestId $operatorMarkerSelfTestRequestId `
                -Proof $operatorReceivedProof
        }
        catch {
            $incompleteReceivedMarkerFailure = $_.Exception.Message
        }
        if ($incompleteReceivedMarkerFailure -cne "An operator received marker requires the complete click and stable-native-sample proof.") {
            throw "The foreground-arm received marker accepted an incomplete or duplicate-click proof."
        }
    }
    finally {
        if ([IO.Directory]::Exists($operatorMarkerSelfTestRoot)) {
            [IO.Directory]::Delete($operatorMarkerSelfTestRoot, $true)
        }
    }

    $candidateBindingSelfTestRoot = [IO.Path]::Combine(
        [IO.Path]::GetTempPath(),
        "lbb-candidate-binding-self-test-" + [Guid]::NewGuid().ToString("N")
    )
    [IO.Directory]::CreateDirectory($candidateBindingSelfTestRoot) | Out-Null
    try {
        $sha256SelfTestPath = [IO.Path]::Combine($candidateBindingSelfTestRoot, "sha256-probe.bin")
        [IO.File]::WriteAllBytes($sha256SelfTestPath, [byte[]](0x61, 0x62, 0x63))
        if ((Get-FileSha256 $sha256SelfTestPath) -cne
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad") {
            throw "The module-independent SHA-256 helper failed its canonical test vector."
        }
        $candidateBindingSelfTestPath = [IO.Path]::Combine($candidateBindingSelfTestRoot, "candidate-binding.json")
        $candidateBindingNames = @(
            "local-browser-bridge-v0.12.14-windows-x86_64.exe",
            "local-computer-helper-v0.12.14-windows-x86_64.exe",
            "local-browser-bridge-v0.12.14-macos-universal.tar.gz",
            "local-browser-bridge-extension-v0.12.14.zip"
        )
        $candidateBindingChecksums = [Collections.Generic.Dictionary[string, string]]::new([StringComparer]::Ordinal)
        for ($index = 0; $index -lt $candidateBindingNames.Count; $index++) {
            $candidateBindingChecksums.Add(
                $candidateBindingNames[$index],
                [String]::new([char]([int][char]'1' + $index), 64)
            )
        }
        $candidateBindingManifestSha = [String]::new([char]'a', 64)
        $candidateBindingAssets = New-Object Collections.Generic.List[object]
        for ($index = 0; $index -lt $candidateBindingNames.Count; $index++) {
            $candidateBindingAssets.Add([ordered]@{
                file = $candidateBindingNames[$index]
                bytes = [int64](100 + $index)
                sha256 = $candidateBindingChecksums[$candidateBindingNames[$index]]
            })
        }
        $candidateBindingAssets.Add([ordered]@{
            file = "SHA256SUMS.txt"
            bytes = [int64]500
            sha256 = $candidateBindingManifestSha
        })
        $candidateBindingSelfTestRecord = [ordered]@{
            schemaVersion = 1
            productVersion = "0.12.14"
            repository = "flrngel/local-browser-bridge"
            tag = "v0.12.14"
            sourceSha = [String]::new([char]'b', 40)
            tagObjectSha = [String]::new([char]'c', 40)
            workflowRunId = "32650000000"
            workflowRunAttempt = "1"
            artifactId = "9500000000"
            artifactName = "release-candidate"
            artifactZipBytes = [int64]4096
            artifactZipSha256 = [String]::new([char]'d', 64)
            checksumManifestSha256 = $candidateBindingManifestSha
            attestationInvocationUri = "https://github.com/flrngel/local-browser-bridge/actions/runs/32650000000/attempts/1"
            attestedAssetCount = 5
            githubHostedRunner = $true
            assets = $candidateBindingAssets.ToArray()
            passed = $true
        }
        [IO.File]::WriteAllText(
            $candidateBindingSelfTestPath,
            ($candidateBindingSelfTestRecord | ConvertTo-Json -Depth 10 -Compress) + [Environment]::NewLine,
            [Text.UTF8Encoding]::new($false)
        )
        $candidateBindingSelfTestResult = Read-ExactReleaseCandidateBinding `
            -Path $candidateBindingSelfTestPath `
            -ExpectedVersion "0.12.14" `
            -ExpectedManifestSha256 $candidateBindingManifestSha `
            -ExpectedChecksums $candidateBindingChecksums `
            -ExpectedAssetNames $candidateBindingNames
        if ($candidateBindingSelfTestResult.workflowRunAttempt -cne "1" -or
            $candidateBindingSelfTestResult.artifactId -cne "9500000000" -or
            $candidateBindingSelfTestResult.assets.Count -ne 5) {
            throw "The exact release-candidate binding self-test lost its attempt or asset identity."
        }
        $candidateBindingSelfTestRecord.workflowRunAttempt = "0"
        [IO.File]::WriteAllText(
            $candidateBindingSelfTestPath,
            ($candidateBindingSelfTestRecord | ConvertTo-Json -Depth 10 -Compress) + [Environment]::NewLine,
            [Text.UTF8Encoding]::new($false)
        )
        $candidateBindingFailure = $null
        try {
            $null = Read-ExactReleaseCandidateBinding `
                -Path $candidateBindingSelfTestPath `
                -ExpectedVersion "0.12.14" `
                -ExpectedManifestSha256 $candidateBindingManifestSha `
                -ExpectedChecksums $candidateBindingChecksums `
                -ExpectedAssetNames $candidateBindingNames
        }
        catch {
            $candidateBindingFailure = $_.Exception.Message
        }
        if ($candidateBindingFailure -cne "CandidateBindingPath does not bind the exact frozen workflow candidate.") {
            throw "The release-candidate binding self-test accepted a noncanonical workflow attempt."
        }
    }
    finally {
        if ([IO.Directory]::Exists($candidateBindingSelfTestRoot)) {
            [IO.Directory]::Delete($candidateBindingSelfTestRoot, $true)
        }
    }

    Write-Output "Windows computer-use acceptance self-test passed."
    return
}

$sessionId = (Get-Process -Id $PID).SessionId
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not [Environment]::UserInteractive -or $sessionId -eq 0 -or $identity.IsSystem) {
    throw "A signed-in, non-System interactive Windows session is required; service and Session 0 runs are invalid evidence."
}
if (-not $script:nativeProbeType::HasInteractiveInputDesktop()) {
    throw "The current process cannot open the interactive input desktop."
}
$initialProbe = $script:nativeProbeType::Capture()
if ($initialProbe.ForegroundHwnd -eq 0 -or $initialProbe.InputDesktop -eq "unavailable") {
    throw "The interactive foreground or input desktop is unavailable."
}

function Get-EphemeralPort {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([Net.IPEndPoint]$listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
}

function Test-PortBindable {
    param([int]$Candidate)
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $Candidate)
    try {
        $listener.Start()
        return $true
    }
    catch {
        return $false
    }
    finally {
        $listener.Stop()
    }
}

if ($Port -eq 0) {
    $Port = Get-EphemeralPort
}
elseif (-not (Test-PortBindable $Port)) {
    throw "The requested loopback port is already in use. The runner will not disturb its owner."
}

function ConvertTo-NativeArgument {
    param([AllowEmptyString()][string]$Value)
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') {
        return $Value
    }
    $builder = [Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $slashes++
            continue
        }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * (($slashes * 2) + 1)))
            [void]$builder.Append('"')
            $slashes = 0
            continue
        }
        if ($slashes -gt 0) {
            [void]$builder.Append(('\' * $slashes))
            $slashes = 0
        }
        [void]$builder.Append($character)
    }
    if ($slashes -gt 0) {
        [void]$builder.Append(('\' * ($slashes * 2)))
    }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Start-IsolatedProcess {
    param(
        [string]$Path,
        [string[]]$Arguments,
        [hashtable]$Environment
    )
    $quotedApplication = ConvertTo-NativeArgument $Path
    $quotedArguments = (($Arguments | ForEach-Object { ConvertTo-NativeArgument $_ }) -join ' ')
    $commandLine = if ([String]::IsNullOrEmpty($quotedArguments)) {
        $quotedApplication
    }
    else {
        $quotedApplication + " " + $quotedArguments
    }
    # CreateProcess starts suspended; the private Job Object owns the process
    # before ResumeThread, so supervisors cannot race by spawning an unowned
    # worker. All standard handles point at NUL because server stdout contains
    # the bearer token and must never become evidence.
    $processId = $script:ownedJob.StartProcess(
        $Path,
        $commandLine,
        [IO.Path]::GetDirectoryName($Path),
        $Environment
    )
    return [Diagnostics.Process]::GetProcessById($processId)
}

function Request-FixtureStop {
    param([Diagnostics.Process]$Process)
    if ($null -eq $Process) {
        return $true
    }
    try {
        $Process.Refresh()
        if ($Process.HasExited) {
            return $true
        }
        if ($null -ne $script:fixtureReady) {
            $handle = [Int64]$script:fixtureReady.targetHwnd
            if ($handle -ne 0) {
                [void]$script:nativeProbeType::PostMessage([IntPtr]$handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
                if ($Process.WaitForExit(2500)) {
                    return $true
                }
            }
        }
        return $false
    }
    catch [InvalidOperationException] {
        # The exact process exited between Refresh and cleanup.
        return $true
    }
}

function Read-JsonFile {
    param([string]$Path)
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ([IO.File]::Exists($Path)) {
            try {
                return ([IO.File]::ReadAllText($Path, [Text.Encoding]::UTF8) | ConvertFrom-Json)
            }
            catch {
                # The fixture replaces this small state document in place; retry a partial read.
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for fixture evidence."
}

function Write-EvidenceJson {
    param([string]$Path, [object]$Value)
    $json = $Value | ConvertTo-Json -Depth 40
    [IO.File]::WriteAllText($Path, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}

function Test-FileContainsSecret {
    param([string]$Path, [string]$Secret)
    if ([String]::IsNullOrEmpty($Secret)) {
        return $false
    }
    $bytes = [IO.File]::ReadAllBytes($Path)
    # ISO-8859-1 maps each byte to one character, so an ordinal search finds
    # the ASCII token even inside a binary file without interpreting the file.
    $binaryView = [Text.Encoding]::GetEncoding(28591).GetString($bytes)
    return $binaryView.IndexOf($Secret, [StringComparison]::Ordinal) -ge 0
}

function ConvertTo-SafeObject {
    param([object]$Value)
    if ($null -eq $Value) {
        return $null
    }
    if ($Value -is [string]) {
        return ConvertTo-SafeFailureText $Value
    }
    if ($Value -is [ValueType]) {
        return $Value
    }
    if ($Value -is [Collections.IDictionary]) {
        $output = [ordered]@{}
        foreach ($key in $Value.Keys) {
            $name = [string]$key
            if ($name.ToLowerInvariant() -in @("token", "sessiontoken", "csrf", "authorization", "windows", "tabs", "events", "dataurl", "screenshotdata", "value")) {
                continue
            }
            $output[$name] = ConvertTo-SafeObject $Value[$key]
        }
        return $output
    }
    if ($Value -is [Collections.IEnumerable]) {
        return @($Value | ForEach-Object { ConvertTo-SafeObject $_ })
    }
    $properties = @($Value.PSObject.Properties | Where-Object { $_.MemberType -in @("NoteProperty", "Property", "AliasProperty") })
    if ($properties.Count -eq 0) {
        return [string]$Value
    }
    $objectOutput = [ordered]@{}
    foreach ($property in $properties) {
        $name = $property.Name
        if ($name.ToLowerInvariant() -in @("token", "sessiontoken", "csrf", "authorization", "windows", "tabs", "events", "dataurl", "screenshotdata", "value")) {
            continue
        }
        $objectOutput[$name] = ConvertTo-SafeObject $property.Value
    }
    return $objectOutput
}

function Get-PropertyValue {
    param([object]$Object, [string]$Name)
    if ($null -eq $Object) {
        return $null
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function ConvertTo-SafeResponse {
    param([object]$Response)
    $result = Get-PropertyValue $Response "result"
    $errorObject = Get-PropertyValue $Response "error"
    $taxonomy = Get-PropertyValue $Response "taxonomy"
    $state = Get-PropertyValue $Response "state"
    $safe = [ordered]@{
        result = ConvertTo-SafeObject $result
        error = ConvertTo-SafeObject $errorObject
        taxonomy = ConvertTo-SafeObject $taxonomy
    }
    if ($null -ne $Response.PSObject.Properties["callId"]) {
        $safe.callId = $Response.callId
    }
    if ($null -ne $state) {
        $safe.targetState = [ordered]@{
            computerConnected = Get-PropertyValue $state "computerConnected"
            computer = ConvertTo-SafeObject (Get-PropertyValue $state "computer")
            computerObservation = ConvertTo-SafeObject (Get-PropertyValue $state "computerObservation")
        }
    }
    return $safe
}

function ConvertTo-SafeFailureText {
    param([string]$Message)
    $safe = if ($null -eq $Message) { "Unknown failure" } else { $Message }
    foreach ($replacement in @(
        @($Token, "[REDACTED_TOKEN]"),
        @($evidenceRoot, "[EVIDENCE_DIRECTORY]"),
        @($resolvedServer, "[SERVER]"),
        @($resolvedHelper, "[HELPER]"),
        @($resolvedFixture, "[FIXTURE]"),
        @($resolvedChecksumManifest, "[CHECKSUM_MANIFEST]"),
        @($PSCommandPath, "[RUNNER]")
    )) {
        if (-not [String]::IsNullOrEmpty([string]$replacement[0])) {
            $safe = $safe.Replace([string]$replacement[0], [string]$replacement[1])
        }
    }
    if ($safe.Length -gt 1200) {
        $safe = $safe.Substring(0, 1200)
    }
    return $safe
}

$script:stepNumber = 0
$script:stepResults = [Collections.Generic.List[object]]::new()

function Save-StepResponse {
    param([string]$Name, [object]$Response)
    $script:stepNumber++
    $slug = ($Name.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')
    $fileName = "{0:D2}-{1}.json" -f $script:stepNumber, $slug
    Write-EvidenceJson ([IO.Path]::Combine($stepEvidence, $fileName)) (ConvertTo-SafeResponse $Response)
    $script:stepResults.Add([ordered]@{ name = $Name; passed = $true; evidence = $fileName })
}

function Save-StepRecord {
    param([string]$Name, [object]$Record)
    $script:stepNumber++
    $slug = ($Name.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')
    $fileName = "{0:D2}-{1}.json" -f $script:stepNumber, $slug
    Write-EvidenceJson ([IO.Path]::Combine($stepEvidence, $fileName)) (ConvertTo-SafeObject $Record)
    $script:stepResults.Add([ordered]@{ name = $Name; passed = $true; evidence = $fileName })
}

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function New-LbbHttpClient {
    $client = [Net.Http.HttpClient]::new()
    $client.Timeout = [TimeSpan]::FromSeconds($TimeoutSeconds)
    $client.DefaultRequestHeaders.Authorization = [Net.Http.Headers.AuthenticationHeaderValue]::new("Bearer", $Token)
    return $client
}

function Close-LbbPendingRequest {
    param([object]$Pending)
    if ($null -eq $Pending -or $Pending.disposed -eq $true) {
        return
    }
    if ($null -ne $Pending.content) {
        $Pending.content.Dispose()
    }
    $Pending.client.Dispose()
    $Pending.disposed = $true
}

function Start-LbbJsonPost {
    param([string]$Path, [object]$Body)
    $client = New-LbbHttpClient
    $content = $null
    try {
        $json = $Body | ConvertTo-Json -Depth 20 -Compress
        $content = [Net.Http.StringContent]::new($json, [Text.Encoding]::UTF8, "application/json")
        $task = $client.PostAsync($script:baseUrl + $Path, $content)
        return [pscustomobject]@{
            client = $client
            content = $content
            task = $task
            disposed = $false
        }
    }
    catch {
        if ($null -ne $content) {
            $content.Dispose()
        }
        $client.Dispose()
        throw
    }
}

function Receive-LbbJsonResponse {
    param([object]$Pending)
    $httpResponse = $null
    try {
        $httpResponse = ($Pending.task.GetAwaiter()).GetResult()
        $json = (($httpResponse.Content.ReadAsStringAsync()).GetAwaiter()).GetResult()
        if ([String]::IsNullOrWhiteSpace($json)) {
            throw "Loopback API returned an empty JSON response."
        }
        $body = $json | ConvertFrom-Json
        return [pscustomobject]@{
            status = [int]$httpResponse.StatusCode
            httpOk = $httpResponse.IsSuccessStatusCode
            body = $body
        }
    }
    catch {
        throw "Loopback API request failed: $(ConvertTo-SafeFailureText $_.Exception.Message)"
    }
    finally {
        if ($null -ne $httpResponse) {
            $httpResponse.Dispose()
        }
        Close-LbbPendingRequest $Pending
    }
}

function Invoke-LbbJsonPost {
    param([string]$Path, [object]$Body)
    $pending = Start-LbbJsonPost $Path $Body
    return Receive-LbbJsonResponse $pending
}

function Invoke-LbbJsonGet {
    param([string]$Path)
    $client = New-LbbHttpClient
    $httpResponse = $null
    try {
        $httpResponse = (($client.GetAsync($script:baseUrl + $Path)).GetAwaiter()).GetResult()
        $json = (($httpResponse.Content.ReadAsStringAsync()).GetAwaiter()).GetResult()
        if ([String]::IsNullOrWhiteSpace($json)) {
            throw "Loopback API returned an empty JSON response."
        }
        return [pscustomobject]@{
            status = [int]$httpResponse.StatusCode
            httpOk = $httpResponse.IsSuccessStatusCode
            body = ($json | ConvertFrom-Json)
        }
    }
    catch {
        throw "Loopback API request failed: $(ConvertTo-SafeFailureText $_.Exception.Message)"
    }
    finally {
        if ($null -ne $httpResponse) {
            $httpResponse.Dispose()
        }
        $client.Dispose()
    }
}

function Start-LbbCommandRequest {
    param([string]$Method, [hashtable]$Params, [string]$CallId)
    return Start-LbbJsonPost "/api/v1/command" ([ordered]@{
        method = $Method
        params = $Params
        callId = $CallId
    })
}

function Invoke-LbbCommandResponse {
    param([string]$Method, [hashtable]$Params, [string]$CallId)
    return Invoke-LbbJsonPost "/api/v1/command" ([ordered]@{
        method = $Method
        params = $Params
        callId = $CallId
    })
}

function Invoke-LbbCancelResponse {
    param([string]$CallId)
    return Invoke-LbbJsonPost "/api/v1/command/cancel" ([ordered]@{ callId = $CallId })
}

function Invoke-LbbCommand {
    param([string]$Method, [hashtable]$Params)
    $callId = "windows-fixture-" + [Guid]::NewGuid().ToString("N")
    $raw = Invoke-LbbCommandResponse $Method $Params $callId
    $response = $raw.body
    $responseError = Get-PropertyValue $response "error"
    if ($null -ne $responseError) {
        throw "Loopback command $Method returned $($responseError.code): $($responseError.message)"
    }
    Assert-True ($raw.httpOk -eq $true) "Loopback command $Method returned HTTP $($raw.status) without a structured error."
    return $response
}

function Get-LbbState {
    $response = Invoke-RestMethod -UseBasicParsing -Uri "$script:baseUrl/api/state" -Method Get -Headers @{ Authorization = "Bearer $Token" } -TimeoutSec 5
    return $response.state
}

function Get-CurrentObservation {
    $state = Get-LbbState
    if ($null -eq $state.computerObservation) {
        throw "The bridge has no current computer observation."
    }
    return $state.computerObservation
}

function Save-ObservationScreenshot {
    param([object]$Observation, [string]$Name)
    $relative = [string]$Observation.screenshotUrl
    if (-not $relative.StartsWith("/api/computer/screenshot?id=", [StringComparison]::Ordinal) -or $relative.Contains("://")) {
        throw "The bridge returned an invalid computer screenshot URL."
    }
    $fileName = (($Name.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')) + ".png"
    $path = [IO.Path]::Combine($screenshotEvidence, $fileName)
    Invoke-WebRequest -UseBasicParsing -Uri ($script:baseUrl + $relative) -Method Get -Headers @{ Authorization = "Bearer $Token" } -OutFile $path -TimeoutSec $TimeoutSeconds | Out-Null
    $bytes = [IO.File]::ReadAllBytes($path)
    Assert-True ($bytes.Length -gt 1000) "The exact-window screenshot was unexpectedly small."
    Assert-True ($bytes[0] -eq 0x89 -and $bytes[1] -eq 0x50 -and $bytes[2] -eq 0x4E -and $bytes[3] -eq 0x47) "The screenshot was not a PNG."
    return [ordered]@{
        file = $fileName
        bytes = $bytes.Length
        sha256 = Get-FileSha256 $path
        frameId = $Observation.frameId
        contentHash = $Observation.contentHash
    }
}

function Get-FixtureState {
    return Read-JsonFile ([IO.Path]::Combine($fixtureEvidence, "fixture-state.json"))
}

function Get-FixtureStateSnapshot {
    $path = [IO.Path]::Combine($fixtureEvidence, "fixture-state.json")
    if (-not [IO.File]::Exists($path)) {
        return $null
    }
    try {
        $json = [IO.File]::ReadAllText($path, [Text.Encoding]::UTF8)
        if ([String]::IsNullOrWhiteSpace($json)) {
            return $null
        }
        return ($json | ConvertFrom-Json)
    }
    catch {
        # The fixture replaces this small state document in place. A partial
        # snapshot resets arm stability instead of extending the arm timeout.
        return $null
    }
}

function Get-TextHash {
    param([string]$Value)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
        return (($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString("x2") }) -join '')
    }
    finally {
        $sha.Dispose()
    }
}

function Capture-InvariantProbe {
    param([Int64]$AfterStatePublicationGeneration = -1)
    if ($AfterStatePublicationGeneration -ge 0) {
        $minimumPublicationGeneration = $AfterStatePublicationGeneration
        $fixture = Wait-ForFixtureProof `
            -FixturePredicate {
                param($state)
                return $null -ne $state -and [Int64]$state.statePublicationGeneration -gt $minimumPublicationGeneration
            } `
            -Description "a fixture state publication newer than the accepted invariant boundary" `
            -StateReader { Get-FixtureStateSnapshot }
    }
    else {
        $fixture = Get-FixtureState
    }
    $native = $script:nativeProbeType::Capture()
    return [ordered]@{
        statePublicationGeneration = [Int64]$fixture.statePublicationGeneration
        foregroundHwnd = $native.ForegroundHwnd.ToString()
        focusHwnd = $native.FocusHwnd.ToString()
        cursor = [ordered]@{ x = $native.CursorX; y = $native.CursorY }
        cursorAvailable = $native.CursorAvailable
        inputDesktop = $native.InputDesktop
        targetActivatedCount = $fixture.targetActivatedCount
        sentinelActivatedCount = $fixture.sentinelActivatedCount
        sentinelDeactivatedCount = $fixture.sentinelDeactivatedCount
        foregroundArmRequestedGeneration = [int]$fixture.foregroundArmRequestedGeneration
        foregroundArmAcknowledgedGeneration = [int]$fixture.foregroundArmAcknowledgedGeneration
        foregroundArmRequestCount = [int]$fixture.foregroundArmRequestCount
        foregroundArmAcknowledgementCount = [int]$fixture.foregroundArmAcknowledgementCount
        foregroundArmLeftMouseDownCount = [int]$fixture.foregroundArmLeftMouseDownCount
        foregroundArmLeftMouseUpCount = [int]$fixture.foregroundArmLeftMouseUpCount
        foregroundArmButtonEnabled = $fixture.foregroundArmButtonEnabled -eq $true
    }
}

function Assert-InvariantsHeld {
    param([object]$Before, [string]$Step)
    # The fixture snapshots its cross-thread activation counters every 200 ms.
    # Wait through one complete publication interval before reading the oracle.
    Start-Sleep -Milliseconds 300
    $minimumPublicationGeneration = [Math]::Max(
        [Int64]$Before.statePublicationGeneration,
        [Int64]$script:lastInvariantPublicationGeneration
    )
    $after = Capture-InvariantProbe -AfterStatePublicationGeneration $minimumPublicationGeneration
    $script:lastInvariantPublicationGeneration = [Int64]$after.statePublicationGeneration
    Assert-True ($after.foregroundHwnd -eq $Before.foregroundHwnd) "$Step changed the foreground HWND."
    Assert-True ($after.focusHwnd -eq $Before.focusHwnd) "$Step changed the foreground focus HWND."
    Assert-True ($Before.cursorAvailable -eq $true -and $after.cursorAvailable -eq $true) "$Step could not verify the OS-global cursor position."
    Assert-True ($after.cursor.x -eq $Before.cursor.x -and $after.cursor.y -eq $Before.cursor.y) "$Step moved the OS-global cursor position."
    Assert-True ($after.inputDesktop -eq $Before.inputDesktop) "$Step changed the input desktop."
    Assert-True ($after.targetActivatedCount -eq $Before.targetActivatedCount) "$Step activated the background target."
    Assert-True ($after.sentinelDeactivatedCount -eq $Before.sentinelDeactivatedCount) "$Step deactivated the foreground sentinel."
    Assert-True ($after.foregroundArmRequestedGeneration -eq $Before.foregroundArmRequestedGeneration) "$Step changed the foreground-arm request generation."
    Assert-True ($after.foregroundArmAcknowledgedGeneration -eq $Before.foregroundArmAcknowledgedGeneration) "$Step changed the foreground-arm acknowledgement generation."
    Assert-True ($after.foregroundArmRequestCount -eq $Before.foregroundArmRequestCount) "$Step changed the foreground-arm request count."
    Assert-True ($after.foregroundArmAcknowledgementCount -eq $Before.foregroundArmAcknowledgementCount) "$Step changed the foreground-arm acknowledgement count."
    Assert-True ($after.foregroundArmLeftMouseDownCount -eq $Before.foregroundArmLeftMouseDownCount) "$Step received another foreground-arm left mouse down."
    Assert-True ($after.foregroundArmLeftMouseUpCount -eq $Before.foregroundArmLeftMouseUpCount) "$Step received another foreground-arm left mouse up."
    Assert-True ($Before.foregroundArmButtonEnabled -eq $true -and $after.foregroundArmButtonEnabled -eq $true) "$Step lost the foreground-arm button-enabled receipt."
    return $after
}

function Convert-ScreenPointToImage {
    param([object]$Observation, [double]$ScreenX, [double]$ScreenY)
    $x = [Math]::Round(($ScreenX - [double]$Observation.screenX) * [double]$Observation.transportScaleX)
    $y = [Math]::Round(($ScreenY - [double]$Observation.screenY) * [double]$Observation.transportScaleY)
    $x = [Math]::Max(0, [Math]::Min(([double]$Observation.imageWidth - 1), $x))
    $y = [Math]::Max(0, [Math]::Min(([double]$Observation.imageHeight - 1), $y))
    return [ordered]@{ x = [double]$x; y = [double]$y }
}

function Get-SurfacePoint {
    param([object]$Observation, [double]$FractionX, [double]$FractionY)
    $fixture = Get-FixtureState
    $bounds = $fixture.surfaceScreenBounds
    return Convert-ScreenPointToImage $Observation ([double]$bounds.x + ([double]$bounds.width * $FractionX)) ([double]$bounds.y + ([double]$bounds.height * $FractionY))
}

function Get-MagentaPixelCount {
    param([string]$FileName)
    Add-Type -AssemblyName System.Drawing
    $bitmap = [Drawing.Bitmap]::FromFile([IO.Path]::Combine($screenshotEvidence, $FileName))
    try {
        $count = 0
        for ($y = 0; $y -lt $bitmap.Height; $y += 2) {
            for ($x = 0; $x -lt $bitmap.Width; $x += 2) {
                $pixel = $bitmap.GetPixel($x, $y)
                if ($pixel.R -ge 240 -and $pixel.G -le 20 -and $pixel.B -ge 240) {
                    $count++
                }
            }
        }
        return $count
    }
    finally {
        $bitmap.Dispose()
    }
}

function Save-SanitizedDesktopCrop {
    param([string]$Name)
    Add-Type -AssemblyName System.Drawing
    Add-Type -AssemblyName System.Windows.Forms
    $fixture = Get-FixtureState
    $target = $fixture.targetBounds
    $margin = 16
    $virtual = [Windows.Forms.SystemInformation]::VirtualScreen
    $left = [Math]::Max($virtual.Left, [int]$target.x - $margin)
    $top = [Math]::Max($virtual.Top, [int]$target.y - $margin)
    $right = [Math]::Min($virtual.Right, [int]$target.x + [int]$target.width + $margin)
    $bottom = [Math]::Min($virtual.Bottom, [int]$target.y + [int]$target.height + $margin)
    $width = $right - $left
    $height = $bottom - $top
    Assert-True ($width -gt 100 -and $height -gt 100) "The fixture crop bounds were invalid."
    $fileName = (($Name.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')) + ".png"
    $path = [IO.Path]::Combine($screenshotEvidence, $fileName)
    $bitmap = [Drawing.Bitmap]::new($width, $height, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($left, $top, 0, 0, [Drawing.Size]::new($width, $height), [Drawing.CopyPixelOperation]::SourceCopy)
        }
        finally {
            $graphics.Dispose()
        }
        # The fixture owns a #101820 topmost backdrop that extends 28 pixels
        # beyond the target. More than 95% of the crop's outermost perimeter
        # must match it, or the crop is deleted rather than risking unrelated
        # desktop content in the evidence bundle.
        $perimeter = 0
        $matchingBackdrop = 0
        for ($x = 0; $x -lt $width; $x++) {
            foreach ($y in @(0, 1, ($height - 2), ($height - 1))) {
                $pixel = $bitmap.GetPixel($x, $y)
                $perimeter++
                if ([Math]::Abs([int]$pixel.R - 16) -le 2 -and [Math]::Abs([int]$pixel.G - 24) -le 2 -and [Math]::Abs([int]$pixel.B - 32) -le 2) {
                    $matchingBackdrop++
                }
            }
        }
        for ($y = 2; $y -lt ($height - 2); $y++) {
            foreach ($x in @(0, 1, ($width - 2), ($width - 1))) {
                $pixel = $bitmap.GetPixel($x, $y)
                $perimeter++
                if ([Math]::Abs([int]$pixel.R - 16) -le 2 -and [Math]::Abs([int]$pixel.G - 24) -le 2 -and [Math]::Abs([int]$pixel.B - 32) -le 2) {
                    $matchingBackdrop++
                }
            }
        }
        $backdropRatio = [double]$matchingBackdrop / [double]$perimeter
        if ($backdropRatio -lt 0.95) {
            throw "The desktop crop perimeter was not fixture-owned; no desktop-level evidence was retained."
        }
        $bitmap.Save($path, [Drawing.Imaging.ImageFormat]::Png)
    }
    catch {
        if ([IO.File]::Exists($path)) {
            [IO.File]::Delete($path)
        }
        throw
    }
    finally {
        $bitmap.Dispose()
    }
    return [ordered]@{
        file = $fileName
        bytes = ([IO.FileInfo]::new($path)).Length
        sha256 = Get-FileSha256 $path
        crop = [ordered]@{ x = $left; y = $top; width = $width; height = $height; targetMargin = $margin }
        fixtureBackdropRgb = "#101820"
        backdropPerimeterRatio = $backdropRatio
        scope = "fixture-owned target plus 16-pixel indicator band"
        fullDesktopCaptured = $false
    }
}

function Compare-IndicatorBand {
    param([string]$BeforeFile, [string]$DuringFile, [int]$Margin = 16, [int]$InnerBand = 8)
    Add-Type -AssemblyName System.Drawing
    $before = [Drawing.Bitmap]::FromFile([IO.Path]::Combine($screenshotEvidence, $BeforeFile))
    $during = [Drawing.Bitmap]::FromFile([IO.Path]::Combine($screenshotEvidence, $DuringFile))
    try {
        Assert-True ($before.Width -eq $during.Width -and $before.Height -eq $during.Height) "Desktop indicator crops changed dimensions."
        $changed = 0
        $sampled = 0
        $band = $Margin + $InnerBand
        for ($y = 0; $y -lt $before.Height; $y++) {
            for ($x = 0; $x -lt $before.Width; $x++) {
                if ($x -ge $band -and $x -lt ($before.Width - $band) -and $y -ge $band -and $y -lt ($before.Height - $band)) {
                    continue
                }
                $first = $before.GetPixel($x, $y)
                $second = $during.GetPixel($x, $y)
                $sampled++
                if ([Math]::Abs([int]$first.R - [int]$second.R) -gt 8 -or [Math]::Abs([int]$first.G - [int]$second.G) -gt 8 -or [Math]::Abs([int]$first.B - [int]$second.B) -gt 8) {
                    $changed++
                }
            }
        }
        return [ordered]@{
            changedPixels = $changed
            sampledPixels = $sampled
            changedRatio = [double]$changed / [double]$sampled
            comparison = "pre-share versus active-share outer 16px plus inner 8px target-edge band"
        }
    }
    finally {
        $before.Dispose()
        $during.Dispose()
    }
}

function Get-SanitizedFileProvenance {
    param([string]$Path)
    $file = [IO.FileInfo]::new($Path)
    $version = [Diagnostics.FileVersionInfo]::GetVersionInfo($Path).FileVersion
    return [ordered]@{
        bytes = $file.Length
        sha256 = Get-FileSha256 $Path
        fileVersion = if ([String]::IsNullOrWhiteSpace($version)) { $null } else { $version }
        pathRecorded = $false
    }
}

function Get-SanitizedHostProvenance {
    param([object]$Share, [object]$DesktopCrop, [object]$IndicatorDifference)
    $probe = $script:nativeProbeType::Capture()
    return [ordered]@{
        platform = "Windows"
        osVersion = [Environment]::OSVersion.Version.ToString()
        osVersionString = [Environment]::OSVersion.VersionString
        is64BitOperatingSystem = [Environment]::Is64BitOperatingSystem
        processArchitecture = [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
        powershellVersion = $PSVersionTable.PSVersion.ToString()
        powershellEdition = $PSVersionTable.PSEdition
        interactiveSessionId = $sessionId
        inputDesktop = $probe.InputDesktop
        captureBackend = $Share.captureBackend
        selectionMode = $Share.selectionMode
        systemIndicatorPolicy = $Share.systemIndicator
        artifacts = [ordered]@{
            server = $candidateBinding.server
            helper = $candidateBinding.helper
            fixture = Get-SanitizedFileProvenance $resolvedFixture
            runner = Get-SanitizedFileProvenance $PSCommandPath
        }
        candidateBinding = $candidateBinding
        desktopCrop = $DesktopCrop
        indicatorDifference = $IndicatorDifference
        hostnameRecorded = $false
        usernameRecorded = $false
        fullDesktopRecorded = $false
    }
}

function Record-HelperTopologyObservation {
    param([string]$Stage)
    $script:helperTopologyPollCount++
    $signature = @(
        $Stage,
        $script:helperTopologyDiagnostic.supervisorExited,
        $script:helperTopologyDiagnostic.reportedWorkerPid,
        $script:helperTopologyDiagnostic.matchingHelperChildCount,
        (@($script:helperTopologyDiagnostic.matchingHelperChildPids) -join ","),
        $script:helperTopologyDiagnostic.connected,
        $script:helperTopologyDiagnostic.helloStateMatched,
        $script:helperTopologyDiagnostic.workerSessionId,
        $script:helperTopologyDiagnostic.runnerSessionMatched,
        $script:helperTopologyDiagnostic.stablePolls,
        $script:helperTopologyDiagnostic.nativeErrorCode
    ) -join "|"
    if ($signature -ceq $script:helperTopologyLastSignature) {
        return
    }
    $script:helperTopologyLastSignature = $signature
    $script:helperTopologyTransitionCount++
    $script:helperTopologyHistory.Add([ordered]@{
        stage = $Stage
        observedAtUtc = [DateTime]::UtcNow.ToString("o")
        poll = $script:helperTopologyPollCount
        supervisorExited = $script:helperTopologyDiagnostic.supervisorExited
        reportedWorkerPid = $script:helperTopologyDiagnostic.reportedWorkerPid
        matchingHelperChildCount = $script:helperTopologyDiagnostic.matchingHelperChildCount
        matchingHelperChildPids = @($script:helperTopologyDiagnostic.matchingHelperChildPids)
        connected = $script:helperTopologyDiagnostic.connected
        helloStateMatched = $script:helperTopologyDiagnostic.helloStateMatched
        workerSessionId = $script:helperTopologyDiagnostic.workerSessionId
        runnerSessionMatched = $script:helperTopologyDiagnostic.runnerSessionMatched
        stablePolls = $script:helperTopologyDiagnostic.stablePolls
        nativeErrorCode = $script:helperTopologyDiagnostic.nativeErrorCode
    })
    if ($script:helperTopologyHistory.Count -gt 32) {
        $script:helperTopologyHistory.RemoveAt(0)
    }
}

function Wait-ForDirectHelperWorker {
    param(
        [Diagnostics.Process]$SupervisorProcess,
        [string]$ExpectedSessionId,
        [string]$Description
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $stableIdentity = $null
    $stablePolls = 0
    do {
        $SupervisorProcess.Refresh()
        $script:helperTopologyDiagnostic.supervisorPid = $SupervisorProcess.Id
        if ($SupervisorProcess.HasExited) {
            $script:helperTopologyDiagnostic.supervisorExited = $true
            $script:helperTopologyDiagnostic.supervisorExitCode = $SupervisorProcess.ExitCode
            Record-HelperTopologyObservation $Description
            throw "The helper supervisor exited with code $($SupervisorProcess.ExitCode) while waiting for $Description."
        }

        $candidate = Get-LbbState
        $candidateComputer = Get-PropertyValue $candidate "computer"
        $candidateSessionId = [string](Get-PropertyValue $candidateComputer "sessionId")
        $reportedWorkerPid = [int64](Get-PropertyValue $candidateComputer "processId")
        $script:helperTopologyDiagnostic.supervisorExited = $false
        $script:helperTopologyDiagnostic.supervisorExitCode = $null
        $script:helperTopologyDiagnostic.reportedWorkerPid = $reportedWorkerPid
        $script:helperTopologyDiagnostic.matchingHelperChildCount = $null
        $script:helperTopologyDiagnostic.matchingHelperChildPids = @()
        $script:helperTopologyDiagnostic.connected = $candidate.computerConnected -eq $true
        $script:helperTopologyDiagnostic.helloStateMatched = $candidateSessionId -ceq $ExpectedSessionId
        $script:helperTopologyDiagnostic.workerSessionId = $null
        $script:helperTopologyDiagnostic.runnerSessionMatched = $false
        $script:helperTopologyDiagnostic.stablePolls = $stablePolls
        $script:helperTopologyDiagnostic.nativeErrorCode = $null
        try {
            $children = @($script:nativeProbeType::GetDirectChildProcessIds($SupervisorProcess.Id, $resolvedHelper))
        }
        catch [ComponentModel.Win32Exception] {
            $script:helperTopologyDiagnostic.nativeErrorCode = $_.Exception.NativeErrorCode
            Record-HelperTopologyObservation $Description
            throw
        }

        $script:helperTopologyDiagnostic.matchingHelperChildCount = $children.Count
        $script:helperTopologyDiagnostic.matchingHelperChildPids = @($children)

        if ($children.Count -gt 1) {
            $stableIdentity = $null
            $stablePolls = 0
            $script:helperTopologyDiagnostic.stablePolls = 0
            Record-HelperTopologyObservation $Description
            throw "The helper supervisor had multiple exact-image worker children; authority is ambiguous."
        }

        $identity = "$candidateSessionId|$reportedWorkerPid"
        $workerSessionId = $null
        if (
            $candidate.computerConnected -eq $true -and
            -not [String]::IsNullOrWhiteSpace($candidateSessionId) -and
            $candidateSessionId -ceq $ExpectedSessionId -and
            $reportedWorkerPid -gt 0 -and
            $children.Count -eq 1 -and
            [int64]$children[0] -eq $reportedWorkerPid
        ) {
            try {
                $workerSessionId = $script:nativeProbeType::GetProcessSessionId([int]$reportedWorkerPid)
            }
            catch [ComponentModel.Win32Exception] {
                if ($_.Exception.NativeErrorCode -ne 87) {
                    $script:helperTopologyDiagnostic.nativeErrorCode = $_.Exception.NativeErrorCode
                    Record-HelperTopologyObservation $Description
                    throw
                }
                # The exact worker retired between snapshot and session lookup.
                $script:helperTopologyDiagnostic.nativeErrorCode = 87
                $workerSessionId = $null
            }
            if ($null -ne $workerSessionId -and $workerSessionId -ne $sessionId) {
                $script:helperTopologyDiagnostic.workerSessionId = $workerSessionId
                $script:helperTopologyDiagnostic.runnerSessionMatched = $false
                Record-HelperTopologyObservation $Description
                throw "The authenticated helper worker was outside the interactive acceptance session."
            }
            if ($workerSessionId -eq $sessionId) {
                if ($identity -ceq $stableIdentity) {
                    $stablePolls++
                }
                else {
                    $stableIdentity = $identity
                    $stablePolls = 1
                }
                $script:helperTopologyDiagnostic.workerSessionId = $workerSessionId
                $script:helperTopologyDiagnostic.runnerSessionMatched = $true
                $script:helperTopologyDiagnostic.stablePolls = $stablePolls
                Record-HelperTopologyObservation $Description
                if ($stablePolls -ge 2) {
                    $script:helperTopologyChecks.Add([ordered]@{
                        description = $Description
                        supervisorPid = $SupervisorProcess.Id
                        workerPid = [int]$reportedWorkerPid
                        exactImageMatched = $true
                        interactiveSessionId = $workerSessionId
                        stableConsecutivePolls = $stablePolls
                        helloStateMatched = $true
                        protocolRoundTrip = $false
                        roundTripMethod = $null
                    })
                    return [pscustomobject]@{
                        processId = [int]$reportedWorkerPid
                        state = $candidate
                    }
                }
            }
            else {
                $stableIdentity = $null
                $stablePolls = 0
                $script:helperTopologyDiagnostic.workerSessionId = $workerSessionId
                $script:helperTopologyDiagnostic.runnerSessionMatched = $false
                $script:helperTopologyDiagnostic.stablePolls = 0
            }
        }
        else {
            $stableIdentity = $null
            $stablePolls = 0
            $script:helperTopologyDiagnostic.workerSessionId = $workerSessionId
            $script:helperTopologyDiagnostic.runnerSessionMatched = $false
            $script:helperTopologyDiagnostic.stablePolls = 0
        }
        Record-HelperTopologyObservation $Description
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)

    throw "Timed out waiting for $Description (connected=$($script:helperTopologyDiagnostic.connected), helloStateMatched=$($script:helperTopologyDiagnostic.helloStateMatched), reportedWorkerPid=$($script:helperTopologyDiagnostic.reportedWorkerPid), exactImageChildren=$($script:helperTopologyDiagnostic.matchingHelperChildCount), stablePolls=$($script:helperTopologyDiagnostic.stablePolls), topologyTransitions=$script:helperTopologyTransitionCount)."
}

function Complete-HelperTopologyRoundTrip {
    param(
        [string]$Description,
        [string]$ExpectedSessionId,
        [int]$ExpectedProcessId,
        [string]$Method,
        [object]$Response
    )
    $responseState = Get-PropertyValue $Response "state"
    $responseComputer = Get-PropertyValue $responseState "computer"
    Assert-True ([string](Get-PropertyValue $responseComputer "sessionId") -ceq $ExpectedSessionId) "The helper round trip changed its authenticated protocol session."
    Assert-True ([int64](Get-PropertyValue $responseComputer "processId") -eq $ExpectedProcessId) "The helper round trip changed its authenticated worker process identity."

    for ($index = $script:helperTopologyChecks.Count - 1; $index -ge 0; $index--) {
        $check = $script:helperTopologyChecks[$index]
        if ([string]$check["description"] -ceq $Description -and [int]$check["workerPid"] -eq $ExpectedProcessId) {
            $check["protocolRoundTrip"] = $true
            $check["roundTripMethod"] = $Method
            return
        }
    }
    throw "The helper command round trip had no matching process-bound topology record."
}

function New-WatchdogCausalityProof {
    param(
        [DateTime]$FaultTriggeredAtUtc,
        [int64]$EventObservedAfterShareStartMs,
        [int64]$ReplacementObservedAfterShareStartMs,
        [bool]$WatchdogErrorObserved,
        [int64]$ElapsedLowerBoundMs
    )
    $replacementAfterEventMs = $ReplacementObservedAfterShareStartMs - $EventObservedAfterShareStartMs
    $elapsedLowerBoundSatisfied = $replacementAfterEventMs -ge $ElapsedLowerBoundMs
    $causalityProven = $WatchdogErrorObserved -or $elapsedLowerBoundSatisfied
    $causalityMode = if ($WatchdogErrorObserved) {
        "observed-COMPUTER_HELPER_WATCHDOG"
    }
    elseif ($elapsedLowerBoundSatisfied) {
        "elapsed-lower-bound"
    }
    else {
        "unproven"
    }
    return [ordered]@{
        faultTriggeredAtUtc = $FaultTriggeredAtUtc.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        eventObservedAfterShareStartMs = $EventObservedAfterShareStartMs
        replacementObservedAfterShareStartMs = $ReplacementObservedAfterShareStartMs
        replacementObservedAfterEventMs = $replacementAfterEventMs
        watchdogErrorObserved = $WatchdogErrorObserved
        elapsedLowerBoundMs = $ElapsedLowerBoundMs
        elapsedLowerBoundSatisfied = $elapsedLowerBoundSatisfied
        causalityMode = $causalityMode
        causalityProven = $causalityProven
    }
}

$selectedSuites = if ($Suite -contains "All") {
    @("Smoke", "Recovery", "Semantic", "Keyboard", "Pixel", "Capture", "Cancellation")
}
else {
    @($Suite | Select-Object -Unique)
}
if ($selectedSuites.Count -eq 0) {
    throw "At least one suite is required."
}
if ($selectedSuites -contains "Recovery" -and $TimeoutSeconds -lt 25) {
    throw "Recovery requires TimeoutSeconds of at least 25 for the 12-second worker watchdog and bounded supervisor reconnect."
}

$fixtureProcess = $null
$serverProcess = $null
$helperProcess = $null
$pendingCancellationRequest = $null
$script:ownedJob = $null
$script:fixtureReady = $null
$script:baseUrl = "http://127.0.0.1:$Port"
$shareStarted = $false
$runPassed = $false
$failureText = $null
$failureDetails = $null
$script:runStage = "initialize-owned-processes"
$cleanupIssues = [Collections.Generic.List[string]]::new()
$startedAt = [DateTime]::UtcNow
$baselineProbe = $null
$script:lastInvariantPublicationGeneration = 0
$targetWindowId = $null
$targetPid = 0
$recoveryEventName = "Local\LBBTestSharePump-" + [Guid]::NewGuid().ToString("N")
$initialWorkerPid = $null
$initialHelperSessionId = $null
$recoveryEventReleased = $true
$watchdogCausalityMinimumMs = 11500
$foregroundArmRequestDeliveryTimeoutSeconds = 10
# Acceptance-only WM_APP handshake shared with the fixture; never exposed by
# the server or helper protocol.
$foregroundArmMessage = 0x8126
$foregroundArmOperatorRequestId = [Guid]::NewGuid().ToString("N")
$foregroundArmRequestGeneration = [BitConverter]::ToInt32([Guid]::NewGuid().ToByteArray(), 0) -band 0x7fffffff
if ($foregroundArmRequestGeneration -eq 0) {
    $foregroundArmRequestGeneration = 1
}
$script:foregroundArmProof = [ordered]@{
    requestedGeneration = $foregroundArmRequestGeneration
    requestPosted = $false
    requestDelivered = $false
    requestDeliveryTimeoutSeconds = $foregroundArmRequestDeliveryTimeoutSeconds
    acknowledgementMode = "fresh-foreground-focused-left-mouse-down-up"
    nativeForegroundAndFocusRequired = $true
    fixtureRequestedGeneration = $null
    fixtureAcknowledgedGeneration = $null
    fixtureRequestMatched = $false
    fixtureAcknowledgementMatched = $false
    fixtureRequestCount = 0
    fixtureAcknowledgementCount = 0
    fixtureLeftMouseDownCount = 0
    fixtureLeftMouseUpCount = 0
    requestDeliveryPublicationGeneration = $null
    fixtureStatePublicationGeneration = $null
    statePublicationAdvanced = $false
    requestCountMatched = $false
    acknowledgementCountMatched = $false
    leftMouseDownCountMatched = $false
    leftMouseUpCountMatched = $false
    armButtonEnabled = $false
    armButtonIdentityMatched = $false
    nativeTopologyMatched = $false
    foregroundMatched = $false
    foregroundStable = $false
    focusMatched = $false
    focusStable = $false
    cursorAvailable = $false
    cursorStable = $false
    inputDesktopAvailable = $false
    inputDesktopStable = $false
    stableSamplesRequired = 3
    stableSamplesObserved = 0
    stablePublicationSamplesObserved = 0
    timeoutSeconds = $ForegroundArmTimeoutSeconds
    completed = $false
    baselineContinuityMatched = $false
    rawWindowHandlesRecorded = $false
    rawCursorCoordinatesRecorded = $false
}
$tokenPersistenceVerified = $false
$tokenBearingEvidenceRemoved = 0
$script:helperTopologyChecks = [Collections.Generic.List[object]]::new()
$script:helperTopologyHistory = [Collections.Generic.List[object]]::new()
$script:helperTopologyLastSignature = $null
$script:helperTopologyPollCount = 0
$script:helperTopologyTransitionCount = 0
$script:helperTopologyDiagnostic = [ordered]@{
    supervisorPid = $null
    supervisorExited = $null
    supervisorExitCode = $null
    reportedWorkerPid = $null
    matchingHelperChildCount = $null
    matchingHelperChildPids = @()
    connected = $null
    helloStateMatched = $null
    workerSessionId = $null
    runnerSessionMatched = $null
    stablePolls = 0
    nativeErrorCode = $null
    imagePathRecorded = $false
    commandLineRecorded = $false
}

try {
    $script:ownedJob = $script:ownedJobType::new()
    $script:runStage = "start-fixture"
    $hostPath = (Get-Process -Id $PID).Path
    $fixtureArguments = @("-NoLogo", "-NoProfile", "-NonInteractive", "-File", $resolvedFixture, "-EvidenceDirectory", $fixtureEvidence)
    if ($ShowOccluder) {
        $fixtureArguments += "-ShowOccluder"
    }
    $fixtureProcess = Start-IsolatedProcess $hostPath $fixtureArguments @{}
    $script:fixtureReady = Wait-Condition {
        if ($fixtureProcess.HasExited) { throw "The fixture exited during startup." }
        $path = [IO.Path]::Combine($fixtureEvidence, "fixture-ready.json")
        if ([IO.File]::Exists($path)) { return Read-JsonFile $path }
        return $false
    } "the Windows fixture"
    $targetPid = [int]$script:fixtureReady.processId
    Assert-True ($targetPid -eq $fixtureProcess.Id) "The fixture-ready process identity did not match the exact runner-owned fixture process."

    $script:runStage = "start-loopback-server"
    $processEnvironment = @{
        LBB_PORT = $Port.ToString([Globalization.CultureInfo]::InvariantCulture)
        LBB_TOKEN = $Token
        LBB_DISABLE_UPDATE_CHECK = "1"
    }
    $serverProcess = Start-IsolatedProcess $resolvedServer @("--no-update-check") $processEnvironment
    Wait-Condition {
        if ($serverProcess.HasExited) { throw "The server exited during startup." }
        try {
            $null = Get-LbbState
            return $true
        }
        catch { return $false }
    } "the loopback server" | Out-Null

    $script:runStage = "start-computer-helper"
    $helperEnvironment = @{}
    foreach ($item in $processEnvironment.GetEnumerator()) {
        $helperEnvironment[[string]$item.Key] = [string]$item.Value
    }
    if ($selectedSuites -contains "Recovery") {
        Assert-True ($script:nativeProbeType::GetKernelEventState($recoveryEventName) -eq 0) "The unique one-shot recovery event already existed."
        $helperEnvironment["LBB_TEST_STALL_SHARE_PUMP_ONCE_EVENT"] = $recoveryEventName
    }
    $helperProcess = Start-IsolatedProcess $resolvedHelper @() $helperEnvironment
    $bridgeState = Wait-Condition {
        if ($helperProcess.HasExited) {
            $script:helperTopologyDiagnostic.supervisorPid = $helperProcess.Id
            $script:helperTopologyDiagnostic.supervisorExited = $true
            $script:helperTopologyDiagnostic.supervisorExitCode = $helperProcess.ExitCode
            throw "The computer helper supervisor exited with code $($helperProcess.ExitCode) during startup."
        }
        $candidate = Get-LbbState
        if ($candidate.computerConnected -eq $true -and $null -ne $candidate.computer) { return $candidate }
        return $false
    } "the authenticated computer helper"
    $script:runStage = "bind-initial-helper-readiness"
    $initialHelperSessionId = [string]$bridgeState.computer.sessionId
    Assert-True (-not [String]::IsNullOrWhiteSpace($initialHelperSessionId)) "The initial helper session identity was missing."
    $initialWorker = Wait-ForDirectHelperWorker $helperProcess $initialHelperSessionId "the initial disposable helper worker"
    $initialWorkerPid = [int]$initialWorker.processId
    $bridgeState = $initialWorker.state
    $readinessProbe = Invoke-LbbCommand "computer.status" @{}
    Complete-HelperTopologyRoundTrip "the initial disposable helper worker" $initialHelperSessionId $initialWorkerPid "computer.status" $readinessProbe
    Save-StepResponse "protocol-bound helper readiness" $readinessProbe
    if ($selectedSuites -contains "Recovery") {
        $script:runStage = "wait-recovery-event-ready"
        $null = Wait-Condition {
            if ($script:nativeProbeType::GetKernelEventState($recoveryEventName) -eq 1) { return $true }
            return $false
        } "the supervisor-owned unsignaled one-shot recovery event"
    }

    $script:runStage = "select-exact-fixture-window"
    $matchingWindows = @($bridgeState.computer.windows | Where-Object {
        [int]$_.pid -eq $targetPid -and $_.title -eq "LBB Windows Fixture Target"
    })
    Assert-True ($matchingWindows.Count -eq 1) "The helper did not enumerate exactly one fixture target window."
    $targetWindowId = [string]$matchingWindows[0].id
    Assert-True ($targetWindowId -eq [string]$script:fixtureReady.targetHwnd) "The helper window ID did not match the fixture-owned HWND."

    $sentinelHwnd = [Int64]$script:fixtureReady.sentinelHwnd
    $armButtonHwnd = [Int64]$script:fixtureReady.armButtonHwnd
    Assert-True ($sentinelHwnd -gt 0) "The fixture did not publish a valid foreground sentinel HWND."
    Assert-True ($armButtonHwnd -gt 0) "The fixture did not publish a valid foreground-arm button HWND."
    Assert-True ($script:nativeProbeType::ValidateFixtureArmTopology($sentinelHwnd, $armButtonHwnd, $fixtureProcess.Id)) "The foreground sentinel and arm button are not a current same-thread fixture-owned root/child pair."
    $script:runStage = "request-foreground-arm"
    $armRequestPosted = $script:nativeProbeType::PostMessage(
        [IntPtr]$sentinelHwnd,
        $foregroundArmMessage,
        [IntPtr]$foregroundArmRequestGeneration,
        [IntPtr]::Zero
    )
    Assert-True ($armRequestPosted -eq $true) "The runner could not post the test-only foreground-arm request to the exact sentinel."
    $script:foregroundArmProof.requestPosted = $true
    $script:runStage = "wait-foreground-arm-request-delivery"
    $armRequestDelivery = Wait-ForFixtureProof `
        -FixturePredicate {
            param($state)
            return Test-ForegroundArmRequestDeliveryState $state $foregroundArmRequestGeneration $armButtonHwnd
        } `
        -Description "the exact foreground-arm request delivery and enabled button receipt" `
        -StateReader { Get-FixtureStateSnapshot } `
        -TimeoutMilliseconds ($foregroundArmRequestDeliveryTimeoutSeconds * 1000)
    $armRequestTopologyMatched = $script:nativeProbeType::ValidateFixtureArmTopology($sentinelHwnd, $armButtonHwnd, $fixtureProcess.Id)
    Assert-True ($armRequestTopologyMatched -eq $true) "The fixture-owned foreground-arm topology changed during request delivery."
    $script:foregroundArmProof.requestDelivered = $true
    $script:foregroundArmProof.armButtonEnabled = $true
    $script:foregroundArmProof.nativeTopologyMatched = $true
    $armRequestInputState = if ([int]$armRequestDelivery.foregroundArmAcknowledgedGeneration -eq $foregroundArmRequestGeneration) { "already-acknowledged" } else { "not-started" }
    Save-StepRecord "foreground arm request delivery" ([ordered]@{
        requestedGeneration = $foregroundArmRequestGeneration
        requestPosted = $true
        requestDelivered = $true
        requestDeliveryTimeoutSeconds = $foregroundArmRequestDeliveryTimeoutSeconds
        fixtureRequestMatched = [int]$armRequestDelivery.foregroundArmRequestedGeneration -eq $foregroundArmRequestGeneration
        fixtureInputState = $armRequestInputState
        fixtureRequestCount = [int]$armRequestDelivery.foregroundArmRequestCount
        fixtureAcknowledgementCount = [int]$armRequestDelivery.foregroundArmAcknowledgementCount
        fixtureLeftMouseDownCount = [int]$armRequestDelivery.foregroundArmLeftMouseDownCount
        fixtureLeftMouseUpCount = [int]$armRequestDelivery.foregroundArmLeftMouseUpCount
        fixtureStatePublicationGeneration = [Int64]$armRequestDelivery.statePublicationGeneration
        armButtonEnabled = $armRequestDelivery.foregroundArmButtonEnabled -eq $true
        armButtonIdentityMatched = [Int64]$armRequestDelivery.armButtonHwnd -eq $armButtonHwnd
        nativeTopologyMatched = $armRequestTopologyMatched
        rawWindowHandlesRecorded = $false
        rawCursorCoordinatesRecorded = $false
    })
    $foregroundArmRequestMarker = New-ForegroundArmRequestMarker `
        -ProductVersion $Version `
        -RequestId $foregroundArmOperatorRequestId `
        -InputStateAtPublication $armRequestInputState `
        -TimeoutSeconds $ForegroundArmTimeoutSeconds `
        -RequestDelivered $true `
        -ButtonEnabled ($armRequestDelivery.foregroundArmButtonEnabled -eq $true) `
        -NativeTopologyMatched $armRequestTopologyMatched
    $null = Write-NewOperatorMarker `
        -Directory $operatorEvidence `
        -FileName "foreground-arm-request.json" `
        -Value $foregroundArmRequestMarker
    Write-Host "ACTION REQUIRED: Through a separately authorized Windows Computer Use app share, visually confirm the orange LBB Foreground Sentinel and click CLICK TO ARM exactly once within $ForegroundArmTimeoutSeconds seconds. If it already says ARMED or the outcome is uncertain, do not click or retry. Stop all Windows UI use after the action."
    $script:runStage = "wait-foreground-arm"
    $foregroundArm = Wait-ForStableForegroundArm `
        -RequestedGeneration $foregroundArmRequestGeneration `
        -SentinelHwnd $sentinelHwnd `
        -ArmButtonHwnd $armButtonHwnd `
        -ExpectedFixtureProcessId $fixtureProcess.Id `
        -RequestDeliveryTimeoutSeconds $foregroundArmRequestDeliveryTimeoutSeconds `
        -AfterPublicationGeneration ([Int64]$armRequestDelivery.statePublicationGeneration) `
        -RequiredStableSamples 3 `
        -TimeoutMilliseconds ($ForegroundArmTimeoutSeconds * 1000)
    $script:foregroundArmProof.requestPosted = $true
    $foregroundArmReceivedMarker = New-ForegroundArmReceivedMarker `
        -ProductVersion $Version `
        -RequestId $foregroundArmOperatorRequestId `
        -Proof $foregroundArm.proof
    $null = Write-NewOperatorMarker `
        -Directory $operatorEvidence `
        -FileName "foreground-arm-received.json" `
        -Value $foregroundArmReceivedMarker
    $baselineProbe = Capture-InvariantProbe -AfterStatePublicationGeneration ([Int64]$foregroundArm.fixtureState.statePublicationGeneration)
    $script:lastInvariantPublicationGeneration = [Int64]$baselineProbe.statePublicationGeneration
    $armNativeSample = $foregroundArm.nativeSample
    Assert-True ($script:nativeProbeType::ValidateFixtureArmTopology($sentinelHwnd, $armButtonHwnd, $fixtureProcess.Id)) "The fixture-owned foreground-arm topology changed before the invariant baseline."
    Assert-True ($baselineProbe.foregroundHwnd -eq $armNativeSample.ForegroundHwnd.ToString()) "The foreground window changed between the accepted arm sample and the invariant baseline."
    Assert-True ($baselineProbe.focusHwnd -eq $armNativeSample.FocusHwnd.ToString()) "Foreground focus changed between the accepted arm sample and the invariant baseline."
    Assert-True ($armNativeSample.CursorAvailable -eq $true -and $baselineProbe.cursorAvailable -eq $true) "The OS-global cursor position was unavailable at the arm-to-baseline boundary."
    Assert-True ($baselineProbe.cursor.x -eq $armNativeSample.CursorX -and $baselineProbe.cursor.y -eq $armNativeSample.CursorY) "The OS-global cursor position moved between the accepted arm sample and the invariant baseline."
    Assert-True ($baselineProbe.inputDesktop -ceq [string]$armNativeSample.InputDesktop) "The input desktop changed between the accepted arm sample and the invariant baseline."
    Assert-True ($baselineProbe.foregroundArmRequestedGeneration -eq $foregroundArmRequestGeneration) "The foreground-arm request generation changed before the invariant baseline."
    Assert-True ($baselineProbe.foregroundArmAcknowledgedGeneration -eq $foregroundArmRequestGeneration) "The foreground-arm acknowledgement generation changed before the invariant baseline."
    Assert-True ($baselineProbe.foregroundArmRequestCount -eq 1) "The foreground-arm request count changed before the invariant baseline."
    Assert-True ($baselineProbe.foregroundArmAcknowledgementCount -eq 1) "The foreground-arm acknowledgement count changed before the invariant baseline."
    Assert-True ($baselineProbe.foregroundArmLeftMouseDownCount -eq 1) "The foreground-arm left-mouse-down count changed before the invariant baseline."
    Assert-True ($baselineProbe.foregroundArmLeftMouseUpCount -eq 1) "The foreground-arm left-mouse-up count changed before the invariant baseline."
    Assert-True ($baselineProbe.foregroundArmButtonEnabled -eq $true) "The foreground-arm button-enabled receipt was lost before the invariant baseline."
    Assert-True ($baselineProbe.statePublicationGeneration -gt [Int64]$foregroundArm.fixtureState.statePublicationGeneration) "The fixture state did not publish a fresh arm-to-baseline boundary."
    $script:foregroundArmProof.baselineContinuityMatched = $true
    Assert-True ($baselineProbe.foregroundHwnd -eq [string]$script:fixtureReady.sentinelHwnd) "The test-owned sentinel is not the foreground window."
    Assert-True ($baselineProbe.focusHwnd -eq [string]$script:fixtureReady.armButtonHwnd) "The exact foreground-arm button does not retain foreground focus."
    Assert-True ($baselineProbe.cursorAvailable -eq $true) "The OS-global cursor position became unavailable after foreground arming."
    Save-StepRecord "foreground arm proof" $script:foregroundArmProof

    $script:runStage = "rebind-post-arm-helper-readiness"
    $postArmHelperDescription = "the original helper worker after foreground arming"
    $postArmWorker = Wait-ForDirectHelperWorker $helperProcess $initialHelperSessionId $postArmHelperDescription
    Assert-True ([int]$postArmWorker.processId -eq $initialWorkerPid) "The helper worker changed while foreground arming was pending."
    $bridgeState = $postArmWorker.state

    $script:runStage = "baseline-status-and-observation"
    $statusResponse = Invoke-LbbCommand "computer.status" @{}
    Complete-HelperTopologyRoundTrip $postArmHelperDescription $initialHelperSessionId $initialWorkerPid "computer.status" $statusResponse
    Save-StepResponse "post-arm protocol-bound helper continuity" $statusResponse
    Assert-True ($statusResponse.result.inputReady -eq $true) "The helper did not report pixel input readiness."
    Assert-True ($statusResponse.result.semanticReady -eq $true) "The helper did not report semantic input readiness."

    $observationResponse = Invoke-LbbCommand "computer.observe" @{ windowId = $targetWindowId }
    Save-StepResponse "baseline exact window observe" $observationResponse
    $observation = $observationResponse.state.computerObservation
    Assert-True ([string]$observation.windowId -eq $targetWindowId) "Observation escaped the exact fixture HWND."
    Assert-True ([int]$observation.pid -eq $targetPid) "Observation escaped the fixture process."
    $baselineShot = Save-ObservationScreenshot $observation "00-baseline-observe"
    Save-StepRecord "baseline screenshot" $baselineShot
    $null = Assert-InvariantsHeld $baselineProbe "Baseline observation"

    if ($selectedSuites -contains "Recovery") {
        $script:runStage = "recovery-suite"
        $supervisorPidBefore = $helperProcess.Id
        $serverPidBefore = $serverProcess.Id
        $faultTriggeredAtUtc = [DateTime]::UtcNow
        $faultStopwatch = [Diagnostics.Stopwatch]::StartNew()
        $faultStart = Invoke-LbbCommand "computer.share.start" @{ windowId = $targetWindowId; fps = 4 }
        $shareStarted = $true
        Save-StepResponse "one-shot share pump stall start" $faultStart
        $null = Wait-Condition {
            if ($script:nativeProbeType::GetKernelEventState($recoveryEventName) -eq 2) { return $true }
            return $false
        } "the signaled launch-time-only share-pump stall event"
        $faultEventObservedElapsedMs = [int64]$faultStopwatch.ElapsedMilliseconds

        $recoveryDeadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        $statePolls = 0
        $statePollFailures = 0
        $disconnectObserved = $false
        $watchdogErrorObserved = $false
        $recoveredBridgeState = $null
        $replacementObservedElapsedMs = $null
        do {
            Assert-True (-not $serverProcess.HasExited -and $serverProcess.Id -eq $serverPidBefore) "The loopback server exited during disposable-worker recovery."
            Assert-True (-not $helperProcess.HasExited -and $helperProcess.Id -eq $supervisorPidBefore) "The helper supervisor exited instead of replacing only its disposable worker."
            try {
                $candidate = Get-LbbState
                $statePolls++
                if ($candidate.computerConnected -ne $true) {
                    $disconnectObserved = $true
                }
                $candidateComputer = Get-PropertyValue $candidate "computer"
                $candidateShare = Get-PropertyValue $candidateComputer "share"
                if ((Get-PropertyValue $candidateShare "code") -eq "COMPUTER_HELPER_WATCHDOG") {
                    $watchdogErrorObserved = $true
                }
                $candidateSession = [string](Get-PropertyValue $candidateComputer "sessionId")
                if ($candidate.computerConnected -eq $true -and -not [String]::IsNullOrWhiteSpace($candidateSession) -and $candidateSession -ne $initialHelperSessionId) {
                    $recoveredBridgeState = $candidate
                    $replacementObservedElapsedMs = [int64]$faultStopwatch.ElapsedMilliseconds
                    break
                }
            }
            catch {
                $statePollFailures++
                throw "The loopback server API stopped responding during disposable-worker recovery."
            }
            Start-Sleep -Milliseconds 200
        } while ([DateTime]::UtcNow -lt $recoveryDeadline)

        Assert-True ($null -ne $recoveredBridgeState) "The helper supervisor did not reconnect a replacement worker within the bounded recovery window."
        $faultStopwatch.Stop()
        $watchdogCausalityProof = New-WatchdogCausalityProof `
            -FaultTriggeredAtUtc $faultTriggeredAtUtc `
            -EventObservedAfterShareStartMs $faultEventObservedElapsedMs `
            -ReplacementObservedAfterShareStartMs $replacementObservedElapsedMs `
            -WatchdogErrorObserved $watchdogErrorObserved `
            -ElapsedLowerBoundMs $watchdogCausalityMinimumMs
        Save-StepRecord "share pump watchdog causality proof" $watchdogCausalityProof
        Assert-True $watchdogCausalityProof.causalityProven "Worker replacement lacked COMPUTER_HELPER_WATCHDOG evidence and occurred too early to prove the 12-second share-pump watchdog caused it."
        $shareStarted = $false
        $replacementSessionId = [string]$recoveredBridgeState.computer.sessionId
        $replacementWorker = Wait-ForDirectHelperWorker $helperProcess $replacementSessionId "the replacement disposable helper worker"
        $replacementWorkerPid = [int]$replacementWorker.processId
        $recoveredBridgeState = $replacementWorker.state
        $replacementReadiness = Invoke-LbbCommand "computer.status" @{}
        Complete-HelperTopologyRoundTrip "the replacement disposable helper worker" $replacementSessionId $replacementWorkerPid "computer.status" $replacementReadiness
        Save-StepResponse "replacement protocol-bound helper readiness" $replacementReadiness
        Assert-True ($replacementWorkerPid -ne $initialWorkerPid) "The helper supervisor did not replace the stalled worker process."
        Assert-True ($replacementSessionId -ne $initialHelperSessionId) "The replacement worker reused the stale protocol session."
        Assert-True ($helperProcess.Id -eq $supervisorPidBefore -and -not $helperProcess.HasExited) "The helper supervisor identity did not survive worker replacement."
        Assert-True ($serverProcess.Id -eq $serverPidBefore -and -not $serverProcess.HasExited) "The loopback server identity did not survive worker replacement."
        Assert-True ($statePolls -gt 0 -and $statePollFailures -eq 0) "The loopback server was not continuously queryable during worker replacement."

        $recoveryObserve = Invoke-LbbCommand "computer.observe" @{ windowId = $targetWindowId }
        Save-StepResponse "replacement worker fresh observe" $recoveryObserve
        $observation = $recoveryObserve.state.computerObservation
        $recoveryObserveShot = Save-ObservationScreenshot $observation "05-recovery-fresh-observe"
        Save-StepRecord "replacement worker fresh observe screenshot" $recoveryObserveShot

        $recoveryShareStart = Invoke-LbbCommand "computer.share.start" @{ windowId = $targetWindowId; fps = 4 }
        $shareStarted = $true
        Save-StepResponse "replacement worker fresh share start" $recoveryShareStart
        $recoveryShareFrame = Wait-Condition {
            $candidate = Get-CurrentObservation
            if ($candidate.share.active -eq $true -and [int64]$candidate.share.sequence -gt 0) { return $candidate }
            return $false
        } "a live native frame from the replacement worker"
        $recoveryShareShot = Save-ObservationScreenshot $recoveryShareFrame "06-recovery-fresh-share"
        Save-StepRecord "replacement worker fresh share screenshot" $recoveryShareShot
        $recoveryShareStop = Invoke-LbbCommand "computer.share.stop" @{}
        $shareStarted = $false
        Save-StepResponse "replacement worker fresh share stop" $recoveryShareStop

        Save-StepRecord "disposable worker recovery proof" ([ordered]@{
            faultHook = "launch-time-only one-shot share-pump stall kernel event"
            eventSignaled = $script:nativeProbeType::GetKernelEventState($recoveryEventName) -eq 2
            eventName = $recoveryEventName
            serverPidBefore = $serverPidBefore
            serverPidAfter = $serverProcess.Id
            supervisorPidBefore = $supervisorPidBefore
            supervisorPidAfter = $helperProcess.Id
            workerPidBefore = $initialWorkerPid
            workerPidAfter = $replacementWorkerPid
            helperSessionBefore = $initialHelperSessionId
            helperSessionAfter = $replacementSessionId
            disconnectObserved = $disconnectObserved
            watchdogErrorObserved = $watchdogErrorObserved
            watchdogCausality = $watchdogCausalityProof
            successfulServerStatePolls = $statePolls
            failedServerStatePolls = $statePollFailures
            recoveredObserveFrameId = $observation.frameId
            recoveredShareFrameId = $recoveryShareFrame.frameId
            remoteProtocolFaultMethodAdded = $false
        })
        $null = Assert-InvariantsHeld $baselineProbe "Disposable worker recovery"
    }

    if ($selectedSuites -contains "Semantic") {
        $script:runStage = "semantic-suite"
        $semanticElement = @($observation.elements | Where-Object {
            $_.name -eq "Fixture Value Input" -and $_.actions -contains "setValue"
        })
        Assert-True ($semanticElement.Count -eq 1) "UI Automation did not expose the fixture ValuePattern field exactly once."
        $semanticText = "semantic-background-proof"
        $response = Invoke-LbbCommand "computer.setValue" @{
            frameId = [string]$observation.frameId
            elementRef = [string]$semanticElement[0].ref
            value = $semanticText
        }
        Save-StepResponse "semantic set value" $response
        $semanticState = Wait-ForFixtureProof {
            param($state)
            return [string]$state.semanticValue.sha256 -eq (Get-TextHash $semanticText)
        } "the fixture ValuePattern postcondition"
        $observation = $response.state.computerObservation
        $semanticShot = Save-ObservationScreenshot $observation "10-semantic-set-value"
        Save-StepRecord "semantic set value screenshot" $semanticShot
        $null = Assert-InvariantsHeld $baselineProbe "computer.setValue"

        $button = @($observation.elements | Where-Object {
            $_.name -eq "Increment Counter" -and $_.actions -contains "press"
        })
        Assert-True ($button.Count -eq 1) "UI Automation did not expose the fixture InvokePattern button exactly once."
        $beforeInvoke = [int]$semanticState.invokeCount
        $response = Invoke-LbbCommand "computer.invoke" @{
            frameId = [string]$observation.frameId
            elementRef = [string]$button[0].ref
            action = "press"
        }
        Save-StepResponse "semantic invoke" $response
        $null = Wait-ForFixtureProof {
            param($state)
            return [int]$state.invokeCount -eq ($beforeInvoke + 1)
        } "the fixture InvokePattern postcondition"
        $observation = $response.state.computerObservation
        $invokeShot = Save-ObservationScreenshot $observation "11-semantic-invoke"
        Save-StepRecord "semantic invoke screenshot" $invokeShot
        $null = Assert-InvariantsHeld $baselineProbe "computer.invoke"
    }

    if ($selectedSuites -contains "Keyboard") {
        $script:runStage = "keyboard-suite"
        $typedText = "typed-background-proof"
        $response = Invoke-LbbCommand "computer.typeText" @{
            frameId = [string]$observation.frameId
            text = $typedText
        }
        Save-StepResponse "background type text" $response
        $keyboardState = Wait-ForFixtureProof {
            param($state)
            return [string]$state.focusedText.sha256 -eq (Get-TextHash $typedText)
        } "the focused text postcondition"
        $observation = $response.state.computerObservation
        $typeShot = Save-ObservationScreenshot $observation "20-background-type-text"
        Save-StepRecord "background type text screenshot" $typeShot
        $null = Assert-InvariantsHeld $baselineProbe "computer.typeText"

        $beforeKeys = $keyboardState.messageCounters
        $response = Invoke-LbbCommand "computer.key" @{
            frameId = [string]$observation.frameId
            key = "F6"
        }
        Save-StepResponse "background key" $response
        $observation = $response.state.computerObservation
        $keyShot = Save-ObservationScreenshot $observation "21-background-key-f6"
        Save-StepRecord "background key F6 screenshot" $keyShot
        $null = Wait-ForFixtureProof {
            param($state)
            return [int]$state.messageCounters.keyDown -gt [int]$beforeKeys.keyDown -and [int]$state.messageCounters.keyUp -gt [int]$beforeKeys.keyUp
        } "WM_KEYDOWN and WM_KEYUP"
        $null = Assert-InvariantsHeld $baselineProbe "computer.key F6"

        $beforeSystemKeys = (Get-FixtureState).messageCounters
        $response = Invoke-LbbCommand "computer.key" @{
            frameId = [string]$observation.frameId
            key = "Alt+A"
        }
        Save-StepResponse "background system key" $response
        $observation = $response.state.computerObservation
        $systemKeyShot = Save-ObservationScreenshot $observation "22-background-system-key-alt-a"
        Save-StepRecord "background system key Alt+A screenshot" $systemKeyShot
        $null = Wait-ForFixtureProof {
            param($state)
            return [int]$state.messageCounters.sysKeyDown -gt [int]$beforeSystemKeys.sysKeyDown -and [int]$state.messageCounters.sysKeyUp -gt [int]$beforeSystemKeys.sysKeyUp
        } "WM_SYSKEYDOWN and WM_SYSKEYUP"
        $events = @([IO.File]::ReadAllLines([IO.Path]::Combine($fixtureEvidence, "fixture-events.ndjson")) | ForEach-Object { $_ | ConvertFrom-Json })
        $systemDown = @($events | Where-Object { $_.event -eq "sysKeyDown" -and $_.source -eq "focusedTextInput" } | Select-Object -Last 1)
        $systemUp = @($events | Where-Object { $_.event -eq "sysKeyUp" -and $_.source -eq "focusedTextInput" } | Select-Object -Last 1)
        Assert-True ($systemDown.Count -eq 1 -and $systemDown[0].repeatCount -eq 1 -and $systemDown[0].scanCode -gt 0 -and $systemDown[0].altContext -eq $true -and $systemDown[0].previousState -eq $false -and $systemDown[0].transitionState -eq $false) "WM_SYSKEYDOWN carried an invalid lParam."
        Assert-True ($systemUp.Count -eq 1 -and $systemUp[0].repeatCount -eq 1 -and $systemUp[0].scanCode -gt 0 -and $systemUp[0].altContext -eq $true -and $systemUp[0].previousState -eq $true -and $systemUp[0].transitionState -eq $true) "WM_SYSKEYUP carried an invalid lParam."
        Save-StepRecord "key message lparam proof" ([ordered]@{
            wmKey = "observed"
            wmSysKeyDown = ConvertTo-SafeObject $systemDown[0]
            wmSysKeyUp = ConvertTo-SafeObject $systemUp[0]
        })
        $null = Assert-InvariantsHeld $baselineProbe "computer.key Alt+A"
    }

    if ($selectedSuites -contains "Pixel") {
        $script:runStage = "pixel-suite"
        $from = Get-SurfacePoint $observation 0.30 0.45
        $to = Get-SurfacePoint $observation 0.72 0.68
        $beforePixel = (Get-FixtureState).messageCounters

        $response = Invoke-LbbCommand "computer.move" @{
            frameId = [string]$observation.frameId
            x = $from.x
            y = $from.y
            coordinateSpace = "image"
            durationMs = 120
        }
        Save-StepResponse "pixel move" $response
        $observation = $response.state.computerObservation
        $moveShot = Save-ObservationScreenshot $observation "30-pixel-move"
        Save-StepRecord "pixel move screenshot" $moveShot
        $null = Wait-ForFixtureProof { param($state) return [int]$state.messageCounters.mouseMove -gt [int]$beforePixel.mouseMove } "WM_MOUSEMOVE"
        $null = Assert-InvariantsHeld $baselineProbe "computer.move"

        $beforeClick = (Get-FixtureState).messageCounters
        $response = Invoke-LbbCommand "computer.click" @{
            frameId = [string]$observation.frameId
            x = $from.x
            y = $from.y
            coordinateSpace = "image"
            button = "left"
            clickCount = 1
            durationMs = 80
        }
        Save-StepResponse "pixel click" $response
        $observation = $response.state.computerObservation
        $clickShot = Save-ObservationScreenshot $observation "31-pixel-click"
        Save-StepRecord "pixel click screenshot" $clickShot
        $null = Wait-ForFixtureProof {
            param($state)
            return [int]$state.messageCounters.mouseDown -gt [int]$beforeClick.mouseDown -and [int]$state.messageCounters.mouseUp -gt [int]$beforeClick.mouseUp
        } "mouse down and up"
        $null = Assert-InvariantsHeld $baselineProbe "computer.click"

        $beforeDouble = (Get-FixtureState).messageCounters
        $response = Invoke-LbbCommand "computer.click" @{
            frameId = [string]$observation.frameId
            x = $from.x
            y = $from.y
            coordinateSpace = "image"
            button = "left"
            clickCount = 2
            durationMs = 80
        }
        Save-StepResponse "pixel double click" $response
        $observation = $response.state.computerObservation
        $doubleShot = Save-ObservationScreenshot $observation "32-pixel-double-click"
        Save-StepRecord "pixel double click screenshot" $doubleShot
        $null = Wait-ForFixtureProof { param($state) return [int]$state.messageCounters.mouseDoubleClick -gt [int]$beforeDouble.mouseDoubleClick } "WM_LBUTTONDBLCLK"
        $null = Assert-InvariantsHeld $baselineProbe "computer.click count 2"

        $beforeDrag = (Get-FixtureState).messageCounters
        $response = Invoke-LbbCommand "computer.drag" @{
            frameId = [string]$observation.frameId
            fromX = $from.x
            fromY = $from.y
            toX = $to.x
            toY = $to.y
            coordinateSpace = "image"
            durationMs = 240
        }
        Save-StepResponse "pixel drag" $response
        $observation = $response.state.computerObservation
        $dragShot = Save-ObservationScreenshot $observation "33-pixel-drag"
        Save-StepRecord "pixel drag screenshot" $dragShot
        $null = Wait-ForFixtureProof { param($state) return [int]$state.messageCounters.dragMove -gt [int]$beforeDrag.dragMove } "a pressed WM_MOUSEMOVE drag path"
        $null = Assert-InvariantsHeld $baselineProbe "computer.drag"

        $beforeScroll = (Get-FixtureState).messageCounters
        $response = Invoke-LbbCommand "computer.scroll" @{
            frameId = [string]$observation.frameId
            x = $to.x
            y = $to.y
            coordinateSpace = "image"
            deltaX = 1
            deltaY = -1
        }
        Save-StepResponse "pixel scroll" $response
        $observation = $response.state.computerObservation
        $scrollShot = Save-ObservationScreenshot $observation "34-pixel-scroll"
        Save-StepRecord "pixel scroll screenshot" $scrollShot
        $null = Wait-ForFixtureProof {
            param($state)
            return [int]$state.messageCounters.mouseWheel -gt [int]$beforeScroll.mouseWheel -and [int]$state.messageCounters.mouseHWheel -gt [int]$beforeScroll.mouseHWheel
        } "vertical and horizontal wheel messages"
        $null = Assert-InvariantsHeld $baselineProbe "computer.scroll"
    }

    if ($selectedSuites -contains "Capture") {
        $script:runStage = "capture-suite"
        $desktopBeforeShare = Save-SanitizedDesktopCrop "39-desktop-crop-before-share"
        Save-StepRecord "sanitized desktop crop before share" $desktopBeforeShare
        $response = Invoke-LbbCommand "computer.share.start" @{ windowId = $targetWindowId; fps = 4 }
        $shareStarted = $true
        Save-StepResponse "native share start" $response
        $firstShare = Wait-Condition {
            $candidate = Get-CurrentObservation
            if ($candidate.share.active -eq $true -and [int64]$candidate.share.sequence -gt 0) { return $candidate }
            return $false
        } "the first native share frame"
        Assert-True ($firstShare.share.nativeStream -eq $true) "The share did not report a native stream."
        Assert-True ($firstShare.share.captureScope -eq "exact-window") "The share did not report exact-window scope."
        Assert-True ($firstShare.share.systemIndicator -eq $true) "The helper did not report its required no-suppression capture-indicator policy."
        $shareShot1 = Save-ObservationScreenshot $firstShare "40-native-share-frame-1"
        Start-Sleep -Milliseconds 700
        $secondShare = Wait-Condition {
            $candidate = Get-CurrentObservation
            if ([int64]$candidate.share.sequence -gt [int64]$firstShare.share.sequence) { return $candidate }
            return $false
        } "a later native share frame"
        $shareShot2 = Save-ObservationScreenshot $secondShare "41-native-share-frame-2"
        Assert-True ($shareShot2.sha256 -ne $shareShot1.sha256) "Animated native-share frames did not change."
        $desktopDuringShare = Save-SanitizedDesktopCrop "41-desktop-crop-during-share"
        $indicatorDifference = Compare-IndicatorBand $desktopBeforeShare.file $desktopDuringShare.file
        Assert-True ([int]$indicatorDifference.changedPixels -ge 20) "The sanitized desktop-level crop did not visibly prove a Windows-owned capture indicator around the fixture target."
        $hostProvenance = Get-SanitizedHostProvenance $secondShare.share $desktopDuringShare $indicatorDifference
        Save-StepRecord "windows capture indicator and host provenance" $hostProvenance
        $magentaPixels = $null
        if ($ShowOccluder) {
            $magentaPixels = Get-MagentaPixelCount $shareShot2.file
            Assert-True ($magentaPixels -eq 0) "The exact-window capture leaked the magenta occluder."
        }
        Save-StepRecord "native share frame progression" ([ordered]@{
            first = $shareShot1
            second = $shareShot2
            firstSequence = $firstShare.share.sequence
            secondSequence = $secondShare.share.sequence
            sourceDroppedFrames = $secondShare.share.sourceDroppedFrames
            transportDroppedFrames = $secondShare.share.transportDroppedFrames
            magentaSampleCount = $magentaPixels
            indicatorPolicyReported = $firstShare.share.systemIndicator
            indicatorVisibleRuntimeProof = [ordered]@{
                before = $desktopBeforeShare.file
                during = $desktopDuringShare.file
                changedPixels = $indicatorDifference.changedPixels
                sampledPixels = $indicatorDifference.sampledPixels
            }
            indicatorEvidenceBoundary = "The exact-window screenshots are non-proof; the separately sanitized fixture-region desktop crop provides the runtime border comparison."
        })
        $null = Assert-InvariantsHeld $baselineProbe "computer.share.start"

        $sharePoint = Get-SurfacePoint $secondShare 0.50 0.50
        $sequenceBeforeAction = [int64]$secondShare.share.sequence
        $response = Invoke-LbbCommand "computer.move" @{
            frameId = [string]$secondShare.frameId
            x = $sharePoint.x
            y = $sharePoint.y
            coordinateSpace = "image"
            durationMs = 160
        }
        Save-StepResponse "native share action" $response
        $afterShareAction = Wait-Condition {
            $candidate = Get-CurrentObservation
            if ($candidate.share.active -eq $true -and [int64]$candidate.share.sequence -gt $sequenceBeforeAction) { return $candidate }
            return $false
        } "native share progression after an action"
        $shareActionShot = Save-ObservationScreenshot $afterShareAction "42-native-share-after-action"
        Save-StepRecord "native share after action screenshot" $shareActionShot
        $null = Assert-InvariantsHeld $baselineProbe "native-share computer.move"

        $response = Invoke-LbbCommand "computer.share.stop" @{}
        $shareStarted = $false
        Save-StepResponse "native share stop" $response
    }

    if ($selectedSuites -contains "Cancellation") {
        $script:runStage = "cancellation-suite"
        $cancelShareStart = Invoke-LbbCommand "computer.share.start" @{ windowId = $targetWindowId; fps = 4 }
        $shareStarted = $true
        Save-StepResponse "cancellation native share start" $cancelShareStart
        $cancelFrame = Wait-Condition {
            $candidate = Get-CurrentObservation
            if ($candidate.share.active -eq $true -and [int64]$candidate.share.sequence -gt 0) { return $candidate }
            return $false
        } "a native frame for explicit cancellation"
        Assert-True ($cancelFrame.share.nativeStream -eq $true -and $cancelFrame.share.captureScope -eq "exact-window") "Explicit cancellation did not start from a native exact-window share frame."
        $cancelShot = Save-ObservationScreenshot $cancelFrame "50-explicit-cancel-live-frame"
        Save-StepRecord "explicit cancellation live frame screenshot" $cancelShot

        $canceledFrameId = [string]$cancelFrame.frameId
        $canceledScreenshotUrl = [string]$cancelFrame.screenshotUrl
        Assert-True ($canceledScreenshotUrl.StartsWith("/api/computer/screenshot?id=", [StringComparison]::Ordinal) -and -not $canceledScreenshotUrl.Contains("://")) "The cancellation frame returned an invalid screenshot URL."
        $cancellationSessionId = [string](Get-LbbState).computer.sessionId
        Assert-True (-not [String]::IsNullOrWhiteSpace($cancellationSessionId)) "The cancellation frame had no exact helper session identity."
        $cancellationSupervisorPid = $helperProcess.Id
        $cancellationServerPid = $serverProcess.Id
        $cancellationWorker = Wait-ForDirectHelperWorker $helperProcess $cancellationSessionId "the pre-cancellation disposable helper worker"
        $cancellationWorkerPid = [int]$cancellationWorker.processId
        $cancellationReadiness = Invoke-LbbCommand "computer.status" @{}
        Complete-HelperTopologyRoundTrip "the pre-cancellation disposable helper worker" $cancellationSessionId $cancellationWorkerPid "computer.status" $cancellationReadiness
        Save-StepResponse "pre-cancellation protocol-bound helper readiness" $cancellationReadiness

        $cancelPoint = Get-SurfacePoint $cancelFrame 0.25 0.33
        $cancelParams = @{
            frameId = $canceledFrameId
            x = $cancelPoint.x
            y = $cancelPoint.y
            coordinateSpace = "image"
            durationMs = 2000
        }
        $cancelCallId = "windows-cancel-" + [Guid]::NewGuid().ToString("N")
        $cancelFixtureBefore = Get-FixtureState
        $cancelProbeBefore = Capture-InvariantProbe
        $cancelWatch = [Diagnostics.Stopwatch]::StartNew()
        $pendingCancellationRequest = Start-LbbCommandRequest "computer.move" $cancelParams $cancelCallId

        $cancelDispatchState = Wait-ForFixtureProof {
            param($state)
            return [int]$state.messageCounters.mouseMove -gt [int]$cancelFixtureBefore.messageCounters.mouseMove
        } "target-routed WM_MOUSEMOVE before explicit cancellation"
        $cancelDispatchElapsedMs = [int64]$cancelWatch.ElapsedMilliseconds
        Assert-True ([int]$cancelDispatchState.messageCounters.mouseMove -gt [int]$cancelFixtureBefore.messageCounters.mouseMove) "The cancellation test did not causally observe native move delivery."
        Assert-True ([int]$cancelDispatchState.messageCounters.mouseDown -eq [int]$cancelFixtureBefore.messageCounters.mouseDown -and [int]$cancelDispatchState.messageCounters.mouseUp -eq [int]$cancelFixtureBefore.messageCounters.mouseUp) "The cancellation move unexpectedly delivered a click."

        $inProgressDuplicate = Invoke-LbbCommandResponse "computer.move" $cancelParams $cancelCallId
        Assert-True ($inProgressDuplicate.status -eq 409 -and $inProgressDuplicate.httpOk -eq $false -and $inProgressDuplicate.body.error.code -eq "CALL_IN_PROGRESS" -and $inProgressDuplicate.body.callId -eq $cancelCallId) "An exact duplicate did not observe CALL_IN_PROGRESS after causal native dispatch."
        Save-StepResponse "explicit cancellation in-progress duplicate" $inProgressDuplicate.body

        $cancelAccepted = Invoke-LbbCancelResponse $cancelCallId
        Assert-True ($cancelAccepted.status -eq 202 -and $cancelAccepted.httpOk -eq $true -and $cancelAccepted.body.ok -eq $true -and $cancelAccepted.body.callId -eq $cancelCallId -and $cancelAccepted.body.cancellationRequested -eq $true) "The authenticated exact-call cancellation request was not accepted."
        Save-StepResponse "explicit cancellation accepted" $cancelAccepted.body

        $canceledOriginal = Receive-LbbJsonResponse $pendingCancellationRequest
        $pendingCancellationRequest = $null
        $cancelWatch.Stop()
        $cancellationElapsedMs = [int64]$cancelWatch.ElapsedMilliseconds
        Assert-True ($canceledOriginal.status -eq 504 -and $canceledOriginal.httpOk -eq $false -and $canceledOriginal.body.error.code -eq "COMMAND_OUTCOME_UNKNOWN" -and $canceledOriginal.body.taxonomy.code -eq "outcome_unknown" -and $canceledOriginal.body.taxonomy.retriable -eq $false -and $canceledOriginal.body.taxonomy.recoveryHint -eq "reobserve" -and $canceledOriginal.body.callId -eq $cancelCallId) "The canceled original did not settle as conservative COMMAND_OUTCOME_UNKNOWN."
        Save-StepResponse "explicit cancellation original outcome" $canceledOriginal.body

        $duplicateCancel = Invoke-LbbCancelResponse $cancelCallId
        Assert-True ($duplicateCancel.status -eq 409 -and $duplicateCancel.httpOk -eq $false -and $duplicateCancel.body.error.code -eq "CALL_NOT_IN_PROGRESS" -and $duplicateCancel.body.callId -eq $cancelCallId) "A completed canceled call was cancellable twice."
        Save-StepResponse "explicit cancellation duplicate refused" $duplicateCancel.body

        $replayedCanceled = Invoke-LbbCommandResponse "computer.move" $cancelParams $cancelCallId
        Assert-True ($replayedCanceled.status -eq 504 -and $replayedCanceled.httpOk -eq $false -and $replayedCanceled.body.error.code -eq "COMMAND_OUTCOME_UNKNOWN" -and $replayedCanceled.body.callId -eq $cancelCallId -and $replayedCanceled.body.replayed -eq $true) "The exact canceled call did not replay its cached outcome without redispatch."
        $originalComparable = $canceledOriginal.body | ConvertTo-Json -Depth 40 -Compress
        $replayComparableObject = ($replayedCanceled.body | ConvertTo-Json -Depth 40 -Compress) | ConvertFrom-Json
        $replayComparableObject.PSObject.Properties.Remove("replayed")
        $replayComparable = $replayComparableObject | ConvertTo-Json -Depth 40 -Compress
        Assert-True ($replayComparable -ceq $originalComparable) "The replayed cancellation body differed from the original after removing only the replay marker."
        Save-StepResponse "explicit cancellation cached replay" $replayedCanceled.body

        $changedCancelParams = @{}
        foreach ($entry in $cancelParams.GetEnumerator()) {
            $changedCancelParams[$entry.Key] = $entry.Value
        }
        $changedCancelParams.x = [double]$changedCancelParams.x + 1
        $reusedCallId = Invoke-LbbCommandResponse "computer.move" $changedCancelParams $cancelCallId
        Assert-True ($reusedCallId.status -eq 409 -and $reusedCallId.httpOk -eq $false -and $reusedCallId.body.error.code -eq "CALL_ID_REUSED" -and $reusedCallId.body.taxonomy.code -eq "invalid_request" -and $reusedCallId.body.callId -eq $cancelCallId) "Changed parameters reused a canceled call identity."
        Save-StepResponse "explicit cancellation changed request refused" $reusedCallId.body

        $clearedScreenshot = Invoke-LbbJsonGet $canceledScreenshotUrl
        Assert-True ($clearedScreenshot.status -eq 404 -and $clearedScreenshot.httpOk -eq $false -and $clearedScreenshot.body.error.code -eq "NO_COMPUTER_SCREENSHOT") "Cancellation did not remove the exact screenshot surface."
        Save-StepResponse "explicit cancellation screenshot removed" $clearedScreenshot.body

        $replacementState = Wait-Condition {
            $candidate = Get-LbbState
            $candidateSessionId = [string](Get-PropertyValue (Get-PropertyValue $candidate "computer") "sessionId")
            if ($candidate.computerConnected -eq $true -and -not [String]::IsNullOrWhiteSpace($candidateSessionId) -and $candidateSessionId -ne $cancellationSessionId -and $null -eq $candidate.computerObservation -and $candidate.computer.share.active -eq $false) {
                return $candidate
            }
            return $false
        } "a replacement Windows helper worker after outcome-unknown cancellation"
        $replacementSessionId = [string]$replacementState.computer.sessionId
        $cancellationReplacementWorker = Wait-ForDirectHelperWorker $helperProcess $replacementSessionId "the post-cancellation replacement helper worker"
        $cancellationReplacementWorkerPid = [int]$cancellationReplacementWorker.processId
        $replacementState = $cancellationReplacementWorker.state
        $cancellationReplacementReadiness = Invoke-LbbCommand "computer.status" @{}
        Complete-HelperTopologyRoundTrip "the post-cancellation replacement helper worker" $replacementSessionId $cancellationReplacementWorkerPid "computer.status" $cancellationReplacementReadiness
        Save-StepResponse "post-cancellation protocol-bound helper readiness" $cancellationReplacementReadiness
        Assert-True ($cancellationReplacementWorkerPid -ne $cancellationWorkerPid) "Outcome-unknown cancellation did not replace the disposable Windows worker."
        Assert-True ($helperProcess.Id -eq $cancellationSupervisorPid -and -not $helperProcess.HasExited) "Outcome-unknown cancellation replaced the helper supervisor instead of only its disposable worker."
        Assert-True ($serverProcess.Id -eq $cancellationServerPid -and -not $serverProcess.HasExited) "Outcome-unknown cancellation restarted the loopback server."
        $shareStarted = $false
        Start-Sleep -Milliseconds 900
        $replacementStateSettled = Get-LbbState
        Assert-True ($replacementStateSettled.computer.sessionId -eq $replacementSessionId -and $null -eq $replacementStateSettled.computerObservation -and $replacementStateSettled.computer.share.active -eq $false) "An old worker or queued native frame replaced the ready helper or republished old-session authority after cancellation."

        $idempotentStop = Invoke-LbbCommand "computer.share.stop" @{}
        Assert-True ($idempotentStop.result.active -eq $false -and $idempotentStop.result.stopped -eq $false -and $idempotentStop.result.reason -eq "not-active") "Post-cancellation share stop was not idempotently fail-closed."
        Save-StepResponse "explicit cancellation idempotent share stop" $idempotentStop

        $oldFrameClick = @{
            frameId = $canceledFrameId
            x = $cancelPoint.x
            y = $cancelPoint.y
            coordinateSpace = "image"
            button = "left"
            clickCount = 1
            durationMs = 50
        }
        $replacementNoFrameParams = @{
            frameId = $canceledFrameId
            x = 250
            y = 330
            coordinateSpace = "normalized1000"
            button = "left"
            clickCount = 1
            durationMs = 50
        }
        $replacementNoFrame = Invoke-LbbCommandResponse "computer.click" $replacementNoFrameParams ("windows-no-frame-" + [Guid]::NewGuid().ToString("N"))
        Assert-True ($replacementNoFrame.status -eq 409 -and $replacementNoFrame.httpOk -eq $false -and $replacementNoFrame.body.error.code -eq "NO_COMPUTER_FRAME" -and $replacementNoFrame.body.taxonomy.code -eq "stale_snapshot" -and $replacementNoFrame.body.taxonomy.recoveryHint -eq "reobserve") "The replacement helper accepted normalized coordinates before an explicit observation supplied frame dimensions."
        $replacementNoFrameState = Get-LbbState
        Assert-True ($replacementNoFrameState.computer.sessionId -eq $replacementSessionId -and $null -eq $replacementNoFrameState.computerObservation -and $replacementNoFrameState.computer.share.active -eq $false) "The replacement-session no-frame refusal recreated a computer surface or changed helper sessions."
        Save-StepResponse "explicit cancellation replacement has no frame before observe" $replacementNoFrame.body

        $recoveryObserve = Invoke-LbbCommand "computer.observe" @{ windowId = $targetWindowId }
        $recoveryFrame = $recoveryObserve.state.computerObservation
        Assert-True ($recoveryObserve.state.computer.sessionId -eq $replacementSessionId -and $recoveryFrame.frameId -ne $canceledFrameId -and $recoveryFrame.windowId -eq $targetWindowId -and $recoveryFrame.share.active -eq $false) "Explicit one-shot observation did not recover the replacement helper session with a fresh frame."
        Save-StepResponse "explicit cancellation fresh recovery observe" $recoveryObserve

        $staleAfterRecovery = Invoke-LbbCommandResponse "computer.click" $oldFrameClick ("windows-stale-" + [Guid]::NewGuid().ToString("N"))
        Assert-True ($staleAfterRecovery.status -eq 409 -and $staleAfterRecovery.httpOk -eq $false -and $staleAfterRecovery.body.error.code -eq "COMPUTER_STALE_FRAME" -and $staleAfterRecovery.body.taxonomy.code -eq "stale_snapshot" -and $staleAfterRecovery.body.taxonomy.recoveryHint -eq "reobserve") "The pre-cancellation frame became usable after explicit recovery."
        $recoveredState = Get-LbbState
        Assert-True ($recoveredState.computerObservation.frameId -eq $recoveryFrame.frameId -and $recoveredState.computerObservation.windowId -eq $targetWindowId -and $recoveredState.computer.share.active -eq $false) "The rejected stale action replaced or revoked the recovered one-shot frame."
        Save-StepResponse "explicit cancellation stale frame after recovery" $staleAfterRecovery.body

        $recoveryMoveBefore = (Get-FixtureState).messageCounters
        $recoveryPoint = Get-SurfacePoint $recoveryFrame 0.60 0.55
        $recoveryMove = Invoke-LbbCommand "computer.move" @{
            frameId = [string]$recoveryFrame.frameId
            x = $recoveryPoint.x
            y = $recoveryPoint.y
            coordinateSpace = "image"
            durationMs = 120
        }
        $null = Wait-ForFixtureProof { param($state) return [int]$state.messageCounters.mouseMove -gt [int]$recoveryMoveBefore.mouseMove } "a fresh post-cancellation WM_MOUSEMOVE"
        $observation = $recoveryMove.state.computerObservation
        $recoveryShot = Save-ObservationScreenshot $observation "51-explicit-cancel-recovered-action"
        Save-StepResponse "explicit cancellation recovered action" $recoveryMove
        Save-StepRecord "explicit cancellation recovered action screenshot" $recoveryShot

        $cancelFixtureAfter = Get-FixtureState
        Assert-True ([int]$cancelFixtureAfter.messageCounters.mouseDown -eq [int]$cancelFixtureBefore.messageCounters.mouseDown -and [int]$cancelFixtureAfter.messageCounters.mouseUp -eq [int]$cancelFixtureBefore.messageCounters.mouseUp -and [int]$cancelFixtureAfter.messageCounters.dragMove -eq [int]$cancelFixtureBefore.messageCounters.dragMove) "Cancellation, replay, gating, or stale refusal caused an unexpected functional pointer mutation."
        $cancelFinalProbe = Assert-InvariantsHeld $cancelProbeBefore "Explicit cancellation and recovery"
        Save-StepRecord "explicit cancellation authority and recovery proof" ([ordered]@{
            callId = $cancelCallId
            method = "computer.move"
            durationMs = 2000
            elapsedMs = $cancellationElapsedMs
            dispatchProof = [ordered]@{
                type = "fixture-owned WM_MOUSEMOVE counter"
                elapsedMs = $cancelDispatchElapsedMs
                before = $cancelFixtureBefore.messageCounters.mouseMove
                observed = $cancelDispatchState.messageCounters.mouseMove
            }
            inProgressCode = $inProgressDuplicate.body.error.code
            cancellationHttpStatus = $cancelAccepted.status
            originalHttpStatus = $canceledOriginal.status
            originalCode = $canceledOriginal.body.error.code
            exactReplay = $replayComparable -ceq $originalComparable
            changedRequestCode = $reusedCallId.body.error.code
            oldSessionReplaced = $replacementSessionId -ne $cancellationSessionId
            replacementSessionObserved = $recoveryObserve.state.computer.sessionId -eq $replacementSessionId
            disposableWorkerReplaced = $cancellationReplacementWorkerPid -ne $cancellationWorkerPid
            helperSupervisorPreserved = $helperProcess.Id -eq $cancellationSupervisorPid
            loopbackServerPreserved = $serverProcess.Id -eq $cancellationServerPid
            observationCleared = $replacementState.computerObservation -eq $null
            screenshotHttpStatus = $clearedScreenshot.status
            stayedTornDownAfterThreeFramePeriods = $replacementStateSettled.computerObservation -eq $null
            replacementNoFrameCode = $replacementNoFrame.body.error.code
            recoveryMethod = "computer.observe"
            recoveredFreshFrame = $recoveryFrame.frameId -ne $canceledFrameId
            staleAfterRecoveryCode = $staleAfterRecovery.body.error.code
            recoveredActionDelivered = [int]$cancelFixtureAfter.messageCounters.mouseMove -gt [int]$recoveryMoveBefore.mouseMove
            independentNonInterruptionSample = $cancelFinalProbe
        })
    }

    $script:runStage = "final-invariants"
    $finalProbe = Assert-InvariantsHeld $baselineProbe "Full acceptance run"
    Save-StepRecord "foreground cursor focus desktop invariants" ([ordered]@{
        before = $baselineProbe
        after = $finalProbe
    })
    $script:runStage = "completed"
    $runPassed = $true
}
catch {
    $failureText = ConvertTo-SafeFailureText $_.Exception.Message
    $failureDetails = [ordered]@{
        stage = $script:runStage
        exceptionType = $_.Exception.GetType().FullName
        fullyQualifiedErrorId = ConvertTo-SafeFailureText ([string]$_.FullyQualifiedErrorId)
        category = [string]$_.CategoryInfo.Category
        scriptLineNumber = [int]$_.InvocationInfo.ScriptLineNumber
        offsetInLine = [int]$_.InvocationInfo.OffsetInLine
        line = ConvertTo-SafeFailureText ([string]$_.InvocationInfo.Line).Trim()
        scriptStackTrace = ConvertTo-SafeFailureText ([string]$_.ScriptStackTrace)
        pathsRecorded = $false
    }
}
finally {
    if ($null -ne $pendingCancellationRequest) {
        try {
            Close-LbbPendingRequest $pendingCancellationRequest
            $pendingCancellationRequest = $null
        }
        catch {
            $cleanupIssues.Add("The runner-owned pending cancellation request did not close cleanly.")
        }
    }
    if ($shareStarted -and $null -ne $serverProcess -and -not $serverProcess.HasExited -and $null -ne $helperProcess -and -not $helperProcess.HasExited) {
        try {
            $null = Invoke-LbbCommand "computer.share.stop" @{}
        }
        catch {
            $cleanupIssues.Add("The active share did not acknowledge stop before process cleanup.")
        }
    }
    if ($null -ne $fixtureProcess) {
        try {
            $null = Request-FixtureStop $fixtureProcess
        }
        catch {
            $cleanupIssues.Add("The runner-owned fixture did not complete graceful shutdown before Job cleanup.")
        }
    }
    if ($null -ne $script:ownedJob) {
        try {
            $script:ownedJob.Terminate()
            $jobDeadline = [DateTime]::UtcNow.AddSeconds(5)
            do {
                $ownedActive = [uint32]$script:ownedJob.ActiveProcessCount
                if ($ownedActive -eq 0) {
                    break
                }
                Start-Sleep -Milliseconds 100
            } while ([DateTime]::UtcNow -lt $jobDeadline)
            if ($ownedActive -ne 0) {
                $cleanupIssues.Add("The private Job Object still reported runner-owned descendants after termination.")
            }
        }
        catch {
            $cleanupIssues.Add("Private Job Object termination or descendant verification failed; closing the kill-on-close handle was still attempted.")
        }
        finally {
            # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE is the final fail-safe even if
            # explicit TerminateJobObject or accounting verification failed.
            $script:ownedJob.Dispose()
            $script:ownedJob = $null
        }
    }
    if ($selectedSuites -contains "Recovery") {
        $recoveryEventReleased = $script:nativeProbeType::GetKernelEventState($recoveryEventName) -eq 0
        if (-not $recoveryEventReleased) {
            $cleanupIssues.Add("The supervisor-owned one-shot recovery event remained after Job cleanup.")
        }
    }
    try {
        foreach ($evidenceFile in [IO.Directory]::EnumerateFiles($evidenceRoot, "*", [IO.SearchOption]::AllDirectories)) {
            if (Test-FileContainsSecret $evidenceFile $Token) {
                [IO.File]::Delete($evidenceFile)
                $tokenBearingEvidenceRemoved++
            }
        }
        $tokenPersistenceVerified = $true
        if ($tokenBearingEvidenceRemoved -gt 0) {
            $cleanupIssues.Add("Bearer-token evidence was removed; the run is invalid even though the retained bundle is sanitized.")
        }
    }
    catch {
        $cleanupIssues.Add("Bearer-token evidence scanning or removal failed.")
    }
    Start-Sleep -Milliseconds 250
    if (-not (Test-PortBindable $Port)) {
        $cleanupIssues.Add("The runner-owned loopback port was not bindable after cleanup; no unrelated listener was terminated.")
    }
    if ($cleanupIssues.Count -gt 0) {
        $runPassed = $false
        if ([String]::IsNullOrWhiteSpace($failureText)) {
            $failureText = "Cleanup verification failed."
        }
    }
    $summary = [ordered]@{
        schemaVersion = 2
        passed = $runPassed
        suites = $selectedSuites
        startedAtUtc = $startedAt.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        finishedAtUtc = [DateTime]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        interactiveSessionId = $sessionId
        loopbackPort = $Port
        targetPid = $targetPid
        targetWindowId = $targetWindowId
        steps = $script:stepResults
        failure = $failureText
        failureDetails = $failureDetails
        cleanupIssues = @($cleanupIssues)
        childHandleInheritancePolicy = "PROC_THREAD_ATTRIBUTE_HANDLE_LIST:NUL-only"
        allowedInheritedHandleCount = 1
        tokenPersistenceVerified = $tokenPersistenceVerified
        tokenPersisted = if ($tokenPersistenceVerified) { $false } else { $null }
        tokenBearingEvidenceRemoved = $tokenBearingEvidenceRemoved
        unrelatedProcessesTerminated = $false
        recoveryEventReleased = $recoveryEventReleased
        helperTopologyChecks = @($script:helperTopologyChecks)
        helperTopologyPollCount = $script:helperTopologyPollCount
        helperTopologyTransitionCount = $script:helperTopologyTransitionCount
        helperTopologyHistory = @($script:helperTopologyHistory)
        helperTopologyLastObservation = $script:helperTopologyDiagnostic
        foregroundArmProof = $script:foregroundArmProof
        releaseCandidateBinding = $releaseCandidateBinding
        candidateBinding = $candidateBinding
    }
    Write-EvidenceJson ([IO.Path]::Combine($evidenceRoot, "summary.json")) $summary
    Remove-Variable Token -ErrorAction SilentlyContinue
}

if (-not $runPassed) {
    Write-Error "Windows computer-use acceptance failed. Review the sanitized summary and step evidence."
    exit 1
}

Write-Output "Windows computer-use acceptance passed. Evidence was written to the caller-selected directory."
