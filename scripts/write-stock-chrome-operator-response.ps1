#requires -Version 5.1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Respond", "AttestExternalSurfaces", "RelayGitHubToken", "NewSessionRef", "SelfTest")]
    [string]$Mode,
    [string]$RequestPath,
    [ValidateSet("independent-agent", "user-via-orchestrator")]
    [string]$ResponderKind,
    [string]$ResponderSessionRef,
    [string]$DecisionJson,
    [string]$CandidateBindingPath,
    [ValidateSet("preflight", "postflight")]
    [string]$ExternalSurfacePhase,
    [string]$OutputPath,
    [string]$GitHubTokenPipeName,
    [ValidateRange(1, 120)]
    [int]$RelayTimeoutSeconds = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$script:Utf8 = [Text.UTF8Encoding]::new($false, $true)
$script:Version = "0.12.20"

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

function Set-DirectoryAccessControlPortable(
    [string]$Path,
    [Security.AccessControl.DirectorySecurity]$Security
) {
    if ($PSVersionTable.PSEdition -ceq "Core") {
        [IO.FileSystemAclExtensions]::SetAccessControl([IO.DirectoryInfo]::new($Path), $Security)
    }
    else {
        [IO.Directory]::SetAccessControl($Path, $Security)
    }
}

function Assert-NoReparseAncestorChain([string]$Path, [string]$Label) {
    $Full = [IO.Path]::GetFullPath($Path)
    $Directory = if ([IO.Directory]::Exists($Full)) {
        [IO.DirectoryInfo]::new($Full)
    } else { [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName($Full)) }
    while ($null -ne $Directory) {
        if ($Directory.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "$Label traverses a reparse-point directory."
        }
        $Directory = $Directory.Parent
    }
}

function Assert-PrivateDirectory([string]$Path) {
    $Full = [IO.Path]::GetFullPath($Path)
    if (-not [IO.Directory]::Exists($Full) -or
        ([IO.DirectoryInfo]::new($Full).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "The response exchange directory is not an ordinary directory."
    }
    Assert-NoReparseAncestorChain $Full "response exchange directory"
    $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        if ($null -eq $Identity -or $null -eq $Identity.User) {
            throw "The current Windows identity is unavailable."
        }
        $Acl = Get-DirectoryAccessControlPortable $Full
        $Owner = $Acl.GetOwner([Security.Principal.SecurityIdentifier])
        $Rules = @($Acl.GetAccessRules(
            $true, $true, [Security.Principal.SecurityIdentifier]
        ))
        $FullControl = @($Rules | Where-Object {
            $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
            $_.IdentityReference.Value -ceq $Identity.User.Value -and
            ($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -eq
                [Security.AccessControl.FileSystemRights]::FullControl -and
            -not $_.IsInherited
        })
        if ($Owner.Value -cne $Identity.User.Value -or -not $Acl.AreAccessRulesProtected -or
            $Rules.Count -lt 1 -or $FullControl.Count -lt 1 -or
            @($Rules | Where-Object {
                $_.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
                $_.IdentityReference.Value -cne $Identity.User.Value -or $_.IsInherited
            }).Count -ne 0) {
            throw "The response exchange directory ACL is not protected and private to the current user."
        }
    }
    finally { if ($null -ne $Identity) { $Identity.Dispose() } }
}

function Get-BytesSha256([byte[]]$Bytes) {
    $Hasher = [Security.Cryptography.SHA256]::Create()
    $Digest = $null
    try {
        $Digest = $Hasher.ComputeHash($Bytes)
        return ([BitConverter]::ToString($Digest)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        if ($null -ne $Digest) { [Array]::Clear($Digest, 0, $Digest.Length) }
        $Hasher.Dispose()
    }
}

function Get-CanonicalObjectSha256([object]$Value) {
    $Bytes = $script:Utf8.GetBytes(($Value | ConvertTo-Json -Depth 30 -Compress))
    try { return Get-BytesSha256 $Bytes }
    finally { [Array]::Clear($Bytes, 0, $Bytes.Length) }
}

function New-OpaqueSessionRef {
    $Bytes = New-Object byte[] 32
    $Rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $Rng.GetBytes($Bytes)
        return ([BitConverter]::ToString($Bytes)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        [Array]::Clear($Bytes, 0, $Bytes.Length)
        $Rng.Dispose()
    }
}

function Get-RemainingRelayMilliseconds([DateTimeOffset]$Deadline) {
    $Remaining = $Deadline - [DateTimeOffset]::UtcNow
    if ($Remaining.TotalMilliseconds -le 0) {
        throw "The GitHub token relay exceeded its absolute deadline."
    }
    return [Math]::Max(1, [Math]::Min(120000, [int]$Remaining.TotalMilliseconds))
}

function New-PrivateTokenRelayPipe([string]$Name) {
    $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        $Security = New-Object IO.Pipes.PipeSecurity
        $Security.SetOwner($Identity.User)
        $Rule = New-Object IO.Pipes.PipeAccessRule(
            $Identity.User,
            [IO.Pipes.PipeAccessRights]::ReadWrite,
            [Security.AccessControl.AccessControlType]::Allow
        )
        [void]$Security.AddAccessRule($Rule)
        if ($PSVersionTable.PSEdition -ceq "Core") {
            return [IO.Pipes.NamedPipeServerStreamAcl]::Create(
                $Name,
                [IO.Pipes.PipeDirection]::Out,
                1,
                [IO.Pipes.PipeTransmissionMode]::Byte,
                [IO.Pipes.PipeOptions]::Asynchronous,
                4096,
                4096,
                $Security,
                [IO.HandleInheritability]::None,
                [IO.Pipes.PipeAccessRights]0
            )
        }
        return [IO.Pipes.NamedPipeServerStream]::new(
            $Name,
            [IO.Pipes.PipeDirection]::Out,
            1,
            [IO.Pipes.PipeTransmissionMode]::Byte,
            [IO.Pipes.PipeOptions]::Asynchronous,
            4096,
            4096,
            $Security
        )
    }
    finally { if ($null -ne $Identity) { $Identity.Dispose() } }
}

function Invoke-GitHubTokenRelay(
    [string]$PipeName,
    [IO.Stream]$InputStream,
    [int]$TimeoutSeconds
) {
    if ($PipeName -cnotmatch '^lbb-gh-[0-9a-f]{32}$') {
        throw "GitHubTokenPipeName must be a fresh canonical non-secret pipe name."
    }
    if ($null -eq $InputStream -or -not $InputStream.CanRead) {
        throw "The GitHub token relay requires a readable stdin stream."
    }
    $Deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    $Pipe = New-PrivateTokenRelayPipe $PipeName
    $OneByte = New-Object byte[] 1
    try {
        $AcceptTask = $Pipe.WaitForConnectionAsync()
        if (-not $AcceptTask.Wait((Get-RemainingRelayMilliseconds $Deadline))) {
            throw "The GitHub token relay timed out waiting for its one local client."
        }
        [void]$AcceptTask.GetAwaiter().GetResult()
        $Completed = $false
        for ($Count = 0; $Count -le 4096; $Count += 1) {
            $ReadTask = $InputStream.ReadAsync($OneByte, 0, 1)
            if (-not $ReadTask.Wait((Get-RemainingRelayMilliseconds $Deadline))) {
                throw "The GitHub token relay stdin stalled before its line terminator."
            }
            $ReadCount = $ReadTask.GetAwaiter().GetResult()
            if ($ReadCount -ne 1) {
                throw "The GitHub token relay stdin ended before its line terminator."
            }
            $Byte = [int]$OneByte[0]
            if ($Byte -eq 10) {
                if ($Count -lt 16) { throw "The relayed GitHub token is too short." }
                $Completed = $true
            }
            elseif ($Byte -lt 33 -or $Byte -gt 126) {
                throw "The GitHub token relay accepts one bounded printable-ASCII line."
            }
            $WriteTask = $Pipe.WriteAsync($OneByte, 0, 1)
            if (-not $WriteTask.Wait((Get-RemainingRelayMilliseconds $Deadline))) {
                throw "The GitHub token relay client stalled during delivery."
            }
            [void]$WriteTask.GetAwaiter().GetResult()
            $OneByte[0] = 0
            if ($Completed) { break }
        }
        if (-not $Completed) {
            throw "The GitHub token relay exceeded its 4096-byte bound."
        }
        $Flush = $Pipe.FlushAsync()
        if (-not $Flush.Wait((Get-RemainingRelayMilliseconds $Deadline))) {
            throw "The GitHub token relay timed out while flushing its client."
        }
        [void]$Flush.GetAwaiter().GetResult()
    }
    finally {
        [Array]::Clear($OneByte, 0, $OneByte.Length)
        $Pipe.Dispose()
    }
}

function Read-StableJson([string]$Path, [string]$Label) {
    $Item = [IO.FileInfo]::new([IO.Path]::GetFullPath($Path))
    if (-not $Item.Exists -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $Item.Length -le 0 -or $Item.Length -gt 4MB) {
        throw "$Label is not an ordinary bounded JSON file."
    }
    Assert-NoReparseAncestorChain $Item.FullName $Label
    $Stream = [IO.File]::Open(
        $Item.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None
    )
    $Bytes = $null
    try {
        if ($Stream.Length -ne $Item.Length -or $Stream.Length -gt [int]::MaxValue) {
            throw "$Label changed before its stable read."
        }
        $Bytes = New-Object byte[] ([int]$Stream.Length)
        $Offset = 0
        while ($Offset -lt $Bytes.Length) {
            $Read = $Stream.Read($Bytes, $Offset, $Bytes.Length - $Offset)
            if ($Read -le 0) { throw "$Label ended during its stable read." }
            $Offset += $Read
        }
        if ($Stream.ReadByte() -ne -1) { throw "$Label grew during its stable read." }
        try { $Value = ConvertFrom-JsonPreservingStrings ($script:Utf8.GetString($Bytes)) }
        catch { throw "$Label is not strict UTF-8 JSON." }
        return [pscustomobject]@{
            Value = $Value
            Sha256 = Get-BytesSha256 $Bytes
            Bytes = $Bytes.Length
        }
    }
    finally {
        $Stream.Dispose()
        if ($null -ne $Bytes) { [Array]::Clear($Bytes, 0, $Bytes.Length) }
    }
}

function Read-StablePng([string]$Path, [string]$Label) {
    $Item = [IO.FileInfo]::new([IO.Path]::GetFullPath($Path))
    if (-not $Item.Exists -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $Item.Length -lt 24 -or $Item.Length -gt 20MB) {
        throw "$Label is not an ordinary bounded PNG."
    }
    $Stream = [IO.File]::Open(
        $Item.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None
    )
    $Bytes = $null
    try {
        if ($Stream.Length -ne $Item.Length -or $Stream.Length -lt 24 -or
            $Stream.Length -gt 20MB -or $Stream.Length -gt [int]::MaxValue) {
            throw "$Label changed size before its exclusive read."
        }
        $Bytes = New-Object byte[] ([int]$Stream.Length)
        $Offset = 0
        while ($Offset -lt $Bytes.Length) {
            $Read = $Stream.Read($Bytes, $Offset, $Bytes.Length - $Offset)
            if ($Read -le 0) { throw "$Label ended during its stable read." }
            $Offset += $Read
        }
        if (([BitConverter]::ToString($Bytes, 0, 8)) -cne "89-50-4E-47-0D-0A-1A-0A" -or
            [Text.Encoding]::ASCII.GetString($Bytes, 12, 4) -cne "IHDR") {
            throw "$Label is not a PNG."
        }
        $Width = ([uint32]$Bytes[16] -shl 24) -bor ([uint32]$Bytes[17] -shl 16) -bor
            ([uint32]$Bytes[18] -shl 8) -bor [uint32]$Bytes[19]
        $Height = ([uint32]$Bytes[20] -shl 24) -bor ([uint32]$Bytes[21] -shl 16) -bor
            ([uint32]$Bytes[22] -shl 8) -bor [uint32]$Bytes[23]
        if ($Width -lt 120 -or $Height -lt 32 -or $Width -gt 8192 -or $Height -gt 8192) {
            throw "$Label has invalid dimensions."
        }
        return [pscustomobject]@{
            Sha256 = Get-BytesSha256 $Bytes
            Bytes = $Bytes.Length
            Width = [int64]$Width
            Height = [int64]$Height
        }
    }
    finally {
        $Stream.Dispose()
        if ($null -ne $Bytes) { [Array]::Clear($Bytes, 0, $Bytes.Length) }
    }
}

function Resolve-ExchangeImageLeaf([string]$Directory, [object]$Name, [string]$Label) {
    if ($Name -isnot [string] -or
        [string]$Name -cnotmatch '^(?:frame-[0-9a-f]{32}|browser-0[1-6]-[a-z0-9-]+(?:\.raw)?)\.png$' -or
        [IO.Path]::GetFileName([string]$Name) -cne [string]$Name -or
        [IO.Path]::IsPathRooted([string]$Name)) {
        throw "$Label is not a canonical exchange image leaf name."
    }
    $Full = [IO.Path]::GetFullPath([IO.Path]::Combine($Directory, [string]$Name))
    $Prefix = [IO.Path]::GetFullPath($Directory).TrimEnd('\') + '\'
    if (-not $Full.StartsWith($Prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label escaped the exact response exchange directory."
    }
    Assert-NoReparseAncestorChain $Full $Label
    return $Full
}

function Assert-ExactKeys([object]$Value, [string[]]$Expected, [string]$Label) {
    if ((@($Value.PSObject.Properties.Name) -join "`n") -cne ($Expected -join "`n")) {
        throw "$Label fields are missing, extra, reordered, or case-changed."
    }
}

function Assert-Hex([object]$Value, [int]$Length, [string]$Label) {
    if ($Value -isnot [string] -or [string]$Value -cnotmatch "^[0-9a-f]{$Length}$") {
        throw "$Label is not canonical lowercase hexadecimal."
    }
}

function Format-CanonicalUtc([DateTimeOffset]$Value) {
    return $Value.UtcDateTime.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
}

function Assert-CanonicalTimestamp([object]$Value, [string]$Label) {
    $Parsed = [DateTimeOffset]::MinValue
    if ($Value -isnot [string] -or
        [string]$Value -cnotmatch '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{7}Z$' -or
        -not [DateTimeOffset]::TryParseExact(
            [string]$Value, "o", [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind, [ref]$Parsed
        )) {
        throw "$Label is not canonical Z-suffixed UTC."
    }
    return $Parsed
}

function Get-FreshResponsePublicationTime(
    [DateTimeOffset]$CreatedAt,
    [DateTimeOffset]$ExpiresAt
) {
    $PublishedAt = [DateTimeOffset]::UtcNow
    if ($ExpiresAt -le $CreatedAt -or $PublishedAt -lt $CreatedAt -or
        $PublishedAt -gt $ExpiresAt) {
        throw "Request expired before response publication."
    }
    return $PublishedAt
}

function Convert-StrictDecision([string]$Json) {
    if ([String]::IsNullOrWhiteSpace($Json) -or $Json.Length -gt 1MB) {
        throw "DecisionJson is empty or oversized."
    }
    try { $Decision = ConvertFrom-JsonPreservingStrings $Json }
    catch { throw "DecisionJson is not strict JSON." }
    $Canonical = $Decision | ConvertTo-Json -Depth 30 -Compress
    if ($Canonical -cne $Json) {
        throw "DecisionJson must be canonical compact JSON with no duplicate, case-colliding, or trailing data."
    }
    return $Decision
}

function Assert-OperatorDecision([string]$Kind, [object]$Decision, [object]$Allowed) {
    switch ($Kind) {
        "window-selection" {
            Assert-ExactKeys $Decision @("index") "window-selection decision"
            if (-not (Test-ExactJsonInteger $Decision.index) -or
                [int64]$Decision.index -lt 0) {
                throw "Window-selection index is invalid."
            }
        }
        "ui-point" {
            Assert-ExactKeys $Decision @("x", "y") "UI-point decision"
            if (-not (Test-ExactJsonInteger $Decision.x) -or
                -not (Test-ExactJsonInteger $Decision.y)) {
                throw "UI-point coordinates are invalid."
            }
        }
        "ui-state" {
            Assert-ExactKeys $Decision @("value") "UI-state decision"
            if ($Decision.value -isnot [string] -or
                @($Allowed.values) -cnotcontains [string]$Decision.value) {
                throw "UI-state value is outside the request enum."
            }
        }
        "ui-verification" {
            Assert-ExactKeys $Decision @("passed") "UI-verification decision"
            if ($Decision.passed -isnot [bool]) { throw "UI-verification decision is invalid." }
        }
        "scoped-user-approval" {
            Assert-ExactKeys $Decision @("approved", "approvedBy", "confirmationMode") `
                "scoped approval decision"
            if ($Decision.approved -isnot [bool] -or $Decision.approvedBy -cne "user" -or
                $Decision.confirmationMode -cne "batched-action-time") {
                throw "Scoped approval decision is not an exact user answer shape."
            }
        }
        default { throw "Operator request kind is unsupported." }
    }
}

function Test-UnableDecision([object]$Decision) {
    if ($null -eq $Decision.PSObject.Properties["unable"]) { return $false }
    Assert-ExactKeys $Decision @("unable") "unable decision"
    if ($Decision.unable -isnot [bool] -or $Decision.unable -ne $true) {
        throw 'Unable decision must be exactly {"unable":true}.'
    }
    return $true
}

function Test-ExactJsonInteger([object]$Value) {
    return $Value -is [int] -or $Value -is [long]
}

function Assert-CropDecision([object]$Decision, [object]$Source) {
    Assert-ExactKeys $Decision @(
        "cropX", "cropY", "cropWidth", "cropHeight",
        "requiredStateVisible", "sensitivePixelsInsideCrop", "uncertain"
    ) "reviewer crop decision"
    if (-not (Test-ExactJsonInteger $Decision.cropX) -or
        -not (Test-ExactJsonInteger $Decision.cropY) -or
        -not (Test-ExactJsonInteger $Decision.cropWidth) -or
        -not (Test-ExactJsonInteger $Decision.cropHeight) -or
        [int64]$Decision.cropX -lt 0 -or [int64]$Decision.cropY -lt 0 -or
        [int64]$Decision.cropWidth -lt 120 -or [int64]$Decision.cropHeight -lt 32 -or
        ([int64]$Decision.cropX + [int64]$Decision.cropWidth) -gt [int64]$Source.width -or
        ([int64]$Decision.cropY + [int64]$Decision.cropHeight) -gt [int64]$Source.height -or
        $Decision.requiredStateVisible -isnot [bool] -or
        $Decision.sensitivePixelsInsideCrop -isnot [bool] -or
        $Decision.uncertain -isnot [bool]) {
        throw "Reviewer crop decision is invalid or out of bounds."
    }
}

function Assert-SixCropDecision([object]$Decision, [object[]]$Expected) {
    Assert-ExactKeys $Decision @("entries", "aggregate") "six-crop decision"
    if (@($Decision.entries).Count -ne 6 -or $Expected.Count -ne 6) {
        throw "Six-crop decision does not contain exactly six entries."
    }
    $Fields = @(
        "sequence", "purpose", "image", "sha256", "width", "height",
        "requiredVisibleStateSha256", "digestMatched", "requiredStateVerdict",
        "sensitivePixelsObserved", "uncertain"
    )
    for ($Index = 0; $Index -lt 6; $Index += 1) {
        $Actual = $Decision.entries[$Index]
        $Wanted = $Expected[$Index]
        Assert-ExactKeys $Actual $Fields "six-crop entry"
        if (-not (Test-ExactJsonInteger $Actual.sequence) -or
            -not (Test-ExactJsonInteger $Actual.width) -or
            -not (Test-ExactJsonInteger $Actual.height) -or
            [int64]$Actual.sequence -ne ($Index + 1) -or $Actual.purpose -cne $Wanted.purpose -or
            $Actual.image -cne $Wanted.image -or $Actual.sha256 -cne $Wanted.sha256 -or
            [int64]$Actual.width -ne [int64]$Wanted.width -or
            [int64]$Actual.height -ne [int64]$Wanted.height -or
            $Actual.requiredVisibleStateSha256 -cne $Wanted.requiredVisibleStateSha256 -or
            $Actual.digestMatched -isnot [bool] -or
            $Actual.requiredStateVerdict -notin @("pass", "fail") -or
            $Actual.sensitivePixelsObserved -isnot [bool] -or $Actual.uncertain -isnot [bool]) {
            throw "Six-crop decision is reordered, changed, or structurally invalid."
        }
    }
    Assert-ExactKeys $Decision.aggregate @(
        "reviewedCropCount", "everySanitizedCropOpenedByReviewer", "allImageDigestsMatched",
        "requiredVisibleStateConfirmedByReviewer", "noSensitivePixelsObservedByReviewer",
        "noUncertaintyReported", "visualJudgmentNotPixelSafetyProof"
    ) "six-crop aggregate"
    if (-not (Test-ExactJsonInteger $Decision.aggregate.reviewedCropCount) -or
        [int64]$Decision.aggregate.reviewedCropCount -ne 6 -or
        $Decision.aggregate.everySanitizedCropOpenedByReviewer -isnot [bool] -or
        $Decision.aggregate.allImageDigestsMatched -isnot [bool] -or
        $Decision.aggregate.requiredVisibleStateConfirmedByReviewer -isnot [bool] -or
        $Decision.aggregate.noSensitivePixelsObservedByReviewer -isnot [bool] -or
        $Decision.aggregate.noUncertaintyReported -isnot [bool] -or
        $Decision.aggregate.visualJudgmentNotPixelSafetyProof -isnot [bool] -or
        $Decision.aggregate.visualJudgmentNotPixelSafetyProof -ne $true) {
        throw "Six-crop aggregate is not structurally valid."
    }
}

function Assert-RequestInput([object]$Request, [string]$Directory) {
    if ($Request.evidenceType -ceq "stock-user-chrome-operator-request") {
        if ($null -ne $Request.frame) {
            $FramePath = Resolve-ExchangeImageLeaf $Directory $Request.frame.name `
                "operator request frame name"
            $Frame = Read-StablePng $FramePath "operator request frame"
            if ($Frame.Sha256 -cne $Request.inputDigestSha256 -or
                $Frame.Bytes -ne $Request.frame.bytes -or $Frame.Width -ne $Request.frame.width -or
                $Frame.Height -ne $Request.frame.height) {
                throw "Operator request frame does not match its exact input digest."
            }
        }
        elseif ($null -eq $Request.context -or
            [string]$Request.context.statusResponseSha256 -cne $Request.inputDigestSha256) {
            throw "Frameless operator request is not bound to an exact status digest."
        }
    }
    elseif ($Request.kind -ceq "screenshot-crop") {
        $Source = $Request.context.source
        $Image = Read-StablePng (Resolve-ExchangeImageLeaf `
            $Directory $Source.name "reviewer crop source name") `
            "reviewer crop source"
        if ($Image.Sha256 -cne $Request.inputDigestSha256 -or
            $Image.Bytes -ne $Source.bytes -or $Image.Width -ne $Source.width -or
            $Image.Height -ne $Source.height) {
            throw "Reviewer crop source does not match its exact input digest."
        }
    }
    else {
        if ((Get-CanonicalObjectSha256 $Request.context) -cne $Request.inputDigestSha256) {
            throw "Six-crop request context does not match its canonical input digest."
        }
        foreach ($Entry in @($Request.context.entries)) {
            $Image = Read-StablePng (Resolve-ExchangeImageLeaf `
                $Directory $Entry.image "six-crop request image name") `
                "six-crop request image"
            if ($Image.Sha256 -cne $Entry.sha256 -or $Image.Width -ne $Entry.width -or
                $Image.Height -ne $Entry.height) {
                throw "Six-crop request image does not match its exact digest and dimensions."
            }
        }
    }
}

function Write-AtomicJson([string]$Path, [object]$Response, [switch]$ReserveClaimedName) {
    $Temporary = "$Path.new"
    $Claimed = [IO.Path]::Combine(
        [IO.Path]::GetDirectoryName($Path),
        [IO.Path]::GetFileNameWithoutExtension($Path) + ".claimed.json"
    )
    $Targets = @($Path, $Temporary)
    if ($ReserveClaimedName) { $Targets += $Claimed }
    foreach ($Target in $Targets) {
        if ([IO.File]::Exists($Target) -or [IO.Directory]::Exists($Target)) {
            throw "Response publication paths must all be new."
        }
    }
    $Bytes = $script:Utf8.GetBytes((($Response | ConvertTo-Json -Depth 30 -Compress) + "`n"))
    try {
        $Stream = [IO.File]::Open(
            $Temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
        )
        try { $Stream.Write($Bytes, 0, $Bytes.Length); $Stream.Flush($true) }
        finally { $Stream.Dispose() }
        [IO.File]::Move($Temporary, $Path)
    }
    finally {
        [Array]::Clear($Bytes, 0, $Bytes.Length)
        if ([IO.File]::Exists($Temporary)) { [IO.File]::Delete($Temporary) }
    }
}

function Get-ExactReleaseCandidateBinding([object]$Binding) {
    Assert-ExactKeys $Binding @(
        "schemaVersion", "productVersion", "repository", "tag", "sourceSha",
        "tagObjectSha", "workflowRunId", "workflowRunAttempt", "artifactId",
        "artifactName", "artifactZipBytes", "artifactZipSha256",
        "checksumManifestSha256", "attestationInvocationUri", "attestedAssetCount",
        "githubHostedRunner", "assets", "passed"
    ) "release-candidate trust binding"
    if ($Binding.schemaVersion -ne 1 -or
        $Binding.productVersion -cne $script:Version -or
        $Binding.repository -cne "flrngel/local-browser-bridge" -or
        $Binding.tag -cne "v$($script:Version)" -or
        [string]$Binding.sourceSha -cnotmatch '^[0-9a-f]{40}$' -or
        [string]$Binding.tagObjectSha -cnotmatch '^[0-9a-f]{40}$' -or
        [string]$Binding.workflowRunId -cnotmatch '^[1-9][0-9]*$' -or
        [string]$Binding.workflowRunAttempt -cnotmatch '^[1-9][0-9]*$' -or
        [string]$Binding.artifactId -cnotmatch '^[1-9][0-9]*$' -or
        $Binding.artifactName -cne "release-candidate" -or
        $Binding.artifactZipBytes -isnot [ValueType] -or [int64]$Binding.artifactZipBytes -le 0 -or
        [string]$Binding.artifactZipSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Binding.checksumManifestSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Binding.attestationInvocationUri -cnotmatch '^https://github\.com/flrngel/local-browser-bridge/actions/runs/[1-9][0-9]*/attempts/[1-9][0-9]*$' -or
        $Binding.attestedAssetCount -ne 5 -or $Binding.githubHostedRunner -ne $true -or
        $Binding.passed -ne $true -or @($Binding.assets).Count -ne 5) {
        throw "Release-candidate trust binding is not the exact passing v$($script:Version) domain."
    }
    $ExpectedNames = @(
        "local-browser-bridge-v$($script:Version)-windows-x86_64.exe",
        "local-computer-helper-v$($script:Version)-windows-x86_64.exe",
        "local-browser-bridge-v$($script:Version)-macos-universal.tar.gz",
        "local-browser-bridge-extension-v$($script:Version).zip",
        "SHA256SUMS.txt"
    )
    $Assets = @()
    for ($Index = 0; $Index -lt 5; $Index += 1) {
        $Asset = $Binding.assets[$Index]
        Assert-ExactKeys $Asset @("file", "bytes", "sha256") "release-candidate asset"
        if ($Asset.file -cne $ExpectedNames[$Index] -or
            $Asset.bytes -isnot [ValueType] -or [int64]$Asset.bytes -le 0 -or
            [string]$Asset.sha256 -cnotmatch '^[0-9a-f]{64}$') {
            throw "Release-candidate asset identity, order, size, or digest is invalid."
        }
        $Assets += [ordered]@{
            file = [string]$Asset.file
            bytes = [int64]$Asset.bytes
            sha256 = [string]$Asset.sha256
        }
    }
    return [ordered]@{
        productVersion = [string]$Binding.productVersion
        repository = [string]$Binding.repository
        tag = [string]$Binding.tag
        sourceSha = [string]$Binding.sourceSha
        tagObjectSha = [string]$Binding.tagObjectSha
        workflowRunId = [string]$Binding.workflowRunId
        workflowRunAttempt = [string]$Binding.workflowRunAttempt
        artifactId = [string]$Binding.artifactId
        artifactName = [string]$Binding.artifactName
        artifactZipBytes = [int64]$Binding.artifactZipBytes
        artifactZipSha256 = [string]$Binding.artifactZipSha256
        checksumManifestSha256 = [string]$Binding.checksumManifestSha256
        attestationInvocationUri = [string]$Binding.attestationInvocationUri
        attestedAssetCount = [int]$Binding.attestedAssetCount
        githubHostedRunner = [bool]$Binding.githubHostedRunner
        assets = $Assets
    }
}

function Invoke-AttestExternalSurfaces {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "External-surface attestation runs only on Windows."
    }
    if ([String]::IsNullOrWhiteSpace($CandidateBindingPath) -or
        [String]::IsNullOrWhiteSpace($ExternalSurfacePhase) -or
        [String]::IsNullOrWhiteSpace($OutputPath) -or
        [String]::IsNullOrWhiteSpace($ResponderSessionRef)) {
        throw "CandidateBindingPath, ExternalSurfacePhase, OutputPath, and ResponderSessionRef are required."
    }
    if (@("preflight", "postflight") -cnotcontains $ExternalSurfacePhase) {
        throw "ExternalSurfacePhase is unsupported."
    }
    Assert-Hex $ResponderSessionRef 64 "orchestrator session reference"
    $OutputFull = [IO.Path]::GetFullPath($OutputPath)
    $ExpectedLeaf = "external-surface-$ExternalSurfacePhase.json"
    if ([IO.Path]::GetFileName($OutputFull) -cne $ExpectedLeaf) {
        throw "External-surface attestation output must use the exact phase filename."
    }
    $OutputDirectory = [IO.Path]::GetDirectoryName($OutputFull)
    if (-not [IO.Directory]::Exists($OutputDirectory)) {
        if ($ExternalSurfacePhase -cne "preflight" -or [IO.File]::Exists($OutputDirectory)) {
            throw "Only preflight may initialize one new external-surface directory."
        }
        Assert-NoReparseAncestorChain ([IO.Path]::GetDirectoryName($OutputDirectory)) `
            "external-surface directory parent"
        [IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
        Set-PrivateDirectoryAcl $OutputDirectory
    }
    Assert-PrivateDirectory $OutputDirectory
    Assert-PrivateDirectory ([IO.Path]::GetDirectoryName(
        [IO.Path]::GetFullPath($CandidateBindingPath)
    ))
    $Stable = Read-StableJson $CandidateBindingPath "release-candidate trust binding"
    $ReleaseBinding = Get-ExactReleaseCandidateBinding $Stable.Value
    if ($ExternalSurfacePhase -ceq "preflight") {
        $ChromeMcpState = "not-used-before-candidate-execution"
        $ComputerUseState = "released-before-candidate-execution"
        $ReviewerInputState = "review-not-started"
    }
    else {
        $ChromeMcpState = "never-used-through-independent-review"
        $ComputerUseState = "not-resumed-through-independent-review"
        $ReviewerInputState = "exported-digest-bound-files-only"
    }
    $StableAgain = Read-StableJson $CandidateBindingPath `
        "release-candidate trust binding before attestation publication"
    if ($StableAgain.Sha256 -cne $Stable.Sha256 -or
        ($StableAgain.Value | ConvertTo-Json -Depth 30 -Compress) -cne
            ($Stable.Value | ConvertTo-Json -Depth 30 -Compress)) {
        throw "Release-candidate trust binding changed before attestation publication."
    }
    Write-AtomicJson $OutputFull ([ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-external-surface-attestation"
        phase = $ExternalSurfacePhase
        releaseCandidateBinding = $ReleaseBinding
        releaseCandidateBindingSha256 = Get-CanonicalObjectSha256 $ReleaseBinding
        orchestrationSurface = "user-orchestrator-secured-ssh-exported-file-review"
        chromeMcpState = $ChromeMcpState
        computerUseState = $ComputerUseState
        reviewerInputState = $ReviewerInputState
        attestorKind = "orchestrator-agent"
        attestorSessionRef = $ResponderSessionRef
        attestedAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
    })
    Write-Output "Stock-Chrome external-surface $ExternalSurfacePhase attestation atomically published."
}

function Invoke-Respond {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "The stock-Chrome responder runs only on Windows."
    }
    if ([String]::IsNullOrWhiteSpace($RequestPath) -or
        [String]::IsNullOrWhiteSpace($ResponderKind) -or
        [String]::IsNullOrWhiteSpace($ResponderSessionRef)) {
        throw "RequestPath, ResponderKind, and ResponderSessionRef are required."
    }
    Assert-Hex $ResponderSessionRef 64 "responder session reference"
    $FullRequest = [IO.Path]::GetFullPath($RequestPath)
    $Directory = [IO.Path]::GetDirectoryName($FullRequest)
    Assert-PrivateDirectory $Directory
    $Stable = Read-StableJson $FullRequest "operator/reviewer request"
    $Request = $Stable.Value
    $CommonFields = @(
        "schemaVersion", "evidenceType", "productVersion", "releaseCandidateBinding",
        "candidateBinding", "candidateBindingSha256", "requestId", "sequence", "kind",
        "actionName", "createdAtUtc", "expiresAtUtc", "executorSessionRef"
    )
    $TailFields = @("inputDigestSha256", "context", "allowedResponse", "instruction")
    if ($Request.evidenceType -ceq "stock-user-chrome-operator-request") {
        $ExpectedFields = $CommonFields + @("inputDigestSha256", "frame", "context", "allowedResponse", "instruction")
        $ExpectedResponder = if ($Request.kind -ceq "scoped-user-approval") {
            "user-via-orchestrator"
        } else { "independent-agent" }
        $ResponseEvidenceType = "stock-user-chrome-operator-response"
    }
    elseif ($Request.evidenceType -ceq "stock-user-chrome-reviewer-request") {
        $ExpectedFields = $CommonFields + @("reviewerSessionRef") + $TailFields
        $ExpectedResponder = "independent-agent"
        $ResponseEvidenceType = "stock-user-chrome-reviewer-response"
    }
    else { throw "Request evidenceType is unsupported." }
    Assert-ExactKeys $Request $ExpectedFields "operator/reviewer request"
    if ($Request.schemaVersion -ne 1 -or $Request.productVersion -cne $script:Version -or
        $Request.requestId -cnotmatch '^[0-9a-f]{32}$' -or
        [IO.Path]::GetFileName($FullRequest) -cne "request-$($Request.requestId).json" -or
        $Request.sequence -isnot [ValueType] -or [int64]$Request.sequence -lt 1 -or
        $ResponderKind -cne $ExpectedResponder) {
        throw "Request identity, sequence, or responder role is invalid."
    }
    foreach ($Name in @("candidateBindingSha256", "inputDigestSha256", "executorSessionRef")) {
        Assert-Hex $Request.$Name 64 "request $Name"
    }
    if ((Get-CanonicalObjectSha256 $Request.candidateBinding) -cne
        $Request.candidateBindingSha256) {
        throw "Request candidate binding digest is invalid."
    }
    if ($ResponderSessionRef -ceq $Request.executorSessionRef) {
        throw "Responder session must differ from the executor session."
    }
    if ($Request.evidenceType -ceq "stock-user-chrome-reviewer-request" -and
        $ResponderSessionRef -cne $Request.reviewerSessionRef) {
        throw "Reviewer response does not use the request's persistent reviewer session."
    }
    $CreatedAt = Assert-CanonicalTimestamp $Request.createdAtUtc "request creation"
    $ExpiresAt = Assert-CanonicalTimestamp $Request.expiresAtUtc "request expiry"
    $ValidationStartedAt = [DateTimeOffset]::UtcNow
    if ($ExpiresAt -le $CreatedAt -or $ValidationStartedAt -lt $CreatedAt -or
        $ValidationStartedAt -gt $ExpiresAt) {
        throw "Request is premature, expired, or has an invalid interval."
    }
    $Decision = Convert-StrictDecision $DecisionJson
    $Unable = Test-UnableDecision $Decision
    Assert-RequestInput $Request $Directory
    if ($Unable) {
        # A structurally exact negative response is published so the consumer can
        # enter rollback immediately instead of waiting for request expiry.
    }
    elseif ($Request.evidenceType -ceq "stock-user-chrome-operator-request") {
        Assert-OperatorDecision $Request.kind $Decision $Request.allowedResponse
    }
    elseif ($Request.kind -ceq "screenshot-crop") {
        Assert-CropDecision $Decision $Request.context.source
    }
    elseif ($Request.kind -ceq "six-crop-review") {
        Assert-SixCropDecision $Decision @($Request.context.entries)
    }
    else { throw "Reviewer request kind is unsupported." }
    $StableAgain = Read-StableJson $FullRequest "operator/reviewer request before response publication"
    if ($StableAgain.Sha256 -cne $Stable.Sha256) {
        throw "Operator/reviewer request changed before response publication."
    }
    Assert-RequestInput $Request $Directory
    $RespondedAt = Get-FreshResponsePublicationTime $CreatedAt $ExpiresAt
    $ResponsePath = Join-Path $Directory "response-$($Request.requestId).json"
    Write-AtomicJson $ResponsePath ([ordered]@{
        schemaVersion = 1
        evidenceType = $ResponseEvidenceType
        requestId = [string]$Request.requestId
        requestSha256 = [string]$Stable.Sha256
        candidateBindingSha256 = [string]$Request.candidateBindingSha256
        inputDigestSha256 = [string]$Request.inputDigestSha256
        responderKind = $ResponderKind
        responderSessionRef = $ResponderSessionRef
        respondedAtUtc = Format-CanonicalUtc $RespondedAt
        decision = $Decision
    }) -ReserveClaimedName
    if ([DateTimeOffset]::UtcNow -gt $ExpiresAt) {
        throw "Request expired during atomic response publication."
    }
    Write-Output "Stock-Chrome operator response atomically published."
}

function Set-PrivateDirectoryAcl([string]$Path) {
    $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        $Security = New-Object Security.AccessControl.DirectorySecurity
        $Security.SetOwner($Identity.User)
        $Security.SetAccessRuleProtection($true, $false)
        $Rule = New-Object Security.AccessControl.FileSystemAccessRule(
            $Identity.User,
            [Security.AccessControl.FileSystemRights]::FullControl,
            [Security.AccessControl.InheritanceFlags]"ContainerInherit, ObjectInherit",
            [Security.AccessControl.PropagationFlags]::None,
            [Security.AccessControl.AccessControlType]::Allow
        )
        [void]$Security.AddAccessRule($Rule)
        Set-DirectoryAccessControlPortable $Path $Security
    }
    finally { if ($null -ne $Identity) { $Identity.Dispose() } }
}

function Invoke-SelfTest {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "The stock-Chrome responder self-test requires Windows."
    }
    $Root = [IO.Path]::Combine(
        [IO.Path]::GetTempPath(), "lbb-response-self-test-" + [Guid]::NewGuid().ToString("N")
    )
    [IO.Directory]::CreateDirectory($Root) | Out-Null
    Set-PrivateDirectoryAcl $Root
    try {
        $Candidate = [ordered]@{
            runNonce = [String]::new([char]"1", 64)
            finalSha = [String]::new([char]"2", 40)
        }
        $CandidateSha = Get-CanonicalObjectSha256 $Candidate
        $RequestId = [String]::new([char]"3", 32)
        $ExecutorRef = [String]::new([char]"4", 64)
        $ReviewerRef = [String]::new([char]"5", 64)
        $OrchestratorRef = [String]::new([char]"8", 64)
        $Now = [DateTimeOffset]::UtcNow
        $TrustAssets = @()
        foreach ($AssetName in @(
            "local-browser-bridge-v$($script:Version)-windows-x86_64.exe",
            "local-computer-helper-v$($script:Version)-windows-x86_64.exe",
            "local-browser-bridge-v$($script:Version)-macos-universal.tar.gz",
            "local-browser-bridge-extension-v$($script:Version).zip",
            "SHA256SUMS.txt"
        )) {
            $TrustAssets += [pscustomobject]([ordered]@{
                file = $AssetName; bytes = 1000; sha256 = [String]::new([char]"a", 64)
            })
        }
        $TrustBinding = [ordered]@{
            schemaVersion = 1; productVersion = $script:Version
            repository = "flrngel/local-browser-bridge"; tag = "v$($script:Version)"
            sourceSha = [String]::new([char]"b", 40)
            tagObjectSha = [String]::new([char]"c", 40)
            workflowRunId = "123"; workflowRunAttempt = "1"; artifactId = "456"
            artifactName = "release-candidate"; artifactZipBytes = 5000
            artifactZipSha256 = [String]::new([char]"d", 64)
            checksumManifestSha256 = [String]::new([char]"e", 64)
            attestationInvocationUri = "https://github.com/flrngel/local-browser-bridge/actions/runs/123/attempts/1"
            attestedAssetCount = 5; githubHostedRunner = $true; assets = $TrustAssets; passed = $true
        }
        $TrustPath = Join-Path $Root "candidate-binding.json"
        [IO.File]::WriteAllText(
            $TrustPath, (($TrustBinding | ConvertTo-Json -Depth 30) + "`n"), $script:Utf8
        )
        $ExpectedRelease = Get-ExactReleaseCandidateBinding ([pscustomobject]$TrustBinding)
        foreach ($Phase in @("preflight", "postflight")) {
            $script:CandidateBindingPath = $TrustPath
            $script:ExternalSurfacePhase = $Phase
            $script:OutputPath = Join-Path $Root "external-surface-$Phase.json"
            $script:ResponderSessionRef = $OrchestratorRef
            Invoke-AttestExternalSurfaces | Out-Null
            $Attestation = Read-StableJson $script:OutputPath "self-test external attestation"
            if ($Attestation.Value.phase -cne $Phase -or
                $Attestation.Value.attestorSessionRef -cne $OrchestratorRef -or
                $Attestation.Value.releaseCandidateBindingSha256 -cne
                    (Get-CanonicalObjectSha256 $ExpectedRelease)) {
                throw "External-surface attestation self-test lost its phase, session, or candidate binding."
            }
            $ReplayRejected = $false
            try { Invoke-AttestExternalSurfaces | Out-Null } catch { $ReplayRejected = $true }
            if (-not $ReplayRejected) {
                throw "External-surface attestation self-test accepted create-once replay."
            }
        }
        $BadPhaseRejected = $false
        try {
            $script:ExternalSurfacePhase = "invalid"
            $script:OutputPath = Join-Path $Root "external-surface-invalid.json"
            Invoke-AttestExternalSurfaces | Out-Null
        }
        catch { $BadPhaseRejected = $true }
        finally { $script:ExternalSurfacePhase = "postflight" }
        if (-not $BadPhaseRejected) {
            throw "External-surface attestation self-test accepted an invalid phase."
        }
        function Write-SelfTestRequest([object]$Value) {
            $Path = Join-Path $Root "request-$($Value.requestId).json"
            $Bytes = $script:Utf8.GetBytes((($Value | ConvertTo-Json -Depth 30) + "`n"))
            try {
                $Stream = [IO.File]::Open(
                    $Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
                )
                try { $Stream.Write($Bytes, 0, $Bytes.Length); $Stream.Flush($true) }
                finally { $Stream.Dispose() }
            }
            finally { [Array]::Clear($Bytes, 0, $Bytes.Length) }
            return $Path
        }
        function New-SelfTestPng([string]$Name) {
            $Path = Join-Path $Root $Name
            $Bytes = New-Object byte[] 25
            [byte[]]$Signature = @(137, 80, 78, 71, 13, 10, 26, 10)
            $Signature.CopyTo($Bytes, 0)
            [Text.Encoding]::ASCII.GetBytes("IHDR").CopyTo($Bytes, 12)
            $Bytes[19] = 120
            $Bytes[23] = 32
            [IO.File]::WriteAllBytes($Path, $Bytes)
            [Array]::Clear($Bytes, 0, $Bytes.Length)
            [Array]::Clear($Signature, 0, $Signature.Length)
            return Read-StablePng $Path "self-test PNG"
        }
        function Invoke-SelfTestPublication(
            [object]$Value,
            [string]$Kind,
            [string]$SessionRef,
            [string]$Json
        ) {
            $script:RequestPath = Write-SelfTestRequest $Value
            $script:ResponderKind = $Kind
            $script:ResponderSessionRef = $SessionRef
            $script:DecisionJson = $Json
            Invoke-Respond | Out-Null
            $Response = Join-Path $Root "response-$($Value.requestId).json"
            if (-not [IO.File]::Exists($Response) -or [IO.File]::Exists("$Response.new")) {
                throw "Responder self-test did not atomically publish an exact response."
            }
            $StableResponse = Read-StableJson $Response "self-test published response"
            $Canonical = $script:Utf8.GetBytes(
                (($StableResponse.Value | ConvertTo-Json -Depth 30 -Compress) + "`n")
            )
            try {
                if ($Canonical.Length -ne $StableResponse.Bytes -or
                    (Get-BytesSha256 $Canonical) -cne $StableResponse.Sha256) {
                    throw "Responder self-test response was not canonical compact UTF-8 JSON plus LF."
                }
            }
            finally { [Array]::Clear($Canonical, 0, $Canonical.Length) }
            return $Response
        }
        $Request = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-operator-request"
            productVersion = $script:Version
            releaseCandidateBinding = [ordered]@{ sourceSha = [String]::new([char]"6", 40) }
            candidateBinding = $Candidate
            candidateBindingSha256 = $CandidateSha
            requestId = $RequestId
            sequence = 1
            kind = "window-selection"
            actionName = "self-test-window"
            createdAtUtc = Format-CanonicalUtc $Now
            expiresAtUtc = Format-CanonicalUtc ($Now.AddMinutes(1))
            executorSessionRef = $ExecutorRef
            inputDigestSha256 = [String]::new([char]"7", 64)
            frame = $null
            context = [ordered]@{ statusResponseSha256 = [String]::new([char]"7", 64) }
            allowedResponse = [ordered]@{ type = "index" }
            instruction = "Select the exact self-test window."
        }
        $RequestFile = Join-Path $Root "request-$RequestId.json"
        $RequestBytes = $script:Utf8.GetBytes((($Request | ConvertTo-Json -Depth 30) + "`n"))
        [IO.File]::WriteAllBytes($RequestFile, $RequestBytes)
        [Array]::Clear($RequestBytes, 0, $RequestBytes.Length)
        $script:RequestPath = $RequestFile
        $script:ResponderKind = "independent-agent"
        $script:ResponderSessionRef = $ReviewerRef
        $script:DecisionJson = '{"index":0}'
        Invoke-Respond | Out-Null
        $ResponseFile = Join-Path $Root "response-$RequestId.json"
        if (-not [IO.File]::Exists($ResponseFile) -or [IO.File]::Exists("$ResponseFile.new")) {
            throw "Responder self-test did not atomically publish the response."
        }
        $ReplayRejected = $false
        try { Invoke-Respond | Out-Null } catch { $ReplayRejected = $true }
        if (-not $ReplayRejected) { throw "Responder self-test accepted response replay." }
        foreach ($BadJson in @(
            '{"index":0,"index":1}', '{"index":0,"Index":1}', '{"index":0} trailing'
        )) {
            $Rejected = $false
            try { [void](Convert-StrictDecision $BadJson) } catch { $Rejected = $true }
            if (-not $Rejected) {
                throw "Responder self-test accepted duplicate, case-colliding, or trailing JSON."
            }
        }
        foreach ($BadUnableJson in @('{"unable":false}', '{"unable":true,"index":0}')) {
            $Rejected = $false
            try {
                $BadUnable = Convert-StrictDecision $BadUnableJson
                [void](Test-UnableDecision $BadUnable)
            }
            catch { $Rejected = $true }
            if (-not $Rejected) {
                throw "Responder self-test accepted a malformed unable decision."
            }
        }
        foreach ($ApprovalCase in @(
            [pscustomobject]@{ Id = [String]::new([char]"9", 32); Approved = $false },
            [pscustomobject]@{ Id = [String]::new([char]"a", 32); Approved = $true }
        )) {
            $FrameName = "frame-$($ApprovalCase.Id).png"
            $FrameFacts = New-SelfTestPng $FrameName
            $ApprovalRequest = [ordered]@{
                schemaVersion = 1
                evidenceType = "stock-user-chrome-operator-request"
                productVersion = $script:Version
                releaseCandidateBinding = [ordered]@{ sourceSha = [String]::new([char]"6", 40) }
                candidateBinding = $Candidate
                candidateBindingSha256 = $CandidateSha
                requestId = $ApprovalCase.Id
                sequence = 2
                kind = "scoped-user-approval"
                actionName = "self-test-approval"
                createdAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
                expiresAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow.AddMinutes(1))
                executorSessionRef = $ExecutorRef
                inputDigestSha256 = $FrameFacts.Sha256
                frame = [ordered]@{
                    name = $FrameName; bytes = $FrameFacts.Bytes
                    sha256 = $FrameFacts.Sha256; width = $FrameFacts.Width; height = $FrameFacts.Height
                }
                context = [ordered]@{ scopeSha256 = [String]::new([char]"b", 64) }
                allowedResponse = [ordered]@{
                    type = "scoped-user-approval"; approvedBy = "user"
                    confirmationMode = "batched-action-time"
                }
                instruction = "Answer the exact candidate-bound approval challenge."
            }
            $ApprovalJson = ([ordered]@{
                approved = [bool]$ApprovalCase.Approved
                approvedBy = "user"
                confirmationMode = "batched-action-time"
            } | ConvertTo-Json -Compress)
            [void](Invoke-SelfTestPublication `
                $ApprovalRequest "user-via-orchestrator" $OrchestratorRef $ApprovalJson)
        }

        $CropName = "browser-01-self-test.raw.png"
        $CropFacts = New-SelfTestPng $CropName
        $CropId = [String]::new([char]"b", 32)
        $CropContext = [ordered]@{
            purpose = "self-test-crop"
            source = [ordered]@{
                name = $CropName; bytes = $CropFacts.Bytes; sha256 = $CropFacts.Sha256
                width = $CropFacts.Width; height = $CropFacts.Height
            }
            requiredVisibleState = "self-test state"
        }
        $CropRequest = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-reviewer-request"
            productVersion = $script:Version
            releaseCandidateBinding = [ordered]@{ sourceSha = [String]::new([char]"6", 40) }
            candidateBinding = $Candidate
            candidateBindingSha256 = $CandidateSha
            requestId = $CropId
            sequence = 1
            kind = "screenshot-crop"
            actionName = "self-test-crop"
            createdAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
            expiresAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow.AddMinutes(1))
            executorSessionRef = $ExecutorRef
            reviewerSessionRef = $ReviewerRef
            inputDigestSha256 = $CropFacts.Sha256
            context = $CropContext
            allowedResponse = [ordered]@{ type = "tight-crop" }
            instruction = "Return an exact crop verdict, including unsafe findings."
        }
        $UnsafeCropJson = ([ordered]@{
            cropX = 0; cropY = 0; cropWidth = 120; cropHeight = 32
            requiredStateVisible = $false; sensitivePixelsInsideCrop = $true; uncertain = $true
        } | ConvertTo-Json -Compress)
        [void](Invoke-SelfTestPublication `
            $CropRequest "independent-agent" $ReviewerRef $UnsafeCropJson)
        foreach ($BadCropDecision in @(
            [pscustomobject]@{
                cropX = 0.5; cropY = 0; cropWidth = 120; cropHeight = 32
                requiredStateVisible = $true; sensitivePixelsInsideCrop = $false; uncertain = $false
            },
            [pscustomobject]@{
                cropX = 0; cropY = 0; cropWidth = 120; cropHeight = 32
                requiredStateVisible = "True"; sensitivePixelsInsideCrop = $false; uncertain = $false
            }
        )) {
            $BadCropRejected = $false
            try { Assert-CropDecision $BadCropDecision $CropContext.source }
            catch { $BadCropRejected = $true }
            if (-not $BadCropRejected) {
                throw "Responder self-test accepted fractional coordinates or a string crop boolean."
            }
        }

        $SixEntries = @()
        for ($Index = 1; $Index -le 6; $Index += 1) {
            $ImageName = "browser-0$Index-self-test.png"
            $ImageFacts = New-SelfTestPng $ImageName
            $SixEntries += [ordered]@{
                sequence = $Index; purpose = "purpose-$Index"; image = $ImageName
                sha256 = $ImageFacts.Sha256; width = $ImageFacts.Width; height = $ImageFacts.Height
                requiredVisibleState = "state-$Index"
                requiredVisibleStateSha256 = [String]::new([char](96 + $Index), 64)
            }
        }
        $SixContext = [ordered]@{ entries = $SixEntries }
        $SixId = [String]::new([char]"c", 32)
        $SixRequest = [ordered]@{
            schemaVersion = 1
            evidenceType = "stock-user-chrome-reviewer-request"
            productVersion = $script:Version
            releaseCandidateBinding = [ordered]@{ sourceSha = [String]::new([char]"6", 40) }
            candidateBinding = $Candidate
            candidateBindingSha256 = $CandidateSha
            requestId = $SixId
            sequence = 2
            kind = "six-crop-review"
            actionName = "self-test-six"
            createdAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
            expiresAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow.AddMinutes(1))
            executorSessionRef = $ExecutorRef
            reviewerSessionRef = $ReviewerRef
            inputDigestSha256 = Get-CanonicalObjectSha256 $SixContext
            context = $SixContext
            allowedResponse = [ordered]@{ type = "ordered-six-crop-review" }
            instruction = "Return six exact ordered verdicts, including unsafe findings."
        }
        $SixDecisionEntries = @()
        foreach ($Expected in $SixEntries) {
            $SixDecisionEntries += [ordered]@{
                sequence = $Expected.sequence; purpose = $Expected.purpose; image = $Expected.image
                sha256 = $Expected.sha256; width = $Expected.width; height = $Expected.height
                requiredVisibleStateSha256 = $Expected.requiredVisibleStateSha256
                digestMatched = $true
                requiredStateVerdict = $(if ($Expected.sequence -eq 1) { "fail" } else { "pass" })
                sensitivePixelsObserved = $Expected.sequence -eq 1
                uncertain = $Expected.sequence -eq 1
            }
        }
        $UnsafeSixJson = ([ordered]@{
            entries = $SixDecisionEntries
            aggregate = [ordered]@{
                reviewedCropCount = 6
                everySanitizedCropOpenedByReviewer = $true
                allImageDigestsMatched = $true
                requiredVisibleStateConfirmedByReviewer = $false
                noSensitivePixelsObservedByReviewer = $false
                noUncertaintyReported = $false
                visualJudgmentNotPixelSafetyProof = $true
            }
        } | ConvertTo-Json -Depth 12 -Compress)
        [void](Invoke-SelfTestPublication `
            $SixRequest "independent-agent" $ReviewerRef $UnsafeSixJson)
        foreach ($SixMutation in @("string-sequence", "string-bool", "string-count")) {
            $BadSix = ConvertFrom-JsonPreservingStrings $UnsafeSixJson
            switch ($SixMutation) {
                "string-sequence" { $BadSix.entries[0].sequence = "1" }
                "string-bool" { $BadSix.entries[0].digestMatched = "True" }
                "string-count" { $BadSix.aggregate.reviewedCropCount = "6" }
            }
            $BadSixRejected = $false
            try { Assert-SixCropDecision $BadSix @($SixEntries) }
            catch { $BadSixRejected = $true }
            if (-not $BadSixRejected) {
                throw "Responder self-test accepted a string-typed six-crop field."
            }
        }

        foreach ($UnableKind in @(
            [pscustomobject]@{ Request = $Request; Responder = "independent-agent"; Session = $ReviewerRef },
            [pscustomobject]@{
                Request = $ApprovalRequest; Responder = "user-via-orchestrator"; Session = $OrchestratorRef
            },
            [pscustomobject]@{ Request = $CropRequest; Responder = "independent-agent"; Session = $ReviewerRef },
            [pscustomobject]@{ Request = $SixRequest; Responder = "independent-agent"; Session = $ReviewerRef }
        )) {
            $UnableCopy = ConvertFrom-JsonPreservingStrings `
                ($UnableKind.Request | ConvertTo-Json -Depth 30)
            $UnableCopy.requestId = [Guid]::NewGuid().ToString("N")
            [void](Invoke-SelfTestPublication $UnableCopy $UnableKind.Responder `
                $UnableKind.Session '{"unable":true}')
        }

        $RoleMismatch = ConvertFrom-JsonPreservingStrings ($Request | ConvertTo-Json -Depth 30)
        $RoleMismatch.requestId = [String]::new([char]"d", 32)
        $script:RequestPath = Write-SelfTestRequest $RoleMismatch
        $script:ResponderKind = "user-via-orchestrator"
        $script:ResponderSessionRef = $OrchestratorRef
        $script:DecisionJson = '{"index":0}'
        $RoleRejected = $false
        try { Invoke-Respond | Out-Null } catch { $RoleRejected = $true }
        if (-not $RoleRejected) { throw "Responder self-test accepted a responder-role mismatch." }

        $StaleRequest = ConvertFrom-JsonPreservingStrings ($Request | ConvertTo-Json -Depth 30)
        $StaleRequest.requestId = [String]::new([char]"e", 32)
        $StaleRequest.createdAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow.AddMinutes(-2))
        $StaleRequest.expiresAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow.AddMinutes(-1))
        $script:RequestPath = Write-SelfTestRequest $StaleRequest
        $script:ResponderKind = "independent-agent"
        $script:ResponderSessionRef = $ReviewerRef
        $script:DecisionJson = '{"index":0}'
        $StaleRejected = $false
        try { Invoke-Respond | Out-Null } catch { $StaleRejected = $true }
        if (-not $StaleRejected) { throw "Responder self-test accepted an expired request." }

        $PublicationExpiryRejected = $false
        try {
            [void](Get-FreshResponsePublicationTime `
                ([DateTimeOffset]::UtcNow.AddMinutes(-2)) `
                ([DateTimeOffset]::UtcNow.AddMinutes(-1)))
        }
        catch { $PublicationExpiryRejected = $true }
        if (-not $PublicationExpiryRejected) {
            throw "Responder self-test accepted publication after request expiry."
        }

        $FirstRef = New-OpaqueSessionRef
        $SecondRef = New-OpaqueSessionRef
        if ($FirstRef -cnotmatch '^[0-9a-f]{64}$' -or
            $SecondRef -cnotmatch '^[0-9a-f]{64}$' -or $FirstRef -ceq $SecondRef) {
            throw "Responder self-test did not create distinct CSPRNG session references."
        }
        foreach ($RelayIteration in 1..2) {
            $RelayName = "lbb-gh-" + [Guid]::NewGuid().ToString("N")
            $RelayClient = [IO.Pipes.NamedPipeClientStream]::new(
                ".", $RelayName, [IO.Pipes.PipeDirection]::In,
                [IO.Pipes.PipeOptions]::Asynchronous
            )
            $ConnectTask = $RelayClient.ConnectAsync(5000)
            $DummyBytes = [Text.Encoding]::ASCII.GetBytes(
                "github_pat_self_test_value_1234567890`n"
            )
            $RelayInput = [IO.MemoryStream]::new($DummyBytes, $false)
            $Received = New-Object byte[] $DummyBytes.Length
            try {
                Invoke-GitHubTokenRelay $RelayName $RelayInput 5
                if (-not $ConnectTask.Wait(5000) -or -not $RelayClient.IsConnected) {
                    throw "Responder self-test token relay client did not connect."
                }
                $Offset = 0
                while ($Offset -lt $Received.Length) {
                    $Read = $RelayClient.Read($Received, $Offset, $Received.Length - $Offset)
                    if ($Read -le 0) { break }
                    $Offset += $Read
                }
                if ($Offset -ne $DummyBytes.Length) {
                    throw "Responder self-test token relay truncated its bounded input."
                }
                for ($Index = 0; $Index -lt $DummyBytes.Length; $Index += 1) {
                    if ($Received[$Index] -ne $DummyBytes[$Index]) {
                        throw "Responder self-test token relay changed its bounded input."
                    }
                }
            }
            finally {
                $RelayInput.Dispose()
                $RelayClient.Dispose()
                if (-not $ConnectTask.IsCompleted) {
                    try { [void]$ConnectTask.Wait(1000) } catch {}
                }
                [Array]::Clear($Received, 0, $Received.Length)
                [Array]::Clear($DummyBytes, 0, $DummyBytes.Length)
            }
        }

        foreach ($EscapingName in @("..\outside.png", "/outside.png", "C:\outside.png")) {
            $PathRejected = $false
            try {
                [void](Resolve-ExchangeImageLeaf $Root $EscapingName "self-test escaping image")
            }
            catch { $PathRejected = $true }
            if (-not $PathRejected) {
                throw "Responder self-test accepted an absolute or traversal image path."
            }
        }
    }
    finally {
        if ([IO.Directory]::Exists($Root)) { [IO.Directory]::Delete($Root, $true) }
    }
    Write-Output "Stock-Chrome operator-response writer self-test passed."
}

if ($Mode -ceq "SelfTest") {
    Invoke-SelfTest
    return
}
if ($Mode -ceq "AttestExternalSurfaces") {
    Invoke-AttestExternalSurfaces
    return
}
if ($Mode -ceq "NewSessionRef") {
    Write-Output (New-OpaqueSessionRef)
    return
}
if ($Mode -ceq "RelayGitHubToken") {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "The GitHub token relay runs only on Windows."
    }
    if ([String]::IsNullOrWhiteSpace($GitHubTokenPipeName)) {
        throw "GitHubTokenPipeName is required for RelayGitHubToken."
    }
    $StandardInput = [Console]::OpenStandardInput()
    try {
        Invoke-GitHubTokenRelay $GitHubTokenPipeName $StandardInput $RelayTimeoutSeconds
    }
    finally { $StandardInput.Dispose() }
    Write-Output "GitHub token relay completed without retaining credential bytes."
    return
}
Invoke-Respond
