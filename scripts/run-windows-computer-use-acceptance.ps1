#requires -Version 5.1

[CmdletBinding()]
param(
    [ValidateSet("Start", "Follow", "SelfTest")]
    [string]$Mode = "Start",

    [string]$Version,
    [string]$ServerPath,
    [string]$HelperPath,
    [string]$ChecksumManifest,
    [string]$ChecksumManifestSha256,
    [string]$CandidateBindingPath,
    [string]$FixturePath,
    [string]$EvidenceDirectory,
    [string]$CoordinatorDirectory,

    [ValidateRange(15, 300)]
    [int]$ForegroundArmTimeoutSeconds = 300,

    [ValidateRange(1, 30)]
    [int]$StartupTimeoutSeconds = 15,

    [string]$CleanCoordinatorNonce,
    [string]$InternalWorkerNonce,
    [string]$InternalWorkerSupportSelfTestPath,
    [string]$InternalWorkerSupportSelfTestSha256,
    [string]$InternalWorkerSupportSelfTestNonce,
    [string]$InternalNestedJobRunnerSelfTestNonce
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$script:SchemaVersion = 1
$script:AttemptReservationSchemaVersion = 2
$script:ProductVersion = "0.12.68"
$script:ForegroundGateMode = "automatic-stable-external-foreground"
$script:AutomaticHandoffSchemaVersion = 2
$script:SuccessMessage = "Windows computer-use acceptance coordinator self-test passed."
$script:Utf8NoBom = [Text.UTF8Encoding]::new($false)
$script:SensitiveEnvironmentPattern = '(?i)(?:TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIAL|COOKIE|API[_-]?KEY|PRIVATE[_-]?KEY|AUTHORIZATION)'
$script:AttemptLedgerDomain = "local-browser-bridge/windows-acceptance-attempt/v1"
$script:MaximumRunnerMilliseconds = 1800000
$script:MaximumStreamDrainMilliseconds = 10000
$script:WorkerLifetimeJobName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeJob-v1"
$script:WorkerLifetimeRecoveryMilliseconds = 10000
$script:SelfTestAttemptLedgerRoot = $null
$script:SelfTestCoordinatorParent = $null
$script:SelfTestEvidenceParent = $null
$script:ProcessLifetimeCoordinatorMutex = $null

function Resolve-SystemWindowsPowerShell {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "The Windows acceptance coordinator can run only on Windows."
    }
    $registryView = if ([Environment]::Is64BitOperatingSystem) {
        [Microsoft.Win32.RegistryView]::Registry64
    }
    else {
        [Microsoft.Win32.RegistryView]::Registry32
    }
    $localMachine = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
        [Microsoft.Win32.RegistryHive]::LocalMachine,
        $registryView
    )
    $windowsNt = $null
    try {
        $windowsNt = $localMachine.OpenSubKey("SOFTWARE\Microsoft\Windows NT\CurrentVersion", $false)
        if ($null -eq $windowsNt) {
            throw "The Windows NT machine registry key is unavailable."
        }
        $machineSystemRoot = [string]$windowsNt.GetValue(
            "SystemRoot",
            $null,
            [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
        )
    }
    finally {
        if ($null -ne $windowsNt) { $windowsNt.Dispose() }
        $localMachine.Dispose()
    }
    if ([String]::IsNullOrWhiteSpace($machineSystemRoot) -or
        -not [IO.Path]::IsPathRooted($machineSystemRoot)) {
        throw "The machine Windows SystemRoot is unavailable or invalid."
    }
    $relativeSystemDirectory = if (
        [Environment]::Is64BitOperatingSystem -and -not [Environment]::Is64BitProcess
    ) { "Sysnative" } else { "System32" }
    $path = [IO.Path]::GetFullPath([IO.Path]::Combine(
        $machineSystemRoot,
        $relativeSystemDirectory,
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe"
    ))
    return Resolve-OrdinaryPath $path $true "System powershell.exe"
}

function Resolve-OrdinaryPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][bool]$RequireFile,
        [Parameter(Mandatory = $true)][string]$Description
    )
    if ([String]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathRooted($Path)) {
        throw "$Description must be an absolute path."
    }
    $full = [IO.Path]::GetFullPath($Path)
    if ($RequireFile) {
        if (-not [IO.File]::Exists($full)) { throw "$Description does not exist." }
        $current = [IO.FileInfo]::new($full)
    }
    else {
        if (-not [IO.Directory]::Exists($full)) { throw "$Description does not exist." }
        $current = [IO.DirectoryInfo]::new($full)
    }
    while ($null -ne $current) {
        if (($current.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "$Description must not traverse a reparse point."
        }
        if ($current -is [IO.FileInfo]) { $current = $current.Directory }
        else { $current = $current.Parent }
    }
    return $full
}

function Resolve-NewOrdinaryPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Description
    )
    if ([String]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathRooted($Path)) {
        throw "$Description must be an absolute path."
    }
    $full = [IO.Path]::GetFullPath($Path)
    if ([IO.File]::Exists($full) -or [IO.Directory]::Exists($full)) {
        throw "$Description must not already exist."
    }
    $parent = [IO.Path]::GetDirectoryName($full)
    if ([String]::IsNullOrWhiteSpace($parent)) {
        throw "$Description must have an ordinary existing parent."
    }
    $null = Resolve-OrdinaryPath $parent $false "$Description parent"
    return $full
}

function Assert-NewChildPath {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Leaf
    )
    if ([String]::IsNullOrWhiteSpace($Leaf) -or [IO.Path]::GetFileName($Leaf) -cne $Leaf) {
        throw "Coordinator leaf names must be one ordinary filename."
    }
    $path = [IO.Path]::GetFullPath([IO.Path]::Combine($Root, $Leaf))
    $prefix = $Root.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Coordinator output escaped its private root."
    }
    if ([IO.File]::Exists($path) -or [IO.Directory]::Exists($path)) {
        throw "Coordinator output already exists."
    }
    return $path
}

function Set-PrivateDirectoryAcl {
    param([Parameter(Mandatory = $true)][string]$Path)
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    if ($null -eq $identity.User) { throw "The current Windows identity has no SID." }
    $security = [Security.AccessControl.DirectorySecurity]::new()
    $security.SetAccessRuleProtection($true, $false)
    $security.SetOwner($identity.User)
    $inheritance = [Security.AccessControl.InheritanceFlags]::ContainerInherit -bor
        [Security.AccessControl.InheritanceFlags]::ObjectInherit
    $rule = [Security.AccessControl.FileSystemAccessRule]::new(
        $identity.User,
        [Security.AccessControl.FileSystemRights]::FullControl,
        $inheritance,
        [Security.AccessControl.PropagationFlags]::None,
        [Security.AccessControl.AccessControlType]::Allow
    )
    $security.AddAccessRule($rule)
    [IO.Directory]::SetAccessControl($Path, $security)
}

function Assert-PrivateDirectoryAcl {
    param([Parameter(Mandatory = $true)][string]$Path)
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    if ($null -eq $identity.User) { throw "The current Windows identity has no SID." }
    $security = [IO.Directory]::GetAccessControl($Path)
    $owner = $security.GetOwner([Security.Principal.SecurityIdentifier])
    $rules = @($security.GetAccessRules(
        $true,
        $true,
        [Security.Principal.SecurityIdentifier]
    ))
    $validRule = $rules.Count -eq 1 -and
        $rules[0].IdentityReference -eq $identity.User -and
        $rules[0].AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
        ($rules[0].FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -eq
            [Security.AccessControl.FileSystemRights]::FullControl -and
        ($rules[0].InheritanceFlags -band [Security.AccessControl.InheritanceFlags]::ContainerInherit) -ne 0 -and
        ($rules[0].InheritanceFlags -band [Security.AccessControl.InheritanceFlags]::ObjectInherit) -ne 0
    if (-not $security.AreAccessRulesProtected -or $owner -ne $identity.User -or -not $validRule) {
        throw "The private directory is not protected by the exact owner-only ACL."
    }
}

function Assert-ExactPropertyOrder {
    param(
        [Parameter(Mandatory = $true)][object]$Object,
        [Parameter(Mandatory = $true)][string[]]$Expected,
        [Parameter(Mandatory = $true)][string]$Description
    )
    if ($null -eq $Object -or $Object -is [Array]) {
        throw "$Description must be one object."
    }
    $actual = @($Object.PSObject.Properties.Name)
    if (($actual -join "`n") -cne ($Expected -join "`n")) {
        throw "$Description fields are not in canonical order."
    }
}

function Assert-ExactBoolean {
    param([object]$Actual, [bool]$Expected, [string]$Description)
    if ($Actual -isnot [bool] -or $Actual -ne $Expected) {
        throw "$Description must be $Expected."
    }
}

function Assert-ExactIntegerRange {
    param([object]$Actual, [Int64]$Minimum, [Int64]$Maximum, [string]$Description)
    $integerTypes = @(
        [TypeCode]::SByte, [TypeCode]::Byte, [TypeCode]::Int16, [TypeCode]::UInt16,
        [TypeCode]::Int32, [TypeCode]::UInt32, [TypeCode]::Int64, [TypeCode]::UInt64
    )
    if ($null -eq $Actual -or $Actual -is [bool] -or $Actual -isnot [ValueType] -or
        $integerTypes -notcontains [Type]::GetTypeCode($Actual.GetType()) -or
        [decimal]$Actual -lt $Minimum -or [decimal]$Actual -gt $Maximum) {
        throw "$Description must be an integer from $Minimum through $Maximum."
    }
}

function ConvertFrom-CanonicalUtcString {
    param([object]$Value, [string]$Description)
    $parsed = [DateTimeOffset]::MinValue
    if ($Value -isnot [string] -or
        $Value -cnotmatch '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{7}Z$' -or
        -not [DateTimeOffset]::TryParseExact(
            $Value,
            "o",
            [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind,
            [ref]$parsed
        ) -or $parsed.Offset -ne [TimeSpan]::Zero) {
        throw "$Description must be a canonical UTC round-trip timestamp."
    }
    return $parsed
}

function Get-FileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    $resolved = Resolve-OrdinaryPath $Path $true "SHA-256 input"
    $stream = [IO.File]::Open($resolved, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
        $stream.Dispose()
    }
}

function Get-CanonicalSha256 {
    param([Parameter(Mandatory = $true)][string]$Value)
    if ($Value.Contains("`0")) { throw "The SHA-256 domain input contains a NUL character." }
    $bytes = $script:Utf8NoBom.GetBytes($Value)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
        [Array]::Clear($bytes, 0, $bytes.Length)
    }
}

function Get-CandidateAttemptKey {
    param(
        [string]$CandidateVersion,
        [string]$ManifestSha256,
        [string]$Server,
        [string]$Helper,
        [string]$Manifest,
        [string]$Binding
    )
    # The irrevocable boundary is deliberately broader than mutable candidate
    # paths: one reservation exists per product version for this Windows user.
    # File and manifest bindings are independently revalidated before launch,
    # but changing them cannot mint a second attempt for the same version.
    $identity = @($script:AttemptLedgerDomain, $CandidateVersion) -join "`n"
    return Get-CanonicalSha256 $identity
}

function Resolve-TrustedLocalAppData {
    $path = [Environment]::GetFolderPath(
        [Environment+SpecialFolder]::LocalApplicationData,
        [Environment+SpecialFolderOption]::DoNotVerify
    )
    if ([String]::IsNullOrWhiteSpace($path) -or -not [IO.Path]::IsPathRooted($path) -or
        $path.StartsWith("\\", [StringComparison]::Ordinal) -or
        [IO.Path]::GetPathRoot($path) -cnotmatch '^[A-Za-z]:\\$') {
        throw "The Windows Known Folder API did not return a local drive LocalAppData path."
    }
    $resolved = Resolve-OrdinaryPath $path $false "Known Folder LocalAppData"
    $drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($resolved))
    if (-not $drive.IsReady -or $drive.DriveType -ne [IO.DriveType]::Fixed -or
        @("NTFS", "ReFS") -cnotcontains $drive.DriveFormat) {
        throw "LocalAppData must be on a ready fixed NTFS or ReFS volume."
    }
    return $resolved
}

function Get-AttemptLedgerRoot {
    if (-not [String]::IsNullOrWhiteSpace([string]$script:SelfTestAttemptLedgerRoot)) {
        $selfTestRoot = Resolve-OrdinaryPath `
            ([string]$script:SelfTestAttemptLedgerRoot) `
            $false `
            "Self-test acceptance-attempt ledger"
        Assert-PrivateDirectoryAcl $selfTestRoot
        return $selfTestRoot
    }
    $parent = Resolve-TrustedLocalAppData
    $root = [IO.Path]::GetFullPath([IO.Path]::Combine(
        $parent,
        "LBB-Windows-Acceptance-Attempts-v1"
    ))
    if (-not [IO.Directory]::Exists($root)) {
        [IO.Directory]::CreateDirectory($root) | Out-Null
        $resolved = Resolve-OrdinaryPath $root $false "Acceptance-attempt ledger"
        if ($resolved -cne $root) { throw "The acceptance-attempt ledger changed identity." }
        Set-PrivateDirectoryAcl $root
    }
    $null = Resolve-OrdinaryPath $root $false "Acceptance-attempt ledger"
    Assert-PrivateDirectoryAcl $root
    return $root
}

function Get-PrivateAcceptanceParent {
    param([ValidateSet("Coordinator", "Evidence")][string]$Kind)
    $override = if ($Kind -ceq "Coordinator") {
        [string]$script:SelfTestCoordinatorParent
    }
    else { [string]$script:SelfTestEvidenceParent }
    if (-not [String]::IsNullOrWhiteSpace($override)) {
        $root = Resolve-OrdinaryPath $override $false "$Kind self-test parent"
        Assert-PrivateDirectoryAcl $root
        return $root
    }
    $parent = Resolve-TrustedLocalAppData
    $leaf = if ($Kind -ceq "Coordinator") {
        "LBB-Windows-Acceptance-Coordinators-v1"
    }
    else { "LBB-Windows-Acceptance-Evidence-v1" }
    $root = [IO.Path]::GetFullPath([IO.Path]::Combine($parent, $leaf))
    if (-not [IO.Directory]::Exists($root)) {
        [IO.Directory]::CreateDirectory($root) | Out-Null
        $resolved = Resolve-OrdinaryPath $root $false "$Kind acceptance parent"
        if ($resolved -cne $root) { throw "The $Kind acceptance parent changed identity." }
        Set-PrivateDirectoryAcl $root
    }
    $null = Resolve-OrdinaryPath $root $false "$Kind acceptance parent"
    Assert-PrivateDirectoryAcl $root
    return $root
}

function Assert-DirectPrivateChildPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$PrivateParent,
        [Parameter(Mandatory = $true)][string]$Description
    )
    $parent = Resolve-OrdinaryPath $PrivateParent $false "$Description private parent"
    Assert-PrivateDirectoryAcl $parent
    if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($Path)) -cne $parent) {
        throw "$Description must be a direct child of its fixed owner-private parent."
    }
}

function New-PrivateChildDirectory {
    param(
        [Parameter(Mandatory = $true)][string]$Parent,
        [Parameter(Mandatory = $true)][string]$Leaf,
        [Parameter(Mandatory = $true)][string]$Description
    )
    $root = Resolve-OrdinaryPath $Parent $false "$Description parent"
    Assert-PrivateDirectoryAcl $root
    $path = Assert-NewChildPath $root $Leaf
    [IO.Directory]::CreateDirectory($path) | Out-Null
    $resolved = Resolve-OrdinaryPath $path $false $Description
    if ($resolved -cne $path) { throw "$Description changed identity." }
    Set-PrivateDirectoryAcl $resolved
    Assert-PrivateDirectoryAcl $resolved
    return $resolved
}

function Copy-FileToPrivateStage {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination,
        [Parameter(Mandatory = $true)][string]$Description
    )
    $resolvedSource = Resolve-OrdinaryPath $Source $true "$Description source"
    $zoneStreams = @(
        Microsoft.PowerShell.Management\Get-Item `
            -LiteralPath $resolvedSource `
            -Stream "Zone.Identifier" `
            -ErrorAction SilentlyContinue
    )
    if ($zoneStreams.Count -ne 0) {
        throw "$Description source carries Windows download-zone metadata and was refused before staging."
    }
    $destinationParent = Resolve-OrdinaryPath ([IO.Path]::GetDirectoryName($Destination)) $false "$Description destination parent"
    Assert-PrivateDirectoryAcl $destinationParent
    $resolvedDestination = Assert-NewChildPath $destinationParent ([IO.Path]::GetFileName($Destination))
    $input = [IO.File]::Open(
        $resolvedSource,
        [IO.FileMode]::Open,
        [IO.FileAccess]::Read,
        [IO.FileShare]::Read
    )
    $output = $null
    try {
        $output = [IO.File]::Open(
            $resolvedDestination,
            [IO.FileMode]::CreateNew,
            [IO.FileAccess]::Write,
            [IO.FileShare]::None
        )
        $input.CopyTo($output)
        $output.Flush($true)
    }
    finally {
        if ($null -ne $output) { $output.Dispose() }
        $input.Dispose()
    }
    return Resolve-OrdinaryPath $resolvedDestination $true "$Description staged copy"
}

function Get-CandidateAttemptReservationPath {
    param(
        [Parameter(Mandatory = $true)][string]$LedgerRoot,
        [Parameter(Mandatory = $true)][string]$AttemptKey
    )
    if ($AttemptKey -cnotmatch '^[0-9a-f]{64}$') { throw "The candidate attempt key is invalid." }
    $root = Resolve-OrdinaryPath $LedgerRoot $false "Acceptance-attempt ledger"
    Assert-PrivateDirectoryAcl $root
    return [IO.Path]::Combine($root, "attempt-$AttemptKey.json")
}

function Reserve-CandidateAttempt {
    param(
        [Parameter(Mandatory = $true)][string]$LedgerRoot,
        [Parameter(Mandatory = $true)][string]$AttemptKey,
        [Parameter(Mandatory = $true)][string]$CandidateVersion,
        [Parameter(Mandatory = $true)][string]$ManifestSha256,
        [Parameter(Mandatory = $true)][string]$CoordinatorInstanceId
    )
    if ($CoordinatorInstanceId -cnotmatch '^[0-9a-f]{32}$') {
        throw "The coordinator instance identifier is invalid."
    }
    $path = Get-CandidateAttemptReservationPath `
        -LedgerRoot $LedgerRoot `
        -AttemptKey $AttemptKey
    if ([IO.File]::Exists($path)) {
        throw "This product version already has a persistent Windows acceptance attempt reservation."
    }
    try {
        Write-CreateOnceJson $path ([ordered]@{
            schemaVersion = $script:AttemptReservationSchemaVersion
            kind = "windows-acceptance-attempt-reservation"
            status = "reserved-no-retry"
            productVersion = $CandidateVersion
            attemptKey = $AttemptKey
            checksumManifestSha256 = $ManifestSha256.ToLowerInvariant()
            coordinatorInstanceId = $CoordinatorInstanceId
            retryAllowed = $false
            reservedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
            pathsRecorded = $false
            secretsRecorded = $false
        })
    }
    catch [IO.IOException] {
        if ([IO.File]::Exists($path)) {
            throw "This product version already has a persistent Windows acceptance attempt reservation."
        }
        throw
    }
    return Resolve-OrdinaryPath $path $true "Acceptance-attempt reservation"
}

function ConvertTo-NativeArgument {
    param([AllowEmptyString()][string]$Value)
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') { return $Value }
    $builder = [Text.StringBuilder]::new()
    $null = $builder.Append('"')
    $backslashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $backslashes++
            continue
        }
        if ($character -eq '"') {
            $null = $builder.Append(('\' * (($backslashes * 2) + 1)))
            $null = $builder.Append('"')
            $backslashes = 0
            continue
        }
        if ($backslashes -gt 0) {
            $null = $builder.Append(('\' * $backslashes))
            $backslashes = 0
        }
        $null = $builder.Append($character)
    }
    if ($backslashes -gt 0) {
        $null = $builder.Append(('\' * ($backslashes * 2)))
    }
    $null = $builder.Append('"')
    return $builder.ToString()
}

function Join-NativeArguments {
    param([Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Values)
    return (($Values | ForEach-Object { ConvertTo-NativeArgument $_ }) -join ' ')
}

function ConvertTo-CanonicalUtcString {
    param([Parameter(Mandatory = $true)][DateTime]$Value)
    return $Value.ToUniversalTime().ToString("o", [Globalization.CultureInfo]::InvariantCulture)
}

function Write-CreateOnceJson {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][object]$Record
    )
    $json = $Record | ConvertTo-Json -Depth 12 -Compress
    if ([String]::IsNullOrWhiteSpace($json) -or $json.Contains("`0") -or $json.Length -gt 65536) {
        throw "Coordinator record serialization is invalid."
    }
    $bytes = $script:Utf8NoBom.GetBytes($json)
    $directory = Resolve-OrdinaryPath ([IO.Path]::GetDirectoryName($Path)) $false "Coordinator record parent"
    $temporaryLeaf = "." + [IO.Path]::GetFileName($Path) + "." + [Guid]::NewGuid().ToString("N") + ".tmp"
    $temporaryPath = Assert-NewChildPath $directory $temporaryLeaf
    $stream = [IO.File]::Open(
        $temporaryPath,
        [IO.FileMode]::CreateNew,
        [IO.FileAccess]::Write,
        [IO.FileShare]::None
    )
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    }
    finally {
        $stream.Dispose()
    }
    try {
        # File.Move is a same-directory, no-replace publication boundary on the
        # supported Windows runtime. Readers can observe either no record or the
        # complete flushed record, never a partially written final pathname.
        [IO.File]::Move($temporaryPath, $Path)
    }
    finally {
        if ([IO.File]::Exists($temporaryPath)) {
            [IO.File]::Delete($temporaryPath)
        }
    }
}

function Read-BoundedJson {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][int]$MaximumBytes,
        [Parameter(Mandatory = $true)][string]$Description
    )
    $resolved = Resolve-OrdinaryPath $Path $true $Description
    $stream = $null
    $bytes = $null
    try {
        $stream = [IO.File]::Open(
            $resolved,
            [IO.FileMode]::Open,
            [IO.FileAccess]::Read,
            [IO.FileShare]::Read
        )
        $length = $stream.Length
        if ($length -le 1 -or $length -gt $MaximumBytes) {
            throw "$Description has an invalid size."
        }
        $bytes = [byte[]]::new([int]$length)
        $offset = 0
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -le 0) { throw "$Description ended before its observed length." }
            $offset += $read
        }
        if ($stream.ReadByte() -ne -1 -or $stream.Length -ne $length) {
            throw "$Description changed while it was read."
        }
        $text = [Text.UTF8Encoding]::new($false, $true).GetString($bytes)
        if ($text.Contains("`0")) { throw "$Description contains a NUL character." }
        return $text | ConvertFrom-Json
    }
    finally {
        if ($null -ne $stream) { $stream.Dispose() }
        if ($null -ne $bytes) { [Array]::Clear($bytes, 0, $bytes.Length) }
    }
}

function New-ProcessStartInfo {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory,
        [switch]$Hidden
    )
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Executable
    $info.Arguments = Join-NativeArguments $Arguments
    $info.WorkingDirectory = $WorkingDirectory
    $info.UseShellExecute = $false
    $info.CreateNoWindow = [bool]$Hidden
    return $info
}

function Remove-SensitiveInheritedEnvironment {
    param([Parameter(Mandatory = $true)][Diagnostics.ProcessStartInfo]$Info)
    foreach ($key in @($Info.EnvironmentVariables.Keys)) {
        if ([string]$key -match $script:SensitiveEnvironmentPattern) {
            $Info.EnvironmentVariables.Remove([string]$key)
        }
    }
}

function Get-WhitelistedWorkerEnvironment {
    $allowedNames = @(
        "ALLUSERSPROFILE", "APPDATA", "CommonProgramFiles", "CommonProgramFiles(x86)",
        "CommonProgramW6432", "ComSpec", "HOMEDRIVE", "HOMEPATH", "LOCALAPPDATA",
        "LOGONSERVER", "NUMBER_OF_PROCESSORS", "OS", "PROCESSOR_ARCHITECTURE",
        "PROCESSOR_IDENTIFIER", "PROCESSOR_LEVEL", "PROCESSOR_REVISION", "ProgramData",
        "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "PUBLIC", "SESSIONNAME",
        "SystemDrive", "SystemRoot", "TEMP", "TMP", "USERDOMAIN", "USERDOMAIN_ROAMINGPROFILE",
        "USERNAME", "USERPROFILE", "windir"
    )
    $environment = [ordered]@{}
    foreach ($name in $allowedNames) {
        $value = [Environment]::GetEnvironmentVariable($name, "Process")
        if ($null -ne $value -and -not $value.Contains("`0")) {
            $environment[$name] = $value
        }
    }
    foreach ($required in @("SystemRoot", "TEMP", "TMP", "USERPROFILE")) {
        if (-not $environment.Contains($required) -or
            [String]::IsNullOrWhiteSpace([string]$environment[$required])) {
            throw "The clean worker environment is missing $required."
        }
    }
    return $environment
}

function Set-ExactProcessEnvironment {
    param(
        [Parameter(Mandatory = $true)][Diagnostics.ProcessStartInfo]$Info,
        [Parameter(Mandatory = $true)][Collections.IDictionary]$Environment
    )
    $Info.EnvironmentVariables.Clear()
    foreach ($item in $Environment.GetEnumerator()) {
        $name = [string]$item.Key
        $value = [string]$item.Value
        if ([String]::IsNullOrWhiteSpace($name) -or $name.Contains("=") -or
            $name.Contains("`0") -or $value.Contains("`0")) {
            throw "The exact child environment contains an invalid entry."
        }
        $Info.EnvironmentVariables[$name] = $value
    }
}

function Start-DetachedWorkerProcess {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$Arguments,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath,
        [Parameter(Mandatory = $true)][Collections.IDictionary]$Environment
    )
    if ([IO.File]::Exists($StdoutPath) -or [IO.File]::Exists($StderrPath)) {
        throw "Detached worker logs must be fresh."
    }
    if (-not ("LbbCoordinator.NativeDetachedWorkerLauncher" -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Collections;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

namespace LbbCoordinator {
  public sealed class DetachedWorkerProcess : IDisposable {
    private const uint WAIT_OBJECT_0 = 0x00000000;
    private const uint WAIT_TIMEOUT = 0x00000102;
    private const uint WAIT_FAILED = 0xFFFFFFFF;
    private const uint STILL_ACTIVE = 259;
    private const uint DUPLICATE_SAME_ACCESS = 0x00000002;
    private const uint PROCESS_TERMINATE = 0x0001;
    private const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x1000;
    private const uint SYNCHRONIZE = 0x00100000;
    private IntPtr handle;
    private IntPtr guardJob;
    public int Id { get; private set; }
    public DateTime StartedAtUtc { get; private set; }

    internal DetachedWorkerProcess(
      IntPtr processHandle, IntPtr guardJobHandle, int processId, DateTime startedAtUtc) {
      handle = processHandle;
      guardJob = guardJobHandle;
      Id = processId;
      StartedAtUtc = startedAtUtc;
    }

    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern uint WaitForSingleObject(IntPtr value, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool TerminateProcess(IntPtr process, uint exitCode);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetExitCodeProcess(IntPtr process, out uint exitCode);
    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern IntPtr OpenProcess(
      uint desiredAccess,
      [MarshalAs(UnmanagedType.Bool)] bool inheritHandle,
      int processId);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetProcessTimes(
      IntPtr process,
      out long creationTime,
      out long exitTime,
      out long kernelTime,
      out long userTime);
    [DllImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseHandle(IntPtr value);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool DuplicateHandle(
      IntPtr sourceProcess,
      IntPtr sourceHandle,
      IntPtr targetProcess,
      out IntPtr targetHandle,
      uint desiredAccess,
      [MarshalAs(UnmanagedType.Bool)] bool inheritHandle,
      uint options);
    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();

    public static DetachedWorkerProcess OpenExact(int processId, DateTime expectedStartedAtUtc) {
      IntPtr process = OpenProcess(
        PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
        false,
        processId);
      if (process == IntPtr.Zero) {
        throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not open the exact probe process");
      }
      try {
        long creationTime;
        long exitTime;
        long kernelTime;
        long userTime;
        if (!GetProcessTimes(process, out creationTime, out exitTime, out kernelTime, out userTime)) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not bind the exact probe process start time");
        }
        DateTime actual = DateTime.FromFileTimeUtc(creationTime);
        if (actual.Ticks != expectedStartedAtUtc.ToUniversalTime().Ticks) {
          throw new InvalidOperationException("The probe PID no longer identifies the expected process instance");
        }
        DetachedWorkerProcess result = new DetachedWorkerProcess(
          process, IntPtr.Zero, processId, actual);
        process = IntPtr.Zero;
        return result;
      }
      finally {
        if (process != IntPtr.Zero) { CloseHandle(process); }
      }
    }

    public bool GuardOwnershipTransferred { get { return guardJob == IntPtr.Zero; } }

    public void TransferGuardOwnership() {
      EnsureOpen();
      if (guardJob == IntPtr.Zero) {
        throw new InvalidOperationException("The detached worker guard ownership was already transferred");
      }
      if (HasExited) {
        throw new InvalidOperationException("The detached worker exited before guard ownership transfer");
      }
      IntPtr remoteHandle;
      if (!DuplicateHandle(
        GetCurrentProcess(), guardJob, handle, out remoteHandle,
        0, false, DUPLICATE_SAME_ACCESS)) {
        throw new Win32Exception(
          Marshal.GetLastWin32Error(),
          "Could not transfer the kill-on-close guard Job to the detached worker");
      }
      // remoteHandle is meaningful only in the target process. It is
      // intentionally left unreferenced there so Windows closes it exactly at
      // worker teardown, which triggers KILL_ON_JOB_CLOSE for descendants.
      if (!CloseHandle(guardJob)) {
        guardJob = IntPtr.Zero;
        throw new Win32Exception(
          Marshal.GetLastWin32Error(),
          "Could not release the launcher copy of the detached worker guard Job");
      }
      guardJob = IntPtr.Zero;
    }

    public bool HasExited {
      get {
        EnsureOpen();
        uint wait = WaitForSingleObject(handle, 0);
        if (wait == WAIT_OBJECT_0) { return true; }
        if (wait == WAIT_TIMEOUT) { return false; }
        int error = wait == WAIT_FAILED ? Marshal.GetLastWin32Error() : unchecked((int)wait);
        throw new Win32Exception(error, "Could not inspect the exact detached worker handle");
      }
    }

    public void Kill() {
      EnsureOpen();
      if (HasExited) { return; }
      if (!TerminateProcess(handle, 1)) {
        int error = Marshal.GetLastWin32Error();
        if (!HasExited) {
          throw new Win32Exception(error, "Could not terminate the exact detached worker handle");
        }
      }
    }

    public bool WaitForExit(int milliseconds) {
      EnsureOpen();
      if (milliseconds < 0) { throw new ArgumentOutOfRangeException("milliseconds"); }
      uint wait = WaitForSingleObject(handle, (uint)milliseconds);
      if (wait == WAIT_OBJECT_0) { return true; }
      if (wait == WAIT_TIMEOUT) { return false; }
      int error = wait == WAIT_FAILED ? Marshal.GetLastWin32Error() : unchecked((int)wait);
      throw new Win32Exception(error, "Could not wait on the exact detached worker handle");
    }

    public int ExitCode {
      get {
        EnsureOpen();
        uint value;
        if (!GetExitCodeProcess(handle, out value)) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not read the exact detached worker exit code");
        }
        if (value == STILL_ACTIVE) {
          throw new InvalidOperationException("The detached worker is still running");
        }
        return unchecked((int)value);
      }
    }

    public void Dispose() {
      if (guardJob != IntPtr.Zero) {
        CloseHandle(guardJob);
        guardJob = IntPtr.Zero;
      }
      if (handle != IntPtr.Zero) {
        CloseHandle(handle);
        handle = IntPtr.Zero;
      }
      GC.SuppressFinalize(this);
    }

    private void EnsureOpen() {
      if (handle == IntPtr.Zero) {
        throw new ObjectDisposedException("DetachedWorkerProcess");
      }
    }
  }

  public sealed class DetachedWorkerCleanupException : Exception {
    public DetachedWorkerCleanupException(string message, Exception inner)
      : base(message, inner) { }
  }

  public static class NativeDetachedWorkerLauncher {
    private const uint CREATE_SUSPENDED = 0x00000004;
    private const uint CREATE_UNICODE_ENVIRONMENT = 0x00000400;
    private const uint CREATE_BREAKAWAY_FROM_JOB = 0x01000000;
    private const uint CREATE_NO_WINDOW = 0x08000000;
    private const uint EXTENDED_STARTUPINFO_PRESENT = 0x00080000;
    private const uint STARTF_USESTDHANDLES = 0x00000100;
    private const uint PROC_THREAD_ATTRIBUTE_HANDLE_LIST = 0x00020002;
    private const uint PROC_THREAD_ATTRIBUTE_JOB_LIST = 0x0002000D;
    private const uint JOB_OBJECT_LIMIT_BREAKAWAY_OK = 0x00000800;
    private const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
    private const uint GENERIC_READ = 0x80000000;
    private const uint GENERIC_WRITE = 0x40000000;
    private const uint FILE_SHARE_READ = 0x00000001;
    private const uint FILE_SHARE_WRITE = 0x00000002;
    private const uint CREATE_NEW = 1;
    private const uint OPEN_EXISTING = 3;
    private const int ERROR_INSUFFICIENT_BUFFER = 122;
    private const uint WAIT_OBJECT_0 = 0x00000000;
    private const uint WAIT_TIMEOUT = 0x00000102;
    private const uint WAIT_FAILED = 0xFFFFFFFF;
    private const uint TERMINATION_WAIT_MS = 5000;
    private static readonly IntPtr InvalidHandle = new IntPtr(-1);

    [StructLayout(LayoutKind.Sequential)]
    private struct SECURITY_ATTRIBUTES {
      internal int Length;
      internal IntPtr SecurityDescriptor;
      [MarshalAs(UnmanagedType.Bool)] internal bool InheritHandle;
    }

    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    private struct STARTUPINFO {
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
    private struct STARTUPINFOEX {
      internal STARTUPINFO StartupInfo;
      internal IntPtr AttributeList;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct PROCESS_INFORMATION {
      internal IntPtr Process;
      internal IntPtr Thread;
      internal int ProcessId;
      internal int ThreadId;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct IO_COUNTERS {
      internal ulong ReadOperationCount;
      internal ulong WriteOperationCount;
      internal ulong OtherOperationCount;
      internal ulong ReadTransferCount;
      internal ulong WriteTransferCount;
      internal ulong OtherTransferCount;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
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
    private struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
      internal JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
      internal IO_COUNTERS IoInfo;
      internal UIntPtr ProcessMemoryLimit;
      internal UIntPtr JobMemoryLimit;
      internal UIntPtr PeakProcessMemoryUsed;
      internal UIntPtr PeakJobMemoryUsed;
    }

    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
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

    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool InitializeProcThreadAttributeList(
      IntPtr attributeList, int attributeCount, int flags, ref UIntPtr size);

    [DllImport("kernel32.dll", SetLastError=true)]
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

    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern IntPtr CreateFile(
      string fileName,
      uint desiredAccess,
      uint shareMode,
      ref SECURITY_ATTRIBUTES securityAttributes,
      uint creationDisposition,
      uint flagsAndAttributes,
      IntPtr templateFile);

    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool IsProcessInJob(
      IntPtr process, IntPtr job, [MarshalAs(UnmanagedType.Bool)] out bool result);

    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern IntPtr CreateJobObject(IntPtr attributes, string name);

    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetInformationJobObject(
      IntPtr job, int infoClass, IntPtr info, uint length);

    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetProcessTimes(
      IntPtr process,
      out long creationTime,
      out long exitTime,
      out long kernelTime,
      out long userTime);

    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern uint ResumeThread(IntPtr thread);

    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool TerminateProcess(IntPtr process, uint exitCode);

    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [DllImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseHandle(IntPtr value);

    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();

    public static bool CurrentProcessIsInJob() {
      bool inJob;
      if (!IsProcessInJob(GetCurrentProcess(), IntPtr.Zero, out inJob)) {
        throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not inspect the launcher Job boundary");
      }
      return inJob;
    }

    public static DetachedWorkerProcess Start(
      string executable,
      string arguments,
      string workingDirectory,
      string stdoutPath,
      string stderrPath,
      IDictionary environment) {
      IntPtr environmentBlock = IntPtr.Zero;
      IntPtr stdinHandle = IntPtr.Zero;
      IntPtr stdoutHandle = IntPtr.Zero;
      IntPtr stderrHandle = IntPtr.Zero;
      IntPtr handleList = IntPtr.Zero;
      IntPtr jobList = IntPtr.Zero;
      IntPtr attributeList = IntPtr.Zero;
      IntPtr guardJob = IntPtr.Zero;
      bool attributeListInitialized = false;
      PROCESS_INFORMATION nativeProcess = new PROCESS_INFORMATION();
      bool created = false;
      bool ownershipTransferred = false;
      Exception primaryFailure = null;
      try {
        environmentBlock = BuildEnvironment(environment);
        guardJob = CreateKillOnCloseJob();
        SECURITY_ATTRIBUTES security = new SECURITY_ATTRIBUTES();
        security.Length = Marshal.SizeOf(typeof(SECURITY_ATTRIBUTES));
        security.InheritHandle = true;
        stdinHandle = CreateFile(
          "NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
          ref security, OPEN_EXISTING, 0, IntPtr.Zero);
        if (stdinHandle == InvalidHandle) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not open the detached worker null input");
        }
        stdoutHandle = CreateFile(
          stdoutPath, GENERIC_WRITE, FILE_SHARE_READ,
          ref security, CREATE_NEW, 0, IntPtr.Zero);
        if (stdoutHandle == InvalidHandle) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not create the detached worker stdout record");
        }
        stderrHandle = CreateFile(
          stderrPath, GENERIC_WRITE, FILE_SHARE_READ,
          ref security, CREATE_NEW, 0, IntPtr.Zero);
        if (stderrHandle == InvalidHandle) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not create the detached worker stderr record");
        }

        UIntPtr attributeBytes = UIntPtr.Zero;
        bool sizingUnexpectedlySucceeded = InitializeProcThreadAttributeList(
          IntPtr.Zero, 2, 0, ref attributeBytes);
        int sizingError = Marshal.GetLastWin32Error();
        ulong attributeLength = attributeBytes.ToUInt64();
        if (sizingUnexpectedlySucceeded || sizingError != ERROR_INSUFFICIENT_BUFFER ||
            attributeLength == 0 || attributeLength > Int32.MaxValue) {
          throw new Win32Exception(sizingError, "Could not size the detached worker handle list");
        }
        attributeList = Marshal.AllocHGlobal(checked((int)attributeLength));
        if (!InitializeProcThreadAttributeList(attributeList, 2, 0, ref attributeBytes)) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not initialize the detached worker handle list");
        }
        attributeListInitialized = true;
        handleList = Marshal.AllocHGlobal(checked(IntPtr.Size * 3));
        Marshal.WriteIntPtr(handleList, 0, stdinHandle);
        Marshal.WriteIntPtr(handleList, IntPtr.Size, stdoutHandle);
        Marshal.WriteIntPtr(handleList, IntPtr.Size * 2, stderrHandle);
        if (!UpdateProcThreadAttribute(
          attributeList,
          0,
          new UIntPtr(PROC_THREAD_ATTRIBUTE_HANDLE_LIST),
          handleList,
          new UIntPtr((uint)(IntPtr.Size * 3)),
          IntPtr.Zero,
          IntPtr.Zero)) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not restrict detached worker handle inheritance");
        }
        jobList = Marshal.AllocHGlobal(IntPtr.Size);
        Marshal.WriteIntPtr(jobList, guardJob);
        if (!UpdateProcThreadAttribute(
          attributeList,
          0,
          new UIntPtr(PROC_THREAD_ATTRIBUTE_JOB_LIST),
          jobList,
          new UIntPtr((uint)IntPtr.Size),
          IntPtr.Zero,
          IntPtr.Zero)) {
          throw new Win32Exception(
            Marshal.GetLastWin32Error(),
            "Could not bind the detached worker to its atomic guard Job list");
        }

        STARTUPINFOEX startup = new STARTUPINFOEX();
        startup.StartupInfo.cb = Marshal.SizeOf(typeof(STARTUPINFOEX));
        startup.StartupInfo.Flags = STARTF_USESTDHANDLES;
        startup.StartupInfo.StdInput = stdinHandle;
        startup.StartupInfo.StdOutput = stdoutHandle;
        startup.StartupInfo.StdError = stderrHandle;
        startup.AttributeList = attributeList;
        string command = "\"" + executable + "\"";
        if (!String.IsNullOrEmpty(arguments)) { command += " " + arguments; }
        bool started = CreateProcess(
          executable,
          new StringBuilder(command),
          IntPtr.Zero,
          IntPtr.Zero,
          true,
          CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_BREAKAWAY_FROM_JOB |
            CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
          environmentBlock,
          workingDirectory,
          ref startup,
          out nativeProcess);
        if (!started) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "The detached worker breakaway launch is unavailable");
        }
        created = true;
        bool guarded;
        if (!IsProcessInJob(nativeProcess.Process, guardJob, out guarded)) {
          throw new Win32Exception(
            Marshal.GetLastWin32Error(),
            "Could not verify the suspended detached worker guard Job");
        }
        if (!guarded) {
          throw new InvalidOperationException(
            "The suspended detached worker was not atomically assigned to its guard Job");
        }
        long creationTime;
        long exitTime;
        long kernelTime;
        long userTime;
        if (!GetProcessTimes(
          nativeProcess.Process, out creationTime, out exitTime, out kernelTime, out userTime)) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not bind the detached worker start time");
        }
        DateTime startedAtUtc = DateTime.FromFileTimeUtc(creationTime);
        if (ResumeThread(nativeProcess.Thread) == UInt32.MaxValue) {
          throw new Win32Exception(
            Marshal.GetLastWin32Error(),
            "The guarded detached worker could not be resumed");
        }
        DetachedWorkerProcess result = new DetachedWorkerProcess(
          nativeProcess.Process, guardJob, nativeProcess.ProcessId, startedAtUtc);
        nativeProcess.Process = IntPtr.Zero;
        guardJob = IntPtr.Zero;
        ownershipTransferred = true;
        return result;
      }
      catch (Exception error) {
        primaryFailure = error;
        throw;
      }
      finally {
        Exception terminationFailure = null;
        if (created && !ownershipTransferred && nativeProcess.Process != IntPtr.Zero) {
          terminationFailure = TerminateAndVerify(nativeProcess.Process);
        }
        if (nativeProcess.Thread != IntPtr.Zero) { CloseHandle(nativeProcess.Thread); }
        if (nativeProcess.Process != IntPtr.Zero) { CloseHandle(nativeProcess.Process); }
        if (attributeListInitialized) { DeleteProcThreadAttributeList(attributeList); }
        if (attributeList != IntPtr.Zero) { Marshal.FreeHGlobal(attributeList); }
        if (handleList != IntPtr.Zero) { Marshal.FreeHGlobal(handleList); }
        if (jobList != IntPtr.Zero) { Marshal.FreeHGlobal(jobList); }
        if (stdinHandle != IntPtr.Zero && stdinHandle != InvalidHandle) { CloseHandle(stdinHandle); }
        if (stdoutHandle != IntPtr.Zero && stdoutHandle != InvalidHandle) { CloseHandle(stdoutHandle); }
        if (stderrHandle != IntPtr.Zero && stderrHandle != InvalidHandle) { CloseHandle(stderrHandle); }
        if (guardJob != IntPtr.Zero) { CloseHandle(guardJob); }
        if (environmentBlock != IntPtr.Zero) { Marshal.FreeHGlobal(environmentBlock); }
        if (terminationFailure != null) {
          if (primaryFailure != null) {
            throw new DetachedWorkerCleanupException(
              "The detached worker launch failed and exact cleanup also failed",
              new AggregateException(new Exception[] { primaryFailure, terminationFailure }));
          }
          throw new DetachedWorkerCleanupException(
            "The detached worker launch cleanup failed",
            terminationFailure);
        }
      }
    }

    private static IntPtr CreateKillOnCloseJob() {
      IntPtr job = CreateJobObject(IntPtr.Zero, null);
      if (job == IntPtr.Zero) {
        throw new Win32Exception(
          Marshal.GetLastWin32Error(),
          "Could not create the detached worker guard Job");
      }
      JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits =
        new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
      limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE |
        JOB_OBJECT_LIMIT_BREAKAWAY_OK;
      int size = Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION));
      IntPtr buffer = Marshal.AllocHGlobal(size);
      try {
        Marshal.StructureToPtr(limits, buffer, false);
        if (!SetInformationJobObject(job, 9, buffer, (uint)size)) {
          int error = Marshal.GetLastWin32Error();
          CloseHandle(job);
          throw new Win32Exception(error, "Could not configure the detached worker guard Job");
        }
      }
      finally {
        Marshal.FreeHGlobal(buffer);
      }
      return job;
    }

    private static Exception TerminateAndVerify(IntPtr process) {
      bool requested = TerminateProcess(process, 1);
      int terminationError = Marshal.GetLastWin32Error();
      uint wait = WaitForSingleObject(process, requested ? TERMINATION_WAIT_MS : 0);
      if (wait == WAIT_OBJECT_0) { return null; }
      if (!requested) {
        return new Win32Exception(terminationError, "The suspended detached worker could not be terminated");
      }
      if (wait == WAIT_TIMEOUT) {
        return new TimeoutException("The suspended detached worker did not terminate within the cleanup bound");
      }
      int waitError = wait == WAIT_FAILED ? Marshal.GetLastWin32Error() : unchecked((int)wait);
      return new Win32Exception(waitError, "The suspended detached worker termination could not be verified");
    }

    private static IntPtr BuildEnvironment(IDictionary input) {
      if (input == null) { throw new ArgumentNullException("environment"); }
      SortedDictionary<string, string> values =
        new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase);
      foreach (DictionaryEntry item in input) {
        string name = item.Key as string;
        string value = item.Value as string;
        if (String.IsNullOrWhiteSpace(name) || name.IndexOf('=') >= 0 ||
            name.IndexOf('\0') >= 0 || value == null || value.IndexOf('\0') >= 0) {
          throw new ArgumentException("The detached worker environment contains an invalid entry");
        }
        values[name] = value;
      }
      StringBuilder block = new StringBuilder();
      foreach (KeyValuePair<string, string> item in values) {
        block.Append(item.Key).Append('=').Append(item.Value).Append('\0');
      }
      block.Append('\0');
      return Marshal.StringToHGlobalUni(block.ToString());
    }
  }
}
'@
    }
    return [LbbCoordinator.NativeDetachedWorkerLauncher]::Start(
        $Executable,
        $Arguments,
        $WorkingDirectory,
        $StdoutPath,
        $StderrPath,
        $Environment
    )
}

function Stop-DetachedWorkerProcessExact {
    param(
        [Parameter(Mandatory = $true)][object]$Process,
        [Parameter(Mandatory = $true)][string]$Description
    )
    if (-not $Process.HasExited) {
        $Process.Kill()
    }
    if (-not $Process.WaitForExit(5000) -or -not $Process.HasExited) {
        throw "$Description termination could not be confirmed."
    }
}

function Test-ExceptionChainTypeName {
    param([System.Exception]$Exception, [string]$FullName)
    $current = $Exception
    $depth = 0
    while ($null -ne $current -and $depth -lt 16) {
        if ($current.GetType().FullName -ceq $FullName) { return $true }
        $current = $current.InnerException
        $depth++
    }
    return $false
}

function Test-Win32ErrorInChain {
    param(
        [Parameter(Mandatory = $true)][Exception]$Exception,
        [Parameter(Mandatory = $true)][int]$NativeErrorCode
    )
    $current = $Exception
    $depth = 0
    while ($null -ne $current -and $depth -lt 16) {
        if ($current -is [ComponentModel.Win32Exception] -and
            $current.NativeErrorCode -eq $NativeErrorCode) {
            return $true
        }
        $current = $current.InnerException
        $depth++
    }
    return $false
}

function Get-WorkerLifetimeSupportSource {
    param([switch]$IncludeSelfTestHooks)
    $source = @'
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

namespace LbbCoordinator {
  public sealed class WorkerLifetimeJob {
    private const int ERROR_FILE_NOT_FOUND = 2;
    private const int ERROR_ALREADY_EXISTS = 183;
    private const uint JOB_OBJECT_QUERY = 0x0004;
    private const uint JOB_OBJECT_TERMINATE = 0x0008;
    private const int JobObjectBasicAccountingInformation = 1;
    private const int JobObjectExtendedLimitInformation = 9;
    private const uint JOB_OBJECT_LIMIT_BREAKAWAY_OK = 0x00000800;
    private const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
    private IntPtr handle;
    public bool IsBound { get { return handle != IntPtr.Zero; } }
    public bool RecoveredExistingJob { get; private set; }

__LBB_WORKER_LIFETIME_SELF_TEST_FACTORY__

    [StructLayout(LayoutKind.Sequential)]
    private struct IO_COUNTERS {
      public ulong ReadOperationCount;
      public ulong WriteOperationCount;
      public ulong OtherOperationCount;
      public ulong ReadTransferCount;
      public ulong WriteTransferCount;
      public ulong OtherTransferCount;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
      public long PerProcessUserTimeLimit;
      public long PerJobUserTimeLimit;
      public uint LimitFlags;
      public UIntPtr MinimumWorkingSetSize;
      public UIntPtr MaximumWorkingSetSize;
      public uint ActiveProcessLimit;
      public UIntPtr Affinity;
      public uint PriorityClass;
      public uint SchedulingClass;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
      public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
      public IO_COUNTERS IoInfo;
      public UIntPtr ProcessMemoryLimit;
      public UIntPtr JobMemoryLimit;
      public UIntPtr PeakProcessMemoryUsed;
      public UIntPtr PeakJobMemoryUsed;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct JOBOBJECT_BASIC_ACCOUNTING_INFORMATION {
      public long TotalUserTime;
      public long TotalKernelTime;
      public long ThisPeriodTotalUserTime;
      public long ThisPeriodTotalKernelTime;
      public uint TotalPageFaultCount;
      public uint TotalProcesses;
      public uint ActiveProcesses;
      public uint TotalTerminatedProcesses;
    }

    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern IntPtr CreateJobObject(IntPtr attributes, string name);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern IntPtr OpenJobObject(
      uint desiredAccess,
      [MarshalAs(UnmanagedType.Bool)] bool inheritHandle,
      string name);
    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern bool SetInformationJobObject(IntPtr job, int infoClass, IntPtr info, uint length);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool QueryInformationJobObject(
      IntPtr job,
      int infoClass,
      out JOBOBJECT_BASIC_ACCOUNTING_INFORMATION information,
      uint length,
      IntPtr returnLength);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool TerminateJobObject(IntPtr job, uint exitCode);
    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern void SetLastError(uint errorCode);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseHandle(IntPtr handle);

    public WorkerLifetimeJob(
      bool allowChildBreakaway,
      string name,
      bool recoverExisting,
      int recoveryTimeoutMilliseconds)
      : this(
        allowChildBreakaway,
        name,
        recoverExisting,
        recoveryTimeoutMilliseconds,
        null) {
    }

    private WorkerLifetimeJob(
      bool allowChildBreakaway,
      string name,
      bool recoverExisting,
      int recoveryTimeoutMilliseconds,
      Action beforeFreshCreateForSelfTest) {
      if (recoveryTimeoutMilliseconds < 1) {
        throw new ArgumentOutOfRangeException("recoveryTimeoutMilliseconds");
      }
      if (recoverExisting && String.IsNullOrEmpty(name)) {
        throw new ArgumentException("A recovery Job must have a stable name", "name");
      }

      Stopwatch recoveryDeadline = recoverExisting && !String.IsNullOrEmpty(name)
        ? Stopwatch.StartNew()
        : null;
      bool recoveredExistingJob = false;
      if (!String.IsNullOrEmpty(name)) {
        IntPtr previous = OpenJobObject(
          JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE,
          false,
          name);
        if (previous != IntPtr.Zero) {
          try {
            if (!recoverExisting) {
              throw new InvalidOperationException("The named coordinator lifetime Job already exists");
            }
            TerminateAndWaitForEmpty(previous, recoveryDeadline, recoveryTimeoutMilliseconds);
            recoveredExistingJob = true;
          }
          finally {
            CloseJobHandleOnce(
              previous,
              "Could not close the inspected prior coordinator lifetime Job");
          }
          WaitForJobNameToDisappear(name, recoveryDeadline, recoveryTimeoutMilliseconds);
        }
        else {
          int openError = Marshal.GetLastWin32Error();
          if (openError != ERROR_FILE_NOT_FOUND) {
            throw new Win32Exception(openError, "Could not inspect the stable coordinator lifetime Job");
          }
        }
      }

      EnsureRecoveryTimeRemaining(
        recoveryDeadline,
        recoveryTimeoutMilliseconds,
        "The coordinator lifetime Job recovery deadline expired before fresh creation");
      if (beforeFreshCreateForSelfTest != null) {
        beforeFreshCreateForSelfTest();
        EnsureRecoveryTimeRemaining(
          recoveryDeadline,
          recoveryTimeoutMilliseconds,
          "The coordinator lifetime Job recovery deadline expired before fresh creation");
      }

      // CreateJobObject reports pre-existence through last-error even when it
      // returns a valid handle. Clear stale thread state, invoke it exactly
      // once, and capture the status before any other native call.
      IntPtr candidate = CreateFreshJobOnce(name);
      try {
        EnsureRecoveryTimeRemaining(
          recoveryDeadline,
          recoveryTimeoutMilliseconds,
          "The coordinator lifetime Job recovery deadline expired during fresh creation");
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE |
          (allowChildBreakaway ? JOB_OBJECT_LIMIT_BREAKAWAY_OK : 0);
        int size = Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION));
        IntPtr buffer = IntPtr.Zero;
        buffer = Marshal.AllocHGlobal(size);
        try {
          Marshal.StructureToPtr(limits, buffer, false);
          if (!SetInformationJobObject(candidate, JobObjectExtendedLimitInformation, buffer, (uint)size)) {
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not configure the coordinator lifetime job");
          }
        }
        finally {
          Marshal.FreeHGlobal(buffer);
        }
        EnsureRecoveryTimeRemaining(
          recoveryDeadline,
          recoveryTimeoutMilliseconds,
          "The coordinator lifetime Job recovery deadline expired during configuration");
        if (!AssignProcessToJobObject(candidate, GetCurrentProcess())) {
          int error = Marshal.GetLastWin32Error();
          throw new Win32Exception(error, "Could not bind the coordinator worker to its lifetime job");
        }
        EnsureRecoveryTimeRemaining(
          recoveryDeadline,
          recoveryTimeoutMilliseconds,
          "The coordinator lifetime Job recovery deadline expired during worker binding");
        if (recoveryDeadline != null) { recoveryDeadline.Stop(); }
        handle = candidate;
        candidate = IntPtr.Zero;
        RecoveredExistingJob = recoveredExistingJob;
      }
      finally {
        if (candidate != IntPtr.Zero) {
          CloseJobHandleOnce(
            candidate,
            "Could not close an unbound coordinator lifetime Job handle");
        }
      }
    }

    private static IntPtr CreateFreshJobOnce(string name) {
      SetLastError(0);
      IntPtr candidate = CreateJobObject(IntPtr.Zero, name);
      int createError = Marshal.GetLastWin32Error();
      if (candidate == IntPtr.Zero) {
        if (createError == 0) {
          throw new InvalidOperationException(
            "CreateJobObject returned a null handle without a failure status");
        }
        throw new Win32Exception(createError, "Could not create the coordinator lifetime job");
      }
      if (createError == 0) {
        return candidate;
      }

      CloseJobHandleOnce(
        candidate,
        "Could not close an uninspected coordinator lifetime Job handle");
      throw new Win32Exception(
        createError,
        createError == ERROR_ALREADY_EXISTS
          ? "CreateJobObject returned an existing uninspected coordinator lifetime Job"
          : "CreateJobObject returned a handle with a nonzero failure status");
    }

    private static void WaitForJobNameToDisappear(
      string name,
      Stopwatch recoveryDeadline,
      int timeoutMilliseconds) {
      while (true) {
        EnsureRecoveryTimeRemaining(
          recoveryDeadline,
          timeoutMilliseconds,
          "The prior coordinator lifetime Job name did not leave the namespace");
        IntPtr observed = OpenJobObject(JOB_OBJECT_QUERY, false, name);
        if (observed == IntPtr.Zero) {
          int openError = Marshal.GetLastWin32Error();
          if (openError == ERROR_FILE_NOT_FOUND) {
            EnsureRecoveryTimeRemaining(
              recoveryDeadline,
              timeoutMilliseconds,
              "The prior coordinator lifetime Job name did not leave the namespace");
            return;
          }
          throw new Win32Exception(
            openError,
            "Could not poll the prior coordinator lifetime Job namespace");
        }
        CloseJobHandleOnce(
          observed,
          "Could not close a polled coordinator lifetime Job handle");
        SleepForRecoveryPoll(
          recoveryDeadline,
          timeoutMilliseconds,
          "The prior coordinator lifetime Job name did not leave the namespace");
      }
    }

    private static void TerminateAndWaitForEmpty(
      IntPtr job,
      Stopwatch recoveryDeadline,
      int timeoutMilliseconds) {
      EnsureRecoveryTimeRemaining(
        recoveryDeadline,
        timeoutMilliseconds,
        "The prior coordinator Job recovery deadline expired before termination");
      if (!TerminateJobObject(job, 1)) {
        throw new Win32Exception(
          Marshal.GetLastWin32Error(),
          "Could not terminate the prior coordinator lifetime Job");
      }
      while (true) {
        EnsureRecoveryTimeRemaining(
          recoveryDeadline,
          timeoutMilliseconds,
          "The prior coordinator Job did not reach ACTIVE_PROCESS_ZERO");
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION accounting;
        if (!QueryInformationJobObject(
          job,
          JobObjectBasicAccountingInformation,
          out accounting,
          (uint)Marshal.SizeOf(typeof(JOBOBJECT_BASIC_ACCOUNTING_INFORMATION)),
          IntPtr.Zero)) {
          throw new Win32Exception(
            Marshal.GetLastWin32Error(),
            "Could not inspect prior coordinator Job teardown");
        }
        if (accounting.ActiveProcesses == 0) {
          EnsureRecoveryTimeRemaining(
            recoveryDeadline,
            timeoutMilliseconds,
            "The prior coordinator Job did not reach ACTIVE_PROCESS_ZERO");
          return;
        }
        SleepForRecoveryPoll(
          recoveryDeadline,
          timeoutMilliseconds,
          "The prior coordinator Job did not reach ACTIVE_PROCESS_ZERO");
      }
    }

    private static void SleepForRecoveryPoll(
      Stopwatch recoveryDeadline,
      int timeoutMilliseconds,
      string timeoutMessage) {
      double remaining = timeoutMilliseconds - recoveryDeadline.Elapsed.TotalMilliseconds;
      if (remaining <= 0.0) {
        throw new TimeoutException(timeoutMessage);
      }
      int sleepMilliseconds = (int)Math.Min(25.0, Math.Floor(remaining));
      Thread.Sleep(Math.Max(0, sleepMilliseconds));
    }

    private static void EnsureRecoveryTimeRemaining(
      Stopwatch recoveryDeadline,
      int timeoutMilliseconds,
      string timeoutMessage) {
      if (recoveryDeadline != null &&
          recoveryDeadline.Elapsed.TotalMilliseconds >= timeoutMilliseconds) {
        throw new TimeoutException(timeoutMessage);
      }
    }

    private static void CloseJobHandleOnce(IntPtr value, string failureMessage) {
      if (!CloseHandle(value)) {
        int error = Marshal.GetLastWin32Error();
        throw new Win32Exception(error, failureMessage);
      }
    }

    // This handle deliberately has process lifetime. A finalizer could close
    // the last KILL_ON_JOB_CLOSE handle while the PowerShell worker is still
    // running and terminate that worker. The OS closes the handle atomically
    // with process teardown, which is the ownership boundary we need.
  }
}
'@
    $selfTestFactory = ""
    if ($IncludeSelfTestHooks) {
        $selfTestFactory = @'
    public static WorkerLifetimeJob CreateForSelfTest(
      bool allowChildBreakaway,
      string name,
      bool recoverExisting,
      int recoveryTimeoutMilliseconds,
      Action beforeFreshCreate) {
      if (beforeFreshCreate == null) {
        throw new ArgumentNullException("beforeFreshCreate");
      }
      return new WorkerLifetimeJob(
        allowChildBreakaway,
        name,
        recoverExisting,
        recoveryTimeoutMilliseconds,
        beforeFreshCreate);
    }

    public static void WaitForNameAbsenceForSelfTest(
      string name,
      int timeoutMilliseconds) {
      if (String.IsNullOrEmpty(name)) {
        throw new ArgumentException("A self-test Job name is required", "name");
      }
      if (timeoutMilliseconds < 1) {
        throw new ArgumentOutOfRangeException("timeoutMilliseconds");
      }
      Stopwatch deadline = Stopwatch.StartNew();
      WaitForJobNameToDisappear(name, deadline, timeoutMilliseconds);
    }
'@
    }
    return $source.Replace(
        "__LBB_WORKER_LIFETIME_SELF_TEST_FACTORY__",
        $selfTestFactory
    )
}

function New-WorkerLifetimeSupportAssembly {
    param([Parameter(Mandatory = $true)][string]$OutputPath)
    $parent = Resolve-OrdinaryPath ([IO.Path]::GetDirectoryName($OutputPath)) $false "Worker support parent"
    $expected = Assert-NewChildPath $parent ([IO.Path]::GetFileName($OutputPath))
    $null = Add-Type `
        -TypeDefinition (Get-WorkerLifetimeSupportSource) `
        -Language CSharp `
        -OutputAssembly $expected `
        -OutputType Library
    $resolved = Resolve-OrdinaryPath $expected $true "Worker lifetime support assembly"
    return [pscustomobject]@{
        Path = $resolved
        Sha256 = Get-FileSha256 $resolved
    }
}

function Initialize-WorkerLifetimeSupport {
    param([string]$AssemblyPath, [string]$AssemblySha256, [switch]$SelfTestCompile)
    if ("LbbCoordinator.WorkerLifetimeJob" -as [type]) { return }
    if ($SelfTestCompile) {
        Add-Type -TypeDefinition (Get-WorkerLifetimeSupportSource -IncludeSelfTestHooks)
        return
    }
    if ($AssemblySha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw "The worker lifetime support SHA-256 is invalid."
    }
    $resolved = Resolve-OrdinaryPath $AssemblyPath $true "Worker lifetime support assembly"
    $assemblyBytes = [IO.File]::ReadAllBytes($resolved)
    if ($assemblyBytes.Length -lt 1 -or $assemblyBytes.Length -gt 1048576) {
        throw "The worker lifetime support assembly size is invalid."
    }
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        $loadedAssemblySha256 = ([BitConverter]::ToString(
            $sha256.ComputeHash($assemblyBytes)
        )).Replace("-", "").ToLowerInvariant()
    }
    finally { $sha256.Dispose() }
    if ($loadedAssemblySha256 -cne $AssemblySha256) {
        throw "The worker lifetime support assembly hash does not match its private configuration."
    }
    # Load exactly the byte array whose digest was checked. A path-based load
    # would reopen the file and create a same-account hash/load race.
    $null = [Reflection.Assembly]::Load($assemblyBytes)
    [Array]::Clear($assemblyBytes, 0, $assemblyBytes.Length)
    if (-not ("LbbCoordinator.WorkerLifetimeJob" -as [type])) {
        throw "The worker lifetime support assembly did not expose its expected type."
    }
}

function Invoke-WorkerSupportLoaderSelfTest {
    param(
        [Parameter(Mandatory = $true)][string]$AssemblyPath,
        [Parameter(Mandatory = $true)][string]$AssemblySha256,
        [Parameter(Mandatory = $true)][string]$Nonce
    )
    $environmentNonce = [Environment]::GetEnvironmentVariable(
        "LBB_COORDINATOR_WORKER_SUPPORT_SELF_TEST_NONCE",
        "Process"
    )
    [Environment]::SetEnvironmentVariable(
        "LBB_COORDINATOR_WORKER_SUPPORT_SELF_TEST_NONCE",
        $null,
        "Process"
    )
    if ($Nonce -cnotmatch '^[0-9a-f]{32}$' -or $environmentNonce -cne $Nonce) {
        throw "The staged worker-support loader self-test nonce is invalid."
    }
    if ("LbbCoordinator.WorkerLifetimeJob" -as [type]) {
        throw "The staged worker-support loader self-test did not start in a fresh process."
    }
    $incorrectSha256 = if ($AssemblySha256 -cne ("0" * 64)) { "0" * 64 } else { "1" * 64 }
    $incorrectHashRefused = $false
    try {
        Initialize-WorkerLifetimeSupport `
            -AssemblyPath $AssemblyPath `
            -AssemblySha256 $incorrectSha256
    }
    catch {
        $incorrectHashRefused = $_.Exception.Message -ceq `
            "The worker lifetime support assembly hash does not match its private configuration."
    }
    if (-not $incorrectHashRefused -or ("LbbCoordinator.WorkerLifetimeJob" -as [type])) {
        throw "The staged worker-support loader did not reject an incorrect digest before loading."
    }
    Initialize-WorkerLifetimeSupport `
        -AssemblyPath $AssemblyPath `
        -AssemblySha256 $AssemblySha256
    if (-not ("LbbCoordinator.WorkerLifetimeJob" -as [type])) {
        throw "The staged worker-support loader self-test did not load its expected type."
    }
    $probeJob = [LbbCoordinator.WorkerLifetimeJob]::new(
        $true,
        "Local\LBBWindowsAcceptanceCoordinatorLoaderSelfTest-$Nonce",
        $false,
        2000
    )
    if (-not $probeJob.IsBound -or $probeJob.RecoveredExistingJob) {
        throw "The staged worker-support loader self-test did not bind a fresh isolated Job."
    }
    [GC]::KeepAlive($probeJob)
    Write-Output "Worker lifetime support staged-loader self-test passed."
}

function New-WorkerLifetimeJob {
    param(
        [switch]$AllowChildBreakaway,
        [string]$Name,
        [switch]$RecoverExisting,
        [ValidateRange(1, 60000)]
        [int]$RecoveryTimeoutMilliseconds = $script:WorkerLifetimeRecoveryMilliseconds,
        [string]$SupportAssemblyPath,
        [string]$SupportAssemblySha256
    )
    Initialize-WorkerLifetimeSupport `
        -AssemblyPath $SupportAssemblyPath `
        -AssemblySha256 $SupportAssemblySha256 `
        -SelfTestCompile:$AllowChildBreakaway
    $job = [LbbCoordinator.WorkerLifetimeJob]::new(
        [bool]$AllowChildBreakaway,
        $Name,
        [bool]$RecoverExisting,
        $RecoveryTimeoutMilliseconds
    )
    if (-not $job.IsBound) { throw "The coordinator lifetime job is not bound." }
    return $job
}

function Start-CapturedProcess {
    param(
        [Parameter(Mandatory = $true)][Diagnostics.ProcessStartInfo]$Info,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )
    if ([IO.File]::Exists($StdoutPath) -or [IO.File]::Exists($StderrPath)) {
        throw "Captured process logs must be fresh."
    }
    $stdout = $null
    $stderr = $null
    $process = $null
    $started = $false
    $stdoutTask = $null
    $stderrTask = $null
    try {
        $stdout = [IO.File]::Open($StdoutPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        $stderr = [IO.File]::Open($StderrPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        $process = [Diagnostics.Process]::new()
        $Info.RedirectStandardOutput = $true
        $Info.RedirectStandardError = $true
        $process.StartInfo = $Info
        if (-not $process.Start()) { throw "The captured child process did not start." }
        $started = $true
        $stdoutTask = $process.StandardOutput.BaseStream.CopyToAsync($stdout)
        $stderrTask = $process.StandardError.BaseStream.CopyToAsync($stderr)
        return [pscustomobject]@{
            Process = $process
            StdoutStream = $stdout
            StderrStream = $stderr
            StdoutTask = $stdoutTask
            StderrTask = $stderrTask
            StartedAtUtc = $process.StartTime.ToUniversalTime()
        }
    }
    catch {
        if ($started) {
            try {
                if (-not $process.HasExited) { $process.Kill() }
                $null = $process.WaitForExit(5000)
            }
            catch {
                # Preserve the original launch/capture failure after making the
                # bounded best effort to terminate the exact child.
            }
        }
        if ($null -ne $stdoutTask) {
            try {
                if ($stdoutTask.Wait(5000)) { $null = $stdoutTask.GetAwaiter().GetResult() }
            }
            catch {}
        }
        if ($null -ne $stderrTask) {
            try {
                if ($stderrTask.Wait(5000)) { $null = $stderrTask.GetAwaiter().GetResult() }
            }
            catch {}
        }
        if ($null -ne $process) { $process.Dispose() }
        if ($null -ne $stdout) { $stdout.Dispose() }
        if ($null -ne $stderr) { $stderr.Dispose() }
        throw
    }
}

function Complete-CapturedProcess {
    param(
        [Parameter(Mandatory = $true)][object]$Capture,
        [ValidateRange(1, 3600000)][int]$TimeoutMilliseconds = 600000,
        [ValidateRange(1, 60000)][int]$StreamTimeoutMilliseconds = $script:MaximumStreamDrainMilliseconds
    )
    $timedOut = $false
    try {
        if (-not $Capture.Process.WaitForExit($TimeoutMilliseconds)) {
            $timedOut = $true
            if (-not $Capture.Process.HasExited) { $Capture.Process.Kill() }
            if (-not $Capture.Process.WaitForExit(5000) -or -not $Capture.Process.HasExited) {
                throw "The exact captured process exceeded its deadline and termination could not be confirmed."
            }
        }
        if (-not $Capture.StdoutTask.Wait($StreamTimeoutMilliseconds) -or
            -not $Capture.StderrTask.Wait($StreamTimeoutMilliseconds)) {
            throw "The captured process streams did not finish within their bounded drain interval."
        }
        $null = $Capture.StdoutTask.GetAwaiter().GetResult()
        $null = $Capture.StderrTask.GetAwaiter().GetResult()
        $Capture.StdoutStream.Flush($true)
        $Capture.StderrStream.Flush($true)
        if ($timedOut) {
            throw "The exact captured process exceeded its execution deadline and was terminated."
        }
        return [int]$Capture.Process.ExitCode
    }
    finally {
        $Capture.StdoutStream.Dispose()
        $Capture.StderrStream.Dispose()
    }
}

function Test-BoundProcessAlive {
    param([Parameter(Mandatory = $true)][int]$ProcessId, [Parameter(Mandatory = $true)][DateTime]$StartedAtUtc)
    try {
        $process = [Diagnostics.Process]::GetProcessById($ProcessId)
        try {
            return (-not $process.HasExited -and
                $process.StartTime.ToUniversalTime().Ticks -eq $StartedAtUtc.ToUniversalTime().Ticks)
        }
        finally { $process.Dispose() }
    }
    catch { return $false }
}

function Assert-InteractiveInputDesktop {
    $current = [Diagnostics.Process]::GetCurrentProcess()
    try { $currentSessionId = $current.SessionId }
    finally { $current.Dispose() }
    if ($currentSessionId -le 0 -or -not [Environment]::UserInteractive) {
        throw "Start requires the signed-in interactive Windows session."
    }
    if (-not ("LbbCoordinator.NativeDesktopProbe" -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace LbbCoordinator {
  public static class NativeDesktopProbe {
    [DllImport("user32.dll", SetLastError=true)]
    private static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll", SetLastError=true)]
    private static extern bool CloseDesktop(IntPtr desktop);
    public static bool IsAccessible() {
      IntPtr desktop = OpenInputDesktop(0, false, 0x0100);
      if (desktop == IntPtr.Zero) return false;
      if (!CloseDesktop(desktop)) {
        throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not close the input desktop probe handle");
      }
      return true;
    }
  }
}
'@
    }
    if (-not [LbbCoordinator.NativeDesktopProbe]::IsAccessible()) {
        throw "Start requires access to the current input desktop."
    }
}

function New-BridgeToken {
    $bytes = [byte[]]::new(32)
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    try { $rng.GetBytes($bytes) }
    finally { $rng.Dispose() }
    return [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
}

function Get-CoordinatorFiles {
    param([Parameter(Mandatory = $true)][string]$Root)
    return [ordered]@{
        Config = [IO.Path]::Combine($Root, "private-config.json")
        Start = [IO.Path]::Combine($Root, "01-start-request.json")
        Worker = [IO.Path]::Combine($Root, "02-worker-started.json")
        Ownership = [IO.Path]::Combine($Root, "03-worker-ownership-transferred.json")
        Intent = [IO.Path]::Combine($Root, "04-runner-launch-intent.json")
        Runner = [IO.Path]::Combine($Root, "05-runner-started.json")
        Watcher = [IO.Path]::Combine($Root, "06-watcher-finished.json")
        Handoff = [IO.Path]::Combine($Root, "07-handoff.json")
        Final = [IO.Path]::Combine($Root, "08-runner-finished.json")
        Failure = [IO.Path]::Combine($Root, "99-terminal-failure.json")
        RunnerOut = [IO.Path]::Combine($Root, "runner.stdout.log")
        RunnerErr = [IO.Path]::Combine($Root, "runner.stderr.log")
        WatcherOut = [IO.Path]::Combine($Root, "watcher.stdout.log")
        WatcherErr = [IO.Path]::Combine($Root, "watcher.stderr.log")
        WorkerOut = [IO.Path]::Combine($Root, "worker.stdout.log")
        WorkerErr = [IO.Path]::Combine($Root, "worker.stderr.log")
        WorkerSupport = [IO.Path]::Combine($Root, "worker-lifetime-support.dll")
    }
}

function Assert-ExactStringValue {
    param([object]$Actual, [string]$Expected, [string]$Description)
    if ($Actual -isnot [string] -or $Actual -cne $Expected) {
        throw "$Description is invalid."
    }
}

function Assert-ExactConfiguration {
    param(
        [Parameter(Mandatory = $true)][object]$Config,
        [switch]$BeforeReservation,
        [switch]$ForeignReservation
    )
    if ($BeforeReservation -and $ForeignReservation) {
        throw "Configuration reservation validation modes are mutually exclusive."
    }
    $fields = @(
        "schemaVersion", "version", "sourceDirectory", "coordinatorScript", "runnerScript",
        "watcherScript", "serverPath", "helperPath", "checksumManifest",
        "checksumManifestSha256", "candidateBindingPath", "fixturePath", "evidenceDirectory",
        "coordinatorDirectory", "foregroundArmTimeoutSeconds", "attemptKey",
        "attemptLedgerPath", "coordinatorInstanceId", "workerSupportAssembly", "workerSupportSha256",
        "coordinatorScriptSha256", "runnerScriptSha256", "watcherScriptSha256",
        "fixtureSha256", "serverSha256", "helperSha256", "candidateBindingSha256"
    )
    Assert-ExactPropertyOrder $Config $fields "Private coordinator configuration"
    Assert-ExactIntegerRange $Config.schemaVersion $script:SchemaVersion $script:SchemaVersion "configuration schemaVersion"
    Assert-ExactStringValue $Config.version $script:ProductVersion "configuration version"
    Assert-ExactIntegerRange $Config.foregroundArmTimeoutSeconds 15 300 "configuration foregroundArmTimeoutSeconds"
    if ($Config.checksumManifestSha256 -isnot [string] -or
        $Config.checksumManifestSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $Config.attemptKey -isnot [string] -or $Config.attemptKey -cnotmatch '^[0-9a-f]{64}$' -or
        $Config.coordinatorInstanceId -isnot [string] -or
        $Config.coordinatorInstanceId -cnotmatch '^[0-9a-f]{32}$' -or
        $Config.workerSupportSha256 -isnot [string] -or
        $Config.workerSupportSha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw "The private coordinator hashes are invalid."
    }
    foreach ($hashField in @(
        "coordinatorScriptSha256", "runnerScriptSha256", "watcherScriptSha256",
        "fixtureSha256", "serverSha256", "helperSha256", "candidateBindingSha256"
    )) {
        if ($Config.$hashField -isnot [string] -or $Config.$hashField -cnotmatch '^[0-9a-f]{64}$') {
            throw "The private coordinator staged-file hashes are invalid."
        }
    }

    $source = Resolve-OrdinaryPath ([string]$Config.sourceDirectory) $false "Configured source directory"
    $coordinatorScript = Resolve-OrdinaryPath ([string]$Config.coordinatorScript) $true "Configured coordinator script"
    $runner = Resolve-OrdinaryPath ([string]$Config.runnerScript) $true "Configured runner script"
    $watcher = Resolve-OrdinaryPath ([string]$Config.watcherScript) $true "Configured watcher script"
    $server = Resolve-OrdinaryPath ([string]$Config.serverPath) $true "Configured server candidate"
    $helper = Resolve-OrdinaryPath ([string]$Config.helperPath) $true "Configured helper candidate"
    $manifest = Resolve-OrdinaryPath ([string]$Config.checksumManifest) $true "Configured checksum manifest"
    $binding = Resolve-OrdinaryPath ([string]$Config.candidateBindingPath) $true "Configured candidate binding"
    $fixture = Resolve-OrdinaryPath ([string]$Config.fixturePath) $true "Configured fixture"
    $evidence = Resolve-OrdinaryPath ([string]$Config.evidenceDirectory) $false "Configured evidence directory"
    $coordinator = Resolve-OrdinaryPath ([string]$Config.coordinatorDirectory) $false "Configured coordinator directory"
    $support = Resolve-OrdinaryPath ([string]$Config.workerSupportAssembly) $true "Configured worker support assembly"
    $ledger = if ($BeforeReservation) {
        Resolve-NewOrdinaryPath `
            ([string]$Config.attemptLedgerPath) `
            "Prospective configured attempt reservation"
    }
    else {
        Resolve-OrdinaryPath `
            ([string]$Config.attemptLedgerPath) `
            $true `
            "Configured attempt reservation"
    }
    Assert-PrivateDirectoryAcl $evidence
    Assert-PrivateDirectoryAcl $coordinator
    Assert-PrivateDirectoryAcl $source
    Assert-PrivateDirectoryAcl ([IO.Path]::GetDirectoryName($runner))
    Assert-PrivateDirectoryAcl ([IO.Path]::GetDirectoryName($fixture))
    Assert-PrivateDirectoryAcl ([IO.Path]::GetDirectoryName($server))
    Assert-PrivateDirectoryAcl ([IO.Path]::GetDirectoryName($ledger))

    if ($source -cne [IO.Path]::Combine($coordinator, "staged-source") -or
        $coordinatorScript -cne [IO.Path]::Combine($source, "scripts", "run-windows-computer-use-acceptance.ps1") -or
        $runner -cne [IO.Path]::Combine($source, "scripts", "test-windows-computer-use.ps1") -or
        $watcher -cne [IO.Path]::Combine($source, "scripts", "wait-windows-foreground-arm-handoff.ps1") -or
        $fixture -cne [IO.Path]::Combine($source, "tests", "fixtures", "windows", "WindowsComputerUseFixture.ps1") -or
        $support -cne [IO.Path]::Combine($coordinator, "worker-lifetime-support.dll") -or
        $manifest -cne [IO.Path]::Combine($coordinator, "candidate", "SHA256SUMS.txt") -or
        $binding -cne [IO.Path]::Combine($coordinator, "candidate", "candidate-binding.json") -or
        [IO.Path]::GetDirectoryName($server) -cne [IO.Path]::Combine($coordinator, "candidate") -or
        [IO.Path]::GetDirectoryName($helper) -cne [IO.Path]::Combine($coordinator, "candidate") -or
        [IO.Path]::GetFileName($ledger) -cne "attempt-$($Config.attemptKey).json") {
        throw "The private coordinator paths are not canonically bound."
    }
    if ([IO.Path]::GetDirectoryName($coordinator) -cne (Get-PrivateAcceptanceParent "Coordinator") -or
        [IO.Path]::GetDirectoryName($evidence) -cne (Get-PrivateAcceptanceParent "Evidence")) {
        throw "The acceptance directories are outside their fixed owner-private parents."
    }
    $expectedLedgerRoot = Get-AttemptLedgerRoot
    if ([IO.Path]::GetDirectoryName($ledger) -cne $expectedLedgerRoot) {
        throw "The attempt reservation is outside the persistent owner-private ledger."
    }
    if ((Get-FileSha256 $support) -cne [string]$Config.workerSupportSha256) {
        throw "The worker support assembly hash is invalid."
    }
    $stagedBindings = [ordered]@{
        coordinatorScriptSha256 = $coordinatorScript
        runnerScriptSha256 = $runner
        watcherScriptSha256 = $watcher
        fixtureSha256 = $fixture
        serverSha256 = $server
        helperSha256 = $helper
        candidateBindingSha256 = $binding
    }
    foreach ($bindingEntry in $stagedBindings.GetEnumerator()) {
        if ((Get-FileSha256 ([string]$bindingEntry.Value)) -cne [string]$Config.($bindingEntry.Key)) {
            throw "A private staged file hash changed after staging."
        }
    }
    if ((Get-FileSha256 $manifest) -cne [string]$Config.checksumManifestSha256) {
        throw "The private staged checksum manifest hash is invalid."
    }
    $expectedAttemptKey = Get-CandidateAttemptKey `
        -CandidateVersion ([string]$Config.version) `
        -ManifestSha256 ([string]$Config.checksumManifestSha256) `
        -Server $server `
        -Helper $helper `
        -Manifest $manifest `
        -Binding $binding
    if ($expectedAttemptKey -cne [string]$Config.attemptKey) {
        throw "The persistent attempt key is not bound to the configured product version."
    }
    if (-not $BeforeReservation) {
        $reservation = Read-BoundedJson $ledger 16384 "Acceptance-attempt reservation"
        Assert-ExactPropertyOrder $reservation @(
            "schemaVersion", "kind", "status", "productVersion", "attemptKey",
            "checksumManifestSha256", "coordinatorInstanceId", "retryAllowed",
            "reservedAtUtc", "pathsRecorded", "secretsRecorded"
        ) "Acceptance-attempt reservation"
        Assert-ExactIntegerRange `
            $reservation.schemaVersion `
            $script:AttemptReservationSchemaVersion `
            $script:AttemptReservationSchemaVersion `
            "reservation schemaVersion"
        Assert-ExactStringValue $reservation.kind "windows-acceptance-attempt-reservation" "reservation kind"
        Assert-ExactStringValue $reservation.status "reserved-no-retry" "reservation status"
        Assert-ExactStringValue $reservation.productVersion ([string]$Config.version) "reservation productVersion"
        Assert-ExactStringValue $reservation.attemptKey ([string]$Config.attemptKey) "reservation attemptKey"
        Assert-ExactStringValue $reservation.checksumManifestSha256 ([string]$Config.checksumManifestSha256) "reservation manifest hash"
        if ($ForeignReservation) {
            if ($reservation.coordinatorInstanceId -isnot [string] -or
                $reservation.coordinatorInstanceId -cnotmatch '^[0-9a-f]{32}$' -or
                $reservation.coordinatorInstanceId -ceq [string]$Config.coordinatorInstanceId) {
                throw "The attempt reservation is not owned by a different coordinator."
            }
        }
        else {
            Assert-ExactStringValue `
                $reservation.coordinatorInstanceId `
                ([string]$Config.coordinatorInstanceId) `
                "reservation coordinatorInstanceId"
        }
        Assert-ExactBoolean $reservation.retryAllowed $false "reservation retryAllowed"
        $null = ConvertFrom-CanonicalUtcString $reservation.reservedAtUtc "reservation reservedAtUtc"
        Assert-ExactBoolean $reservation.pathsRecorded $false "reservation pathsRecorded"
        Assert-ExactBoolean $reservation.secretsRecorded $false "reservation secretsRecorded"
    }
}

function Get-AttemptReservationRelationship {
    param([Parameter(Mandatory = $true)][object]$Config)
    $ledgerPath = [string]$Config.attemptLedgerPath
    if ([IO.Directory]::Exists($ledgerPath)) { return "invalid" }
    if (-not [IO.File]::Exists($ledgerPath)) { return "absent" }
    try {
        $reservation = Read-BoundedJson $ledgerPath 16384 "Acceptance-attempt reservation"
        Assert-ExactPropertyOrder $reservation @(
            "schemaVersion", "kind", "status", "productVersion", "attemptKey",
            "checksumManifestSha256", "coordinatorInstanceId", "retryAllowed",
            "reservedAtUtc", "pathsRecorded", "secretsRecorded"
        ) "Acceptance-attempt reservation"
        if ($reservation.schemaVersion -isnot [int] -or
            [int]$reservation.schemaVersion -ne $script:AttemptReservationSchemaVersion -or
            $reservation.kind -isnot [string] -or
            [string]$reservation.kind -cne "windows-acceptance-attempt-reservation" -or
            $reservation.status -isnot [string] -or
            [string]$reservation.status -cne "reserved-no-retry" -or
            $reservation.productVersion -isnot [string] -or
            [string]$reservation.productVersion -cne [string]$Config.version -or
            $reservation.attemptKey -isnot [string] -or
            [string]$reservation.attemptKey -cne [string]$Config.attemptKey -or
            $reservation.checksumManifestSha256 -isnot [string] -or
            [string]$reservation.checksumManifestSha256 -cne
                [string]$Config.checksumManifestSha256 -or
            $reservation.coordinatorInstanceId -isnot [string] -or
            [string]$reservation.coordinatorInstanceId -cnotmatch '^[0-9a-f]{32}$' -or
            $reservation.retryAllowed -isnot [bool] -or $reservation.retryAllowed -ne $false -or
            $reservation.pathsRecorded -isnot [bool] -or $reservation.pathsRecorded -ne $false -or
            $reservation.secretsRecorded -isnot [bool] -or $reservation.secretsRecorded -ne $false) {
            return "invalid"
        }
        $null = ConvertFrom-CanonicalUtcString `
            $reservation.reservedAtUtc `
            "reservation reservedAtUtc"
        if ([string]$reservation.coordinatorInstanceId -ceq
            [string]$Config.coordinatorInstanceId) {
            return "owned"
        }
        return "foreign"
    }
    catch { return "invalid" }
}

function Assert-ExactConfigurationForCurrentReservationState {
    param(
        [Parameter(Mandatory = $true)][object]$Config,
        [Parameter(Mandatory = $true)][Collections.IDictionary]$Files
    )
    $hasBoundaryDependentRecord = $false
    foreach ($boundaryDependentRecord in @(
        $Files.Intent,
        $Files.Runner,
        $Files.Watcher,
        $Files.Handoff,
        $Files.Final
    )) {
        if ([IO.File]::Exists($boundaryDependentRecord)) {
            $hasBoundaryDependentRecord = $true
            break
        }
    }
    $relationship = Get-AttemptReservationRelationship $Config
    if ($relationship -ceq "absent") {
        try { Assert-ExactConfiguration $Config -BeforeReservation }
        catch {
            $relationship = Get-AttemptReservationRelationship $Config
            if ($relationship -ceq "absent") { throw }
        }
        if ($relationship -ceq "absent") {
            $relationship = Get-AttemptReservationRelationship $Config
        }
    }
    switch ($relationship) {
        "owned" { Assert-ExactConfiguration $Config }
        "foreign" { Assert-ExactConfiguration $Config -ForeignReservation }
        "absent" {
            if ($hasBoundaryDependentRecord) {
                throw "A post-boundary coordinator record has no persistent attempt reservation."
            }
        }
        default { throw "The persistent attempt reservation is invalid." }
    }
    if ($relationship -ceq "foreign" -and $hasBoundaryDependentRecord) {
        throw "A post-boundary coordinator record belongs to a different persistent reservation."
    }
}

function Get-ObservedAttemptState {
    param(
        [Parameter(Mandatory = $true)][Collections.IDictionary]$Files,
        [Parameter(Mandatory = $true)][object]$Config
    )
    if ([IO.File]::Exists($Files.Runner)) {
        return "runner-started-terminal"
    }
    if ([IO.File]::Exists($Files.Intent)) {
        return "candidate-execution-unknown"
    }
    $relationship = Get-AttemptReservationRelationship $Config
    if ($relationship -ceq "owned" -or $relationship -ceq "invalid") {
        return "candidate-execution-unknown"
    }
    return "not-started"
}

function Assert-WorkerSupportBinding {
    param(
        [Parameter(Mandatory = $true)][object]$Config,
        [Parameter(Mandatory = $true)][string]$Root
    )
    Assert-ExactPropertyOrder $Config @(
        "schemaVersion", "version", "sourceDirectory", "coordinatorScript", "runnerScript",
        "watcherScript", "serverPath", "helperPath", "checksumManifest",
        "checksumManifestSha256", "candidateBindingPath", "fixturePath", "evidenceDirectory",
        "coordinatorDirectory", "foregroundArmTimeoutSeconds", "attemptKey",
        "attemptLedgerPath", "coordinatorInstanceId", "workerSupportAssembly", "workerSupportSha256",
        "coordinatorScriptSha256", "runnerScriptSha256", "watcherScriptSha256",
        "fixtureSha256", "serverSha256", "helperSha256", "candidateBindingSha256"
    ) "Private coordinator configuration"
    Assert-ExactIntegerRange $Config.schemaVersion 1 1 "configuration schemaVersion"
    Assert-ExactStringValue $Config.version $script:ProductVersion "configuration version"
    if ([string]$Config.coordinatorDirectory -cne $Root -or
        [IO.Path]::GetDirectoryName($Root) -cne (Get-PrivateAcceptanceParent "Coordinator") -or
        [string]$Config.workerSupportAssembly -cne [IO.Path]::Combine($Root, "worker-lifetime-support.dll") -or
        $Config.coordinatorInstanceId -isnot [string] -or
        $Config.coordinatorInstanceId -cnotmatch '^[0-9a-f]{32}$' -or
        $Config.workerSupportSha256 -isnot [string] -or
        $Config.workerSupportSha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw "The worker support binding is not canonical."
    }
    Assert-PrivateDirectoryAcl $Root
    $support = Resolve-OrdinaryPath ([string]$Config.workerSupportAssembly) $true "Worker support assembly"
    if ((Get-FileSha256 $support) -cne [string]$Config.workerSupportSha256) {
        throw "The worker support assembly does not match its private configuration."
    }
}

function Assert-ExactStartRecord {
    param(
        [Parameter(Mandatory = $true)][object]$Record,
        [Parameter(Mandatory = $true)][object]$Config
    )
    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "kind", "status", "version", "coordinatorInstanceId", "attemptState",
        "retryOnUnknownOutcome", "recordedAtUtc", "pathsRecorded", "secretsRecorded"
    ) "Start request record"
    Assert-ExactIntegerRange $Record.schemaVersion 1 1 "start schemaVersion"
    Assert-ExactStringValue $Record.kind "windows-acceptance-start-request" "start kind"
    Assert-ExactStringValue $Record.status "accepted" "start status"
    Assert-ExactStringValue $Record.version ([string]$Config.version) "start version"
    Assert-ExactStringValue `
        $Record.coordinatorInstanceId `
        ([string]$Config.coordinatorInstanceId) `
        "start coordinatorInstanceId"
    Assert-ExactStringValue $Record.attemptState "not-started" "start attemptState"
    Assert-ExactBoolean $Record.retryOnUnknownOutcome $false "start retryOnUnknownOutcome"
    $null = ConvertFrom-CanonicalUtcString $Record.recordedAtUtc "start recordedAtUtc"
    Assert-ExactBoolean $Record.pathsRecorded $false "start pathsRecorded"
    Assert-ExactBoolean $Record.secretsRecorded $false "start secretsRecorded"
}

function Assert-ExactWorkerRecord {
    param([Parameter(Mandatory = $true)][object]$Record)
    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "kind", "status", "workerPid", "workerStartedAtUtc",
        "attemptState", "retryOnUnknownOutcome", "pathsRecorded", "secretsRecorded"
    ) "Worker start record"
    Assert-ExactIntegerRange $Record.schemaVersion 1 1 "worker schemaVersion"
    Assert-ExactStringValue $Record.kind "windows-acceptance-worker-started" "worker kind"
    Assert-ExactStringValue $Record.status "running" "worker status"
    Assert-ExactIntegerRange $Record.workerPid 1 ([Int32]::MaxValue) "worker PID"
    $null = ConvertFrom-CanonicalUtcString $Record.workerStartedAtUtc "worker start time"
    Assert-ExactStringValue $Record.attemptState "not-started" "worker attemptState"
    Assert-ExactBoolean $Record.retryOnUnknownOutcome $false "worker retryOnUnknownOutcome"
    Assert-ExactBoolean $Record.pathsRecorded $false "worker pathsRecorded"
    Assert-ExactBoolean $Record.secretsRecorded $false "worker secretsRecorded"
}

function Assert-ExactIntentRecord {
    param([Parameter(Mandatory = $true)][object]$Record)
    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "kind", "status", "candidateExecutionState",
        "retryOnUnknownOutcome", "recordedAtUtc", "pathsRecorded", "secretsRecorded"
    ) "Runner launch intent record"
    Assert-ExactIntegerRange $Record.schemaVersion 1 1 "intent schemaVersion"
    Assert-ExactStringValue $Record.kind "windows-acceptance-runner-launch-intent" "intent kind"
    Assert-ExactStringValue $Record.status "terminal-attempt-boundary" "intent status"
    Assert-ExactStringValue $Record.candidateExecutionState "unknown-after-this-record" "intent candidateExecutionState"
    Assert-ExactBoolean $Record.retryOnUnknownOutcome $false "intent retryOnUnknownOutcome"
    $null = ConvertFrom-CanonicalUtcString $Record.recordedAtUtc "intent recordedAtUtc"
    Assert-ExactBoolean $Record.pathsRecorded $false "intent pathsRecorded"
    Assert-ExactBoolean $Record.secretsRecorded $false "intent secretsRecorded"
}

function Assert-ExactOwnershipRecord {
    param(
        [Parameter(Mandatory = $true)][object]$Record,
        [Parameter(Mandatory = $true)][object]$Worker
    )
    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "kind", "status", "workerPid", "workerStartedAtUtc",
        "attemptState", "retryOnUnknownOutcome", "recordedAtUtc", "pathsRecorded", "secretsRecorded"
    ) "Worker ownership record"
    Assert-ExactIntegerRange $Record.schemaVersion 1 1 "ownership schemaVersion"
    Assert-ExactStringValue $Record.kind "windows-acceptance-worker-ownership-transferred" "ownership kind"
    Assert-ExactStringValue $Record.status "guard-owned-by-worker" "ownership status"
    Assert-ExactIntegerRange $Record.workerPid 1 ([Int32]::MaxValue) "ownership worker PID"
    $null = ConvertFrom-CanonicalUtcString $Record.workerStartedAtUtc "ownership worker start time"
    if ([int]$Record.workerPid -ne [int]$Worker.workerPid -or
        [string]$Record.workerStartedAtUtc -cne [string]$Worker.workerStartedAtUtc) {
        throw "The worker ownership record is not bound to the exact worker."
    }
    Assert-ExactStringValue $Record.attemptState "not-started" "ownership attemptState"
    Assert-ExactBoolean $Record.retryOnUnknownOutcome $false "ownership retryOnUnknownOutcome"
    $null = ConvertFrom-CanonicalUtcString $Record.recordedAtUtc "ownership recordedAtUtc"
    Assert-ExactBoolean $Record.pathsRecorded $false "ownership pathsRecorded"
    Assert-ExactBoolean $Record.secretsRecorded $false "ownership secretsRecorded"
}

function Assert-ExactRunnerRecord {
    param([Parameter(Mandatory = $true)][object]$Record)
    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "kind", "status", "runnerPid", "runnerStartedAtUtc",
        "attemptState", "retryOnUnknownOutcome", "pathsRecorded", "secretsRecorded"
    ) "Runner start record"
    Assert-ExactIntegerRange $Record.schemaVersion 1 1 "runner schemaVersion"
    Assert-ExactStringValue $Record.kind "windows-acceptance-runner-started" "runner kind"
    Assert-ExactStringValue $Record.status "running" "runner status"
    Assert-ExactIntegerRange $Record.runnerPid 1 ([Int32]::MaxValue) "runner PID"
    $null = ConvertFrom-CanonicalUtcString $Record.runnerStartedAtUtc "runner start time"
    Assert-ExactStringValue $Record.attemptState "runner-started-terminal" "runner attemptState"
    Assert-ExactBoolean $Record.retryOnUnknownOutcome $false "runner retryOnUnknownOutcome"
    Assert-ExactBoolean $Record.pathsRecorded $false "runner pathsRecorded"
    Assert-ExactBoolean $Record.secretsRecorded $false "runner secretsRecorded"
}

function Assert-ExactWatcherRecord {
    param([Parameter(Mandatory = $true)][object]$Record)
    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "kind", "status", "exitCode", "stdoutPresent", "stderrPresent",
        "runnerIdentityMatched", "retryAllowed", "recordedAtUtc", "pathsRecorded", "secretsRecorded"
    ) "Watcher final record"
    Assert-ExactIntegerRange $Record.schemaVersion 1 1 "watcher schemaVersion"
    Assert-ExactStringValue $Record.kind "windows-acceptance-watcher-finished" "watcher kind"
    if ($Record.status -isnot [string] -or @("accepted", "failed-closed") -cnotcontains [string]$Record.status) {
        throw "The watcher status is invalid."
    }
    Assert-ExactIntegerRange $Record.exitCode ([Int32]::MinValue) ([Int32]::MaxValue) "watcher exitCode"
    foreach ($field in @("stdoutPresent", "stderrPresent", "runnerIdentityMatched", "retryAllowed", "pathsRecorded", "secretsRecorded")) {
        if ($Record.$field -isnot [bool]) { throw "Watcher $field must be Boolean." }
    }
    Assert-ExactBoolean $Record.retryAllowed $false "watcher retryAllowed"
    Assert-ExactBoolean $Record.pathsRecorded $false "watcher pathsRecorded"
    Assert-ExactBoolean $Record.secretsRecorded $false "watcher secretsRecorded"
    $null = ConvertFrom-CanonicalUtcString $Record.recordedAtUtc "watcher recordedAtUtc"
    if ([string]$Record.status -ceq "accepted" -and
        ([int]$Record.exitCode -ne 0 -or $Record.stdoutPresent -ne $true -or
            $Record.stderrPresent -ne $false -or $Record.runnerIdentityMatched -ne $true)) {
        throw "An accepted watcher record is not a complete passing result."
    }
}

function Assert-ExactFailureRecord {
    param([Parameter(Mandatory = $true)][object]$Record)
    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "kind", "status", "stage", "attemptState", "reasonCode",
        "retryAllowed", "recordedAtUtc", "pathsRecorded", "secretsRecorded"
    ) "Terminal failure record"
    Assert-ExactIntegerRange $Record.schemaVersion 1 1 "failure schemaVersion"
    Assert-ExactStringValue $Record.kind "windows-acceptance-coordinator-terminal" "failure kind"
    Assert-ExactStringValue $Record.status "failed-closed" "failure status"
    if ($Record.stage -isnot [string] -or $Record.stage -cnotmatch '^[a-z0-9-]+$' -or
        $Record.reasonCode -isnot [string] -or $Record.reasonCode -cnotmatch '^[a-z0-9-]+$' -or
        $Record.attemptState -isnot [string] -or @(
            "not-started", "candidate-execution-unknown", "runner-started-terminal"
        ) -cnotcontains [string]$Record.attemptState) {
        throw "The terminal failure semantics are invalid."
    }
    Assert-ExactBoolean $Record.retryAllowed $false "failure retryAllowed"
    $null = ConvertFrom-CanonicalUtcString $Record.recordedAtUtc "failure recordedAtUtc"
    Assert-ExactBoolean $Record.pathsRecorded $false "failure pathsRecorded"
    Assert-ExactBoolean $Record.secretsRecorded $false "failure secretsRecorded"
}

function Assert-ExactFinalRecord {
    param([Parameter(Mandatory = $true)][object]$Record)
    Assert-ExactPropertyOrder $Record @(
        "schemaVersion", "kind", "status", "exitCode", "summaryPresent", "summaryPassed",
        "evidenceDirectoryPresent", "attemptState", "retryAllowed", "finishedAtUtc",
        "pathsRecorded", "secretsRecorded"
    ) "Runner final record"
    Assert-ExactIntegerRange $Record.schemaVersion 1 1 "final schemaVersion"
    Assert-ExactStringValue $Record.kind "windows-acceptance-runner-finished" "final kind"
    if ($Record.status -isnot [string] -or @("completed", "failed-closed") -cnotcontains [string]$Record.status) {
        throw "The runner final status is invalid."
    }
    Assert-ExactIntegerRange $Record.exitCode ([Int32]::MinValue) ([Int32]::MaxValue) "final exitCode"
    if ($Record.summaryPresent -isnot [bool] -or $Record.summaryPassed -isnot [bool] -or
        $Record.evidenceDirectoryPresent -isnot [bool]) {
        throw "The runner final evidence booleans are invalid."
    }
    Assert-ExactStringValue $Record.attemptState "runner-started-terminal" "final attemptState"
    Assert-ExactBoolean $Record.retryAllowed $false "final retryAllowed"
    $null = ConvertFrom-CanonicalUtcString $Record.finishedAtUtc "final finishedAtUtc"
    Assert-ExactBoolean $Record.pathsRecorded $false "final pathsRecorded"
    Assert-ExactBoolean $Record.secretsRecorded $false "final secretsRecorded"
    if ([string]$Record.status -ceq "completed" -and
        ([int]$Record.exitCode -ne 0 -or $Record.summaryPresent -ne $true -or
            $Record.summaryPassed -ne $true -or $Record.evidenceDirectoryPresent -ne $true)) {
        throw "A completed runner final record is not a complete passing result."
    }
}

function Assert-ExactHandoffRecord {
    param(
        [Parameter(Mandatory = $true)][object]$Record,
        [Parameter(Mandatory = $true)][object]$Config,
        [Parameter(Mandatory = $true)][object]$Runner
    )
    $fields = @(
        "schemaVersion", "productVersion", "kind", "status", "requestId",
        "publishedAtUtc", "receivedAtUtc", "observedAtUtc", "deadlineAtUtc", "mode",
        "operatorActionRequired", "action", "clickAttemptsObserved", "stableSamplesObserved",
        "stableSamplesRequired", "nativeSampleSeqlockMatched", "ownerIdentityStable",
        "focusRootMatched", "fixtureProcessExcluded", "interactiveSessionMatched", "cursorStable", "inputDesktopStable",
        "globalInputUsed", "focusChangedByRunner", "cursorChangedByRunner", "syntheticInputUsed",
        "notificationOnly", "acceptedAsAuthority", "runnerIdentityMatched", "requestFresh",
        "receivedBeforeDeadline", "rawWindowHandlesRecorded", "rawProcessIdentifiersRecorded",
        "rawCursorCoordinatesRecorded", "pathsRecorded", "secretsRecorded"
    )
    Assert-ExactPropertyOrder $Record $fields "Handoff record"
    Assert-ExactIntegerRange `
        $Record.schemaVersion `
        $script:AutomaticHandoffSchemaVersion `
        $script:AutomaticHandoffSchemaVersion `
        "handoff schemaVersion"
    Assert-ExactStringValue $Record.productVersion ([string]$Config.version) "handoff productVersion"
    Assert-ExactStringValue $Record.kind "windows-acceptance-automatic-handoff" "handoff kind"
    Assert-ExactStringValue $Record.status "automatic-ready" "handoff status"
    if ($Record.requestId -isnot [string] -or $Record.requestId -cnotmatch '^[0-9a-f]{32}$') {
        throw "The handoff status or request ID is invalid."
    }
    $published = ConvertFrom-CanonicalUtcString $Record.publishedAtUtc "handoff publishedAtUtc"
    $received = ConvertFrom-CanonicalUtcString $Record.receivedAtUtc "handoff receivedAtUtc"
    $observed = ConvertFrom-CanonicalUtcString $Record.observedAtUtc "handoff observedAtUtc"
    $deadline = ConvertFrom-CanonicalUtcString $Record.deadlineAtUtc "handoff deadlineAtUtc"
    if ($received -lt $published -or $received -gt $deadline -or $observed -lt $received -or
        $deadline -le $published -or ($deadline - $published).TotalSeconds -gt 300) {
        throw "The handoff freshness interval is invalid."
    }
    Assert-ExactStringValue $Record.mode $script:ForegroundGateMode "handoff mode"
    Assert-ExactBoolean $Record.operatorActionRequired $false "handoff operatorActionRequired"
    Assert-ExactStringValue $Record.action "none" "handoff action"
    Assert-ExactIntegerRange $Record.clickAttemptsObserved 0 0 "handoff clickAttemptsObserved"
    Assert-ExactIntegerRange $Record.stableSamplesRequired 3 3 "handoff stableSamplesRequired"
    Assert-ExactIntegerRange $Record.stableSamplesObserved 3 3 "handoff stableSamplesObserved"
    foreach ($field in @(
        "nativeSampleSeqlockMatched", "ownerIdentityStable", "focusRootMatched",
        "fixtureProcessExcluded", "interactiveSessionMatched", "cursorStable", "inputDesktopStable", "runnerIdentityMatched",
        "requestFresh", "receivedBeforeDeadline"
    )) {
        Assert-ExactBoolean $Record.$field $true "handoff $field"
    }
    foreach ($field in @(
        "globalInputUsed", "focusChangedByRunner", "cursorChangedByRunner", "syntheticInputUsed",
        "acceptedAsAuthority", "rawWindowHandlesRecorded", "rawProcessIdentifiersRecorded",
        "rawCursorCoordinatesRecorded", "pathsRecorded", "secretsRecorded"
    )) {
        Assert-ExactBoolean $Record.$field $false "handoff $field"
    }
    Assert-ExactBoolean $Record.notificationOnly $true "handoff notificationOnly"
    Assert-ExactIntegerRange $Runner.runnerPid 1 ([Int32]::MaxValue) "runner record PID"
}

function Get-ValidatedFollowChain {
    param(
        [Parameter(Mandatory = $true)][Collections.IDictionary]$Files,
        [Parameter(Mandatory = $true)][object]$Config,
        [switch]$SkipFinal
    )
    if (-not [IO.File]::Exists($Files.Start)) {
        throw "The coordinator state chain is missing its start request."
    }
    $start = Read-BoundedJson $Files.Start 16384 "Start request record"
    Assert-ExactStartRecord $start $Config

    $worker = $null
    if ([IO.File]::Exists($Files.Worker)) {
        $worker = Read-BoundedJson $Files.Worker 16384 "Worker start record"
        Assert-ExactWorkerRecord $worker
    }
    $ownership = $null
    if ([IO.File]::Exists($Files.Ownership)) {
        if ($null -eq $worker) { throw "The coordinator ownership record has no worker predecessor." }
        $ownership = Read-BoundedJson $Files.Ownership 16384 "Worker ownership record"
        Assert-ExactOwnershipRecord $ownership $worker
    }
    $intent = $null
    if ([IO.File]::Exists($Files.Intent)) {
        if ($null -eq $ownership) { throw "The runner intent has no ownership predecessor." }
        $intent = Read-BoundedJson $Files.Intent 16384 "Runner launch intent record"
        Assert-ExactIntentRecord $intent
    }
    $runner = $null
    if ([IO.File]::Exists($Files.Runner)) {
        if ($null -eq $intent) { throw "The runner record has no launch-intent predecessor." }
        $runner = Read-BoundedJson $Files.Runner 16384 "Runner start record"
        Assert-ExactRunnerRecord $runner
    }
    $watcher = $null
    if ([IO.File]::Exists($Files.Watcher)) {
        if ($null -eq $runner) { throw "The watcher record has no runner predecessor." }
        $watcher = Read-BoundedJson $Files.Watcher 16384 "Watcher final record"
        Assert-ExactWatcherRecord $watcher
    }
    $handoff = $null
    if ([IO.File]::Exists($Files.Handoff)) {
        if ($null -eq $watcher -or [string]$watcher.status -cne "accepted") {
            throw "The handoff record has no accepted watcher predecessor."
        }
        $handoff = Read-BoundedJson $Files.Handoff 32768 "Handoff record"
        Assert-ExactHandoffRecord $handoff $Config $runner
    }
    $final = $null
    if (-not $SkipFinal -and [IO.File]::Exists($Files.Final)) {
        if ($null -eq $runner) { throw "The final record has no runner predecessor." }
        $final = Read-BoundedJson $Files.Final 16384 "Runner final record"
        Assert-ExactFinalRecord $final
    }
    return [pscustomobject]@{
        Start = $start
        Worker = $worker
        Ownership = $ownership
        Intent = $intent
        Runner = $runner
        Watcher = $watcher
        Handoff = $handoff
        Final = $final
    }
}

function ConvertTo-ExactPrivateHandoffRecord {
    param(
        [Parameter(Mandatory = $true)][object]$WatcherHandoff,
        [Parameter(Mandatory = $true)][object]$Config,
        [Parameter(Mandatory = $true)][object]$Runner
    )
    Assert-ExactPropertyOrder $WatcherHandoff @(
        "schemaVersion", "productVersion", "kind", "status", "requestId",
        "publishedAtUtc", "receivedAtUtc", "observedAtUtc", "deadlineAtUtc", "mode",
        "operatorActionRequired", "action", "clickAttemptsObserved", "stableSamplesObserved",
        "stableSamplesRequired", "nativeSampleSeqlockMatched", "ownerIdentityStable",
        "focusRootMatched", "fixtureProcessExcluded", "interactiveSessionMatched", "cursorStable", "inputDesktopStable",
        "globalInputUsed", "focusChangedByRunner", "cursorChangedByRunner", "syntheticInputUsed",
        "notificationOnly", "acceptedAsAuthority", "runnerIdentityMatched", "requestFresh",
        "receivedBeforeDeadline", "rawWindowHandlesRecorded", "rawProcessIdentifiersRecorded",
        "rawCursorCoordinatesRecorded", "pathsRecorded", "secretsRecorded"
    ) "Watcher handoff"
    Assert-ExactStringValue $WatcherHandoff.kind "foreground-baseline-ready-handoff" "watcher handoff kind"
    $record = [pscustomobject]([ordered]@{
        schemaVersion = $WatcherHandoff.schemaVersion
        productVersion = $WatcherHandoff.productVersion
        kind = "windows-acceptance-automatic-handoff"
        status = $WatcherHandoff.status
        requestId = $WatcherHandoff.requestId
        publishedAtUtc = $WatcherHandoff.publishedAtUtc
        receivedAtUtc = $WatcherHandoff.receivedAtUtc
        observedAtUtc = $WatcherHandoff.observedAtUtc
        deadlineAtUtc = $WatcherHandoff.deadlineAtUtc
        mode = $WatcherHandoff.mode
        operatorActionRequired = $WatcherHandoff.operatorActionRequired
        action = $WatcherHandoff.action
        clickAttemptsObserved = $WatcherHandoff.clickAttemptsObserved
        stableSamplesObserved = $WatcherHandoff.stableSamplesObserved
        stableSamplesRequired = $WatcherHandoff.stableSamplesRequired
        nativeSampleSeqlockMatched = $WatcherHandoff.nativeSampleSeqlockMatched
        ownerIdentityStable = $WatcherHandoff.ownerIdentityStable
        focusRootMatched = $WatcherHandoff.focusRootMatched
        fixtureProcessExcluded = $WatcherHandoff.fixtureProcessExcluded
        interactiveSessionMatched = $WatcherHandoff.interactiveSessionMatched
        cursorStable = $WatcherHandoff.cursorStable
        inputDesktopStable = $WatcherHandoff.inputDesktopStable
        globalInputUsed = $WatcherHandoff.globalInputUsed
        focusChangedByRunner = $WatcherHandoff.focusChangedByRunner
        cursorChangedByRunner = $WatcherHandoff.cursorChangedByRunner
        syntheticInputUsed = $WatcherHandoff.syntheticInputUsed
        notificationOnly = $WatcherHandoff.notificationOnly
        acceptedAsAuthority = $WatcherHandoff.acceptedAsAuthority
        runnerIdentityMatched = $WatcherHandoff.runnerIdentityMatched
        requestFresh = $WatcherHandoff.requestFresh
        receivedBeforeDeadline = $WatcherHandoff.receivedBeforeDeadline
        rawWindowHandlesRecorded = $WatcherHandoff.rawWindowHandlesRecorded
        rawProcessIdentifiersRecorded = $WatcherHandoff.rawProcessIdentifiersRecorded
        rawCursorCoordinatesRecorded = $WatcherHandoff.rawCursorCoordinatesRecorded
        pathsRecorded = $WatcherHandoff.pathsRecorded
        secretsRecorded = $WatcherHandoff.secretsRecorded
    })
    Assert-ExactHandoffRecord $record $Config $Runner
    return $record
}

function Write-TerminalFailure {
    param(
        [Parameter(Mandatory = $true)][Collections.IDictionary]$Files,
        [Parameter(Mandatory = $true)][string]$Stage,
        [Parameter(Mandatory = $true)][string]$AttemptState,
        [Parameter(Mandatory = $true)][string]$ReasonCode
    )
    if (-not [IO.File]::Exists($Files.Failure)) {
        try {
            Write-CreateOnceJson $Files.Failure ([ordered]@{
                schemaVersion = $script:SchemaVersion
                kind = "windows-acceptance-coordinator-terminal"
                status = "failed-closed"
                stage = $Stage
                attemptState = $AttemptState
                reasonCode = $ReasonCode
                retryAllowed = $false
                recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            })
        }
        catch [IO.IOException] {
            if (-not [IO.File]::Exists($Files.Failure)) { throw }
        }
    }
}

function Get-RunnerSummaryFailureReasonCode {
    param([object]$Summary)
    if ($null -eq $Summary -or $Summary -is [Array]) {
        return "runner-summary-failed"
    }
    $failureDetailsProperty = $Summary.PSObject.Properties["failureDetails"]
    if ($null -eq $failureDetailsProperty -or $null -eq $failureDetailsProperty.Value -or
        $failureDetailsProperty.Value -is [Array]) {
        return "runner-summary-failed"
    }
    $detailReasonProperty = $failureDetailsProperty.Value.PSObject.Properties["reasonCode"]
    if ($null -ne $detailReasonProperty -and $detailReasonProperty.Value -is [string]) {
        switch -CaseSensitive ([string]$detailReasonProperty.Value) {
            "foreground-baseline-timeout" { return "runner-foreground-baseline-timeout" }
            "foreground-baseline-state-refused" { return "runner-foreground-baseline-state-refused" }
            "foreground-baseline-continuity-failed" { return "runner-foreground-baseline-continuity-failed" }
            "acceptance-test-failed" { return "runner-acceptance-test-failed" }
        }
    }
    $stageProperty = $failureDetailsProperty.Value.PSObject.Properties["stage"]
    if ($null -eq $stageProperty -or $stageProperty.Value -isnot [string]) {
        return "runner-summary-failed"
    }
    $stage = [string]$stageProperty.Value
    switch -CaseSensitive ($stage) {
        "wait-stable-external-foreground" {
            $failureProperty = $Summary.PSObject.Properties["failure"]
            if ($null -ne $failureProperty -and
                $failureProperty.Value -is [string] -and
                ([string]$failureProperty.Value).StartsWith(
                    "Timed out waiting for ",
                    [StringComparison]::Ordinal
                )) {
                return "runner-foreground-baseline-timeout"
            }
            return "runner-foreground-baseline-failed"
        }
        "publish-foreground-baseline-request" {
            return "runner-foreground-baseline-publication-failed"
        }
        "bind-foreground-baseline" {
            return "runner-foreground-baseline-binding-failed"
        }
        { @(
            "initialize-owned-processes", "build-dedicated-fixture", "self-test-dedicated-fixture",
            "start-dedicated-fixture", "select-exact-fixture-window", "start-loopback-server",
            "start-computer-helper", "bind-initial-helper-readiness"
        ) -ccontains $_ } {
            return "runner-pre-baseline-failed"
        }
        { @(
            "rebind-post-baseline-helper-readiness", "baseline-status-and-observation",
            "wait-recovery-event-ready", "recovery-suite", "semantic-suite", "keyboard-suite",
            "pixel-suite", "capture-suite", "cancellation-suite", "final-invariants"
        ) -ccontains $_ } {
            return "runner-post-baseline-failed"
        }
    }
    return "runner-summary-failed"
}

function Get-WorkerConfiguration {
    param([Parameter(Mandatory = $true)][string]$ExpectedNonce)
    $nonce = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_COORDINATOR_WORKER_NONCE", "Process")
    $configPath = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_COORDINATOR_CONFIG", "Process")
    [Environment]::SetEnvironmentVariable("LBB_WINDOWS_COORDINATOR_WORKER_NONCE", $null, "Process")
    [Environment]::SetEnvironmentVariable("LBB_WINDOWS_COORDINATOR_CONFIG", $null, "Process")
    if ($ExpectedNonce -cnotmatch '^[0-9a-f]{32}$' -or $nonce -cne $ExpectedNonce) {
        throw "The internal worker nonce is invalid."
    }
    $resolvedConfigPath = Resolve-OrdinaryPath $configPath $true "Private coordinator configuration"
    $configRoot = Resolve-OrdinaryPath ([IO.Path]::GetDirectoryName($resolvedConfigPath)) $false "CoordinatorDirectory"
    Assert-PrivateDirectoryAcl $configRoot
    $config = Read-BoundedJson $resolvedConfigPath 65536 "Private coordinator configuration"
    if ($config.coordinatorDirectory -isnot [string] -or
        [string]$config.coordinatorDirectory -cne $configRoot -or
        $resolvedConfigPath -cne [IO.Path]::Combine($configRoot, "private-config.json")) {
        throw "The worker configuration path is not bound to its private coordinator root."
    }
    return [pscustomobject]@{
        Config = $config
        ConfigPath = $resolvedConfigPath
        Root = $configRoot
    }
}

function Invoke-CoordinatorWorker {
    param([Parameter(Mandatory = $true)][string]$Nonce)
    $configurationBundle = Get-WorkerConfiguration $Nonce
    $config = $configurationBundle.Config
    $root = [string]$configurationBundle.Root
    Assert-PrivateDirectoryAcl $root
    $files = Get-CoordinatorFiles $root
    $workerLifetimeJob = $null
    $exclusiveMutex = $null
    $exclusiveMutexHeld = $false
    try {
        Assert-WorkerSupportBinding $config $root
        Assert-ExactConfiguration $config -BeforeReservation
        $exclusiveMutex = [Threading.Mutex]::new(
            $false,
            "Local\LBBWindowsAcceptanceCoordinator"
        )
        try { $exclusiveMutexHeld = $exclusiveMutex.WaitOne(0) }
        catch [Threading.AbandonedMutexException] { $exclusiveMutexHeld = $true }
        if (-not $exclusiveMutexHeld) {
            Write-TerminalFailure $files "worker-exclusivity" "not-started" "another-coordinator-is-active"
            throw "Another Windows acceptance coordinator is already active in this session."
        }
        # The mutex serializes admission and recovery across coordinator
        # versions. If its prior owner died, the stable named Job is terminated
        # and observed at ACTIVE_PROCESS_ZERO before this worker is assigned to
        # a genuinely fresh same-name Job. Retain the mutex for process lifetime
        # so no peer can enter that recovery boundary while this worker is live.
        $script:ProcessLifetimeCoordinatorMutex = $exclusiveMutex
        $workerLifetimeJob = New-WorkerLifetimeJob `
            -AllowChildBreakaway `
            -Name $script:WorkerLifetimeJobName `
            -RecoverExisting `
            -RecoveryTimeoutMilliseconds $script:WorkerLifetimeRecoveryMilliseconds `
            -SupportAssemblyPath ([string]$config.workerSupportAssembly) `
            -SupportAssemblySha256 ([string]$config.workerSupportSha256)
    }
    catch {
        if (-not [IO.File]::Exists($files.Failure)) {
            Write-TerminalFailure $files "worker-ownership" "not-started" "worker-ownership-unavailable"
        }
        if ($null -ne $exclusiveMutex -and -not $exclusiveMutexHeld) {
            $exclusiveMutex.Dispose()
        }
        elseif ($exclusiveMutexHeld) {
            $script:ProcessLifetimeCoordinatorMutex = $exclusiveMutex
        }
        [GC]::KeepAlive($script:ProcessLifetimeCoordinatorMutex)
        [GC]::KeepAlive($workerLifetimeJob)
        throw
    }
    $currentWorkerProcess = [Diagnostics.Process]::GetCurrentProcess()
    try {
        $currentWorkerPid = $currentWorkerProcess.Id
        $currentWorkerStartedAtUtc = ConvertTo-CanonicalUtcString (
            $currentWorkerProcess.StartTime.ToUniversalTime()
        )
    }
    finally { $currentWorkerProcess.Dispose() }
    Write-CreateOnceJson $files.Worker ([ordered]@{
        schemaVersion = $script:SchemaVersion
        kind = "windows-acceptance-worker-started"
        status = "running"
        workerPid = $currentWorkerPid
        workerStartedAtUtc = $currentWorkerStartedAtUtc
        attemptState = "not-started"
        retryOnUnknownOutcome = $false
        pathsRecorded = $false
        secretsRecorded = $false
    })

    $ownershipDeadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not [IO.File]::Exists($files.Ownership) -and
        -not [IO.File]::Exists($files.Failure) -and
        [DateTime]::UtcNow -lt $ownershipDeadline) {
        Start-Sleep -Milliseconds 50
    }
    if ([IO.File]::Exists($files.Failure)) {
        throw "The coordinator worker was canceled before guard ownership transfer."
    }
    if (-not [IO.File]::Exists($files.Ownership)) {
        Write-TerminalFailure $files "worker-ownership" "not-started" "guard-ownership-transfer-missing"
        throw "The detached worker guard ownership transfer was not confirmed."
    }
    $workerRecordForOwnership = Read-BoundedJson $files.Worker 16384 "Worker start record"
    Assert-ExactWorkerRecord $workerRecordForOwnership
    Assert-ExactOwnershipRecord `
        (Read-BoundedJson $files.Ownership 16384 "Worker ownership record") `
        $workerRecordForOwnership

    $runnerCapture = $null
    $watcherCapture = $null
    $watcherAccepted = $false
    $watcherRefusedWhileRunnerLive = $false
    $handoffPublished = $false
    $runnerInfo = $null
    $token = $null
    try {
        $systemPowerShell = Resolve-SystemWindowsPowerShell
        $runnerArguments = @(
            "-NoLogo", "-NoProfile", "-NonInteractive", "-File", [string]$config.runnerScript,
            "-Version", [string]$config.version,
            "-ServerPath", [string]$config.serverPath,
            "-HelperPath", [string]$config.helperPath,
            "-ChecksumManifest", [string]$config.checksumManifest,
            "-ChecksumManifestSha256", [string]$config.checksumManifestSha256,
            "-CandidateBindingPath", [string]$config.candidateBindingPath,
            "-FixturePath", [string]$config.fixturePath,
            "-EvidenceDirectory", [string]$config.evidenceDirectory,
            "-ForegroundArmTimeoutSeconds", [string]$config.foregroundArmTimeoutSeconds,
            "-Suite", "All", "-ShowOccluder"
        )
        $runnerInfo = New-ProcessStartInfo `
            $systemPowerShell `
            $runnerArguments `
            ([string]$config.sourceDirectory) `
            -Hidden
        Remove-SensitiveInheritedEnvironment $runnerInfo
        $token = New-BridgeToken
        $runnerInfo.EnvironmentVariables["LBB_TOKEN"] = $token
        if ([IO.File]::Exists($files.Failure)) {
            throw "The coordinator worker was canceled before candidate intent."
        }
        Assert-ExactConfiguration $config -BeforeReservation
        $reservedAttemptPath = Reserve-CandidateAttempt `
            -LedgerRoot ([IO.Path]::GetDirectoryName([string]$config.attemptLedgerPath)) `
            -AttemptKey ([string]$config.attemptKey) `
            -CandidateVersion ([string]$config.version) `
            -ManifestSha256 ([string]$config.checksumManifestSha256) `
            -CoordinatorInstanceId ([string]$config.coordinatorInstanceId)
        if ($reservedAttemptPath -cne [string]$config.attemptLedgerPath) {
            throw "The persistent attempt reservation changed identity."
        }
        Assert-ExactConfiguration $config
        if ([IO.File]::Exists($files.Failure)) {
            throw "The coordinator worker was canceled at the candidate boundary."
        }
        Write-CreateOnceJson $files.Intent ([ordered]@{
            schemaVersion = $script:SchemaVersion
            kind = "windows-acceptance-runner-launch-intent"
            status = "terminal-attempt-boundary"
            candidateExecutionState = "unknown-after-this-record"
            retryOnUnknownOutcome = $false
            recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
            pathsRecorded = $false
            secretsRecorded = $false
        })
        try {
            if ([IO.File]::Exists($files.Failure)) {
                throw "The coordinator worker was canceled before candidate process creation."
            }
            $runnerCapture = Start-CapturedProcess $runnerInfo $files.RunnerOut $files.RunnerErr
        }
        finally {
            $runnerInfo.EnvironmentVariables.Remove("LBB_TOKEN")
            $token = $null
        }
        $runnerRecord = [pscustomobject]([ordered]@{
            schemaVersion = $script:SchemaVersion
            kind = "windows-acceptance-runner-started"
            status = "running"
            runnerPid = $runnerCapture.Process.Id
            runnerStartedAtUtc = ConvertTo-CanonicalUtcString $runnerCapture.StartedAtUtc
            attemptState = "runner-started-terminal"
            retryOnUnknownOutcome = $false
            pathsRecorded = $false
            secretsRecorded = $false
        })
        Assert-ExactRunnerRecord $runnerRecord
        Write-CreateOnceJson $files.Runner $runnerRecord

        $runnerDeadline = [DateTime]::UtcNow.AddMilliseconds($script:MaximumRunnerMilliseconds)
        $requestMarkerAppeared = $false
        while (-not $runnerCapture.Process.HasExited -and [DateTime]::UtcNow -lt $runnerDeadline) {
            if ([IO.Directory]::Exists([string]$config.evidenceDirectory)) {
                $resolvedEvidenceDirectory = Resolve-OrdinaryPath ([string]$config.evidenceDirectory) $false "EvidenceDirectory"
                $operatorDirectory = [IO.Path]::Combine($resolvedEvidenceDirectory, "operator")
                if ([IO.Directory]::Exists($operatorDirectory)) {
                    $resolvedOperatorDirectory = Resolve-OrdinaryPath $operatorDirectory $false "Operator marker directory"
                    $requestMarkerPath = [IO.Path]::Combine($resolvedOperatorDirectory, "foreground-arm-request.json")
                    if ([IO.File]::Exists($requestMarkerPath)) {
                        $null = Resolve-OrdinaryPath $requestMarkerPath $true "Foreground-arm request marker"
                        $requestMarkerAppeared = $true
                        break
                    }
                }
            }
            Start-Sleep -Milliseconds 100
        }
        if (-not $runnerCapture.Process.HasExited -and [DateTime]::UtcNow -ge $runnerDeadline) {
            $runnerCapture.Process.Kill()
            $null = Complete-CapturedProcess $runnerCapture -TimeoutMilliseconds 10000
            $runnerCapture.Process.Dispose()
            $runnerCapture = $null
            throw "The Windows acceptance runner exceeded the coordinator deadline before handoff."
        }

        if ($requestMarkerAppeared -and -not $runnerCapture.Process.HasExited) {
            $watcherArguments = @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File", [string]$config.watcherScript,
                "-Mode", "Watch",
                "-EvidenceDirectory", [string]$config.evidenceDirectory,
                "-RunnerProcessId", [string]$runnerCapture.Process.Id,
                "-RunnerStartedAtUtc", (ConvertTo-CanonicalUtcString $runnerCapture.StartedAtUtc),
                "-WaitTimeoutSeconds", [string]$config.foregroundArmTimeoutSeconds
            )
            $watcherInfo = New-ProcessStartInfo $systemPowerShell $watcherArguments ([string]$config.sourceDirectory) -Hidden
            Remove-SensitiveInheritedEnvironment $watcherInfo
            $watcherCapture = Start-CapturedProcess $watcherInfo $files.WatcherOut $files.WatcherErr
            $watcherExit = Complete-CapturedProcess `
                $watcherCapture `
                -TimeoutMilliseconds (([int]$config.foregroundArmTimeoutSeconds + 5) * 1000)
            $watcherCapture.Process.Dispose()
            $watcherCapture = $null
            $watcherStdout = [IO.File]::ReadAllText($files.WatcherOut, $script:Utf8NoBom).Trim()
            $watcherStderr = [IO.File]::ReadAllText($files.WatcherErr, $script:Utf8NoBom)
            $handoff = $null
            $privateHandoff = $null
            if ($watcherExit -eq 0 -and [String]::IsNullOrWhiteSpace($watcherStderr) -and
                -not [String]::IsNullOrWhiteSpace($watcherStdout)) {
                try {
                    $handoff = $watcherStdout | ConvertFrom-Json
                    $privateHandoff = ConvertTo-ExactPrivateHandoffRecord $handoff $config $runnerRecord
                    Assert-PrivateDirectoryAcl ([string]$config.evidenceDirectory)
                    $watcherAccepted = Test-BoundProcessAlive `
                        $runnerCapture.Process.Id `
                        $runnerCapture.StartedAtUtc
                }
                catch { $watcherAccepted = $false }
            }
            $watcherRecord = [pscustomobject]([ordered]@{
                schemaVersion = $script:SchemaVersion
                kind = "windows-acceptance-watcher-finished"
                status = if ($watcherAccepted) { "accepted" } else { "failed-closed" }
                exitCode = $watcherExit
                stdoutPresent = -not [String]::IsNullOrWhiteSpace($watcherStdout)
                stderrPresent = -not [String]::IsNullOrWhiteSpace($watcherStderr)
                runnerIdentityMatched = if ($watcherAccepted) { $true } else { $false }
                retryAllowed = $false
                recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            })
            Assert-ExactWatcherRecord $watcherRecord
            Write-CreateOnceJson $files.Watcher $watcherRecord
            if ($watcherAccepted) {
                Write-CreateOnceJson $files.Handoff $privateHandoff
                $handoffPublished = $true
            }
            else {
                $watcherRefusedWhileRunnerLive = -not $runnerCapture.Process.HasExited
                if (-not $runnerCapture.Process.HasExited) {
                    $runnerCapture.Process.Kill()
                }
            }
        }

        $remainingRunnerMilliseconds = [Math]::Max(
            1,
            [Math]::Min(
                $script:MaximumRunnerMilliseconds,
                [int][Math]::Ceiling(($runnerDeadline - [DateTime]::UtcNow).TotalMilliseconds)
            )
        )
        $runnerExit = Complete-CapturedProcess `
            $runnerCapture `
            -TimeoutMilliseconds $remainingRunnerMilliseconds
        $summaryPath = [IO.Path]::Combine([string]$config.evidenceDirectory, "summary.json")
        $summaryExists = [IO.File]::Exists($summaryPath)
        $summaryPassed = $false
        $summaryFailureReasonCode = "runner-summary-failed"
        if ($summaryExists) {
            try {
                $summary = Read-BoundedJson $summaryPath 1048576 "Windows acceptance summary"
                $summaryPassed = $summary.passed -is [bool] -and $summary.passed -eq $true
                if (-not $summaryPassed) {
                    $summaryFailureReasonCode = Get-RunnerSummaryFailureReasonCode $summary
                }
            }
            catch { $summaryPassed = $false }
        }
        Assert-PrivateDirectoryAcl ([string]$config.evidenceDirectory)
        $evidenceDirectoryPresent = [IO.Directory]::Exists([string]$config.evidenceDirectory)
        $acceptanceCompleted = (
            $runnerExit -eq 0 -and
            $summaryExists -and
            $summaryPassed -and
            $evidenceDirectoryPresent -and
            $watcherAccepted -and
            $handoffPublished -and
            -not [IO.File]::Exists($files.Failure)
        )
        $finalRecord = [pscustomobject]([ordered]@{
            schemaVersion = $script:SchemaVersion
            kind = "windows-acceptance-runner-finished"
            status = if ($acceptanceCompleted) { "completed" } else { "failed-closed" }
            exitCode = $runnerExit
            summaryPresent = $summaryExists
            summaryPassed = $summaryPassed
            evidenceDirectoryPresent = $evidenceDirectoryPresent
            attemptState = "runner-started-terminal"
            retryAllowed = $false
            finishedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
            pathsRecorded = $false
            secretsRecorded = $false
        })
        Assert-ExactFinalRecord $finalRecord
        $runnerCapture.Process.Dispose()
        $runnerCapture = $null
        if (-not $acceptanceCompleted) {
            Write-TerminalFailure $files "runner-finished" "runner-started-terminal" $(
                if ($watcherRefusedWhileRunnerLive) { "watcher-refused" }
                elseif (-not $summaryExists) { "runner-summary-missing" }
                elseif (-not $summaryPassed) { $summaryFailureReasonCode }
                elseif ($runnerExit -ne 0) { "runner-nonzero" }
                elseif (-not $watcherAccepted -or -not $handoffPublished) { "watcher-handoff-missing" }
                else { "prior-terminal-failure" }
            )
        }
        Write-CreateOnceJson $files.Final $finalRecord
    }
    catch {
        $attemptState = Get-ObservedAttemptState `
            $files `
            $config
        Write-TerminalFailure $files "worker-exception" $attemptState "coordinator-exception"
        throw
    }
    finally {
        if ($null -ne $watcherCapture) {
            try {
                if (-not $watcherCapture.Process.HasExited) { $watcherCapture.Process.Kill() }
                $null = Complete-CapturedProcess $watcherCapture -TimeoutMilliseconds 10000
            }
            catch {}
            finally { $watcherCapture.Process.Dispose() }
        }
        if ($null -ne $runnerCapture) {
            try {
                if (-not $runnerCapture.Process.HasExited) { $runnerCapture.Process.Kill() }
                $null = Complete-CapturedProcess $runnerCapture -TimeoutMilliseconds 10000
            }
            catch {}
            finally { $runnerCapture.Process.Dispose() }
        }
        if ($null -ne $runnerInfo) {
            $runnerInfo.EnvironmentVariables.Remove("LBB_TOKEN")
        }
        $token = $null
        [GC]::KeepAlive($script:ProcessLifetimeCoordinatorMutex)
        [GC]::KeepAlive($workerLifetimeJob)
    }
}

function Get-BootstrapFieldNames {
    return @(
        "MODE", "VERSION", "SERVER", "HELPER", "MANIFEST", "MANIFEST_SHA256",
        "BINDING", "FIXTURE", "EVIDENCE", "COORDINATOR", "ARM_TIMEOUT", "START_TIMEOUT"
    )
}

function Invoke-CleanBootstrap {
    param([Parameter(Mandatory = $true)][string]$SystemPowerShell)
    $nonce = [Guid]::NewGuid().ToString("N")
    $info = New-ProcessStartInfo $SystemPowerShell @(
        "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $PSCommandPath,
        "-CleanCoordinatorNonce", $nonce
    ) ([IO.Path]::GetDirectoryName($SystemPowerShell)) -Hidden
    Set-ExactProcessEnvironment $info (Get-WhitelistedWorkerEnvironment)
    $values = [ordered]@{
        MODE = $Mode; VERSION = $Version; SERVER = $ServerPath; HELPER = $HelperPath
        MANIFEST = $ChecksumManifest; MANIFEST_SHA256 = $ChecksumManifestSha256
        BINDING = $CandidateBindingPath; FIXTURE = $FixturePath; EVIDENCE = $EvidenceDirectory
        COORDINATOR = $CoordinatorDirectory; ARM_TIMEOUT = [string]$ForegroundArmTimeoutSeconds
        START_TIMEOUT = [string]$StartupTimeoutSeconds
    }
    $info.EnvironmentVariables["LBB_WINDOWS_COORDINATOR_BOOTSTRAP_NONCE"] = $nonce
    foreach ($name in Get-BootstrapFieldNames) {
        $info.EnvironmentVariables["LBB_WINDOWS_COORDINATOR_$name"] = [string]$values[$name]
    }
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $started = $false
    $stdoutTask = $null
    $stderrTask = $null
    $process.StartInfo = $info
    try {
        if (-not $process.Start()) { throw "The clean coordinator child did not start." }
        $started = $true
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $bootstrapTimeout = if ($Mode -ceq "SelfTest") { 900000 } else { 60000 }
        if (-not $process.WaitForExit($bootstrapTimeout)) {
            $process.Kill()
            if (-not $process.WaitForExit(5000)) {
                throw "The clean coordinator child exceeded its deadline and termination was not confirmed."
            }
            throw "The clean coordinator child exceeded its execution deadline."
        }
        if (-not $stdoutTask.Wait($script:MaximumStreamDrainMilliseconds) -or
            -not $stderrTask.Wait($script:MaximumStreamDrainMilliseconds)) {
            throw "The clean coordinator output streams did not finish within their bounded drain interval."
        }
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        if (-not [String]::IsNullOrEmpty($stdout)) { [Console]::Out.Write($stdout) }
        if (-not [String]::IsNullOrEmpty($stderr)) { [Console]::Error.Write($stderr) }
        exit ([int]$process.ExitCode)
    }
    finally {
        if ($started) {
            try {
                if (-not $process.HasExited) {
                    $process.Kill()
                    $null = $process.WaitForExit(5000)
                }
            }
            catch {}
        }
        $process.Dispose()
    }
}

function Import-CleanBootstrap {
    param([Parameter(Mandatory = $true)][string]$ExpectedNonce)
    $actualNonce = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_COORDINATOR_BOOTSTRAP_NONCE", "Process")
    if ($ExpectedNonce -cnotmatch '^[0-9a-f]{32}$' -or $actualNonce -cne $ExpectedNonce) {
        throw "The clean coordinator nonce is invalid."
    }
    $values = @{}
    foreach ($name in Get-BootstrapFieldNames) {
        $key = "LBB_WINDOWS_COORDINATOR_$name"
        $values[$name] = [Environment]::GetEnvironmentVariable($key, "Process")
        [Environment]::SetEnvironmentVariable($key, $null, "Process")
    }
    [Environment]::SetEnvironmentVariable("LBB_WINDOWS_COORDINATOR_BOOTSTRAP_NONCE", $null, "Process")
    return $values
}

function Start-Coordinator {
    Assert-InteractiveInputDesktop
    if ($Version -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+$') { throw "Version is invalid." }
    if ($Version -cne $script:ProductVersion) {
        throw "This coordinator is bound only to version $($script:ProductVersion)."
    }
    if ($ChecksumManifestSha256 -cnotmatch '^[0-9a-fA-F]{64}$') {
        throw "ChecksumManifestSha256 is invalid."
    }
    $scriptPath = Resolve-OrdinaryPath $PSCommandPath $true "Coordinator script"
    $sourceDirectory = Resolve-OrdinaryPath ([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($scriptPath))) $false "Source directory"
    $runnerScript = Resolve-OrdinaryPath ([IO.Path]::Combine($sourceDirectory, "scripts", "test-windows-computer-use.ps1")) $true "Windows acceptance runner"
    $watcherScript = Resolve-OrdinaryPath ([IO.Path]::Combine($sourceDirectory, "scripts", "wait-windows-foreground-arm-handoff.ps1")) $true "Windows foreground watcher"
    $resolvedServer = Resolve-OrdinaryPath $ServerPath $true "ServerPath"
    $resolvedHelper = Resolve-OrdinaryPath $HelperPath $true "HelperPath"
    $resolvedManifest = Resolve-OrdinaryPath $ChecksumManifest $true "ChecksumManifest"
    $resolvedBinding = Resolve-OrdinaryPath $CandidateBindingPath $true "CandidateBindingPath"
    $resolvedFixture = Resolve-OrdinaryPath $FixturePath $true "FixturePath"
    if ((Get-FileSha256 $resolvedManifest) -cne $ChecksumManifestSha256.ToLowerInvariant()) {
        throw "ChecksumManifest does not match its independently supplied SHA-256."
    }
    $coordinatorParent = Get-PrivateAcceptanceParent "Coordinator"
    $evidenceParent = Get-PrivateAcceptanceParent "Evidence"
    $resolvedEvidence = Resolve-NewOrdinaryPath $EvidenceDirectory "EvidenceDirectory"
    $resolvedCoordinator = Resolve-NewOrdinaryPath $CoordinatorDirectory "CoordinatorDirectory"
    Assert-DirectPrivateChildPath $resolvedCoordinator $coordinatorParent "CoordinatorDirectory"
    Assert-DirectPrivateChildPath $resolvedEvidence $evidenceParent "EvidenceDirectory"
    $attemptKey = Get-CandidateAttemptKey `
        -CandidateVersion $Version `
        -ManifestSha256 $ChecksumManifestSha256 `
        -Server $resolvedServer `
        -Helper $resolvedHelper `
        -Manifest $resolvedManifest `
        -Binding $resolvedBinding
    $attemptLedgerRoot = Get-AttemptLedgerRoot
    # Resolve the final reservation identity before staging, but do not consume
    # the one-shot version boundary until every private input and coordinator
    # record is prepared. A pre-dispatch interruption can therefore leave
    # diagnosable scratch state without needlessly withdrawing the version.
    $attemptLedgerPath = Get-CandidateAttemptReservationPath `
        -LedgerRoot $attemptLedgerRoot `
        -AttemptKey $attemptKey
    $coordinatorInstanceId = [Guid]::NewGuid().ToString("N")
    [IO.Directory]::CreateDirectory($resolvedCoordinator) | Out-Null
    [IO.Directory]::CreateDirectory($resolvedEvidence) | Out-Null
    $resolvedCoordinatorAfterCreation = Resolve-OrdinaryPath $resolvedCoordinator $false "CoordinatorDirectory"
    $resolvedEvidenceAfterCreation = Resolve-OrdinaryPath $resolvedEvidence $false "EvidenceDirectory"
    if ($resolvedCoordinatorAfterCreation -cne $resolvedCoordinator -or
        $resolvedEvidenceAfterCreation -cne $resolvedEvidence) {
        throw "A newly created acceptance directory changed identity."
    }
    Set-PrivateDirectoryAcl $resolvedCoordinator
    Set-PrivateDirectoryAcl $resolvedEvidence
    Assert-PrivateDirectoryAcl $resolvedCoordinator
    Assert-PrivateDirectoryAcl $resolvedEvidence
    $files = Get-CoordinatorFiles $resolvedCoordinator
    $stagedSource = New-PrivateChildDirectory $resolvedCoordinator "staged-source" "Staged source directory"
    $stagedScripts = New-PrivateChildDirectory $stagedSource "scripts" "Staged scripts directory"
    $stagedTests = New-PrivateChildDirectory $stagedSource "tests" "Staged tests directory"
    $stagedFixtures = New-PrivateChildDirectory $stagedTests "fixtures" "Staged fixtures directory"
    $stagedWindowsFixtures = New-PrivateChildDirectory $stagedFixtures "windows" "Staged Windows fixtures directory"
    $stagedCandidate = New-PrivateChildDirectory $resolvedCoordinator "candidate" "Staged candidate directory"
    $stagedCoordinatorScript = Copy-FileToPrivateStage `
        $scriptPath `
        ([IO.Path]::Combine($stagedScripts, "run-windows-computer-use-acceptance.ps1")) `
        "Coordinator script"
    $stagedRunnerScript = Copy-FileToPrivateStage `
        $runnerScript `
        ([IO.Path]::Combine($stagedScripts, "test-windows-computer-use.ps1")) `
        "Windows acceptance runner"
    $stagedWatcherScript = Copy-FileToPrivateStage `
        $watcherScript `
        ([IO.Path]::Combine($stagedScripts, "wait-windows-foreground-arm-handoff.ps1")) `
        "Windows foreground watcher"
    $stagedFixture = Copy-FileToPrivateStage `
        $resolvedFixture `
        ([IO.Path]::Combine($stagedWindowsFixtures, "WindowsComputerUseFixture.ps1")) `
        "Windows fixture"
    $stagedServer = Copy-FileToPrivateStage `
        $resolvedServer `
        ([IO.Path]::Combine($stagedCandidate, [IO.Path]::GetFileName($resolvedServer))) `
        "Server candidate"
    $stagedHelper = Copy-FileToPrivateStage `
        $resolvedHelper `
        ([IO.Path]::Combine($stagedCandidate, [IO.Path]::GetFileName($resolvedHelper))) `
        "Helper candidate"
    $stagedManifest = Copy-FileToPrivateStage `
        $resolvedManifest `
        ([IO.Path]::Combine($stagedCandidate, "SHA256SUMS.txt")) `
        "Checksum manifest"
    $stagedBinding = Copy-FileToPrivateStage `
        $resolvedBinding `
        ([IO.Path]::Combine($stagedCandidate, "candidate-binding.json")) `
        "Candidate binding"
    $workerSupport = New-WorkerLifetimeSupportAssembly $files.WorkerSupport

    $config = [ordered]@{
        schemaVersion = $script:SchemaVersion
        version = $Version
        sourceDirectory = $stagedSource
        coordinatorScript = $stagedCoordinatorScript
        runnerScript = $stagedRunnerScript
        watcherScript = $stagedWatcherScript
        serverPath = $stagedServer
        helperPath = $stagedHelper
        checksumManifest = $stagedManifest
        checksumManifestSha256 = $ChecksumManifestSha256.ToLowerInvariant()
        candidateBindingPath = $stagedBinding
        fixturePath = $stagedFixture
        evidenceDirectory = $resolvedEvidence
        coordinatorDirectory = $resolvedCoordinator
        foregroundArmTimeoutSeconds = $ForegroundArmTimeoutSeconds
        attemptKey = $attemptKey
        attemptLedgerPath = $attemptLedgerPath
        coordinatorInstanceId = $coordinatorInstanceId
        workerSupportAssembly = [string]$workerSupport.Path
        workerSupportSha256 = [string]$workerSupport.Sha256
        coordinatorScriptSha256 = Get-FileSha256 $stagedCoordinatorScript
        runnerScriptSha256 = Get-FileSha256 $stagedRunnerScript
        watcherScriptSha256 = Get-FileSha256 $stagedWatcherScript
        fixtureSha256 = Get-FileSha256 $stagedFixture
        serverSha256 = Get-FileSha256 $stagedServer
        helperSha256 = Get-FileSha256 $stagedHelper
        candidateBindingSha256 = Get-FileSha256 $stagedBinding
    }
    Write-CreateOnceJson $files.Config $config
    Write-CreateOnceJson $files.Start ([ordered]@{
        schemaVersion = $script:SchemaVersion
        kind = "windows-acceptance-start-request"
        status = "accepted"
        version = $Version
        coordinatorInstanceId = $coordinatorInstanceId
        attemptState = "not-started"
        retryOnUnknownOutcome = $false
        recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
        pathsRecorded = $false
        secretsRecorded = $false
    })
    $validatedConfig = Read-BoundedJson `
        $files.Config `
        65536 `
        "Private coordinator configuration"
    Assert-ExactConfiguration $validatedConfig -BeforeReservation
    Assert-ExactStartRecord `
        (Read-BoundedJson $files.Start 16384 "Start request record") `
        $validatedConfig

    $systemPowerShell = Resolve-SystemWindowsPowerShell
    $workerNonce = [Guid]::NewGuid().ToString("N")
    $workerArguments = Join-NativeArguments @(
        "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $stagedCoordinatorScript,
        "-Mode", "Start", "-InternalWorkerNonce", $workerNonce
    )
    $workerEnvironment = Get-WhitelistedWorkerEnvironment
    $workerEnvironment["LBB_WINDOWS_COORDINATOR_WORKER_NONCE"] = $workerNonce
    $workerEnvironment["LBB_WINDOWS_COORDINATOR_CONFIG"] = $files.Config
    $worker = $null
    $workerHandedOff = $false
    $startOutput = $null
    try {
        $worker = Start-DetachedWorkerProcess `
            -Executable $systemPowerShell `
            -Arguments $workerArguments `
            -WorkingDirectory $stagedSource `
            -StdoutPath $files.WorkerOut `
            -StderrPath $files.WorkerErr `
            -Environment $workerEnvironment
        if ($null -eq $worker) { throw "The retained coordinator worker did not start." }
        $deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
        while ([DateTime]::UtcNow -lt $deadline -and
            -not [IO.File]::Exists($files.Worker) -and
            -not [IO.File]::Exists($files.Failure) -and
            -not $worker.HasExited) {
            Start-Sleep -Milliseconds 100
        }
        if ([IO.File]::Exists($files.Failure)) {
            Stop-DetachedWorkerProcessExact $worker "Retained coordinator worker"
            throw "The retained coordinator worker failed closed during startup."
        }
        if (-not [IO.File]::Exists($files.Worker)) {
            Stop-DetachedWorkerProcessExact $worker "Retained coordinator worker"
            Write-TerminalFailure $files "worker-startup" "not-started" "worker-start-record-missing"
            throw "The retained coordinator worker did not publish its start record."
        }
        $workerRecord = Read-BoundedJson $files.Worker 16384 "Worker start record"
        Assert-ExactWorkerRecord $workerRecord
        $workerStartedAt = [DateTime]::Parse(
            [string]$workerRecord.workerStartedAtUtc,
            [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind
        ).ToUniversalTime()
        $workerIdentityMatched = (
            [int]$workerRecord.workerPid -eq $worker.Id -and
            $workerStartedAt.Ticks -eq $worker.StartedAtUtc.Ticks -and
            -not $worker.HasExited -and
            (Test-BoundProcessAlive $worker.Id $workerStartedAt)
        )
        if (-not $workerIdentityMatched -or [IO.File]::Exists($files.Failure)) {
            Stop-DetachedWorkerProcessExact $worker "Retained coordinator worker"
            $attemptState = Get-ObservedAttemptState $files $config
            Write-TerminalFailure $files "worker-startup" $attemptState "worker-identity-or-terminal-state-invalid"
            throw "The retained coordinator worker identity or terminal state is invalid."
        }
        $worker.TransferGuardOwnership()
        if (-not $worker.GuardOwnershipTransferred -or $worker.HasExited) {
            throw "The retained coordinator worker did not take ownership of its kill-on-close guard Job."
        }
        $ownershipRecord = [pscustomobject]([ordered]@{
            schemaVersion = $script:SchemaVersion
            kind = "windows-acceptance-worker-ownership-transferred"
            status = "guard-owned-by-worker"
            workerPid = [int]$workerRecord.workerPid
            workerStartedAtUtc = [string]$workerRecord.workerStartedAtUtc
            attemptState = "not-started"
            retryOnUnknownOutcome = $false
            recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
            pathsRecorded = $false
            secretsRecorded = $false
        })
        Assert-ExactOwnershipRecord $ownershipRecord $workerRecord
        Write-CreateOnceJson $files.Ownership $ownershipRecord
        if ($worker.HasExited -or
            -not (Test-BoundProcessAlive $worker.Id $workerStartedAt)) {
            throw "The retained coordinator worker exited during guard ownership handoff."
        }
        $startOutput = (([ordered]@{
            schemaVersion = $script:SchemaVersion
            kind = "windows-acceptance-coordinator-started"
            status = "running"
            workerPid = [int]$workerRecord.workerPid
            workerStartedAtUtc = [string]$workerRecord.workerStartedAtUtc
            attemptState = "not-started-or-transitioning"
            foregroundGateMode = $script:ForegroundGateMode
            operatorActionRequired = $false
            action = "none"
            retryOnUnknownOutcome = $false
            pathsRecorded = $false
            secretsRecorded = $false
        }) | ConvertTo-Json -Compress)
        $workerHandedOff = $true
    }
    catch {
        $workerCleanupConfirmed = -not (Test-ExceptionChainTypeName `
            $_.Exception `
            "LbbCoordinator.DetachedWorkerCleanupException")
        if (-not $workerHandedOff -and $null -ne $worker) {
            try {
                Stop-DetachedWorkerProcessExact $worker "Retained coordinator worker"
            }
            catch { $workerCleanupConfirmed = $false }
        }
        if (-not $workerHandedOff -and -not [IO.File]::Exists($files.Failure)) {
            $attemptState = Get-ObservedAttemptState $files $config
            if (-not $workerCleanupConfirmed -and $attemptState -ceq "not-started") {
                $attemptState = "candidate-execution-unknown"
            }
            $reasonCode = if ($workerCleanupConfirmed) {
                "worker-launch-failed"
            }
            else { "worker-cleanup-unconfirmed" }
            Write-TerminalFailure $files "worker-startup" $attemptState $reasonCode
        }
        if (-not $workerCleanupConfirmed) {
            throw "The retained coordinator worker cleanup could not be confirmed."
        }
        throw
    }
    finally {
        if ($null -ne $worker) { $worker.Dispose() }
    }
    Write-Output $startOutput
}

function Write-FollowFailureOutput {
    param(
        [Parameter(Mandatory = $true)][string]$Stage,
        [Parameter(Mandatory = $true)][string]$AttemptState,
        [Parameter(Mandatory = $true)][string]$ReasonCode,
        [Nullable[int]]$ExitCode,
        [Nullable[bool]]$SummaryPresent
    )
    $record = [ordered]@{
        schemaVersion = $script:SchemaVersion
        kind = "windows-acceptance-follow"
        status = "failed-closed"
        stage = $Stage
        attemptState = $AttemptState
        reasonCode = $ReasonCode
        retryAllowed = $false
        uiActionAllowed = $false
        notificationOnly = $true
        acceptedAsAuthority = $false
        pathsRecorded = $false
        secretsRecorded = $false
    }
    if ($null -ne $ExitCode) { $record.exitCode = [int]$ExitCode }
    if ($null -ne $SummaryPresent) { $record.summaryPresent = [bool]$SummaryPresent }
    Write-Output ($record | ConvertTo-Json -Compress)
}

function Write-FollowWaitingOutput {
    param(
        [Parameter(Mandatory = $true)][string]$AttemptState,
        [Parameter(Mandatory = $true)][string]$Phase
    )
    Write-Output (([ordered]@{
        schemaVersion = $script:SchemaVersion
        kind = "windows-acceptance-follow"
        status = "waiting"
        phase = $Phase
        attemptState = $AttemptState
        retryOnUnknownOutcome = $false
        uiActionAllowed = $false
        notificationOnly = $true
        acceptedAsAuthority = $false
        pathsRecorded = $false
        secretsRecorded = $false
    }) | ConvertTo-Json -Compress)
}

function Get-TerminalFollowOutput {
    param(
        [Parameter(Mandatory = $true)][Collections.IDictionary]$Files,
        [Parameter(Mandatory = $true)][object]$Config
    )
    if ([IO.File]::Exists($Files.Failure)) {
        $failure = Read-BoundedJson $Files.Failure 16384 "Terminal failure record"
        Assert-ExactFailureRecord $failure
        $failureChain = Get-ValidatedFollowChain $Files $Config -SkipFinal
        $attemptReservationRelationship = Get-AttemptReservationRelationship $Config
        if ([string]$failure.attemptState -ceq "not-started" -and
            ($attemptReservationRelationship -ceq "owned" -or
                $null -ne $failureChain.Intent -or
                $null -ne $failureChain.Runner)) {
            throw "A not-started terminal failure cannot follow a persistent boundary, intent, or runner record."
        }
        if ([string]$failure.attemptState -ceq "candidate-execution-unknown" -and
            $null -ne $failureChain.Runner) {
            throw "An outcome-unknown terminal failure cannot follow a conclusive runner record."
        }
        if ([string]$failure.attemptState -ceq "runner-started-terminal" -and
            $null -eq $failureChain.Runner) {
            throw "A runner-started terminal failure has no runner predecessor."
        }
        $finalForFailure = $null
        if ([IO.File]::Exists($Files.Final)) {
            try {
                $value = Read-BoundedJson $Files.Final 16384 "Runner final record"
                Assert-ExactFinalRecord $value
                $finalForFailure = $value
            }
            catch {
                # A malformed later final record cannot outrank the first
                # valid terminal failure record.
            }
        }
        return [string](Write-FollowFailureOutput `
            -Stage ([string]$failure.stage) `
            -AttemptState ([string]$failure.attemptState) `
            -ReasonCode ([string]$failure.reasonCode) `
            -ExitCode $(if ($null -ne $finalForFailure) { [int]$finalForFailure.exitCode } else { $null }) `
            -SummaryPresent $(if ($null -ne $finalForFailure) { [bool]$finalForFailure.summaryPresent } else { $null }))
    }
    if (-not [IO.File]::Exists($Files.Final)) { return }
    $chain = Get-ValidatedFollowChain $Files $Config
    $final = $chain.Final
    if ([IO.File]::Exists($Files.Failure)) {
        return (Get-TerminalFollowOutput $Files $Config)
    }
    if ([string]$final.status -cne "completed" -or [int]$final.exitCode -ne 0 -or
        $final.summaryPresent -ne $true -or $final.summaryPassed -ne $true -or
        $final.evidenceDirectoryPresent -ne $true -or
        [string]$final.attemptState -cne "runner-started-terminal" -or
        $final.retryAllowed -ne $false) {
        return [string](Write-FollowFailureOutput `
            -Stage "runner-finished" `
            -AttemptState ([string]$final.attemptState) `
            -ReasonCode "final-record-not-successful" `
            -ExitCode ([int]$final.exitCode) `
            -SummaryPresent ([bool]$final.summaryPresent))
    }
    if ($null -eq $chain.Watcher -or [string]$chain.Watcher.status -cne "accepted" -or
        $null -eq $chain.Handoff) {
        throw "A completed final record does not have the full accepted handoff chain."
    }
    $runnerStartedAt = (ConvertFrom-CanonicalUtcString `
        $chain.Runner.runnerStartedAtUtc `
        "runner start time").UtcDateTime
    if (Test-BoundProcessAlive ([int]$chain.Runner.runnerPid) $runnerStartedAt) {
        return [string](Write-FollowFailureOutput `
            -Stage "runner-finished" `
            -AttemptState "runner-started-terminal" `
            -ReasonCode "final-record-runner-still-live" `
            -ExitCode ([int]$final.exitCode) `
            -SummaryPresent ([bool]$final.summaryPresent))
    }
    $workerStartedAt = (ConvertFrom-CanonicalUtcString `
        $chain.Worker.workerStartedAtUtc `
        "worker start time").UtcDateTime
    if (Test-BoundProcessAlive ([int]$chain.Worker.workerPid) $workerStartedAt) {
        return [string](Write-FollowWaitingOutput `
            -AttemptState "runner-started-terminal" `
            -Phase "worker-terminal-cleanup")
    }
    if ([IO.File]::Exists($Files.Failure)) {
        return (Get-TerminalFollowOutput $Files $Config)
    }
    return (([ordered]@{
        schemaVersion = $script:SchemaVersion
        kind = "windows-acceptance-follow"
        status = "completed"
        exitCode = [int]$final.exitCode
        summaryPresent = [bool]$final.summaryPresent
        summaryPassed = [bool]$final.summaryPassed
        attemptState = [string]$final.attemptState
        retryAllowed = $false
        uiActionAllowed = $false
        notificationOnly = $true
        acceptedAsAuthority = $false
        pathsRecorded = $false
        secretsRecorded = $false
    }) | ConvertTo-Json -Compress)
}

function Follow-Coordinator {
    $root = Resolve-OrdinaryPath $CoordinatorDirectory $false "CoordinatorDirectory"
    Assert-PrivateDirectoryAcl $root
    $files = Get-CoordinatorFiles $root
    $config = Read-BoundedJson $files.Config 65536 "Private coordinator configuration"
    Assert-ExactConfigurationForCurrentReservationState $config $files
    if ([string]$config.coordinatorDirectory -cne $root) {
        throw "Follow is not bound to the configured private coordinator directory."
    }
    $terminalOutput = @(Get-TerminalFollowOutput $files $config)
    if ($terminalOutput.Count -gt 0) {
        if ($terminalOutput.Count -ne 1) { throw "Terminal Follow projection is not singular." }
        Write-Output ([string]$terminalOutput[0])
        return
    }
    $followChain = Get-ValidatedFollowChain $files $config -SkipFinal

    if (-not [IO.File]::Exists($files.Worker)) {
        $attemptState = Get-ObservedAttemptState $files $config
        Write-FollowFailureOutput `
            -Stage "worker-liveness" `
            -AttemptState $attemptState `
            -ReasonCode "worker-start-record-missing"
        return
    }
    $workerRecord = Read-BoundedJson $files.Worker 16384 "Worker start record"
    Assert-ExactWorkerRecord $workerRecord
    $workerStartedAt = (ConvertFrom-CanonicalUtcString `
        $workerRecord.workerStartedAtUtc `
        "worker start time").UtcDateTime
    $workerAlive = Test-BoundProcessAlive ([int]$workerRecord.workerPid) $workerStartedAt
    if (-not [IO.File]::Exists($files.Ownership)) {
        $attemptState = Get-ObservedAttemptState $files $config
        if ($workerAlive) {
            Write-FollowWaitingOutput `
                -AttemptState $attemptState `
                -Phase "worker-guard-ownership-transfer"
        }
        else {
            Write-FollowFailureOutput `
                -Stage "worker-ownership" `
                -AttemptState $attemptState `
                -ReasonCode "guard-ownership-record-missing"
        }
        return
    }
    Assert-ExactOwnershipRecord `
        (Read-BoundedJson $files.Ownership 16384 "Worker ownership record") `
        $workerRecord
    if (-not $workerAlive) {
        $terminalOutput = @(Get-TerminalFollowOutput $files $config)
        if ($terminalOutput.Count -gt 0) {
            if ($terminalOutput.Count -ne 1) { throw "Terminal Follow projection is not singular." }
            Write-Output ([string]$terminalOutput[0])
            return
        }
        $attemptState = Get-ObservedAttemptState `
            $files `
            $config
        Write-FollowFailureOutput `
            -Stage "worker-liveness" `
            -AttemptState $attemptState `
            -ReasonCode "bound-worker-not-alive"
        return
    }
    if ([IO.File]::Exists($files.Handoff)) {
        if (-not [IO.File]::Exists($files.Runner) -or -not [IO.File]::Exists($files.Watcher)) {
            Write-FollowFailureOutput `
                -Stage "handoff-binding" `
                -AttemptState "candidate-execution-unknown" `
                -ReasonCode "handoff-prerequisite-record-missing"
            return
        }
        $runnerRecord = Read-BoundedJson $files.Runner 16384 "Runner start record"
        $watcherRecord = Read-BoundedJson $files.Watcher 16384 "Watcher final record"
        $handoff = Read-BoundedJson $files.Handoff 32768 "Handoff record"
        Assert-ExactRunnerRecord $runnerRecord
        Assert-ExactWatcherRecord $watcherRecord
        if ([string]$watcherRecord.status -cne "accepted") {
            throw "A handoff cannot follow a refused watcher record."
        }
        Assert-ExactHandoffRecord $handoff $config $runnerRecord
        $started = (ConvertFrom-CanonicalUtcString `
            $runnerRecord.runnerStartedAtUtc `
            "runner start time").UtcDateTime
        if (-not (Test-BoundProcessAlive ([int]$runnerRecord.runnerPid) $started)) {
            $terminalOutput = @(Get-TerminalFollowOutput $files $config)
            if ($terminalOutput.Count -gt 0) {
                if ($terminalOutput.Count -ne 1) { throw "Terminal Follow projection is not singular." }
                Write-Output ([string]$terminalOutput[0])
                return
            }
            if (-not (Test-BoundProcessAlive ([int]$workerRecord.workerPid) $workerStartedAt)) {
                $terminalOutput = @(Get-TerminalFollowOutput $files $config)
                if ($terminalOutput.Count -gt 0) {
                    if ($terminalOutput.Count -ne 1) { throw "Terminal Follow projection is not singular." }
                    Write-Output ([string]$terminalOutput[0])
                    return
                }
                Write-FollowFailureOutput `
                    -Stage "worker-liveness" `
                    -AttemptState "runner-started-terminal" `
                    -ReasonCode "bound-worker-not-alive"
                return
            }
            Write-FollowWaitingOutput `
                -AttemptState "runner-started-terminal" `
                -Phase "runner-finalizing"
            return
        }
        $terminalOutput = @(Get-TerminalFollowOutput $files $config)
        if ($terminalOutput.Count -gt 0) {
            if ($terminalOutput.Count -ne 1) { throw "Terminal Follow projection is not singular." }
            Write-Output ([string]$terminalOutput[0])
            return
        }
        if (-not (Test-BoundProcessAlive ([int]$workerRecord.workerPid) $workerStartedAt)) {
            $terminalOutput = @(Get-TerminalFollowOutput $files $config)
            if ($terminalOutput.Count -gt 0) {
                if ($terminalOutput.Count -ne 1) { throw "Terminal Follow projection is not singular." }
                Write-Output ([string]$terminalOutput[0])
                return
            }
            Write-FollowFailureOutput `
                -Stage "worker-liveness" `
                -AttemptState "runner-started-terminal" `
                -ReasonCode "bound-worker-not-alive"
            return
        }
        if (-not (Test-BoundProcessAlive ([int]$runnerRecord.runnerPid) $started)) {
            $terminalOutput = @(Get-TerminalFollowOutput $files $config)
            if ($terminalOutput.Count -gt 0) {
                if ($terminalOutput.Count -ne 1) { throw "Terminal Follow projection is not singular." }
                Write-Output ([string]$terminalOutput[0])
                return
            }
            if (-not (Test-BoundProcessAlive ([int]$workerRecord.workerPid) $workerStartedAt)) {
                Write-FollowFailureOutput `
                    -Stage "worker-liveness" `
                    -AttemptState "runner-started-terminal" `
                    -ReasonCode "bound-worker-not-alive"
                return
            }
            Write-FollowWaitingOutput `
                -AttemptState "runner-started-terminal" `
                -Phase "runner-finalizing"
            return
        }
        Write-Output (([ordered]@{
            schemaVersion = $script:SchemaVersion
            productVersion = [string]$config.version
            kind = "windows-acceptance-follow"
            status = "automatic-ready"
            requestId = [string]$handoff.requestId
            publishedAtUtc = [string]$handoff.publishedAtUtc
            receivedAtUtc = [string]$handoff.receivedAtUtc
            observedAtUtc = [string]$handoff.observedAtUtc
            deadlineAtUtc = [string]$handoff.deadlineAtUtc
            mode = [string]$handoff.mode
            operatorActionRequired = [bool]$handoff.operatorActionRequired
            action = [string]$handoff.action
            clickAttemptsObserved = [int]$handoff.clickAttemptsObserved
            stableSamplesObserved = [int]$handoff.stableSamplesObserved
            stableSamplesRequired = [int]$handoff.stableSamplesRequired
            nativeSampleSeqlockMatched = [bool]$handoff.nativeSampleSeqlockMatched
            ownerIdentityStable = [bool]$handoff.ownerIdentityStable
            focusRootMatched = [bool]$handoff.focusRootMatched
            fixtureProcessExcluded = [bool]$handoff.fixtureProcessExcluded
            interactiveSessionMatched = [bool]$handoff.interactiveSessionMatched
            cursorStable = [bool]$handoff.cursorStable
            inputDesktopStable = [bool]$handoff.inputDesktopStable
            globalInputUsed = [bool]$handoff.globalInputUsed
            focusChangedByRunner = [bool]$handoff.focusChangedByRunner
            cursorChangedByRunner = [bool]$handoff.cursorChangedByRunner
            syntheticInputUsed = [bool]$handoff.syntheticInputUsed
            retryOnUnknownOutcome = $false
            runnerIdentityMatched = [bool]$handoff.runnerIdentityMatched
            requestFresh = [bool]$handoff.requestFresh
            receivedBeforeDeadline = [bool]$handoff.receivedBeforeDeadline
            uiActionAllowed = $false
            notificationOnly = [bool]$handoff.notificationOnly
            acceptedAsAuthority = [bool]$handoff.acceptedAsAuthority
            rawWindowHandlesRecorded = [bool]$handoff.rawWindowHandlesRecorded
            rawProcessIdentifiersRecorded = [bool]$handoff.rawProcessIdentifiersRecorded
            rawCursorCoordinatesRecorded = [bool]$handoff.rawCursorCoordinatesRecorded
            pathsRecorded = [bool]$handoff.pathsRecorded
            secretsRecorded = [bool]$handoff.secretsRecorded
        }) | ConvertTo-Json -Compress)
        return
    }
    if ([IO.File]::Exists($files.Intent)) {
        Assert-ExactIntentRecord (Read-BoundedJson $files.Intent 16384 "Runner launch intent record")
    }
    if ([IO.File]::Exists($files.Runner)) {
        Assert-ExactRunnerRecord (Read-BoundedJson $files.Runner 16384 "Runner start record")
    }
    if ([IO.File]::Exists($files.Watcher)) {
        Assert-ExactWatcherRecord (Read-BoundedJson $files.Watcher 16384 "Watcher final record")
    }
    $attemptState = Get-ObservedAttemptState `
        $files `
        $config
    $terminalOutput = @(Get-TerminalFollowOutput $files $config)
    if ($terminalOutput.Count -gt 0) {
        if ($terminalOutput.Count -ne 1) { throw "Terminal Follow projection is not singular." }
        Write-Output ([string]$terminalOutput[0])
        return
    }
    if ($null -ne $followChain.Runner) {
        $runnerStartedAt = (ConvertFrom-CanonicalUtcString `
            $followChain.Runner.runnerStartedAtUtc `
            "runner start time").UtcDateTime
        if (-not (Test-BoundProcessAlive ([int]$followChain.Runner.runnerPid) $runnerStartedAt)) {
            $terminalOutput = @(Get-TerminalFollowOutput $files $config)
            if ($terminalOutput.Count -gt 0) {
                if ($terminalOutput.Count -ne 1) { throw "Terminal Follow projection is not singular." }
                Write-Output ([string]$terminalOutput[0])
                return
            }
            if (-not (Test-BoundProcessAlive ([int]$workerRecord.workerPid) $workerStartedAt)) {
                Write-FollowFailureOutput `
                    -Stage "worker-liveness" `
                    -AttemptState "runner-started-terminal" `
                    -ReasonCode "bound-worker-not-alive"
                return
            }
            Write-FollowWaitingOutput `
                -AttemptState "runner-started-terminal" `
                -Phase "runner-finalizing"
            return
        }
    }
    if (-not (Test-BoundProcessAlive ([int]$workerRecord.workerPid) $workerStartedAt)) {
        Write-FollowFailureOutput `
            -Stage "worker-liveness" `
            -AttemptState $attemptState `
            -ReasonCode "bound-worker-not-alive"
        return
    }
    Write-FollowWaitingOutput `
        -AttemptState $attemptState `
        -Phase "runner-starting-or-waiting-for-handoff"
}

function Remove-SelfTestStreamFiles {
    param(
        [Parameter(Mandatory = $true)][string]$TestRoot,
        [Parameter(Mandatory = $true)][string[]]$Paths
    )
    $rootBoundary = [IO.Path]::GetFullPath($TestRoot).TrimEnd(
        [IO.Path]::DirectorySeparatorChar,
        [IO.Path]::AltDirectorySeparatorChar
    ) + [IO.Path]::DirectorySeparatorChar
    foreach ($path in $Paths) {
        $fullPath = [IO.Path]::GetFullPath($path)
        if (-not $fullPath.StartsWith($rootBoundary, [StringComparison]::OrdinalIgnoreCase)) {
            throw "A self-test stream path escaped its GUID-scoped root."
        }
        if (-not [IO.File]::Exists($fullPath)) { continue }
        $exclusive = $null
        try {
            $exclusive = [IO.File]::Open(
                $fullPath,
                [IO.FileMode]::Open,
                [IO.FileAccess]::ReadWrite,
                [IO.FileShare]::None
            )
        }
        finally {
            if ($null -ne $exclusive) { $exclusive.Dispose() }
        }
        [IO.File]::Delete($fullPath)
        if ([IO.File]::Exists($fullPath)) {
            throw "A self-test stream file remained after exact cleanup."
        }
    }
}

function Complete-SelfTestCapturedProcess {
    param(
        [Parameter(Mandatory = $true)][object]$Capture,
        [Parameter(Mandatory = $true)][string]$TestRoot,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath,
        [switch]$Terminate
    )
    $exitCode = $null
    try {
        if ($Terminate -and -not $Capture.Process.HasExited) {
            $Capture.Process.Kill()
        }
        $exitCode = Complete-CapturedProcess $Capture -TimeoutMilliseconds 10000
    }
    finally {
        $Capture.Process.Dispose()
        Remove-SelfTestStreamFiles $TestRoot @($StdoutPath, $StderrPath)
    }
    return $exitCode
}

function Wait-SelfTestStateFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][object]$Capture,
        [ValidateRange(1, 60000)][int]$TimeoutMilliseconds = 10000
    )
    $deadline = [Diagnostics.Stopwatch]::StartNew()
    while (-not [IO.File]::Exists($Path) -and
        -not $Capture.Process.HasExited -and
        $deadline.ElapsedMilliseconds -lt $TimeoutMilliseconds) {
        $remaining = $TimeoutMilliseconds - $deadline.ElapsedMilliseconds
        Start-Sleep -Milliseconds ([Math]::Min(25, [Math]::Max(1, $remaining)))
    }
    if (-not [IO.File]::Exists($Path)) {
        throw "A named-Job self-test process did not publish its exact state."
    }
}

function ConvertTo-BoundedSelfTestFailureDetail {
    param([AllowEmptyString()][string]$Value)
    if ([String]::IsNullOrEmpty($Value)) {
        return "<empty>"
    }
    $detail = $Value.Replace("`r", " ").Replace("`n", " ")
    $detail = [Text.RegularExpressions.Regex]::Replace($detail, '\s+', ' ').Trim()
    if ($detail.Length -gt 2000) {
        $detail = $detail.Substring(0, 2000) + "[truncated]"
    }
    return $detail
}

function Invoke-NestedJobRunnerSelfTest {
    param([Parameter(Mandatory = $true)][string]$Nonce)
    $environmentNonce = [Environment]::GetEnvironmentVariable(
        "LBB_WINDOWS_NESTED_JOB_RUNNER_SELF_TEST_NONCE",
        "Process"
    )
    [Environment]::SetEnvironmentVariable(
        "LBB_WINDOWS_NESTED_JOB_RUNNER_SELF_TEST_NONCE",
        $null,
        "Process"
    )
    if ($Nonce -cnotmatch '^[0-9a-f]{32}$' -or $environmentNonce -cne $Nonce) {
        throw "The internal nested-Job runner self-test binding is invalid."
    }

    $testRoot = [IO.Path]::Combine(
        [IO.Path]::GetTempPath(),
        "lbb-nested-job-runner-self-test-$Nonce"
    )
    if ([IO.File]::Exists($testRoot) -or [IO.Directory]::Exists($testRoot)) {
        throw "The internal nested-Job runner self-test root is not fresh."
    }
    [IO.Directory]::CreateDirectory($testRoot) | Out-Null
    Set-PrivateDirectoryAcl $testRoot
    Assert-PrivateDirectoryAcl $testRoot
    $runnerCapture = $null
    $runnerOut = [IO.Path]::Combine($testRoot, "runner.stdout.log")
    $runnerErr = [IO.Path]::Combine($testRoot, "runner.stderr.log")
    $lifetimeJobName = "Local\LBBWindowsAcceptanceNestedRunnerSelfTest-$Nonce"
    try {
        # The calling process was created through Start-DetachedWorkerProcess,
        # so it already belongs to the production guard Job. Adding the same
        # breakaway-enabled lifetime Job used by the real worker reproduces the
        # exact two-Job parent chain before the real runner self-test launches.
        $lifetimeJob = New-WorkerLifetimeJob `
            -AllowChildBreakaway `
            -Name $lifetimeJobName
        if (-not $lifetimeJob.IsBound -or $lifetimeJob.RecoveredExistingJob) {
            throw "The internal nested-Job runner self-test did not bind its fresh lifetime Job."
        }

        $runnerScript = Resolve-OrdinaryPath (
            [IO.Path]::Combine($PSScriptRoot, "test-windows-computer-use.ps1")
        ) $true "Nested acceptance-runner self-test script"
        $runnerInfo = New-ProcessStartInfo `
            (Resolve-SystemWindowsPowerShell) `
            @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File",
                $runnerScript, "-SelfTest"
            ) `
            ([IO.Path]::GetFullPath([IO.Path]::Combine($PSScriptRoot, ".."))) `
            -Hidden
        Set-ExactProcessEnvironment $runnerInfo (Get-WhitelistedWorkerEnvironment)
        $runnerCapture = Start-CapturedProcess $runnerInfo $runnerOut $runnerErr
        $runnerExit = Complete-CapturedProcess `
            $runnerCapture `
            -TimeoutMilliseconds 240000
        $runnerStdout = [IO.File]::ReadAllText(
            $runnerOut,
            $script:Utf8NoBom
        ).TrimEnd([char[]]"`r`n")
        $runnerStderr = [IO.File]::ReadAllText($runnerErr, $script:Utf8NoBom)
        if ($runnerExit -ne 0 -or
            $runnerStdout -cne "Windows computer-use acceptance self-test passed." -or
            -not [String]::IsNullOrEmpty($runnerStderr)) {
            $runnerStdoutDetail = ConvertTo-BoundedSelfTestFailureDetail $runnerStdout
            $runnerStderrDetail = ConvertTo-BoundedSelfTestFailureDetail $runnerStderr
            throw "The exact guard-plus-lifetime Job runner self-test failed. (runnerExit=$runnerExit; stdout=$runnerStdoutDetail; stderr=$runnerStderrDetail)."
        }
        [GC]::KeepAlive($lifetimeJob)
    }
    finally {
        if ($null -ne $runnerCapture) { $runnerCapture.Process.Dispose() }
        Remove-SelfTestStreamFiles $testRoot @($runnerOut, $runnerErr)
        if ([IO.Directory]::Exists($testRoot)) {
            [IO.Directory]::Delete($testRoot, $false)
        }
    }
    Write-Output "Nested guard/lifetime Job runner self-test passed."
}

function Invoke-SelfTest {
    $testRoot = [IO.Path]::Combine([IO.Path]::GetTempPath(), "lbb-coordinator-self-test-" + [Guid]::NewGuid().ToString("N"))
    [IO.Directory]::CreateDirectory($testRoot) | Out-Null
    $selfTestLifetimeJob = $null
    $selfTestCleanLifetimeJob = $null
    $selfTestRecoveredLifetimeJob = $null
    $selfTestDelayedHandleLifetimeJob = $null
    $selfTestTimeoutCleanupLifetimeJob = $null
    $selfTestPostRaceLifetimeJob = $null
    $currentProcess = $null
    $originalSelfTestAttemptLedgerRoot = $script:SelfTestAttemptLedgerRoot
    $originalSelfTestCoordinatorParent = $script:SelfTestCoordinatorParent
    $originalSelfTestEvidenceParent = $script:SelfTestEvidenceParent
    $selfTestSucceeded = $false
    try {
        $null = Resolve-OrdinaryPath $testRoot $false "Coordinator self-test root"
        Set-PrivateDirectoryAcl $testRoot
        Assert-PrivateDirectoryAcl $testRoot
        $selfTestCoordinatorParent = New-PrivateChildDirectory $testRoot "coordinators" "Self-test coordinator parent"
        $selfTestEvidenceParent = New-PrivateChildDirectory $testRoot "evidence" "Self-test evidence parent"
        $selfTestLedgerRoot = New-PrivateChildDirectory $testRoot "ledger" "Self-test attempt ledger"
        $script:SelfTestCoordinatorParent = $selfTestCoordinatorParent
        $script:SelfTestEvidenceParent = $selfTestEvidenceParent
        $script:SelfTestAttemptLedgerRoot = $selfTestLedgerRoot
        $workerSupportProbeScript = Copy-FileToPrivateStage `
            (Resolve-OrdinaryPath $PSCommandPath $true "Coordinator self-test script") `
            ([IO.Path]::Combine($testRoot, "worker-support-loader-coordinator.ps1")) `
            "Worker support loader self-test coordinator"
        if ((Get-FileSha256 $workerSupportProbeScript) -cne
            (Get-FileSha256 (Resolve-OrdinaryPath `
                $PSCommandPath `
                $true `
                "Coordinator self-test script"))) {
            throw "The private staged loader self-test coordinator changed bytes."
        }
        $workerSupportProbePath = [IO.Path]::Combine(
            $testRoot,
            "worker-lifetime-support-loader-probe.dll"
        )
        $workerSupportProbe = New-WorkerLifetimeSupportAssembly $workerSupportProbePath
        $workerSupportProbeNonce = [Guid]::NewGuid().ToString("N")
        $workerSupportProbeOut = [IO.Path]::Combine(
            $testRoot,
            "worker-support-loader.stdout.log"
        )
        $workerSupportProbeErr = [IO.Path]::Combine(
            $testRoot,
            "worker-support-loader.stderr.log"
        )
        $workerSupportProbeInfo = New-ProcessStartInfo `
            (Resolve-SystemWindowsPowerShell) `
            @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $workerSupportProbeScript,
                "-Mode", "SelfTest",
                "-InternalWorkerSupportSelfTestPath", $workerSupportProbe.Path,
                "-InternalWorkerSupportSelfTestSha256", $workerSupportProbe.Sha256,
                "-InternalWorkerSupportSelfTestNonce", $workerSupportProbeNonce
            ) `
            $testRoot `
            -Hidden
        Set-ExactProcessEnvironment `
            $workerSupportProbeInfo `
            (Get-WhitelistedWorkerEnvironment)
        $workerSupportProbeInfo.EnvironmentVariables[
            "LBB_COORDINATOR_WORKER_SUPPORT_SELF_TEST_NONCE"
        ] = $workerSupportProbeNonce
        $workerSupportProbeCapture = Start-CapturedProcess `
            $workerSupportProbeInfo `
            $workerSupportProbeOut `
            $workerSupportProbeErr
        try {
            $workerSupportProbeExit = Complete-CapturedProcess `
                $workerSupportProbeCapture `
                -TimeoutMilliseconds 120000
            $workerSupportProbeStdout = [IO.File]::ReadAllText(
                $workerSupportProbeOut,
                $script:Utf8NoBom
            ).TrimEnd([char[]]"`r`n")
            $workerSupportProbeStderr = [IO.File]::ReadAllText(
                $workerSupportProbeErr,
                $script:Utf8NoBom
            )
            if ($workerSupportProbeExit -ne 0 -or
                $workerSupportProbeStdout -cne
                    "Worker lifetime support staged-loader self-test passed." -or
                -not [String]::IsNullOrEmpty($workerSupportProbeStderr)) {
                throw "The fresh staged worker-support loader self-test failed."
            }
            $workerSupportProbeFiles = Get-CoordinatorFiles $testRoot
            foreach ($unexpectedCandidateBoundary in @(
                $workerSupportProbeFiles.Start,
                $workerSupportProbeFiles.Worker,
                $workerSupportProbeFiles.Ownership,
                $workerSupportProbeFiles.Intent,
                $workerSupportProbeFiles.Runner,
                $workerSupportProbeFiles.Watcher,
                $workerSupportProbeFiles.Handoff,
                $workerSupportProbeFiles.Final,
                $workerSupportProbeFiles.Failure
            )) {
                if ([IO.File]::Exists($unexpectedCandidateBoundary)) {
                    throw "The staged loader self-test crossed a candidate execution boundary."
                }
            }
        }
        finally { $workerSupportProbeCapture.Process.Dispose() }
        $selfTestLifetimeJob = New-WorkerLifetimeJob -AllowChildBreakaway
        [LbbCoordinator.WorkerLifetimeJob]::WaitForNameAbsenceForSelfTest(
            "Local\LBBWindowsAcceptanceCoordinatorLoaderSelfTest-$workerSupportProbeNonce",
            3000
        )
        # Reproduce the production worker topology, including both its atomic
        # detached-worker guard Job and its named lifetime Job. The internal
        # worker starts the real runner self-test, which compiles and executes
        # the source-bound fixture through the private atomic Job-list launcher.
        $nestedRunnerSelfTestNonce = [Guid]::NewGuid().ToString("N")
        $nestedRunnerSelfTestOut = [IO.Path]::Combine(
            $testRoot,
            "nested-runner.stdout.log"
        )
        $nestedRunnerSelfTestErr = [IO.Path]::Combine(
            $testRoot,
            "nested-runner.stderr.log"
        )
        $nestedRunnerSelfTestEnvironment = Get-WhitelistedWorkerEnvironment
        $nestedRunnerSelfTestEnvironment[
            "LBB_WINDOWS_NESTED_JOB_RUNNER_SELF_TEST_NONCE"
        ] = $nestedRunnerSelfTestNonce
        $nestedRunnerSelfTestProcess = $null
        try {
            $nestedRunnerSelfTestResult = @(Start-DetachedWorkerProcess `
                -Executable (Resolve-SystemWindowsPowerShell) `
                -Arguments (Join-NativeArguments @(
                    "-NoLogo", "-NoProfile", "-NonInteractive", "-File",
                    (Resolve-OrdinaryPath $PSCommandPath $true "Coordinator self-test script"),
                    "-Mode", "SelfTest",
                    "-InternalNestedJobRunnerSelfTestNonce", $nestedRunnerSelfTestNonce
                )) `
                -WorkingDirectory ([IO.Path]::GetFullPath([IO.Path]::Combine($PSScriptRoot, ".."))) `
                -StdoutPath $nestedRunnerSelfTestOut `
                -StderrPath $nestedRunnerSelfTestErr `
                -Environment $nestedRunnerSelfTestEnvironment)
            if ($nestedRunnerSelfTestResult.Count -ne 1 -or
                $nestedRunnerSelfTestResult[0] -isnot
                    [LbbCoordinator.DetachedWorkerProcess]) {
                throw "The nested-Job self-test launcher did not return exactly one worker."
            }
            $nestedRunnerSelfTestProcess = [LbbCoordinator.DetachedWorkerProcess](
                $nestedRunnerSelfTestResult[0]
            )
            $nestedRunnerSelfTestProcess.TransferGuardOwnership()
            if (-not $nestedRunnerSelfTestProcess.GuardOwnershipTransferred) {
                throw "The nested-Job self-test worker did not own its guard Job."
            }
            if (-not $nestedRunnerSelfTestProcess.WaitForExit(300000)) {
                $nestedRunnerSelfTestProcess.Kill()
                throw "The exact guard-plus-lifetime Job runner self-test timed out."
            }
            $nestedRunnerSelfTestExit = [int]$nestedRunnerSelfTestProcess.ExitCode
            $nestedRunnerSelfTestStdout = [IO.File]::ReadAllText(
                $nestedRunnerSelfTestOut,
                $script:Utf8NoBom
            ).TrimEnd([char[]]"`r`n")
            $nestedRunnerSelfTestStderr = [IO.File]::ReadAllText(
                $nestedRunnerSelfTestErr,
                $script:Utf8NoBom
            )
            if ($nestedRunnerSelfTestExit -ne 0 -or
                $nestedRunnerSelfTestStdout -cne
                    "Nested guard/lifetime Job runner self-test passed." -or
                -not [String]::IsNullOrEmpty($nestedRunnerSelfTestStderr)) {
                $nestedStdoutDetail = ConvertTo-BoundedSelfTestFailureDetail `
                    $nestedRunnerSelfTestStdout
                $nestedStderrDetail = ConvertTo-BoundedSelfTestFailureDetail `
                    $nestedRunnerSelfTestStderr
                throw "The exact guard-plus-lifetime Job runner self-test failed. (workerExit=$nestedRunnerSelfTestExit; stdout=$nestedStdoutDetail; stderr=$nestedStderrDetail)."
            }
            [LbbCoordinator.WorkerLifetimeJob]::WaitForNameAbsenceForSelfTest(
                "Local\LBBWindowsAcceptanceNestedRunnerSelfTest-$nestedRunnerSelfTestNonce",
                3000
            )
            $nestedRunnerBoundaryFiles = Get-CoordinatorFiles $testRoot
            foreach ($unexpectedCandidateBoundary in @(
                $nestedRunnerBoundaryFiles.Start,
                $nestedRunnerBoundaryFiles.Worker,
                $nestedRunnerBoundaryFiles.Ownership,
                $nestedRunnerBoundaryFiles.Intent,
                $nestedRunnerBoundaryFiles.Runner,
                $nestedRunnerBoundaryFiles.Watcher,
                $nestedRunnerBoundaryFiles.Handoff,
                $nestedRunnerBoundaryFiles.Final,
                $nestedRunnerBoundaryFiles.Failure
            )) {
                if ([IO.File]::Exists($unexpectedCandidateBoundary)) {
                    throw "The nested-Job runner self-test crossed a candidate execution boundary."
                }
            }
        }
        finally {
            if ($null -ne $nestedRunnerSelfTestProcess) {
                $nestedRunnerSelfTestProcess.Dispose()
            }
            Remove-SelfTestStreamFiles `
                $testRoot `
                @($nestedRunnerSelfTestOut, $nestedRunnerSelfTestErr)
        }
        $cleanJobName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeJobCleanSelfTest-" +
            [Guid]::NewGuid().ToString("N")
        $cleanMutexName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeCleanSelfTest-" +
            [Guid]::NewGuid().ToString("N")
        if ($cleanJobName -ceq $script:WorkerLifetimeJobName -or
            $cleanMutexName -ceq "Local\LBBWindowsAcceptanceCoordinator") {
            throw "The clean-start self-test selected a production coordinator name."
        }
        $cleanMutex = [Threading.Mutex]::new($false, $cleanMutexName)
        $cleanMutexHeld = $false
        try {
            $cleanMutexHeld = $cleanMutex.WaitOne(0)
            if (-not $cleanMutexHeld) {
                throw "The clean-start self-test could not acquire its isolated admission mutex."
            }
            $selfTestCleanLifetimeJob = New-WorkerLifetimeJob `
                -AllowChildBreakaway `
                -Name $cleanJobName `
                -RecoverExisting `
                -RecoveryTimeoutMilliseconds 2000
            if (-not $selfTestCleanLifetimeJob.IsBound -or
                $selfTestCleanLifetimeJob.RecoveredExistingJob) {
                throw "The clean-start self-test did not bind a fresh named Job."
            }
        }
        finally {
            if ($cleanMutexHeld) { $cleanMutex.ReleaseMutex() }
            $cleanMutex.Dispose()
        }
        $quoted = @(
            @{ Value = ""; Expected = '""' },
            @{ Value = "plain"; Expected = "plain" },
            @{ Value = "two words"; Expected = '"two words"' },
            @{ Value = 'C:\space path\'; Expected = '"C:\space path\\"' },
            @{ Value = 'a"b'; Expected = '"a\"b"' }
        )
        foreach ($case in $quoted) {
            if ((ConvertTo-NativeArgument $case.Value) -cne $case.Expected) {
                throw "Native argument quoting self-test failed."
            }
        }
        $once = [IO.Path]::Combine($testRoot, "once.json")
        Write-CreateOnceJson $once ([ordered]@{ schemaVersion = 1; kind = "self-test" })
        $duplicateRefused = $false
        try { Write-CreateOnceJson $once ([ordered]@{ schemaVersion = 1; kind = "duplicate" }) }
        catch { $duplicateRefused = $true }
        if (-not $duplicateRefused) { throw "Create-once state self-test failed." }
        if (@([IO.Directory]::EnumerateFiles($testRoot, "*.tmp")).Count -ne 0) {
            throw "Atomic create-once state self-test left a temporary record."
        }

        $secretEnvironmentName = "LBB_COORDINATOR_SELF_TEST_SECRET"
        $originalSecretEnvironment = [Environment]::GetEnvironmentVariable($secretEnvironmentName, "Process")
        $detachedChild = [IO.Path]::Combine($testRoot, "detached-child.ps1")
        $detachedChildSource = @'
$secret = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_SELF_TEST_SECRET", "Process")
$probe = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_SELF_TEST_PROBE", "Process")
$mutexName = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_SELF_TEST_MUTEX", "Process")
$mutex = [Threading.Mutex]::new($false, $mutexName)
try {
    $mutexAcquired = $mutex.WaitOne(0)
    if ($mutexAcquired) { $mutex.ReleaseMutex() }
}
finally { $mutex.Dispose() }
[Console]::Out.Write("secretPresent=" + (-not [String]::IsNullOrEmpty($secret)).ToString().ToLowerInvariant() + ";probe=" + $probe + ";mutexAcquired=" + $mutexAcquired.ToString().ToLowerInvariant())
'@
        [IO.File]::WriteAllText($detachedChild, $detachedChildSource, $script:Utf8NoBom)
        [Environment]::SetEnvironmentVariable($secretEnvironmentName, "must-not-cross", "Process")
        $selfTestMutex = $null
        $selfTestMutexHeld = $false
        try {
            $detachedEnvironment = Get-WhitelistedWorkerEnvironment
            if ($detachedEnvironment.Contains($secretEnvironmentName)) {
                throw "The detached worker allowlist retained an unlisted environment value."
            }
            $detachedEnvironment["LBB_COORDINATOR_SELF_TEST_PROBE"] = "present"
            $selfTestMutexName = "Local\LBBWindowsAcceptanceCoordinatorSelfTest-" + [Guid]::NewGuid().ToString("N")
            $selfTestMutex = [Threading.Mutex]::new($false, $selfTestMutexName)
            $selfTestMutexHeld = $selfTestMutex.WaitOne(0)
            if (-not $selfTestMutexHeld) {
                throw "The cross-process coordinator mutex self-test could not acquire its owner."
            }
            $detachedEnvironment["LBB_COORDINATOR_SELF_TEST_MUTEX"] = $selfTestMutexName
            $systemPowerShell = Resolve-SystemWindowsPowerShell
            $detachedProcessResult = @(Start-DetachedWorkerProcess `
                -Executable $systemPowerShell `
                -Arguments (Join-NativeArguments @(
                    "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $detachedChild
                )) `
                -WorkingDirectory $testRoot `
                -StdoutPath ([IO.Path]::Combine($testRoot, "detached.stdout.log")) `
                -StderrPath ([IO.Path]::Combine($testRoot, "detached.stderr.log")) `
                -Environment $detachedEnvironment)
            if (-not [LbbCoordinator.NativeDetachedWorkerLauncher]::CurrentProcessIsInJob()) {
                throw "The detached-worker self-test parent is not inside its breakaway-enabled outer Job."
            }
            if ($detachedProcessResult.Count -ne 1 -or
                $detachedProcessResult[0] -isnot [LbbCoordinator.DetachedWorkerProcess]) {
                throw "The detached worker launcher did not return exactly one process."
            }
            $detachedProcess = [LbbCoordinator.DetachedWorkerProcess]$detachedProcessResult[0]
            $detachedExitCode = $null
            try {
                if (-not $detachedProcess.WaitForExit(30000)) {
                    $detachedProcess.Kill()
                    throw "The detached worker environment self-test timed out."
                }
                $detachedExitCode = [int]$detachedProcess.ExitCode
            }
            finally { $detachedProcess.Dispose() }
            $detachedOut = [IO.File]::ReadAllText(
                [IO.Path]::Combine($testRoot, "detached.stdout.log"),
                $script:Utf8NoBom
            )
            $detachedErr = [IO.File]::ReadAllText(
                [IO.Path]::Combine($testRoot, "detached.stderr.log"),
                $script:Utf8NoBom
            )
            if ($detachedExitCode -ne 0 -or
                $detachedOut -cne "secretPresent=false;probe=present;mutexAcquired=false" -or
                -not [String]::IsNullOrEmpty($detachedErr)) {
                throw "The detached worker environment was not explicit and clean (exit=$detachedExitCode stderrPresent=$(-not [String]::IsNullOrEmpty($detachedErr)))."
            }
        }
        finally {
            if ($selfTestMutexHeld) {
                try { $selfTestMutex.ReleaseMutex() } catch {}
            }
            if ($null -ne $selfTestMutex) { $selfTestMutex.Dispose() }
            [Environment]::SetEnvironmentVariable(
                $secretEnvironmentName,
                $originalSecretEnvironment,
                "Process"
            )
        }

        $bootstrapEnvironmentNames = @(
            "LBB_COORDINATOR_NONPATTERN_VALUE",
            "COR_ENABLE_PROFILING",
            "PSModulePath",
            "HTTPS_PROXY"
        )
        $bootstrapEnvironmentOriginal = @{}
        foreach ($name in $bootstrapEnvironmentNames) {
            $bootstrapEnvironmentOriginal[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
        }
        $bootstrapEnvironmentChild = [IO.Path]::Combine($testRoot, "bootstrap-environment-child.ps1")
$bootstrapEnvironmentSource = @'
$names = @("LBB_COORDINATOR_NONPATTERN_VALUE", "COR_ENABLE_PROFILING", "PSModulePath", "HTTPS_PROXY")
$forbidden = @("must-not-cross", "1", "must-not-cross", "http://credential.invalid/")
$states = for ($index = 0; $index -lt $names.Count; $index++) {
    ([string][Environment]::GetEnvironmentVariable($names[$index], "Process") -ceq $forbidden[$index]).ToString().ToLowerInvariant()
}
$probe = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_BOOTSTRAP_PROBE", "Process")
[Console]::Out.Write(($states -join ";") + ";probe=" + $probe)
'@
        [IO.File]::WriteAllText($bootstrapEnvironmentChild, $bootstrapEnvironmentSource, $script:Utf8NoBom)
        try {
            [Environment]::SetEnvironmentVariable("LBB_COORDINATOR_NONPATTERN_VALUE", "must-not-cross", "Process")
            [Environment]::SetEnvironmentVariable("COR_ENABLE_PROFILING", "1", "Process")
            [Environment]::SetEnvironmentVariable("PSModulePath", "must-not-cross", "Process")
            [Environment]::SetEnvironmentVariable("HTTPS_PROXY", "http://credential.invalid/", "Process")
            $bootstrapProbeInfo = New-ProcessStartInfo $systemPowerShell @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $bootstrapEnvironmentChild
            ) $testRoot -Hidden
            Set-ExactProcessEnvironment $bootstrapProbeInfo (Get-WhitelistedWorkerEnvironment)
            $bootstrapProbeInfo.EnvironmentVariables["LBB_COORDINATOR_BOOTSTRAP_PROBE"] = "present"
            $bootstrapCapture = Start-CapturedProcess `
                $bootstrapProbeInfo `
                ([IO.Path]::Combine($testRoot, "bootstrap-environment.stdout.log")) `
                ([IO.Path]::Combine($testRoot, "bootstrap-environment.stderr.log"))
            $bootstrapExit = Complete-CapturedProcess $bootstrapCapture
            $bootstrapCapture.Process.Dispose()
            $bootstrapOut = [IO.File]::ReadAllText(
                [IO.Path]::Combine($testRoot, "bootstrap-environment.stdout.log"),
                $script:Utf8NoBom
            )
            $bootstrapErr = [IO.File]::ReadAllText(
                [IO.Path]::Combine($testRoot, "bootstrap-environment.stderr.log"),
                $script:Utf8NoBom
            )
            if ($bootstrapExit -ne 0 -or
                $bootstrapOut -cne "false;false;false;false;probe=present" -or
                -not [String]::IsNullOrEmpty($bootstrapErr)) {
                throw "The clean bootstrap environment retained a disallowed inherited value."
            }
        }
        finally {
            foreach ($name in $bootstrapEnvironmentNames) {
                [Environment]::SetEnvironmentVariable(
                    $name,
                    $bootstrapEnvironmentOriginal[$name],
                    "Process"
                )
            }
        }

        $childScript = [IO.Path]::Combine($testRoot, "captured-child.ps1")
        $childSource = @'
$token = [Environment]::GetEnvironmentVariable("LBB_TOKEN", "Process")
$commandLine = [Environment]::CommandLine
[Console]::Out.Write("tokenPresent=" + (-not [String]::IsNullOrEmpty($token)).ToString().ToLowerInvariant() + ";argvContainsToken=" + $commandLine.Contains($token).ToString().ToLowerInvariant() + ";")
[Console]::Out.Write(("o" * 262144))
[Console]::Error.Write("retained-error;")
[Console]::Error.Write(("e" * 262144))
exit 23
'@
        [IO.File]::WriteAllText($childScript, $childSource, $script:Utf8NoBom)
        $systemPowerShell = Resolve-SystemWindowsPowerShell
        $acquisitionFailureOut = [IO.Path]::Combine($testRoot, "capture-acquisition.stdout.log")
        $acquisitionFailureErr = [IO.Path]::Combine($testRoot, "capture-acquisition.stderr.log")
        [IO.Directory]::CreateDirectory($acquisitionFailureErr) | Out-Null
        $acquisitionFailureInfo = New-ProcessStartInfo $systemPowerShell @(
            "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", "exit 0"
        ) $testRoot -Hidden
        $acquisitionFailureRefused = $false
        try {
            $null = Start-CapturedProcess `
                $acquisitionFailureInfo `
                $acquisitionFailureOut `
                $acquisitionFailureErr
        }
        catch { $acquisitionFailureRefused = $true }
        if (-not $acquisitionFailureRefused) {
            throw "Captured-stream partial-acquisition self-test did not fail closed."
        }
        $exclusiveAcquisitionProbe = [IO.File]::Open(
            $acquisitionFailureOut,
            [IO.FileMode]::Open,
            [IO.FileAccess]::ReadWrite,
            [IO.FileShare]::None
        )
        $exclusiveAcquisitionProbe.Dispose()
        [IO.File]::Delete($acquisitionFailureOut)
        [IO.Directory]::Delete($acquisitionFailureErr, $false)
        if ([IO.File]::Exists($acquisitionFailureOut) -or
            [IO.Directory]::Exists($acquisitionFailureErr)) {
            throw "Captured-stream partial-acquisition self-test left a locked path."
        }
        $info = New-ProcessStartInfo $systemPowerShell @(
            "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $childScript
        ) $testRoot -Hidden
        Remove-SensitiveInheritedEnvironment $info
        $token = New-BridgeToken
        $tokenForLeakCheck = $token
        $info.EnvironmentVariables["LBB_TOKEN"] = $token
        $capture = Start-CapturedProcess $info ([IO.Path]::Combine($testRoot, "out.log")) ([IO.Path]::Combine($testRoot, "err.log"))
        $info.EnvironmentVariables.Remove("LBB_TOKEN")
        $token = $null
        $exitCode = Complete-CapturedProcess $capture
        $capture.Process.Dispose()
        $out = [IO.File]::ReadAllText([IO.Path]::Combine($testRoot, "out.log"), $script:Utf8NoBom)
        $err = [IO.File]::ReadAllText([IO.Path]::Combine($testRoot, "err.log"), $script:Utf8NoBom)
        $stdoutPrefixMatched = $out.StartsWith("tokenPresent=true;argvContainsToken=false;")
        $stderrPrefixMatched = $err.StartsWith("retained-error;")
        if ($exitCode -ne 23 -or $out.Length -lt 262144 -or $err.Length -lt 262144 -or
            -not $stdoutPrefixMatched -or -not $stderrPrefixMatched) {
            throw "Concurrent captured-stream self-test failed (exit=$exitCode stdoutBytes=$($out.Length) stderrBytes=$($err.Length) stdoutPrefix=$stdoutPrefixMatched stderrPrefix=$stderrPrefixMatched)."
        }
        $combined = $out + $err + ([IO.File]::ReadAllText($once, $script:Utf8NoBom))
        if ($combined.Contains($tokenForLeakCheck)) {
            throw "Coordinator self-test logs retained a token-shaped value."
        }
        $tokenForLeakCheck = $null
        Remove-SelfTestStreamFiles $testRoot @(
            [IO.Path]::Combine($testRoot, "out.log"),
            [IO.Path]::Combine($testRoot, "err.log")
        )

        $ownershipProbePath = [IO.Path]::Combine($testRoot, "coordinator-ownership-probe.exe")
        $ownershipStatePath = [IO.Path]::Combine($testRoot, "coordinator-ownership.state")
        $ownershipProbeSource = @'
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class CoordinatorOwnershipProbe {
  [StructLayout(LayoutKind.Sequential)]
  private struct IO_COUNTERS {
    public ulong ReadOperationCount;
    public ulong WriteOperationCount;
    public ulong OtherOperationCount;
    public ulong ReadTransferCount;
    public ulong WriteTransferCount;
    public ulong OtherTransferCount;
  }

  [StructLayout(LayoutKind.Sequential)]
  private struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
    public long PerProcessUserTimeLimit;
    public long PerJobUserTimeLimit;
    public uint LimitFlags;
    public UIntPtr MinimumWorkingSetSize;
    public UIntPtr MaximumWorkingSetSize;
    public uint ActiveProcessLimit;
    public UIntPtr Affinity;
    public uint PriorityClass;
    public uint SchedulingClass;
  }

  [StructLayout(LayoutKind.Sequential)]
  private struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
    public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
    public IO_COUNTERS IoInfo;
    public UIntPtr ProcessMemoryLimit;
    public UIntPtr JobMemoryLimit;
    public UIntPtr PeakProcessMemoryUsed;
    public UIntPtr PeakJobMemoryUsed;
  }

  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  private static extern IntPtr CreateJobObject(IntPtr attributes, string name);
  [DllImport("kernel32.dll", SetLastError=true)]
  private static extern bool SetInformationJobObject(IntPtr job, int infoClass, IntPtr info, uint length);
  [DllImport("kernel32.dll", SetLastError=true)]
  private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
  [DllImport("kernel32.dll")]
  private static extern IntPtr GetCurrentProcess();
  [DllImport("kernel32.dll", SetLastError=true)]
  private static extern bool CloseHandle(IntPtr handle);

  private static string Quote(string value) {
    return "\"" + value.Replace("\"", "\\\"") + "\"";
  }

  private static Process StartMode(string mode, string statePath) {
    string executable;
    using (Process current = Process.GetCurrentProcess()) {
      executable = current.MainModule.FileName;
    }
    ProcessStartInfo info = new ProcessStartInfo();
    info.FileName = executable;
    info.Arguments = statePath == null ? mode : mode + " " + Quote(statePath);
    info.WorkingDirectory = Path.GetDirectoryName(executable);
    info.UseShellExecute = false;
    info.CreateNoWindow = true;
    info.RedirectStandardInput = true;
    info.RedirectStandardOutput = true;
    info.RedirectStandardError = true;
    Process process = new Process();
    process.StartInfo = info;
    try {
      if (!process.Start()) throw new InvalidOperationException("probe child did not start");
      return process;
    }
    catch {
      process.Dispose();
      throw;
    }
  }

  private static IntPtr BindCurrentProcessToKillOnCloseJob() {
    const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
    IntPtr job = CreateJobObject(IntPtr.Zero, null);
    if (job == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
    bool retained = false;
    try {
      JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
      limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
      int size = Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION));
      IntPtr buffer = Marshal.AllocHGlobal(size);
      try {
        Marshal.StructureToPtr(limits, buffer, false);
        if (!SetInformationJobObject(job, 9, buffer, (uint)size)) {
          throw new Win32Exception(Marshal.GetLastWin32Error());
        }
      }
      finally {
        Marshal.FreeHGlobal(buffer);
      }
      if (!AssignProcessToJobObject(job, GetCurrentProcess())) {
        throw new Win32Exception(Marshal.GetLastWin32Error());
      }
      retained = true;
      return job;
    }
    finally {
      if (!retained && !CloseHandle(job)) {
        throw new Win32Exception(Marshal.GetLastWin32Error());
      }
    }
  }

  public static int Main(string[] args) {
    if (args.Length < 1) return 2;
    if (args[0] == "sleeper" || args[0] == "control") {
      Thread.Sleep(120000);
      return 0;
    }
    if (args[0] == "launcher" && args.Length == 2) {
      using (Process worker = StartMode("worker", args[1])) { }
      return 0;
    }
    if (args[0] == "worker" && args.Length == 2) {
      IntPtr lifetimeJob = BindCurrentProcessToKillOnCloseJob();
      using (Process sleeper = StartMode("sleeper", null)) {
        string payload;
        using (Process current = Process.GetCurrentProcess()) {
          payload = current.Id.ToString(CultureInfo.InvariantCulture) + "|" +
            current.StartTime.ToUniversalTime().Ticks.ToString(CultureInfo.InvariantCulture) + "|" +
            sleeper.Id.ToString(CultureInfo.InvariantCulture) + "|" +
            sleeper.StartTime.ToUniversalTime().Ticks.ToString(CultureInfo.InvariantCulture);
        }
        string temporary = args[1] + "." + Guid.NewGuid().ToString("N") + ".tmp";
        File.WriteAllText(temporary, payload, new UTF8Encoding(false));
        File.Move(temporary, args[1]);
      }
      GC.KeepAlive(lifetimeJob);
      // The PowerShell owner normally terminates this exact probe immediately.
      // A hard bound prevents an interrupted self-test from orphaning a
      // breakaway probe indefinitely outside the parent test Job.
      Thread.Sleep(120000);
      return 4;
    }
    return 3;
  }
}
'@
        $null = Add-Type `
            -TypeDefinition $ownershipProbeSource `
            -Language CSharp `
            -OutputAssembly $ownershipProbePath `
            -OutputType ConsoleApplication
        if (-not [IO.File]::Exists($ownershipProbePath)) {
            throw "The coordinator ownership probe did not compile."
        }

        $controlCapture = $null
        $guardCloseStdoutPath = [IO.Path]::Combine($testRoot, "guard-close.stdout.log")
        $guardCloseStderrPath = [IO.Path]::Combine($testRoot, "guard-close.stderr.log")
        $controlStdoutPath = [IO.Path]::Combine($testRoot, "ownership-control.stdout.log")
        $controlStderrPath = [IO.Path]::Combine($testRoot, "ownership-control.stderr.log")
        $ownershipStdoutPath = [IO.Path]::Combine($testRoot, "ownership-launcher.stdout.log")
        $ownershipStderrPath = [IO.Path]::Combine($testRoot, "ownership-launcher.stderr.log")
        $guardCloseLauncherProcess = $null
        $guardCloseBoundProcess = $null
        $ownershipLauncherProcess = $null
        $boundWorkerProcess = $null
        $boundSleeperProcess = $null
        $boundControlProcess = $null
        $controlStartedAt = $null
        $workerStartedAt = $null
        $sleeperStartedAt = $null
        try {
            $guardCloseLauncherProcess = Start-DetachedWorkerProcess `
                -Executable $ownershipProbePath `
                -Arguments (Join-NativeArguments @("sleeper")) `
                -WorkingDirectory $testRoot `
                -StdoutPath $guardCloseStdoutPath `
                -StderrPath $guardCloseStderrPath `
                -Environment (Get-WhitelistedWorkerEnvironment)
            $guardClosePid = [int]$guardCloseLauncherProcess.Id
            $guardCloseStartedAt = $guardCloseLauncherProcess.StartedAtUtc
            # Retain an independent exact process handle before closing the
            # launcher's last guard-Job handle. Do not transfer guard ownership:
            # Dispose must close the guard and terminate this still-live worker.
            $guardCloseBoundProcess = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $guardClosePid,
                $guardCloseStartedAt
            )
            if ($guardCloseLauncherProcess.GuardOwnershipTransferred -or
                $guardCloseLauncherProcess.HasExited -or
                $guardCloseBoundProcess.HasExited) {
                throw "The pre-transfer guard-close worker was not live under launcher ownership."
            }
            $guardCloseLauncherProcess.Dispose()
            $guardCloseLauncherProcess = $null
            if (-not $guardCloseBoundProcess.WaitForExit(5000) -or
                -not $guardCloseBoundProcess.HasExited -or
                (Test-BoundProcessAlive $guardClosePid $guardCloseStartedAt)) {
                throw "Closing the pre-transfer guard Job did not terminate its exact live worker."
            }
            $guardCloseBoundProcess.Dispose()
            $guardCloseBoundProcess = $null
            foreach ($guardCloseLogPath in @($guardCloseStdoutPath, $guardCloseStderrPath)) {
                $guardCloseExclusiveStream = $null
                try {
                    $guardCloseExclusiveStream = [IO.File]::Open(
                        $guardCloseLogPath,
                        [IO.FileMode]::Open,
                        [IO.FileAccess]::ReadWrite,
                        [IO.FileShare]::None
                    )
                }
                finally {
                    if ($null -ne $guardCloseExclusiveStream) {
                        $guardCloseExclusiveStream.Dispose()
                    }
                }
                [IO.File]::Delete($guardCloseLogPath)
                if ([IO.File]::Exists($guardCloseLogPath)) {
                    throw "The pre-transfer guard-close probe left a worker log path."
                }
            }

            $controlInfo = New-ProcessStartInfo $ownershipProbePath @("control") $testRoot -Hidden
            $controlCapture = Start-CapturedProcess `
                $controlInfo `
                $controlStdoutPath `
                $controlStderrPath
            $controlStartedAt = $controlCapture.StartedAtUtc
            $boundControlProcess = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $controlCapture.Process.Id,
                $controlStartedAt
            )

            $ownershipLauncherProcess = Start-DetachedWorkerProcess `
                -Executable $ownershipProbePath `
                -Arguments (Join-NativeArguments @("worker", $ownershipStatePath)) `
                -WorkingDirectory $testRoot `
                -StdoutPath $ownershipStdoutPath `
                -StderrPath $ownershipStderrPath `
                -Environment (Get-WhitelistedWorkerEnvironment)
            $ownershipDeadline = [DateTime]::UtcNow.AddSeconds(10)
            while (-not [IO.File]::Exists($ownershipStatePath) -and
                [DateTime]::UtcNow -lt $ownershipDeadline) {
                Start-Sleep -Milliseconds 50
            }
            if (-not [IO.File]::Exists($ownershipStatePath)) {
                throw "The detached probe worker did not publish its state."
            }
            $ownershipParts = [IO.File]::ReadAllText($ownershipStatePath, $script:Utf8NoBom).Split('|')
            if ($ownershipParts.Count -ne 4) {
                throw "The coordinator ownership probe state is malformed."
            }
            $workerPid = [int]$ownershipParts[0]
            $workerStartedAt = [DateTime]::new([Int64]$ownershipParts[1], [DateTimeKind]::Utc)
            $sleeperPid = [int]$ownershipParts[2]
            $sleeperStartedAt = [DateTime]::new([Int64]$ownershipParts[3], [DateTimeKind]::Utc)
            if ($workerPid -ne $ownershipLauncherProcess.Id -or
                $workerStartedAt.Ticks -ne $ownershipLauncherProcess.StartedAtUtc.Ticks) {
                throw "The ownership probe state is not bound to the exact guarded worker handle."
            }
            # Retain exact cleanup handles before transferring the temporary
            # guard away from this process. Any assertion after transfer is
            # deliberately fallible; its failure must still leave finally able
            # to terminate the exact worker and its worker-Job-owned sleeper.
            $boundWorkerProcess = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $workerPid,
                $workerStartedAt
            )
            $boundSleeperProcess = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $sleeperPid,
                $sleeperStartedAt
            )
            if ($boundWorkerProcess.HasExited -or
                $boundSleeperProcess.HasExited -or
                $boundControlProcess.HasExited) {
                throw "The transferred-guard fixture was not fully live before ownership transfer."
            }
            $ownershipLauncherProcess.TransferGuardOwnership()
            if (-not $ownershipLauncherProcess.GuardOwnershipTransferred) {
                throw "The ownership probe did not transfer its guard Job to the worker."
            }
            $ownershipLauncherProcess.Dispose()
            $ownershipLauncherProcess = $null
            if ($boundWorkerProcess.HasExited -or
                $boundSleeperProcess.HasExited -or
                $boundControlProcess.HasExited -or
                -not (Test-BoundProcessAlive $workerPid $workerStartedAt) -or
                -not (Test-BoundProcessAlive $sleeperPid $sleeperStartedAt) -or
                -not (Test-BoundProcessAlive $controlCapture.Process.Id $controlStartedAt)) {
                throw "The launcher-exit durability probe did not retain only its intended processes."
            }
            if ($boundWorkerProcess.WaitForExit(250) -or
                $boundWorkerProcess.HasExited -or
                $boundSleeperProcess.HasExited -or
                $boundControlProcess.HasExited) {
                throw "The transferred worker did not survive a bounded launcher-disposal dwell."
            }
            $boundWorkerProcess.Kill()
            if (-not $boundWorkerProcess.WaitForExit(5000)) {
                throw "The exact probe worker did not terminate."
            }
            if (-not $boundSleeperProcess.WaitForExit(10000) -or
                -not $boundSleeperProcess.HasExited -or
                (Test-BoundProcessAlive $sleeperPid $sleeperStartedAt)) {
                throw "KILL_ON_JOB_CLOSE did not terminate the worker-owned descendant."
            }
            if ($boundControlProcess.HasExited -or
                -not (Test-BoundProcessAlive $controlCapture.Process.Id $controlStartedAt)) {
                throw "Worker termination affected the unrelated control process."
            }
            $null = Complete-SelfTestCapturedProcess `
                $controlCapture $testRoot $controlStdoutPath $controlStderrPath -Terminate
            $controlCapture = $null
            if (-not $boundControlProcess.WaitForExit(5000) -or
                -not $boundControlProcess.HasExited) {
                throw "The exact unrelated control process did not terminate during self-test cleanup."
            }
            Remove-SelfTestStreamFiles `
                $testRoot `
                @($ownershipStdoutPath, $ownershipStderrPath)
        }
        finally {
            if ($null -ne $guardCloseLauncherProcess) {
                try {
                    if (-not $guardCloseLauncherProcess.HasExited) {
                        $guardCloseLauncherProcess.Kill()
                    }
                    $null = $guardCloseLauncherProcess.WaitForExit(5000)
                }
                catch {}
                $guardCloseLauncherProcess.Dispose()
            }
            if ($null -ne $guardCloseBoundProcess) {
                try {
                    if (-not $guardCloseBoundProcess.HasExited) {
                        $guardCloseBoundProcess.Kill()
                        $null = $guardCloseBoundProcess.WaitForExit(5000)
                    }
                }
                catch {}
                $guardCloseBoundProcess.Dispose()
            }
            if ($null -ne $ownershipLauncherProcess) {
                try {
                    if (-not $ownershipLauncherProcess.HasExited) {
                        $ownershipLauncherProcess.Kill()
                    }
                    $null = $ownershipLauncherProcess.WaitForExit(5000)
                }
                catch {}
                $ownershipLauncherProcess.Dispose()
            }
            if ($null -ne $boundWorkerProcess) {
                try {
                    if (-not $boundWorkerProcess.HasExited) {
                        $boundWorkerProcess.Kill()
                        $null = $boundWorkerProcess.WaitForExit(5000)
                    }
                }
                catch {}
                $boundWorkerProcess.Dispose()
            }
            if ($null -ne $boundSleeperProcess) {
                try {
                    if (-not $boundSleeperProcess.HasExited) {
                        $boundSleeperProcess.Kill()
                        $null = $boundSleeperProcess.WaitForExit(5000)
                    }
                }
                catch {}
                $boundSleeperProcess.Dispose()
            }
            if ($null -ne $boundControlProcess) {
                try {
                    if (-not $boundControlProcess.HasExited) {
                        $boundControlProcess.Kill()
                        $null = $boundControlProcess.WaitForExit(5000)
                    }
                }
                catch {}
                $boundControlProcess.Dispose()
            }
            if ($null -ne $controlCapture) {
                try {
                    $null = Complete-SelfTestCapturedProcess `
                        $controlCapture $testRoot $controlStdoutPath $controlStderrPath -Terminate
                }
                catch {}
            }
            try {
                Remove-SelfTestStreamFiles `
                    $testRoot `
                    @($ownershipStdoutPath, $ownershipStderrPath)
            }
            catch {}
        }

        $namedRecoverySourcePath = [IO.Path]::Combine($testRoot, "named-job-recovery-support.cs")
        $namedRecoveryOwnerPath = [IO.Path]::Combine($testRoot, "named-job-recovery-owner.ps1")
        $namedRecoveryStatePath = [IO.Path]::Combine($testRoot, "named-job-recovery.state")
        [IO.File]::WriteAllText(
            $namedRecoverySourcePath,
            (Get-WorkerLifetimeSupportSource),
            $script:Utf8NoBom
        )
        $namedRecoveryOwnerSource = @'
param(
    [Parameter(Mandatory = $true)][string]$SupportSourcePath,
    [Parameter(Mandatory = $true)][string]$JobName,
    [Parameter(Mandatory = $true)][string]$StatePath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Add-Type -TypeDefinition ([IO.File]::ReadAllText($SupportSourcePath))
$job = [LbbCoordinator.WorkerLifetimeJob]::new($false, $JobName, $false, 10000)
$selfProcess = [Diagnostics.Process]::GetCurrentProcess()
try { $selfPath = $selfProcess.MainModule.FileName }
finally { $selfProcess.Dispose() }
$childInfo = [Diagnostics.ProcessStartInfo]::new()
$childInfo.FileName = $selfPath
$childInfo.Arguments = '-NoLogo -NoProfile -NonInteractive -Command "Start-Sleep -Seconds 120"'
$childInfo.WorkingDirectory = [IO.Path]::GetDirectoryName($selfPath)
$childInfo.UseShellExecute = $false
$childInfo.CreateNoWindow = $true
$child = [Diagnostics.Process]::new()
$child.StartInfo = $childInfo
try {
    if (-not $child.Start()) { throw "The named-Job recovery descendant did not start." }
    $owner = [Diagnostics.Process]::GetCurrentProcess()
    try {
        $payload = @(
            $owner.Id,
            $owner.StartTime.ToUniversalTime().Ticks,
            $child.Id,
            $child.StartTime.ToUniversalTime().Ticks
        ) -join '|'
    }
    finally { $owner.Dispose() }
    $temporary = $StatePath + "." + [Guid]::NewGuid().ToString("N") + ".tmp"
    [IO.File]::WriteAllText($temporary, $payload, [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporary, $StatePath)
    while ($true) { Start-Sleep -Seconds 1 }
}
finally {
    $child.Dispose()
    [GC]::KeepAlive($job)
}
'@
        [IO.File]::WriteAllText(
            $namedRecoveryOwnerPath,
            $namedRecoveryOwnerSource,
            $script:Utf8NoBom
        )
        $namedHandleHolderPath = [IO.Path]::Combine($testRoot, "named-job-handle-holder.ps1")
        $namedHandleHolderSource = @'
param(
    [Parameter(Mandatory = $true)][string]$JobName,
    [Parameter(Mandatory = $true)][string]$StatePath,
    [Parameter(Mandatory = $true)][int]$HoldMilliseconds
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$nativeSource = @"
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

namespace LbbCoordinatorSelfTest {
  public static class NamedJobHandleLease {
    private const uint JOB_OBJECT_QUERY = 0x0004;
    private const int JobObjectBasicAccountingInformation = 1;

    [StructLayout(LayoutKind.Sequential)]
    private struct JOBOBJECT_BASIC_ACCOUNTING_INFORMATION {
      public long TotalUserTime;
      public long TotalKernelTime;
      public long ThisPeriodTotalUserTime;
      public long ThisPeriodTotalKernelTime;
      public uint TotalPageFaultCount;
      public uint TotalProcesses;
      public uint ActiveProcesses;
      public uint TotalTerminatedProcesses;
    }

    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern IntPtr OpenJobObject(
      uint desiredAccess,
      [MarshalAs(UnmanagedType.Bool)] bool inheritHandle,
      string name);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool QueryInformationJobObject(
      IntPtr job,
      int infoClass,
      out JOBOBJECT_BASIC_ACCOUNTING_INFORMATION information,
      uint length,
      IntPtr returnLength);
    [DllImport("kernel32.dll", SetLastError=true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseHandle(IntPtr handle);

    public static IntPtr Open(string name) {
      IntPtr handle = OpenJobObject(JOB_OBJECT_QUERY, false, name);
      if (handle == IntPtr.Zero) {
        throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not open the self-test named Job");
      }
      return handle;
    }

    public static void WaitForEmpty(IntPtr handle, int timeoutMilliseconds) {
      Stopwatch deadline = Stopwatch.StartNew();
      while (true) {
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION accounting;
        if (!QueryInformationJobObject(
          handle,
          JobObjectBasicAccountingInformation,
          out accounting,
          (uint)Marshal.SizeOf(typeof(JOBOBJECT_BASIC_ACCOUNTING_INFORMATION)),
          IntPtr.Zero)) {
          throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not query the self-test named Job");
        }
        if (accounting.ActiveProcesses == 0) return;
        if (deadline.ElapsedMilliseconds >= timeoutMilliseconds) {
          throw new TimeoutException("The self-test named Job did not become empty");
        }
        Thread.Sleep(5);
      }
    }

    public static void CloseOnce(IntPtr handle) {
      if (!CloseHandle(handle)) {
        throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not close the self-test named Job handle");
      }
    }
  }
}
"@
Add-Type -TypeDefinition $nativeSource
$handle = [LbbCoordinatorSelfTest.NamedJobHandleLease]::Open($JobName)
try {
    $current = [Diagnostics.Process]::GetCurrentProcess()
    try {
        $payload = @($current.Id, $current.StartTime.ToUniversalTime().Ticks) -join '|'
    }
    finally { $current.Dispose() }
    $temporary = $StatePath + "." + [Guid]::NewGuid().ToString("N") + ".tmp"
    [IO.File]::WriteAllText($temporary, $payload, [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporary, $StatePath)
    [LbbCoordinatorSelfTest.NamedJobHandleLease]::WaitForEmpty($handle, 30000)
    [IO.File]::WriteAllText(
        $StatePath + ".empty",
        "active-processes-zero",
        [Text.UTF8Encoding]::new($false)
    )
    Start-Sleep -Milliseconds $HoldMilliseconds
}
finally {
    [LbbCoordinatorSelfTest.NamedJobHandleLease]::CloseOnce($handle)
}
'@
        [IO.File]::WriteAllText(
            $namedHandleHolderPath,
            $namedHandleHolderSource,
            $script:Utf8NoBom
        )
        $namedRaceOwnerPath = [IO.Path]::Combine($testRoot, "named-job-race-owner.ps1")
        $namedRaceOwnerSource = @'
param(
    [Parameter(Mandatory = $true)][string]$SupportSourcePath,
    [Parameter(Mandatory = $true)][string]$JobName,
    [Parameter(Mandatory = $true)][string]$TriggerPath,
    [Parameter(Mandatory = $true)][string]$StatePath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$triggerDeadline = [Diagnostics.Stopwatch]::StartNew()
while (-not [IO.File]::Exists($TriggerPath) -and $triggerDeadline.ElapsedMilliseconds -lt 30000) {
    Start-Sleep -Milliseconds 5
}
if (-not [IO.File]::Exists($TriggerPath)) {
    throw "The create-race owner did not receive its self-test trigger."
}
Add-Type -TypeDefinition ([IO.File]::ReadAllText($SupportSourcePath))
$job = [LbbCoordinator.WorkerLifetimeJob]::new($false, $JobName, $false, 10000)
try {
    $current = [Diagnostics.Process]::GetCurrentProcess()
    try {
        $payload = @($current.Id, $current.StartTime.ToUniversalTime().Ticks) -join '|'
    }
    finally { $current.Dispose() }
    $temporary = $StatePath + "." + [Guid]::NewGuid().ToString("N") + ".tmp"
    [IO.File]::WriteAllText($temporary, $payload, [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporary, $StatePath)
    while ($true) { Start-Sleep -Seconds 1 }
}
finally {
    [GC]::KeepAlive($job)
}
'@
        [IO.File]::WriteAllText(
            $namedRaceOwnerPath,
            $namedRaceOwnerSource,
            $script:Utf8NoBom
        )
        $namedRecoveryJobName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeJobSelfTest-" +
            [Guid]::NewGuid().ToString("N")
        if ($namedRecoveryJobName -ceq $script:WorkerLifetimeJobName) {
            throw "The named-Job recovery self-test selected the production Job name."
        }
        $namedRecoveryOwnerCapture = $null
        $namedRecoveryBoundOwner = $null
        $namedRecoveryBoundChild = $null
        $namedRecoveryOut = [IO.Path]::Combine($testRoot, "named-job-recovery.stdout.log")
        $namedRecoveryErr = [IO.Path]::Combine($testRoot, "named-job-recovery.stderr.log")
        $namedRecoveryMutex = $null
        $namedRecoveryMutexHeld = $false
        try {
            $namedRecoveryOwnerInfo = New-ProcessStartInfo $systemPowerShell @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $namedRecoveryOwnerPath,
                "-SupportSourcePath", $namedRecoverySourcePath,
                "-JobName", $namedRecoveryJobName,
                "-StatePath", $namedRecoveryStatePath
            ) $testRoot -Hidden
            $namedRecoveryOwnerCapture = Start-CapturedProcess `
                $namedRecoveryOwnerInfo `
                $namedRecoveryOut `
                $namedRecoveryErr
            $namedRecoveryDeadline = [DateTime]::UtcNow.AddSeconds(30)
            while (-not [IO.File]::Exists($namedRecoveryStatePath) -and
                -not $namedRecoveryOwnerCapture.Process.HasExited -and
                [DateTime]::UtcNow -lt $namedRecoveryDeadline) {
                Start-Sleep -Milliseconds 50
            }
            if (-not [IO.File]::Exists($namedRecoveryStatePath)) {
                throw "The named-Job recovery owner did not publish its process state."
            }
            $namedRecoveryParts = [IO.File]::ReadAllText(
                $namedRecoveryStatePath,
                $script:Utf8NoBom
            ).Split('|')
            if ($namedRecoveryParts.Count -ne 4) {
                throw "The named-Job recovery process state is malformed."
            }
            $namedRecoveryOwnerPid = [int]$namedRecoveryParts[0]
            $namedRecoveryOwnerStartedAt = [DateTime]::new(
                [Int64]$namedRecoveryParts[1],
                [DateTimeKind]::Utc
            )
            $namedRecoveryChildPid = [int]$namedRecoveryParts[2]
            $namedRecoveryChildStartedAt = [DateTime]::new(
                [Int64]$namedRecoveryParts[3],
                [DateTimeKind]::Utc
            )
            if ($namedRecoveryOwnerPid -ne $namedRecoveryOwnerCapture.Process.Id -or
                $namedRecoveryOwnerStartedAt.Ticks -ne $namedRecoveryOwnerCapture.StartedAtUtc.Ticks) {
                throw "The named-Job recovery owner state is not bound to its exact captured process."
            }
            $namedRecoveryBoundOwner = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $namedRecoveryOwnerPid,
                $namedRecoveryOwnerStartedAt
            )
            $namedRecoveryBoundChild = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $namedRecoveryChildPid,
                $namedRecoveryChildStartedAt
            )
            if ($namedRecoveryBoundOwner.HasExited -or
                $namedRecoveryBoundChild.HasExited) {
                throw "The exact prior named-Job owner tree was not live before recovery."
            }
            $namedRecoveryMutexName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeRecoverySelfTest-" +
                [Guid]::NewGuid().ToString("N")
            $namedRecoveryMutex = [Threading.Mutex]::new($false, $namedRecoveryMutexName)
            $namedRecoveryMutexHeld = $namedRecoveryMutex.WaitOne(0)
            if (-not $namedRecoveryMutexHeld) {
                throw "The named-Job recovery self-test could not acquire its isolated admission mutex."
            }
            $selfTestRecoveredLifetimeJob = New-WorkerLifetimeJob `
                -Name $namedRecoveryJobName `
                -RecoverExisting `
                -RecoveryTimeoutMilliseconds 10000
            if (-not $selfTestRecoveredLifetimeJob.IsBound -or
                -not $selfTestRecoveredLifetimeJob.RecoveredExistingJob) {
                throw "The stable named Job did not report recovery of its prior owner tree."
            }
            if (-not $namedRecoveryBoundOwner.WaitForExit(5000) -or
                -not $namedRecoveryBoundChild.WaitForExit(5000) -or
                -not $namedRecoveryBoundOwner.HasExited -or
                -not $namedRecoveryBoundChild.HasExited -or
                (Test-BoundProcessAlive $namedRecoveryOwnerPid $namedRecoveryOwnerStartedAt) -or
                (Test-BoundProcessAlive $namedRecoveryChildPid $namedRecoveryChildStartedAt)) {
                throw "Named-Job recovery returned before the prior tree reached ACTIVE_PROCESS_ZERO."
            }
            $null = Complete-SelfTestCapturedProcess `
                $namedRecoveryOwnerCapture $testRoot $namedRecoveryOut $namedRecoveryErr
            $namedRecoveryOwnerCapture = $null
        }
        finally {
            if ($null -ne $namedRecoveryOwnerCapture) {
                try {
                    $null = Complete-SelfTestCapturedProcess `
                        $namedRecoveryOwnerCapture $testRoot $namedRecoveryOut $namedRecoveryErr -Terminate
                }
                catch {}
            }
            if ($null -ne $namedRecoveryBoundOwner) {
                try {
                    if (-not $namedRecoveryBoundOwner.HasExited) {
                        $namedRecoveryBoundOwner.Kill()
                        $null = $namedRecoveryBoundOwner.WaitForExit(5000)
                    }
                }
                catch {}
                $namedRecoveryBoundOwner.Dispose()
            }
            if ($null -ne $namedRecoveryBoundChild) {
                try {
                    if (-not $namedRecoveryBoundChild.HasExited) {
                        $namedRecoveryBoundChild.Kill()
                        $null = $namedRecoveryBoundChild.WaitForExit(5000)
                    }
                }
                catch {}
                $namedRecoveryBoundChild.Dispose()
            }
            if ($namedRecoveryMutexHeld) {
                try { $namedRecoveryMutex.ReleaseMutex() } catch {}
            }
            if ($null -ne $namedRecoveryMutex) { $namedRecoveryMutex.Dispose() }
        }

        # A non-member process retains a query handle after the exact prior
        # owner tree reaches zero. Recovery must wait for that handle to close
        # and for the name to leave the namespace before one fresh create.
        $delayedJobName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeDelayedHandleSelfTest-" +
            [Guid]::NewGuid().ToString("N")
        $delayedMutexName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeDelayedMutexSelfTest-" +
            [Guid]::NewGuid().ToString("N")
        if ($delayedJobName -ceq $script:WorkerLifetimeJobName -or
            $delayedMutexName -ceq "Local\LBBWindowsAcceptanceCoordinator") {
            throw "The delayed-handle self-test selected a production coordinator name."
        }
        $delayedOwnerStatePath = [IO.Path]::Combine($testRoot, "delayed-owner.state")
        $delayedHolderStatePath = [IO.Path]::Combine($testRoot, "delayed-holder.state")
        $delayedOwnerOut = [IO.Path]::Combine($testRoot, "delayed-owner.stdout.log")
        $delayedOwnerErr = [IO.Path]::Combine($testRoot, "delayed-owner.stderr.log")
        $delayedHolderOut = [IO.Path]::Combine($testRoot, "delayed-holder.stdout.log")
        $delayedHolderErr = [IO.Path]::Combine($testRoot, "delayed-holder.stderr.log")
        $delayedOwnerCapture = $null
        $delayedHolderCapture = $null
        $delayedBoundOwner = $null
        $delayedBoundChild = $null
        $delayedBoundHolder = $null
        $delayedMutex = $null
        $delayedMutexHeld = $false
        try {
            $delayedOwnerInfo = New-ProcessStartInfo $systemPowerShell @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $namedRecoveryOwnerPath,
                "-SupportSourcePath", $namedRecoverySourcePath,
                "-JobName", $delayedJobName,
                "-StatePath", $delayedOwnerStatePath
            ) $testRoot -Hidden
            $delayedOwnerCapture = Start-CapturedProcess `
                $delayedOwnerInfo `
                $delayedOwnerOut `
                $delayedOwnerErr
            Wait-SelfTestStateFile $delayedOwnerStatePath $delayedOwnerCapture 30000
            $delayedOwnerParts = [IO.File]::ReadAllText(
                $delayedOwnerStatePath,
                $script:Utf8NoBom
            ).Split('|')
            if ($delayedOwnerParts.Count -ne 4) {
                throw "The delayed-handle owner state is malformed."
            }
            $delayedOwnerPid = [int]$delayedOwnerParts[0]
            $delayedOwnerStartedAt = [DateTime]::new(
                [Int64]$delayedOwnerParts[1],
                [DateTimeKind]::Utc
            )
            $delayedChildPid = [int]$delayedOwnerParts[2]
            $delayedChildStartedAt = [DateTime]::new(
                [Int64]$delayedOwnerParts[3],
                [DateTimeKind]::Utc
            )
            if ($delayedOwnerPid -ne $delayedOwnerCapture.Process.Id -or
                $delayedOwnerStartedAt.Ticks -ne $delayedOwnerCapture.StartedAtUtc.Ticks) {
                throw "The delayed-handle owner state is not bound to its exact captured process."
            }
            $delayedBoundOwner = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $delayedOwnerPid,
                $delayedOwnerStartedAt
            )
            $delayedBoundChild = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $delayedChildPid,
                $delayedChildStartedAt
            )

            $delayedHolderInfo = New-ProcessStartInfo $systemPowerShell @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $namedHandleHolderPath,
                "-JobName", $delayedJobName,
                "-StatePath", $delayedHolderStatePath,
                "-HoldMilliseconds", "750"
            ) $testRoot -Hidden
            $delayedHolderCapture = Start-CapturedProcess `
                $delayedHolderInfo `
                $delayedHolderOut `
                $delayedHolderErr
            Wait-SelfTestStateFile $delayedHolderStatePath $delayedHolderCapture 30000
            $delayedHolderParts = [IO.File]::ReadAllText(
                $delayedHolderStatePath,
                $script:Utf8NoBom
            ).Split('|')
            if ($delayedHolderParts.Count -ne 2) {
                throw "The delayed-handle holder state is malformed."
            }
            $delayedHolderPid = [int]$delayedHolderParts[0]
            $delayedHolderStartedAt = [DateTime]::new(
                [Int64]$delayedHolderParts[1],
                [DateTimeKind]::Utc
            )
            if ($delayedHolderPid -ne $delayedHolderCapture.Process.Id -or
                $delayedHolderStartedAt.Ticks -ne $delayedHolderCapture.StartedAtUtc.Ticks) {
                throw "The delayed handle is not bound to its exact non-member process."
            }
            $delayedBoundHolder = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $delayedHolderPid,
                $delayedHolderStartedAt
            )
            if ($delayedBoundOwner.HasExited -or
                $delayedBoundChild.HasExited -or
                $delayedBoundHolder.HasExited) {
                throw "The delayed-handle fixture was not fully live before recovery."
            }

            $delayedMutex = [Threading.Mutex]::new($false, $delayedMutexName)
            $delayedMutexHeld = $delayedMutex.WaitOne(0)
            if (-not $delayedMutexHeld) {
                throw "The delayed-handle self-test could not acquire its isolated admission mutex."
            }
            $delayedRecoveryClock = [Diagnostics.Stopwatch]::StartNew()
            $selfTestDelayedHandleLifetimeJob = New-WorkerLifetimeJob `
                -Name $delayedJobName `
                -RecoverExisting `
                -RecoveryTimeoutMilliseconds 5000
            $delayedRecoveryClock.Stop()
            if (-not $selfTestDelayedHandleLifetimeJob.IsBound -or
                -not $selfTestDelayedHandleLifetimeJob.RecoveredExistingJob -or
                -not [IO.File]::Exists($delayedHolderStatePath + ".empty") -or
                $delayedRecoveryClock.ElapsedMilliseconds -lt 650 -or
                $delayedRecoveryClock.ElapsedMilliseconds -ge 5000) {
                throw "Recovery did not wait within bounds for delayed Job namespace disappearance."
            }
            if (-not $delayedBoundOwner.WaitForExit(5000) -or
                -not $delayedBoundChild.WaitForExit(5000) -or
                -not $delayedBoundHolder.WaitForExit(5000) -or
                -not $delayedBoundOwner.HasExited -or
                -not $delayedBoundChild.HasExited -or
                -not $delayedBoundHolder.HasExited) {
                throw "Delayed-handle recovery did not preserve exact process lifetime behavior."
            }
            $delayedHolderExit = Complete-SelfTestCapturedProcess `
                $delayedHolderCapture $testRoot $delayedHolderOut $delayedHolderErr
            $delayedHolderCapture = $null
            if ($delayedHolderExit -ne 0) {
                throw "The non-member delayed Job handle process did not exit cleanly."
            }
            $null = Complete-SelfTestCapturedProcess `
                $delayedOwnerCapture $testRoot $delayedOwnerOut $delayedOwnerErr
            $delayedOwnerCapture = $null
        }
        finally {
            if ($null -ne $delayedOwnerCapture) {
                try {
                    $null = Complete-SelfTestCapturedProcess `
                        $delayedOwnerCapture $testRoot $delayedOwnerOut $delayedOwnerErr -Terminate
                }
                catch {}
            }
            if ($null -ne $delayedHolderCapture) {
                try {
                    $null = Complete-SelfTestCapturedProcess `
                        $delayedHolderCapture $testRoot $delayedHolderOut $delayedHolderErr -Terminate
                }
                catch {}
            }
            foreach ($delayedBoundProcess in @(
                $delayedBoundOwner,
                $delayedBoundChild,
                $delayedBoundHolder
            )) {
                if ($null -ne $delayedBoundProcess) {
                    try {
                        if (-not $delayedBoundProcess.HasExited) {
                            $delayedBoundProcess.Kill()
                            $null = $delayedBoundProcess.WaitForExit(5000)
                        }
                    }
                    catch {}
                    $delayedBoundProcess.Dispose()
                }
            }
            if ($delayedMutexHeld) {
                try { $delayedMutex.ReleaseMutex() } catch {}
            }
            if ($null -ne $delayedMutex) { $delayedMutex.Dispose() }
        }

        # Retaining the non-member handle beyond a short shared deadline must
        # fail closed without binding a Job. After exact fixture cleanup, a
        # query-only absence proof and non-recovery create detect leaked poll
        # or constructor handles.
        $timeoutJobName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeTimeoutSelfTest-" +
            [Guid]::NewGuid().ToString("N")
        $timeoutMutexName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeTimeoutMutexSelfTest-" +
            [Guid]::NewGuid().ToString("N")
        if ($timeoutJobName -ceq $script:WorkerLifetimeJobName -or
            $timeoutMutexName -ceq "Local\LBBWindowsAcceptanceCoordinator") {
            throw "The external-handle timeout self-test selected a production coordinator name."
        }
        $timeoutOwnerStatePath = [IO.Path]::Combine($testRoot, "timeout-owner.state")
        $timeoutHolderStatePath = [IO.Path]::Combine($testRoot, "timeout-holder.state")
        $timeoutOwnerOut = [IO.Path]::Combine($testRoot, "timeout-owner.stdout.log")
        $timeoutOwnerErr = [IO.Path]::Combine($testRoot, "timeout-owner.stderr.log")
        $timeoutHolderOut = [IO.Path]::Combine($testRoot, "timeout-holder.stdout.log")
        $timeoutHolderErr = [IO.Path]::Combine($testRoot, "timeout-holder.stderr.log")
        $timeoutOwnerCapture = $null
        $timeoutHolderCapture = $null
        $timeoutBoundOwner = $null
        $timeoutBoundChild = $null
        $timeoutBoundHolder = $null
        $timeoutMutex = $null
        $timeoutMutexHeld = $false
        try {
            $timeoutOwnerInfo = New-ProcessStartInfo $systemPowerShell @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $namedRecoveryOwnerPath,
                "-SupportSourcePath", $namedRecoverySourcePath,
                "-JobName", $timeoutJobName,
                "-StatePath", $timeoutOwnerStatePath
            ) $testRoot -Hidden
            $timeoutOwnerCapture = Start-CapturedProcess `
                $timeoutOwnerInfo `
                $timeoutOwnerOut `
                $timeoutOwnerErr
            Wait-SelfTestStateFile $timeoutOwnerStatePath $timeoutOwnerCapture 30000
            $timeoutOwnerParts = [IO.File]::ReadAllText(
                $timeoutOwnerStatePath,
                $script:Utf8NoBom
            ).Split('|')
            if ($timeoutOwnerParts.Count -ne 4) {
                throw "The external-handle timeout owner state is malformed."
            }
            $timeoutOwnerPid = [int]$timeoutOwnerParts[0]
            $timeoutOwnerStartedAt = [DateTime]::new(
                [Int64]$timeoutOwnerParts[1],
                [DateTimeKind]::Utc
            )
            $timeoutChildPid = [int]$timeoutOwnerParts[2]
            $timeoutChildStartedAt = [DateTime]::new(
                [Int64]$timeoutOwnerParts[3],
                [DateTimeKind]::Utc
            )
            if ($timeoutOwnerPid -ne $timeoutOwnerCapture.Process.Id -or
                $timeoutOwnerStartedAt.Ticks -ne $timeoutOwnerCapture.StartedAtUtc.Ticks) {
                throw "The external-handle timeout owner is not exact."
            }
            $timeoutBoundOwner = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $timeoutOwnerPid,
                $timeoutOwnerStartedAt
            )
            $timeoutBoundChild = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $timeoutChildPid,
                $timeoutChildStartedAt
            )

            $timeoutHolderInfo = New-ProcessStartInfo $systemPowerShell @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $namedHandleHolderPath,
                "-JobName", $timeoutJobName,
                "-StatePath", $timeoutHolderStatePath,
                "-HoldMilliseconds", "10000"
            ) $testRoot -Hidden
            $timeoutHolderCapture = Start-CapturedProcess `
                $timeoutHolderInfo `
                $timeoutHolderOut `
                $timeoutHolderErr
            Wait-SelfTestStateFile $timeoutHolderStatePath $timeoutHolderCapture 30000
            $timeoutHolderParts = [IO.File]::ReadAllText(
                $timeoutHolderStatePath,
                $script:Utf8NoBom
            ).Split('|')
            if ($timeoutHolderParts.Count -ne 2) {
                throw "The external-handle timeout holder state is malformed."
            }
            $timeoutHolderPid = [int]$timeoutHolderParts[0]
            $timeoutHolderStartedAt = [DateTime]::new(
                [Int64]$timeoutHolderParts[1],
                [DateTimeKind]::Utc
            )
            if ($timeoutHolderPid -ne $timeoutHolderCapture.Process.Id -or
                $timeoutHolderStartedAt.Ticks -ne $timeoutHolderCapture.StartedAtUtc.Ticks) {
                throw "The retained timeout handle is not bound to its exact non-member process."
            }
            $timeoutBoundHolder = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $timeoutHolderPid,
                $timeoutHolderStartedAt
            )
            if ($timeoutBoundOwner.HasExited -or
                $timeoutBoundChild.HasExited -or
                $timeoutBoundHolder.HasExited) {
                throw "The external-handle timeout fixture was not fully live before recovery."
            }

            $timeoutMutex = [Threading.Mutex]::new($false, $timeoutMutexName)
            $timeoutMutexHeld = $timeoutMutex.WaitOne(0)
            if (-not $timeoutMutexHeld) {
                throw "The external-handle timeout self-test could not acquire its isolated admission mutex."
            }
            $timeoutReturnedJob = $null
            $timeoutRefused = $false
            $timeoutHookState = [pscustomobject]@{ Invoked = $false }
            $timeoutBeforeFreshCreate = [Action]{ $timeoutHookState.Invoked = $true }
            $timeoutRecoveryClock = [Diagnostics.Stopwatch]::StartNew()
            try {
                $timeoutReturnedJob = [LbbCoordinator.WorkerLifetimeJob]::CreateForSelfTest(
                    $false,
                    $timeoutJobName,
                    $true,
                    2000,
                    $timeoutBeforeFreshCreate
                )
            }
            catch {
                $timeoutRefused = Test-ExceptionChainTypeName `
                    $_.Exception `
                    "System.TimeoutException"
            }
            $timeoutRecoveryClock.Stop()
            if (-not $timeoutRefused -or
                $null -ne $timeoutReturnedJob -or
                $timeoutHookState.Invoked -or
                -not [IO.File]::Exists($timeoutHolderStatePath + ".empty") -or
                $timeoutRecoveryClock.ElapsedMilliseconds -lt 1800 -or
                $timeoutRecoveryClock.ElapsedMilliseconds -gt 4000) {
                throw "The retained external Job handle did not cause a bounded fail-closed timeout."
            }
            if (-not $timeoutBoundOwner.WaitForExit(5000) -or
                -not $timeoutBoundChild.WaitForExit(5000) -or
                -not $timeoutBoundOwner.HasExited -or
                -not $timeoutBoundChild.HasExited -or
                $timeoutBoundHolder.HasExited) {
                throw "The timeout path did not terminate only the inspected prior Job tree."
            }

            $null = Complete-SelfTestCapturedProcess `
                $timeoutHolderCapture $testRoot $timeoutHolderOut $timeoutHolderErr -Terminate
            $timeoutHolderCapture = $null
            if (-not $timeoutBoundHolder.WaitForExit(5000) -or
                -not $timeoutBoundHolder.HasExited) {
                throw "The exact retained-handle process did not terminate during self-test cleanup."
            }
            $null = Complete-SelfTestCapturedProcess `
                $timeoutOwnerCapture $testRoot $timeoutOwnerOut $timeoutOwnerErr
            $timeoutOwnerCapture = $null

            [LbbCoordinator.WorkerLifetimeJob]::WaitForNameAbsenceForSelfTest(
                $timeoutJobName,
                3000
            )
            $selfTestTimeoutCleanupLifetimeJob = New-WorkerLifetimeJob `
                -Name $timeoutJobName `
                -RecoveryTimeoutMilliseconds 2000
            if (-not $selfTestTimeoutCleanupLifetimeJob.IsBound -or
                $selfTestTimeoutCleanupLifetimeJob.RecoveredExistingJob) {
                throw "The timeout path retained a native Job handle after exact cleanup."
            }
        }
        finally {
            if ($null -ne $timeoutOwnerCapture) {
                try {
                    $null = Complete-SelfTestCapturedProcess `
                        $timeoutOwnerCapture $testRoot $timeoutOwnerOut $timeoutOwnerErr -Terminate
                }
                catch {}
            }
            if ($null -ne $timeoutHolderCapture) {
                try {
                    $null = Complete-SelfTestCapturedProcess `
                        $timeoutHolderCapture $testRoot $timeoutHolderOut $timeoutHolderErr -Terminate
                }
                catch {}
            }
            foreach ($timeoutBoundProcess in @(
                $timeoutBoundOwner,
                $timeoutBoundChild,
                $timeoutBoundHolder
            )) {
                if ($null -ne $timeoutBoundProcess) {
                    try {
                        if (-not $timeoutBoundProcess.HasExited) {
                            $timeoutBoundProcess.Kill()
                            $null = $timeoutBoundProcess.WaitForExit(5000)
                        }
                    }
                    catch {}
                    $timeoutBoundProcess.Dispose()
                }
            }
            if ($timeoutMutexHeld) {
                try { $timeoutMutex.ReleaseMutex() } catch {}
            }
            if ($null -ne $timeoutMutex) { $timeoutMutex.Dispose() }
        }

        # A separate exact process creates the same GUID-scoped Job after the
        # coordinator has observed absence but before its single final create.
        # The raced Job must remain live, the returned existing-object handle
        # must be closed once, and the constructor must fail with error 183.
        $raceJobName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeCreateRaceSelfTest-" +
            [Guid]::NewGuid().ToString("N")
        $raceMutexName = "Local\LBBWindowsAcceptanceCoordinatorLifetimeCreateRaceMutexSelfTest-" +
            [Guid]::NewGuid().ToString("N")
        if ($raceJobName -ceq $script:WorkerLifetimeJobName -or
            $raceMutexName -ceq "Local\LBBWindowsAcceptanceCoordinator") {
            throw "The create-race self-test selected a production coordinator name."
        }
        $raceTriggerPath = [IO.Path]::Combine($testRoot, "create-race.trigger")
        $raceStatePath = [IO.Path]::Combine($testRoot, "create-race.state")
        $raceOut = [IO.Path]::Combine($testRoot, "create-race.stdout.log")
        $raceErr = [IO.Path]::Combine($testRoot, "create-race.stderr.log")
        $raceCapture = $null
        $raceBoundOwner = $null
        $raceMutex = $null
        $raceMutexHeld = $false
        try {
            $raceInfo = New-ProcessStartInfo $systemPowerShell @(
                "-NoLogo", "-NoProfile", "-NonInteractive", "-File", $namedRaceOwnerPath,
                "-SupportSourcePath", $namedRecoverySourcePath,
                "-JobName", $raceJobName,
                "-TriggerPath", $raceTriggerPath,
                "-StatePath", $raceStatePath
            ) $testRoot -Hidden
            $raceCapture = Start-CapturedProcess $raceInfo $raceOut $raceErr
            $raceBoundOwner = [LbbCoordinator.DetachedWorkerProcess]::OpenExact(
                $raceCapture.Process.Id,
                $raceCapture.StartedAtUtc
            )
            if ($raceBoundOwner.HasExited -or $raceCapture.Process.HasExited) {
                throw "The exact create-race owner was not live before the absence observation."
            }

            $raceMutex = [Threading.Mutex]::new($false, $raceMutexName)
            $raceMutexHeld = $raceMutex.WaitOne(0)
            if (-not $raceMutexHeld) {
                throw "The create-race self-test could not acquire its isolated admission mutex."
            }
            $raceHookState = [pscustomobject]@{ Count = 0 }
            $raceBeforeFreshCreate = [Action]{
                $raceHookState.Count++
                $triggerStream = [IO.File]::Open(
                    $raceTriggerPath,
                    [IO.FileMode]::CreateNew,
                    [IO.FileAccess]::Write,
                    [IO.FileShare]::None
                )
                try { $triggerStream.Flush($true) }
                finally { $triggerStream.Dispose() }
                Wait-SelfTestStateFile $raceStatePath $raceCapture 10000
            }
            $raceReturnedJob = $null
            $raceRefused = $false
            try {
                $raceReturnedJob = [LbbCoordinator.WorkerLifetimeJob]::CreateForSelfTest(
                    $false,
                    $raceJobName,
                    $true,
                    15000,
                    $raceBeforeFreshCreate
                )
            }
            catch {
                $raceRefused = Test-Win32ErrorInChain $_.Exception 183
            }
            if (-not $raceRefused -or
                $null -ne $raceReturnedJob -or
                $raceHookState.Count -ne 1) {
                throw "The coordinator did not refuse the uninspected same-name create race."
            }
            $raceParts = [IO.File]::ReadAllText($raceStatePath, $script:Utf8NoBom).Split('|')
            if ($raceParts.Count -ne 2) {
                throw "The create-race owner state is malformed."
            }
            $raceOwnerPid = [int]$raceParts[0]
            $raceOwnerStartedAt = [DateTime]::new(
                [Int64]$raceParts[1],
                [DateTimeKind]::Utc
            )
            if ($raceOwnerPid -ne $raceCapture.Process.Id -or
                $raceOwnerStartedAt.Ticks -ne $raceCapture.StartedAtUtc.Ticks -or
                $raceBoundOwner.HasExited -or
                $raceCapture.Process.HasExited) {
                throw "The refused create race adopted or terminated the exact raced Job owner."
            }

            $null = Complete-SelfTestCapturedProcess `
                $raceCapture $testRoot $raceOut $raceErr -Terminate
            $raceCapture = $null
            if (-not $raceBoundOwner.WaitForExit(5000) -or
                -not $raceBoundOwner.HasExited) {
                throw "The exact create-race owner did not terminate during self-test cleanup."
            }
            [LbbCoordinator.WorkerLifetimeJob]::WaitForNameAbsenceForSelfTest(
                $raceJobName,
                3000
            )
            $selfTestPostRaceLifetimeJob = New-WorkerLifetimeJob `
                -Name $raceJobName `
                -RecoveryTimeoutMilliseconds 2000
            if (-not $selfTestPostRaceLifetimeJob.IsBound -or
                $selfTestPostRaceLifetimeJob.RecoveredExistingJob) {
                throw "The refused create race retained an uninspected native Job handle."
            }
        }
        finally {
            if ($null -ne $raceCapture) {
                try {
                    $null = Complete-SelfTestCapturedProcess `
                        $raceCapture $testRoot $raceOut $raceErr -Terminate
                }
                catch {}
            }
            if ($null -ne $raceBoundOwner) {
                try {
                    if (-not $raceBoundOwner.HasExited) {
                        $raceBoundOwner.Kill()
                        $null = $raceBoundOwner.WaitForExit(5000)
                    }
                }
                catch {}
                $raceBoundOwner.Dispose()
            }
            if ($raceMutexHeld) {
                try { $raceMutex.ReleaseMutex() } catch {}
            }
            if ($null -ne $raceMutex) { $raceMutex.Dispose() }
        }

        $currentProcess = [Diagnostics.Process]::GetCurrentProcess()
        $currentStartedAt = $currentProcess.StartTime.ToUniversalTime()
        if (-not (Test-BoundProcessAlive $currentProcess.Id $currentStartedAt) -or
            (Test-BoundProcessAlive $currentProcess.Id $currentStartedAt.AddTicks(1))) {
            throw "Exact process identity self-test failed."
        }
        $selfTestInputs = New-PrivateChildDirectory $testRoot "follow-inputs" "Follow self-test inputs"
        $inputServer = [IO.Path]::Combine($selfTestInputs, "local-browser-bridge-v0.12.68-windows-x86_64.exe")
        $inputHelper = [IO.Path]::Combine($selfTestInputs, "local-computer-helper-v0.12.68-windows-x86_64.exe")
        $inputManifest = [IO.Path]::Combine($selfTestInputs, "SHA256SUMS.txt")
        $inputBinding = [IO.Path]::Combine($selfTestInputs, "candidate-binding.json")
        [IO.File]::WriteAllText($inputServer, "server-self-test", $script:Utf8NoBom)
        [IO.File]::WriteAllText($inputHelper, "helper-self-test", $script:Utf8NoBom)
        [IO.File]::WriteAllText($inputManifest, "manifest-self-test", $script:Utf8NoBom)
        [IO.File]::WriteAllText($inputBinding, "binding-self-test", $script:Utf8NoBom)
        $selfTestManifestSha256 = Get-FileSha256 $inputManifest
        $selfTestAttemptKey = Get-CandidateAttemptKey `
            -CandidateVersion $script:ProductVersion `
            -ManifestSha256 $selfTestManifestSha256 `
            -Server $inputServer `
            -Helper $inputHelper `
            -Manifest $inputManifest `
            -Binding $inputBinding
        $changedInputAttemptKey = Get-CandidateAttemptKey `
            -CandidateVersion $script:ProductVersion `
            -ManifestSha256 ("f" * 64) `
            -Server "changed-server" `
            -Helper "changed-helper" `
            -Manifest "changed-manifest" `
            -Binding "changed-binding"
        if ($changedInputAttemptKey -cne $selfTestAttemptKey) {
            throw "Candidate input changes minted a second per-version attempt key."
        }
        $prospectiveSelfTestAttemptPath = Get-CandidateAttemptReservationPath `
            -LedgerRoot $selfTestLedgerRoot `
            -AttemptKey $selfTestAttemptKey
        if ([IO.File]::Exists($prospectiveSelfTestAttemptPath)) {
            throw "Resolving a prospective attempt reservation consumed the one-shot boundary."
        }
        $selfTestAttemptPath = $prospectiveSelfTestAttemptPath
        $selfTestCoordinatorInstanceId = [Guid]::NewGuid().ToString("N")

        $newFollowFixture = {
            param([string]$Name, [string]$CoordinatorInstanceId)
            if ([String]::IsNullOrWhiteSpace($CoordinatorInstanceId)) {
                $CoordinatorInstanceId = $selfTestCoordinatorInstanceId
            }
            $root = New-PrivateChildDirectory $selfTestCoordinatorParent $Name "Follow coordinator fixture"
            $evidence = New-PrivateChildDirectory $selfTestEvidenceParent $Name "Follow evidence fixture"
            $source = New-PrivateChildDirectory $root "staged-source" "Follow staged source"
            $scripts = New-PrivateChildDirectory $source "scripts" "Follow staged scripts"
            $tests = New-PrivateChildDirectory $source "tests" "Follow staged tests"
            $fixtures = New-PrivateChildDirectory $tests "fixtures" "Follow staged fixtures"
            $windowsFixtures = New-PrivateChildDirectory $fixtures "windows" "Follow staged Windows fixtures"
            $candidate = New-PrivateChildDirectory $root "candidate" "Follow staged candidate"
            $coordinatorScript = [IO.Path]::Combine($scripts, "run-windows-computer-use-acceptance.ps1")
            $runnerScript = [IO.Path]::Combine($scripts, "test-windows-computer-use.ps1")
            $watcherScript = [IO.Path]::Combine($scripts, "wait-windows-foreground-arm-handoff.ps1")
            $fixtureScript = [IO.Path]::Combine($windowsFixtures, "WindowsComputerUseFixture.ps1")
            $server = Copy-FileToPrivateStage $inputServer ([IO.Path]::Combine($candidate, [IO.Path]::GetFileName($inputServer))) "Follow server"
            $helper = Copy-FileToPrivateStage $inputHelper ([IO.Path]::Combine($candidate, [IO.Path]::GetFileName($inputHelper))) "Follow helper"
            $manifest = Copy-FileToPrivateStage $inputManifest ([IO.Path]::Combine($candidate, "SHA256SUMS.txt")) "Follow manifest"
            $binding = Copy-FileToPrivateStage $inputBinding ([IO.Path]::Combine($candidate, "candidate-binding.json")) "Follow binding"
            [IO.File]::WriteAllText($coordinatorScript, "# coordinator", $script:Utf8NoBom)
            [IO.File]::WriteAllText($runnerScript, "# runner", $script:Utf8NoBom)
            [IO.File]::WriteAllText($watcherScript, "# watcher", $script:Utf8NoBom)
            [IO.File]::WriteAllText($fixtureScript, "# fixture", $script:Utf8NoBom)
            $files = Get-CoordinatorFiles $root
            [IO.File]::WriteAllText($files.WorkerSupport, "worker-support-self-test", $script:Utf8NoBom)
            Write-CreateOnceJson $files.Config ([ordered]@{
                schemaVersion = 1
                version = $script:ProductVersion
                sourceDirectory = $source
                coordinatorScript = $coordinatorScript
                runnerScript = $runnerScript
                watcherScript = $watcherScript
                serverPath = $server
                helperPath = $helper
                checksumManifest = $manifest
                checksumManifestSha256 = $selfTestManifestSha256
                candidateBindingPath = $binding
                fixturePath = $fixtureScript
                evidenceDirectory = $evidence
                coordinatorDirectory = $root
                foregroundArmTimeoutSeconds = 300
                attemptKey = $selfTestAttemptKey
                attemptLedgerPath = $selfTestAttemptPath
                coordinatorInstanceId = $CoordinatorInstanceId
                workerSupportAssembly = $files.WorkerSupport
                workerSupportSha256 = Get-FileSha256 $files.WorkerSupport
                coordinatorScriptSha256 = Get-FileSha256 $coordinatorScript
                runnerScriptSha256 = Get-FileSha256 $runnerScript
                watcherScriptSha256 = Get-FileSha256 $watcherScript
                fixtureSha256 = Get-FileSha256 $fixtureScript
                serverSha256 = Get-FileSha256 $server
                helperSha256 = Get-FileSha256 $helper
                candidateBindingSha256 = Get-FileSha256 $binding
            })
            Write-CreateOnceJson $files.Start ([ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-start-request"
                status = "accepted"
                version = $script:ProductVersion
                coordinatorInstanceId = $CoordinatorInstanceId
                attemptState = "not-started"
                retryOnUnknownOutcome = $false
                recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            })
            return [pscustomobject]@{ Root = $root; Files = $files }
        }
        $newWorkerRecord = {
            param([int]$ProcessId, [DateTime]$StartedAt)
            return [ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-worker-started"
                status = "running"
                workerPid = $ProcessId
                workerStartedAtUtc = ConvertTo-CanonicalUtcString $StartedAt
                attemptState = "not-started"
                retryOnUnknownOutcome = $false
                pathsRecorded = $false
                secretsRecorded = $false
            }
        }
        $newRunnerRecord = {
            param([int]$ProcessId, [DateTime]$StartedAt)
            return [ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-runner-started"
                status = "running"
                runnerPid = $ProcessId
                runnerStartedAtUtc = ConvertTo-CanonicalUtcString $StartedAt
                attemptState = "runner-started-terminal"
                retryOnUnknownOutcome = $false
                pathsRecorded = $false
                secretsRecorded = $false
            }
        }
        $newOwnershipRecord = {
            param([int]$ProcessId, [DateTime]$StartedAt)
            return [ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-worker-ownership-transferred"
                status = "guard-owned-by-worker"
                workerPid = $ProcessId
                workerStartedAtUtc = ConvertTo-CanonicalUtcString $StartedAt
                attemptState = "not-started"
                retryOnUnknownOutcome = $false
                recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            }
        }
        $newIntentRecord = {
            return [ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-runner-launch-intent"
                status = "terminal-attempt-boundary"
                candidateExecutionState = "unknown-after-this-record"
                retryOnUnknownOutcome = $false
                recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            }
        }
        $newFailureRecord = {
            param([string]$AttemptState)
            return [ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-coordinator-terminal"
                status = "failed-closed"
                stage = "self-test-stage"
                attemptState = $AttemptState
                reasonCode = "self-test-reason"
                retryAllowed = $false
                recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            }
        }
        $newAcceptedWatcherRecord = {
            return [ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-watcher-finished"
                status = "accepted"
                exitCode = 0
                stdoutPresent = $true
                stderrPresent = $false
                runnerIdentityMatched = $true
                retryAllowed = $false
                recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            }
        }
        $newHandoffRecord = {
            param([DateTime]$PublishedAt, [DateTime]$ObservedAt, [DateTime]$DeadlineAt)
            return [ordered]@{
                schemaVersion = $script:AutomaticHandoffSchemaVersion
                productVersion = $script:ProductVersion
                kind = "windows-acceptance-automatic-handoff"
                status = "automatic-ready"
                requestId = "0123456789abcdef0123456789abcdef"
                publishedAtUtc = ConvertTo-CanonicalUtcString $PublishedAt
                receivedAtUtc = ConvertTo-CanonicalUtcString ($PublishedAt.AddMilliseconds(500))
                observedAtUtc = ConvertTo-CanonicalUtcString $ObservedAt
                deadlineAtUtc = ConvertTo-CanonicalUtcString $DeadlineAt
                mode = $script:ForegroundGateMode
                operatorActionRequired = $false
                action = "none"
                clickAttemptsObserved = 0
                stableSamplesObserved = 3
                stableSamplesRequired = 3
                nativeSampleSeqlockMatched = $true
                ownerIdentityStable = $true
                focusRootMatched = $true
                fixtureProcessExcluded = $true
                interactiveSessionMatched = $true
                cursorStable = $true
                inputDesktopStable = $true
                globalInputUsed = $false
                focusChangedByRunner = $false
                cursorChangedByRunner = $false
                syntheticInputUsed = $false
                notificationOnly = $true
                acceptedAsAuthority = $false
                runnerIdentityMatched = $true
                requestFresh = $true
                receivedBeforeDeadline = $true
                rawWindowHandlesRecorded = $false
                rawProcessIdentifiersRecorded = $false
                rawCursorCoordinatesRecorded = $false
                pathsRecorded = $false
                secretsRecorded = $false
            }
        }
        $automaticTimeoutSummary = [pscustomobject]([ordered]@{
            passed = $false
            failure = "Timed out waiting for 3 fresh stable external foreground publications."
            failureDetails = [pscustomobject]([ordered]@{
                stage = "wait-stable-external-foreground"
                reasonCode = "foreground-baseline-timeout"
            })
        })
        $laterRunnerFailureSummary = [pscustomobject]([ordered]@{
            passed = $false
            failure = "self-test free-form text must not be projected"
            failureDetails = [pscustomobject]([ordered]@{
                stage = "semantic-suite"
                reasonCode = "acceptance-test-failed"
            })
        })
        $untrustedReasonSummary = [pscustomobject]([ordered]@{
            passed = $false
            failure = "untrusted"
            failureDetails = [pscustomobject]([ordered]@{
                stage = "unknown-self-test-stage"
                reasonCode = "free-form-not-in-the-closed-vocabulary"
            })
        })
        if ((Get-RunnerSummaryFailureReasonCode $automaticTimeoutSummary) -cne
                "runner-foreground-baseline-timeout" -or
            (Get-RunnerSummaryFailureReasonCode $laterRunnerFailureSummary) -cne
                "runner-acceptance-test-failed" -or
            (Get-RunnerSummaryFailureReasonCode $untrustedReasonSummary) -cne
                "runner-summary-failed") {
            throw "Runner summary typed-failure projection self-test failed."
        }
        $originalCoordinatorDirectory = $CoordinatorDirectory
        try {
            $waitingFixture = & $newFollowFixture "follow-waiting"
            Write-CreateOnceJson $waitingFixture.Files.Worker (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $waitingFixture.Files.Ownership (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            $CoordinatorDirectory = $waitingFixture.Root
            $waiting = (Follow-Coordinator | ConvertFrom-Json)
            if ($waiting.status -cne "waiting" -or $waiting.attemptState -cne "not-started" -or
                $waiting.uiActionAllowed -ne $false) {
                throw "Follow waiting-state self-test failed."
            }

            $missingBoundaryFixture = & $newFollowFixture "follow-missing-boundary"
            Write-CreateOnceJson `
                $missingBoundaryFixture.Files.Worker `
                (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson `
                $missingBoundaryFixture.Files.Ownership `
                (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $missingBoundaryFixture.Files.Intent (& $newIntentRecord)
            $CoordinatorDirectory = $missingBoundaryFixture.Root
            $missingBoundaryRefused = $false
            try { $null = Follow-Coordinator }
            catch { $missingBoundaryRefused = $true }
            if (-not $missingBoundaryRefused) {
                throw "Follow accepted a post-boundary record without the persistent reservation."
            }

            $followReservationPath = Reserve-CandidateAttempt `
                -LedgerRoot $selfTestLedgerRoot `
                -AttemptKey $selfTestAttemptKey `
                -CandidateVersion $script:ProductVersion `
                -ManifestSha256 $selfTestManifestSha256 `
                -CoordinatorInstanceId $selfTestCoordinatorInstanceId
            if ($followReservationPath -cne $selfTestAttemptPath) {
                throw "The Follow self-test reservation changed identity."
            }
            $reservationLengthBeforeDuplicate = [IO.FileInfo]::new(
                $selfTestAttemptPath
            ).Length
            $reservationHashBeforeDuplicate = Get-FileSha256 $selfTestAttemptPath
            $duplicateAttemptRefused = $false
            try {
                $null = Reserve-CandidateAttempt `
                    -LedgerRoot $selfTestLedgerRoot `
                    -AttemptKey $selfTestAttemptKey `
                    -CandidateVersion $script:ProductVersion `
                    -ManifestSha256 $selfTestManifestSha256 `
                    -CoordinatorInstanceId $selfTestCoordinatorInstanceId
            }
            catch { $duplicateAttemptRefused = $true }
            if (-not $duplicateAttemptRefused) {
                throw "The persistent per-version attempt reservation self-test accepted a retry."
            }
            if ([IO.FileInfo]::new($selfTestAttemptPath).Length -ne
                    $reservationLengthBeforeDuplicate -or
                (Get-FileSha256 $selfTestAttemptPath) -cne
                    $reservationHashBeforeDuplicate) {
                throw "A refused duplicate changed the persistent attempt reservation bytes."
            }
            $CoordinatorDirectory = $waitingFixture.Root
            $boundaryOnly = (Follow-Coordinator | ConvertFrom-Json)
            if ($boundaryOnly.status -cne "waiting" -or
                $boundaryOnly.attemptState -cne "candidate-execution-unknown" -or
                $boundaryOnly.uiActionAllowed -ne $false) {
                throw "Follow persistent-boundary-only self-test failed."
            }

            $foreignFixture = & $newFollowFixture `
                "follow-foreign-boundary" `
                ([Guid]::NewGuid().ToString("N"))
            Write-CreateOnceJson `
                $foreignFixture.Files.Worker `
                (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson `
                $foreignFixture.Files.Ownership `
                (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            $CoordinatorDirectory = $foreignFixture.Root
            $foreignBoundary = (Follow-Coordinator | ConvertFrom-Json)
            if ($foreignBoundary.status -cne "waiting" -or
                $foreignBoundary.attemptState -cne "not-started" -or
                $foreignBoundary.uiActionAllowed -ne $false) {
                throw "Follow confused another coordinator's reservation with its own."
            }

            $foreignFailureFixture = & $newFollowFixture `
                "follow-foreign-not-started-failure" `
                ([Guid]::NewGuid().ToString("N"))
            Write-CreateOnceJson `
                $foreignFailureFixture.Files.Worker `
                (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson `
                $foreignFailureFixture.Files.Ownership `
                (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson `
                $foreignFailureFixture.Files.Failure `
                (& $newFailureRecord "not-started")
            $CoordinatorDirectory = $foreignFailureFixture.Root
            $foreignFailure = (Follow-Coordinator | ConvertFrom-Json)
            if ($foreignFailure.status -cne "failed-closed" -or
                $foreignFailure.attemptState -cne "not-started" -or
                $foreignFailure.reasonCode -cne "self-test-reason") {
                throw "Follow changed a foreign coordinator's local not-started failure."
            }

            $foreignIntentFixture = & $newFollowFixture `
                "follow-foreign-intent" `
                ([Guid]::NewGuid().ToString("N"))
            Write-CreateOnceJson `
                $foreignIntentFixture.Files.Worker `
                (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson `
                $foreignIntentFixture.Files.Ownership `
                (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $foreignIntentFixture.Files.Intent (& $newIntentRecord)
            $CoordinatorDirectory = $foreignIntentFixture.Root
            $foreignIntentRefused = $false
            try { $null = Follow-Coordinator }
            catch { $foreignIntentRefused = $true }
            if (-not $foreignIntentRefused) {
                throw "Follow accepted local intent owned by a foreign reservation."
            }

            $ownedNotStartedFailureFixture = & $newFollowFixture `
                "follow-owned-not-started-failure"
            Write-CreateOnceJson `
                $ownedNotStartedFailureFixture.Files.Worker `
                (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson `
                $ownedNotStartedFailureFixture.Files.Ownership `
                (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson `
                $ownedNotStartedFailureFixture.Files.Failure `
                (& $newFailureRecord "not-started")
            $CoordinatorDirectory = $ownedNotStartedFailureFixture.Root
            $ownedNotStartedFailureRefused = $false
            try { $null = Follow-Coordinator }
            catch { $ownedNotStartedFailureRefused = $true }
            if (-not $ownedNotStartedFailureRefused) {
                throw "Follow accepted a not-started failure after its owned reservation."
            }

            $postValidationFixture = & $newFollowFixture "follow-post-boundary-validation"
            $postValidationConfig = Read-BoundedJson `
                $postValidationFixture.Files.Config `
                65536 `
                "Post-boundary validation self-test configuration"
            [IO.File]::AppendAllText(
                [string]$postValidationConfig.serverPath,
                "-changed-after-boundary",
                $script:Utf8NoBom
            )
            $postValidationFailed = $false
            try { Assert-ExactConfiguration $postValidationConfig }
            catch {
                $postValidationFailed = $true
                $postValidationState = Get-ObservedAttemptState `
                    $postValidationFixture.Files `
                    $postValidationConfig
                Write-TerminalFailure `
                    $postValidationFixture.Files `
                    "worker-exception" `
                    $postValidationState `
                    "coordinator-exception"
            }
            if (-not $postValidationFailed -or
                -not [IO.File]::Exists($postValidationFixture.Files.Failure)) {
                throw "A post-boundary validation failure did not publish terminal state."
            }
            $postValidationFailure = Read-BoundedJson `
                $postValidationFixture.Files.Failure `
                16384 `
                "Post-boundary terminal failure self-test record"
            Assert-ExactFailureRecord $postValidationFailure
            if ($postValidationFailure.attemptState -cne "candidate-execution-unknown") {
                throw "A post-boundary validation failure was not classified outcome-unknown."
            }

            $handoffFixture = & $newFollowFixture "follow-handoff"
            $handoffPublished = [DateTime]::UtcNow.AddSeconds(-120)
            Write-CreateOnceJson $handoffFixture.Files.Worker (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $handoffFixture.Files.Ownership (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $handoffFixture.Files.Intent (& $newIntentRecord)
            $handoffRunnerRecord = [pscustomobject](& $newRunnerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $handoffFixture.Files.Runner $handoffRunnerRecord
            Write-CreateOnceJson $handoffFixture.Files.Watcher (& $newAcceptedWatcherRecord)
            $watcherHandoff = [pscustomobject](& $newHandoffRecord $handoffPublished ($handoffPublished.AddSeconds(1)) ($handoffPublished.AddSeconds(60)))
            $watcherHandoff.kind = "foreground-baseline-ready-handoff"
            $handoffConfig = Read-BoundedJson `
                $handoffFixture.Files.Config `
                65536 `
                "Automatic handoff conversion self-test configuration"
            $privateHandoff = ConvertTo-ExactPrivateHandoffRecord `
                $watcherHandoff `
                $handoffConfig `
                $handoffRunnerRecord

            $extraSampleWatcherHandoff = [pscustomobject](& $newHandoffRecord `
                    $handoffPublished `
                    ($handoffPublished.AddSeconds(1)) `
                    ($handoffPublished.AddSeconds(60)))
            $extraSampleWatcherHandoff.kind = "foreground-baseline-ready-handoff"
            $extraSampleWatcherHandoff.stableSamplesObserved = 4
            $extraSampleRefused = $false
            try {
                $null = ConvertTo-ExactPrivateHandoffRecord `
                    $extraSampleWatcherHandoff `
                    $handoffConfig `
                    $handoffRunnerRecord
            }
            catch {
                $extraSampleRefused = $_.Exception.Message.IndexOf(
                    "handoff stableSamplesObserved must be an integer from 3 through 3.",
                    [StringComparison]::Ordinal
                ) -ge 0
            }
            if (-not $extraSampleRefused) {
                throw "The coordinator accepted a producer-unreachable fourth stable sample."
            }

            $wrongSessionWatcherHandoff = [pscustomobject](& $newHandoffRecord `
                    $handoffPublished `
                    ($handoffPublished.AddSeconds(1)) `
                    ($handoffPublished.AddSeconds(60)))
            $wrongSessionWatcherHandoff.kind = "foreground-baseline-ready-handoff"
            $wrongSessionWatcherHandoff.interactiveSessionMatched = $false
            $wrongSessionRefused = $false
            try {
                $null = ConvertTo-ExactPrivateHandoffRecord `
                    $wrongSessionWatcherHandoff `
                    $handoffConfig `
                    $handoffRunnerRecord
            }
            catch {
                $wrongSessionRefused = $_.Exception.Message.IndexOf(
                    "handoff interactiveSessionMatched must be True.",
                    [StringComparison]::Ordinal
                ) -ge 0
            }
            if (-not $wrongSessionRefused) {
                throw "The coordinator accepted a foreground proof from the wrong interactive session."
            }

            Write-CreateOnceJson $handoffFixture.Files.Handoff $privateHandoff
            $CoordinatorDirectory = $handoffFixture.Root
            $handoffFirstText = [string](Follow-Coordinator)
            $handoffSecondText = [string](Follow-Coordinator)
            $handoffFirst = $handoffFirstText | ConvertFrom-Json
            if ($handoffFirstText -cne $handoffSecondText -or
                $handoffFirst.status -cne "automatic-ready" -or
                $handoffFirst.mode -cne $script:ForegroundGateMode -or
                $handoffFirst.operatorActionRequired -ne $false -or
                $handoffFirst.action -cne "none" -or
                $handoffFirst.clickAttemptsObserved -ne 0 -or
                $handoffFirst.stableSamplesObserved -ne 3 -or
                $handoffFirst.nativeSampleSeqlockMatched -ne $true -or
                $handoffFirst.ownerIdentityStable -ne $true -or
                $handoffFirst.focusRootMatched -ne $true -or
                $handoffFirst.fixtureProcessExcluded -ne $true -or
                $handoffFirst.interactiveSessionMatched -ne $true -or
                $handoffFirst.cursorStable -ne $true -or
                $handoffFirst.inputDesktopStable -ne $true -or
                $handoffFirst.globalInputUsed -ne $false -or
                $handoffFirst.focusChangedByRunner -ne $false -or
                $handoffFirst.cursorChangedByRunner -ne $false -or
                $handoffFirst.syntheticInputUsed -ne $false -or
                $handoffFirst.receivedBeforeDeadline -ne $true -or
                $handoffFirst.uiActionAllowed -ne $false -or
                $handoffFirst.notificationOnly -ne $true -or
                $handoffFirst.acceptedAsAuthority -ne $false) {
                throw "Follow automatic foreground-baseline handoff self-test failed."
            }

            $deadFixture = & $newFollowFixture "follow-dead-worker"
            Write-CreateOnceJson $deadFixture.Files.Worker (& $newWorkerRecord 2147483646 $currentStartedAt)
            Write-CreateOnceJson $deadFixture.Files.Ownership (& $newOwnershipRecord 2147483646 $currentStartedAt)
            Write-CreateOnceJson $deadFixture.Files.Intent (& $newIntentRecord)
            $CoordinatorDirectory = $deadFixture.Root
            $dead = (Follow-Coordinator | ConvertFrom-Json)
            if ($dead.status -cne "failed-closed" -or
                $dead.reasonCode -cne "bound-worker-not-alive" -or
                $dead.attemptState -cne "candidate-execution-unknown") {
                throw "Follow dead-worker self-test failed."
            }

            $deadRunnerFixture = & $newFollowFixture "follow-dead-runner-no-handoff"
            Write-CreateOnceJson $deadRunnerFixture.Files.Worker (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $deadRunnerFixture.Files.Ownership (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $deadRunnerFixture.Files.Intent (& $newIntentRecord)
            Write-CreateOnceJson $deadRunnerFixture.Files.Runner (& $newRunnerRecord 2147483646 $currentStartedAt)
            $CoordinatorDirectory = $deadRunnerFixture.Root
            $deadRunner = (Follow-Coordinator | ConvertFrom-Json)
            if ($deadRunner.status -cne "waiting" -or
                $deadRunner.phase -cne "runner-finalizing" -or
                $deadRunner.attemptState -cne "runner-started-terminal") {
                throw "Follow dead-runner without handoff self-test failed."
            }

            $finalizingFixture = & $newFollowFixture "follow-runner-finalizing"
            $finalizingPublished = [DateTime]::UtcNow.AddSeconds(-1)
            Write-CreateOnceJson $finalizingFixture.Files.Worker (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $finalizingFixture.Files.Ownership (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $finalizingFixture.Files.Intent (& $newIntentRecord)
            Write-CreateOnceJson $finalizingFixture.Files.Runner (& $newRunnerRecord 2147483646 $currentStartedAt)
            Write-CreateOnceJson $finalizingFixture.Files.Watcher (& $newAcceptedWatcherRecord)
            Write-CreateOnceJson $finalizingFixture.Files.Handoff (& $newHandoffRecord $finalizingPublished ([DateTime]::UtcNow) ($finalizingPublished.AddSeconds(60)))
            $CoordinatorDirectory = $finalizingFixture.Root
            $finalizing = (Follow-Coordinator | ConvertFrom-Json)
            if ($finalizing.status -cne "waiting" -or
                $finalizing.phase -cne "runner-finalizing" -or
                $finalizing.attemptState -cne "runner-started-terminal" -or
                $finalizing.uiActionAllowed -ne $false) {
                throw "Follow runner-finalizing self-test failed."
            }

            $loneFinalFixture = & $newFollowFixture "follow-lone-final"
            Write-CreateOnceJson $loneFinalFixture.Files.Final ([ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-runner-finished"
                status = "completed"
                exitCode = 0
                summaryPresent = $true
                summaryPassed = $true
                evidenceDirectoryPresent = $true
                attemptState = "runner-started-terminal"
                retryAllowed = $false
                finishedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            })
            $CoordinatorDirectory = $loneFinalFixture.Root
            $loneFinalRefused = $false
            try { $null = Follow-Coordinator }
            catch { $loneFinalRefused = $true }
            if (-not $loneFinalRefused) {
                throw "Follow lone-final chain self-test accepted an impossible completion."
            }

            $missingIntentFixture = & $newFollowFixture "follow-missing-intent"
            $missingIntentPublished = [DateTime]::UtcNow.AddSeconds(-1)
            Write-CreateOnceJson $missingIntentFixture.Files.Worker (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $missingIntentFixture.Files.Ownership (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $missingIntentFixture.Files.Runner (& $newRunnerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $missingIntentFixture.Files.Watcher (& $newAcceptedWatcherRecord)
            Write-CreateOnceJson $missingIntentFixture.Files.Handoff (& $newHandoffRecord $missingIntentPublished ([DateTime]::UtcNow) ($missingIntentPublished.AddSeconds(60)))
            $CoordinatorDirectory = $missingIntentFixture.Root
            $missingIntentRefused = $false
            try { $null = Follow-Coordinator }
            catch { $missingIntentRefused = $true }
            if (-not $missingIntentRefused) {
                throw "Follow missing-intent chain self-test accepted an impossible handoff."
            }

            $terminalFixture = & $newFollowFixture "follow-terminal-precedence"
            Write-CreateOnceJson $terminalFixture.Files.Worker (& $newWorkerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $terminalFixture.Files.Ownership (& $newOwnershipRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $terminalFixture.Files.Intent (& $newIntentRecord)
            Write-CreateOnceJson $terminalFixture.Files.Runner (& $newRunnerRecord $currentProcess.Id $currentStartedAt)
            Write-CreateOnceJson $terminalFixture.Files.Failure ([ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-coordinator-terminal"
                status = "failed-closed"
                stage = "self-test-stage"
                attemptState = "runner-started-terminal"
                reasonCode = "self-test-reason"
                retryAllowed = $false
                recordedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            })
            Write-CreateOnceJson $terminalFixture.Files.Final ([ordered]@{
                schemaVersion = 1
                kind = "windows-acceptance-runner-finished"
                status = "completed"
                exitCode = 0
                summaryPresent = $true
                summaryPassed = $true
                evidenceDirectoryPresent = $true
                attemptState = "runner-started-terminal"
                retryAllowed = $false
                finishedAtUtc = ConvertTo-CanonicalUtcString ([DateTime]::UtcNow)
                pathsRecorded = $false
                secretsRecorded = $false
            })
            $CoordinatorDirectory = $terminalFixture.Root
            $terminal = (Follow-Coordinator | ConvertFrom-Json)
            if ($terminal.status -cne "failed-closed" -or
                $terminal.stage -cne "self-test-stage" -or
                $terminal.reasonCode -cne "self-test-reason") {
                throw "Follow terminal-failure precedence self-test failed."
            }
        }
        finally { $CoordinatorDirectory = $originalCoordinatorDirectory }
        [GC]::KeepAlive($selfTestRecoveredLifetimeJob)
        [GC]::KeepAlive($selfTestCleanLifetimeJob)
        [GC]::KeepAlive($selfTestDelayedHandleLifetimeJob)
        [GC]::KeepAlive($selfTestTimeoutCleanupLifetimeJob)
        [GC]::KeepAlive($selfTestPostRaceLifetimeJob)
        [GC]::KeepAlive($selfTestLifetimeJob)
        $selfTestSucceeded = $true
    }
    finally {
        $script:SelfTestAttemptLedgerRoot = $originalSelfTestAttemptLedgerRoot
        $script:SelfTestCoordinatorParent = $originalSelfTestCoordinatorParent
        $script:SelfTestEvidenceParent = $originalSelfTestEvidenceParent
        if ($null -ne $currentProcess) { $currentProcess.Dispose() }
        if ([IO.Directory]::Exists($testRoot)) {
            $resolvedTestRoot = [IO.Path]::GetFullPath($testRoot).TrimEnd(
                [IO.Path]::DirectorySeparatorChar,
                [IO.Path]::AltDirectorySeparatorChar
            )
            $resolvedTempRoot = [IO.Path]::GetFullPath(
                [IO.Path]::GetTempPath()
            ).TrimEnd(
                [IO.Path]::DirectorySeparatorChar,
                [IO.Path]::AltDirectorySeparatorChar
            )
            if ([IO.Path]::GetFileName($resolvedTestRoot) -cnotmatch
                    '^lbb-coordinator-self-test-[0-9a-f]{32}$' -or
                -not [String]::Equals(
                    [IO.Path]::GetDirectoryName($resolvedTestRoot),
                    $resolvedTempRoot,
                    [StringComparison]::OrdinalIgnoreCase
                )) {
                throw "The coordinator self-test cleanup root is not its GUID-scoped directory."
            }
            [IO.Directory]::Delete($testRoot, $true)
            if ([IO.Directory]::Exists($testRoot)) {
                throw "The coordinator self-test root remained after cleanup."
            }
        }
    }
    if ($selfTestSucceeded) { Write-Output $script:SuccessMessage }
}

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "The Windows acceptance coordinator can run only on Windows."
}

$systemPowerShellPath = Resolve-SystemWindowsPowerShell
$currentHostProcess = [Diagnostics.Process]::GetCurrentProcess()
try { $currentExecutable = $currentHostProcess.MainModule.FileName }
finally { $currentHostProcess.Dispose() }
$isExactSystemPowerShell = (
    $PSVersionTable.PSEdition -ceq "Desktop" -and
    $PSVersionTable.PSVersion.Major -eq 5 -and
    $PSVersionTable.PSVersion.Minor -eq 1 -and
    (-not [Environment]::Is64BitOperatingSystem -or [Environment]::Is64BitProcess) -and
    [String]::Equals(
        [IO.Path]::GetFullPath($currentExecutable),
        [IO.Path]::GetFullPath($systemPowerShellPath),
        [StringComparison]::OrdinalIgnoreCase
    )
)

$hasWorkerSupportSelfTestPath = -not [String]::IsNullOrWhiteSpace(
    $InternalWorkerSupportSelfTestPath
)
$hasWorkerSupportSelfTestSha256 = -not [String]::IsNullOrWhiteSpace(
    $InternalWorkerSupportSelfTestSha256
)
$hasWorkerSupportSelfTestNonce = -not [String]::IsNullOrWhiteSpace(
    $InternalWorkerSupportSelfTestNonce
)
if ($hasWorkerSupportSelfTestPath -ne $hasWorkerSupportSelfTestSha256 -or
    $hasWorkerSupportSelfTestPath -ne $hasWorkerSupportSelfTestNonce) {
    throw "The internal staged worker-support loader self-test binding is incomplete."
}
$hasWorkerSupportSelfTest = $hasWorkerSupportSelfTestPath -and
    $hasWorkerSupportSelfTestSha256 -and
    $hasWorkerSupportSelfTestNonce
$hasNestedJobRunnerSelfTest = -not [String]::IsNullOrWhiteSpace(
    $InternalNestedJobRunnerSelfTestNonce
)
if ($hasWorkerSupportSelfTest -and (
    -not [String]::IsNullOrWhiteSpace($CleanCoordinatorNonce) -or
    -not [String]::IsNullOrWhiteSpace($InternalWorkerNonce) -or
    $hasNestedJobRunnerSelfTest
)) {
    throw "Internal coordinator entry points cannot be combined."
}
if ($hasNestedJobRunnerSelfTest -and (
    -not [String]::IsNullOrWhiteSpace($CleanCoordinatorNonce) -or
    -not [String]::IsNullOrWhiteSpace($InternalWorkerNonce)
)) {
    throw "Internal coordinator entry points cannot be combined."
}

if ([String]::IsNullOrWhiteSpace($CleanCoordinatorNonce) -and
    [String]::IsNullOrWhiteSpace($InternalWorkerNonce) -and
    -not $hasWorkerSupportSelfTest -and
    -not $hasNestedJobRunnerSelfTest) {
    Invoke-CleanBootstrap $systemPowerShellPath
}

if (-not [String]::IsNullOrWhiteSpace($CleanCoordinatorNonce)) {
    if (-not $isExactSystemPowerShell) {
        throw "The clean coordinator child is not exact system Windows PowerShell 5.1."
    }
    $bootstrap = Import-CleanBootstrap $CleanCoordinatorNonce
    $Mode = [string]$bootstrap.MODE
    $Version = [string]$bootstrap.VERSION
    $ServerPath = [string]$bootstrap.SERVER
    $HelperPath = [string]$bootstrap.HELPER
    $ChecksumManifest = [string]$bootstrap.MANIFEST
    $ChecksumManifestSha256 = [string]$bootstrap.MANIFEST_SHA256
    $CandidateBindingPath = [string]$bootstrap.BINDING
    $FixturePath = [string]$bootstrap.FIXTURE
    $EvidenceDirectory = [string]$bootstrap.EVIDENCE
    $CoordinatorDirectory = [string]$bootstrap.COORDINATOR
    $ForegroundArmTimeoutSeconds = [int]$bootstrap.ARM_TIMEOUT
    $StartupTimeoutSeconds = [int]$bootstrap.START_TIMEOUT
}

if (-not [String]::IsNullOrWhiteSpace($InternalWorkerNonce)) {
    if (-not $isExactSystemPowerShell -or $Mode -cne "Start") {
        throw "The internal worker can run only as exact system Windows PowerShell 5.1 in Start mode."
    }
    Invoke-CoordinatorWorker $InternalWorkerNonce
    return
}

if ($hasWorkerSupportSelfTest) {
    if (-not $isExactSystemPowerShell -or $Mode -cne "SelfTest") {
        throw "The internal staged worker-support loader self-test can run only as exact system Windows PowerShell 5.1 in SelfTest mode."
    }
    Invoke-WorkerSupportLoaderSelfTest `
        -AssemblyPath $InternalWorkerSupportSelfTestPath `
        -AssemblySha256 $InternalWorkerSupportSelfTestSha256 `
        -Nonce $InternalWorkerSupportSelfTestNonce
    return
}

if ($hasNestedJobRunnerSelfTest) {
    if (-not $isExactSystemPowerShell -or $Mode -cne "SelfTest") {
        throw "The internal nested-Job runner self-test can run only as exact system Windows PowerShell 5.1 in SelfTest mode."
    }
    Invoke-NestedJobRunnerSelfTest $InternalNestedJobRunnerSelfTestNonce
    return
}

switch ($Mode) {
    "SelfTest" { Invoke-SelfTest; return }
    "Follow" { Follow-Coordinator; return }
    "Start" { Start-Coordinator; return }
}
