#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Run", "SelfTest")]
    [string]$Mode,
    [string]$PreflightRecord,
    [string]$ApiMatrixRunner,
    [string]$ApiMatrixRecord,
    [string]$ServerExecutable,
    [string]$HelperExecutable,
    [string]$ExtensionDirectory,
    [string]$RawScreenshotDirectory,
    [string]$OperatorExchangeDirectory,
    [string]$ScopedApprovalRecord,
    [string]$ExecutorSessionRef,
    [string]$ExpectedOrchestratorSessionRef,
    [string]$OutputRecord,
    [ValidateRange(60, 1800)]
    [int]$OperatorResponseTimeoutSeconds = 900,
    [ValidateRange(1, 65535)]
    [int]$Port = 17373
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$script:Utf8NoBom = [Text.UTF8Encoding]::new($false, $true)
$script:Version = "0.12.30"

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

function Get-DirectoryAccessControlPortable([string]$Path) {
    $Sections = [Security.AccessControl.AccessControlSections]::Access -bor
        [Security.AccessControl.AccessControlSections]::Owner
    if ($PSVersionTable.PSEdition -ceq "Core") {
        return [IO.FileSystemAclExtensions]::GetAccessControl(
            [IO.DirectoryInfo]::new($Path), $Sections
        )
    }
    return [IO.Directory]::GetAccessControl($Path, $Sections)
}

$script:Source = "local-browser-bridge-computer-helper-via-loopback-api"
$script:OperatorExchange = $null
$script:OperatorExchangeArtifacts = New-Object Collections.Generic.List[string]
$script:OperatorResponseReservations = New-Object Collections.Generic.List[object]
$script:OperatorExpectedTransientArtifacts = New-Object Collections.Generic.List[string]
$script:ServerProcess = $null
$script:ExpectedOrchestratorRef = $null
$script:CoveredApprovalActions = @(
    "conditional-developer-mode-change",
    "load-and-run-exact-unpacked-candidate",
    "conditional-full-access-change",
    "save-ephemeral-loopback-credential",
    "clear-ephemeral-loopback-credential",
    "remove-exact-test-owned-extension",
    "restore-captured-browser-settings",
    "failure-rollback"
)
$script:Screenshots = [ordered]@{
    "extension-loaded" = "browser-01-extension-loaded.raw.png"
    "api-action-result" = "browser-02-api-action-result.raw.png"
    "computer-share-action" = "browser-03-computer-share-action.raw.png"
    "stop-paused" = "browser-04-stop-paused.raw.png"
    "cancel-paused" = "browser-05-cancel-paused.raw.png"
    "post-handback-resume" = "browser-06-post-handback-resume.raw.png"
}
$script:ExtensionFiles = @(
    "background.js", "content.js", "dom-core.js", "frame-agent.js", "lib.js",
    "manifest.json", "popup.css", "popup.html", "popup.js", "stop-guard.js", "LICENSE"
)
$script:EpochNames = @(
    "existing-chrome-bootstrap", "dedicated-chrome-extensions", "native-load-picker",
    "dedicated-chrome-installed", "extension-popup-setup", "dedicated-chrome-demo",
    "stop-paused-popup", "stop-recovered-demo", "cancel-paused-popup", "cancel-recovered-demo",
    "extension-popup-cleanup", "cleanup-chrome-extensions", "dedicated-chrome-close"
)
$script:EpochSurfaces = @(
    "chrome-window", "chrome-window", "native-file-picker", "chrome-window",
    "extension-popup", "chrome-window", "extension-popup", "chrome-window", "extension-popup",
    "chrome-window", "extension-popup", "chrome-window", "chrome-window"
)
$script:ActionNames = @(
    "dedicated-window-created", "chrome-extensions-navigated", "developer-mode-ready",
    "load-unpacked-clicked", "native-picker-completed", "candidate-card-verified",
    "extension-popup-opened", "full-access-ready", "popup-token-saved",
    "extension-proof-revealed", "browser-api-result-revealed", "computer-demo-clicked",
    "in-page-stop-clicked", "stop-popup-opened", "stop-resume-clicked", "stop-recovery-verified",
    "chrome-native-cancel-clicked", "cancel-popup-opened", "cancel-resume-clicked", "cancel-recovery-verified",
    "cleanup-popup-opened",
    "token-clear-initiated", "token-clear-confirmed", "full-access-restored",
    "test-card-removed", "developer-mode-restored", "test-window-closed"
)

function Resolve-OrdinaryFile {
    param([string]$Path, [string]$Label)
    if ([String]::IsNullOrWhiteSpace($Path)) { throw "$Label is required." }
    $full = [IO.Path]::GetFullPath($Path)
    if (-not [IO.File]::Exists($full)) { throw "$Label does not exist." }
    if ([IO.FileInfo]::new($full).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "$Label must not be a reparse point."
    }
    Assert-NoReparseAncestorChain $full $Label
    return $full
}

function Resolve-OrdinaryDirectory {
    param([string]$Path, [string]$Label)
    if ([String]::IsNullOrWhiteSpace($Path)) { throw "$Label is required." }
    $full = [IO.Path]::GetFullPath($Path)
    if (-not [IO.Directory]::Exists($full)) { throw "$Label does not exist." }
    if ([IO.DirectoryInfo]::new($full).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "$Label must not be a reparse point."
    }
    Assert-NoReparseAncestorChain $full $Label
    return $full
}

function Assert-PrivateOperatorExchangeDirectory {
    param([string]$Path)
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        if ($null -eq $identity -or $null -eq $identity.User) {
            throw "The current Windows identity is unavailable for operator-exchange ACL validation."
        }
        $acl = Get-DirectoryAccessControlPortable ([IO.Path]::GetFullPath($Path))
        $owner = $acl.GetOwner([Security.Principal.SecurityIdentifier])
        $rules = @($acl.GetAccessRules(
            $true, $true, [Security.Principal.SecurityIdentifier]
        ))
        $currentSid = $identity.User.Value
        $currentFullControl = @($rules | Where-Object {
            $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
            $_.IdentityReference.Value -ceq $currentSid -and -not $_.IsInherited -and
            ($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -eq
                [Security.AccessControl.FileSystemRights]::FullControl
        })
        if ($owner.Value -cne $currentSid -or -not $acl.AreAccessRulesProtected -or
            $rules.Count -lt 1 -or $currentFullControl.Count -lt 1 -or
            @($rules | Where-Object {
                $_.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
                $_.IdentityReference.Value -cne $currentSid -or $_.IsInherited
            }).Count -ne 0) {
            throw "OperatorExchangeDirectory must be owned by and grant only explicit FullControl to the current executor."
        }
    }
    finally { if ($null -ne $identity) { $identity.Dispose() } }
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

function Resolve-NewJson {
    param([string]$Path)
    if ([String]::IsNullOrWhiteSpace($Path)) { throw "OutputRecord is required." }
    $full = [IO.Path]::GetFullPath($Path)
    if ([IO.Path]::GetExtension($full) -cne ".json" -or
        [IO.File]::Exists($full) -or [IO.Directory]::Exists($full)) {
        throw "OutputRecord must be a new JSON file."
    }
    $parent = [IO.Path]::GetDirectoryName($full)
    if (-not [IO.Directory]::Exists($parent) -or
        ([IO.DirectoryInfo]::new($parent).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "OutputRecord parent must be an existing ordinary directory."
    }
    Assert-NoReparseAncestorChain $parent "OutputRecord parent"
    return $full
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

function Read-StableJsonWithDigest {
    param([string]$Path, [string]$Label)
    $item = [IO.FileInfo]::new([IO.Path]::GetFullPath($Path))
    if (-not $item.Exists -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
        $item.Length -le 0 -or $item.Length -gt 4MB) {
        throw "$Label must be an ordinary non-empty JSON file within the size limit."
    }
    Assert-NoReparseAncestorChain $item.FullName $Label
    $stream = [IO.File]::Open(
        $item.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None
    )
    $bytes = $null
    try {
        if ($stream.Length -ne $item.Length -or $stream.Length -gt [int]::MaxValue) {
            throw "$Label changed before its stable read."
        }
        $bytes = New-Object byte[] ([int]$stream.Length)
        $offset = 0
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -le 0) { throw "$Label ended during its stable read." }
            $offset += $read
        }
        if ($stream.Position -ne $stream.Length) { throw "$Label was not read exactly once." }
        try { $value = ConvertFrom-JsonPreservingStrings ($script:Utf8NoBom.GetString($bytes)) }
        catch { throw "$Label is not strict UTF-8 JSON." }
        return [pscustomobject]@{
            Value = $value
            Sha256 = Get-BytesSha256 $bytes
            Bytes = $bytes.Length
        }
    }
    finally {
        $stream.Dispose()
        if ($null -ne $bytes) { [Array]::Clear($bytes, 0, $bytes.Length) }
    }
}

function Assert-CanonicalCompactJsonResponse {
    param([object]$Stable, [string]$Label)
    $canonicalBytes = $script:Utf8NoBom.GetBytes(
        (($Stable.Value | ConvertTo-Json -Depth 30 -Compress) + "`n")
    )
    try {
        if ($canonicalBytes.Length -ne [int64]$Stable.Bytes -or
            (Get-BytesSha256 $canonicalBytes) -cne [string]$Stable.Sha256) {
            throw "$Label must be canonical compact UTF-8 JSON followed by one LF; duplicate, case-colliding, reordered, or trailing data is forbidden."
        }
    }
    finally { [Array]::Clear($canonicalBytes, 0, $canonicalBytes.Length) }
}

function Get-Sha256 {
    param([string]$Path)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { return ([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace("-", "").ToLowerInvariant() }
        finally { $hasher.Dispose() }
    }
    finally { $stream.Dispose() }
}

function Get-ExactExtensionPayloadDigest {
    param([string]$DirectoryPath, [object[]]$ExpectedInventory, [string]$ExpectedCombinedSha256)
    $entries = @([IO.DirectoryInfo]::new($DirectoryPath).GetFileSystemInfos())
    $actualNames = @($entries | ForEach-Object { $_.Name } | Sort-Object)
    $expectedNames = @($script:ExtensionFiles | Sort-Object)
    if ($entries.Count -ne $script:ExtensionFiles.Count -or
        ($actualNames -join "`n") -cne ($expectedNames -join "`n")) {
        throw "ExtensionDirectory does not contain the exact eleven-file candidate inventory."
    }
    if ($ExpectedInventory.Count -ne $script:ExtensionFiles.Count) {
        throw "PreflightRecord does not contain the exact extracted extension inventory."
    }
    $canonical = ""
    for ($index = 0; $index -lt $script:ExtensionFiles.Count; $index += 1) {
        $name = $script:ExtensionFiles[$index]
        $entry = $entries | Where-Object { $_.Name -ceq $name } | Select-Object -First 1
        if ($entry -isnot [IO.FileInfo] -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            $entry.Length -le 0 -or $entry.Length -gt 5MB) {
            throw "ExtensionDirectory contains a linked, non-file, empty, or oversized candidate entry."
        }
        $expected = $ExpectedInventory[$index]
        $sha256 = Get-Sha256 $entry.FullName
        if ($expected.name -cne $name -or [int64]$expected.bytes -ne $entry.Length -or
            $expected.sha256 -cne $sha256) {
            throw "ExtensionDirectory differs from the exact preflight payload."
        }
        $canonical += "$sha256  $name`n"
    }
    $combined = Get-TextSha256 $canonical
    if ($combined -cne $ExpectedCombinedSha256) {
        throw "ExtensionDirectory combined payload digest differs from preflight."
    }
    return $combined
}

function Assert-ExactKeys {
    param([object]$Object, [string[]]$Expected, [string]$Label)
    $actual = @($Object.PSObject.Properties.Name)
    if (($actual -join "`n") -cne ($Expected -join "`n")) {
        throw "$Label contains missing, unexpected, or reordered fields."
    }
}

function Assert-Hex {
    param([object]$Value, [int]$Length, [string]$Label)
    if ($Value -isnot [string] -or -not [regex]::IsMatch($Value, "^[0-9a-f]{$Length}$")) {
        throw "$Label must be lowercase hexadecimal."
    }
}

function Read-StablePngWithDigest {
    param([string]$Path, [string]$Label)
    $item = [IO.FileInfo]::new([IO.Path]::GetFullPath($Path))
    if (-not $item.Exists -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
        $item.Length -lt 24 -or $item.Length -gt 20MB) {
        throw "$Label must be an ordinary bounded PNG file."
    }
    Assert-NoReparseAncestorChain $item.FullName $Label
    $stream = [IO.File]::Open(
        $item.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None
    )
    $bytes = $null
    try {
        if ($stream.Length -ne $item.Length -or $stream.Length -gt [int]::MaxValue) {
            throw "$Label changed before its stable read."
        }
        $bytes = New-Object byte[] ([int]$stream.Length)
        $offset = 0
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -le 0) { throw "$Label ended during its stable read." }
            $offset += $read
        }
        if ($stream.Position -ne $stream.Length -or
            ([BitConverter]::ToString($bytes, 0, 8)) -cne "89-50-4E-47-0D-0A-1A-0A" -or
            [Text.Encoding]::ASCII.GetString($bytes, 12, 4) -cne "IHDR") {
            throw "$Label is not a canonical PNG."
        }
        $width = ([uint32]$bytes[16] -shl 24) -bor ([uint32]$bytes[17] -shl 16) -bor
            ([uint32]$bytes[18] -shl 8) -bor [uint32]$bytes[19]
        $height = ([uint32]$bytes[20] -shl 24) -bor ([uint32]$bytes[21] -shl 16) -bor
            ([uint32]$bytes[22] -shl 8) -bor [uint32]$bytes[23]
        if ($width -lt 120 -or $height -lt 32 -or $width -gt 8192 -or $height -gt 8192 -or
            ([uint64]$width * [uint64]$height) -gt 50MB) {
            throw "$Label dimensions are invalid."
        }
        return [pscustomobject]@{
            Bytes = $bytes.Length
            Sha256 = Get-BytesSha256 $bytes
            Width = [int64]$width
            Height = [int64]$height
        }
    }
    finally {
        $stream.Dispose()
        if ($null -ne $bytes) { [Array]::Clear($bytes, 0, $bytes.Length) }
    }
}

function Assert-UnchangedOperatorFrame {
    param([string]$Path, [object]$Expected, [string]$Label)
    $observed = Read-StablePngWithDigest $Path $Label
    if ($observed.Sha256 -cne [string]$Expected.sha256 -or
        $observed.Bytes -ne [int64]$Expected.bytes -or
        $observed.Width -ne [int64]$Expected.width -or
        $observed.Height -ne [int64]$Expected.height) {
        throw "$Label changed before its digest-bound response was accepted."
    }
    return $observed
}

function Remove-CanonicalRawScreenshots {
    param([string]$DirectoryPath)
    if (-not [IO.Directory]::Exists($DirectoryPath)) { return }
    $directory = [IO.DirectoryInfo]::new([IO.Path]::GetFullPath($DirectoryPath))
    if ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Raw screenshot cleanup refused a reparse-point directory."
    }
    $errors = New-Object Collections.Generic.List[string]
    foreach ($name in $script:Screenshots.Values) {
        $path = [IO.Path]::Combine($directory.FullName, $name)
        try {
            if ([IO.Directory]::Exists($path)) {
                throw "the canonical raw screenshot path is a directory"
            }
            if ([IO.File]::Exists($path)) {
                $item = [IO.FileInfo]::new($path)
                if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                    throw "the canonical raw screenshot is a reparse point"
                }
                [IO.File]::Delete($path)
            }
        }
        catch { $errors.Add("${name}: $($_.Exception.Message)") }
    }
    if ($errors.Count -ne 0) {
        throw "Canonical raw screenshot cleanup was incomplete: $($errors -join '; ')"
    }
}

function Get-CandidateBinding {
    param([object]$Preflight, [string]$PreflightSha256)
    if ($Preflight.phase -cne "preflight" -or $Preflight.passed -ne $true -or
        $Preflight.candidate.version -cne $script:Version) {
        throw "PreflightRecord is not a passing v0.12.30 preflight."
    }
    Assert-ReleaseCandidateBinding $Preflight.releaseCandidateBinding $Preflight.candidate
    $script:ReleaseCandidateBinding = $Preflight.releaseCandidateBinding
    foreach ($value in @(
        $Preflight.runNonce, $PreflightSha256, $Preflight.candidate.checksumManifest.sha256,
        $Preflight.candidate.server.sha256, $Preflight.candidate.computerHelper.sha256,
        $Preflight.candidate.extension.sha256, $Preflight.candidate.extension.combinedPayloadSha256
    )) { Assert-Hex $value 64 "candidate binding digest" }
    Assert-Hex $Preflight.candidate.finalSha 40 "candidate FINAL_SHA"
    return [ordered]@{
        runNonce = [string]$Preflight.runNonce
        preflightRecordSha256 = $PreflightSha256
        finalSha = [string]$Preflight.candidate.finalSha
        checksumManifestSha256 = [string]$Preflight.candidate.checksumManifest.sha256
        serverSha256 = [string]$Preflight.candidate.server.sha256
        computerHelperSha256 = [string]$Preflight.candidate.computerHelper.sha256
        extensionZipSha256 = [string]$Preflight.candidate.extension.sha256
        extractedPayloadSha256 = [string]$Preflight.candidate.extension.combinedPayloadSha256
    }
}

function Assert-ReleaseCandidateBinding {
    param([object]$Binding, [object]$Candidate)
    Assert-ExactKeys $Binding @(
        "schemaVersion", "version", "releaseTag", "repository", "sourceSha",
        "workflowRunId", "workflowRunAttempt", "workflowEvent", "workflowRef", "workflowPath",
        "artifactId", "artifactName",
        "artifactZipBytes", "artifactZipSha256", "checksumManifestSha256",
        "attestationInvocationUri", "attestedAssetCount", "githubHostedRunner", "assets"
    ) "releaseCandidateBinding"
    if ($Binding.schemaVersion -ne 3 -or
        $Binding.version -cne $script:Version -or
        $Binding.releaseTag -cne "v$($script:Version)" -or
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
        throw "releaseCandidateBinding does not bind the exact release workflow attempt."
    }
    foreach ($asset in @($Binding.assets)) {
        Assert-ExactKeys $asset @("file", "bytes", "sha256") "releaseCandidateBinding asset"
        if ($asset.bytes -isnot [ValueType] -or [int64]$asset.bytes -le 0 -or
            [string]$asset.sha256 -cnotmatch '^[0-9a-f]{64}$') {
            throw "releaseCandidateBinding asset is invalid."
        }
    }
}

function Get-TextSha256 {
    param([string]$Value)
    $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { return ([BitConverter]::ToString($hasher.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant() }
        finally { $hasher.Dispose() }
    }
    finally { [Array]::Clear($bytes, 0, $bytes.Length) }
}

function Format-CanonicalUtc {
    param([DateTimeOffset]$Value)
    return $Value.UtcDateTime.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
}

function Assert-FreshCanonicalResponseTimestamp {
    param([object]$Value, [DateTimeOffset]$CreatedAt, [DateTimeOffset]$ExpiresAt, [string]$Label)
    $parsed = [DateTimeOffset]::MinValue
    if ($Value -isnot [string] -or
        [string]$Value -cnotmatch '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{7}Z$' -or
        -not [DateTimeOffset]::TryParseExact(
            [string]$Value, "o", [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind, [ref]$parsed
        ) -or $parsed -lt $CreatedAt -or $parsed -gt $ExpiresAt) {
        throw "$Label is stale, premature, noncanonical, or not Z-suffixed UTC."
    }
    return $parsed
}

function Get-OpaqueRef {
    param([string]$Domain, [string]$RawValue)
    if ([String]::IsNullOrWhiteSpace($RawValue)) { throw "A raw value was missing for $Domain binding." }
    return Get-TextSha256 "$($script:Binding.runNonce)`n$Domain`n$RawValue"
}

function Write-CreateOnceJson {
    param([string]$Path, [object]$Value, [string]$Label, [ref]$Sha256Out)
    if ([IO.File]::Exists($Path) -or [IO.Directory]::Exists($Path)) {
        throw "$Label already exists; operator exchange artifacts are create-once."
    }
    $temporary = "$Path.new"
    if ([IO.File]::Exists($temporary) -or [IO.Directory]::Exists($temporary)) {
        throw "$Label has a stale temporary artifact."
    }
    $bytes = $null
    try {
        $bytes = $script:Utf8NoBom.GetBytes((($Value | ConvertTo-Json -Depth 30) + "`n"))
        $stream = [IO.File]::Open(
            $temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
        )
        try {
            $stream.Write($bytes, 0, $bytes.Length)
            $stream.Flush($true)
        }
        finally { $stream.Dispose() }
        [IO.File]::Move($temporary, $Path)
        if ($null -ne $Sha256Out) { $Sha256Out.Value = Get-BytesSha256 $bytes }
    }
    finally {
        if ($null -ne $bytes) { [Array]::Clear($bytes, 0, $bytes.Length) }
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
    }
}

function Register-OperatorExchangeArtifact {
    param([string]$Path)
    $full = [IO.Path]::GetFullPath($Path)
    $prefix = [IO.Path]::GetFullPath($script:OperatorExchange).TrimEnd('\') + '\'
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Operator exchange artifact registration escaped the exact private directory."
    }
    $name = [IO.Path]::GetFileName($full)
    if ($script:OperatorExchangeArtifacts.Contains($name)) {
        throw "Operator exchange artifact registration was duplicated."
    }
    $script:OperatorExchangeArtifacts.Add($name)
}

function Register-ExpectedOperatorTransientArtifact {
    param([string]$Path)
    $full = [IO.Path]::GetFullPath($Path)
    $prefix = [IO.Path]::GetFullPath($script:OperatorExchange).TrimEnd('\') + '\'
    $name = [IO.Path]::GetFileName($full)
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or
        $name -notmatch '^response-[0-9a-f]{32}(?:\.claimed)?\.json(?:\.new)?$' -or
        $script:OperatorExchangeArtifacts.Contains($name) -or
        $script:OperatorExpectedTransientArtifacts.Contains($name)) {
        throw "Expected operator transient registration is invalid or duplicated."
    }
    $script:OperatorExpectedTransientArtifacts.Add($name)
}

function Complete-ExpectedOperatorTransientArtifacts {
    param([string[]]$Paths)
    foreach ($path in $Paths) {
        if (-not $script:OperatorExpectedTransientArtifacts.Remove(
            [IO.Path]::GetFileName([IO.Path]::GetFullPath($path))
        )) {
            throw "An expected operator transient artifact was not registered."
        }
    }
}

function Remove-OperatorExchangeArtifact {
    param([string]$Path)
    $full = [IO.Path]::GetFullPath($Path)
    $prefix = [IO.Path]::GetFullPath($script:OperatorExchange).TrimEnd('\') + '\'
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or
        -not [IO.File]::Exists($full)) {
        throw "Operator exchange cleanup refused an unowned artifact."
    }
    $item = [IO.FileInfo]::new($full)
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Operator exchange cleanup refused a reparse point."
    }
    [IO.File]::Delete($full)
    if (-not $script:OperatorExchangeArtifacts.Remove([IO.Path]::GetFileName($full))) {
        throw "Operator exchange cleanup encountered an unregistered artifact."
    }
}

function Remove-OperatorExchangeScratch {
    if ([String]::IsNullOrWhiteSpace([string]$script:OperatorExchange) -or
        -not [IO.Directory]::Exists($script:OperatorExchange)) { return }
    $directory = [IO.DirectoryInfo]::new($script:OperatorExchange)
    if ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Operator exchange cleanup refused a reparse-point directory."
    }
    $entries = @($directory.GetFileSystemInfos())
    $actualNames = @($entries | ForEach-Object { $_.Name } | Sort-Object)
    $registeredNames = @($script:OperatorExchangeArtifacts | Sort-Object)
    $transientNames = @($script:OperatorExpectedTransientArtifacts | Sort-Object)
    $allowedNames = @($registeredNames + $transientNames | Sort-Object -Unique)
    $inventoryMatches = @($actualNames | Where-Object { $allowedNames -cnotcontains $_ }).Count -eq 0 -and
        @($registeredNames | Where-Object { $actualNames -cnotcontains $_ }).Count -eq 0
    $reservationsValid = $true
    try {
        foreach ($held in $script:OperatorResponseReservations) {
            $entry = $entries | Where-Object { $_.Name -ceq $held.Name } | Select-Object -First 1
            if ($entry -isnot [IO.FileInfo] -or
                ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
                $entry.Length -ne 0 -or $held.Stream.Length -ne 0) {
                $reservationsValid = $false
            }
        }
    }
    finally {
        foreach ($held in $script:OperatorResponseReservations) {
            try { $held.Stream.Dispose() } catch { $reservationsValid = $false }
        }
        $script:OperatorResponseReservations.Clear()
    }
    if (-not $inventoryMatches -or -not $reservationsValid) {
        throw "Operator exchange cleanup found an unregistered, missing, extra, or replaced reservation artifact."
    }
    foreach ($entry in $entries) {
        if ($entry -isnot [IO.FileInfo] -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            $entry.Name -notmatch '^(?:(?:request|response|frame)-[0-9a-f]{32}\.(?:json|png)(?:\.new)?|response-[0-9a-f]{32}\.claimed\.json)$') {
            throw "Operator exchange cleanup found an unexpected or linked artifact."
        }
        $deadline = [DateTimeOffset]::UtcNow.AddSeconds(2)
        do {
            try { [IO.File]::Delete($entry.FullName); break }
            catch [IO.IOException] {
                if ([DateTimeOffset]::UtcNow -ge $deadline) { throw }
                Start-Sleep -Milliseconds 50
            }
        } while ($true)
    }
    if (@($directory.GetFileSystemInfos()).Count -ne 0) {
        throw "Operator exchange cleanup did not reach an exact empty directory."
    }
    [IO.Directory]::Delete($script:OperatorExchange, $false)
    $script:OperatorExchangeArtifacts.Clear()
    $script:OperatorExpectedTransientArtifacts.Clear()
    $script:OperatorExchangeScratchDeleted = $true
}

function Save-LoopbackBinaryCreateOnce {
    param([string]$RelativePath, [string]$OutputPath)
    if ([IO.File]::Exists($OutputPath) -or [IO.Directory]::Exists($OutputPath)) {
        throw "The loopback binary output path is not new."
    }
    $request = [Net.HttpWebRequest]::Create("http://127.0.0.1:$Port$RelativePath")
    $request.Method = "GET"
    $request.Timeout = 25000
    $request.ReadWriteTimeout = 25000
    $request.AllowAutoRedirect = $false
    $request.Headers["Authorization"] = "Bearer $($script:Token)"
    $response = $null
    $input = $null
    $output = $null
    try {
        $response = [Net.HttpWebResponse]$request.GetResponse()
        if ([int]$response.StatusCode -ne 200 -or
            -not ([string]$response.ContentType).StartsWith("image/png", [StringComparison]::OrdinalIgnoreCase) -or
            $response.ContentLength -gt 20MB) {
            throw "The loopback screenshot response was not a bounded PNG."
        }
        $input = $response.GetResponseStream()
        $output = [IO.File]::Open(
            $OutputPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
        )
        $buffer = New-Object byte[] 65536
        $total = 0L
        try {
            while (($read = $input.Read($buffer, 0, $buffer.Length)) -gt 0) {
                $total += $read
                if ($total -gt 20MB) { throw "The loopback screenshot exceeded the byte limit." }
                $output.Write($buffer, 0, $read)
            }
            if ($total -le 0) { throw "The loopback screenshot was empty." }
            $output.Flush($true)
        }
        finally { [Array]::Clear($buffer, 0, $buffer.Length) }
    }
    catch {
        if ($null -ne $output) { $output.Dispose(); $output = $null }
        if ([IO.File]::Exists($OutputPath)) { [IO.File]::Delete($OutputPath) }
        throw
    }
    finally {
        if ($null -ne $output) { $output.Dispose() }
        if ($null -ne $input) { $input.Dispose() }
        if ($null -ne $response) { $response.Close() }
    }
}

function Save-OperatorFrame {
    param([object]$Observation, [string]$RequestId)
    if ($null -eq $Observation -or [String]::IsNullOrWhiteSpace([string]$Observation.frameId) -or
        $null -eq $Observation.PSObject.Properties["screenshotUrl"]) {
        throw "A fresh helper observation with a screenshot endpoint is required for UI interpretation."
    }
    $relative = [string]$Observation.screenshotUrl
    if (-not $relative.StartsWith("/api/computer/screenshot?id=", [StringComparison]::Ordinal) -or
        $relative.Contains("://")) {
        throw "The operator frame endpoint is invalid."
    }
    $name = "frame-$RequestId.png"
    $path = [IO.Path]::Combine($script:OperatorExchange, $name)
    if ([IO.File]::Exists($path) -or [IO.Directory]::Exists($path)) {
        throw "The operator frame path is not new."
    }
    Save-LoopbackBinaryCreateOnce $relative $path
    Register-OperatorExchangeArtifact $path
    $facts = Read-StablePngWithDigest $path "operator frame"
    return [ordered]@{
        name = $name
        bytes = $facts.Bytes
        sha256 = $facts.Sha256
        width = $facts.Width
        height = $facts.Height
        frameRef = Get-OpaqueRef "frame" ([string]$Observation.frameId)
    }
}

function Test-ExactOperatorInteger([object]$Value) {
    return $Value -is [int] -or $Value -is [long]
}

function Assert-OperatorDecision {
    param([string]$Kind, [object]$Decision, [object]$AllowedResponse)
    switch ($Kind) {
        "window-selection" {
            Assert-ExactKeys $Decision @("index") "window-selection decision"
            if (-not (Test-ExactOperatorInteger $Decision.index)) {
                throw "Window-selection index must be an exact integer."
            }
        }
        "ui-point" {
            Assert-ExactKeys $Decision @("x", "y") "UI-point decision"
            if (-not (Test-ExactOperatorInteger $Decision.x) -or
                -not (Test-ExactOperatorInteger $Decision.y)) {
                throw "UI-point coordinates must be exact integers."
            }
        }
        "ui-state" {
            Assert-ExactKeys $Decision @("value") "UI-state decision"
            if ($Decision.value -isnot [string] -or
                @($AllowedResponse.values) -cnotcontains [string]$Decision.value) {
                throw "UI-state decision is outside the request's exact enum."
            }
        }
        "ui-verification" {
            Assert-ExactKeys $Decision @("passed") "UI-verification decision"
            if ($Decision.passed -isnot [bool]) { throw "UI-verification decision must be boolean." }
        }
        "scoped-user-approval" {
            Assert-ExactKeys $Decision @("approved", "approvedBy", "confirmationMode") `
                "scoped-user-approval decision"
            if ($Decision.approved -isnot [bool] -or $Decision.approvedBy -isnot [string] -or
                $Decision.confirmationMode -isnot [string]) {
                throw "Scoped-user-approval decision types are invalid."
            }
            if ($Decision.approved -ne $true -or $Decision.approvedBy -cne "user" -or
                $Decision.confirmationMode -cne "batched-action-time") {
                throw "Scoped-user-approval decision was not an exact approval."
            }
        }
        default { throw "The operator request kind is unsupported." }
    }
}

function Test-UnableOperatorDecision([object]$Decision) {
    if ($null -eq $Decision.PSObject.Properties["unable"]) { return $false }
    Assert-ExactKeys $Decision @("unable") "unable operator decision"
    if ($Decision.unable -isnot [bool] -or $Decision.unable -ne $true) {
        throw 'Unable operator decision must be exactly {"unable":true}.'
    }
    return $true
}

function Assert-OperatorResponseEnvelope {
    param(
        [object]$Response,
        [string]$RequestId,
        [string]$RequestSha256,
        [string]$CandidateBindingSha256,
        [string]$InputDigestSha256,
        [string]$ExpectedResponder,
        [string]$ExecutorSessionRef,
        [AllowNull()][string]$ExistingReviewerSessionRef,
        [string]$ExpectedOrchestratorSessionRef
    )
    Assert-ExactKeys $Response @(
        "schemaVersion", "evidenceType", "requestId", "requestSha256", "candidateBindingSha256",
        "inputDigestSha256", "responderKind", "responderSessionRef", "respondedAtUtc", "decision"
    ) "operator response"
    if ($Response.schemaVersion -ne 1 -or
        $Response.evidenceType -cne "stock-user-chrome-operator-response" -or
        $Response.requestId -cne $RequestId -or $Response.requestSha256 -cne $RequestSha256 -or
        $Response.candidateBindingSha256 -cne $CandidateBindingSha256 -or
        $Response.inputDigestSha256 -cne $InputDigestSha256 -or
        $Response.responderKind -cne $ExpectedResponder) {
        throw "The operator response is not bound to the exact request, input, candidate, or responder role."
    }
    Assert-Hex $Response.responderSessionRef 64 "operator responder session reference"
    if ($Response.responderSessionRef -ceq $ExecutorSessionRef) {
        throw "The operator response reused the executor session."
    }
    if ($ExpectedResponder -ceq "independent-agent") {
        if (-not [String]::IsNullOrWhiteSpace($ExistingReviewerSessionRef) -and
            $Response.responderSessionRef -cne $ExistingReviewerSessionRef) {
            throw "The operator exchange changed independent reviewer sessions mid-run."
        }
    }
    elseif ($ExpectedResponder -ceq "user-via-orchestrator") {
        if ([String]::IsNullOrWhiteSpace($ExpectedOrchestratorSessionRef) -or
            $Response.responderSessionRef -cne $ExpectedOrchestratorSessionRef -or
            (-not [String]::IsNullOrWhiteSpace($ExistingReviewerSessionRef) -and
                $Response.responderSessionRef -ceq $ExistingReviewerSessionRef)) {
            throw "The scoped user approval did not use the exact preflight attestor session."
        }
    }
    else {
        throw "The operator response used an unsupported responder role."
    }
}

function Claim-PublishedOperatorResponse {
    param(
        [string]$ResponsePath,
        [string]$ClaimedResponsePath,
        [DateTimeOffset]$ExpiresAt
    )
    $temporaryResponsePath = "$ResponsePath.new"
    if ([IO.File]::Exists($ClaimedResponsePath) -or
        [IO.Directory]::Exists($ClaimedResponsePath)) {
        throw "The operator response claim path was not new."
    }
    while ([DateTimeOffset]::UtcNow -le $ExpiresAt) {
        if ([IO.Directory]::Exists($temporaryResponsePath) -or
            [IO.Directory]::Exists($ResponsePath) -or
            [IO.Directory]::Exists($ClaimedResponsePath)) {
            throw "An operator response publication path became a directory."
        }
        if ($null -ne $script:ServerProcess -and $script:ServerProcess.HasExited) {
            throw "The candidate server exited while waiting for an operator response."
        }
        if ([IO.File]::Exists($temporaryResponsePath)) {
            $temporaryItem = [IO.FileInfo]::new($temporaryResponsePath)
            if ($temporaryItem.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "The operator response temporary file is a reparse point."
            }
        }
        elseif ([IO.File]::Exists($ResponsePath)) {
            $responseItem = [IO.FileInfo]::new($ResponsePath)
            if ($responseItem.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "The operator response is a reparse point."
            }
            try {
                [IO.File]::Move($ResponsePath, $ClaimedResponsePath)
                foreach ($reservationPath in @($ResponsePath, $temporaryResponsePath)) {
                    $reservation = [IO.File]::Open(
                        $reservationPath, [IO.FileMode]::CreateNew,
                        [IO.FileAccess]::ReadWrite, [IO.FileShare]::None
                    )
                    $reservation.Flush($true)
                    Register-OperatorExchangeArtifact $reservationPath
                    $script:OperatorResponseReservations.Add([pscustomobject]@{
                        Name = [IO.Path]::GetFileName($reservationPath)
                        Stream = $reservation
                    })
                }
                Register-OperatorExchangeArtifact $ClaimedResponsePath
                if ([DateTimeOffset]::UtcNow -gt $ExpiresAt) {
                    throw "The operator response was claimed after request expiry."
                }
                return
            }
            catch [IO.IOException] {
                if ([IO.File]::Exists($ClaimedResponsePath)) { throw }
            }
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Timed out waiting for the atomically published create-once operator response."
}

function Invoke-OperatorExchange {
    param(
        [ValidateSet("window-selection", "ui-point", "ui-state", "ui-verification", "scoped-user-approval")]
        [string]$Kind,
        [string]$ActionName,
        [string]$Instruction,
        [object]$AllowedResponse,
        [AllowNull()][object]$Observation,
        [AllowNull()][object]$Context,
        [ValidateSet("independent-agent", "user-via-orchestrator")]
        [string]$ExpectedResponder = "independent-agent"
    )
    if ([String]::IsNullOrWhiteSpace([string]$script:OperatorExchange) -or
        -not [IO.Directory]::Exists($script:OperatorExchange)) {
        throw "The private operator exchange is unavailable."
    }
    if (($Kind -ceq "scoped-user-approval") -ne ($ExpectedResponder -ceq "user-via-orchestrator")) {
        throw "Only the scoped approval request may use the user-via-orchestrator responder role."
    }
    $frameRequired = $Kind -in @("ui-point", "ui-state", "scoped-user-approval") -or
        ($Kind -ceq "ui-verification" -and
            ($null -eq $Context -or $Context.targetClosed -ne $true))
    if ($frameRequired -and $null -eq $Observation) {
        throw "$Kind requires an exact fresh helper frame."
    }
    if (-not $frameRequired -and $Kind -eq "window-selection" -and $null -ne $Observation) {
        throw "Window selection must bind the exact status inventory rather than a frame."
    }
    $requestId = [Guid]::NewGuid().ToString("N")
    $createdAt = [DateTimeOffset]::UtcNow
    $expiresAt = $createdAt.AddSeconds($OperatorResponseTimeoutSeconds)
    $frame = if ($null -ne $Observation) { Save-OperatorFrame $Observation $requestId } else { $null }
    $inputDigestSha256 = if ($null -ne $frame) {
        [string]$frame.sha256
    }
    elseif ($null -ne $Context -and
        $null -ne $Context.PSObject.Properties["statusResponseSha256"]) {
        [string]$Context.statusResponseSha256
    }
    else {
        throw "$Kind lacks an exact frame or status-response digest."
    }
    Assert-Hex $inputDigestSha256 64 "operator request input digest"
    $request = [ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-operator-request"
        productVersion = $script:Version
        releaseCandidateBinding = $script:ReleaseCandidateBinding
        candidateBinding = $script:Binding
        candidateBindingSha256 = $script:CandidateBindingSha256
        requestId = $requestId
        sequence = $script:OperatorRequestCount + 1
        kind = $Kind
        actionName = $ActionName
        createdAtUtc = Format-CanonicalUtc $createdAt
        expiresAtUtc = Format-CanonicalUtc $expiresAt
        executorSessionRef = $script:ExecutorRef
        inputDigestSha256 = $inputDigestSha256
        frame = $frame
        context = $Context
        allowedResponse = $AllowedResponse
        instruction = $Instruction
    }
    $requestPath = [IO.Path]::Combine($script:OperatorExchange, "request-$requestId.json")
    $responsePath = [IO.Path]::Combine($script:OperatorExchange, "response-$requestId.json")
    $temporaryResponsePath = "$responsePath.new"
    $claimedResponsePath = [IO.Path]::Combine(
        $script:OperatorExchange, "response-$requestId.claimed.json"
    )
    foreach ($newResponseArtifact in @($responsePath, $temporaryResponsePath, $claimedResponsePath)) {
        if ([IO.File]::Exists($newResponseArtifact) -or
            [IO.Directory]::Exists($newResponseArtifact)) {
            throw "The operator response publication paths were not all new."
        }
        Register-ExpectedOperatorTransientArtifact $newResponseArtifact
    }
    $requestSha256 = $null
    Write-CreateOnceJson $requestPath $request "operator request" ([ref]$requestSha256)
    Register-OperatorExchangeArtifact $requestPath
    $script:OperatorRequestCount += 1
    Write-Host "OPERATOR_REQUEST $requestPath"
    Claim-PublishedOperatorResponse $responsePath $claimedResponsePath $expiresAt
    Complete-ExpectedOperatorTransientArtifacts @(
        $responsePath, $temporaryResponsePath, $claimedResponsePath
    )
    $stableResponse = Read-StableJsonWithDigest $claimedResponsePath "operator response"
    Assert-CanonicalCompactJsonResponse $stableResponse "operator response"
    $response = $stableResponse.Value
    Assert-OperatorResponseEnvelope $response $requestId $requestSha256 `
        $script:CandidateBindingSha256 $inputDigestSha256 $ExpectedResponder `
        $script:ExecutorRef $script:ReviewerSessionRef $script:ExpectedOrchestratorRef
    [void](Assert-FreshCanonicalResponseTimestamp $response.respondedAtUtc `
        $createdAt $expiresAt "The operator response timestamp")
    if ($ExpectedResponder -ceq "independent-agent") {
        if ([String]::IsNullOrWhiteSpace([string]$script:ReviewerSessionRef)) {
            $script:ReviewerSessionRef = [string]$response.responderSessionRef
        }
        if ($null -eq $Observation) { $script:StatusDecisionCount += 1 }
        else { $script:FreshFrameDecisionCount += 1 }
    }
    $UnableDecision = Test-UnableOperatorDecision $response.decision
    if (-not $UnableDecision) {
        Assert-OperatorDecision $Kind $response.decision $AllowedResponse
    }
    $stableRequest = Read-StableJsonWithDigest $requestPath "operator request after response"
    if ($stableRequest.Sha256 -cne $requestSha256) {
        throw "The exact operator request changed before its response was accepted."
    }
    if ($null -ne $frame) {
        $framePath = [IO.Path]::Combine($script:OperatorExchange, [string]$frame.name)
        [void](Assert-UnchangedOperatorFrame `
            $framePath $frame "operator frame after response")
    }
    if ($UnableDecision) {
        throw "The independent operator reported that the exact request could not be interpreted safely."
    }
    $responseSha256 = $stableResponse.Sha256
    $script:OperatorResponseChainSha256 = Get-TextSha256 (
        "$($script:OperatorResponseChainSha256)`n$requestSha256`n$responseSha256"
    )
    $decision = $response.decision
    Remove-OperatorExchangeArtifact $requestPath
    Remove-OperatorExchangeArtifact $claimedResponsePath
    if ($null -ne $frame) {
        Remove-OperatorExchangeArtifact ([IO.Path]::Combine($script:OperatorExchange, [string]$frame.name))
    }
    return [pscustomobject]@{
        Decision = $decision
        RequestSha256 = $requestSha256
        ResponseSha256 = $responseSha256
        DecisionRef = Get-TextSha256 "operator-decision`n$requestSha256`n$responseSha256"
        ResponderKind = [string]$response.responderKind
        ResponderSessionRef = [string]$response.responderSessionRef
        CreatedAtUtc = $request.createdAtUtc
        ExpiresAtUtc = $request.expiresAtUtc
        RespondedAtUtc = [string]$response.respondedAtUtc
    }
}

function Invoke-LoopbackJson {
    param([string]$Path, [string]$Method, [object]$Body)
    $parameters = @{
        UseBasicParsing = $true
        Uri = "http://127.0.0.1:$Port$Path"
        Method = $Method
        Headers = @{ Authorization = "Bearer $($script:Token)" }
        TimeoutSec = 25
    }
    if ($null -ne $Body) {
        $parameters.ContentType = "application/json"
        $parameters.Body = $Body | ConvertTo-Json -Depth 20 -Compress
    }
    $response = Invoke-WebRequest @parameters
    if ([int]$response.StatusCode -ne 200) { throw "Loopback request failed." }
    return [pscustomobject]@{
        Status = [int]$response.StatusCode
        Body = ConvertFrom-JsonPreservingStrings $response.Content
        Digest = Get-TextSha256 ([string]$response.Content)
    }
}

function Get-BridgeState {
    return (Invoke-LoopbackJson "/api/state" "Get" $null).Body.state
}

function Wait-BridgeState {
    param([scriptblock]$Predicate, [string]$Label, [int]$Seconds = 20)
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if ($null -ne $script:ServerProcess -and $script:ServerProcess.HasExited) {
            throw "The candidate server exited while waiting for $Label."
        }
        $state = Get-BridgeState
        if (& $Predicate $state) { return $state }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Label."
}

function Invoke-ComputerCommand {
    param([string]$Method, [hashtable]$Params)
    $script:CommandSequence += 1
    $response = Invoke-LoopbackJson "/api/v1/command" "Post" ([ordered]@{
        method = $Method
        params = $Params
        callId = "helper-chain-" + [Guid]::NewGuid().ToString("N")
    })
    if ($null -ne $response.Body.error) {
        throw "Loopback computer command $Method returned a structured error."
    }
    return $response
}

function Get-NormalizedObservationApplication {
    param([object]$Observation)
    if ($null -eq $Observation -or [int64]$Observation.pid -le 0) {
        throw "The computer observation has no valid process identity."
    }
    $app = ([string]$Observation.appName).Trim().ToLowerInvariant()
    $leaf = [IO.Path]::GetFileNameWithoutExtension($app)
    if ($app -in @("google chrome", "google chrome.exe", "chrome", "chrome.exe") -or
        $leaf -ceq "chrome") {
        return "google-chrome"
    }
    throw "The computer observation is not stock Google Chrome UI."
}

function Test-ExactObservationIdentity {
    param([object]$Observation, [string]$WindowId, [int64]$ExpectedPid)
    try {
        return $null -ne $Observation -and
            [string]$Observation.windowId -ceq $WindowId -and
            [int64]$Observation.pid -eq $ExpectedPid -and
            (Get-NormalizedObservationApplication $Observation) -ceq "google-chrome" -and
            -not [String]::IsNullOrWhiteSpace([string]$Observation.frameId)
    }
    catch { return $false }
}

function Test-ExactSharedFrame {
    param(
        [object]$Observation,
        [string]$WindowId,
        [int64]$ExpectedPid,
        [string]$ShareId,
        [int64]$PreviousSourceSequence
    )
    try {
        if (-not (Test-ExactObservationIdentity $Observation $WindowId $ExpectedPid) -or
            $null -eq $Observation.PSObject.Properties["shareId"] -or
            $null -eq $Observation.PSObject.Properties["sourceSequence"] -or
            $null -eq $Observation.PSObject.Properties["share"] -or
            [string]$Observation.shareId -cne $ShareId -or
            [int64]$Observation.sourceSequence -le $PreviousSourceSequence -or
            $Observation.share.active -ne $true -or [string]$Observation.share.id -cne $ShareId) {
            return $false
        }
        return $true
    }
    catch { return $false }
}

function Get-FreshObservation {
    param(
        [string]$WindowId,
        [int64]$ExpectedPid,
        [string]$PreviousFrameId
    )
    $response = Invoke-ComputerCommand "computer.observe" @{ windowId = $WindowId }
    $observation = $response.Body.state.computerObservation
    if (-not (Test-ExactObservationIdentity $observation $WindowId $ExpectedPid) -or
        (-not [String]::IsNullOrWhiteSpace($PreviousFrameId) -and
            [string]$observation.frameId -ceq $PreviousFrameId)) {
        throw "computer.observe did not return a new frame for the exact stock-Chrome window and process."
    }
    return [pscustomobject]@{ Observation = $observation; Response = $response }
}

function Get-FreshSharedFrame {
    param([object]$Context, [string]$Label, [int]$Seconds = 20)
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        $state = Get-BridgeState
        $observation = $state.computerObservation
        if (Test-ExactSharedFrame $observation $Context.WindowId $Context.Pid `
                $Context.RawShareId $Context.LastSourceSequence) {
            $Context.Observation = $observation
            $Context.ObservationCount += 1
            $Context.LastSourceSequence = [int64]$observation.sourceSequence
            $Context.LastFrameRef = Get-OpaqueRef "frame" ([string]$observation.frameId)
            return $observation
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for a new exact-bound streamed helper frame for $Label."
}

function Get-NormalizedApplication {
    param([object]$Window)
    if ($null -eq $Window -or [string]$Window.id -eq "" -or [int64]$Window.pid -le 0 -or
        $Window.minimized -eq $true) {
        throw "The selected helper window has an invalid identity or is minimized."
    }
    $app = ([string]$Window.appName).Trim().ToLowerInvariant()
    $leaf = [IO.Path]::GetFileNameWithoutExtension($app)
    if ($app -in @("google chrome", "google chrome.exe", "chrome", "chrome.exe") -or
        $leaf -ceq "chrome") {
        return "google-chrome"
    }
    throw "The selected helper window is not stock Google Chrome UI."
}

function Get-ExactChromeWindow {
    param([string]$WindowId, [int64]$ExpectedPid = 0)
    $status = Invoke-ComputerCommand "computer.status" @{}
    $matches = @($status.Body.result.windows | Where-Object {
        [string]$_.id -ceq $WindowId -and ($ExpectedPid -eq 0 -or [int64]$_.pid -eq $ExpectedPid)
    })
    if ($matches.Count -ne 1 -or (Get-NormalizedApplication $matches[0]) -cne "google-chrome") {
        throw "The exact owned stock-Chrome window identity is no longer available."
    }
    return [pscustomobject]@{
        WindowId = [string]$matches[0].id
        Pid = [int64]$matches[0].pid
        Application = "google-chrome"
        StatusDigest = $status.Digest
    }
}

function Select-ExactWindow {
    param([string]$Instruction)
    $status = Invoke-ComputerCommand "computer.status" @{}
    $windows = @($status.Body.result.windows)
    if ($windows.Count -eq 0) { throw "The helper reported no selectable windows." }
    $choices = @()
    for ($index = 0; $index -lt $windows.Count; $index += 1) {
        $choices += [ordered]@{
            index = $index
            title = [string]$windows[$index].title
            application = [string]$windows[$index].appName
        }
    }
    $exchange = Invoke-OperatorExchange "window-selection" "select-exact-window" $Instruction `
        ([ordered]@{ type = "window-index"; minimum = 0; maximum = $windows.Count - 1 }) $null `
        ([ordered]@{ statusResponseSha256 = $status.Digest; windows = $choices })
    Assert-ExactKeys $exchange.Decision @("index") "window-selection decision"
    $selection = 0
    if ($exchange.Decision.index -isnot [ValueType] -or
        -not [int]::TryParse([string]$exchange.Decision.index, [ref]$selection) -or
        $selection -lt 0 -or $selection -ge $windows.Count) {
        throw "Exact-window selection was invalid."
    }
    $selected = $windows[$selection]
    return [pscustomobject]@{
        WindowId = [string]$selected.id
        Pid = [int64]$selected.pid
        Application = Get-NormalizedApplication $selected
        StatusDigest = $status.Digest
    }
}

function Get-BaselineChromeWindowBinding {
    $status = Invoke-ComputerCommand "computer.status" @{}
    $refs = New-Object Collections.Generic.List[string]
    foreach ($window in @($status.Body.result.windows)) {
        try {
            if ((Get-NormalizedApplication $window) -ceq "google-chrome") {
                $refs.Add((Get-OpaqueRef "target" "$([string]$window.id)|$([int64]$window.pid)"))
            }
        }
        catch { }
    }
    $ordered = @($refs | Sort-Object -Unique)
    $binding = [pscustomobject]@{
        Count = $ordered.Count
        Digest = Get-TextSha256 ($ordered -join [Environment]::NewLine)
        TargetRefs = $ordered
        StatusDigest = $status.Digest
    }
    $refs.Clear()
    return $binding
}

function Bind-NewDedicatedChromeWindow {
    if ($null -eq $script:BaselineChrome) {
        throw "The baseline Chrome-window set was not captured."
    }
    $status = Invoke-ComputerCommand "computer.status" @{}
    $chrome = New-Object Collections.Generic.List[object]
    foreach ($window in @($status.Body.result.windows)) {
        try {
            if ((Get-NormalizedApplication $window) -ceq "google-chrome") {
                $chrome.Add([pscustomobject]@{
                    Window = $window
                    TargetRef = Get-OpaqueRef "target" "$([string]$window.id)|$([int64]$window.pid)"
                })
            }
        }
        catch { }
    }
    $newChrome = @($chrome | Where-Object { $script:BaselineChrome.TargetRefs -cnotcontains $_.TargetRef })
    $missingBaseline = @($script:BaselineChrome.TargetRefs | Where-Object {
        $chrome.TargetRef -cnotcontains $_
    })
    if ($chrome.Count -ne ($script:BaselineChrome.Count + 1) -or
        $newChrome.Count -ne 1 -or $missingBaseline.Count -ne 0) {
        throw "Control+N did not produce exactly one new stock-Chrome window over the unchanged baseline."
    }
    $selected = $newChrome[0].Window
    $script:DedicatedWindowId = [string]$selected.id
    $script:DedicatedWindowPid = [int64]$selected.pid
    $script:DedicatedTargetRef = [string]$newChrome[0].TargetRef
    $script:DedicatedProcessRef = Get-OpaqueRef "process" ([string]$script:DedicatedWindowPid)
    $script:DedicatedAbsentBeforeCreation = $true
    $script:DedicatedCreatedAsOnlyNewChromeWindow = $true
    $chrome.Clear()
    return [pscustomobject]@{
        WindowId = $script:DedicatedWindowId
        Pid = $script:DedicatedWindowPid
        Application = "google-chrome"
        StatusDigest = $status.Digest
    }
}

function Start-RecordedEpoch {
    param([string]$Name, [string]$Surface, [string]$Instruction)
    if ($script:Epochs.Count -ge $script:EpochNames.Count -or
        $Name -cne $script:EpochNames[$script:Epochs.Count] -or
        $Surface -cne $script:EpochSurfaces[$script:Epochs.Count]) {
        throw "The requested helper epoch is out of canonical order."
    }
    if ($Name -in @(
        "dedicated-chrome-extensions", "dedicated-chrome-installed", "dedicated-chrome-demo",
        "cleanup-chrome-extensions", "dedicated-chrome-close"
    )) {
        Write-Host $Instruction
        $selected = Get-ExactChromeWindow $script:DedicatedWindowId $script:DedicatedWindowPid
    }
    else {
        $selected = Select-ExactWindow $Instruction
    }
    $windowId = $selected.WindowId
    $targetRef = Get-OpaqueRef "target" "$windowId|$($selected.Pid)"
    $processRef = Get-OpaqueRef "process" ([string]$selected.Pid)
    if ($Name -ceq "dedicated-chrome-extensions") {
        if (-not $script:DedicatedCreatedAsOnlyNewChromeWindow -or
            $script:BaselineChrome.TargetRefs -contains $targetRef -or
            $targetRef -cne $script:DedicatedTargetRef -or
            $processRef -cne $script:DedicatedProcessRef) {
            throw "The dedicated Chrome epoch is not the sole helper-created post-baseline window."
        }
    }
    elseif ($Name -in @(
        "dedicated-chrome-installed", "dedicated-chrome-demo",
        "cleanup-chrome-extensions", "dedicated-chrome-close"
    )) {
        if ($targetRef -cne $script:DedicatedTargetRef -or
            $processRef -cne $script:DedicatedProcessRef) {
            throw "A later dedicated Chrome epoch selected a different native window or process."
        }
    }
    elseif ($Name -in @("native-load-picker", "extension-popup-setup", "extension-popup-cleanup")) {
        if ($processRef -cne $script:DedicatedProcessRef) {
            throw "The native picker/popup is not process-linked to the exact dedicated Chrome window."
        }
    }
    if ($Surface -ceq "extension-popup") {
        $script:LastOwnedPopupWindowId = $windowId
        $script:LastOwnedPopupWindowPid = $selected.Pid
    }
    elseif ($Surface -ceq "native-file-picker") {
        $script:NativePickerWindowId = $windowId
        $script:NativePickerWindowPid = $selected.Pid
    }
    $startSequence = $script:CommandSequence + 1
    $start = Invoke-ComputerCommand "computer.share.start" @{ windowId = $windowId; fps = 4 }
    $rawShareId = [string]$start.Body.result.id
    if ($start.Body.result.active -ne $true -or [String]::IsNullOrWhiteSpace($rawShareId)) {
        throw "computer.share.start did not return an exact active share identity."
    }
    $state = Wait-BridgeState {
        param($candidate)
        Test-ExactSharedFrame $candidate.computerObservation $windowId $selected.Pid $rawShareId 0
    } "the first exact-bound streamed frame in $Name"
    $observation = $state.computerObservation
    $context = [pscustomobject]@{
        Name = $Name; Surface = $Surface; WindowId = $windowId; Pid = $selected.Pid
        Application = $selected.Application; ProcessRef = $processRef
        EpochRef = Get-OpaqueRef "epoch-$Name" "$windowId|$rawShareId"
        TargetRef = $targetRef; ShareRef = Get-OpaqueRef "share" $rawShareId
        StartSequence = $startSequence
        RawShareId = $rawShareId; LastSourceSequence = [int64]$observation.sourceSequence
        FirstFrameRef = Get-OpaqueRef "frame" ([string]$observation.frameId)
        LastFrameRef = $null; ObservationCount = 1; Observation = $observation
    }
    $script:ActiveEpoch = $context
    $rawShareId = $null
    return $context
}

function Stop-RecordedEpoch {
    param([object]$Context, [switch]$TargetClosed)
    if (-not $TargetClosed) {
        [void](Get-FreshSharedFrame $Context "the final frame before $($Context.Name) teardown")
    }
    elseif ([String]::IsNullOrWhiteSpace([string]$Context.LastFrameRef)) {
        throw "The closed-target epoch lacks a final fresh frame."
    }
    $stopSequence = $script:CommandSequence + 1
    $stopped = Invoke-ComputerCommand "computer.share.stop" @{}
    if ($stopped.Body.result.active -ne $false) { throw "computer.share.stop did not return active:false." }
    [void](Wait-BridgeState { param($state) $state.computer.share.active -ne $true } "share teardown")
    $script:Epochs += [ordered]@{
        sequence = $script:Epochs.Count + 1; name = $Context.Name; surface = $Context.Surface
        application = $Context.Application; processRef = $Context.ProcessRef
        epochRef = $Context.EpochRef; targetRef = $Context.TargetRef; shareRef = $Context.ShareRef
        selectedExactly = $true; shareStartSequence = $Context.StartSequence
        shareStartSucceeded = $true; firstFreshFrameRef = $Context.FirstFrameRef
        lastFreshFrameRef = $Context.LastFrameRef; freshObservationCount = $Context.ObservationCount
        shareStopSequence = $stopSequence; shareStopSucceeded = $true; rawIdentifiersRetained = $false
    }
    $Context.WindowId = $null
    $Context.Observation = $null
    $Context.RawShareId = $null
    $Context.LastSourceSequence = 0
    $script:ActiveEpoch = $null
}

function Read-ClickParams {
    param([object]$Observation, [string]$Label)
    $exchange = Invoke-OperatorExchange "ui-point" $Label "Select only the named target in this exact fresh helper frame." `
        ([ordered]@{
            type = "point"; minimumX = 0; minimumY = 0
            maximumXExclusive = [double]$Observation.imageWidth
            maximumYExclusive = [double]$Observation.imageHeight
        }) $Observation $null
    Assert-ExactKeys $exchange.Decision @("x", "y") "$Label point decision"
    $x = 0.0; $y = 0.0
    if (-not [double]::TryParse([string]$exchange.Decision.x,
            [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$x) -or
        -not [double]::TryParse([string]$exchange.Decision.y,
            [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$y) -or
        $x -lt 0 -or $y -lt 0 -or $x -ge [double]$Observation.imageWidth -or $y -ge [double]$Observation.imageHeight) {
        throw "Click coordinates were outside the fresh exact-window frame."
    }
    $script:LastOperatorDecisionRef = $exchange.DecisionRef
    return [ordered]@{ x = $x; y = $y; coordinateSpace = "image"; button = "left"; clickCount = 1; durationMs = 80 }
}

function Read-LiveToggleState {
    param([object]$Context, [string]$Label)
    $fresh = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
    $Context.Observation = $fresh.Observation
    $Context.ObservationCount += 1
    $frameRef = Get-OpaqueRef "frame" ([string]$fresh.Observation.frameId)
    $Context.LastFrameRef = $frameRef
    $exchange = Invoke-OperatorExchange "ui-state" "read-$Label" `
        "Interpret the named toggle from this exact fresh helper frame before mutation." `
        ([ordered]@{ type = "enum"; values = @("enabled", "disabled") }) $fresh.Observation $null
    Assert-ExactKeys $exchange.Decision @("value") "$Label state decision"
    $value = ([string]$exchange.Decision.value).Trim().ToLowerInvariant()
    if ($value -cne "enabled" -and $value -cne "disabled") {
        throw "$Label live state must be entered exactly as enabled or disabled."
    }
    return [ordered]@{
        value = $value
        epochRef = $Context.EpochRef
        frameRef = $frameRef
        operatorDecisionRef = $exchange.DecisionRef
        capturedBeforeMutation = $true
    }
}

function Read-LiveSavedTokenState {
    param([object]$Context)
    $fresh = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
    $Context.Observation = $fresh.Observation
    $Context.ObservationCount += 1
    $frameRef = Get-OpaqueRef "frame" ([string]$fresh.Observation.frameId)
    $Context.LastFrameRef = $frameRef
    $exchange = Invoke-OperatorExchange "ui-state" "read-saved-token-state" `
        "Interpret the saved-token state from this exact fresh extension-popup frame." `
        ([ordered]@{ type = "enum"; values = @("unconfigured") }) $fresh.Observation $null
    Assert-ExactKeys $exchange.Decision @("value") "saved-token state decision"
    if ([string]$exchange.Decision.value -cne "unconfigured") {
        throw "The exact new candidate popup was not in its required unconfigured state."
    }
    return [ordered]@{
        configured = $false
        epochRef = $Context.EpochRef
        frameRef = $frameRef
        operatorDecisionRef = $exchange.DecisionRef
        capturedBeforeMutation = $true
    }
}

function Read-LiveCandidateCardState {
    param([object]$Context, [string]$ExpectedState, [string]$Label)
    if ($ExpectedState -notin @("present", "absent")) { throw "Candidate-card expected state is invalid." }
    $fresh = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
    $Context.Observation = $fresh.Observation
    $Context.ObservationCount += 1
    $frameRef = Get-OpaqueRef "frame" ([string]$fresh.Observation.frameId)
    $Context.LastFrameRef = $frameRef
    $exchange = Invoke-OperatorExchange "ui-state" "read-candidate-card-state" $Label `
        ([ordered]@{ type = "enum"; values = @("present", "absent") }) $fresh.Observation $null
    Assert-ExactKeys $exchange.Decision @("value") "candidate-card state decision"
    $actual = ([string]$exchange.Decision.value).Trim().ToLowerInvariant()
    if ($actual -cne $ExpectedState) {
        throw "The exact test-owned candidate card did not match the required $ExpectedState state."
    }
    return [ordered]@{
        present = $ExpectedState -ceq "present"
        epochRef = $Context.EpochRef
        frameRef = $frameRef
        operatorDecisionRef = $exchange.DecisionRef
        verifiedFromFreshLiveUi = $true
    }
}

function Request-ScopedActionTimeApproval {
    param([object]$Context, [object]$Preflight, [object]$InitialDeveloperMode, [object]$InitialCandidateCard)
    if (-not [String]::IsNullOrWhiteSpace([string]$script:ApprovalId)) {
        throw "The scoped action-time approval is single-use and was already requested."
    }
    if ($InitialCandidateCard.present -ne $false -or
        $InitialDeveloperMode.value -notin @("enabled", "disabled")) {
        throw "The scoped approval cannot be requested before fresh initial Chrome state is known."
    }
    $approvalObservation = Get-FreshObservation `
        $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
    $Context.Observation = $approvalObservation.Observation
    $Context.ObservationCount += 1
    $script:ApprovalChallengeFrameRef = Get-OpaqueRef `
        "frame" ([string]$approvalObservation.Observation.frameId)
    $scopeFacts = [ordered]@{
        productVersion = $script:Version
        candidateBindingSha256 = $script:CandidateBindingSha256
        extensionZipSha256 = $script:Binding.extensionZipSha256
        extractedPayloadSha256 = $script:Binding.extractedPayloadSha256
        manifestPermissions = @($Preflight.candidate.extension.permissions)
        manifestHostPermissions = @($Preflight.candidate.extension.hostPermissions)
        dedicatedTargetRef = $script:DedicatedTargetRef
        loopbackEndpoint = "http://127.0.0.1:17373"
        initialDeveloperMode = [string]$InitialDeveloperMode.value
        candidateAbsentBeforeInstall = $true
        coveredActions = @($script:CoveredApprovalActions)
        restoreCapturedState = $true
        failureRollback = $true
    }
    $scopeSha256 = Get-TextSha256 ($scopeFacts | ConvertTo-Json -Depth 12 -Compress)
    $exchange = Invoke-OperatorExchange "scoped-user-approval" "scoped-action-time-approval" `
        "The exact unpacked candidate can read and change browser pages and grants loopback control while temporary Full Access and an ephemeral saved credential are enabled. Approve this exact candidate-bound run, its exact test-owned cleanup, and failure rollback?" `
        ([ordered]@{
            type = "approval"; approvedBy = "user"; confirmationMode = "batched-action-time"
            scopeSha256 = $scopeSha256; coveredActions = @($script:CoveredApprovalActions)
        }) $approvalObservation.Observation $scopeFacts "user-via-orchestrator"
    Assert-ExactKeys $exchange.Decision @("approved", "approvedBy", "confirmationMode") `
        "scoped action-time approval decision"
    if ($exchange.Decision.approved -ne $true -or
        $exchange.Decision.approvedBy -cne "user" -or
        $exchange.Decision.confirmationMode -cne "batched-action-time") {
        throw "The exact candidate-bound action-time approval was not granted by the user."
    }
    $script:ApprovalId = Get-TextSha256 (
        "scoped-action-approval`n$($exchange.RequestSha256)`n$($exchange.ResponseSha256)"
    )
    $script:ApprovalScopeSha256 = $scopeSha256
    $script:ApprovalConfirmedAtUtc = [string]$exchange.RespondedAtUtc
    $script:ApprovalExpiresAtUtc = [string]$exchange.ExpiresAtUtc
    $script:ApprovalRequest = [ordered]@{
        createdAtUtc = [string]$exchange.CreatedAtUtc
        expiresAtUtc = [string]$exchange.ExpiresAtUtc
        challengeFrameRef = $script:ApprovalChallengeFrameRef
        scopeSha256 = $scopeSha256
        coveredActions = @($script:CoveredApprovalActions)
        loopbackOnly = $true
        dedicatedWindowOnly = $true
        restoreCapturedState = $true
        noUnrelatedExtensionMutation = $true
    }
    $script:ApprovalResponse = [ordered]@{
        approvedBy = "user"
        deliveredBy = "user-via-orchestrator"
        orchestratorSessionRef = [string]$exchange.ResponderSessionRef
        confirmationMode = "batched-action-time"
        confirmedAtUtc = [string]$exchange.RespondedAtUtc
        requestSha256 = [string]$exchange.RequestSha256
        singleCandidateRun = $true
    }
}

function Confirm-ApprovalPreDispatchStateUnchanged {
    param([object]$Context, [object]$InitialDeveloperMode, [object]$InitialCandidateCard)
    if ([String]::IsNullOrWhiteSpace([string]$script:ApprovalId) -or
        [String]::IsNullOrWhiteSpace([string]$script:ApprovalChallengeFrameRef)) {
        throw "Post-approval state revalidation requires the exact fresh approval challenge."
    }
    $fresh = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
    $Context.Observation = $fresh.Observation
    $Context.ObservationCount += 1
    $frameRef = Get-OpaqueRef "frame" ([string]$fresh.Observation.frameId)
    if ($frameRef -ceq $script:ApprovalChallengeFrameRef) {
        throw "Post-approval state revalidation reused the approval challenge frame."
    }
    $verification = Invoke-OperatorExchange `
        "ui-verification" "revalidate-scoped-approval-preconditions" `
        "Using this new exact helper frame, verify the exact candidate card is still absent, Developer Mode still equals the captured value, and the dedicated chrome://extensions window binding is unchanged. Fail on any change or uncertainty." `
        ([ordered]@{ type = "verdict"; requiredValue = $true }) $fresh.Observation `
        ([ordered]@{
            expectedCandidatePresent = [bool]$InitialCandidateCard.present
            expectedDeveloperMode = [string]$InitialDeveloperMode.value
            expectedDedicatedTargetRef = $script:DedicatedTargetRef
            approvalChallengeFrameRef = $script:ApprovalChallengeFrameRef
        })
    Assert-ExactKeys $verification.Decision @("passed") `
        "post-approval state revalidation decision"
    if ($verification.Decision.passed -ne $true) {
        throw "Candidate absence, Developer Mode, or dedicated-window state changed after approval."
    }
    $script:ApprovalPreDispatchFrameRef = $frameRef
    $script:ApprovalPreDispatchDecisionRef = [string]$verification.DecisionRef
    $script:ApprovalPreDispatchVerifiedAtUtc = [string]$verification.RespondedAtUtc
}

function Invoke-RecordedAction {
    param(
        [object]$Context,
        [string]$Name,
        [object[]]$Steps,
        [string]$ConsentRef,
        [string]$VerificationInstruction,
        [switch]$ExpectTargetClosed
    )
    if ($script:Actions.Count -ge $script:ActionNames.Count -or
        $Name -cne $script:ActionNames[$script:Actions.Count]) {
        throw "The helper action is out of canonical order."
    }
    if ($ConsentRef -cne "none" -and [String]::IsNullOrWhiteSpace([string]$script:ApprovalId)) {
        throw "A covered action was reached without the scoped candidate-bound action-time approval."
    }
    $pre = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
    $Context.Observation = $pre.Observation
    $Context.ObservationCount += 1
    $preFrameRef = Get-OpaqueRef "frame" ([string]$pre.Observation.frameId)
    $methods = New-Object Collections.Generic.List[string]
    $methods.Add("computer.observe")
    $parameterDigests = New-Object Collections.Generic.List[string]
    $responseDigests = New-Object Collections.Generic.List[string]
    $responseDigests.Add($pre.Response.Digest)
    $dispatchedAt = $null
    $script:LastOperatorDecisionRef = $null
    if ($Steps.Count -eq 0) {
        $dispatchedAt = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
        $middle = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
        $Context.Observation = $middle.Observation
        $Context.ObservationCount += 1
        $methods.Add("computer.observe")
        $responseDigests.Add($middle.Response.Digest)
    }
    $lastOpenFrameRef = $preFrameRef
    for ($stepIndex = 0; $stepIndex -lt $Steps.Count; $stepIndex += 1) {
        $step = $Steps[$stepIndex]
        $stepFrameId = [string]$Context.Observation.frameId
        $lastOpenFrameRef = Get-OpaqueRef "frame" $stepFrameId
        $params = [ordered]@{ frameId = [string]$Context.Observation.frameId }
        switch ($step.kind) {
            "click" {
                $click = Read-ClickParams $Context.Observation ([string]$step.label)
                foreach ($entry in $click.GetEnumerator()) { $params[$entry.Key] = $entry.Value }
                $method = "computer.click"
            }
            "key" { $params.key = [string]$step.value; $method = "computer.key" }
            "typeText" { $params.text = [string]$step.value; $method = "computer.typeText" }
            default { throw "The helper action plan contains an unsupported native step." }
        }
        $parameterDigests.Add((Get-TextSha256 ($params | ConvertTo-Json -Depth 8 -Compress)))
        if ([String]::IsNullOrWhiteSpace([string]$dispatchedAt)) {
            $dispatchedAt = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
        }
        if ($ConsentRef -cne "none" -and $script:FirstCoveredActionSequence -eq 0) {
            if ([String]::IsNullOrWhiteSpace([string]$script:ApprovalPreDispatchFrameRef) -or
                [String]::IsNullOrWhiteSpace([string]$script:ApprovalPreDispatchDecisionRef) -or
                [String]::IsNullOrWhiteSpace([string]$script:ApprovalPreDispatchVerifiedAtUtc) -or
                $script:ApprovalPreDispatchFrameRef -ceq $script:ApprovalChallengeFrameRef) {
                throw "The first covered action lacks a distinct fresh post-approval state revalidation."
            }
            $preDispatchVerifiedAt = [DateTimeOffset]::ParseExact(
                $script:ApprovalPreDispatchVerifiedAtUtc, "o",
                [Globalization.CultureInfo]::InvariantCulture,
                [Globalization.DateTimeStyles]::RoundtripKind
            )
            $dispatchInstant = Assert-FreshCanonicalResponseTimestamp $dispatchedAt `
                ([DateTimeOffset]::ParseExact(
                    $script:ApprovalConfirmedAtUtc, "o", [Globalization.CultureInfo]::InvariantCulture,
                    [Globalization.DateTimeStyles]::RoundtripKind
                )) `
                ([DateTimeOffset]::ParseExact(
                    $script:ApprovalExpiresAtUtc, "o", [Globalization.CultureInfo]::InvariantCulture,
                    [Globalization.DateTimeStyles]::RoundtripKind
                )) "The first covered action dispatch timestamp"
            if ($preDispatchVerifiedAt -le [DateTimeOffset]::ParseExact(
                    $script:ApprovalConfirmedAtUtc, "o",
                    [Globalization.CultureInfo]::InvariantCulture,
                    [Globalization.DateTimeStyles]::RoundtripKind
                ) -or $preDispatchVerifiedAt -gt $dispatchInstant) {
                throw "The fresh post-approval state was not revalidated before the first covered action."
            }
            $script:FirstCoveredActionSequence = $script:Actions.Count + 1
            $script:FirstCoveredActionDispatchedAtUtc = $dispatchedAt
            $script:ApprovalConsumedBeforeExpiry = $true
            $dispatchInstant = $null
        }
        $result = Invoke-ComputerCommand $method $params
        $methods.Add($method)
        $responseDigests.Add($result.Digest)
        $isTerminalCloseStep = $ExpectTargetClosed -and $stepIndex -eq ($Steps.Count - 1)
        if (-not $isTerminalCloseStep) {
            # Never accept response-state observation as post-action authority. The
            # server may retain an older public frame when its own follow-up observe
            # fails, so require a new explicit exact-bound observation here.
            $next = Get-FreshObservation $Context.WindowId $Context.Pid $stepFrameId
            $Context.Observation = $next.Observation
            $Context.ObservationCount += 1
            $methods.Add("computer.observe")
            $responseDigests.Add($next.Response.Digest)
        }
        $params.Clear()
        $result = $null
    }
    if ($ExpectTargetClosed) {
        $status = Invoke-ComputerCommand "computer.status" @{}
        $methods.Add("computer.status")
        $responseDigests.Add($status.Digest)
        if (@($status.Body.result.windows | Where-Object {
            [string]$_.id -ceq $Context.WindowId -and [int64]$_.pid -eq $Context.Pid
        }).Count -ne 0) {
            throw "The exact dedicated Chrome window remained after its close action."
        }
        $postFrameRef = Get-OpaqueRef "closed-frame" "$lastOpenFrameRef|$($status.Digest)"
        $Context.LastFrameRef = $lastOpenFrameRef
    }
    else {
        $post = Get-FreshObservation $Context.WindowId $Context.Pid ([string]$Context.Observation.frameId)
        $Context.Observation = $post.Observation
        $Context.ObservationCount += 1
        $methods.Add("computer.observe")
        $responseDigests.Add($post.Response.Digest)
        $postFrameRef = Get-OpaqueRef "frame" ([string]$post.Observation.frameId)
        $Context.LastFrameRef = $postFrameRef
    }
    if ($preFrameRef -ceq $postFrameRef) { throw "The helper action did not obtain a fresh post-action frame." }
    $verificationObservation = if ($ExpectTargetClosed) { $null } else { $Context.Observation }
    $verificationContext = if ($ExpectTargetClosed) {
        [ordered]@{
            targetClosed = $true
            machineStatusBound = $true
            statusResponseSha256 = $status.Digest
        }
    }
    else {
        [ordered]@{ targetClosed = $false; machineStatusBound = $false }
    }
    $verification = Invoke-OperatorExchange "ui-verification" $Name $VerificationInstruction `
        ([ordered]@{ type = "verdict"; requiredValue = $true }) $verificationObservation `
        $verificationContext
    Assert-ExactKeys $verification.Decision @("passed") "$Name verification decision"
    if ($verification.Decision.passed -ne $true) {
        throw "The independent operator did not verify the required postcondition for $Name."
    }
    $script:Actions += [ordered]@{
        sequence = $script:Actions.Count + 1; name = $Name
        atUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
        dispatchedAtUtc = $dispatchedAt
        source = $script:Source; epochRef = $Context.EpochRef; methods = @($methods)
        preFrameRef = $preFrameRef; postFrameRef = $postFrameRef
        normalizedParamsSha256 = Get-TextSha256 (@($parameterDigests) -join "`n")
        responseSha256 = Get-TextSha256 (@($responseDigests) -join "`n")
        httpStatus = 200; resultVerified = $true; postconditionVerified = $true
        riskRef = $ConsentRef
        approvalRef = $(if ($ConsentRef -ceq "none") { "none" } else { $script:ApprovalId })
        operatorDecisionRef = $verification.DecisionRef
    }
    $parameterDigests.Clear(); $responseDigests.Clear(); $methods.Clear()
}

function Save-RecordedScreenshot {
    param([object]$Context, [string]$Purpose)
    $expectedPurpose = @($script:Screenshots.Keys)[$script:ScreenshotRecords.Count]
    if ($Purpose -cne $expectedPurpose) { throw "Screenshot capture is out of canonical order." }
    $streamed = Get-FreshSharedFrame $Context "the $Purpose screenshot"
    $relative = [string]$streamed.screenshotUrl
    if (-not $relative.StartsWith("/api/computer/screenshot?id=", [StringComparison]::Ordinal) -or
        $relative.Contains("://")) { throw "The helper returned an invalid screenshot endpoint." }
    $rawName = $script:Screenshots[$Purpose]
    $rawPath = [IO.Path]::Combine($script:RawDirectory, $rawName)
    if ([IO.File]::Exists($rawPath) -or [IO.Directory]::Exists($rawPath)) { throw "A raw screenshot already exists." }
    Save-LoopbackBinaryCreateOnce $relative $rawPath
    $facts = Read-StablePngWithDigest $rawPath "raw helper screenshot"
    $frameRef = Get-OpaqueRef "frame" ([string]$streamed.frameId)
    $Context.LastFrameRef = $frameRef
    $script:ScreenshotRecords += [ordered]@{
        sequence = $script:ScreenshotRecords.Count + 1; purpose = $Purpose; source = $script:Source
        epochRef = $Context.EpochRef; shareRef = $Context.ShareRef
        frameRef = $frameRef
        rawImage = $rawName; endpoint = "/api/computer/screenshot"
        bytes = $facts.Bytes; sha256 = $facts.Sha256
        width = $facts.Width; height = $facts.Height; exactWindowFrame = $true
        shareFrameFresh = $true; rawImageRetained = $false
    }
    $relative = $null
}

function Invoke-BrowserCommand {
    param([string]$Method, [hashtable]$Params)
    $response = Invoke-LoopbackJson "/api/v1/command" "Post" ([ordered]@{
        method = $Method
        params = $Params
        callId = "helper-browser-" + [Guid]::NewGuid().ToString("N")
    })
    if ($null -ne $response.Body.error) {
        throw "Browser command $Method failed during helper evidence."
    }
    return $response
}

function Wait-ReducedBrowserControlStatus {
    param([bool]$Active, [bool]$HumanPaused, [AllowNull()][string]$Reason)
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $status = Invoke-BrowserCommand "browser.control.status" @{}
        $result = $status.Body.result
        $actualReason = if ($null -eq $result.humanPause) { $null } else { [string]$result.humanPause.reason }
        if ($result.active -eq $Active -and $result.humanPaused -eq $HumanPaused -and
            $result.revocationPending -eq $false -and $actualReason -ceq $Reason) {
            $reduced = [ordered]@{
                active = $Active
                humanPaused = $HumanPaused
                revocationPending = $false
            }
            if ($HumanPaused) {
                $withReason = [ordered]@{
                    active = $Active
                    humanPaused = $HumanPaused
                    reason = $Reason
                    revocationPending = $false
                }
                return $withReason
            }
            return $reduced
        }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for the exact reduced browser-control status."
}

function Invoke-ExpectedHumanPauseRefusal {
    param([string]$Method, [hashtable]$Params)
    $payload = [ordered]@{
        method = $Method
        params = $Params
        callId = "helper-paused-" + [Guid]::NewGuid().ToString("N")
    } | ConvertTo-Json -Depth 10 -Compress
    $response = $null
    $content = $null
    try {
        Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$Port/api/v1/command" -Method Post `
            -Headers @{ Authorization = "Bearer $($script:Token)" } -ContentType "application/json" `
            -Body $payload -TimeoutSec 25 | Out-Null
        throw "A browser mutation unexpectedly succeeded while human control was paused."
    }
    catch {
        $response = $_.Exception.Response
        if ($null -eq $response -or [int]$response.StatusCode -ne 423) { throw }
        $stream = $response.GetResponseStream()
        try {
            $reader = [IO.StreamReader]::new($stream, $script:Utf8NoBom, $true)
            try { $content = $reader.ReadToEnd() }
            finally { $reader.Dispose() }
        }
        finally { $response.Close() }
    }
    try { $body = ConvertFrom-JsonPreservingStrings $content }
    catch { throw "The human-pause refusal body was not JSON." }
    finally { $content = $null; $payload = $null }
    if ($body.error.code -cne "HUMAN_CONTROL_PAUSED" -or
        $body.taxonomy.code -cne "needs_user" -or
        $body.taxonomy.recoveryHint -cne "handback" -or
        $body.taxonomy.retriable -ne $false) {
        throw "The human-pause refusal did not carry the canonical fail-closed taxonomy."
    }
    return [ordered]@{
        httpStatus = 423
        errorCode = "HUMAN_CONTROL_PAUSED"
        taxonomyState = "needs_user"
        taxonomyAction = "handback"
        retriable = $false
    }
}

function Complete-TrustedPopupResume {
    param([long]$TabId)
    $reduced = Wait-ReducedBrowserControlStatus $false $false $null
    $started = Invoke-BrowserCommand "browser.control.start" @{ tabId = $TabId; ttlMs = 900000 }
    if ($started.Body.result.active -ne $true) {
        throw "A trusted-popup Resume did not permit an explicit new lease."
    }
    $active = Wait-ReducedBrowserControlStatus $true $false $null
    return [ordered]@{
        trustedPopupClick = $true
        operatorSurface = "local-browser-bridge-computer-helper"
        statusPollMethod = "browser.control.status"
        statusPolledAfterResume = $true
        reducedStatus = $reduced
        postResumeStartSucceeded = $true
        activeStatusPolled = $true
        activeStatus = $active
    }
}

function Get-HumanPauseMachineProof {
    param([long]$TabId, [string]$Reason)
    return [ordered]@{
        statusPollMethod = "browser.control.status"
        statusPolledAfterTrigger = $true
        reducedStatus = Wait-ReducedBrowserControlStatus $false $true $Reason
        controlStartRefusal = Invoke-ExpectedHumanPauseRefusal "browser.control.start" @{ tabId = $TabId; ttlMs = 900000 }
        tabMutationRefusal = Invoke-ExpectedHumanPauseRefusal "tabs.new" @{}
        indicatorsRemoved = $true
    }
}

function Show-DeterministicGreeting {
    param([object]$OwnedTarget)
    if ($OwnedTarget.runNonce -cne $script:Binding.runNonce -or
        $OwnedTarget.tabId -isnot [ValueType] -or
        $OwnedTarget.groupId -isnot [ValueType] -or
        $OwnedTarget.focusedByExactOwnedTabActivation -ne $true) {
        throw "The browser API matrix handoff is not bound to this exact run and focused target."
    }
    $tabId = [long]$OwnedTarget.tabId
    $groupId = [long]$OwnedTarget.groupId
    $methods = New-Object Collections.Generic.List[string]
    $digests = New-Object Collections.Generic.List[string]

    $started = Invoke-BrowserCommand "browser.control.start" @{ tabId = $tabId; ttlMs = 900000 }
    $methods.Add("browser.control.start")
    $digests.Add($started.Digest)
    if ($started.Body.result.active -ne $true) {
        throw "The exact matrix-owned demo lease did not start."
    }

    $observed = Invoke-BrowserCommand "page.observe" @{ tabId = $tabId }
    $methods.Add("page.observe")
    $digests.Add($observed.Digest)
    $observation = $observed.Body.result.snapshot
    foreach ($spec in @(
        @("textbox", "Display name", "page.fill", "Bridge Matrix"),
        @("select", "Favorite color", "page.select", "blue"),
        @("button", "Show greeting", "page.click", "")
    )) {
        $matches = @($observation.elements | Where-Object {
            $_.role -eq $spec[0] -and $_.name -eq $spec[1]
        })
        if ($matches.Count -ne 1) {
            throw "The deterministic demo target was not uniquely observed."
        }
        $params = @{
            tabId = $tabId
            ref = [string]$matches[0].ref
            generation = [string]$observation.generation
        }
        if ($spec[2] -eq "page.fill") { $params.text = $spec[3] }
        elseif ($spec[2] -eq "page.select") { $params.value = $spec[3] }
        else { $params.button = "left"; $params.clickCount = 1 }
        $mutated = Invoke-BrowserCommand $spec[2] $params
        $methods.Add($spec[2])
        $digests.Add($mutated.Digest)
        $observed = Invoke-BrowserCommand "page.observe" @{ tabId = $tabId }
        $methods.Add("page.observe")
        $digests.Add($observed.Digest)
        $observation = $observed.Body.result.snapshot
        $params.Clear()
    }
    if ([string]$observation.bodyText -notlike "*Hello, Bridge Matrix. blue selected.*") {
        throw "The deterministic greeting was not rendered."
    }
    $visible = Invoke-BrowserCommand "page.evaluate" @{
        tabId = $tabId
        expression = "(() => { const e=document.getElementById('result'); if(!e)return false; e.scrollIntoView({block:'center'}); return e.textContent==='Hello, Bridge Matrix. blue selected.'; })()"
    }
    $methods.Add("page.evaluate")
    $digests.Add($visible.Digest)
    if ($visible.Body.result.value -ne $true) {
        throw "The deterministic greeting was not brought into view."
    }
    $record = [ordered]@{
        source = "local-browser-bridge-api"
        apiMatrixRecordSha256 = $script:MatrixRecordSha256
        targetBindingSha256 = Get-OpaqueRef "browser-target" "$tabId|$groupId"
        methodSequence = @($methods)
        requestResponseSha256 = Get-TextSha256 (@($digests) -join [Environment]::NewLine)
        resultText = "Hello, Bridge Matrix. blue selected."
        resultVerified = $true
    }
    $methods.Clear()
    $digests.Clear()
    $tabId = 0
    $groupId = 0
    return $record
}

function Add-LifecycleEvent {
    param([string]$Name, [string]$ConnectionState, [string]$ProcessRef, [string]$ExecutableSha256, [int]$ExitCode)
    $script:Lifecycle += [ordered]@{
        sequence = $script:Lifecycle.Count + 1; name = $Name
        atUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
        source = $script:Source; connectionState = $ConnectionState; processRef = $ProcessRef
        executableSha256 = $ExecutableSha256; exitCode = $ExitCode; resultVerified = $true
    }
}

function Wait-ServerReady {
    param([string]$ExpectedVersion, [int]$Seconds = 25)
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if ($null -ne $script:ServerProcess -and $script:ServerProcess.HasExited) {
            throw "The exact candidate server exited before readiness."
        }
        try {
            $health = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$Port/health" `
                -Method Get -TimeoutSec 2
            $body = ConvertFrom-JsonPreservingStrings $health.Content
            if ([int]$health.StatusCode -eq 200 -and $body.ok -eq $true -and
                $body.version -ceq $ExpectedVersion) {
                return Get-BridgeState
            }
        }
        catch {
            if ([DateTime]::UtcNow -ge $deadline) { throw }
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for the exact candidate server readiness/version proof."
}

function Get-ProcessFamilyIds {
    param([int]$RootProcessId)
    $processes = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop)
    $ids = @($RootProcessId)
    do {
        $before = $ids.Count
        foreach ($process in $processes) {
            if ($ids -contains [int]$process.ParentProcessId -and
                $ids -notcontains [int]$process.ProcessId) {
                $ids += [int]$process.ProcessId
            }
        }
    } while ($ids.Count -ne $before)
    return @($ids | Sort-Object -Unique)
}

function Get-ExactImageProcessRows {
    param([string]$ExecutablePath)
    $expected = [IO.Path]::GetFullPath($ExecutablePath)
    return @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop | Where-Object {
        -not [String]::IsNullOrWhiteSpace([string]$_.ExecutablePath) -and
        [String]::Equals(
            [IO.Path]::GetFullPath([string]$_.ExecutablePath),
            $expected,
            [StringComparison]::OrdinalIgnoreCase
        )
    })
}

function Wait-ProtocolBoundHelperWorker {
    param([Diagnostics.Process]$Supervisor, [string]$HelperPath, [string]$ExpectedSessionId)
    $interactiveSessionId = [int](Get-Process -Id $PID -ErrorAction Stop).SessionId
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    $stableIdentity = $null
    $stablePolls = 0
    do {
        $Supervisor.Refresh()
        if ($Supervisor.HasExited) { throw "The exact helper supervisor exited before topology binding." }
        $state = Get-BridgeState
        $computer = $state.computer
        $reportedWorkerPid = [int64]$computer.processId
        $reportedSessionId = [string]$computer.sessionId
        $direct = @(Get-ExactImageProcessRows $HelperPath | Where-Object {
            [int64]$_.ParentProcessId -eq [int64]$Supervisor.Id
        })
        if ($direct.Count -gt 1) {
            throw "The exact helper supervisor has multiple exact-image direct workers."
        }
        if ($state.computerConnected -eq $true -and $direct.Count -eq 1 -and
            $reportedWorkerPid -gt 0 -and [int64]$direct[0].ProcessId -eq $reportedWorkerPid -and
            -not [String]::IsNullOrWhiteSpace($reportedSessionId) -and
            $reportedSessionId -ceq $ExpectedSessionId -and
            [int]$direct[0].SessionId -eq $interactiveSessionId) {
            $identity = "$reportedSessionId|$reportedWorkerPid"
            if ($identity -ceq $stableIdentity) { $stablePolls += 1 }
            else { $stableIdentity = $identity; $stablePolls = 1 }
            if ($stablePolls -ge 2) {
                $roundTrip = Invoke-ComputerCommand "computer.status" @{}
                if ($roundTrip.Body.state.computerConnected -ne $true -or
                    [string]$roundTrip.Body.state.computer.sessionId -cne $ExpectedSessionId -or
                    [int64]$roundTrip.Body.state.computer.processId -ne $reportedWorkerPid) {
                    throw "computer.status did not round-trip through the topology-bound helper worker."
                }
                $script:BoundHelperWorkerPid = [int]$reportedWorkerPid
                return [ordered]@{
                    exactImageDirectChildCount = 1
                    exactImageMatched = $true
                    directChildOfLaunchedSupervisor = $true
                    interactiveSessionMatched = $true
                    stableConsecutivePolls = $stablePolls
                    helloStateMatched = $true
                    protocolRoundTrip = $true
                    roundTripMethod = "computer.status"
                    supervisorProcessRef = Get-OpaqueRef "helper-supervisor" ([string]$Supervisor.Id)
                    workerProcessRef = Get-OpaqueRef "helper-worker" ([string]$reportedWorkerPid)
                }
            }
        }
        else {
            $stableIdentity = $null
            $stablePolls = 0
        }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for one stable exact-image, direct-child, same-session helper worker."
}

function Get-CanonicalPortListenerCount {
    return @(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction Stop | Where-Object {
        [string]$_.LocalAddress -in @("127.0.0.1", "0.0.0.0", "::", "::1")
    }).Count
}

function Wait-NoExactImageProcess {
    param([string]$ExecutablePath, [string]$Label)
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        if (@(Get-ExactImageProcessRows $ExecutablePath).Count -eq 0) { return }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "An exact-image process remained after $Label."
}

function Wait-CanonicalPortReleased {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        if ((Get-CanonicalPortListenerCount) -eq 0) { return }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "A relevant listener still covered 127.0.0.1:$Port after server termination."
}

function Get-ListenerCountForProcesses {
    param([int[]]$ProcessIds)
    if ($ProcessIds.Count -eq 0) { return 0 }
    return @(Get-NetTCPConnection -State Listen -ErrorAction Stop | Where-Object {
        $ProcessIds -contains [int]$_.OwningProcess
    }).Count
}

function Set-MutationDisposition {
    param([hashtable]$State, [string]$Name, [string]$Disposition)
    if (-not $State.ContainsKey($Name) -or
        $Disposition -notin @("not_attempted", "outcome_unknown", "verified_applied", "restored")) {
        throw "The mutation disposition transition is invalid."
    }
    $current = [string]$State[$Name]
    $allowed = switch ($current) {
        "not_attempted" { @("outcome_unknown") }
        "outcome_unknown" { @("verified_applied", "restored") }
        "verified_applied" { @("outcome_unknown", "restored") }
        "restored" {
            if ($Name -in @("NativePicker", "ClearTokenDialog", "RemoveExtensionDialog")) {
                @("outcome_unknown")
            }
            else { @() }
        }
        default { throw "The current mutation disposition is invalid." }
    }
    if ($allowed -cnotcontains $Disposition) {
        throw "The mutation disposition transition $current -> $Disposition is not allowed."
    }
    $State[$Name] = $Disposition
}

function Test-UnresolvedMutation {
    param([hashtable]$State, [string]$Name)
    return $State[$Name] -in @("outcome_unknown", "verified_applied")
}

function Get-UnresolvedMutationNames {
    param([hashtable]$State)
    return @($State.Keys | Where-Object { Test-UnresolvedMutation $State $_ } | Sort-Object)
}

function Invoke-UnrecordedNativeSteps {
    param(
        [string]$WindowInstruction,
        [object[]]$Steps,
        [string]$RiskRef,
        [string]$ExpectedWindowId,
        [int64]$ExpectedPid = 0,
        [switch]$ExpectTargetClosed
    )
    if (-not [String]::IsNullOrWhiteSpace($RiskRef) -and
        [String]::IsNullOrWhiteSpace([string]$script:ApprovalId)) {
        throw "Rollback reached a covered action without the scoped approval."
    }
    if ([String]::IsNullOrWhiteSpace($ExpectedWindowId)) {
        $selected = Select-ExactWindow $WindowInstruction
    }
    else {
        Write-Host $WindowInstruction
        $selected = Get-ExactChromeWindow $ExpectedWindowId $ExpectedPid
    }
    $windowId = [string]$selected.WindowId
    [void](Get-ExactChromeWindow $windowId $selected.Pid)
    $observation = (Get-FreshObservation $windowId $selected.Pid $null).Observation
    for ($stepIndex = 0; $stepIndex -lt $Steps.Count; $stepIndex += 1) {
        $step = $Steps[$stepIndex]
        $stepFrameId = [string]$observation.frameId
        $params = [ordered]@{ frameId = [string]$observation.frameId }
        switch ($step.kind) {
            "click" {
                $click = Read-ClickParams $observation ([string]$step.label)
                foreach ($entry in $click.GetEnumerator()) { $params[$entry.Key] = $entry.Value }
                $method = "computer.click"
            }
            "key" { $params.key = [string]$step.value; $method = "computer.key" }
            "typeText" { $params.text = [string]$step.value; $method = "computer.typeText" }
            default { throw "Rollback contains an unsupported native step." }
        }
        [void](Invoke-ComputerCommand $method $params)
        $params.Clear()
        if (-not ($ExpectTargetClosed -and $stepIndex -eq ($Steps.Count - 1))) {
            [void](Get-ExactChromeWindow $windowId $selected.Pid)
            $observation = (Get-FreshObservation $windowId $selected.Pid $stepFrameId).Observation
        }
    }
    if ($ExpectTargetClosed) {
        $status = Invoke-ComputerCommand "computer.status" @{}
        if (@($status.Body.result.windows | Where-Object {
            [string]$_.id -ceq $windowId -and [int64]$_.pid -eq $selected.Pid
        }).Count -ne 0) {
            throw "The exact rollback-owned Chrome window remained open."
        }
    }
    $windowId = $null
    $observation = $null
}

function Read-RollbackToggleState {
    param([string]$WindowId, [int64]$ExpectedPid, [string]$Label)
    [void](Get-ExactChromeWindow $WindowId $ExpectedPid)
    $fresh = Get-FreshObservation $WindowId $ExpectedPid $null
    $exchange = Invoke-OperatorExchange "ui-state" "rollback-$Label" `
        "Interpret the current rollback toggle state from this exact fresh owned frame." `
        ([ordered]@{ type = "enum"; values = @("enabled", "disabled") }) $fresh.Observation $null
    Assert-ExactKeys $exchange.Decision @("value") "$Label rollback-state decision"
    $value = ([string]$exchange.Decision.value).Trim().ToLowerInvariant()
    if ($value -notin @("enabled", "disabled")) {
        throw "$Label rollback state was not reduced to enabled or disabled."
    }
    return $value
}

function Read-RollbackSavedTokenState {
    param([string]$WindowId, [int64]$ExpectedPid)
    [void](Get-ExactChromeWindow $WindowId $ExpectedPid)
    $fresh = Get-FreshObservation $WindowId $ExpectedPid $null
    $exchange = Invoke-OperatorExchange "ui-state" "rollback-saved-token" `
        "Interpret the current saved-token state from this exact fresh owned popup frame." `
        ([ordered]@{ type = "enum"; values = @("configured", "unconfigured") }) $fresh.Observation $null
    Assert-ExactKeys $exchange.Decision @("value") "saved-token rollback-state decision"
    $value = ([string]$exchange.Decision.value).Trim().ToLowerInvariant()
    if ($value -notin @("configured", "unconfigured")) {
        throw "Saved-token rollback state was not reduced to configured or unconfigured."
    }
    return $value
}

function Read-RollbackCandidateCardState {
    param([string]$WindowId, [int64]$ExpectedPid)
    [void](Get-ExactChromeWindow $WindowId $ExpectedPid)
    $fresh = Get-FreshObservation $WindowId $ExpectedPid $null
    $exchange = Invoke-OperatorExchange "ui-state" "rollback-candidate-card" `
        "Interpret exact v0.12.30 test-owned candidate-card presence from this fresh owned frame." `
        ([ordered]@{ type = "enum"; values = @("present", "absent") }) $fresh.Observation $null
    Assert-ExactKeys $exchange.Decision @("value") "candidate-card rollback-state decision"
    $value = ([string]$exchange.Decision.value).Trim().ToLowerInvariant()
    if ($value -notin @("present", "absent")) {
        throw "Candidate-card rollback state was not reduced to present or absent."
    }
    return $value
}

function Read-RollbackModalState {
    param([string]$WindowId, [int64]$ExpectedPid, [string]$Label)
    [void](Get-ExactChromeWindow $WindowId $ExpectedPid)
    $fresh = Get-FreshObservation $WindowId $ExpectedPid $null
    $exchange = Invoke-OperatorExchange "ui-state" "rollback-$Label-modal" `
        "Interpret only whether the exact owned $Label modal is open or closed in this fresh frame." `
        ([ordered]@{ type = "enum"; values = @("open", "closed") }) $fresh.Observation $null
    Assert-ExactKeys $exchange.Decision @("value") "$Label rollback-modal decision"
    $value = ([string]$exchange.Decision.value).Trim().ToLowerInvariant()
    if ($value -notin @("open", "closed")) {
        throw "$Label rollback modal state was not reduced to open or closed."
    }
    return $value
}

function Resolve-NativePickerRollbackTarget {
    if (-not [String]::IsNullOrWhiteSpace([string]$script:NativePickerWindowId)) {
        $status = Invoke-ComputerCommand "computer.status" @{}
        $matches = @($status.Body.result.windows | Where-Object {
            [string]$_.id -ceq $script:NativePickerWindowId -and
            [int64]$_.pid -eq $script:NativePickerWindowPid
        })
        if ($matches.Count -gt 1) { throw "The bound native picker identity is ambiguous." }
        if ($matches.Count -eq 0) { return $null }
        return [pscustomobject]@{
            WindowId = $script:NativePickerWindowId
            Pid = $script:NativePickerWindowPid
        }
    }
    $state = Invoke-ComputerCommand "computer.status" @{}
    $candidates = @($state.Body.result.windows | Where-Object {
        $targetRef = Get-OpaqueRef "target" "$([string]$_.id)|$([int64]$_.pid)"
        [int64]$_.pid -eq $script:DedicatedWindowPid -and
        $targetRef -cne $script:DedicatedTargetRef -and
        $script:BaselineChrome.TargetRefs -cnotcontains $targetRef
    })
    if ($candidates.Count -eq 0) { return $null }
    if ($candidates.Count -ne 1) {
        throw "The unbound native picker could not be resolved as one sole-new process-linked window."
    }
    $script:NativePickerWindowId = [string]$candidates[0].id
    $script:NativePickerWindowPid = [int64]$candidates[0].pid
    return [pscustomobject]@{
        WindowId = $script:NativePickerWindowId
        Pid = $script:NativePickerWindowPid
    }
}

function Resolve-SoleNewDedicatedRollbackBinding {
    param([string[]]$BaselineTargetRefs, [object[]]$CurrentBindings)
    $newBindings = @($CurrentBindings | Where-Object {
        $BaselineTargetRefs -cnotcontains [string]$_.TargetRef
    })
    $missingBaseline = @($BaselineTargetRefs | Where-Object {
        $CurrentBindings.TargetRef -cnotcontains $_
    })
    if ($missingBaseline.Count -ne 0) {
        throw "The trusted baseline Chrome set changed before dedicated-window rollback binding."
    }
    if ($newBindings.Count -eq 0 -and $CurrentBindings.Count -eq $BaselineTargetRefs.Count) {
        return $null
    }
    if ($newBindings.Count -ne 1 -or
        $CurrentBindings.Count -ne ($BaselineTargetRefs.Count + 1)) {
        throw "Dedicated-window rollback did not observe exactly zero or one sole-new Chrome window."
    }
    return $newBindings[0]
}

function Resolve-DedicatedWindowRollbackTarget {
    if (-not [String]::IsNullOrWhiteSpace([string]$script:DedicatedWindowId)) {
        return [pscustomobject]@{
            WindowId = $script:DedicatedWindowId; Pid = $script:DedicatedWindowPid
        }
    }
    if ($null -eq $script:BaselineChrome) {
        throw "The trusted baseline Chrome set is unavailable for dedicated-window rollback."
    }
    $status = Invoke-ComputerCommand "computer.status" @{}
    $bindings = New-Object Collections.Generic.List[object]
    foreach ($window in @($status.Body.result.windows)) {
        try {
            if ((Get-NormalizedApplication $window) -ceq "google-chrome") {
                $bindings.Add([pscustomobject]@{
                    Window = $window
                    TargetRef = Get-OpaqueRef "target" "$([string]$window.id)|$([int64]$window.pid)"
                })
            }
        }
        catch { }
    }
    $resolved = Resolve-SoleNewDedicatedRollbackBinding `
        @($script:BaselineChrome.TargetRefs) @($bindings)
    if ($null -eq $resolved) { return $null }
    $script:DedicatedWindowId = [string]$resolved.Window.id
    $script:DedicatedWindowPid = [int64]$resolved.Window.pid
    $script:DedicatedTargetRef = [string]$resolved.TargetRef
    $script:DedicatedProcessRef = Get-OpaqueRef "process" ([string]$script:DedicatedWindowPid)
    $script:DedicatedAbsentBeforeCreation = $true
    $script:DedicatedCreatedAsOnlyNewChromeWindow = $true
    $bindings.Clear()
    return [pscustomobject]@{
        WindowId = $script:DedicatedWindowId; Pid = $script:DedicatedWindowPid
    }
}

function Invoke-BestEffortUiRollback {
    param(
        [hashtable]$State,
        [object]$InitialDeveloperMode,
        [object]$InitialFullAccess,
        [object]$InitialSavedToken
    )
    $errors = New-Object Collections.Generic.List[string]
    try {
        if ($null -ne $script:ActiveEpoch) {
            $stopped = Invoke-ComputerCommand "computer.share.stop" @{}
            if ($stopped.Body.result.active -ne $false) { throw "Active helper share did not stop." }
            $script:ActiveEpoch = $null
        }
    }
    catch { $errors.Add("share: $($_.Exception.Message)") }

    if (Test-UnresolvedMutation $State "NativePicker") {
        try {
            $picker = Resolve-NativePickerRollbackTarget
            if ($null -ne $picker) {
                Invoke-UnrecordedNativeSteps `
                    "Dismiss only the exact sole-new process-linked native Load unpacked picker." `
                    @([ordered]@{ kind="key"; value="Escape" }) "failureRollback" `
                    $picker.WindowId $picker.Pid -ExpectTargetClosed
            }
            Set-MutationDisposition $State "NativePicker" "restored"
        }
        catch { $errors.Add("native-picker-modal: $($_.Exception.Message)") }
    }

    if (Test-UnresolvedMutation $State "ClearTokenDialog") {
        try {
            if ([String]::IsNullOrWhiteSpace($script:LastOwnedPopupWindowId) -or
                $script:LastOwnedPopupWindowPid -le 0) {
                throw "No exact candidate-popup identity was bound for clear-token modal rollback."
            }
            $modalState = Read-RollbackModalState `
                $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid "clear-token"
            if ($modalState -ceq "open") {
                Invoke-UnrecordedNativeSteps `
                    "Dismiss only the exact clear-token confirmation in the bound candidate popup." `
                    @([ordered]@{ kind="key"; value="Escape" }) "failureRollback" `
                    $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                $modalState = Read-RollbackModalState `
                    $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid "clear-token"
            }
            if ($modalState -cne "closed") { throw "Clear-token modal rollback was not verified." }
            Set-MutationDisposition $State "ClearTokenDialog" "restored"
        }
        catch { $errors.Add("clear-token-modal: $($_.Exception.Message)") }
    }

    if (Test-UnresolvedMutation $State "RemoveExtensionDialog") {
        try {
            if ([String]::IsNullOrWhiteSpace($script:DedicatedWindowId) -or
                $script:DedicatedWindowPid -le 0) {
                throw "No exact dedicated-window identity was bound for removal-modal rollback."
            }
            $modalState = Read-RollbackModalState `
                $script:DedicatedWindowId $script:DedicatedWindowPid "remove-extension"
            if ($modalState -ceq "open") {
                Invoke-UnrecordedNativeSteps `
                    "Dismiss only the exact removal confirmation in the dedicated extensions window." `
                    @([ordered]@{ kind="key"; value="Escape" }) "failureRollback" `
                    $script:DedicatedWindowId $script:DedicatedWindowPid
                $modalState = Read-RollbackModalState `
                    $script:DedicatedWindowId $script:DedicatedWindowPid "remove-extension"
            }
            if ($modalState -cne "closed") { throw "Removal-modal rollback was not verified." }
            Set-MutationDisposition $State "RemoveExtensionDialog" "restored"
        }
        catch { $errors.Add("remove-extension-modal: $($_.Exception.Message)") }
    }

    if ((Test-UnresolvedMutation $State "SavedToken") -or
        (Test-UnresolvedMutation $State "FullAccess")) {
        try {
            if (Test-UnresolvedMutation $State "ClearTokenDialog") {
                throw "Saved-token and Full Access rollback is blocked by an unresolved clear-token modal."
            }
            if ([String]::IsNullOrWhiteSpace($script:LastOwnedPopupWindowId) -or
                $script:LastOwnedPopupWindowPid -le 0) {
                throw "No exact candidate-popup identity was bound before failure; popup state was not touched."
            }
            [void](Get-ExactChromeWindow $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid)
            if (Test-UnresolvedMutation $State "SavedToken") {
                if ($null -eq $InitialSavedToken -or $InitialSavedToken.configured -ne $false) {
                    throw "The live initial saved-token state is unavailable or was not unconfigured."
                }
                $tokenState = Read-RollbackSavedTokenState $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                if ($tokenState -ceq "configured") {
                    Set-MutationDisposition $State "ClearTokenDialog" "outcome_unknown"
                    Invoke-UnrecordedNativeSteps `
                        "Use only the exact bound candidate popup for rollback token clearing." `
                        @([ordered]@{ kind="click"; label="Clear saved token button" }) `
                        "clearSavedTokenInitiate" `
                        $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                    Set-MutationDisposition $State "ClearTokenDialog" "verified_applied"
                    Invoke-UnrecordedNativeSteps `
                        "Use only the same exact bound candidate popup for rollback confirmation." `
                        @([ordered]@{ kind="click"; label="affirmative clear-token confirmation" }) `
                        "clearSavedTokenConfirm" `
                        $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                    Set-MutationDisposition $State "ClearTokenDialog" "restored"
                    $tokenState = Read-RollbackSavedTokenState $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                }
                if ($tokenState -cne "unconfigured") { throw "Saved-token rollback was not verified." }
                Set-MutationDisposition $State "SavedToken" "restored"
            }
            if (Test-UnresolvedMutation $State "FullAccess") {
                if ($null -eq $InitialFullAccess -or
                    $InitialFullAccess.value -notin @("enabled", "disabled")) {
                    throw "The live initial Full Access state is unavailable."
                }
                $fullAccessState = Read-RollbackToggleState `
                    $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid "FullAccess"
                if ($fullAccessState -cne $InitialFullAccess.value) {
                    Invoke-UnrecordedNativeSteps `
                        "Use only the exact bound candidate popup for Full Access restoration." `
                        @([ordered]@{ kind="click"; label="Full Access toggle to the live-captured initial state" }) `
                        "fullAccessUse" `
                        $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid
                    $fullAccessState = Read-RollbackToggleState `
                        $script:LastOwnedPopupWindowId $script:LastOwnedPopupWindowPid "FullAccess"
                }
                if ($fullAccessState -cne $InitialFullAccess.value) { throw "Full Access rollback was not verified." }
                Set-MutationDisposition $State "FullAccess" "restored"
            }
        }
        catch { $errors.Add("popup: $($_.Exception.Message)") }
    }

    if ((Test-UnresolvedMutation $State "CandidateExtension") -or
        (Test-UnresolvedMutation $State "DeveloperMode") -or
        (Test-UnresolvedMutation $State "DedicatedWindow")) {
        try {
            if ((Test-UnresolvedMutation $State "NativePicker") -or
                (Test-UnresolvedMutation $State "RemoveExtensionDialog")) {
                throw "Candidate and Chrome-setting rollback is blocked by an unresolved owned modal."
            }
            if ((Test-UnresolvedMutation $State "DedicatedWindow") -and
                [String]::IsNullOrWhiteSpace([string]$script:DedicatedWindowId)) {
                $dedicatedRollbackTarget = Resolve-DedicatedWindowRollbackTarget
                if ($null -eq $dedicatedRollbackTarget) {
                    Set-MutationDisposition $State "DedicatedWindow" "restored"
                }
            }
            if (-not (Test-UnresolvedMutation $State "CandidateExtension") -and
                -not (Test-UnresolvedMutation $State "DeveloperMode") -and
                -not (Test-UnresolvedMutation $State "DedicatedWindow")) {
                return @($errors)
            }
            if ([String]::IsNullOrWhiteSpace($script:DedicatedWindowId) -or
                $script:DedicatedWindowPid -le 0 -or
                -not $script:DedicatedAbsentBeforeCreation -or
                -not $script:DedicatedCreatedAsOnlyNewChromeWindow -or
                (Get-OpaqueRef "target" "$($script:DedicatedWindowId)|$($script:DedicatedWindowPid)") -cne $script:DedicatedTargetRef) {
                throw "The exact sole-new dedicated Chrome identity was not proved; Chrome state was not touched."
            }
            [void](Get-ExactChromeWindow $script:DedicatedWindowId $script:DedicatedWindowPid)
            Invoke-UnrecordedNativeSteps `
                "Use only the exact bound test-owned dedicated Chrome window for rollback." @(
                    [ordered]@{ kind="key"; value="Control+L" },
                    [ordered]@{ kind="typeText"; value="chrome://extensions" },
                    [ordered]@{ kind="key"; value="Enter" }
                ) "" `
                $script:DedicatedWindowId $script:DedicatedWindowPid
            if (Test-UnresolvedMutation $State "CandidateExtension") {
                $cardState = Read-RollbackCandidateCardState $script:DedicatedWindowId $script:DedicatedWindowPid
                if ($cardState -ceq "present") {
                    Set-MutationDisposition $State "RemoveExtensionDialog" "outcome_unknown"
                    Invoke-UnrecordedNativeSteps `
                        "Use only the exact bound test-owned chrome://extensions window." @(
                            [ordered]@{ kind="click"; label="Remove on the exact v0.12.30 test-owned candidate card" },
                            [ordered]@{ kind="click"; label="confirm removal of that exact candidate card" }
                        ) "extensionDisposition" `
                        $script:DedicatedWindowId $script:DedicatedWindowPid
                    Set-MutationDisposition $State "RemoveExtensionDialog" "restored"
                    $cardState = Read-RollbackCandidateCardState $script:DedicatedWindowId $script:DedicatedWindowPid
                }
                if ($cardState -cne "absent") { throw "Candidate-card rollback was not verified." }
                Set-MutationDisposition $State "CandidateExtension" "restored"
            }
            if (Test-UnresolvedMutation $State "DeveloperMode") {
                if ($null -eq $InitialDeveloperMode -or
                    $InitialDeveloperMode.value -notin @("enabled", "disabled")) {
                    throw "The live initial Developer Mode state is unavailable."
                }
                $developerState = Read-RollbackToggleState `
                    $script:DedicatedWindowId $script:DedicatedWindowPid "DeveloperMode"
                if ($developerState -cne $InitialDeveloperMode.value) {
                    Invoke-UnrecordedNativeSteps `
                        "Use only the exact bound test-owned chrome://extensions window." `
                        @([ordered]@{ kind="click"; label="Developer Mode toggle to the live-captured initial state" }) `
                        "developerModeChange" `
                        $script:DedicatedWindowId $script:DedicatedWindowPid
                    $developerState = Read-RollbackToggleState `
                        $script:DedicatedWindowId $script:DedicatedWindowPid "DeveloperMode"
                }
                if ($developerState -cne $InitialDeveloperMode.value) { throw "Developer Mode rollback was not verified." }
                Set-MutationDisposition $State "DeveloperMode" "restored"
            }
            if (Test-UnresolvedMutation $State "DedicatedWindow") {
                $otherUnresolved = @(Get-UnresolvedMutationNames $State | Where-Object { $_ -cne "DedicatedWindow" })
                if ($otherUnresolved.Count -ne 0) {
                    throw "The exact dedicated window was intentionally left open because other rollback state is unresolved: $($otherUnresolved -join ', ')."
                }
                Invoke-UnrecordedNativeSteps `
                    "Close only the exact sole-new test-owned dedicated Chrome window." `
                    @([ordered]@{ kind="key"; value="Control+Shift+W" }) `
                    "" `
                    $script:DedicatedWindowId $script:DedicatedWindowPid -ExpectTargetClosed
                Set-MutationDisposition $State "DedicatedWindow" "restored"
            }
        }
        catch { $errors.Add("chrome: $($_.Exception.Message)") }
    }
    return @($errors)
}

function Invoke-Run {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "The live computer-helper chain recorder runs only on Windows."
    }
    if ($Port -ne 17373) {
        throw "The v0.12.30 acceptance recorder requires the canonical 127.0.0.1:17373 endpoint."
    }
    $preflightPath = Resolve-OrdinaryFile $PreflightRecord "PreflightRecord"
    $runnerPath = Resolve-OrdinaryFile $ApiMatrixRunner "ApiMatrixRunner"
    $serverPath = Resolve-OrdinaryFile $ServerExecutable "ServerExecutable"
    $helperPath = Resolve-OrdinaryFile $HelperExecutable "HelperExecutable"
    $extensionDirectoryPath = Resolve-OrdinaryDirectory $ExtensionDirectory "ExtensionDirectory"
    $script:RawDirectory = Resolve-OrdinaryDirectory $RawScreenshotDirectory "RawScreenshotDirectory"
    $script:OperatorExchange = Resolve-OrdinaryDirectory $OperatorExchangeDirectory "OperatorExchangeDirectory"
    Assert-PrivateOperatorExchangeDirectory $script:OperatorExchange
    if ([IO.DirectoryInfo]::new($script:RawDirectory).GetFileSystemInfos().Count -ne 0) {
        throw "RawScreenshotDirectory must begin empty."
    }
    if ([IO.DirectoryInfo]::new($script:OperatorExchange).GetFileSystemInfos().Count -ne 0) {
        throw "OperatorExchangeDirectory must begin empty."
    }
    $script:OperatorExchangeArtifacts = New-Object Collections.Generic.List[string]
    $script:OperatorResponseReservations = New-Object Collections.Generic.List[object]
    $script:OperatorExpectedTransientArtifacts = New-Object Collections.Generic.List[string]
    Assert-Hex $ExecutorSessionRef 64 "ExecutorSessionRef"
    Assert-Hex $ExpectedOrchestratorSessionRef 64 "ExpectedOrchestratorSessionRef"
    $script:ExecutorRef = $ExecutorSessionRef
    $script:ExpectedOrchestratorRef = $ExpectedOrchestratorSessionRef
    $script:MatrixOutputPath = Resolve-NewJson $ApiMatrixRecord
    $script:ApprovalOutputPath = Resolve-NewJson $ScopedApprovalRecord
    $outputPath = Resolve-NewJson $OutputRecord
    if (@(@($script:MatrixOutputPath, $script:ApprovalOutputPath, $outputPath) |
            Select-Object -Unique).Count -ne 3) {
        throw "ApiMatrixRecord, ScopedApprovalRecord, and OutputRecord must be distinct new files."
    }

    $stablePreflight = Read-StableJsonWithDigest $preflightPath "PreflightRecord"
    $preflight = $stablePreflight.Value
    $script:Binding = Get-CandidateBinding $preflight $stablePreflight.Sha256
    $script:ReleaseCandidateBinding = $preflight.releaseCandidateBinding
    $script:CandidateBindingSha256 = Get-TextSha256 (
        $script:Binding | ConvertTo-Json -Depth 12 -Compress
    )
    if ([IO.Path]::GetFileName($serverPath) -cne $preflight.candidate.server.name -or
        (Get-Sha256 $serverPath) -cne $preflight.candidate.server.sha256 -or
        [IO.Path]::GetFileName($helperPath) -cne $preflight.candidate.computerHelper.name -or
        (Get-Sha256 $helperPath) -cne $preflight.candidate.computerHelper.sha256) {
        throw "The live harness executables do not match the exact preflight candidate."
    }
    $payloadInventory = @($preflight.candidate.extension.extractedPayloadInventory)
    [void](Get-ExactExtensionPayloadDigest $extensionDirectoryPath $payloadInventory $preflight.candidate.extension.combinedPayloadSha256)

    $credential = New-Object byte[] 32
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    try { $rng.GetBytes($credential) } finally { $rng.Dispose() }
    $script:Token = [Convert]::ToBase64String($credential).TrimEnd('=').Replace('+', '-').Replace('/', '_')

    $script:Lifecycle = @()
    $script:Epochs = @()
    $script:Actions = @()
    $script:ScreenshotRecords = @()
    $script:CommandSequence = 0
    $script:OperatorRequestCount = 0
    $script:StatusDecisionCount = 0
    $script:FreshFrameDecisionCount = 0
    $script:OperatorResponseChainSha256 = Get-TextSha256 (
        "operator-exchange-v1`n$($script:Binding.runNonce)"
    )
    $script:ReviewerSessionRef = $null
    $script:OperatorExchangeScratchDeleted = $false
    $script:ApprovalId = $null
    $script:ApprovalScopeSha256 = $null
    $script:ApprovalConfirmedAtUtc = $null
    $script:ApprovalExpiresAtUtc = $null
    $script:ApprovalConsumedBeforeExpiry = $false
    $script:ApprovalRequest = $null
    $script:ApprovalResponse = $null
    $script:ApprovalChallengeFrameRef = $null
    $script:ApprovalPreDispatchFrameRef = $null
    $script:ApprovalPreDispatchDecisionRef = $null
    $script:ApprovalPreDispatchVerifiedAtUtc = $null
    $script:FirstCoveredActionSequence = 0
    $script:FirstCoveredActionDispatchedAtUtc = $null
    $script:MatrixRecordSha256 = $null
    $script:ServerProcess = $null
    $script:ActiveEpoch = $null
    $script:BaselineChrome = $null
    $script:DedicatedWindowId = $null
    $script:DedicatedWindowPid = 0
    $script:DedicatedTargetRef = $null
    $script:DedicatedProcessRef = $null
    $script:DedicatedAbsentBeforeCreation = $false
    $script:DedicatedCreatedAsOnlyNewChromeWindow = $false
    $script:LastOwnedPopupWindowId = $null
    $script:LastOwnedPopupWindowPid = 0
    $script:NativePickerWindowId = $null
    $script:NativePickerWindowPid = 0
    $script:BoundHelperWorkerPid = 0
    $helperProcess = $null
    $helperFamilyIds = @()
    $serverProcessRef = $null
    $helperProcessRef = $null
    $sessionBinding = $null
    $helperTopology = $null
    $ownedTarget = $null
    $browserAction = $null
    $handback = $null
    $initialCandidateCard = $null
    $finalCandidateCard = $null
    $capturedDeveloperMode = $null
    $capturedFullAccess = $null
    $capturedSavedToken = $null
    $record = $null
    $primaryFailure = $null
    $cleanupErrors = New-Object Collections.Generic.List[string]
    $startedAt = [DateTimeOffset]::UtcNow
    $mutation = @{
        DedicatedWindow = "not_attempted"
        DeveloperMode = "not_attempted"
        CandidateExtension = "not_attempted"
        FullAccess = "not_attempted"
        SavedToken = "not_attempted"
        NativePicker = "not_attempted"
        ClearTokenDialog = "not_attempted"
        RemoveExtensionDialog = "not_attempted"
    }
    $tokenWasPresent = Test-Path Env:LBB_TOKEN
    $portWasPresent = Test-Path Env:LBB_PORT
    $previousToken = [Environment]::GetEnvironmentVariable("LBB_TOKEN", "Process")
    $previousPort = [Environment]::GetEnvironmentVariable("LBB_PORT", "Process")

    try {
        if ((Get-CanonicalPortListenerCount) -ne 0) {
            throw "The acceptance port was not free before server start."
        }
        if (@(Get-ExactImageProcessRows $helperPath).Count -ne 0) {
            throw "An exact-image candidate helper process already existed before this run."
        }
        [Environment]::SetEnvironmentVariable("LBB_TOKEN", $script:Token, "Process")
        [Environment]::SetEnvironmentVariable("LBB_PORT", [string]$Port, "Process")
        $script:ServerProcess = Start-Process -FilePath $serverPath -ArgumentList @("--no-update-check") -WorkingDirectory ([IO.Path]::GetDirectoryName($serverPath)) -PassThru
        $serverProcessRef = Get-OpaqueRef "server-process" "$($script:ServerProcess.Id)|$($script:ServerProcess.StartTime.ToUniversalTime().Ticks)"
        $initialState = Wait-ServerReady $script:Version
        $listeners = @(Get-NetTCPConnection -State Listen -ErrorAction Stop | Where-Object {
            [int]$_.OwningProcess -eq $script:ServerProcess.Id
        })
        if ($listeners.Count -ne 1 -or $listeners[0].LocalAddress -cne "127.0.0.1" -or [int]$listeners[0].LocalPort -ne $Port) {
            throw "The candidate server did not own the sole expected IPv4 loopback listener."
        }
        Add-LifecycleEvent "server-ready" "disconnected" $serverProcessRef $preflight.candidate.server.sha256 -1
        if ($initialState.computerConnected -eq $true) {
            throw "A computer helper was already connected before this run."
        }
        Add-LifecycleEvent "computer-disconnected-before-helper-start" "disconnected" "not-applicable" "not-applicable" -1

        $helperProcess = Start-Process -FilePath $helperPath -WorkingDirectory ([IO.Path]::GetDirectoryName($helperPath)) -PassThru
        $helperProcessRef = Get-OpaqueRef "helper-process" "$($helperProcess.Id)|$($helperProcess.StartTime.ToUniversalTime().Ticks)"
        Add-LifecycleEvent "helper-started" "disconnected" $helperProcessRef $preflight.candidate.computerHelper.sha256 -1
        $connected = Wait-BridgeState {
            param($state)
            $state.computerConnected -eq $true -and $null -ne $state.computer
        } "candidate helper connection"
        $helperTopology = Wait-ProtocolBoundHelperWorker `
            $helperProcess $helperPath ([string]$connected.computer.sessionId)
        $helperFamilyIds = @(Get-ProcessFamilyIds $helperProcess.Id)
        if ((Get-ListenerCountForProcesses $helperFamilyIds) -ne 0) {
            throw "The candidate helper family unexpectedly owned a TCP listener."
        }
        $sessionBinding = Get-OpaqueRef "helper-session" ([string]$connected.computer.sessionId)
        Add-LifecycleEvent "computer-connected" "connected" "not-applicable" "not-applicable" -1
        $script:BaselineChrome = Get-BaselineChromeWindowBinding

        $epoch = Start-RecordedEpoch $script:EpochNames[0] $script:EpochSurfaces[0] "Select the existing stock Chrome window."
        Set-MutationDisposition $mutation "DedicatedWindow" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[0] @([ordered]@{ kind="key"; value="Control+N" }) "none" "Verify exactly one new stock Chrome window appeared without launch flags."
        [void](Bind-NewDedicatedChromeWindow)
        Set-MutationDisposition $mutation "DedicatedWindow" "verified_applied"
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[1] $script:EpochSurfaces[1] "Select only the new dedicated stock Chrome window."
        Invoke-RecordedAction $epoch $script:ActionNames[1] @(
            [ordered]@{ kind="key"; value="Control+L" },
            [ordered]@{ kind="typeText"; value="chrome://extensions" },
            [ordered]@{ kind="key"; value="Enter" }
        ) "none" "Verify chrome://extensions is visible in the dedicated window and no candidate card exists."
        $initialCandidateCard = Read-LiveCandidateCardState `
            $epoch "absent" "Exact v0.12.30 test-owned candidate card before installation"
        $capturedDeveloperMode = Read-LiveToggleState $epoch "DeveloperMode"
        Request-ScopedActionTimeApproval $epoch $preflight $capturedDeveloperMode $initialCandidateCard
        Confirm-ApprovalPreDispatchStateUnchanged `
            $epoch $capturedDeveloperMode $initialCandidateCard
        $developerSteps = if ($capturedDeveloperMode.value -ceq "disabled") {
            Set-MutationDisposition $mutation "DeveloperMode" "outcome_unknown"
            @([ordered]@{ kind="click"; label="Developer Mode toggle" })
        } else { @() }
        $developerConsent = if ($capturedDeveloperMode.value -ceq "disabled") { "developerModeChange" } else { "none" }
        Invoke-RecordedAction $epoch $script:ActionNames[2] $developerSteps $developerConsent "Verify the live-captured initial Developer Mode value was $($capturedDeveloperMode.value) and the required test state is enabled."
        if ($developerSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "DeveloperMode" "verified_applied"
        }
        [void](Get-ExactExtensionPayloadDigest $extensionDirectoryPath $payloadInventory $preflight.candidate.extension.combinedPayloadSha256)
        Set-MutationDisposition $mutation "CandidateExtension" "outcome_unknown"
        Set-MutationDisposition $mutation "NativePicker" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[3] @([ordered]@{ kind="click"; label="Load unpacked button" }) "installCandidate" "Verify Chrome's native Load unpacked picker opened."
        Set-MutationDisposition $mutation "NativePicker" "verified_applied"
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[2] $script:EpochSurfaces[2] "Select only Chrome's native Load unpacked file picker."
        Invoke-RecordedAction $epoch $script:ActionNames[4] @(
            [ordered]@{ kind="key"; value="Control+L" },
            [ordered]@{ kind="typeText"; value=$extensionDirectoryPath },
            [ordered]@{ kind="key"; value="Enter" },
            [ordered]@{ kind="click"; label="Select Folder button in Chrome's native picker" }
        ) "installCandidate" "Verify the helper explicitly invoked Select Folder for the exact new test-owned directory and the native picker closed." -ExpectTargetClosed
        Stop-RecordedEpoch $epoch -TargetClosed
        Set-MutationDisposition $mutation "NativePicker" "restored"
        [void](Get-ExactExtensionPayloadDigest $extensionDirectoryPath $payloadInventory $preflight.candidate.extension.combinedPayloadSha256)

        $epoch = Start-RecordedEpoch $script:EpochNames[3] $script:EpochSurfaces[3] "Reselect the dedicated stock Chrome extensions window."
        Invoke-RecordedAction $epoch $script:ActionNames[5] @() "none" "Verify exactly one enabled unpacked Local Browser Bridge v0.12.30 card, no duplicate, and no load error."
        Set-MutationDisposition $mutation "CandidateExtension" "verified_applied"
        Invoke-RecordedAction $epoch $script:ActionNames[6] @(
            [ordered]@{ kind="click"; label="Chrome Extensions menu button" },
            [ordered]@{ kind="click"; label="Local Browser Bridge entry in the Extensions menu" }
        ) "none" "Verify the Local Browser Bridge extension-owned popup opened."
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[4] $script:EpochSurfaces[4] "Select only the Local Browser Bridge extension-owned popup."
        $capturedFullAccess = Read-LiveToggleState $epoch "FullAccess"
        $capturedSavedToken = Read-LiveSavedTokenState $epoch
        $fullAccessSteps = if ($capturedFullAccess.value -ceq "disabled") {
            Set-MutationDisposition $mutation "FullAccess" "outcome_unknown"
            @([ordered]@{ kind="click"; label="Full Access toggle" })
        } else { @() }
        Invoke-RecordedAction $epoch $script:ActionNames[7] $fullAccessSteps "fullAccessUse" "Verify Full Access is enabled only for this bounded acceptance run."
        if ($fullAccessSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "FullAccess" "verified_applied"
        }
        Set-MutationDisposition $mutation "SavedToken" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[8] @(
            [ordered]@{ kind="click"; label="popup token field" },
            [ordered]@{ kind="typeText"; value=$script:Token },
            [ordered]@{ kind="click"; label="popup Connect button" }
        ) "acceptanceTokenSave" "Verify v0.12.30 is connected and the credential field is empty."
        Set-MutationDisposition $mutation "SavedToken" "verified_applied"
        Stop-RecordedEpoch $epoch

        [Environment]::SetEnvironmentVariable("LBB_TOKEN", $script:Token, "Process")
        $handoffOutput = @(& $runnerPath -Version $script:Version -Port $Port -PreflightRecord $preflightPath -OutputPath $script:MatrixOutputPath -PassThruOwnedTarget)
        if ($handoffOutput.Count -ne 1 -or $handoffOutput[0] -isnot [string]) {
            throw "The browser API matrix did not return exactly one in-memory owned-target JSON value."
        }
        try { $ownedTarget = ConvertFrom-JsonPreservingStrings ([string]$handoffOutput[0]) }
        catch { throw "The browser API matrix owned-target handoff was not JSON." }
        $stableMatrix = Read-StableJsonWithDigest $script:MatrixOutputPath "ApiMatrixRecord"
        $matrix = $stableMatrix.Value
        if ($matrix.version -cne $script:Version -or $matrix.passed -ne $true -or
            $matrix.candidateBinding.runNonce -cne $script:Binding.runNonce -or
            $matrix.candidateBinding.computerHelperSha256 -cne $script:Binding.computerHelperSha256) {
            throw "The API matrix did not pass against the exact candidate/helper/run."
        }
        $script:MatrixRecordSha256 = $stableMatrix.Sha256
        [Environment]::SetEnvironmentVariable("LBB_TOKEN", $script:Token, "Process")
        $browserAction = Show-DeterministicGreeting $ownedTarget

        $epoch = Start-RecordedEpoch $script:EpochNames[5] $script:EpochSurfaces[5] "Select the dedicated Chrome window containing chrome://extensions and the matrix-owned demo."
        Invoke-RecordedAction $epoch $script:ActionNames[9] @([ordered]@{ kind="click"; label="the exact chrome://extensions tab" }) "none" "Verify exactly one enabled unpacked Local Browser Bridge v0.12.30 card, no error, and Chrome's native debugger-use indicator while the exact bridge lease is active."
        Save-RecordedScreenshot $epoch "extension-loaded"
        Invoke-RecordedAction $epoch $script:ActionNames[10] @([ordered]@{ kind="click"; label="the exact matrix-owned loopback demo tab" }) "none" "Verify the exact visible result is Hello, Bridge Matrix. blue selected."
        Save-RecordedScreenshot $epoch "api-action-result"
        Invoke-RecordedAction $epoch $script:ActionNames[11] @([ordered]@{ kind="click"; label="Coordinate target button on the exact loopback demo" }) "none" "Verify the visible Action log says coordinate:true and the synthetic session pointer is present."
        Save-RecordedScreenshot $epoch "computer-share-action"
        Invoke-RecordedAction $epoch $script:ActionNames[12] @(
            [ordered]@{ kind="click"; label="visible in-page Local Browser Bridge Stop button" }
        ) "none" "Verify the page-owned control pill and Chrome debugger-use indicator disappeared after the visible Stop click."
        $stopMachine = Get-HumanPauseMachineProof ([long]$ownedTarget.tabId) "released_by_user"
        Invoke-RecordedAction $epoch $script:ActionNames[13] @(
            [ordered]@{ kind="click"; label="Chrome Extensions menu button" },
            [ordered]@{ kind="click"; label="Local Browser Bridge entry in the Extensions menu" }
        ) "none" "Verify the trusted Local Browser Bridge popup opened and visibly offers Resume remote control."
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[6] $script:EpochSurfaces[6] "Select only the trusted Local Browser Bridge popup paused by visible Stop."
        Save-RecordedScreenshot $epoch "stop-paused"
        Invoke-RecordedAction $epoch $script:ActionNames[14] @(
            [ordered]@{ kind="click"; label="trusted popup Resume remote control button after Stop" }
        ) "none" "Verify the trusted popup no longer reports remote control paused."
        $stopResume = Complete-TrustedPopupResume ([long]$ownedTarget.tabId)
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[7] $script:EpochSurfaces[7] "Reselect the exact matrix-owned demo in the dedicated Chrome window after Stop recovery."
        Invoke-RecordedAction $epoch $script:ActionNames[15] @() "none" "Verify Chrome's debugger-use indicator and the page-owned control pill returned after trusted-popup Resume and an explicit new lease."
        Invoke-RecordedAction $epoch $script:ActionNames[16] @(
            [ordered]@{ kind="click"; label="Chrome browser-owned debugger notice Cancel button" }
        ) "none" "Verify the browser-owned Cancel removed both the Chrome debugger-use indicator and page-owned control pill."
        $cancelMachine = Get-HumanPauseMachineProof ([long]$ownedTarget.tabId) "canceled_by_user"
        Invoke-RecordedAction $epoch $script:ActionNames[17] @(
            [ordered]@{ kind="click"; label="Chrome Extensions menu button" },
            [ordered]@{ kind="click"; label="Local Browser Bridge entry in the Extensions menu" }
        ) "none" "Verify the trusted Local Browser Bridge popup opened and visibly offers Resume remote control after Chrome Cancel."
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[8] $script:EpochSurfaces[8] "Select only the trusted Local Browser Bridge popup paused by Chrome Cancel."
        Save-RecordedScreenshot $epoch "cancel-paused"
        Invoke-RecordedAction $epoch $script:ActionNames[18] @(
            [ordered]@{ kind="click"; label="trusted popup Resume remote control button after Chrome Cancel" }
        ) "none" "Verify the trusted popup no longer reports remote control paused."
        $cancelResume = Complete-TrustedPopupResume ([long]$ownedTarget.tabId)
        Stop-RecordedEpoch $epoch

        $handback = [ordered]@{
            stop = [ordered]@{
                trigger = "in-page-stop"
                operatorSurface = "local-browser-bridge-computer-helper"
                statusPollMethod = $stopMachine.statusPollMethod
                statusPolledAfterTrigger = $stopMachine.statusPolledAfterTrigger
                reducedStatus = $stopMachine.reducedStatus
                controlStartRefusal = $stopMachine.controlStartRefusal
                tabMutationRefusal = $stopMachine.tabMutationRefusal
                indicatorsRemoved = $stopMachine.indicatorsRemoved
                resume = $stopResume
            }
            cancel = [ordered]@{
                trigger = "chrome-native-cancel"
                operatorSurface = "local-browser-bridge-computer-helper"
                statusPollMethod = $cancelMachine.statusPollMethod
                statusPolledAfterTrigger = $cancelMachine.statusPolledAfterTrigger
                reducedStatus = $cancelMachine.reducedStatus
                controlStartRefusal = $cancelMachine.controlStartRefusal
                tabMutationRefusal = $cancelMachine.tabMutationRefusal
                indicatorsRemoved = $cancelMachine.indicatorsRemoved
                resume = $cancelResume
            }
        }
        $stopMachine = $null; $stopResume = $null; $cancelMachine = $null; $cancelResume = $null

        $epoch = Start-RecordedEpoch $script:EpochNames[9] $script:EpochSurfaces[9] "Reselect the exact matrix-owned demo after Chrome Cancel recovery."
        Invoke-RecordedAction $epoch $script:ActionNames[19] @() "none" "Verify the active debugger-use indicator, page control pill, and deterministic demo state all recovered after the second trusted-popup Resume."
        Save-RecordedScreenshot $epoch "post-handback-resume"
        Invoke-RecordedAction $epoch $script:ActionNames[20] @(
            [ordered]@{ kind="click"; label="Chrome Extensions menu button" },
            [ordered]@{ kind="click"; label="Local Browser Bridge entry in the Extensions menu" }
        ) "none" "Verify the cleanup Local Browser Bridge popup opened."
        $stoppedLease = Invoke-BrowserCommand "browser.control.stop" @{}
        if ($stoppedLease.Body.result.active -eq $true) {
            throw "The browser lease remained active before credential cleanup."
        }
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[10] $script:EpochSurfaces[10] "Select only the Local Browser Bridge cleanup popup."
        Set-MutationDisposition $mutation "SavedToken" "outcome_unknown"
        Set-MutationDisposition $mutation "ClearTokenDialog" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[21] @([ordered]@{ kind="click"; label="Clear saved token button" }) "clearSavedTokenInitiate" "Verify the clear-token confirmation dialog appeared."
        Set-MutationDisposition $mutation "ClearTokenDialog" "verified_applied"
        Invoke-RecordedAction $epoch $script:ActionNames[22] @([ordered]@{ kind="click"; label="affirmative clear-token confirmation button" }) "clearSavedTokenConfirm" "Verify Not configured and the disabled Clear saved token button."
        Set-MutationDisposition $mutation "ClearTokenDialog" "restored"
        Set-MutationDisposition $mutation "SavedToken" "restored"
        $restoreFullSteps = if ($capturedFullAccess.value -ceq "disabled") {
            @([ordered]@{ kind="click"; label="Full Access toggle back to disabled" })
        } else { @() }
        if ($restoreFullSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "FullAccess" "outcome_unknown"
        }
        Invoke-RecordedAction $epoch $script:ActionNames[23] $restoreFullSteps "fullAccessUse" "Verify Full Access exactly equals its live-captured $($capturedFullAccess.value) value."
        if ($restoreFullSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "FullAccess" "restored"
        }
        Stop-RecordedEpoch $epoch

        $epoch = Start-RecordedEpoch $script:EpochNames[11] $script:EpochSurfaces[11] "Select only the dedicated test-owned stock Chrome window."
        Set-MutationDisposition $mutation "CandidateExtension" "outcome_unknown"
        Set-MutationDisposition $mutation "RemoveExtensionDialog" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[24] @(
            [ordered]@{ kind="click"; label="the exact existing chrome://extensions tab" },
            [ordered]@{ kind="click"; label="Remove on the exact v0.12.30 test-owned candidate card" },
            [ordered]@{ kind="click"; label="confirm removal of that exact candidate card" }
        ) "extensionDisposition" "Verify the helper switched to the protected chrome://extensions tab before removing only the new test-owned v0.12.30 card."
        $finalCandidateCard = Read-LiveCandidateCardState `
            $epoch "absent" "Exact v0.12.30 test-owned candidate card after removal"
        Set-MutationDisposition $mutation "RemoveExtensionDialog" "restored"
        Set-MutationDisposition $mutation "CandidateExtension" "restored"
        $restoreDeveloperSteps = if ($capturedDeveloperMode.value -ceq "disabled") {
            @([ordered]@{ kind="click"; label="Developer Mode toggle back to disabled" })
        } else { @() }
        $restoreDeveloperConsent = if ($capturedDeveloperMode.value -ceq "disabled") { "developerModeChange" } else { "none" }
        if ($restoreDeveloperSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "DeveloperMode" "outcome_unknown"
        }
        Invoke-RecordedAction $epoch $script:ActionNames[25] $restoreDeveloperSteps $restoreDeveloperConsent "Verify Developer Mode exactly equals its live-captured $($capturedDeveloperMode.value) value."
        if ($restoreDeveloperSteps.Count -ne 0) {
            Set-MutationDisposition $mutation "DeveloperMode" "restored"
        }
        Stop-RecordedEpoch $epoch
        [void](Get-ExactExtensionPayloadDigest $extensionDirectoryPath $payloadInventory $preflight.candidate.extension.combinedPayloadSha256)

        $epoch = Start-RecordedEpoch $script:EpochNames[12] $script:EpochSurfaces[12] "Reselect only the dedicated test-owned stock Chrome window for closure."
        Set-MutationDisposition $mutation "DedicatedWindow" "outcome_unknown"
        Invoke-RecordedAction $epoch $script:ActionNames[26] @([ordered]@{ kind="key"; value="Control+Shift+W" }) "none" "Verify only the dedicated test-owned Chrome window closed." -ExpectTargetClosed
        Stop-RecordedEpoch $epoch -TargetClosed
        Set-MutationDisposition $mutation "DedicatedWindow" "restored"

        Add-LifecycleEvent "helper-owner-forced-termination-requested" "connected" $helperProcessRef $preflight.candidate.computerHelper.sha256 -1
        $helperFamilyIds = @(Get-ProcessFamilyIds $helperProcess.Id)
        Stop-Process -Id $helperProcess.Id -Force -ErrorAction Stop
        if (-not $helperProcess.WaitForExit(10000)) {
            throw "The exact candidate helper supervisor did not terminate within ten seconds."
        }
        $helperExitCode = [int]$helperProcess.ExitCode
        if ($helperExitCode -eq 0) {
            throw "The owner-forced helper termination unexpectedly reported a graceful zero exit."
        }
        Add-LifecycleEvent "helper-owner-forced-terminated" "connected" $helperProcessRef $preflight.candidate.computerHelper.sha256 $helperExitCode
        [void](Wait-BridgeState { param($state) $state.computerConnected -ne $true } "helper disconnection after exact supervisor termination")
        Add-LifecycleEvent "computer-disconnected-after-helper-termination" "disconnected" "not-applicable" "not-applicable" -1
        Wait-NoExactImageProcess $helperPath "exact helper supervisor termination"
        if ((Get-ListenerCountForProcesses $helperFamilyIds) -ne 0) {
            throw "The exact helper supervisor family retained a listener."
        }
        if ((Get-Sha256 $helperPath) -cne $preflight.candidate.computerHelper.sha256) {
            throw "The candidate helper executable changed during the run."
        }

        Stop-Process -Id $script:ServerProcess.Id -Force -ErrorAction Stop
        if (-not $script:ServerProcess.WaitForExit(10000)) {
            throw "The exact candidate server did not terminate within ten seconds."
        }
        $serverExitCode = [int]$script:ServerProcess.ExitCode
        Wait-CanonicalPortReleased
        if ((Get-Sha256 $serverPath) -cne $preflight.candidate.server.sha256) {
            throw "The candidate server executable changed during the run."
        }
        Add-LifecycleEvent "server-owner-forced-terminated" "disconnected" $serverProcessRef $preflight.candidate.server.sha256 $serverExitCode

        Remove-OperatorExchangeScratch

        if ($script:Lifecycle.Count -ne 8 -or $script:Epochs.Count -ne 13 -or
            $script:Actions.Count -ne 27 -or $script:ScreenshotRecords.Count -ne 6 -or
            $null -eq $handback -or $initialCandidateCard.present -ne $false -or
            $finalCandidateCard.present -ne $false) {
            throw "The live helper chain is incomplete."
        }
        $finalStableMatrix = Read-StableJsonWithDigest $script:MatrixOutputPath "ApiMatrixRecord final binding"
        if ($finalStableMatrix.Sha256 -cne $script:MatrixRecordSha256) {
            throw "The API matrix record changed after its exact stable binding."
        }
        $finishedAt = [DateTimeOffset]::UtcNow
        if ([String]::IsNullOrWhiteSpace([string]$script:ApprovalId) -or
            $script:FirstCoveredActionSequence -le 0 -or
            [String]::IsNullOrWhiteSpace([string]$script:FirstCoveredActionDispatchedAtUtc) -or
            -not $script:ApprovalConsumedBeforeExpiry -or
            [String]::IsNullOrWhiteSpace([string]$script:ApprovalChallengeFrameRef) -or
            [String]::IsNullOrWhiteSpace([string]$script:ApprovalPreDispatchFrameRef) -or
            [String]::IsNullOrWhiteSpace([string]$script:ApprovalPreDispatchDecisionRef) -or
            [String]::IsNullOrWhiteSpace([string]$script:ApprovalPreDispatchVerifiedAtUtc) -or
            [String]::IsNullOrWhiteSpace([string]$script:ReviewerSessionRef) -or
            -not $script:OperatorExchangeScratchDeleted) {
            throw "The scoped approval or independent operator exchange did not complete."
        }
        $approvalConfirmedAt = [DateTimeOffset]::ParseExact(
            $script:ApprovalConfirmedAtUtc, "o", [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind
        )
        $firstCoveredAt = [DateTimeOffset]::ParseExact(
            $script:FirstCoveredActionDispatchedAtUtc, "o", [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind
        )
        if ($approvalConfirmedAt -ge $firstCoveredAt) {
            throw "The scoped approval was not consumed before the first covered action."
        }
        $approvalExpiresAt = [DateTimeOffset]::ParseExact(
            $script:ApprovalExpiresAtUtc, "o", [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind
        )
        if ($firstCoveredAt -gt $approvalExpiresAt) {
            throw "The scoped approval expired before the first covered action dispatch."
        }
        $approvalRecord = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-scoped-action-approval"
            releaseCandidateBinding = $script:ReleaseCandidateBinding
            candidateBinding = $script:Binding
            approvalId = $script:ApprovalId
            request = $script:ApprovalRequest
            response = $script:ApprovalResponse
            consumption = [ordered]@{
                consumedBeforeFirstCoveredAction = $true
                consumedBeforeExpiry = $true
                preDispatchFrameRef = $script:ApprovalPreDispatchFrameRef
                preDispatchDecisionRef = $script:ApprovalPreDispatchDecisionRef
                preDispatchVerifiedAtUtc = $script:ApprovalPreDispatchVerifiedAtUtc
                freshStateRevalidatedAfterApproval = $true
                scopeUnchangedThroughRun = $true
                replayed = $false
                cleanupAuthoritySurvivesFailure = $true
            }
        }
        $approvalRecordSha256 = $null
        Write-CreateOnceJson $script:ApprovalOutputPath $approvalRecord `
            "scoped action-time approval record" ([ref]$approvalRecordSha256)
        $record = [ordered]@{
            schemaVersion = 2
            evidenceType = "stock-user-chrome-computer-helper-chain"
            version = $script:Version
            releaseCandidateBinding = $script:ReleaseCandidateBinding
            candidateBinding = $script:Binding
            passed = $true
            recordedBy = [ordered]@{
                name = [IO.Path]::GetFileName($PSCommandPath)
                sha256 = Get-Sha256 $PSCommandPath
                source = "candidate-final-sha-blob"
            }
            run = [ordered]@{
                runNonce = $script:Binding.runNonce
                preflightRecordSha256 = $script:Binding.preflightRecordSha256
                apiMatrixRecordSha256 = $script:MatrixRecordSha256
                startedAtUtc = Format-CanonicalUtc $startedAt
                finishedAtUtc = Format-CanonicalUtc $finishedAt
            }
            server = [ordered]@{
                executableName = $preflight.candidate.server.name
                sha256 = $preflight.candidate.server.sha256
                processRef = $serverProcessRef
                soleListener = "127.0.0.1:17373"
                updateCheckDisabled = $true
            }
            helper = [ordered]@{
                executableName = $preflight.candidate.computerHelper.name
                sha256 = $preflight.candidate.computerHelper.sha256
                connectedThroughLoopbackServer = $true
                serverApiOnly = $true
                processRef = $helperProcessRef
                sessionBindingSha256 = $sessionBinding
                rawSessionIdentifierRetained = $false
                topology = $helperTopology
            }
            extensionPayload = [ordered]@{
                fileCount = 11
                combinedPayloadSha256 = $script:Binding.extractedPayloadSha256
                verifiedBeforeLoad = $true
                verifiedAfterLoad = $true
                verifiedAfterCleanup = $true
            }
            operatorExchange = [ordered]@{
                protocolVersion = 1
                executorSessionRef = $script:ExecutorRef
                reviewerSessionRef = $script:ReviewerSessionRef
                independentSessionBoundary = $true
                requestCount = $script:OperatorRequestCount
                statusDecisionCount = $script:StatusDecisionCount
                freshFrameDecisionCount = $script:FreshFrameDecisionCount
                allRequestsCreateOnce = $true
                allResponsesCreateOnce = $true
                everyFrameDependentDecisionBoundToFreshFrame = $true
                everyStatusDecisionBoundToFreshStatus = $true
                responseChainSha256 = $script:OperatorResponseChainSha256
                scratchDeleted = $true
            }
            scopedActionApproval = [ordered]@{
                recordSha256 = $approvalRecordSha256
                approvalId = $script:ApprovalId
                scopeSha256 = $script:ApprovalScopeSha256
                executorSessionRef = $script:ExecutorRef
                approvalConfirmedAtUtc = $script:ApprovalConfirmedAtUtc
                approvalExpiresAtUtc = $script:ApprovalExpiresAtUtc
                approvalChallengeFrameRef = $script:ApprovalChallengeFrameRef
                preDispatchFrameRef = $script:ApprovalPreDispatchFrameRef
                preDispatchDecisionRef = $script:ApprovalPreDispatchDecisionRef
                preDispatchVerifiedAtUtc = $script:ApprovalPreDispatchVerifiedAtUtc
                firstCoveredActionSequence = $script:FirstCoveredActionSequence
                firstCoveredActionDispatchedAtUtc = $script:FirstCoveredActionDispatchedAtUtc
                consumedBeforeFirstCoveredAction = $true
                consumedBeforeExpiry = $true
                freshStateRevalidatedAfterApproval = $true
                scopeUnchangedThroughRun = $true
            }
            initialState = [ordered]@{
                capturedFromFreshHelperFrames = $true
                developerMode = $capturedDeveloperMode
                fullAccess = $capturedFullAccess
                savedToken = $capturedSavedToken
                candidateCard = $initialCandidateCard
            }
            windowBinding = [ordered]@{
                application = "google-chrome"
                baselineChromeWindowCount = $script:BaselineChrome.Count
                baselineTargetSetSha256 = $script:BaselineChrome.Digest
                dedicatedTargetRef = $script:DedicatedTargetRef
                dedicatedProcessRef = $script:DedicatedProcessRef
                dedicatedAbsentBeforeCreation = $script:DedicatedAbsentBeforeCreation
                dedicatedCreatedAsOnlyNewChromeWindow = $script:DedicatedCreatedAsOnlyNewChromeWindow
                sameDedicatedTargetAcrossEpochs = $true
            }
            lifecycle = $script:Lifecycle
            windowEpochs = $script:Epochs
            actions = $script:Actions
            browserAction = $browserAction
            handback = $handback
            screenshots = $script:ScreenshotRecords
            cleanup = [ordered]@{
                allSharesStopped = $true
                helperTerminationDisposition = "owner-forced-exact-supervisor"
                helperExitCode = $helperExitCode
                helperDisconnectedAfterTermination = $true
                helperChildrenRemaining = 0
                helperListenersRemaining = 0
                exactHelperImageProcessesRemaining = 0
                serverTerminationDisposition = "owner-forced-exact-process"
                serverExitCode = $serverExitCode
                serverListenersRemaining = 0
                canonicalPortListenersRemaining = 0
                candidateExtensionRemoved = $true
                candidateCardAbsentAfterRemoval = $true
                candidateCardAbsenceVerifiedFromFreshLiveUi = $true
                savedTokenCleared = $true
                developerModeRestored = $true
                fullAccessRestored = $true
                testWindowClosed = $true
                helperExecutableUnchanged = $true
                serverExecutableUnchanged = $true
                extensionPayloadUnchanged = $true
                rawIdentifiersCleared = $true
            }
            privacy = [ordered]@{
                rawWindowIdsRetained = $false
                rawFrameIdsRetained = $false
                rawShareIdsRetained = $false
                rawSessionIdsRetained = $false
                rawTabIdsRetained = $false
                credentialRetained = $false
                apiResponseBodiesRetained = $false
                opaqueReferenceMapDiscarded = $true
            }
        }
        $serialized = $record | ConvertTo-Json -Depth 30 -Compress
        if ([regex]::IsMatch($serialized, '(?i)"(?:windowId|frameId|shareId|sessionId|tabId|token|authorization|bearer)"\s*:')) {
            throw "The live helper record retained a raw identifier or secret field."
        }
    }
    catch {
        $primaryFailure = $_
    }
    finally {
        $possibleUiMutation = @(Get-UnresolvedMutationNames $mutation).Count -ne 0
        if ($null -ne $primaryFailure -and $possibleUiMutation) {
            if ($null -ne $helperProcess -and -not $helperProcess.HasExited -and
                $null -ne $script:ServerProcess -and -not $script:ServerProcess.HasExited) {
                try {
                    $state = Get-BridgeState
                    if ($state.computerConnected -eq $true) {
                        foreach ($rollbackError in @(
                            Invoke-BestEffortUiRollback $mutation $capturedDeveloperMode $capturedFullAccess $capturedSavedToken
                        )) {
                            $cleanupErrors.Add($rollbackError)
                        }
                        $unresolved = @(Get-UnresolvedMutationNames $mutation)
                        if ($unresolved.Count -ne 0) {
                            $cleanupErrors.Add("UI rollback left exact mutation dispositions unresolved ($($unresolved -join ', ')); inspect only the dedicated test Chrome window, candidate card, Developer Mode, Full Access, and saved-token state.")
                        }
                    }
                    else {
                        $cleanupErrors.Add("UI rollback was unavailable because the exact packaged helper was disconnected; inspect the dedicated test Chrome window, candidate card, Developer Mode, Full Access, and saved-token state.")
                    }
                }
                catch { $cleanupErrors.Add("rollback: $($_.Exception.Message)") }
            }
            else {
                $cleanupErrors.Add("UI rollback was unavailable because the exact helper/server transport was not alive; inspect the dedicated test Chrome window, candidate card, Developer Mode, Full Access, and saved-token state.")
            }
        }
        if ($null -ne $helperProcess) {
            try {
                $ownedHelperIds = @([int]$helperProcess.Id)
                if ($script:BoundHelperWorkerPid -gt 0) {
                    $ownedHelperIds += [int]$script:BoundHelperWorkerPid
                }
                $ownedHelperIds = @($ownedHelperIds | Sort-Object -Unique)
                $exactRows = @(Get-ExactImageProcessRows $helperPath | Where-Object {
                    $ownedHelperIds -contains [int]$_.ProcessId
                })
                foreach ($row in $exactRows) {
                    Stop-Process -Id ([int]$row.ProcessId) -Force -ErrorAction Stop
                }
                if (-not $helperProcess.HasExited -and -not $helperProcess.WaitForExit(10000)) {
                    throw "helper termination timeout"
                }
                if (@(Get-Process -Id $ownedHelperIds -ErrorAction SilentlyContinue).Count -ne 0) {
                    throw "an ownership-bound helper process remained"
                }
            }
            catch { $cleanupErrors.Add("helper: $($_.Exception.Message)") }
        }
        try { Wait-NoExactImageProcess $helperPath "failure/success cleanup" }
        catch { $cleanupErrors.Add("helper-rescan: $($_.Exception.Message)") }
        if ($null -ne $script:ServerProcess -and -not $script:ServerProcess.HasExited) {
            try {
                Stop-Process -Id $script:ServerProcess.Id -Force -ErrorAction Stop
                if (-not $script:ServerProcess.WaitForExit(10000)) { throw "server termination timeout" }
            }
            catch { $cleanupErrors.Add("server: $($_.Exception.Message)") }
        }
        try { Wait-CanonicalPortReleased }
        catch { $cleanupErrors.Add("listener-rescan: $($_.Exception.Message)") }
        try { Remove-OperatorExchangeScratch }
        catch { $cleanupErrors.Add("operator-exchange: $($_.Exception.Message)") }
        try {
            if ($tokenWasPresent) { [Environment]::SetEnvironmentVariable("LBB_TOKEN", $previousToken, "Process") }
            else { [Environment]::SetEnvironmentVariable("LBB_TOKEN", $null, "Process") }
            if ($portWasPresent) { [Environment]::SetEnvironmentVariable("LBB_PORT", $previousPort, "Process") }
            else { [Environment]::SetEnvironmentVariable("LBB_PORT", $null, "Process") }
        }
        catch { $cleanupErrors.Add("environment: $($_.Exception.Message)") }

        $script:Token = $null
        if ($null -ne $credential) { [Array]::Clear($credential, 0, $credential.Length) }
        $ownedTarget = $null
        $sessionBinding = $null
        $helperTopology = $null
        $script:BoundHelperWorkerPid = 0
        $script:ActiveEpoch = $null

        if ($null -ne $primaryFailure) {
            try {
                $rawEntries = @([IO.DirectoryInfo]::new($script:RawDirectory).GetFileSystemInfos())
                $allowedRawNames = @($script:Screenshots.Values)
                if (@($rawEntries | Where-Object {
                    $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
                    $allowedRawNames -cnotcontains $_.Name
                }).Count -ne 0) {
                    throw "raw screenshot scratch contains an unexpected or linked entry"
                }
                foreach ($entry in $rawEntries) { [IO.File]::Delete($entry.FullName) }
                foreach ($path in @($script:MatrixOutputPath, $script:ApprovalOutputPath, $outputPath)) {
                    if ([IO.File]::Exists($path)) {
                        $file = [IO.FileInfo]::new($path)
                        if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                            throw "failure artifact is a reparse point"
                        }
                        [IO.File]::Delete($path)
                    }
                }
            }
            catch { $cleanupErrors.Add("artifacts: $($_.Exception.Message)") }
        }
    }

    if ($null -ne $primaryFailure) {
        $suffix = if ($cleanupErrors.Count -eq 0) { "Failure rollback completed." }
        else { "Failure rollback was incomplete: " + ($cleanupErrors -join "; ") }
        throw "$($primaryFailure.Exception.Message) $suffix"
    }
    if ($cleanupErrors.Count -ne 0) {
        throw "The acceptance run completed but shell cleanup failed: $($cleanupErrors -join '; ')"
    }
    if ($null -eq $record) {
        throw "The acceptance run produced no final machine record."
    }

    $temporary = "$outputPath.new"
    try {
        $recordBytes = $script:Utf8NoBom.GetBytes(
            (($record | ConvertTo-Json -Depth 30) + [Environment]::NewLine)
        )
        $recordStream = [IO.File]::Open(
            $temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
        )
        try {
            $recordStream.Write($recordBytes, 0, $recordBytes.Length)
            $recordStream.Flush($true)
        }
        finally {
            $recordStream.Dispose()
            [Array]::Clear($recordBytes, 0, $recordBytes.Length)
        }
        [IO.File]::Move($temporary, $outputPath)
    }
    catch {
        $finalWriteFailure = $_
        try {
            Remove-CanonicalRawScreenshots $script:RawDirectory
        }
        catch {
            throw "$($finalWriteFailure.Exception.Message) Final-record failure cleanup was incomplete: $($_.Exception.Message)"
        }
        throw "$($finalWriteFailure.Exception.Message) Canonical raw screenshot cleanup completed."
    }
    finally {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
    }
    Write-Output "Live candidate-bound computer-helper chain record created."
}
function Invoke-SelfTest {
    $bad = [ordered]@{ windowId = 42 } | ConvertTo-Json -Compress
    if (-not [regex]::IsMatch($bad, '(?i)"(?:windowId|frameId|shareId|sessionId|tabId|token|authorization|bearer)"\s*:')) {
        throw "Computer-helper recorder raw-identifier self-test failed."
    }
    $hexRejected = $false
    try { Assert-Hex "not-a-digest" 64 "self-test digest" } catch { $hexRejected = $true }
    if (-not $hexRejected) { throw "Computer-helper recorder digest self-test failed." }
    if ($script:Screenshots.Count -ne 6 -or $script:EpochNames.Count -ne 13 -or
        $script:ActionNames.Count -ne 27 -or
        @($script:Screenshots.Values | Select-Object -Unique).Count -ne 6) {
        throw "Computer-helper recorder canonical sequence self-test failed."
    }
    $mutationSelfTest = @{
        DedicatedWindow = "not_attempted"; DeveloperMode = "not_attempted"
        CandidateExtension = "not_attempted"; FullAccess = "not_attempted"; SavedToken = "not_attempted"
        NativePicker = "not_attempted"; ClearTokenDialog = "not_attempted"
        RemoveExtensionDialog = "not_attempted"
    }
    Set-MutationDisposition $mutationSelfTest "DedicatedWindow" "outcome_unknown"
    if (-not (Test-UnresolvedMutation $mutationSelfTest "DedicatedWindow")) {
        throw "Computer-helper recorder unresolved-mutation self-test failed."
    }
    Set-MutationDisposition $mutationSelfTest "DedicatedWindow" "verified_applied"
    Set-MutationDisposition $mutationSelfTest "DedicatedWindow" "restored"
    if (Test-UnresolvedMutation $mutationSelfTest "DedicatedWindow") {
        throw "Computer-helper recorder restored-mutation self-test failed."
    }
    foreach ($modalName in @("NativePicker", "ClearTokenDialog", "RemoveExtensionDialog")) {
        Set-MutationDisposition $mutationSelfTest $modalName "outcome_unknown"
        Set-MutationDisposition $mutationSelfTest $modalName "verified_applied"
        Set-MutationDisposition $mutationSelfTest $modalName "restored"
        Set-MutationDisposition $mutationSelfTest $modalName "outcome_unknown"
        if (-not (Test-UnresolvedMutation $mutationSelfTest $modalName)) {
            throw "Computer-helper recorder failed the injected modal-failure transition for $modalName."
        }
        Set-MutationDisposition $mutationSelfTest $modalName "restored"
    }
    $baselineRollbackRefs = @(
        [String]::new([char]"1", 64), [String]::new([char]"2", 64)
    )
    $sameRollbackBindings = @($baselineRollbackRefs | ForEach-Object {
        [pscustomobject]@{ TargetRef = $_; Window = [pscustomobject]@{ id = $_; pid = 1 } }
    })
    if ($null -ne (Resolve-SoleNewDedicatedRollbackBinding `
        $baselineRollbackRefs $sameRollbackBindings)) {
        throw "Computer-helper recorder no-new-window rollback delta self-test failed."
    }
    $soleNewRef = [String]::new([char]"3", 64)
    $soleNewBindings = @($sameRollbackBindings) + [pscustomobject]@{
        TargetRef = $soleNewRef; Window = [pscustomobject]@{ id = "new"; pid = 2 }
    }
    if ((Resolve-SoleNewDedicatedRollbackBinding `
        $baselineRollbackRefs $soleNewBindings).TargetRef -cne $soleNewRef) {
        throw "Computer-helper recorder sole-new-window rollback delta self-test failed."
    }
    $multipleNewRejected = $false
    try {
        [void](Resolve-SoleNewDedicatedRollbackBinding `
            $baselineRollbackRefs ($soleNewBindings + [pscustomobject]@{
                TargetRef = [String]::new([char]"4", 64)
                Window = [pscustomobject]@{ id = "other"; pid = 3 }
            }))
    }
    catch { $multipleNewRejected = $true }
    if (-not $multipleNewRejected) {
        throw "Computer-helper recorder accepted an ambiguous multi-window rollback delta."
    }
    $invalidDispositionRejected = $false
    try { Set-MutationDisposition $mutationSelfTest "DedicatedWindow" "assumed_restored" }
    catch { $invalidDispositionRejected = $true }
    if (-not $invalidDispositionRejected) {
        throw "Computer-helper recorder invalid-mutation-disposition self-test failed."
    }
    $invalidJumpRejected = $false
    try { Set-MutationDisposition $mutationSelfTest "DeveloperMode" "restored" }
    catch { $invalidJumpRejected = $true }
    if (-not $invalidJumpRejected) {
        throw "Computer-helper recorder invalid-mutation-transition self-test failed."
    }
    $allowedEnum = [pscustomobject]@{ type = "enum"; values = @("enabled", "disabled") }
    Assert-OperatorDecision "ui-state" ([pscustomobject]@{ value = "enabled" }) $allowedEnum
    Assert-OperatorDecision "ui-point" ([pscustomobject]@{ x = 10; y = 20 }) ([pscustomobject]@{})
    Assert-OperatorDecision "ui-verification" ([pscustomobject]@{ passed = $true }) ([pscustomobject]@{})
    Assert-OperatorDecision "scoped-user-approval" ([pscustomobject]@{
        approved = $true; approvedBy = "user"; confirmationMode = "batched-action-time"
    }) ([pscustomobject]@{})
    foreach ($invalidDecision in @(
        [pscustomobject]@{ Kind = "ui-state"; Decision = [pscustomobject]@{ value = "unknown" } },
        [pscustomobject]@{ Kind = "ui-state"; Decision = [pscustomobject]@{ value = "enabled"; human = $true } },
        [pscustomobject]@{ Kind = "ui-point"; Decision = [pscustomobject]@{ x = "10"; y = 20.0 } },
        [pscustomobject]@{ Kind = "ui-point"; Decision = [pscustomobject]@{ x = 10.5; y = 20 } },
        [pscustomobject]@{ Kind = "ui-verification"; Decision = [pscustomobject]@{ passed = "true" } },
        [pscustomobject]@{ Kind = "ui-state"; Decision = [pscustomobject]@{ unable = $false } },
        [pscustomobject]@{ Kind = "ui-state"; Decision = [pscustomobject]@{ unable = $true; value = "enabled" } },
        [pscustomobject]@{ Kind = "scoped-user-approval"; Decision = [pscustomobject]@{
            approved = $true; approvedBy = "agent"; confirmationMode = "batched-action-time"; receipt = "human"
        } }
    )) {
        $decisionRejected = $false
        try {
            if (-not (Test-UnableOperatorDecision $invalidDecision.Decision)) {
                Assert-OperatorDecision $invalidDecision.Kind $invalidDecision.Decision $allowedEnum
            }
        }
        catch { $decisionRejected = $true }
        if (-not $decisionRejected) {
            throw "Computer-helper recorder accepted an invalid strict operator decision."
        }
    }
    if (-not (Test-UnableOperatorDecision ([pscustomobject]@{ unable = $true }))) {
        throw "Computer-helper recorder did not recognize the universal unable decision."
    }
    $envelopeRequestId = [String]::new([char]"1", 32)
    $envelopeRequestSha = [String]::new([char]"2", 64)
    $envelopeCandidateSha = [String]::new([char]"3", 64)
    $envelopeInputSha = [String]::new([char]"4", 64)
    $envelopeExecutorRef = [String]::new([char]"5", 64)
    $envelopeReviewerRef = [String]::new([char]"6", 64)
    $envelopeOrchestratorRef = [String]::new([char]"7", 64)
    $goodEnvelope = [pscustomobject][ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-operator-response"
        requestId = $envelopeRequestId
        requestSha256 = $envelopeRequestSha
        candidateBindingSha256 = $envelopeCandidateSha
        inputDigestSha256 = $envelopeInputSha
        responderKind = "independent-agent"
        responderSessionRef = $envelopeReviewerRef
        respondedAtUtc = "2026-08-24T00:00:01.0000000+00:00"
        decision = [pscustomobject]@{ value = "fixture" }
    }
    Assert-OperatorResponseEnvelope $goodEnvelope $envelopeRequestId $envelopeRequestSha `
        $envelopeCandidateSha $envelopeInputSha "independent-agent" `
        $envelopeExecutorRef $null $envelopeOrchestratorRef
    foreach ($envelopeMutation in @(
        "request-replay", "input-swap", "same-session", "reviewer-change", "old-human-field"
    )) {
        $invalidEnvelope = ConvertFrom-JsonPreservingStrings ($goodEnvelope | ConvertTo-Json -Depth 8)
        $existingReviewer = $null
        switch ($envelopeMutation) {
            "request-replay" { $invalidEnvelope.requestSha256 = [String]::new([char]"8", 64) }
            "input-swap" { $invalidEnvelope.inputDigestSha256 = [String]::new([char]"9", 64) }
            "same-session" { $invalidEnvelope.responderSessionRef = $envelopeExecutorRef }
            "reviewer-change" { $existingReviewer = $envelopeOrchestratorRef }
            "old-human-field" {
                $invalidEnvelope | Add-Member -NotePropertyName humanReviewed -NotePropertyValue $true
            }
        }
        $envelopeRejected = $false
        try {
            Assert-OperatorResponseEnvelope $invalidEnvelope $envelopeRequestId $envelopeRequestSha `
                $envelopeCandidateSha $envelopeInputSha "independent-agent" `
                $envelopeExecutorRef $existingReviewer $envelopeOrchestratorRef
        }
        catch { $envelopeRejected = $true }
        if (-not $envelopeRejected) {
            throw "Computer-helper recorder accepted $envelopeMutation in an operator response."
        }
    }
    $approvalEnvelope = ConvertFrom-JsonPreservingStrings ($goodEnvelope | ConvertTo-Json -Depth 8)
    $approvalEnvelope.responderKind = "user-via-orchestrator"
    $approvalEnvelope.responderSessionRef = $envelopeOrchestratorRef
    Assert-OperatorResponseEnvelope $approvalEnvelope $envelopeRequestId $envelopeRequestSha `
        $envelopeCandidateSha $envelopeInputSha "user-via-orchestrator" `
        $envelopeExecutorRef $envelopeReviewerRef $envelopeOrchestratorRef
    $wrongOrchestratorRejected = $false
    try {
        $approvalEnvelope.responderSessionRef = [String]::new([char]"a", 64)
        Assert-OperatorResponseEnvelope $approvalEnvelope $envelopeRequestId $envelopeRequestSha `
            $envelopeCandidateSha $envelopeInputSha "user-via-orchestrator" `
            $envelopeExecutorRef $envelopeReviewerRef $envelopeOrchestratorRef
    }
    catch { $wrongOrchestratorRejected = $true }
    if (-not $wrongOrchestratorRejected) {
        throw "Computer-helper recorder accepted approval from a non-attestor orchestrator session."
    }
    $approvalSessionCollisionRejected = $false
    try {
        $approvalEnvelope.responderSessionRef = $envelopeReviewerRef
        Assert-OperatorResponseEnvelope $approvalEnvelope $envelopeRequestId $envelopeRequestSha `
            $envelopeCandidateSha $envelopeInputSha "user-via-orchestrator" `
            $envelopeExecutorRef $envelopeReviewerRef $envelopeOrchestratorRef
    }
    catch { $approvalSessionCollisionRejected = $true }
    if (-not $approvalSessionCollisionRejected) {
        throw "Computer-helper recorder accepted reviewer-authored user approval."
    }
    $timestampStart = [DateTimeOffset]::ParseExact(
        "2026-08-24T00:00:00.0000000Z", "o", [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::RoundtripKind
    )
    [void](Assert-FreshCanonicalResponseTimestamp `
        "2026-08-24T00:00:01.0000000Z" $timestampStart ($timestampStart.AddSeconds(2)) `
        "self-test canonical response timestamp")
    $offsetTimestampRejected = $false
    try {
        [void](Assert-FreshCanonicalResponseTimestamp `
            "2026-08-24T00:00:01.0000000+00:00" $timestampStart ($timestampStart.AddSeconds(2)) `
            "self-test offset response timestamp")
    }
    catch { $offsetTimestampRejected = $true }
    if (-not $offsetTimestampRejected) {
        throw "Computer-helper recorder accepted a noncanonical +00:00 response timestamp."
    }
    $savedExchange = $script:OperatorExchange
    $savedExchangeArtifacts = $script:OperatorExchangeArtifacts
    $savedReservations = $script:OperatorResponseReservations
    $savedExpectedTransients = $script:OperatorExpectedTransientArtifacts
    $publicationDirectory = [IO.Path]::Combine(
        [IO.Path]::GetTempPath(), "lbb-operator-publication-" + [Guid]::NewGuid().ToString("N")
    )
    [IO.Directory]::CreateDirectory($publicationDirectory) | Out-Null
    $script:OperatorExchange = $publicationDirectory
    $script:OperatorExchangeArtifacts = New-Object Collections.Generic.List[string]
    $script:OperatorResponseReservations = New-Object Collections.Generic.List[object]
    $script:OperatorExpectedTransientArtifacts = New-Object Collections.Generic.List[string]
    try {
        $publicationId = [String]::new([char]"a", 32)
        $publicationResponse = [IO.Path]::Combine(
            $publicationDirectory, "response-$publicationId.json"
        )
        $publicationTemporary = "$publicationResponse.new"
        $publicationClaimed = [IO.Path]::Combine(
            $publicationDirectory, "response-$publicationId.claimed.json"
        )
        $partial = [IO.File]::Open(
            $publicationTemporary, [IO.FileMode]::CreateNew,
            [IO.FileAccess]::Write, [IO.FileShare]::None
        )
        try {
            $partialBytes = [Text.Encoding]::ASCII.GetBytes('{"partial":')
            $partial.Write($partialBytes, 0, $partialBytes.Length)
            $partial.Flush($true)
        }
        finally { $partial.Dispose() }
        $partialRejected = $false
        try {
            Claim-PublishedOperatorResponse $publicationResponse $publicationClaimed `
                ([DateTimeOffset]::UtcNow.AddMilliseconds(250))
        }
        catch { $partialRejected = $true }
        if (-not $partialRejected -or [IO.File]::Exists($publicationClaimed)) {
            throw "Computer-helper recorder accepted a partial response publication."
        }
        [IO.File]::Delete($publicationTemporary)
        $expiredId = [String]::new([char]"b", 32)
        $expiredResponse = [IO.Path]::Combine(
            $publicationDirectory, "response-$expiredId.json"
        )
        $expiredClaimed = [IO.Path]::Combine(
            $publicationDirectory, "response-$expiredId.claimed.json"
        )
        [IO.File]::WriteAllText($expiredResponse, "{}`n", $script:Utf8NoBom)
        $expiredPublicationRejected = $false
        try {
            Claim-PublishedOperatorResponse $expiredResponse $expiredClaimed `
                ([DateTimeOffset]::UtcNow.AddSeconds(-1))
        }
        catch { $expiredPublicationRejected = $true }
        if (-not $expiredPublicationRejected -or [IO.File]::Exists($expiredClaimed) -or
            -not [IO.File]::Exists($expiredResponse)) {
            throw "Computer-helper recorder accepted an already-published expired response."
        }
        [IO.File]::Delete($expiredResponse)
        $publicationBytes = [Text.UTF8Encoding]::new($false).GetBytes("{}`n")
        $published = [IO.File]::Open(
            $publicationTemporary, [IO.FileMode]::CreateNew,
            [IO.FileAccess]::Write, [IO.FileShare]::None
        )
        try {
            $published.Write($publicationBytes, 0, $publicationBytes.Length)
            $published.Flush($true)
        }
        finally {
            $published.Dispose()
            [Array]::Clear($publicationBytes, 0, $publicationBytes.Length)
        }
        [IO.File]::Move($publicationTemporary, $publicationResponse)
        Claim-PublishedOperatorResponse $publicationResponse $publicationClaimed `
            ([DateTimeOffset]::UtcNow.AddSeconds(2))
        $canonicalPublication = Read-StableJsonWithDigest `
            $publicationClaimed "self-test canonical operator response"
        Assert-CanonicalCompactJsonResponse `
            $canonicalPublication "self-test canonical operator response"
        foreach ($ambiguousJson in @(
            '{"decision":{},"decision":{}}' + "`n",
            '{"decision":{},"Decision":{}}' + "`n",
            '{"decision":{}} trailing' + "`n",
            "{`n  `"decision`": {}`n}`n"
        )) {
            $ambiguousPath = [IO.Path]::Combine(
                $publicationDirectory, "ambiguous-" + [Guid]::NewGuid().ToString("N") + ".json"
            )
            [IO.File]::WriteAllText($ambiguousPath, $ambiguousJson, $script:Utf8NoBom)
            $ambiguousRejected = $false
            try {
                $ambiguousStable = Read-StableJsonWithDigest `
                    $ambiguousPath "self-test ambiguous operator response"
                Assert-CanonicalCompactJsonResponse `
                    $ambiguousStable "self-test ambiguous operator response"
            }
            catch { $ambiguousRejected = $true }
            finally { if ([IO.File]::Exists($ambiguousPath)) { [IO.File]::Delete($ambiguousPath) } }
            if (-not $ambiguousRejected) {
                throw "Computer-helper recorder accepted a duplicate, case-colliding, trailing, or noncanonical response."
            }
        }
        $reservationReplayRejected = $false
        try { [IO.File]::WriteAllText($publicationResponse, '{"replay":true}') }
        catch { $reservationReplayRejected = $true }
        if (-not $reservationReplayRejected) {
            throw "Computer-helper recorder allowed a reserved response replay."
        }
        $inputPath = [IO.Path]::Combine($publicationDirectory, "digest-input.png")
        $inputBytes = New-Object byte[] 25
        [byte[]]$inputSignature = @(137, 80, 78, 71, 13, 10, 26, 10)
        $inputSignature.CopyTo($inputBytes, 0)
        [Text.Encoding]::ASCII.GetBytes("IHDR").CopyTo($inputBytes, 12)
        $inputBytes[19] = 120
        $inputBytes[23] = 32
        [IO.File]::WriteAllBytes($inputPath, $inputBytes)
        $inputFacts = Read-StablePngWithDigest $inputPath "self-test digest input"
        $inputBytes[24] = 1
        [IO.File]::WriteAllBytes($inputPath, $inputBytes)
        $changedInputRejected = $false
        try {
            [void](Assert-UnchangedOperatorFrame `
                $inputPath ([pscustomobject]@{
                    sha256 = $inputFacts.Sha256; bytes = $inputFacts.Bytes
                    width = $inputFacts.Width; height = $inputFacts.Height
                }) "self-test post-response input")
        }
        catch { $changedInputRejected = $true }
        [IO.File]::Delete($inputPath)
        [Array]::Clear($inputBytes, 0, $inputBytes.Length)
        [Array]::Clear($inputSignature, 0, $inputSignature.Length)
        if (-not $changedInputRejected) {
            throw "Computer-helper recorder accepted a changed post-response frame."
        }
        Remove-OperatorExchangeArtifact $publicationClaimed
        $extraPath = [IO.Path]::Combine(
            $publicationDirectory,
            "frame-" + [String]::new([char]"b", 32) + ".png"
        )
        [IO.File]::WriteAllBytes($extraPath, [byte[]](1, 2, 3))
        $extraRejected = $false
        try { Remove-OperatorExchangeScratch } catch { $extraRejected = $true }
        if (-not $extraRejected) {
            throw "Computer-helper recorder accepted an unregistered extra artifact."
        }
    }
    finally {
        foreach ($held in $script:OperatorResponseReservations) {
            try { $held.Stream.Dispose() } catch {}
        }
        if ([IO.Directory]::Exists($publicationDirectory)) {
            [IO.Directory]::Delete($publicationDirectory, $true)
        }
        $script:OperatorExchange = $savedExchange
        $script:OperatorExchangeArtifacts = $savedExchangeArtifacts
        $script:OperatorResponseReservations = $savedReservations
        $script:OperatorExpectedTransientArtifacts = $savedExpectedTransients
    }
    $goodSharedFrame = [pscustomobject]@{
        windowId = "window-1"; pid = 42; appName = "chrome.exe"; frameId = "frame-2"
        shareId = "share-1"; sourceSequence = 2
        share = [pscustomobject]@{ active = $true; id = "share-1" }
    }
    if (-not (Test-ExactObservationIdentity $goodSharedFrame "window-1" 42) -or
        -not (Test-ExactSharedFrame $goodSharedFrame "window-1" 42 "share-1" 1) -or
        (Test-ExactSharedFrame $goodSharedFrame "window-1" 42 "share-1" 2)) {
        throw "Computer-helper recorder exact shared-frame self-test failed."
    }
    $wrongPidFrame = ConvertFrom-JsonPreservingStrings `
        ($goodSharedFrame | ConvertTo-Json -Depth 5 -Compress)
    $wrongPidFrame.pid = 43
    if (Test-ExactObservationIdentity $wrongPidFrame "window-1" 42) {
        throw "Computer-helper recorder accepted an observation from a reused HWND/wrong PID."
    }
    $wrongAppFrame = ConvertFrom-JsonPreservingStrings `
        ($goodSharedFrame | ConvertTo-Json -Depth 5 -Compress)
    $wrongAppFrame.appName = "notepad.exe"
    if (Test-ExactObservationIdentity $wrongAppFrame "window-1" 42) {
        throw "Computer-helper recorder accepted an observation from a non-Chrome application."
    }
    $source = [IO.File]::ReadAllText($PSCommandPath, $script:Utf8NoBom)
    if ($source.Contains('[string]$' + 'JournalRecord') -or -not $source.Contains("Invoke-ComputerCommand") -or
        -not $source.Contains("computer.share.start") -or -not $source.Contains("computer.share.stop") -or
        $source.Contains("computer." + "helper.shutdown") -or
        -not $source.Contains('Stop-Process -Id $helperProcess.Id -Force') -or
        -not $source.Contains('ArgumentList @("--no-update-check")') -or
        -not $source.Contains("Get-ExactExtensionPayloadDigest") -or
        -not $source.Contains("Invoke-BestEffortUiRollback") -or
        -not $source.Contains("Read-LiveToggleState") -or
        -not $source.Contains("Read-LiveSavedTokenState") -or
        -not $source.Contains("Read-LiveCandidateCardState") -or
        -not $source.Contains("Wait-ProtocolBoundHelperWorker") -or
        -not $source.Contains("Invoke-ExpectedHumanPauseRefusal") -or
        -not $source.Contains("Complete-TrustedPopupResume") -or
        -not $source.Contains("Wait-NoExactImageProcess") -or
        -not $source.Contains("Wait-CanonicalPortReleased") -or
        -not $source.Contains("[void](Bind-NewDedicatedChromeWindow)") -or
        -not $source.Contains("Control+N did not produce exactly one new stock-Chrome window") -or
        -not $source.Contains("Test-ExactSharedFrame") -or
        -not $source.Contains('Get-FreshSharedFrame $Context "the $Purpose screenshot"') -or
        -not $source.Contains("Never accept response-state observation as post-action authority") -or
        -not $source.Contains("Get-ExactChromeWindow `$ExpectedWindowId `$ExpectedPid") -or
        -not $source.Contains('Set-MutationDisposition $mutation "DedicatedWindow" "outcome_unknown"') -or
        $source.Contains("MayBe" + "Changed") -or $source.Contains("May" + "Exist") -or
        -not $source.Contains("Select Folder button in Chrome's native picker") -or
        -not $source.Contains("the exact existing chrome://extensions tab") -or
        -not $source.Contains("UI rollback was unavailable because the exact helper/server transport was not alive") -or
        -not $source.Contains("Remove-CanonicalRawScreenshots `$script:RawDirectory") -or
        -not $source.Contains("PassThruOwnedTarget") -or
        -not $source.Contains("ConvertFrom-Json") -or
        -not $source.Contains("Assert-PrivateOperatorExchangeDirectory") -or
        -not $source.Contains("Read-StableJsonWithDigest") -or
        -not $source.Contains("response-`$requestId.claimed.json") -or
        -not $source.Contains('[IO.FileMode]::CreateNew') -or
        -not $source.Contains('The operator response reused the executor session.') -or
        -not $source.Contains('Only the scoped approval request may use the user-via-orchestrator responder role.') -or
        $source.Contains('-Out' + 'File') -or
        $source.Contains('manual' + 'VisualReviewConfirmed') -or
        $source.Contains('human' + 'VisualReview')) {
        throw "Computer-helper recorder live-execution self-test failed."
    }
    $clickDecisionIndex = $source.IndexOf('$click = Read-ClickParams $Context.Observation')
    $firstCoveredIndex = $source.IndexOf('$script:FirstCoveredActionDispatchedAtUtc = $dispatchedAt')
    $mutationDispatchIndex = $source.IndexOf('$result = Invoke-ComputerCommand $method $params')
    if ($clickDecisionIndex -lt 0 -or $firstCoveredIndex -le $clickDecisionIndex -or
        $mutationDispatchIndex -le $firstCoveredIndex) {
        throw "Computer-helper recorder first-covered-action timestamp is not immediately mutation-bound."
    }
    Write-Output "Computer-helper chain recorder self-test passed."
}

switch ($Mode) {
    "Run" { Invoke-Run }
    "SelfTest" { Invoke-SelfTest }
}
