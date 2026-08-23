#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Preflight", "Postflight", "SelfTest")]
    [string]$Mode,

    [ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')]
    [string]$Version,

    [ValidatePattern('^[0-9a-f]{40}$')]
    [string]$FinalSha,

    [string]$Repository,
    [string]$TrustedGitExecutable,
    [string]$TrustedEmptyHooksDirectory,
    [string]$ChecksumManifest,

    [ValidatePattern('^[0-9a-f]{64}$')]
    [string]$ChecksumManifestSha256,

    [string]$ServerExecutable,
    [string]$ComputerHelperExecutable,
    [string]$ExtensionZip,
    [string]$ExtractedExtension,
    [string]$ReleaseCandidateBinding,
    [string]$PreflightRecord,
    [string]$OutputRecord
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$script:ExtensionFiles = @(
    "background.js",
    "content.js",
    "dom-core.js",
    "frame-agent.js",
    "lib.js",
    "manifest.json",
    "popup.css",
    "popup.html",
    "popup.js",
    "stop-guard.js",
    "LICENSE"
)
$script:ExtensionPermissions = @("tabs", "scripting", "storage", "alarms", "debugger", "tabGroups")
$script:ExtensionHostPermissions = @("http://*/*", "https://*/*", "file://*/*")
$script:ExtensionDescription = "Connects browser tabs to a loopback-only control surface for local browser agents."
$script:TrustedRepositoryOrigin = "https://github.com/flrngel/local-browser-bridge.git"
$script:ReleaseCandidateBindingFields = @(
    "productVersion", "repository", "tag", "sourceSha", "tagObjectSha",
    "workflowRunId", "workflowRunAttempt", "artifactId", "artifactName",
    "artifactZipBytes", "artifactZipSha256", "checksumManifestSha256",
    "attestationInvocationUri", "attestedAssetCount", "githubHostedRunner", "assets"
)
$script:MaxExtensionEntryBytes = 2MB
$script:MaxExtensionArchiveBytes = 8MB
$script:SelfTestCleanupRetryMilliseconds = @(0, 50, 100, 250, 500, 1000)
$script:Utf8NoBom = [Text.UTF8Encoding]::new($false, $true)
$script:AsciiStrict = [Text.Encoding]::GetEncoding(
    "us-ascii",
    [Text.EncoderFallback]::ExceptionFallback,
    [Text.DecoderFallback]::ExceptionFallback
)

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
    $full = [IO.Path]::GetFullPath($Path)
    $directory = if ([IO.Directory]::Exists($full)) { [IO.DirectoryInfo]::new($full) }
        else { [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName($full)) }
    while ($null -ne $directory) {
        if (($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "$Label must not traverse a reparse-point directory."
        }
        $directory = $directory.Parent
    }
}

function Initialize-TrustedGitExecutable {
    if ($Mode -cne "SelfTest" -and $Version -ceq "0.12.12") {
        Assert-RequiredArgument $TrustedGitExecutable "TrustedGitExecutable"
        if (-not [IO.Path]::IsPathRooted($TrustedGitExecutable)) {
            throw "TrustedGitExecutable must be an absolute path for v0.12.12."
        }
        $script:GitExecutable = Resolve-RequiredFile $TrustedGitExecutable "TrustedGitExecutable"
        Assert-RequiredArgument $TrustedEmptyHooksDirectory "TrustedEmptyHooksDirectory"
        if (-not [IO.Path]::IsPathRooted($TrustedEmptyHooksDirectory)) {
            throw "TrustedEmptyHooksDirectory must be an absolute path for v0.12.12."
        }
        $script:EmptyHooksDirectory = Resolve-RequiredDirectory $TrustedEmptyHooksDirectory "TrustedEmptyHooksDirectory"
        if (@(Get-ChildItem -LiteralPath $script:EmptyHooksDirectory -Force).Count -ne 0) {
            throw "TrustedEmptyHooksDirectory must be empty."
        }
        $script:UseHardenedGit = $true
        return
    }
    $gitCommand = Get-Command git -CommandType Application -ErrorAction Stop | Select-Object -First 1
    if ($null -eq $gitCommand -or -not [IO.Path]::IsPathRooted($gitCommand.Source)) {
        throw "A trusted Git executable could not be resolved."
    }
    $script:GitExecutable = [IO.Path]::GetFullPath($gitCommand.Source)
    $script:UseHardenedGit = $false
}

function Resolve-RequiredDirectory {
    param([string]$Path, [string]$Label)
    $resolved = [IO.Path]::GetFullPath($Path)
    if (-not [IO.Directory]::Exists($resolved)) {
        throw "$Label does not exist."
    }
    $item = [IO.DirectoryInfo]::new($resolved)
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "$Label must not be a reparse point."
    }
    Assert-NoReparseAncestorChain $resolved $Label
    return $resolved.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
}

function Resolve-NewOutputFile {
    param([string]$Path, [string]$Label)
    $resolved = [IO.Path]::GetFullPath($Path)
    if ([IO.File]::Exists($resolved) -or [IO.Directory]::Exists($resolved)) {
        throw "$Label already exists; evidence records are never overwritten."
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
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-BytesSha256 {
    param([byte[]]$Bytes)
    $hasher = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($hasher.ComputeHash($Bytes))).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
    }
}

function Get-StringSha256 {
    param([string]$Value)
    return Get-BytesSha256 $script:Utf8NoBom.GetBytes($Value)
}

function New-RunNonce {
    $bytes = [byte[]]::new(32)
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $rng.GetBytes($bytes)
        return ([BitConverter]::ToString($bytes)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $rng.Dispose()
        [Array]::Clear($bytes, 0, $bytes.Length)
    }
}

function Read-BoundedBytes {
    param([IO.Stream]$Stream, [int64]$ExpectedLength, [string]$Label)
    if ($ExpectedLength -le 0 -or $ExpectedLength -gt $script:MaxExtensionEntryBytes) {
        throw "$Label has an invalid uncompressed size."
    }
    $memory = [IO.MemoryStream]::new()
    try {
        $buffer = [byte[]]::new(65536)
        while (($count = $Stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $memory.Write($buffer, 0, $count)
            if ($memory.Length -gt $script:MaxExtensionEntryBytes) {
                throw "$Label exceeded the evidence payload limit."
            }
        }
        if ($memory.Length -ne $ExpectedLength) {
            throw "$Label length changed while it was read."
        }
        return $memory.ToArray()
    }
    finally {
        $memory.Dispose()
    }
}

function Assert-ExactKeys {
    param([object]$Object, [string[]]$Expected, [string]$Label)
    if ($null -eq $Object) {
        throw "$Label must be an object."
    }
    $actual = @($Object.PSObject.Properties.Name | Sort-Object)
    $wanted = @($Expected | Sort-Object)
    if (($actual -join "`n") -cne ($wanted -join "`n")) {
        throw "$Label contains missing or unexpected fields."
    }
}

function Assert-ExactStringArray {
    param([object[]]$Actual, [string[]]$Expected, [string]$Label)
    if ($Actual.Count -ne $Expected.Count) {
        throw "$Label does not contain the exact canonical values."
    }
    for ($index = 0; $index -lt $Expected.Count; $index += 1) {
        if ($Actual[$index] -isnot [string] -or $Actual[$index] -cne $Expected[$index]) {
            throw "$Label does not contain the exact canonical values."
        }
    }
}

function Write-NewJson {
    param([string]$Path, [object]$Value)
    $json = $Value | ConvertTo-Json -Depth 20
    $temporary = "$Path.new"
    if ([IO.File]::Exists($temporary)) {
        throw "A stale temporary evidence record exists."
    }
    try {
        $bytes = $script:Utf8NoBom.GetBytes("$json`n")
        $stream = [IO.File]::Open(
            $temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
        )
        try {
            $stream.Write($bytes, 0, $bytes.Length)
            $stream.Flush($true)
        }
        finally {
            $stream.Dispose()
            [Array]::Clear($bytes, 0, $bytes.Length)
        }
        [IO.File]::Move($temporary, $Path)
    }
    catch {
        if ([IO.File]::Exists($temporary)) {
            [IO.File]::Delete($temporary)
        }
        throw
    }
}

function Get-PathStringComparer {
    if ([IO.Path]::DirectorySeparatorChar -eq [char]92) {
        return [StringComparer]::OrdinalIgnoreCase
    }
    return [StringComparer]::Ordinal
}

function Get-RootPreservingFullDirectoryPath {
    param([string]$Path)
    $resolved = [IO.Path]::GetFullPath($Path)
    $fileSystemRoot = [IO.Path]::GetPathRoot($resolved)
    if ([String]::IsNullOrEmpty($fileSystemRoot)) {
        throw "Self-test cleanup path does not have a filesystem root."
    }
    $pathComparer = Get-PathStringComparer
    if ($pathComparer.Equals($resolved, $fileSystemRoot)) {
        return $fileSystemRoot
    }
    $directorySeparators = [char[]]@([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    return $resolved.TrimEnd($directorySeparators)
}

function Test-IsTruePathNotFoundException {
    param([Exception]$Exception)
    $current = $Exception
    while ($null -ne $current) {
        if ($current -is [IO.FileNotFoundException] -or $current -is [IO.DirectoryNotFoundException]) {
            return $true
        }
        $current = $current.InnerException
    }
    return $false
}

function Test-ExactPathAbsent {
    param([string]$Path)
    try {
        [void][IO.File]::GetAttributes($Path)
        return $false
    }
    catch {
        if (Test-IsTruePathNotFoundException $_.Exception) {
            return $true
        }
        throw
    }
}

function Resolve-ExactSelfTestCleanupRoot {
    param([string]$Path)
    $resolved = Get-RootPreservingFullDirectoryPath $Path
    $temporaryRoot = Get-RootPreservingFullDirectoryPath ([IO.Path]::GetTempPath())
    $parent = [IO.Path]::GetDirectoryName($resolved)
    $name = [IO.Path]::GetFileName($resolved)
    if (-not [String]::IsNullOrEmpty($parent)) {
        $parent = Get-RootPreservingFullDirectoryPath $parent
    }
    $pathComparer = Get-PathStringComparer
    if ([String]::IsNullOrEmpty($parent) -or -not $pathComparer.Equals($parent, $temporaryRoot) -or
        $name -cnotmatch '^lbb-browser-candidate-[0-9a-f]{32}$') {
        throw "Self-test cleanup refused a path outside its exact temporary fixture scope."
    }
    return $resolved
}

function Remove-ExactSelfTestTreeOnce {
    param([string]$RootPath)
    if (Test-ExactPathAbsent $RootPath) {
        return
    }
    $rootAttributes = [IO.File]::GetAttributes($RootPath)
    if (($rootAttributes -band [IO.FileAttributes]::Directory) -eq 0) {
        throw "Self-test cleanup fixture root is not a directory."
    }
    $pending = [Collections.Generic.Stack[string]]::new()
    $directories = [Collections.Generic.List[string]]::new()
    $pending.Push($RootPath)
    while ($pending.Count -gt 0) {
        $directoryPath = $pending.Pop()
        if (Test-ExactPathAbsent $directoryPath) {
            continue
        }
        $directoryAttributes = [IO.File]::GetAttributes($directoryPath)
        if (($directoryAttributes -band [IO.FileAttributes]::Directory) -eq 0) {
            throw "Self-test cleanup encountered a non-directory in its directory inventory."
        }
        if (($directoryAttributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Self-test cleanup refused a reparse point in its temporary fixture."
        }
        $directory = [IO.DirectoryInfo]::new($directoryPath)
        [void]$directories.Add($directory.FullName)
        foreach ($entry in $directory.GetFileSystemInfos()) {
            $entry.Refresh()
            if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Self-test cleanup refused a reparse point in its temporary fixture."
            }
            if (($entry.Attributes -band [IO.FileAttributes]::Directory) -ne 0) {
                $pending.Push($entry.FullName)
                continue
            }
            if (($entry.Attributes -band [IO.FileAttributes]::ReadOnly) -ne 0) {
                [IO.File]::SetAttributes(
                    $entry.FullName,
                    ($entry.Attributes -band (-bnot [IO.FileAttributes]::ReadOnly))
                )
            }
            [IO.File]::Delete($entry.FullName)
        }
    }
    for ($index = $directories.Count - 1; $index -ge 0; $index -= 1) {
        $directoryPath = $directories[$index]
        if (Test-ExactPathAbsent $directoryPath) {
            continue
        }
        $attributes = [IO.File]::GetAttributes($directoryPath)
        if (($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Self-test cleanup refused a reparse point in its temporary fixture."
        }
        if (($attributes -band [IO.FileAttributes]::ReadOnly) -ne 0) {
            [IO.File]::SetAttributes(
                $directoryPath,
                ($attributes -band (-bnot [IO.FileAttributes]::ReadOnly))
            )
        }
        [IO.Directory]::Delete($directoryPath, $false)
    }
}

function Remove-ExactSelfTestDirectory {
    param([string]$Path)
    $rootPath = Resolve-ExactSelfTestCleanupRoot $Path
    $lastError = $null
    foreach ($delayMilliseconds in $script:SelfTestCleanupRetryMilliseconds) {
        if ($delayMilliseconds -gt 0) {
            Start-Sleep -Milliseconds $delayMilliseconds
        }
        try {
            Remove-ExactSelfTestTreeOnce $rootPath
            if (Test-ExactPathAbsent $rootPath) {
                return
            }
            throw "The exact temporary fixture still exists after cleanup."
        }
        catch {
            $lastError = $_
        }
    }
    $detail = if ($null -eq $lastError) { "unknown cleanup failure" } else { $lastError.Exception.Message }
    throw "Self-test cleanup could not remove its exact temporary fixture after bounded retries: $detail"
}

function Invoke-GitText {
    param([string]$RepositoryPath, [string[]]$Arguments)
    if ($script:UseHardenedGit) {
        # v0.12.12 accepts only a fresh isolated clone. These global switches
        # prevent replacement-object substitution and lazy-fetch helpers;
        # command-scoped settings neutralize monitor and hook execution.
        $output = & $script:GitExecutable --no-replace-objects --no-lazy-fetch `
            -c core.longpaths=true -c core.fsmonitor=false -c core.hooksPath=$script:EmptyHooksDirectory `
            -C $RepositoryPath @Arguments 2>$null
    }
    else {
        # Preserve the v0.12.2 contract, including compatibility with Git
        # versions that predate --no-lazy-fetch.
        $output = & $script:GitExecutable -C $RepositoryPath @Arguments 2>$null
    }
    if ($LASTEXITCODE -ne 0) {
        throw "The candidate Git identity could not be verified."
    }
    return (@($output) -join "`n").Trim()
}

function Assert-HardenedGitEnvironment {
    if ($env:GIT_CONFIG_NOSYSTEM -cne "1" -or $env:GIT_CONFIG_GLOBAL -cne "NUL" -or
        $env:GIT_ATTR_NOSYSTEM -cne "1" -or $env:GIT_ALLOW_PROTOCOL -cne "https" -or
        $env:GIT_CONFIG_COUNT -cne "0" -or $env:GIT_TERMINAL_PROMPT -cne "0") {
        throw "v0.12.12 requires the isolated Git environment declared by the acceptance protocol."
    }
    $allowedGitEnvironment = @(
        "GIT_ALLOW_PROTOCOL", "GIT_ATTR_NOSYSTEM", "GIT_CONFIG_COUNT",
        "GIT_CONFIG_GLOBAL", "GIT_CONFIG_NOSYSTEM", "GIT_TERMINAL_PROMPT"
    )
    foreach ($entry in [Environment]::GetEnvironmentVariables("Process").GetEnumerator()) {
        $name = [string]$entry.Key
        if ($name -cmatch '^GIT_' -and $allowedGitEnvironment -cnotcontains $name) {
            throw "v0.12.12 refuses unexpected Git process variables."
        }
    }
    if (-not [String]::IsNullOrEmpty($env:SSH_ASKPASS)) {
        throw "v0.12.12 refuses SSH_ASKPASS in the isolated HTTPS Git environment."
    }
    # Git is isolated with its explicit process-scoped configuration variables.
    # The acceptance lane must not repurpose HOME or USERPROFILE because they are
    # ambient OS/user state and are not an authority for candidate verification.
}

function Assert-HardenedRepositoryMetadata {
    param([string]$RepositoryPath)
    $gitDirectory = [IO.Path]::Combine($RepositoryPath, ".git")
    if (-not [IO.Directory]::Exists($gitDirectory)) {
        throw "v0.12.12 requires a fresh clone with an ordinary .git directory."
    }
    $gitDirectoryInfo = [IO.DirectoryInfo]::new($gitDirectory)
    if (($gitDirectoryInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "v0.12.12 refuses a reparse-point Git directory."
    }
    $configPath = Resolve-RequiredFile ([IO.Path]::Combine($gitDirectory, "config")) "repository local Git config"
    $configText = [IO.File]::ReadAllText($configPath, $script:Utf8NoBom)
    $dangerousConfig = '(?im)^\s*\[(?:include(?:if)?|filter|diff|credential|protocol|url)(?:\s|\])|^\s*(?:fsmonitor|hooksPath|attributesFile|promisor|partialCloneFilter|uploadPack|receivePack)\s*='
    if ([regex]::IsMatch($configText, $dangerousConfig)) {
        throw "v0.12.12 refuses executable, included, rewritten, or promisor local Git configuration."
    }
    $originPattern = '(?im)^\s*url\s*=\s*' + [regex]::Escape($script:TrustedRepositoryOrigin) + '\s*$'
    if (-not [regex]::IsMatch($configText, '^\s*\[remote\s+"origin"\]\s*$', [Text.RegularExpressions.RegexOptions]::Multiline) -or
        -not [regex]::IsMatch($configText, $originPattern)) {
        throw "v0.12.12 repository origin is not the fixed HTTPS release repository."
    }
    foreach ($forbiddenPath in @(
        [IO.Path]::Combine($gitDirectory, "refs", "replace"),
        [IO.Path]::Combine($gitDirectory, "objects", "info", "alternates"),
        [IO.Path]::Combine($gitDirectory, "shallow")
    )) {
        if ([IO.File]::Exists($forbiddenPath) -or [IO.Directory]::Exists($forbiddenPath)) {
            throw "v0.12.12 repository contains replacement, alternate, or shallow object state."
        }
    }
    $packDirectory = [IO.Path]::Combine($gitDirectory, "objects", "pack")
    if ([IO.Directory]::Exists($packDirectory)) {
        $packInfo = [IO.DirectoryInfo]::new($packDirectory)
        if (($packInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            @($packInfo.GetFiles("*.promisor", [IO.SearchOption]::TopDirectoryOnly)).Count -ne 0) {
            throw "v0.12.12 repository contains promisor object state."
        }
    }
}

function Assert-CleanExactCheckout {
    param([string]$RepositoryPath, [string]$ExpectedSha)
    if ($script:UseHardenedGit) {
        Assert-HardenedGitEnvironment
        Assert-HardenedRepositoryMetadata $RepositoryPath
    }
    $inside = Invoke-GitText $RepositoryPath @("rev-parse", "--is-inside-work-tree")
    if ($inside -cne "true") {
        throw "Repository is not a Git worktree."
    }
    $head = Invoke-GitText $RepositoryPath @("rev-parse", "HEAD")
    if ($head -cne $ExpectedSha) {
        throw "Repository HEAD does not equal FINAL_SHA."
    }
    $resolved = Invoke-GitText $RepositoryPath @("rev-parse", "$ExpectedSha^{commit}")
    if ($resolved -cne $ExpectedSha) {
        throw "FINAL_SHA does not resolve to its exact commit."
    }
    $status = Invoke-GitText $RepositoryPath @("status", "--porcelain=v1", "--untracked-files=all")
    if (-not [String]::IsNullOrEmpty($status)) {
        throw "Repository must be clean, including untracked files."
    }
    $ignored = Invoke-GitText $RepositoryPath @("ls-files", "--others", "--ignored", "--exclude-standard")
    if (-not [String]::IsNullOrEmpty($ignored)) {
        throw "Repository must be an exact clean checkout without ignored files."
    }
    if ($script:UseHardenedGit) {
        $indexFlags = @(Invoke-GitText $RepositoryPath @("ls-files", "-v") -split "`n")
        if (@($indexFlags | Where-Object { $_ -cnotmatch '^H ' }).Count -ne 0) {
            throw "v0.12.12 refuses assume-unchanged or skip-worktree index entries."
        }
    }
}

function Read-CanonicalChecksums {
    param([string]$Path, [string]$ExpectedVersion)
    if ([IO.Path]::GetFileName($Path) -cne "SHA256SUMS.txt") {
        throw "Checksum manifest must use the canonical SHA256SUMS.txt filename."
    }
    $raw = [IO.File]::ReadAllBytes($Path)
    if ($raw.Length -eq 0 -or $raw.Length -gt 16384) {
        throw "Checksum manifest has an invalid size."
    }
    $text = $script:AsciiStrict.GetString($raw)
    if ($text.Contains("`r") -or -not $text.EndsWith("`n")) {
        throw "Checksum manifest must be canonical LF-terminated ASCII."
    }
    $lines = @($text.Substring(0, $text.Length - 1).Split("`n"))
    $expectedNames = @(
        "local-browser-bridge-v$ExpectedVersion-windows-x86_64.exe",
        "local-computer-helper-v$ExpectedVersion-windows-x86_64.exe",
        "local-browser-bridge-v$ExpectedVersion-macos-universal.tar.gz",
        "local-browser-bridge-extension-v$ExpectedVersion.zip"
    )
    if ($lines.Count -ne $expectedNames.Count) {
        throw "Checksum manifest must contain exactly four canonical entries."
    }
    $entries = [ordered]@{}
    for ($index = 0; $index -lt $expectedNames.Count; $index += 1) {
        $match = [Text.RegularExpressions.Regex]::Match(
            $lines[$index],
            '^(?<hash>[0-9a-f]{64})  (?<name>[^\\/:]+)$',
            [Text.RegularExpressions.RegexOptions]::CultureInvariant
        )
        if (-not $match.Success -or $match.Groups['name'].Value -cne $expectedNames[$index]) {
            throw "Checksum manifest entry order or spelling is not canonical."
        }
        $entries[$expectedNames[$index]] = $match.Groups['hash'].Value
    }
    return [pscustomobject]@{
        Names = $expectedNames
        Hashes = $entries
    }
}

function Read-SourceVersion {
    param([string]$RepositoryPath)
    $cargoText = [IO.File]::ReadAllText([IO.Path]::Combine($RepositoryPath, "Cargo.toml"), $script:Utf8NoBom)
    $cargoMatch = [regex]::Match($cargoText, '(?m)^version = "(?<version>[^"]+)"$')
    if (-not $cargoMatch.Success) {
        throw "Cargo.toml package version could not be read."
    }
    $lockText = [IO.File]::ReadAllText([IO.Path]::Combine($RepositoryPath, "Cargo.lock"), $script:Utf8NoBom)
    $lockMatch = [regex]::Match(
        $lockText,
        '(?ms)^\[\[package\]\]\r?\nname = "local-browser-bridge"\r?\nversion = "(?<version>[^"]+)"'
    )
    if (-not $lockMatch.Success -or $lockMatch.Groups['version'].Value -cne $cargoMatch.Groups['version'].Value) {
        throw "Cargo package versions are not aligned."
    }
    return $cargoMatch.Groups['version'].Value
}

function Read-ExtensionVersion {
    param([byte[]]$ManifestBytes, [byte[]]$LibraryBytes, [string]$ExpectedVersion)
    $manifestText = $script:Utf8NoBom.GetString($ManifestBytes)
    $manifest = $manifestText | ConvertFrom-Json
    Assert-ExactKeys $manifest @(
        "manifest_version", "name", "version", "description", "minimum_chrome_version",
        "permissions", "host_permissions", "background", "content_scripts", "action",
        "content_security_policy"
    ) "extension manifest"
    if ($manifest.manifest_version -ne 3 -or $manifest.name -cne "Local Browser Bridge" -or
        $manifest.description -cne $script:ExtensionDescription -or
        $manifest.version -cne $ExpectedVersion -or $manifest.minimum_chrome_version -isnot [string] -or
        $manifest.minimum_chrome_version -cne "140") {
        throw "Extension manifest identity does not match the candidate."
    }
    Assert-ExactStringArray @($manifest.permissions) $script:ExtensionPermissions "extension manifest permissions"
    Assert-ExactStringArray @($manifest.host_permissions) $script:ExtensionHostPermissions "extension manifest host permissions"

    Assert-ExactKeys $manifest.background @("service_worker", "type") "extension manifest background"
    if ($manifest.background.service_worker -cne "background.js" -or $manifest.background.type -cne "module") {
        throw "Extension manifest background is not canonical."
    }

    $contentScripts = @($manifest.content_scripts)
    if ($contentScripts.Count -ne 2) {
        throw "Extension manifest must contain exactly two canonical content-script stages."
    }
    foreach ($stage in $contentScripts) {
        Assert-ExactKeys $stage @("matches", "js", "run_at") "extension manifest content-script stage"
        Assert-ExactStringArray @($stage.matches) $script:ExtensionHostPermissions "extension manifest content-script matches"
    }
    Assert-ExactStringArray @($contentScripts[0].js) @("stop-guard.js") "extension manifest early Stop guard"
    if ($contentScripts[0].run_at -cne "document_start") {
        throw "Extension manifest early Stop guard must run at document_start."
    }
    Assert-ExactStringArray @($contentScripts[1].js) @("dom-core.js", "content.js") "extension manifest control content scripts"
    if ($contentScripts[1].run_at -cne "document_idle") {
        throw "Extension manifest control content scripts must run at document_idle."
    }

    Assert-ExactKeys $manifest.action @("default_popup", "default_title") "extension manifest action"
    if ($manifest.action.default_popup -cne "popup.html" -or
        $manifest.action.default_title -cne "Local Browser Bridge") {
        throw "Extension manifest action is not canonical."
    }
    Assert-ExactKeys $manifest.content_security_policy @("extension_pages") "extension manifest content security policy"
    if ($manifest.content_security_policy.extension_pages -cne "script-src 'self'; object-src 'none'") {
        throw "Extension manifest content security policy is not canonical."
    }

    $libraryText = $script:Utf8NoBom.GetString($LibraryBytes)
    $libraryMatch = [regex]::Match($libraryText, '(?m)^export const VERSION = "(?<version>[^"]+)";$')
    if (-not $libraryMatch.Success -or $libraryMatch.Groups['version'].Value -cne $ExpectedVersion) {
        throw "Extension library version does not match the candidate."
    }
    return [pscustomobject]@{
        Manifest = $manifest.version
        Library = $libraryMatch.Groups['version'].Value
        MinimumChrome = $manifest.minimum_chrome_version
        Permissions = @($manifest.permissions)
        HostPermissions = @($manifest.host_permissions)
    }
}

function Get-CheckoutPayload {
    param([string]$RepositoryPath)
    $payload = [ordered]@{}
    foreach ($name in $script:ExtensionFiles) {
        $source = if ($name -ceq "LICENSE") {
            [IO.Path]::Combine($RepositoryPath, "LICENSE")
        }
        else {
            [IO.Path]::Combine($RepositoryPath, "extension", $name)
        }
        $resolved = Resolve-RequiredFile $source "candidate source payload"
        $bytes = [IO.File]::ReadAllBytes($resolved)
        if ($bytes.Length -le 0 -or $bytes.Length -gt $script:MaxExtensionEntryBytes) {
            throw "Candidate source payload has an invalid size."
        }
        $payload[$name] = $bytes
    }
    return $payload
}

function Get-ExtractedPayload {
    param([string]$DirectoryPath)
    $items = @(Get-ChildItem -LiteralPath $DirectoryPath -Force)
    $actualNames = @($items | ForEach-Object { $_.Name } | Sort-Object)
    $expectedNames = @($script:ExtensionFiles | Sort-Object)
    if (($actualNames -join "`n") -cne ($expectedNames -join "`n")) {
        throw "Extracted extension folder does not contain the exact allowed inventory."
    }
    $payload = [ordered]@{}
    foreach ($name in $script:ExtensionFiles) {
        $path = [IO.Path]::Combine($DirectoryPath, $name)
        $resolved = Resolve-RequiredFile $path "extracted extension payload"
        $bytes = [IO.File]::ReadAllBytes($resolved)
        if ($bytes.Length -le 0 -or $bytes.Length -gt $script:MaxExtensionEntryBytes) {
            throw "Extracted extension payload has an invalid size."
        }
        $payload[$name] = $bytes
    }
    return $payload
}

function Get-ZipPayload {
    param([string]$ZipPath)
    $zipInfo = [IO.FileInfo]::new($ZipPath)
    if ($zipInfo.Length -le 0 -or $zipInfo.Length -gt $script:MaxExtensionArchiveBytes) {
        throw "Extension ZIP has an invalid archive size."
    }
    Add-Type -AssemblyName System.IO.Compression -ErrorAction Stop
    Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction SilentlyContinue
    $stream = [IO.File]::Open($ZipPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $archive = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Read, $false)
        try {
            if ($archive.Entries.Count -ne $script:ExtensionFiles.Count) {
                throw "Extension ZIP does not contain the exact allowed inventory."
            }
            $payload = [ordered]@{}
            $total = 0L
            for ($index = 0; $index -lt $script:ExtensionFiles.Count; $index += 1) {
                $entry = $archive.Entries[$index]
                $expectedName = $script:ExtensionFiles[$index]
                if ($entry.FullName -cne $expectedName -or $entry.Name -cne $expectedName) {
                    throw "Extension ZIP inventory order or spelling is not canonical."
                }
                if (
                    $entry.LastWriteTime.Year -ne 1980 -or
                    $entry.LastWriteTime.Month -ne 1 -or
                    $entry.LastWriteTime.Day -ne 1 -or
                    $entry.LastWriteTime.Hour -ne 0 -or
                    $entry.LastWriteTime.Minute -ne 0 -or
                    $entry.LastWriteTime.Second -ne 0
                ) {
                    throw "Extension ZIP timestamps are not deterministic."
                }
                $unixType = (($entry.ExternalAttributes -shr 16) -band 0xF000)
                $dosDirectory = (($entry.ExternalAttributes -band 0x10) -ne 0)
                if ($dosDirectory -or ($unixType -ne 0 -and $unixType -ne 0x8000)) {
                    throw "Extension ZIP contains a linked or non-regular entry."
                }
                $entryStream = $entry.Open()
                try {
                    $bytes = Read-BoundedBytes $entryStream $entry.Length "extension ZIP entry"
                }
                finally {
                    $entryStream.Dispose()
                }
                $total += $bytes.Length
                if ($total -gt $script:MaxExtensionArchiveBytes) {
                    throw "Extension ZIP exceeded the total evidence payload limit."
                }
                $payload[$expectedName] = $bytes
            }
            return $payload
        }
        finally {
            $archive.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}

function Assert-PayloadEqual {
    param([Collections.Specialized.OrderedDictionary]$Left, [Collections.Specialized.OrderedDictionary]$Right, [string]$Label)
    foreach ($name in $script:ExtensionFiles) {
        $leftBytes = [byte[]]$Left[$name]
        $rightBytes = [byte[]]$Right[$name]
        if ($leftBytes.Length -ne $rightBytes.Length -or (Get-BytesSha256 $leftBytes) -cne (Get-BytesSha256 $rightBytes)) {
            throw "$Label payload bytes differ."
        }
    }
}

function Get-SafeInventory {
    param([Collections.Specialized.OrderedDictionary]$Payload)
    $inventory = @()
    foreach ($name in $script:ExtensionFiles) {
        $bytes = [byte[]]$Payload[$name]
        $inventory += [ordered]@{
            name = $name
            bytes = $bytes.Length
            sha256 = Get-BytesSha256 $bytes
        }
    }
    return $inventory
}

function Get-CombinedPayloadSha256 {
    param([object[]]$Inventory)
    $canonical = ""
    foreach ($item in $Inventory) {
        $canonical += "$($item.sha256)  $($item.name)`n"
    }
    return Get-StringSha256 $canonical
}

function Get-ValidatedReleaseCandidateBinding {
    param([string]$Path, [object]$Candidate, [string]$ReleaseDirectory)
    $bindingPath = Resolve-RequiredFile $Path "ReleaseCandidateBinding"
    $bindingInfo = [IO.FileInfo]::new($bindingPath)
    if ($bindingInfo.Length -le 0 -or $bindingInfo.Length -gt 1MB) {
        throw "ReleaseCandidateBinding has an invalid size."
    }
    $bytes = [IO.File]::ReadAllBytes($bindingPath)
    try {
        $wrapper = $script:Utf8NoBom.GetString($bytes) | ConvertFrom-Json
    }
    catch { throw "ReleaseCandidateBinding is not strict UTF-8 JSON." }
    finally { [Array]::Clear($bytes, 0, $bytes.Length) }

    Assert-ExactKeys $wrapper @(
        "schemaVersion", "productVersion", "repository", "tag", "sourceSha",
        "tagObjectSha", "workflowRunId", "workflowRunAttempt", "artifactId",
        "artifactName", "artifactZipBytes", "artifactZipSha256",
        "checksumManifestSha256", "attestationInvocationUri", "attestedAssetCount",
        "githubHostedRunner", "assets", "passed"
    ) "release-candidate wrapper binding"
    if ($wrapper.schemaVersion -ne 1 -or $wrapper.passed -ne $true -or
        $wrapper.productVersion -cne $Candidate.version -or
        $wrapper.repository -cne "flrngel/local-browser-bridge" -or
        $wrapper.tag -cne "v$($Candidate.version)" -or
        $wrapper.sourceSha -cne $Candidate.finalSha -or
        [string]$wrapper.tagObjectSha -cnotmatch '^[0-9a-f]{40}$' -or
        [string]$wrapper.workflowRunId -cnotmatch '^[1-9][0-9]*$' -or
        [string]$wrapper.workflowRunAttempt -cnotmatch '^[1-9][0-9]*$' -or
        [string]$wrapper.artifactId -cnotmatch '^[1-9][0-9]*$' -or
        $wrapper.artifactName -cne "release-candidate" -or
        $wrapper.artifactZipBytes -isnot [ValueType] -or [int64]$wrapper.artifactZipBytes -le 0 -or
        [string]$wrapper.artifactZipSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $wrapper.checksumManifestSha256 -cne $Candidate.checksumManifest.sha256 -or
        $wrapper.attestationInvocationUri -cne ("https://github.com/flrngel/local-browser-bridge/actions/runs/{0}/attempts/{1}" -f
            [string]$wrapper.workflowRunId, [string]$wrapper.workflowRunAttempt) -or
        $wrapper.attestedAssetCount -ne 5 -or $wrapper.githubHostedRunner -ne $true) {
        throw "ReleaseCandidateBinding does not bind the exact release workflow attempt and candidate."
    }

    $expectedNames = @($Candidate.checksumManifest.canonicalNamesInOrder) + "SHA256SUMS.txt"
    $assets = @($wrapper.assets)
    if ($assets.Count -ne 5) {
        throw "ReleaseCandidateBinding must contain the exact five assets."
    }
    $normalizedAssets = @()
    for ($index = 0; $index -lt $expectedNames.Count; $index += 1) {
        $asset = $assets[$index]
        Assert-ExactKeys $asset @("file", "bytes", "sha256") "release-candidate asset"
        $assetPath = Resolve-RequiredFile ([IO.Path]::Combine($ReleaseDirectory, $expectedNames[$index])) "release-candidate asset"
        if ($asset.file -cne $expectedNames[$index] -or
            $asset.bytes -isnot [ValueType] -or [int64]$asset.bytes -ne ([IO.FileInfo]::new($assetPath)).Length -or
            [string]$asset.sha256 -cnotmatch '^[0-9a-f]{64}$' -or
            $asset.sha256 -cne (Get-Sha256 $assetPath)) {
            throw "ReleaseCandidateBinding asset inventory does not byte-match the exact candidate."
        }
        $normalizedAssets += [ordered]@{
            file = [string]$asset.file
            bytes = [int64]$asset.bytes
            sha256 = [string]$asset.sha256
        }
    }
    return [ordered]@{
        productVersion = [string]$wrapper.productVersion
        repository = [string]$wrapper.repository
        tag = [string]$wrapper.tag
        sourceSha = [string]$wrapper.sourceSha
        tagObjectSha = [string]$wrapper.tagObjectSha
        workflowRunId = [string]$wrapper.workflowRunId
        workflowRunAttempt = [string]$wrapper.workflowRunAttempt
        artifactId = [string]$wrapper.artifactId
        artifactName = [string]$wrapper.artifactName
        artifactZipBytes = [int64]$wrapper.artifactZipBytes
        artifactZipSha256 = [string]$wrapper.artifactZipSha256
        checksumManifestSha256 = [string]$wrapper.checksumManifestSha256
        attestationInvocationUri = [string]$wrapper.attestationInvocationUri
        attestedAssetCount = [int]$wrapper.attestedAssetCount
        githubHostedRunner = [bool]$wrapper.githubHostedRunner
        assets = @($normalizedAssets)
    }
}

function Assert-ReleaseCandidateBindingDomain {
    param([object]$Binding, [object]$Candidate, [string]$Label)
    Assert-ExactKeys $Binding $script:ReleaseCandidateBindingFields $Label
    if ($Binding.productVersion -cne $Candidate.version -or
        $Binding.repository -cne "flrngel/local-browser-bridge" -or
        $Binding.tag -cne "v$($Candidate.version)" -or
        $Binding.sourceSha -cne $Candidate.finalSha -or
        [string]$Binding.tagObjectSha -cnotmatch '^[0-9a-f]{40}$' -or
        [string]$Binding.workflowRunId -cnotmatch '^[1-9][0-9]*$' -or
        [string]$Binding.workflowRunAttempt -cnotmatch '^[1-9][0-9]*$' -or
        [string]$Binding.artifactId -cnotmatch '^[1-9][0-9]*$' -or
        $Binding.artifactName -cne "release-candidate" -or
        $Binding.artifactZipBytes -isnot [ValueType] -or
        [int64]$Binding.artifactZipBytes -le 0 -or
        [string]$Binding.artifactZipSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $Binding.checksumManifestSha256 -cne $Candidate.checksumManifest.sha256 -or
        $Binding.attestationInvocationUri -cne
            ("https://github.com/flrngel/local-browser-bridge/actions/runs/{0}/attempts/{1}" -f
                [string]$Binding.workflowRunId, [string]$Binding.workflowRunAttempt) -or
        $Binding.githubHostedRunner -ne $true -or $Binding.attestedAssetCount -ne 5 -or
        @($Binding.assets).Count -ne 5) {
        throw "$Label does not match the exact candidate."
    }
    $expectedNames = @($Candidate.checksumManifest.canonicalNamesInOrder) + "SHA256SUMS.txt"
    $assets = @($Binding.assets)
    for ($index = 0; $index -lt $assets.Count; $index += 1) {
        Assert-ExactKeys $assets[$index] @("file", "bytes", "sha256") "$Label asset"
        if ($assets[$index].file -cne $expectedNames[$index] -or
            $assets[$index].bytes -isnot [ValueType] -or [int64]$assets[$index].bytes -le 0 -or
            [string]$assets[$index].sha256 -cnotmatch '^[0-9a-f]{64}$') {
            throw "$Label contains an invalid asset binding."
        }
    }
    if ($assets[0].sha256 -cne $Candidate.server.sha256 -or
        $assets[1].sha256 -cne $Candidate.computerHelper.sha256 -or
        $assets[3].sha256 -cne $Candidate.extension.sha256 -or
        $assets[4].sha256 -cne $Candidate.checksumManifest.sha256) {
        throw "$Label asset digests do not match the exact candidate."
    }
}

function Get-CandidateSnapshot {
    param(
        [string]$ExpectedVersion,
        [string]$ExpectedSha,
        [string]$RepositoryPath,
        [string]$ManifestPath,
        [string]$ManifestExpectedSha,
        [string]$ServerPath,
        [AllowNull()][string]$ComputerHelperPath,
        [string]$ZipPath,
        [string]$ExtractedPath
    )
    Assert-CleanExactCheckout $RepositoryPath $ExpectedSha
    $sourceVersion = Read-SourceVersion $RepositoryPath
    if ($sourceVersion -cne $ExpectedVersion) {
        throw "Source package version does not match the requested stable version."
    }
    $manifestSha = Get-Sha256 $ManifestPath
    if ($manifestSha -cne $ManifestExpectedSha.ToLowerInvariant()) {
        throw "Checksum manifest does not match its externally supplied SHA-256."
    }
    $checksums = Read-CanonicalChecksums $ManifestPath $ExpectedVersion
    $expectedServerName = "local-browser-bridge-v$ExpectedVersion-windows-x86_64.exe"
    if ([IO.Path]::GetFileName($ServerPath) -cne $expectedServerName) {
        throw "Server executable does not use the canonical candidate filename."
    }
    $serverInfo = [IO.FileInfo]::new($ServerPath)
    if ($serverInfo.Length -le 0 -or $serverInfo.Length -gt 100MB) {
        throw "Server executable has an invalid size."
    }
    $serverSha = Get-Sha256 $ServerPath
    if ($serverSha -cne $checksums.Hashes[$expectedServerName]) {
        throw "Server executable does not match the canonical checksum manifest."
    }
    $helperBinding = $null
    if ($ExpectedVersion -ceq "0.12.12") {
        if ([String]::IsNullOrWhiteSpace($ComputerHelperPath)) {
            throw "Computer helper executable is required for v0.12.12."
        }
        $expectedHelperName = "local-computer-helper-v$ExpectedVersion-windows-x86_64.exe"
        if ([IO.Path]::GetFileName($ComputerHelperPath) -cne $expectedHelperName) {
            throw "Computer helper executable does not use the canonical candidate filename."
        }
        $helperInfo = [IO.FileInfo]::new($ComputerHelperPath)
        if ($helperInfo.Length -le 0 -or $helperInfo.Length -gt 100MB) {
            throw "Computer helper executable has an invalid size."
        }
        $helperSha = Get-Sha256 $ComputerHelperPath
        if ($helperSha -cne $checksums.Hashes[$expectedHelperName]) {
            throw "Computer helper executable does not match the canonical checksum manifest."
        }
        $helperBinding = [ordered]@{
            name = $expectedHelperName
            bytes = $helperInfo.Length
            sha256 = $helperSha
        }
    }
    $expectedZipName = "local-browser-bridge-extension-v$ExpectedVersion.zip"
    if ([IO.Path]::GetFileName($ZipPath) -cne $expectedZipName) {
        throw "Extension ZIP does not use the canonical candidate filename."
    }
    $zipSha = Get-Sha256 $ZipPath
    if ($zipSha -cne $checksums.Hashes[$expectedZipName]) {
        throw "Extension ZIP does not match the canonical checksum manifest."
    }
    $checkoutPayload = Get-CheckoutPayload $RepositoryPath
    $zipPayload = Get-ZipPayload $ZipPath
    $extractedPayload = Get-ExtractedPayload $ExtractedPath
    Assert-PayloadEqual $checkoutPayload $zipPayload "checkout and extension ZIP"
    Assert-PayloadEqual $checkoutPayload $extractedPayload "checkout and extracted extension"
    $versions = Read-ExtensionVersion $zipPayload["manifest.json"] $zipPayload["lib.js"] $ExpectedVersion
    $checkoutInventory = @(Get-SafeInventory $checkoutPayload)
    $zipInventory = @(Get-SafeInventory $zipPayload)
    $extractedInventory = @(Get-SafeInventory $extractedPayload)
    $candidate = [ordered]@{
        version = $ExpectedVersion
        finalSha = $ExpectedSha
        gitClean = $true
        checksumManifest = [ordered]@{
            name = "SHA256SUMS.txt"
            bytes = ([IO.FileInfo]::new($ManifestPath)).Length
            sha256 = $manifestSha
            externallySuppliedSha256 = $ManifestExpectedSha.ToLowerInvariant()
            canonicalEntryCount = 4
            canonicalNamesInOrder = @($checksums.Names)
        }
        server = [ordered]@{
            name = $expectedServerName
            bytes = $serverInfo.Length
            sha256 = $serverSha
        }
    }
    if ($ExpectedVersion -ceq "0.12.12") {
        $candidate.computerHelper = $helperBinding
    }
    $candidate.extension = [ordered]@{
            name = $expectedZipName
            bytes = ([IO.FileInfo]::new($ZipPath)).Length
            sha256 = $zipSha
            manifestVersion = $versions.Manifest
            libraryVersion = $versions.Library
            minimumChromeVersion = $versions.MinimumChrome
            permissions = @($versions.Permissions)
            hostPermissions = @($versions.HostPermissions)
            archiveInventory = $zipInventory
            extractedPayloadInventory = $extractedInventory
            checkoutPayloadInventory = $checkoutInventory
            combinedPayloadSha256 = Get-CombinedPayloadSha256 $checkoutInventory
    }
    return $candidate
}

function Get-CandidateBindingDomain {
    param([string]$RunNonce, [string]$PreflightSha256, [object]$Candidate)
    $binding = [ordered]@{
        runNonce = $RunNonce
        preflightRecordSha256 = $PreflightSha256
        finalSha = [string]$Candidate.finalSha
        checksumManifestSha256 = [string]$Candidate.checksumManifest.sha256
        serverSha256 = [string]$Candidate.server.sha256
    }
    if ($Candidate.version -ceq "0.12.12") {
        $binding.computerHelperSha256 = [string]$Candidate.computerHelper.sha256
    }
    $binding.extensionZipSha256 = [string]$Candidate.extension.sha256
    $binding.extractedPayloadSha256 = [string]$Candidate.extension.combinedPayloadSha256
    return $binding
}

function Assert-BindingRecord {
    param([object]$Record, [string]$ExpectedPhase)
    $expectedKeys = @("schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed", "runNonce", "candidate")
    if ($Record.candidate.version -ceq "0.12.12") {
        $expectedKeys = @(
            "schemaVersion", "evidenceType", "phase", "recordedAtUtc", "passed",
            "runNonce", "releaseCandidateBinding", "candidate"
        )
    }
    if ($ExpectedPhase -ceq "postflight") {
        $expectedKeys += @("candidateBinding", "preflightRecordSha256", "unchanged")
    }
    Assert-ExactKeys $Record $expectedKeys "candidate binding record"
    if ($Record.schemaVersion -ne 1 -or $Record.evidenceType -cne "stock-user-chrome-candidate-binding" -or $Record.phase -cne $ExpectedPhase -or $Record.passed -ne $true) {
        throw "Candidate binding record identity is invalid."
    }
    if ($Record.runNonce -isnot [string] -or $Record.runNonce -cnotmatch '^[0-9a-f]{64}$') {
        throw "Candidate binding record run nonce is invalid."
    }
    if ($Record.candidate.version -ceq "0.12.12") {
        Assert-ReleaseCandidateBindingDomain $Record.releaseCandidateBinding $Record.candidate "releaseCandidateBinding"
    }
    if ($ExpectedPhase -ceq "postflight") {
        if ($Record.preflightRecordSha256 -isnot [string] -or $Record.preflightRecordSha256 -cnotmatch '^[0-9a-f]{64}$') {
            throw "Candidate postflight preflight digest is invalid."
        }
        $unchangedKeys = @(
            "checkoutHead", "checkoutClean", "checksumManifest", "serverExecutable", "extensionZip", "extractedPayload"
        )
        if ($Record.candidate.version -ceq "0.12.12") {
            $unchangedKeys = @(
                "checkoutHead", "checkoutClean", "checksumManifest", "serverExecutable",
                "computerHelperExecutable", "extensionZip", "extractedPayload"
            )
        }
        Assert-ExactKeys $Record.unchanged $unchangedKeys "candidate unchanged assertions"
        foreach ($property in $Record.unchanged.PSObject.Properties) {
            if ($property.Value -ne $true) {
                throw "Candidate unchanged assertion is not true."
            }
        }
        $bindingKeys = @(
            "runNonce", "preflightRecordSha256", "finalSha", "checksumManifestSha256",
            "serverSha256", "extensionZipSha256", "extractedPayloadSha256"
        )
        if ($Record.candidate.version -ceq "0.12.12") {
            $bindingKeys = @(
                "runNonce", "preflightRecordSha256", "finalSha", "checksumManifestSha256",
                "serverSha256", "computerHelperSha256", "extensionZipSha256", "extractedPayloadSha256"
            )
        }
        Assert-ExactKeys $Record.candidateBinding $bindingKeys "candidate postflight domain"
        $expectedBinding = Get-CandidateBindingDomain $Record.runNonce $Record.preflightRecordSha256 $Record.candidate
        if (($Record.candidateBinding | ConvertTo-Json -Depth 5 -Compress) -cne
            ($expectedBinding | ConvertTo-Json -Depth 5 -Compress)) {
            throw "Candidate postflight domain does not match its candidate and preflight digest."
        }
    }
    $candidateKeys = @("version", "finalSha", "gitClean", "checksumManifest", "server", "extension")
    if ($Record.candidate.version -ceq "0.12.12") {
        $candidateKeys = @("version", "finalSha", "gitClean", "checksumManifest", "server", "computerHelper", "extension")
    }
    Assert-ExactKeys $Record.candidate $candidateKeys "candidate binding"
    Assert-ExactKeys $Record.candidate.checksumManifest @(
        "name", "bytes", "sha256", "externallySuppliedSha256", "canonicalEntryCount", "canonicalNamesInOrder"
    ) "candidate checksum binding"
    Assert-ExactKeys $Record.candidate.extension @(
        "name", "bytes", "sha256", "manifestVersion", "libraryVersion", "minimumChromeVersion", "archiveInventory",
        "permissions", "hostPermissions", "extractedPayloadInventory", "checkoutPayloadInventory", "combinedPayloadSha256"
    ) "candidate extension binding"
    Assert-ExactKeys $Record.candidate.server @("name", "bytes", "sha256") "candidate server binding"
    if ($Record.candidate.version -ceq "0.12.12") {
        Assert-ExactKeys $Record.candidate.computerHelper @("name", "bytes", "sha256") "candidate computer helper binding"
    }
}

function Invoke-Preflight {
    foreach ($item in @(
        @($Version, "Version"), @($FinalSha, "FinalSha"), @($Repository, "Repository"),
        @($ChecksumManifest, "ChecksumManifest"), @($ChecksumManifestSha256, "ChecksumManifestSha256"),
        @($ServerExecutable, "ServerExecutable"), @($ExtensionZip, "ExtensionZip"), @($ExtractedExtension, "ExtractedExtension"),
        @($OutputRecord, "OutputRecord")
    )) {
        Assert-RequiredArgument $item[0] $item[1]
    }
    $repositoryPath = Resolve-RequiredDirectory $Repository "Repository"
    $manifestPath = Resolve-RequiredFile $ChecksumManifest "ChecksumManifest"
    $serverPath = Resolve-RequiredFile $ServerExecutable "ServerExecutable"
    $helperPath = $null
    if ($Version -ceq "0.12.12") {
        Assert-RequiredArgument $ComputerHelperExecutable "ComputerHelperExecutable"
        Assert-RequiredArgument $ReleaseCandidateBinding "ReleaseCandidateBinding"
        $helperPath = Resolve-RequiredFile $ComputerHelperExecutable "ComputerHelperExecutable"
    }
    $zipPath = Resolve-RequiredFile $ExtensionZip "ExtensionZip"
    $extractedPath = Resolve-RequiredDirectory $ExtractedExtension "ExtractedExtension"
    $outputPath = Resolve-NewOutputFile $OutputRecord "OutputRecord"
    $candidate = Get-CandidateSnapshot $Version $FinalSha $repositoryPath $manifestPath $ChecksumManifestSha256 $serverPath $helperPath $zipPath $extractedPath
    $releaseBinding = $null
    if ($Version -ceq "0.12.12") {
        $releaseBinding = Get-ValidatedReleaseCandidateBinding `
            $ReleaseCandidateBinding $candidate ([IO.Path]::GetDirectoryName($manifestPath))
    }
    $record = [ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-candidate-binding"
        phase = "preflight"
        recordedAtUtc = [DateTime]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        passed = $true
        runNonce = New-RunNonce
    }
    if ($Version -ceq "0.12.12") { $record.releaseCandidateBinding = $releaseBinding }
    $record.candidate = $candidate
    Write-NewJson $outputPath $record
    Write-Output "Candidate preflight passed; the allowlisted binding record was written."
}

function Invoke-Postflight {
    foreach ($item in @(
        @($Version, "Version"), @($FinalSha, "FinalSha"), @($Repository, "Repository"),
        @($ChecksumManifest, "ChecksumManifest"), @($ChecksumManifestSha256, "ChecksumManifestSha256"),
        @($ServerExecutable, "ServerExecutable"), @($ExtensionZip, "ExtensionZip"), @($ExtractedExtension, "ExtractedExtension"),
        @($PreflightRecord, "PreflightRecord"), @($OutputRecord, "OutputRecord")
    )) {
        Assert-RequiredArgument $item[0] $item[1]
    }
    $repositoryPath = Resolve-RequiredDirectory $Repository "Repository"
    $manifestPath = Resolve-RequiredFile $ChecksumManifest "ChecksumManifest"
    $serverPath = Resolve-RequiredFile $ServerExecutable "ServerExecutable"
    $helperPath = $null
    if ($Version -ceq "0.12.12") {
        Assert-RequiredArgument $ComputerHelperExecutable "ComputerHelperExecutable"
        Assert-RequiredArgument $ReleaseCandidateBinding "ReleaseCandidateBinding"
        $helperPath = Resolve-RequiredFile $ComputerHelperExecutable "ComputerHelperExecutable"
    }
    $zipPath = Resolve-RequiredFile $ExtensionZip "ExtensionZip"
    $extractedPath = Resolve-RequiredDirectory $ExtractedExtension "ExtractedExtension"
    $preflightPath = Resolve-RequiredFile $PreflightRecord "PreflightRecord"
    $outputPath = Resolve-NewOutputFile $OutputRecord "OutputRecord"
    $preflight = [IO.File]::ReadAllText($preflightPath, $script:Utf8NoBom) | ConvertFrom-Json
    Assert-BindingRecord $preflight "preflight"
    $preflightSha256 = Get-Sha256 $preflightPath
    $candidate = Get-CandidateSnapshot $Version $FinalSha $repositoryPath $manifestPath $ChecksumManifestSha256 $serverPath $helperPath $zipPath $extractedPath
    $releaseBinding = $null
    if ($Version -ceq "0.12.12") {
        $releaseBinding = Get-ValidatedReleaseCandidateBinding `
            $ReleaseCandidateBinding $candidate ([IO.Path]::GetDirectoryName($manifestPath))
        if (($releaseBinding | ConvertTo-Json -Depth 10 -Compress) -cne
            ($preflight.releaseCandidateBinding | ConvertTo-Json -Depth 10 -Compress)) {
            throw "ReleaseCandidateBinding changed after preflight."
        }
    }
    $before = $preflight.candidate | ConvertTo-Json -Depth 20 -Compress
    $after = $candidate | ConvertTo-Json -Depth 20 -Compress
    if ($before -cne $after) {
        throw "Candidate payload or identity changed after browser acceptance."
    }
    $record = [ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-candidate-binding"
        phase = "postflight"
        recordedAtUtc = [DateTime]::UtcNow.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
        passed = $true
        runNonce = [string]$preflight.runNonce
    }
    if ($Version -ceq "0.12.12") { $record.releaseCandidateBinding = $releaseBinding }
    $record.candidate = $candidate
    $record.candidateBinding = Get-CandidateBindingDomain ([string]$preflight.runNonce) $preflightSha256 $candidate
    $record.preflightRecordSha256 = $preflightSha256
    $record.unchanged = [ordered]@{
            checkoutHead = $true
            checkoutClean = $true
            checksumManifest = $true
            serverExecutable = $true
    }
    if ($Version -ceq "0.12.12") {
        $record.unchanged.computerHelperExecutable = $true
    }
    $record.unchanged.extensionZip = $true
    $record.unchanged.extractedPayload = $true
    Write-NewJson $outputPath $record
    Write-Output "Candidate postflight passed; checkout and payload match their preflight boundary hashes."
}

function Invoke-SelfTest {
    $selfTestSource = [IO.File]::ReadAllText($PSCommandPath, $script:Utf8NoBom)
    if (-not $selfTestSource.Contains('& $script:GitExecutable --no-replace-objects --no-lazy-fetch `') -or
        -not $selfTestSource.Contains('-c core.longpaths=true -c core.fsmonitor=false -c core.hooksPath=$script:EmptyHooksDirectory `') -or
        -not $selfTestSource.Contains('& $script:GitExecutable -C $RepositoryPath @Arguments') -or
        -not $selfTestSource.Contains('Preserve the v0.12.2 contract')) {
        throw "Candidate binding lost its version-scoped hardened and legacy Git dispatch."
    }
    $root = [IO.Path]::Combine([IO.Path]::GetTempPath(), "lbb-browser-candidate-" + [Guid]::NewGuid().ToString("N"))
    $repositoryPath = [IO.Path]::Combine($root, "repository")
    $releasePath = [IO.Path]::Combine($root, "release")
    $extractedPath = [IO.Path]::Combine($root, "extracted")
    $fixtureOwned = $false
    try {
        $fileSystemRoot = [IO.Path]::GetPathRoot([IO.Path]::GetFullPath($root))
        $normalizedFileSystemRoot = Get-RootPreservingFullDirectoryPath $fileSystemRoot
        if (-not (Get-PathStringComparer).Equals($normalizedFileSystemRoot, $fileSystemRoot)) {
            throw "Self-test cleanup root-preserving path normalization failed."
        }
        if (-not (Test-ExactPathAbsent $root)) {
            throw "Self-test cleanup missing-path probe failed."
        }
        [IO.Directory]::CreateDirectory($root) | Out-Null
        $fixtureOwned = $true
        if (Test-ExactPathAbsent $root) {
            throw "Self-test cleanup existing-path probe failed."
        }
        [IO.Directory]::CreateDirectory([IO.Path]::Combine($repositoryPath, "extension")) | Out-Null
        [IO.Directory]::CreateDirectory($releasePath) | Out-Null
        [IO.Directory]::CreateDirectory($extractedPath) | Out-Null
        $testVersion = "0.12.2"
        [IO.File]::WriteAllText([IO.Path]::Combine($repositoryPath, "Cargo.toml"), "[package]`nname = `"local-browser-bridge`"`nversion = `"$testVersion`"`n", $script:Utf8NoBom)
        [IO.File]::WriteAllText([IO.Path]::Combine($repositoryPath, "Cargo.lock"), "[[package]]`nname = `"local-browser-bridge`"`nversion = `"$testVersion`"`n", $script:Utf8NoBom)
        [IO.File]::WriteAllText([IO.Path]::Combine($repositoryPath, ".gitignore"), "ignored.tmp`n", $script:Utf8NoBom)
        foreach ($name in $script:ExtensionFiles) {
            $contents = switch ($name) {
                "manifest.json" { '{"manifest_version":3,"name":"Local Browser Bridge","version":"0.12.2","description":"Connects browser tabs to a loopback-only control surface for local browser agents.","minimum_chrome_version":"140","permissions":["tabs","scripting","storage","alarms","debugger","tabGroups"],"host_permissions":["http://*/*","https://*/*","file://*/*"],"background":{"service_worker":"background.js","type":"module"},"content_scripts":[{"matches":["http://*/*","https://*/*","file://*/*"],"js":["stop-guard.js"],"run_at":"document_start"},{"matches":["http://*/*","https://*/*","file://*/*"],"js":["dom-core.js","content.js"],"run_at":"document_idle"}],"action":{"default_popup":"popup.html","default_title":"Local Browser Bridge"},"content_security_policy":{"extension_pages":"script-src ''self''; object-src ''none''"}}' }
                "lib.js" { 'export const VERSION = "0.12.2";' }
                default { "self-test-$name" }
            }
            $path = if ($name -ceq "LICENSE") {
                [IO.Path]::Combine($repositoryPath, $name)
            }
            else {
                [IO.Path]::Combine($repositoryPath, "extension", $name)
            }
            [IO.File]::WriteAllText($path, $contents, $script:Utf8NoBom)
            [IO.File]::WriteAllText([IO.Path]::Combine($extractedPath, $name), $contents, $script:Utf8NoBom)
        }
        & $script:GitExecutable -C $repositoryPath init -q
        & $script:GitExecutable -C $repositoryPath config user.email browser-evidence@example.invalid
        & $script:GitExecutable -C $repositoryPath config user.name "Browser Evidence Self Test"
        & $script:GitExecutable -C $repositoryPath remote add origin $script:TrustedRepositoryOrigin
        & $script:GitExecutable -C $repositoryPath add --all
        & $script:GitExecutable -C $repositoryPath commit -q -m "test fixture"
        if ($LASTEXITCODE -ne 0) { throw "Self-test Git fixture failed." }
        $testSha = (& $script:GitExecutable -C $repositoryPath rev-parse HEAD).Trim()

        # Exercise the v0.12.12 metadata gate without enabling hardened dispatch
        # for the recursive v0.12.2 compatibility fixture below.
        Assert-HardenedRepositoryMetadata $repositoryPath
        foreach ($adversarialConfig in @(
            [pscustomobject]@{ Key = "core.fsmonitor"; Value = "malicious-monitor"; Label = "fsmonitor" },
            [pscustomobject]@{ Key = "filter.malicious.process"; Value = "malicious-filter"; Label = "process filter" },
            [pscustomobject]@{ Key = "remote.origin.promisor"; Value = "true"; Label = "promisor remote" }
        )) {
            & $script:GitExecutable -C $repositoryPath config $adversarialConfig.Key $adversarialConfig.Value
            if ($LASTEXITCODE -ne 0) { throw "Self-test could not create adversarial Git config." }
            $refused = $false
            try { Assert-HardenedRepositoryMetadata $repositoryPath }
            catch { $refused = $true }
            if (-not $refused) {
                throw "v0.12.12 metadata gate accepted adversarial $($adversarialConfig.Label) config."
            }
            & $script:GitExecutable -C $repositoryPath config --unset-all $adversarialConfig.Key
            if ($LASTEXITCODE -ne 0) { throw "Self-test could not remove adversarial Git config." }
        }
        $replaceDirectory = [IO.Path]::Combine($repositoryPath, ".git", "refs", "replace")
        [IO.Directory]::CreateDirectory($replaceDirectory) | Out-Null
        [IO.File]::WriteAllText([IO.Path]::Combine($replaceDirectory, $testSha), $testSha + "`n", $script:AsciiStrict)
        $replaceRefused = $false
        try { Assert-HardenedRepositoryMetadata $repositoryPath }
        catch { $replaceRefused = $true }
        if (-not $replaceRefused) {
            throw "v0.12.12 metadata gate accepted a replacement-object ref."
        }
        [IO.File]::Delete([IO.Path]::Combine($replaceDirectory, $testSha))
        [IO.Directory]::Delete($replaceDirectory, $false)
        $promisorPath = [IO.Path]::Combine($repositoryPath, ".git", "objects", "pack", "self-test.promisor")
        [IO.File]::WriteAllBytes($promisorPath, [byte[]]::new(0))
        $promisorRefused = $false
        try { Assert-HardenedRepositoryMetadata $repositoryPath }
        catch { $promisorRefused = $true }
        if (-not $promisorRefused) {
            throw "v0.12.12 metadata gate accepted a promisor object marker."
        }
        [IO.File]::Delete($promisorPath)
        Assert-HardenedRepositoryMetadata $repositoryPath

        Add-Type -AssemblyName System.IO.Compression -ErrorAction Stop
        Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction SilentlyContinue
        $zipName = "local-browser-bridge-extension-v$testVersion.zip"
        $zipPath = [IO.Path]::Combine($releasePath, $zipName)
        $zipStream = [IO.File]::Open($zipPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        try {
            $archive = [IO.Compression.ZipArchive]::new($zipStream, [IO.Compression.ZipArchiveMode]::Create, $false)
            try {
                foreach ($name in $script:ExtensionFiles) {
                    $entry = $archive.CreateEntry($name, [IO.Compression.CompressionLevel]::Optimal)
                    $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
                    $entryStream = $entry.Open()
                    try {
                        $bytes = [IO.File]::ReadAllBytes([IO.Path]::Combine($extractedPath, $name))
                        $entryStream.Write($bytes, 0, $bytes.Length)
                    }
                    finally { $entryStream.Dispose() }
                }
            }
            finally { $archive.Dispose() }
        }
        finally { $zipStream.Dispose() }

        $assetNames = @(
            "local-browser-bridge-v$testVersion-windows-x86_64.exe",
            "local-computer-helper-v$testVersion-windows-x86_64.exe",
            "local-browser-bridge-v$testVersion-macos-universal.tar.gz",
            $zipName
        )
        foreach ($name in $assetNames[0..2]) {
            [IO.File]::WriteAllText([IO.Path]::Combine($releasePath, $name), "self-test-$name", $script:Utf8NoBom)
        }
        $manifestPath = [IO.Path]::Combine($releasePath, "SHA256SUMS.txt")
        $manifestText = ""
        foreach ($name in $assetNames) {
            $manifestText += "$(Get-Sha256 ([IO.Path]::Combine($releasePath, $name)))  $name`n"
        }
        [IO.File]::WriteAllText($manifestPath, $manifestText, $script:AsciiStrict)
        $manifestSha = Get-Sha256 $manifestPath
        $preflightPath = [IO.Path]::Combine($root, "preflight.json")
        $postflightPath = [IO.Path]::Combine($root, "postflight.json")
        & $PSCommandPath -Mode Preflight -Version $testVersion -FinalSha $testSha -Repository $repositoryPath `
            -ChecksumManifest $manifestPath -ChecksumManifestSha256 $manifestSha -ServerExecutable ([IO.Path]::Combine($releasePath, $assetNames[0])) -ExtensionZip $zipPath `
            -ExtractedExtension $extractedPath -OutputRecord $preflightPath | Out-Null
        if (-not [IO.File]::Exists($preflightPath)) {
            throw "Candidate preflight self-test failed."
        }
        & $PSCommandPath -Mode Postflight -Version $testVersion -FinalSha $testSha -Repository $repositoryPath `
            -ChecksumManifest $manifestPath -ChecksumManifestSha256 $manifestSha -ServerExecutable ([IO.Path]::Combine($releasePath, $assetNames[0])) -ExtensionZip $zipPath `
            -ExtractedExtension $extractedPath -PreflightRecord $preflightPath -OutputRecord $postflightPath | Out-Null
        if (-not [IO.File]::Exists($postflightPath)) {
            throw "Candidate postflight self-test failed."
        }
        $preflightRecord = [IO.File]::ReadAllText($preflightPath, $script:Utf8NoBom) | ConvertFrom-Json
        $postflightRecord = [IO.File]::ReadAllText($postflightPath, $script:Utf8NoBom) | ConvertFrom-Json
        Assert-BindingRecord $preflightRecord "preflight"
        Assert-BindingRecord $postflightRecord "postflight"
        if ($preflightRecord.runNonce -cne $postflightRecord.runNonce -or
            $postflightRecord.preflightRecordSha256 -cne (Get-Sha256 $preflightPath)) {
            throw "Candidate binding self-test did not preserve the exact preflight domain."
        }
        $v2Hash = [String]::new([char]"a", 64)
        $v2Source = [String]::new([char]"b", 40)
        $v2Names = @(
            "local-browser-bridge-v0.12.12-windows-x86_64.exe",
            "local-computer-helper-v0.12.12-windows-x86_64.exe",
            "local-browser-bridge-v0.12.12-macos-universal.tar.gz",
            "local-browser-bridge-extension-v0.12.12.zip"
        )
        $v2Candidate = [ordered]@{
            version = "0.12.12"
            finalSha = $v2Source
            checksumManifest = [ordered]@{
                sha256 = $v2Hash
                canonicalNamesInOrder = $v2Names
            }
            server = [ordered]@{ sha256 = $v2Hash }
            computerHelper = [ordered]@{ sha256 = $v2Hash }
            extension = [ordered]@{ sha256 = $v2Hash }
        }
        $v2Assets = @()
        foreach ($name in (@($v2Names) + "SHA256SUMS.txt")) {
            $v2Assets += [ordered]@{ file = $name; bytes = 1; sha256 = $v2Hash }
        }
        $v2ReleaseBinding = [ordered]@{
            productVersion = "0.12.12"
            repository = "flrngel/local-browser-bridge"
            tag = "v0.12.12"
            sourceSha = $v2Source
            tagObjectSha = [String]::new([char]"c", 40)
            workflowRunId = "123456789"
            workflowRunAttempt = "1"
            artifactId = "987654321"
            artifactName = "release-candidate"
            artifactZipBytes = 5
            artifactZipSha256 = $v2Hash
            checksumManifestSha256 = $v2Hash
            attestationInvocationUri = "https://github.com/flrngel/local-browser-bridge/actions/runs/123456789/attempts/1"
            attestedAssetCount = 5
            githubHostedRunner = $true
            assets = $v2Assets
        }
        Assert-ReleaseCandidateBindingDomain $v2ReleaseBinding $v2Candidate `
            "self-test releaseCandidateBinding"
        $mismatchedReleaseBinding = $v2ReleaseBinding | ConvertTo-Json -Depth 10 -Compress | ConvertFrom-Json
        $mismatchedReleaseBinding.workflowRunAttempt = "2"
        $attemptMismatchRejected = $false
        try {
            Assert-ReleaseCandidateBindingDomain $mismatchedReleaseBinding $v2Candidate `
                "self-test mismatched releaseCandidateBinding"
        }
        catch { $attemptMismatchRejected = $true }
        if (-not $attemptMismatchRejected) {
            throw "Candidate binding accepted a mismatched release workflow attempt."
        }
        $persisted = [IO.File]::ReadAllText($preflightPath, $script:Utf8NoBom) + [IO.File]::ReadAllText($postflightPath, $script:Utf8NoBom)
        if ($persisted.Contains($root)) {
            throw "Candidate binding self-test persisted a filesystem path."
        }

        $canonicalLines = @($manifestText.Substring(0, $manifestText.Length - 1).Split("`n"))
        $uppercaseHashLine = $canonicalLines[0].Substring(0, 64).ToUpperInvariant() + $canonicalLines[0].Substring(64)
        $negativeIndex = 0
        foreach ($badText in @(
            (($canonicalLines[1], $canonicalLines[0], $canonicalLines[2], $canonicalLines[3]) -join "`n") + "`n",
            ($canonicalLines -join "`r`n") + "`r`n",
            (($uppercaseHashLine, $canonicalLines[1], $canonicalLines[2], $canonicalLines[3]) -join "`n") + "`n",
            $manifestText.Replace("  ", " "),
            (($canonicalLines[0], $canonicalLines[1], $canonicalLines[2]) -join "`n") + "`n",
            $manifestText + $canonicalLines[0] + "`n",
            $manifestText.Substring(0, $manifestText.Length - 1),
            $manifestText.Replace($assetNames[0], "renamed-server.exe")
        )) {
            $negativeIndex += 1
            $badDirectory = [IO.Path]::Combine($root, "bad-manifest-$negativeIndex")
            [IO.Directory]::CreateDirectory($badDirectory) | Out-Null
            $badManifest = [IO.Path]::Combine($badDirectory, "SHA256SUMS.txt")
            [IO.File]::WriteAllText($badManifest, $badText, $script:AsciiStrict)
            $badOutput = [IO.Path]::Combine($root, "bad-manifest-$negativeIndex.json")
            $refused = $false
            try {
                & $PSCommandPath -Mode Preflight -Version $testVersion -FinalSha $testSha -Repository $repositoryPath `
                    -ChecksumManifest $badManifest -ChecksumManifestSha256 (Get-Sha256 $badManifest) `
                    -ServerExecutable ([IO.Path]::Combine($releasePath, $assetNames[0])) -ExtensionZip $zipPath `
                    -ExtractedExtension $extractedPath -OutputRecord $badOutput | Out-Null
            }
            catch { $refused = $true }
            if (-not $refused -or [IO.File]::Exists($badOutput)) {
                throw "Candidate binding accepted a noncanonical checksum manifest."
            }
        }

        $ignoredFixture = [IO.Path]::Combine($repositoryPath, "ignored.tmp")
        [IO.File]::WriteAllText($ignoredFixture, "ignored acceptance residue", $script:Utf8NoBom)
        $ignoredOutput = [IO.Path]::Combine($root, "ignored-checkout.json")
        $ignoredRefused = $false
        try {
            & $PSCommandPath -Mode Preflight -Version $testVersion -FinalSha $testSha -Repository $repositoryPath `
                -ChecksumManifest $manifestPath -ChecksumManifestSha256 $manifestSha `
                -ServerExecutable ([IO.Path]::Combine($releasePath, $assetNames[0])) -ExtensionZip $zipPath `
                -ExtractedExtension $extractedPath -OutputRecord $ignoredOutput | Out-Null
        }
        catch { $ignoredRefused = $true }
        if (-not $ignoredRefused -or [IO.File]::Exists($ignoredOutput)) {
            throw "Candidate binding accepted an ignored checkout file."
        }

        $manifestFixturePath = [IO.Path]::Combine($repositoryPath, "extension", "manifest.json")
        $libraryFixturePath = [IO.Path]::Combine($repositoryPath, "extension", "lib.js")
        $manifestFixtureText = $script:Utf8NoBom.GetString([IO.File]::ReadAllBytes($manifestFixturePath))
        $manifestMutations = @(
            [pscustomobject]@{ Search = '"tabGroups"'; Replace = '"bookmarks"'; Label = "permissions" },
            [pscustomobject]@{ Search = '"js":["stop-guard.js"]'; Replace = '"js":["content.js"]'; Label = "early Stop guard" },
            [pscustomobject]@{ Search = '"run_at":"document_start"'; Replace = '"run_at":"document_idle"'; Label = "early run time" },
            [pscustomobject]@{ Search = '"service_worker":"background.js","type":"module"'; Replace = '"service_worker":"background.js","type":"classic"'; Label = "background" },
            [pscustomobject]@{ Search = '"default_popup":"popup.html"'; Replace = '"default_popup":"background.js"'; Label = "action" },
            [pscustomobject]@{ Search = "script-src 'self'; object-src 'none'"; Replace = "script-src 'self' 'unsafe-eval'; object-src 'none'"; Label = "content security policy" },
            [pscustomobject]@{ Search = '"type":"module"'; Replace = '"type":"module","unexpected":true'; Label = "unexpected nested field" }
        )
        foreach ($mutation in $manifestMutations) {
            $mutatedText = $manifestFixtureText.Replace($mutation.Search, $mutation.Replace)
            if ($mutatedText -ceq $manifestFixtureText) {
                throw "Candidate binding self-test could not construct the manifest mutation."
            }
            $refused = $false
            try {
                [void](Read-ExtensionVersion ($script:Utf8NoBom.GetBytes($mutatedText)) ([IO.File]::ReadAllBytes($libraryFixturePath)) $testVersion)
            }
            catch { $refused = $true }
            if (-not $refused) {
                throw "Candidate binding accepted noncanonical manifest $($mutation.Label)."
            }
        }
        $cleanupProbePath = [IO.Path]::Combine($root, "cleanup-read-only.probe")
        [IO.File]::WriteAllText($cleanupProbePath, "self-test cleanup probe", $script:Utf8NoBom)
        [IO.File]::SetAttributes(
            $cleanupProbePath,
            ([IO.File]::GetAttributes($cleanupProbePath) -bor [IO.FileAttributes]::ReadOnly)
        )
    }
    finally {
        if ($fixtureOwned) {
            Remove-ExactSelfTestDirectory $root
        }
    }
    Write-Output "Browser candidate binding self-test passed."
}

Initialize-TrustedGitExecutable

switch ($Mode) {
    "Preflight" { Invoke-Preflight }
    "Postflight" { Invoke-Postflight }
    "SelfTest" { Invoke-SelfTest }
}
