[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallRoot = "",
    [switch]$NoStartup,
    [switch]$StartHelper,
    [switch]$EnableShell,
    [switch]$NoDesktopControl,
    [switch]$NoShell,
    [switch]$NoLaunch,
    [switch]$Uninstall,
    [switch]$ResetToken,
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0
$ProgressPreference = "SilentlyContinue"
$script:Repository = "flrngel/local-browser-bridge"
$script:StartupName = "Local Browser Bridge.lnk"
$script:StartMenuName = "Local Browser Bridge"
$script:OwnerMarker = ".lbb-install-owner"
$script:OwnerMarkerValue = "local-browser-bridge-install-v1"

# Desktop control (the computer helper) and shell access are on by default; pass
# -NoDesktopControl / -NoShell to opt out. -StartHelper and -EnableShell remain accepted
# no-op aliases so existing documented commands keep working.
$StartHelper = -not $NoDesktopControl
$EnableShell = -not $NoShell

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
    foreach ($blocked in @($env:USERPROFILE, $env:LOCALAPPDATA, $env:APPDATA, $env:ProgramFiles, ${env:ProgramFiles(x86)}, $env:SystemRoot)) {
        if ($blocked -and $full -ieq [IO.Path]::GetFullPath($blocked).TrimEnd('\')) {
            throw "Refusing a broad install path."
        }
    }
    $cursor = $full
    while (-not [string]::IsNullOrWhiteSpace($cursor)) {
        if ([IO.Directory]::Exists($cursor)) { Assert-OrdinaryDirectory $cursor }
        elseif ([IO.File]::Exists($cursor)) { throw "An install-root ancestor is a file: $cursor" }
        if ($cursor -ieq $root) { break }
        $parent = [IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName.TrimEnd('\')
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

function Assert-OrdinaryFile([string]$Path, [switch]$AllowMissing) {
    if (-not [IO.File]::Exists($Path)) {
        if ($AllowMissing -and -not [IO.Directory]::Exists($Path)) { return }
        throw "Required file does not exist: $Path"
    }
    if (([IO.File]::GetAttributes($Path) -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Refusing a reparse-point product file: $Path"
    }
}

function Assert-OrdinaryTree([string]$Path, [switch]$AllowMissing) {
    if (-not [IO.Directory]::Exists($Path)) {
        if ($AllowMissing -and -not [IO.File]::Exists($Path)) { return }
        throw "Required directory does not exist: $Path"
    }
    Assert-OrdinaryDirectory $Path
    $pending = New-Object 'Collections.Generic.Stack[string]'
    $pending.Push($Path)
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        foreach ($entry in [IO.Directory]::EnumerateFileSystemEntries($directory)) {
            $attributes = [IO.File]::GetAttributes($entry)
            if (($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Refusing a product tree containing a reparse point: $entry"
            }
            if (($attributes -band [IO.FileAttributes]::Directory) -ne 0) { $pending.Push($entry) }
        }
    }
}

function Test-AllowlistedInstallName([string]$Name) {
    if ($Name -in @(
        "extension",
        "SHA256SUMS.txt",
        "Start Local Browser Bridge.cmd",
        "Open Local Browser Bridge.cmd",
        "Finish Browser Extension Setup.cmd",
        "Start Computer Helper.cmd",
        "Uninstall Local Browser Bridge.cmd",
        $script:OwnerMarker
    )) { return $true }
    return $Name -cmatch '^local-(browser-bridge(?:-desktop)?|computer-helper)-v[0-9]+\.[0-9]+\.[0-9]+-windows-x86_64\.exe$'
}

function Test-InstallerOwnedRoot([string]$Root) {
    $marker = Join-Path $Root $script:OwnerMarker
    if ([IO.File]::Exists($marker) -or [IO.Directory]::Exists($marker)) {
        Assert-OrdinaryFile $marker
        return [IO.File]::ReadAllText($marker, [Text.Encoding]::UTF8).TrimEnd([char[]]"`r`n") -ceq $script:OwnerMarkerValue
    }
    $defaultRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA "Programs\Local Browser Bridge")).TrimEnd('\')
    if ([IO.Path]::GetFullPath($Root).TrimEnd('\') -ine $defaultRoot) { return $false }
    $server = @([IO.Directory]::EnumerateFiles($Root, "local-browser-bridge-v*-windows-x86_64.exe", [IO.SearchOption]::TopDirectoryOnly))
    return $server.Count -ge 1 -and [IO.File]::Exists((Join-Path $Root "extension\manifest.json"))
}

function Assert-InstallerOwnedRoot([string]$Root) {
    if (-not [IO.Directory]::Exists($Root) -and -not [IO.File]::Exists($Root)) { return }
    Assert-OrdinaryDirectory $Root
    if (-not (Test-InstallerOwnedRoot $Root)) {
        throw "The directory is not a recognized installer-owned Local Browser Bridge installation: $Root"
    }
}

function Assert-AllowlistedInstallEntries([string]$Root) {
    foreach ($file in @([IO.Directory]::EnumerateFiles($Root, "*", [IO.SearchOption]::TopDirectoryOnly) | Where-Object {
        Test-AllowlistedInstallName ([IO.Path]::GetFileName($_))
    })) {
        Assert-OrdinaryFile $file
    }
    Assert-OrdinaryTree (Join-Path $Root "extension") -AllowMissing
}

function Remove-AllowlistedInstall([string]$Root) {
    if (-not [IO.Directory]::Exists($Root) -and -not [IO.File]::Exists($Root)) { return }
    Assert-InstallerOwnedRoot $Root
    Assert-AllowlistedInstallEntries $Root
    $files = @([IO.Directory]::EnumerateFiles($Root, "*", [IO.SearchOption]::TopDirectoryOnly) | Where-Object {
        Test-AllowlistedInstallName ([IO.Path]::GetFileName($_))
    })
    Stop-InstalledProcesses $Root
    Start-Sleep -Milliseconds 250
    foreach ($file in @($files | Where-Object { [IO.Path]::GetFileName($_) -cne $script:OwnerMarker })) {
        [IO.File]::Delete($file)
    }
    $extension = Join-Path $Root "extension"
    if ([IO.Directory]::Exists($extension)) { [IO.Directory]::Delete($extension, $true) }
    $unknown = @([IO.Directory]::EnumerateFileSystemEntries($Root) | Where-Object {
        -not (Test-AllowlistedInstallName ([IO.Path]::GetFileName($_)))
    })
    if ($unknown.Count -ne 0) {
        Write-Warning "Retained the install directory because it contains files not owned by the installer: $Root"
    }
    else {
        $marker = Join-Path $Root $script:OwnerMarker
        if ([IO.File]::Exists($marker)) { [IO.File]::Delete($marker) }
        try { [IO.Directory]::Delete($Root, $false) } catch { }
    }
}

function Test-OwnedLauncher([string]$Path, [string]$Root) {
    if (-not [IO.File]::Exists($Path)) { return $false }
    Assert-OrdinaryFile $Path
    $content = [IO.File]::ReadAllText($Path)
    return $content.Contains("Local Browser Bridge") -and
        ($content.Contains($Root) -or $content.Contains("flrngel/local-browser-bridge"))
}

function New-OwnedShortcut(
    [string]$Path,
    [string]$TargetPath,
    [string]$Arguments,
    [string]$WorkingDirectory,
    [string]$Description
) {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($Path)
    $shortcut.TargetPath = $TargetPath
    $shortcut.Arguments = $Arguments
    $shortcut.WorkingDirectory = $WorkingDirectory
    $shortcut.Description = $Description
    $shortcut.WindowStyle = 1
    $shortcut.Save()
}

function Test-OwnedShortcut([string]$Path, [string]$Root) {
    if (-not [IO.File]::Exists($Path)) { return $false }
    Assert-OrdinaryFile $Path
    try {
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($Path)
        return ([string]$shortcut.TargetPath).Contains($Root) -or
            ([string]$shortcut.Arguments).Contains($Root) -or
            ([string]$shortcut.WorkingDirectory).Contains($Root) -or
            ([string]$shortcut.Arguments).Contains("flrngel/local-browser-bridge")
    }
    catch { return $false }
}

function Write-SettingsFile([bool]$ShellEnabled, [bool]$DesktopControlEnabled, [bool]$StartAtLogin, [string]$SettingsPath = "") {
    if ([string]::IsNullOrWhiteSpace($SettingsPath)) {
        $settingsDirectory = Join-Path $env:USERPROFILE ".local-browser-bridge"
        $SettingsPath = Join-Path $settingsDirectory "settings.json"
    }
    else {
        $settingsDirectory = [IO.Path]::GetDirectoryName($SettingsPath)
    }
    if (-not [IO.Directory]::Exists($settingsDirectory)) { [IO.Directory]::CreateDirectory($settingsDirectory) | Out-Null }

    $existing = $null
    if ([IO.File]::Exists($SettingsPath)) {
        try {
            $text = [IO.File]::ReadAllText($SettingsPath)
            if (-not [string]::IsNullOrWhiteSpace($text)) { $existing = $text | ConvertFrom-Json -ErrorAction Stop }
        }
        catch {
            # Corrupt or unreadable existing settings: fall through and treat
            # as absent, so the install still succeeds with this run's
            # freshly computed defaults instead of failing.
            $existing = $null
        }
    }

    # Merge, do not regenerate: only the field whose opt-out switch was
    # actually passed this run may change. Every other field - startAtLogin
    # included, plus any field a newer version of this script does not know
    # about - is carried forward unchanged from the existing file. This is
    # what stops a re-install or upgrade from silently re-enabling a
    # capability the user had previously turned off.
    $merged = [ordered]@{
        version               = 1
        shellEnabled          = $ShellEnabled
        desktopControlEnabled = $DesktopControlEnabled
        startAtLogin          = $StartAtLogin
    }
    if ($existing -is [Management.Automation.PSCustomObject]) {
        foreach ($property in $existing.PSObject.Properties) {
            $merged[$property.Name] = $property.Value
        }
        if ($NoShell.IsPresent) { $merged["shellEnabled"] = $ShellEnabled }
        if ($NoDesktopControl.IsPresent) { $merged["desktopControlEnabled"] = $DesktopControlEnabled }
        if ($NoStartup.IsPresent) { $merged["startAtLogin"] = $StartAtLogin }
    }

    $json = (ConvertTo-Json -InputObject $merged -Depth 10 -Compress) + "`n"
    $tempPath = "$SettingsPath.tmp"
    [IO.File]::WriteAllText($tempPath, $json, (New-Object Text.UTF8Encoding($false)))
    if ([IO.File]::Exists($SettingsPath)) { [IO.File]::Delete($SettingsPath) }
    [IO.File]::Move($tempPath, $SettingsPath)
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
            return $true
        }
    }
    return $false
}

function Show-ExtensionSetup([string]$ExtensionRoot, [string]$Token) {
    try { $ExtensionRoot | & (Join-Path $env:SystemRoot "System32\clip.exe") }
    catch { }
    Start-Process -FilePath $ExtensionRoot
    $opened = Open-ExtensionsPage
    $browserStep = if ($opened) {
        "The browser extensions page is open."
    }
    else {
        "Open chrome://extensions or edge://extensions in your browser."
    }
    Write-Output ""
    Write-Output "== Finish Local Browser Bridge Setup =="
    Write-Output "1. $browserStep"
    Write-Output "2. Turn on Developer mode."
    Write-Output "3. Click Load unpacked."
    Write-Output "4. Paste the extension folder path (already copied) and select it:"
    Write-Output "   $ExtensionRoot"
    if ($Token -match '^[A-Za-z0-9_-]{32,}$') {
        try { $Token | & (Join-Path $env:SystemRoot "System32\clip.exe") }
        catch { }
        Write-Output "The bridge token is now copied. Open the Local Browser Bridge extension, paste it, and choose Save and connect."
        Write-Output "You can repeat this guide from Start > Local Browser Bridge > Finish Browser Extension Setup."
    }
    else {
        Write-Output "The server token is not ready yet. Open Local Browser Bridge from the Start menu, then run Finish Browser Extension Setup again."
    }
}

function Remove-Install {
    if ([IO.Directory]::Exists($InstallRoot)) {
        Assert-InstallerOwnedRoot $InstallRoot
        Assert-AllowlistedInstallEntries $InstallRoot
    }
    $startupDirectory = [Environment]::GetFolderPath("Startup")
    Assert-OrdinaryDirectory $startupDirectory
    foreach ($startupName in @($script:StartupName, "Local Browser Bridge.cmd")) {
        $startup = Join-Path $startupDirectory $startupName
        if ([IO.File]::Exists($startup)) {
            $owned = if ($startup.EndsWith(".lnk")) { Test-OwnedShortcut $startup $InstallRoot } else { Test-OwnedLauncher $startup $InstallRoot }
            if ($owned) { [IO.File]::Delete($startup) }
            else { Write-Warning "Retained an unrecognized same-named Startup item: $startup" }
        }
    }
    $startMenu = Join-Path ([Environment]::GetFolderPath("Programs")) $script:StartMenuName
    if ([IO.Directory]::Exists($startMenu)) {
        Assert-OrdinaryDirectory $startMenu
        foreach ($name in @(
            "Local Browser Bridge.lnk",
            "Finish Browser Extension Setup.lnk",
            "Uninstall Local Browser Bridge.lnk",
            "Open Local Browser Bridge.cmd",
            "Finish Browser Extension Setup.cmd",
            "Start Computer Helper.cmd",
            "Uninstall Local Browser Bridge.cmd"
        )) {
            $path = Join-Path $startMenu $name
            if ([IO.File]::Exists($path)) {
                $owned = if ($path.EndsWith(".lnk")) { Test-OwnedShortcut $path $InstallRoot } else { Test-OwnedLauncher $path $InstallRoot }
                if ($owned) { [IO.File]::Delete($path) }
                else { Write-Warning "Retained an unrecognized Start-menu file: $path" }
            }
        }
        if (@([IO.Directory]::EnumerateFileSystemEntries($startMenu)).Count -eq 0) {
            [IO.Directory]::Delete($startMenu, $false)
        }
    }
    Remove-AllowlistedInstall $InstallRoot
    Write-Output "Removed installed program files: $InstallRoot"
    if ($ResetToken) {
        $token = Join-Path $env:USERPROFILE ".local-browser-bridge\token"
        $tokenDirectory = Split-Path -Parent $token
        if ([IO.Directory]::Exists($tokenDirectory)) { Assert-OrdinaryDirectory $tokenDirectory }
        if ([IO.File]::Exists($token)) { Assert-OrdinaryFile $token; [IO.File]::Delete($token) }
        Write-Output "Removed the bridge token."
    }
    $settings = Join-Path $env:USERPROFILE ".local-browser-bridge\settings.json"
    if ([IO.File]::Exists($settings)) {
        Assert-OrdinaryFile $settings
        [IO.File]::Delete($settings)
        Write-Output "Removed settings.json."
    }
    $null = Open-ExtensionsPage
    Write-Output "The unpacked extension files are gone. If a Local Browser Bridge card remains in the extensions page, click Remove once."
    Write-Output "Browser profile files were intentionally left untouched."
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
        Test-SettingsMergeSelfTest $scratch
        Write-Output "Windows one-command installer self-test passed."
    }
    finally {
        if ([IO.Directory]::Exists($scratch)) { [IO.Directory]::Delete($scratch, $true) }
    }
}

function Test-SettingsMergeSelfTest([string]$Scratch) {
    $settingsPath = Join-Path $Scratch "settings.json"
    $savedNoShell = $NoShell
    $savedNoDesktopControl = $NoDesktopControl
    $savedNoStartup = $NoStartup
    try {
        # Fresh install (no existing file): full defaults are written.
        $script:NoShell = $false
        $script:NoDesktopControl = $false
        $script:NoStartup = $false
        Write-SettingsFile -ShellEnabled $true -DesktopControlEnabled $true -StartAtLogin $true -SettingsPath $settingsPath
        $written = ([IO.File]::ReadAllText($settingsPath) | ConvertFrom-Json)
        if (-not $written.shellEnabled -or -not $written.desktopControlEnabled -or -not $written.startAtLogin) {
            throw "Settings self-test failed: fresh install did not write full defaults."
        }

        # Re-install passing only -NoShell must turn shellEnabled off without
        # touching desktopControlEnabled or startAtLogin.
        $script:NoShell = $true
        $script:NoDesktopControl = $false
        $script:NoStartup = $false
        Write-SettingsFile -ShellEnabled $false -DesktopControlEnabled $true -StartAtLogin $true -SettingsPath $settingsPath
        $merged = ([IO.File]::ReadAllText($settingsPath) | ConvertFrom-Json)
        if ($merged.shellEnabled) { throw "Settings self-test failed: -NoShell did not disable shellEnabled." }
        if (-not $merged.desktopControlEnabled) { throw "Settings self-test failed: unrelated -NoShell run clobbered desktopControlEnabled." }
        if (-not $merged.startAtLogin) { throw "Settings self-test failed: unrelated -NoShell run clobbered startAtLogin." }

        # An unknown field from a newer version of this script must survive
        # a merge unchanged.
        $withExtra = '{"version":1,"shellEnabled":false,"desktopControlEnabled":true,"startAtLogin":true,"futureField":"keep-me"}'
        [IO.File]::WriteAllText($settingsPath, $withExtra, (New-Object Text.UTF8Encoding($false)))
        $script:NoDesktopControl = $true
        $script:NoShell = $false
        $script:NoStartup = $false
        Write-SettingsFile -ShellEnabled $true -DesktopControlEnabled $false -StartAtLogin $true -SettingsPath $settingsPath
        $mergedExtra = ([IO.File]::ReadAllText($settingsPath) | ConvertFrom-Json)
        if ($mergedExtra.desktopControlEnabled) { throw "Settings self-test failed: -NoDesktopControl did not disable desktopControlEnabled." }
        if ($mergedExtra.shellEnabled) { throw "Settings self-test failed: unrelated -NoDesktopControl run clobbered shellEnabled." }
        if ($mergedExtra.futureField -ne "keep-me") { throw "Settings self-test failed: unknown field was not preserved through a merge." }

        # A corrupt existing file must fall back to this run's computed
        # defaults instead of failing the install.
        [IO.File]::WriteAllText($settingsPath, "{not valid json", (New-Object Text.UTF8Encoding($false)))
        $script:NoShell = $true
        $script:NoDesktopControl = $false
        $script:NoStartup = $false
        Write-SettingsFile -ShellEnabled $false -DesktopControlEnabled $true -StartAtLogin $true -SettingsPath $settingsPath
        $recovered = ([IO.File]::ReadAllText($settingsPath) | ConvertFrom-Json)
        if ($recovered.shellEnabled -or -not $recovered.desktopControlEnabled -or -not $recovered.startAtLogin) {
            throw "Settings self-test failed: corrupt existing file was not recovered with this run's defaults."
        }
    }
    finally {
        $script:NoShell = $savedNoShell
        $script:NoDesktopControl = $savedNoDesktopControl
        $script:NoStartup = $savedNoStartup
    }
}

if ($SelfTest) { Invoke-SelfTest; exit 0 }
if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { throw "LOCALAPPDATA is unavailable." }
    $InstallRoot = Join-Path $env:LOCALAPPDATA "Programs\Local Browser Bridge"
}
$InstallRoot = [IO.Path]::GetFullPath($InstallRoot).TrimEnd('\')
Assert-SafeInstallRoot $InstallRoot
if ($Uninstall) { Remove-Install; exit 0 }
if (-not [Environment]::Is64BitOperatingSystem) { throw "64-bit Windows is required." }

Write-Output "Resolving release $Version..."
$release = Get-Release $Version
$resolved = $release.Version
Write-Output "Resolved release v$resolved."
$serverName = "local-browser-bridge-v$resolved-windows-x86_64.exe"
$helperName = "local-computer-helper-v$resolved-windows-x86_64.exe"
$extensionName = "local-browser-bridge-extension-v$resolved.zip"
$downloadNames = @($serverName, $helperName, $extensionName, "SHA256SUMS.txt")
$stage = Join-Path ([IO.Path]::GetTempPath()) ("lbb-install-" + [Guid]::NewGuid().ToString("N"))
[IO.Directory]::CreateDirectory($stage) | Out-Null

try {
    foreach ($name in $downloadNames) {
        Write-Output "Downloading $name..."
        $asset = @($release.Assets | Where-Object { $_.name -ceq $name })[0]
        $destination = Join-Path $stage $name
        Invoke-WebRequest -UseBasicParsing -Headers @{ "User-Agent" = "local-browser-bridge-installer" } -Uri $asset.browser_download_url -OutFile $destination
        Write-Output "Checking $name..."
        $actual = Get-Sha256 $destination
        if (("sha256:" + $actual) -cne [string]$asset.digest) { throw "GitHub digest mismatch for $name" }
    }
    Write-Output "Checking the release manifest..."
    $manifest = Get-ManifestMap (Join-Path $stage "SHA256SUMS.txt")
    foreach ($name in @($serverName, $helperName, $extensionName)) {
        if (-not $manifest.ContainsKey($name) -or $manifest[$name] -cne (Get-Sha256 (Join-Path $stage $name))) {
            throw "Manifest digest mismatch for $name"
        }
    }

    Write-Output "Extracting the browser extension..."
    $extensionStage = Join-Path $stage "extension"
    Expand-Archive -LiteralPath (Join-Path $stage $extensionName) -DestinationPath $extensionStage
    if (-not [IO.File]::Exists((Join-Path $extensionStage "manifest.json"))) {
        throw "The extension archive does not contain manifest.json at its root."
    }

    Write-Output "Installing Local Browser Bridge to $InstallRoot..."
    $installParent = Split-Path -Parent $InstallRoot
    if (-not [IO.Directory]::Exists($installParent)) { [IO.Directory]::CreateDirectory($installParent) | Out-Null }
    Assert-OrdinaryDirectory $installParent
    if ([IO.Directory]::Exists($InstallRoot)) {
        Assert-OrdinaryDirectory $InstallRoot
        if (@([IO.Directory]::EnumerateFileSystemEntries($InstallRoot)).Count -ne 0) {
            Remove-AllowlistedInstall $InstallRoot
        }
    }
    if (-not [IO.Directory]::Exists($InstallRoot)) { [IO.Directory]::CreateDirectory($InstallRoot) | Out-Null }
    Assert-OrdinaryDirectory $InstallRoot
    [IO.File]::Copy((Join-Path $stage $serverName), (Join-Path $InstallRoot $serverName), $true)
    [IO.File]::Copy((Join-Path $stage $helperName), (Join-Path $InstallRoot $helperName), $true)
    [IO.File]::Copy((Join-Path $stage "SHA256SUMS.txt"), (Join-Path $InstallRoot "SHA256SUMS.txt"), $true)
    $extensionRoot = Join-Path $InstallRoot "extension"
    if ([IO.Directory]::Exists($extensionRoot)) { Assert-OrdinaryDirectory $extensionRoot; [IO.Directory]::Delete($extensionRoot, $true) }
    [IO.Directory]::Move($extensionStage, $extensionRoot)
    [IO.File]::WriteAllText((Join-Path $InstallRoot $script:OwnerMarker), $script:OwnerMarkerValue + "`n", (New-Object Text.UTF8Encoding($false)))

    $desktopPath = Join-Path $InstallRoot $serverName
    $startupArguments = if ($EnableShell) {
        if ($StartHelper) { "--enable-shell --start-helper" } else { "--enable-shell" }
    }
    else {
        if ($StartHelper) { "--start-helper" } else { "" }
    }
    $launchArguments = @()
    if ($EnableShell) { $launchArguments += "--enable-shell" }
    if ($StartHelper) { $launchArguments += "--start-helper" }
    $launchArgumentText = $launchArguments -join " "
    Write-Output "Saving settings..."
    Write-SettingsFile -ShellEnabled $EnableShell -DesktopControlEnabled $StartHelper -StartAtLogin (-not $NoStartup)
    $tokenPath = Join-Path $env:USERPROFILE ".local-browser-bridge\token"
    $uninstallerUrl = "https://raw.githubusercontent.com/$($script:Repository)/v$resolved/scripts/uninstall-windows.ps1"
    $startupDirectory = [Environment]::GetFolderPath("Startup")
    Assert-OrdinaryDirectory $startupDirectory
    $legacyStartup = Join-Path $startupDirectory "Local Browser Bridge.cmd"
    if ([IO.File]::Exists($legacyStartup) -and (Test-OwnedLauncher $legacyStartup $InstallRoot)) {
        [IO.File]::Delete($legacyStartup)
    }
    $startMenu = Join-Path ([Environment]::GetFolderPath("Programs")) $script:StartMenuName
    if (-not [IO.Directory]::Exists($startMenu)) { [IO.Directory]::CreateDirectory($startMenu) | Out-Null }
    Assert-OrdinaryDirectory $startMenu
    foreach ($legacyName in @(
        "Open Local Browser Bridge.cmd",
        "Finish Browser Extension Setup.cmd",
        "Start Computer Helper.cmd",
        "Uninstall Local Browser Bridge.cmd"
    )) {
        $legacyPath = Join-Path $startMenu $legacyName
        if ([IO.File]::Exists($legacyPath) -and (Test-OwnedLauncher $legacyPath $InstallRoot)) {
            [IO.File]::Delete($legacyPath)
        }
    }
    New-OwnedShortcut `
        (Join-Path $startMenu "Local Browser Bridge.lnk") `
        $desktopPath "" $InstallRoot "Open Local Browser Bridge"
    New-OwnedShortcut `
        (Join-Path $startMenu "Finish Browser Extension Setup.lnk") `
        $desktopPath "--extension-setup" $InstallRoot "Finish Local Browser Bridge extension setup"
    $powershellPath = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
    $uninstallArguments = "-NoLogo -NoProfile -WindowStyle Hidden -Command `"& ([scriptblock]::Create((Invoke-RestMethod '$uninstallerUrl')))`""
    New-OwnedShortcut `
        (Join-Path $startMenu "Uninstall Local Browser Bridge.lnk") `
        $powershellPath $uninstallArguments $InstallRoot "Uninstall Local Browser Bridge"
    if (-not $NoStartup) {
        $startup = Join-Path $startupDirectory $script:StartupName
        New-OwnedShortcut $startup $desktopPath $startupArguments $InstallRoot "Start Local Browser Bridge at sign-in"
    }
    else {
        $startup = Join-Path $startupDirectory $script:StartupName
        if ([IO.File]::Exists($startup)) { [IO.File]::Delete($startup) }
    }

    if (-not $NoLaunch) {
        Write-Output "Starting Local Browser Bridge..."
        if ([string]::IsNullOrWhiteSpace($launchArgumentText)) {
            Start-Process -FilePath $desktopPath
        }
        else {
            Start-Process -FilePath $desktopPath -ArgumentList $launchArgumentText
        }
        Write-Output "Waiting for the authentication token..."
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while (-not [IO.File]::Exists($tokenPath) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        if ([IO.File]::Exists($tokenPath)) {
            $token = [IO.File]::ReadAllText($tokenPath).Trim()
            if ($token -match '^[A-Za-z0-9_-]{32,}$') { Start-Process "http://127.0.0.1:17373/#token=$token" }
        }
        else {
            $token = ""
        }
        Show-ExtensionSetup $extensionRoot $token
    }

    Write-Output "Installed Local Browser Bridge $resolved for the current user."
    Write-Output "Extension folder: $extensionRoot"
    Write-Output "Finish setup: open Start > Local Browser Bridge > Finish Browser Extension Setup."
    Write-Output "Open later: Start > Local Browser Bridge > Local Browser Bridge."
    if ($EnableShell) {
        Write-Output "Shell access is on by default: authenticated local API clients can run shell commands as you. Add -NoShell to turn it off."
    }
    else {
        Write-Output "Shell access is off. Re-run this installer with -EnableShell only if you intend to grant it."
    }
    if ($StartHelper) {
        Write-Output "Desktop control is on by default: this computer can be observed and controlled through the bridge. Add -NoDesktopControl to turn it off."
    }
    else {
        Write-Output "Desktop control is off. Re-run this installer with -StartHelper only if you intend to grant it."
    }
}
finally {
    if ([IO.Directory]::Exists($stage)) { [IO.Directory]::Delete($stage, $true) }
}
