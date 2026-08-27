[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallRoot = "",
    [switch]$NoStartup,
    [switch]$StartHelper,
    [switch]$NoLaunch,
    [switch]$Uninstall,
    [switch]$ResetToken,
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0
$script:Repository = "flrngel/local-browser-bridge"
$script:StartupName = "Local Browser Bridge.cmd"

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $hasher = [Security.Cryptography.SHA256]::Create()
    try {
        return (($hasher.ComputeHash($stream) | ForEach-Object { $_.ToString("x2") }) -join "")
    }
    finally {
        $hasher.Dispose()
        $stream.Dispose()
    }
}

function Assert-OrdinaryDirectory([string]$Path, [switch]$AllowMissing) {
    if (-not [IO.Directory]::Exists($Path)) {
        if ($AllowMissing) { return }
        throw "Required directory does not exist: $Path"
    }
    $attributes = [IO.File]::GetAttributes($Path)
    if (($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Refusing a reparse-point install path: $Path"
    }
}

function Assert-SafeInstallRoot([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $root = [IO.Path]::GetPathRoot($full).TrimEnd('\')
    if ($full -ieq $root) { throw "Refusing a filesystem-root install path." }
    foreach ($blocked in @($env:USERPROFILE, $env:LOCALAPPDATA, $env:ProgramFiles, ${env:ProgramFiles(x86)})) {
        if ($blocked -and $full -ieq [IO.Path]::GetFullPath($blocked).TrimEnd('\')) {
            throw "Refusing a broad install path."
        }
    }
}

function Get-Release([string]$RequestedVersion) {
    $uri = if ($RequestedVersion -ceq "latest") {
        "https://api.github.com/repos/$($script:Repository)/releases/latest"
    }
    else {
        if ($RequestedVersion -notmatch '^v?[0-9]+\.[0-9]+\.[0-9]+$') {
            throw "Version must be 'latest' or a stable semantic version."
        }
        "https://api.github.com/repos/$($script:Repository)/releases/tags/v$($RequestedVersion.TrimStart('v'))"
    }
    $headers = @{ Accept = "application/vnd.github+json"; "User-Agent" = "local-browser-bridge-installer" }
    $release = Invoke-RestMethod -UseBasicParsing -Headers $headers -Uri $uri
    if ($release.draft -or $release.prerelease -or -not $release.immutable) {
        throw "GitHub did not return an immutable stable release."
    }
    if ([string]$release.tag_name -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') {
        throw "The release tag is not canonical."
    }
    $resolved = ([string]$release.tag_name).Substring(1)
    $expected = @(
        "local-browser-bridge-extension-v$resolved.zip",
        "local-browser-bridge-v$resolved-macos-universal.tar.gz",
        "local-browser-bridge-v$resolved-windows-x86_64.exe",
        "local-computer-helper-v$resolved-windows-x86_64.exe",
        "SHA256SUMS.txt"
    )
    $assets = @($release.assets)
    if ($assets.Count -ne $expected.Count) { throw "Unexpected release asset inventory." }
    foreach ($name in $expected) {
        $matches = @($assets | Where-Object { $_.name -ceq $name })
        if ($matches.Count -ne 1 -or [int64]$matches[0].size -le 0 -or
            [string]$matches[0].state -cne "uploaded" -or
            [string]$matches[0].digest -notmatch '^sha256:[0-9a-f]{64}$') {
            throw "Missing or unverifiable release asset: $name"
        }
    }
    return @{ Version = $resolved; Assets = $assets }
}

function Get-ManifestMap([string]$Path) {
    $result = @{}
    $lines = @([IO.File]::ReadAllLines($Path, [Text.Encoding]::UTF8))
    if ($lines.Count -ne 4) { throw "The checksum manifest must contain exactly four entries." }
    foreach ($line in $lines) {
        if ($line -notmatch '^([0-9a-f]{64})  ([A-Za-z0-9._-]+)$') {
            throw "The checksum manifest is not canonical."
        }
        if ($result.ContainsKey($matches[2])) { throw "The checksum manifest contains a duplicate filename." }
        $result[$matches[2]] = $matches[1]
    }
    return $result
}

function Stop-InstalledProcesses([string]$Root) {
    $prefix = [IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    foreach ($process in @(Get-Process -ErrorAction SilentlyContinue)) {
        try {
            $path = $process.Path
            if ($path -and [IO.Path]::GetFullPath($path).StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
                Stop-Process -Id $process.Id -Force
            }
        }
        catch { }
    }
}

function Open-ExtensionsPage {
    $candidates = @(
        (Join-Path ${env:ProgramFiles} "Google\Chrome\Application\chrome.exe"),
        (Join-Path ${env:ProgramFiles(x86)} "Google\Chrome\Application\chrome.exe"),
        (Join-Path $env:LOCALAPPDATA "Google\Chrome\Application\chrome.exe"),
        (Join-Path ${env:ProgramFiles(x86)} "Microsoft\Edge\Application\msedge.exe"),
        (Join-Path ${env:ProgramFiles} "Microsoft\Edge\Application\msedge.exe")
    )
    foreach ($candidate in $candidates) {
        if ($candidate -and [IO.File]::Exists($candidate)) {
            $url = if ([IO.Path]::GetFileName($candidate) -ieq "msedge.exe") { "edge://extensions" } else { "chrome://extensions" }
            Start-Process -FilePath $candidate -ArgumentList $url
            return
        }
    }
}

function Remove-Install {
    $startup = Join-Path ([Environment]::GetFolderPath("Startup")) $script:StartupName
    if ([IO.File]::Exists($startup)) { [IO.File]::Delete($startup) }
    if ([IO.Directory]::Exists($InstallRoot)) {
        Assert-OrdinaryDirectory $InstallRoot
        Stop-InstalledProcesses $InstallRoot
        Start-Sleep -Milliseconds 250
        [IO.Directory]::Delete($InstallRoot, $true)
    }
    if ($ResetToken) {
        $token = Join-Path $env:USERPROFILE ".local-browser-bridge\token"
        if ([IO.File]::Exists($token)) { [IO.File]::Delete($token) }
    }
    Write-Output "Local Browser Bridge was removed for the current user."
}

function Invoke-SelfTest {
    $scratch = Join-Path ([IO.Path]::GetTempPath()) ("lbb-installer-self-test-" + [Guid]::NewGuid().ToString("N"))
    [IO.Directory]::CreateDirectory($scratch) | Out-Null
    try {
        $manifest = Join-Path $scratch "SHA256SUMS.txt"
        [IO.File]::WriteAllText($manifest, ((1..4 | ForEach-Object { ("a" * 64) + "  file$_.bin" }) -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
        $map = Get-ManifestMap $manifest
        if ($map.Count -ne 4 -or $map["file4.bin"] -cne ("a" * 64)) { throw "Manifest parser self-test failed." }
        $bad = Join-Path $scratch "bad.txt"
        [IO.File]::WriteAllText($bad, ("a" * 64) + " file.bin`n", [Text.UTF8Encoding]::new($false))
        $rejected = $false
        try { $null = Get-ManifestMap $bad } catch { $rejected = $true }
        if (-not $rejected) { throw "Malformed manifest self-test failed." }
        Assert-OrdinaryDirectory $scratch
        Write-Output "Windows one-command installer self-test passed."
    }
    finally {
        if ([IO.Directory]::Exists($scratch)) { [IO.Directory]::Delete($scratch, $true) }
    }
}

if ($SelfTest) { Invoke-SelfTest; exit 0 }
if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { throw "LOCALAPPDATA is unavailable." }
    $InstallRoot = Join-Path $env:LOCALAPPDATA "Programs\Local Browser Bridge"
}
Assert-SafeInstallRoot $InstallRoot
if ($Uninstall) { Remove-Install; exit 0 }
if (-not [Environment]::Is64BitOperatingSystem) { throw "64-bit Windows is required." }

$release = Get-Release $Version
$resolved = $release.Version
$serverName = "local-browser-bridge-v$resolved-windows-x86_64.exe"
$helperName = "local-computer-helper-v$resolved-windows-x86_64.exe"
$extensionName = "local-browser-bridge-extension-v$resolved.zip"
$downloadNames = @($serverName, $helperName, $extensionName, "SHA256SUMS.txt")
$stage = Join-Path ([IO.Path]::GetTempPath()) ("lbb-install-" + [Guid]::NewGuid().ToString("N"))
[IO.Directory]::CreateDirectory($stage) | Out-Null

try {
    foreach ($name in $downloadNames) {
        $asset = @($release.Assets | Where-Object { $_.name -ceq $name })[0]
        $destination = Join-Path $stage $name
        Invoke-WebRequest -UseBasicParsing -Headers @{ "User-Agent" = "local-browser-bridge-installer" } -Uri $asset.browser_download_url -OutFile $destination
        $actual = Get-Sha256 $destination
        if (("sha256:" + $actual) -cne [string]$asset.digest) { throw "GitHub digest mismatch for $name" }
    }
    $manifest = Get-ManifestMap (Join-Path $stage "SHA256SUMS.txt")
    foreach ($name in @($serverName, $helperName, $extensionName)) {
        if (-not $manifest.ContainsKey($name) -or $manifest[$name] -cne (Get-Sha256 (Join-Path $stage $name))) {
            throw "Manifest digest mismatch for $name"
        }
    }

    $extensionStage = Join-Path $stage "extension"
    Expand-Archive -LiteralPath (Join-Path $stage $extensionName) -DestinationPath $extensionStage
    if (-not [IO.File]::Exists((Join-Path $extensionStage "manifest.json"))) {
        throw "The extension archive does not contain manifest.json at its root."
    }

    $installParent = Split-Path -Parent $InstallRoot
    if (-not [IO.Directory]::Exists($installParent)) { [IO.Directory]::CreateDirectory($installParent) | Out-Null }
    Assert-OrdinaryDirectory $installParent
    if (-not [IO.Directory]::Exists($InstallRoot)) { [IO.Directory]::CreateDirectory($InstallRoot) | Out-Null }
    Assert-OrdinaryDirectory $InstallRoot
    Stop-InstalledProcesses $InstallRoot
    foreach ($old in @([IO.Directory]::GetFiles($InstallRoot, "*.exe"))) { [IO.File]::Delete($old) }
    [IO.File]::Copy((Join-Path $stage $serverName), (Join-Path $InstallRoot $serverName), $true)
    [IO.File]::Copy((Join-Path $stage $helperName), (Join-Path $InstallRoot $helperName), $true)
    [IO.File]::Copy((Join-Path $stage "SHA256SUMS.txt"), (Join-Path $InstallRoot "SHA256SUMS.txt"), $true)
    $extensionRoot = Join-Path $InstallRoot "extension"
    if ([IO.Directory]::Exists($extensionRoot)) { Assert-OrdinaryDirectory $extensionRoot; [IO.Directory]::Delete($extensionRoot, $true) }
    [IO.Directory]::Move($extensionStage, $extensionRoot)

    $serverPath = Join-Path $InstallRoot $serverName
    $helperPath = Join-Path $InstallRoot $helperName
    $launcher = Join-Path $InstallRoot "Start Local Browser Bridge.cmd"
    [IO.File]::WriteAllText($launcher, "@echo off`r`nstart `"Local Browser Bridge`" /min `"$serverPath`"`r`n", [Text.ASCIIEncoding]::new())
    if (-not $NoStartup) {
        $startup = Join-Path ([Environment]::GetFolderPath("Startup")) $script:StartupName
        [IO.File]::Copy($launcher, $startup, $true)
    }
    else {
        $startup = Join-Path ([Environment]::GetFolderPath("Startup")) $script:StartupName
        if ([IO.File]::Exists($startup)) { [IO.File]::Delete($startup) }
    }

    if (-not $NoLaunch) {
        Start-Process -FilePath $serverPath -WindowStyle Minimized
        if ($StartHelper) { Start-Process -FilePath $helperPath -WindowStyle Minimized }
        $tokenPath = Join-Path $env:USERPROFILE ".local-browser-bridge\token"
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while (-not [IO.File]::Exists($tokenPath) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        if ([IO.File]::Exists($tokenPath)) {
            $token = [IO.File]::ReadAllText($tokenPath).Trim()
            if ($token -match '^[A-Za-z0-9_-]{32,}$') { Start-Process "http://127.0.0.1:17373/#token=$token" }
        }
        Open-ExtensionsPage
    }

    Write-Output "Installed Local Browser Bridge $resolved for the current user."
    Write-Output "Extension folder: $extensionRoot"
    Write-Output "In chrome://extensions, enable Developer mode, choose Load unpacked, and select that folder."
    if (-not $StartHelper) { Write-Output "Desktop control remains off. Run '$helperPath' only when you want it." }
}
finally {
    if ([IO.Directory]::Exists($stage)) { [IO.Directory]::Delete($stage, $true) }
}
