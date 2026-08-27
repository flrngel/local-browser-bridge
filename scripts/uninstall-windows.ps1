[CmdletBinding()]
param(
    [string]$InstallRoot = "",
    [switch]$KeepToken,
    [switch]$NoBrowser,
    [switch]$DryRun,
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0
$script:OwnerMarker = ".lbb-install-owner"
$script:OwnerMarkerValue = "local-browser-bridge-install-v1"
$script:StartupName = "Local Browser Bridge.cmd"
$script:StartMenuName = "Local Browser Bridge"
$script:RemovedInstall = $false

if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
    if (-not $SelfTest) { throw "LOCALAPPDATA is unavailable." }
    $script:DefaultInstallRoot = [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) "Programs\Local Browser Bridge")).TrimEnd('\')
}
else {
    $script:DefaultInstallRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA "Programs\Local Browser Bridge")).TrimEnd('\')
}
if ([string]::IsNullOrWhiteSpace($InstallRoot)) { $InstallRoot = $script:DefaultInstallRoot }

function Write-Action([string]$Description) {
    if ($DryRun) { Write-Output "Would $Description" }
    else { Write-Output $Description }
}

function Assert-OrdinaryDirectory([string]$Path, [switch]$AllowMissing) {
    if (-not [IO.Directory]::Exists($Path)) {
        if ($AllowMissing -and -not [IO.File]::Exists($Path)) { return }
        throw "Required directory does not exist: $Path"
    }
    $attributes = [IO.File]::GetAttributes($Path)
    if (($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Refusing a reparse-point directory: $Path"
    }
}

function Assert-SafeInstallRoot([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathRooted($Path)) {
        throw "Install root must be an absolute path."
    }
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $volumeRoot = [IO.Path]::GetPathRoot($full).TrimEnd('\')
    if ($full -ieq $volumeRoot) { throw "Refusing a filesystem-root install path." }
    foreach ($blocked in @(
        $env:USERPROFILE,
        $env:LOCALAPPDATA,
        $env:APPDATA,
        $env:ProgramFiles,
        ${env:ProgramFiles(x86)},
        $env:SystemRoot
    )) {
        if ($blocked -and $full -ieq [IO.Path]::GetFullPath($blocked).TrimEnd('\')) {
            throw "Refusing a broad install path."
        }
    }

    $cursor = $full
    while (-not [string]::IsNullOrWhiteSpace($cursor)) {
        if ([IO.Directory]::Exists($cursor)) {
            Assert-OrdinaryDirectory $cursor
        }
        elseif ([IO.File]::Exists($cursor)) {
            throw "An install-root ancestor is a file: $cursor"
        }
        $parent = [IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName.TrimEnd('\')
    }
    return $full
}

function Assert-OrdinaryFileOrMissing([string]$Path) {
    if (-not [IO.File]::Exists($Path) -and -not [IO.Directory]::Exists($Path)) { return }
    if (-not [IO.File]::Exists($Path)) { throw "Refusing a non-file product path: $Path" }
    $attributes = [IO.File]::GetAttributes($Path)
    if (($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Refusing a reparse-point product file: $Path"
    }
}

function Assert-OrdinaryTreeOrMissing([string]$Path) {
    if (-not [IO.Directory]::Exists($Path) -and -not [IO.File]::Exists($Path)) { return }
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
            if (($attributes -band [IO.FileAttributes]::Directory) -ne 0) {
                $pending.Push($entry)
            }
        }
    }
}

function Test-ValidOwnerMarker([string]$Root) {
    $marker = Join-Path $Root $script:OwnerMarker
    if (-not [IO.File]::Exists($marker)) { return $false }
    Assert-OrdinaryFileOrMissing $marker
    $value = [IO.File]::ReadAllText($marker, [Text.Encoding]::UTF8).TrimEnd([char[]]"`r`n")
    return $value -ceq $script:OwnerMarkerValue
}

function Test-LegacyDefaultLayout([string]$Root) {
    if ([IO.Path]::GetFullPath($Root).TrimEnd('\') -ine $script:DefaultInstallRoot) { return $false }
    $server = @([IO.Directory]::EnumerateFiles($Root, "local-browser-bridge-v*-windows-x86_64.exe", [IO.SearchOption]::TopDirectoryOnly))
    return $server.Count -ge 1 -and [IO.File]::Exists((Join-Path $Root "extension\manifest.json"))
}

function Assert-OwnedInstallRoot([string]$Root) {
    if (-not [IO.Directory]::Exists($Root) -and -not [IO.File]::Exists($Root)) { return }
    Assert-OrdinaryDirectory $Root
    $marker = Join-Path $Root $script:OwnerMarker
    if ([IO.File]::Exists($marker) -or [IO.Directory]::Exists($marker)) {
        if (-not (Test-ValidOwnerMarker $Root)) {
            throw "The install ownership marker is invalid; nothing was removed."
        }
        return
    }
    if (-not (Test-LegacyDefaultLayout $Root)) {
        throw "The directory is not a recognized installer-owned Local Browser Bridge installation: $Root"
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
    return $Name -cmatch '^local-(browser-bridge|computer-helper)-v[0-9]+\.[0-9]+\.[0-9]+-windows-x86_64\.exe$'
}

function Get-AllowlistedProductFiles([string]$Root) {
    if (-not [IO.Directory]::Exists($Root)) { return @() }
    return @([IO.Directory]::EnumerateFiles($Root, "*", [IO.SearchOption]::TopDirectoryOnly) | Where-Object {
        Test-AllowlistedInstallName ([IO.Path]::GetFileName($_))
    })
}

function Assert-InstallEntries([string]$Root) {
    foreach ($file in @(Get-AllowlistedProductFiles $Root)) { Assert-OrdinaryFileOrMissing $file }
    Assert-OrdinaryTreeOrMissing (Join-Path $Root "extension")
}

function Stop-InstalledProcesses([string]$Root) {
    $prefix = [IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    foreach ($process in @(Get-Process -ErrorAction SilentlyContinue)) {
        try {
            $path = $process.Path
            if ($path -and [IO.Path]::GetFullPath($path).StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
                Write-Action "stop product process $($process.Id)"
                if (-not $DryRun) { Stop-Process -Id $process.Id -Force }
            }
        }
        catch { }
    }
}

function Remove-OrdinaryFile([string]$Path) {
    if (-not [IO.File]::Exists($Path)) { return }
    Assert-OrdinaryFileOrMissing $Path
    Write-Action "remove file: $Path"
    if (-not $DryRun) { [IO.File]::Delete($Path) }
}

function Remove-OrdinaryTree([string]$Path) {
    if (-not [IO.Directory]::Exists($Path)) { return }
    Assert-OrdinaryTreeOrMissing $Path
    Write-Action "remove product directory: $Path"
    if (-not $DryRun) { [IO.Directory]::Delete($Path, $true) }
}

function Test-InstallRootHasUnknownEntries([string]$Root) {
    foreach ($entry in [IO.Directory]::EnumerateFileSystemEntries($Root)) {
        if (-not (Test-AllowlistedInstallName ([IO.Path]::GetFileName($entry)))) { return $true }
    }
    return $false
}

function Remove-InstallTree([string]$Root) {
    if (-not [IO.Directory]::Exists($Root) -and -not [IO.File]::Exists($Root)) { return }
    Assert-OwnedInstallRoot $Root
    Assert-InstallEntries $Root
    $script:RemovedInstall = $true
    Stop-InstalledProcesses $Root
    if (-not $DryRun) { Start-Sleep -Milliseconds 250 }

    foreach ($file in @(Get-AllowlistedProductFiles $Root | Where-Object {
        [IO.Path]::GetFileName($_) -cne $script:OwnerMarker
    })) {
        Remove-OrdinaryFile $file
    }
    Remove-OrdinaryTree (Join-Path $Root "extension")

    if (Test-InstallRootHasUnknownEntries $Root) {
        Write-Warning "Retained the install directory because it contains files not owned by the installer: $Root"
    }
    else {
        Remove-OrdinaryFile (Join-Path $Root $script:OwnerMarker)
        Write-Action "remove empty install directory: $Root"
        if (-not $DryRun -and [IO.Directory]::Exists($Root)) {
            try { [IO.Directory]::Delete($Root, $false) } catch { }
        }
    }
}

function Test-OwnedLauncher([string]$Path, [string]$Root) {
    if (-not [IO.File]::Exists($Path)) { return $false }
    Assert-OrdinaryFileOrMissing $Path
    $content = [IO.File]::ReadAllText($Path)
    return $content.Contains("Local Browser Bridge") -and
        ($content.Contains($Root) -or $content.Contains("flrngel/local-browser-bridge"))
}

function Remove-StartupAndStartMenu([string]$Root) {
    $startupDirectory = [Environment]::GetFolderPath("Startup")
    Assert-OrdinaryDirectory $startupDirectory
    $startup = Join-Path $startupDirectory $script:StartupName
    if ([IO.File]::Exists($startup)) {
        if (Test-OwnedLauncher $startup $Root) { Remove-OrdinaryFile $startup }
        else { Write-Warning "Retained a same-named Startup item that was not recognized as product-owned: $startup" }
    }

    $startMenu = Join-Path ([Environment]::GetFolderPath("Programs")) $script:StartMenuName
    if (-not [IO.Directory]::Exists($startMenu)) { return }
    Assert-OrdinaryDirectory $startMenu
    foreach ($name in @(
        "Open Local Browser Bridge.cmd",
        "Finish Browser Extension Setup.cmd",
        "Start Computer Helper.cmd",
        "Uninstall Local Browser Bridge.cmd"
    )) {
        $path = Join-Path $startMenu $name
        if ([IO.File]::Exists($path)) {
            if (Test-OwnedLauncher $path $Root) { Remove-OrdinaryFile $path }
            else { Write-Warning "Retained an unrecognized Start-menu file: $path" }
        }
    }
    if (@([IO.Directory]::EnumerateFileSystemEntries($startMenu)).Count -eq 0) {
        Write-Action "remove empty Start-menu directory: $startMenu"
        if (-not $DryRun) { [IO.Directory]::Delete($startMenu, $false) }
    }
}

function Remove-Token {
    if ($KeepToken) { Write-Output "Kept the bridge token by request."; return }
    if ([string]::IsNullOrWhiteSpace($env:USERPROFILE)) { throw "USERPROFILE is unavailable." }
    $token = Join-Path $env:USERPROFILE ".local-browser-bridge\token"
    $tokenDirectory = Split-Path -Parent $token
    if ([IO.Directory]::Exists($tokenDirectory)) { Assert-OrdinaryDirectory $tokenDirectory }
    elseif ([IO.File]::Exists($tokenDirectory)) { throw "Refusing a non-directory token parent: $tokenDirectory" }
    if ([IO.File]::Exists($token)) { Remove-OrdinaryFile $token }
    elseif ([IO.Directory]::Exists($token)) { throw "Refusing a non-file token path: $token" }
    if ([IO.Directory]::Exists($tokenDirectory) -and
        @([IO.Directory]::EnumerateFileSystemEntries($tokenDirectory)).Count -eq 0) {
        Write-Action "remove empty token directory: $tokenDirectory"
        if (-not $DryRun) { [IO.Directory]::Delete($tokenDirectory, $false) }
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
    $opened = $false
    foreach ($candidate in $candidates) {
        if ($candidate -and [IO.File]::Exists($candidate)) {
            $url = if ([IO.Path]::GetFileName($candidate) -ieq "msedge.exe") { "edge://extensions" } else { "chrome://extensions" }
            Start-Process -FilePath $candidate -ArgumentList $url
            $opened = $true
        }
    }
    return $opened
}

function Finish-BrowserCleanup {
    if ($NoBrowser -or $DryRun -or -not $script:RemovedInstall) { return }
    $opened = Open-ExtensionsPage
    $firstLine = if ($opened) { "The installed browser extensions page is open." } else { "Open chrome://extensions or edge://extensions." }
    $message = "$firstLine`n`nThe unpacked extension files are gone. If a Local Browser Bridge card remains, click Remove once. Browser profile files were intentionally left untouched."
    try {
        Add-Type -AssemblyName PresentationFramework
        [System.Windows.MessageBox]::Show($message, "Finish Local Browser Bridge Removal") | Out-Null
    }
    catch { Write-Output $message }
}

function Invoke-SelfTest {
    $scratch = Join-Path ([IO.Path]::GetFullPath((Get-Location).Path)) (".lbb-uninstaller-self-test-" + [Guid]::NewGuid().ToString("N"))
    [IO.Directory]::CreateDirectory($scratch) | Out-Null
    $savedDryRun = $DryRun
    try {
        $root = Join-Path $scratch "Local Browser Bridge"
        [IO.Directory]::CreateDirectory((Join-Path $root "extension")) | Out-Null
        [IO.File]::WriteAllText((Join-Path $root $script:OwnerMarker), $script:OwnerMarkerValue + "`n", (New-Object Text.UTF8Encoding($false)))
        [IO.File]::WriteAllText((Join-Path $root "local-browser-bridge-v0.0.0-windows-x86_64.exe"), "binary")
        [IO.File]::WriteAllText((Join-Path $root "extension\manifest.json"), "{}")
        [IO.File]::WriteAllText((Join-Path $root "user-note.txt"), "keep")
        $null = Assert-SafeInstallRoot $root
        Assert-OwnedInstallRoot $root
        Assert-InstallEntries $root

        $DryRun = $true
        Remove-InstallTree $root
        if (-not [IO.File]::Exists((Join-Path $root "local-browser-bridge-v0.0.0-windows-x86_64.exe"))) {
            throw "Dry-run self-test changed product files."
        }
        $DryRun = $false
        Remove-InstallTree $root
        if ([IO.File]::Exists((Join-Path $root "local-browser-bridge-v0.0.0-windows-x86_64.exe")) -or
            [IO.Directory]::Exists((Join-Path $root "extension"))) {
            throw "Allowlist removal self-test failed."
        }
        if (-not [IO.File]::Exists((Join-Path $root "user-note.txt")) -or
            -not [IO.File]::Exists((Join-Path $root $script:OwnerMarker))) {
            throw "Unknown-file retention self-test failed."
        }
        Remove-InstallTree $root

        $unownedRoot = Join-Path $scratch "Unowned LBB"
        [IO.Directory]::CreateDirectory((Join-Path $unownedRoot "extension")) | Out-Null
        [IO.File]::WriteAllText((Join-Path $unownedRoot "local-browser-bridge-v0.0.0-windows-x86_64.exe"), "binary")
        [IO.File]::WriteAllText((Join-Path $unownedRoot "extension\manifest.json"), "{}")
        $unownedRejected = $false
        try { Assert-OwnedInstallRoot $unownedRoot } catch { $unownedRejected = $true }
        if (-not $unownedRejected) { throw "Custom-root ownership refusal self-test failed." }

        $rejected = $false
        try { $null = Assert-SafeInstallRoot ([IO.Path]::GetPathRoot($root)) } catch { $rejected = $true }
        if (-not $rejected) { throw "Broad-root refusal self-test failed." }
        Write-Output "Windows one-command uninstaller self-test passed."
    }
    finally {
        $DryRun = $savedDryRun
        if ([IO.Directory]::Exists($scratch)) { [IO.Directory]::Delete($scratch, $true) }
    }
}

if ($SelfTest) { Invoke-SelfTest; exit 0 }
$InstallRoot = Assert-SafeInstallRoot $InstallRoot
Assert-OwnedInstallRoot $InstallRoot
if ([IO.Directory]::Exists($InstallRoot)) { Assert-InstallEntries $InstallRoot }
Remove-StartupAndStartMenu $InstallRoot
Remove-InstallTree $InstallRoot
Remove-Token
Finish-BrowserCleanup

if ($DryRun) {
    Write-Output "Dry run complete. No files, processes, browser state, or credentials were changed."
}
else {
    Write-Output "Local Browser Bridge was removed for the current user."
    Write-Output "Browser profiles were not edited. Remove any stale extension card from the extensions page."
}
