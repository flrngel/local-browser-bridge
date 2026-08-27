#requires -Version 5.1

param(
  [string]$Version,
  [Alias("RunId")]
  [string]$WorkflowRunId,
  [Alias("RunAttempt")]
  [string]$WorkflowRunAttempt,
  [Alias("ReleaseCandidateArtifactId")]
  [string]$ArtifactId,
  [Alias("FinalSha")]
  [string]$SourceSha,
  [string]$Destination,
  [string]$TrustedGit,
  [string]$TrustedGh,
  [switch]$SelfTest,
  [string]$CleanCoordinatorNonce
)

# This trust gate deliberately bootstraps itself into the exact 64-bit Windows
# PowerShell system binary. All caller-supplied values cross that boundary in
# the environment so path quoting cannot alter the child command line.
$RegistryView = $(if ([Environment]::Is64BitOperatingSystem) {
  [Microsoft.Win32.RegistryView]::Registry64
} else {
  [Microsoft.Win32.RegistryView]::Registry32
})
$LocalMachine = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
  [Microsoft.Win32.RegistryHive]::LocalMachine,
  $RegistryView
)
$WindowsNt = $null
try {
  $WindowsNt = $LocalMachine.OpenSubKey("SOFTWARE\Microsoft\Windows NT\CurrentVersion", $false)
  if ($null -eq $WindowsNt) { throw "The Windows NT machine registry key is unavailable." }
  $MachineSystemRoot = [string]$WindowsNt.GetValue(
    "SystemRoot",
    $null,
    [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
  )
}
finally {
  if ($null -ne $WindowsNt) { $WindowsNt.Dispose() }
  $LocalMachine.Dispose()
}
if ([String]::IsNullOrWhiteSpace($MachineSystemRoot) -or
    -not [IO.Path]::IsPathRooted($MachineSystemRoot)) {
  throw "The machine Windows SystemRoot is unavailable or invalid."
}
$System32PowerShell = [IO.Path]::GetFullPath([IO.Path]::Combine(
  $MachineSystemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"
))
$LaunchPowerShell = $System32PowerShell
if ([Environment]::Is64BitOperatingSystem -and -not [Environment]::Is64BitProcess) {
  $LaunchPowerShell = [IO.Path]::GetFullPath([IO.Path]::Combine(
    $MachineSystemRoot, "Sysnative", "WindowsPowerShell", "v1.0", "powershell.exe"
  ))
}

function Assert-OrdinaryAbsolutePath([string]$Path, [bool]$RequireFile, [string]$Description) {
  if ([String]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathRooted($Path)) {
    throw "$Description must be an absolute path."
  }
  $Full = [IO.Path]::GetFullPath($Path)
  if ($RequireFile) {
    if (-not [IO.File]::Exists($Full)) { throw "$Description does not exist." }
    $Current = [IO.FileInfo]::new($Full)
  }
  else {
    if (-not [IO.Directory]::Exists($Full)) { throw "$Description does not exist." }
    $Current = [IO.DirectoryInfo]::new($Full)
  }
  while ($null -ne $Current) {
    if ($Current.Attributes -band [IO.FileAttributes]::ReparsePoint) {
      throw "$Description must not traverse a reparse point."
    }
    if ($Current -is [IO.FileInfo]) { $Current = $Current.Directory }
    else { $Current = $Current.Parent }
  }
  return $Full
}

if ([String]::IsNullOrWhiteSpace($CleanCoordinatorNonce)) {
  if ([String]::IsNullOrWhiteSpace($PSCommandPath)) {
    throw "The trust wrapper must be launched from an ordinary script file."
  }
  $ScriptForChild = Assert-OrdinaryAbsolutePath $PSCommandPath $true "Trust wrapper"
  $PowerShellForChild = Assert-OrdinaryAbsolutePath $LaunchPowerShell $true "System powershell.exe"
  if ([IO.Path]::GetFileName($PowerShellForChild) -cne "powershell.exe") {
    throw "The clean child executable is not powershell.exe."
  }
  $Nonce = [Guid]::NewGuid().ToString("N")
  $Info = [Diagnostics.ProcessStartInfo]::new()
  $Info.FileName = $PowerShellForChild
  $Info.WorkingDirectory = [IO.Path]::GetDirectoryName($PowerShellForChild)
  $Info.Arguments = '-NoLogo -NoProfile -File "' + $ScriptForChild + '" -CleanCoordinatorNonce ' + $Nonce
  $Info.UseShellExecute = $false
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_NONCE"] = $Nonce
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_SELF_TEST"] = $(if ($SelfTest) { "1" } else { "0" })
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_VERSION"] = $Version
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_RUN_ID"] = $WorkflowRunId
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_RUN_ATTEMPT"] = $WorkflowRunAttempt
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_ARTIFACT_ID"] = $ArtifactId
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_SOURCE_SHA"] = $SourceSha
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_DESTINATION"] = $Destination
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_GIT"] = $TrustedGit
  $Info.EnvironmentVariables["LBB_WINDOWS_TRUST_GH"] = $TrustedGh
  $Child = [Diagnostics.Process]::new()
  $Child.StartInfo = $Info
  try {
    if (-not $Child.Start()) { throw "The clean trust child did not start." }
    $Child.WaitForExit()
    $ChildExitCode = $Child.ExitCode
  }
  finally {
    $Child.Dispose()
  }
  exit $ChildExitCode
}

$ExpectedNonce = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_NONCE", "Process")
$ActualExecutable = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
if ($CleanCoordinatorNonce -cnotmatch '^[0-9a-f]{32}$' -or
    $CleanCoordinatorNonce -cne $ExpectedNonce -or
    -not [Environment]::Is64BitProcess -or
    -not [String]::Equals(
      [IO.Path]::GetFullPath($ActualExecutable),
      $System32PowerShell,
      [StringComparison]::OrdinalIgnoreCase
    )) {
  throw "The trust body must run only in its exact self-spawned 64-bit system powershell.exe child."
}

$SelfTestRequested = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_SELF_TEST", "Process") -ceq "1"
$Version = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_VERSION", "Process")
$WorkflowRunId = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_RUN_ID", "Process")
$WorkflowRunAttempt = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_RUN_ATTEMPT", "Process")
$ArtifactId = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_ARTIFACT_ID", "Process")
$SourceSha = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_SOURCE_SHA", "Process")
$Destination = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_DESTINATION", "Process")
$TrustedGit = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_GIT", "Process")
$TrustedGh = [Environment]::GetEnvironmentVariable("LBB_WINDOWS_TRUST_GH", "Process")
foreach ($HandoffName in @(
  "LBB_WINDOWS_TRUST_NONCE", "LBB_WINDOWS_TRUST_SELF_TEST",
  "LBB_WINDOWS_TRUST_VERSION", "LBB_WINDOWS_TRUST_RUN_ID",
  "LBB_WINDOWS_TRUST_RUN_ATTEMPT", "LBB_WINDOWS_TRUST_ARTIFACT_ID",
  "LBB_WINDOWS_TRUST_SOURCE_SHA",
  "LBB_WINDOWS_TRUST_DESTINATION", "LBB_WINDOWS_TRUST_GIT", "LBB_WINDOWS_TRUST_GH"
)) {
  [Environment]::SetEnvironmentVariable($HandoffName, $null, "Process")
}

$TrustedModuleRoot = [IO.Path]::GetFullPath([IO.Path]::Combine($PSHOME, "Modules"))
$null = Assert-OrdinaryAbsolutePath $TrustedModuleRoot $false "Windows PowerShell module root"
[Environment]::SetEnvironmentVariable("PSModulePath", $TrustedModuleRoot, "Process")
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Repository = "flrngel/local-browser-bridge"
$Origin = "https://github.com/$Repository.git"
$WorkflowPath = ".github/workflows/deploy.yml"
$ProductVersion = "0.12.40"
$WorkflowRef = "refs/heads/main"
$MaximumCandidateBytes = [int64]536870912

function Get-TrustedSha256([string]$Path) {
  $Stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
  try {
    $Hasher = [Security.Cryptography.SHA256]::Create()
    try {
      return ([BitConverter]::ToString($Hasher.ComputeHash($Stream))).Replace("-", "").ToLowerInvariant()
    }
    finally { $Hasher.Dispose() }
  }
  finally { $Stream.Dispose() }
}

function ConvertTo-WindowsCommandLineArgument([string]$Value) {
  if ($null -eq $Value -or $Value.IndexOf([char]0) -ge 0 -or
      $Value.IndexOf([char]13) -ge 0 -or $Value.IndexOf([char]10) -ge 0) {
    throw "A trusted child argument contains a forbidden character."
  }
  $Builder = [Text.StringBuilder]::new()
  [void]$Builder.Append([char]34)
  $Backslashes = 0
  foreach ($Character in $Value.ToCharArray()) {
    if ($Character -eq [char]92) {
      $Backslashes += 1
      continue
    }
    if ($Character -eq [char]34) {
      [void]$Builder.Append(([string][char]92) * (($Backslashes * 2) + 1))
      [void]$Builder.Append([char]34)
      $Backslashes = 0
      continue
    }
    if ($Backslashes -gt 0) {
      [void]$Builder.Append(([string][char]92) * $Backslashes)
      $Backslashes = 0
    }
    [void]$Builder.Append($Character)
  }
  if ($Backslashes -gt 0) {
    [void]$Builder.Append(([string][char]92) * ($Backslashes * 2))
  }
  [void]$Builder.Append([char]34)
  return $Builder.ToString()
}

function Join-TrustedArguments([string[]]$Arguments) {
  return (($Arguments | ForEach-Object { ConvertTo-WindowsCommandLineArgument ([string]$_) }) -join " ")
}

function Assert-Pe32PlusX64([string]$Path) {
  $Stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
  $Reader = [IO.BinaryReader]::new($Stream)
  try {
    if ($Stream.Length -lt 256 -or $Reader.ReadUInt16() -ne 0x5a4d) {
      throw "Windows candidate is not an MZ executable."
    }
    $Stream.Position = 0x3c
    $PeOffset = $Reader.ReadUInt32()
    if ($PeOffset -lt 0x40 -or [int64]$PeOffset + 26 -gt $Stream.Length) {
      throw "Windows candidate has an invalid PE header offset."
    }
    $Stream.Position = $PeOffset
    if ($Reader.ReadUInt32() -ne 0x00004550) { throw "Windows candidate lacks a PE signature." }
    if ($Reader.ReadUInt16() -ne 0x8664) { throw "Windows candidate is not PE x86-64." }
    $SectionCount = $Reader.ReadUInt16()
    $Stream.Position = [int64]$PeOffset + 20
    $OptionalHeaderBytes = $Reader.ReadUInt16()
    if ($SectionCount -lt 1 -or $OptionalHeaderBytes -lt 2 -or
        [int64]$PeOffset + 24 + $OptionalHeaderBytes -gt $Stream.Length) {
      throw "Windows candidate has an invalid COFF header."
    }
    $Stream.Position = [int64]$PeOffset + 24
    if ($Reader.ReadUInt16() -ne 0x020b) { throw "Windows candidate is not PE32+." }
  }
  finally {
    $Reader.Dispose()
    $Stream.Dispose()
  }
}

function Get-CanonicalManifestRows([string]$Manifest, [string[]]$ExpectedAssets) {
  $Bytes = [IO.File]::ReadAllBytes($Manifest)
  if ($Bytes.Length -lt 1 -or $Bytes[$Bytes.Length - 1] -ne 0x0a) {
    throw "SHA256SUMS.txt must end in one LF byte."
  }
  foreach ($Byte in $Bytes) {
    if ($Byte -eq 0x0d -or ($Byte -ne 0x0a -and ($Byte -lt 0x20 -or $Byte -gt 0x7e))) {
      throw "SHA256SUMS.txt must contain canonical printable ASCII and LF bytes only."
    }
  }
  $Text = [Text.Encoding]::ASCII.GetString($Bytes)
  $Lines = $Text.Split([char]0x0a)
  if ($Lines.Count -ne 5 -or $Lines[4] -cne "") {
    throw "SHA256SUMS.txt must contain exactly four LF-terminated rows."
  }
  $Rows = New-Object Collections.Generic.List[object]
  for ($Index = 0; $Index -lt 4; $Index += 1) {
    if ($Lines[$Index] -cnotmatch '^([0-9a-f]{64})  ([A-Za-z0-9._-]+)$' -or
        $Matches[2] -cne $ExpectedAssets[$Index] -or
        $Lines[$Index].Length -ne (66 + $ExpectedAssets[$Index].Length)) {
      throw "SHA256SUMS.txt row order or spelling is not canonical."
    }
    $Rows.Add([pscustomobject]@{ sha256 = $Matches[1]; file = $Matches[2] })
  }
  return @($Rows | ForEach-Object { $_ })
}

function Get-ByteArraySha256([byte[]]$Bytes) {
  $Hasher = [Security.Cryptography.SHA256]::Create()
  try { return ([BitConverter]::ToString($Hasher.ComputeHash($Bytes))).Replace("-", "").ToLowerInvariant() }
  finally { $Hasher.Dispose() }
}

function Read-ZipEntryBytesBounded([object]$Entry, [int64]$MaximumBytes) {
  if ($Entry.Length -lt 0 -or $Entry.Length -gt $MaximumBytes -or $Entry.Length -gt [int]::MaxValue) {
    throw "Archive metadata entry exceeds its bounded size."
  }
  $Input = $Entry.Open()
  $Memory = [IO.MemoryStream]::new([int]$Entry.Length)
  try {
    $Input.CopyTo($Memory)
    if ($Memory.Length -ne $Entry.Length) { throw "Archive metadata entry length changed while reading." }
    return ,$Memory.ToArray()
  }
  finally {
    $Memory.Dispose()
    $Input.Dispose()
  }
}

function Assert-ZipEntryReadable([object]$Entry, [int64]$MaximumBytes) {
  if ($Entry.Length -lt 0 -or $Entry.Length -gt $MaximumBytes) {
    throw "Archive entry exceeds its bounded readable size."
  }
  $Input = $Entry.Open()
  $Buffer = New-Object byte[] 65536
  $Observed = [int64]0
  try {
    while (($Read = $Input.Read($Buffer, 0, $Buffer.Length)) -gt 0) {
      if ($Observed -gt ($MaximumBytes - $Read)) { throw "Archive entry expanded beyond its bound." }
      $Observed += $Read
    }
    if ($Observed -ne $Entry.Length) { throw "Archive entry did not expand to its declared length." }
  }
  finally { $Input.Dispose() }
}

function Copy-ZipEntryBounded([object]$Entry, [IO.Stream]$Output, [int64]$MaximumBytes) {
  if ($Entry.Length -lt 0 -or $Entry.Length -gt $MaximumBytes) {
    throw "Archive entry exceeds its bounded extraction size."
  }
  $Input = $Entry.Open()
  $Buffer = New-Object byte[] 65536
  $Observed = [int64]0
  try {
    while (($Read = $Input.Read($Buffer, 0, $Buffer.Length)) -gt 0) {
      if ($Observed -gt ($MaximumBytes - $Read) -or $Observed -gt ($Entry.Length - $Read)) {
        throw "Archive entry expanded beyond its declared or allowed size."
      }
      $Output.Write($Buffer, 0, $Read)
      $Observed += $Read
    }
    if ($Observed -ne $Entry.Length) { throw "Archive entry did not match its declared length." }
  }
  finally { $Input.Dispose() }
}

function Assert-ExtensionArchive(
  [string]$ArchivePath,
  [string]$ExpectedVersion,
  [string]$SourceLicense,
  [int64]$MaximumBytes
) {
  $ExpectedEntries = @(
    "background.js", "content.js", "dom-core.js", "frame-agent.js", "lib.js",
    "manifest.json", "popup.css", "popup.html", "popup.js", "stop-guard.js", "LICENSE"
  )
  $Archive = [IO.Compression.ZipFile]::OpenRead($ArchivePath)
  try {
    $Entries = @($Archive.Entries)
    $TotalBytes = [int64]0
    foreach ($Entry in $Entries) {
      $ExternalAttributes = [BitConverter]::ToUInt32(
        [BitConverter]::GetBytes([int32]$Entry.ExternalAttributes), 0
      )
      $UnixType = ($ExternalAttributes -shr 16) -band 0xf000
      if ($Entry.Length -lt 1 -or $Entry.Length -gt $MaximumBytes -or
          $TotalBytes -gt ($MaximumBytes - $Entry.Length) -or
          $Entry.Name -cne $Entry.FullName -or $ExpectedEntries -cnotcontains $Entry.FullName -or
          (($UnixType -ne 0) -and ($UnixType -ne 0x8000)) -or
          (($ExternalAttributes -band 0x10) -ne 0)) {
        throw "Extension archive contains an unexpected, linked, or oversized entry."
      }
      $TotalBytes += $Entry.Length
    }
    if ($Entries.Count -ne $ExpectedEntries.Count) {
      throw "Extension archive does not contain the exact expected file count."
    }
    foreach ($Name in $ExpectedEntries) {
      if (@($Entries | Where-Object { $_.FullName -ceq $Name }).Count -ne 1) {
        throw "Extension archive inventory is missing or ambiguous."
      }
    }
    foreach ($Entry in $Entries) { Assert-ZipEntryReadable $Entry $MaximumBytes }
    $LicenseEntry = @($Entries | Where-Object { $_.FullName -ceq "LICENSE" })[0]
    $ManifestEntry = @($Entries | Where-Object { $_.FullName -ceq "manifest.json" })[0]
    $LibraryEntry = @($Entries | Where-Object { $_.FullName -ceq "lib.js" })[0]
    $LicenseBytes = Read-ZipEntryBytesBounded $LicenseEntry 4194304
    if ((Get-ByteArraySha256 $LicenseBytes) -cne (Get-TrustedSha256 $SourceLicense)) {
      throw "Extension archive project license differs from the exact source license."
    }
    $ManifestBytes = Read-ZipEntryBytesBounded $ManifestEntry 4194304
    $LibraryBytes = Read-ZipEntryBytesBounded $LibraryEntry 4194304
    try {
      $Utf8 = [Text.UTF8Encoding]::new($false, $true)
      $ManifestObject = $Utf8.GetString($ManifestBytes) | ConvertFrom-Json
      $LibraryText = $Utf8.GetString($LibraryBytes).Replace("`r`n", "`n")
    }
    catch { throw "Extension version sources are not valid UTF-8 JSON and JavaScript." }
    if ([string]$ManifestObject.version -cne $ExpectedVersion -or
        @($LibraryText.Split([char]0x0a) | Where-Object {
          $_ -ceq "export const VERSION = `"$ExpectedVersion`";"
        }).Count -ne 1) {
      throw "Extension archive version does not match the release candidate."
    }
  }
  finally { $Archive.Dispose() }
}

function Read-ExactStreamBytes([IO.Stream]$Stream, [int]$Count, [bool]$AllowCleanEof = $false) {
  if ($Count -lt 0) { throw "A negative bounded stream read was refused." }
  $Bytes = New-Object byte[] $Count
  $Offset = 0
  while ($Offset -lt $Count) {
    $Read = $Stream.Read($Bytes, $Offset, $Count - $Offset)
    if ($Read -eq 0) {
      if ($AllowCleanEof -and $Offset -eq 0) { return $null }
      throw "Archive stream ended before its declared boundary."
    }
    $Offset += $Read
  }
  return ,$Bytes
}

function Test-ZeroBytes([byte[]]$Bytes) {
  foreach ($Byte in $Bytes) { if ($Byte -ne 0) { return $false } }
  return $true
}

function Get-TarAscii([byte[]]$Header, [int]$Offset, [int]$Count) {
  $End = $Offset
  while ($End -lt ($Offset + $Count) -and $Header[$End] -ne 0) {
    if ($Header[$End] -lt 0x20 -or $Header[$End] -gt 0x7e) {
      throw "macOS tar header contains a noncanonical text field."
    }
    $End += 1
  }
  return [Text.Encoding]::ASCII.GetString($Header, $Offset, $End - $Offset)
}

function Get-TarOctal([byte[]]$Header, [int]$Offset, [int]$Count) {
  $Raw = (Get-TarAscii $Header $Offset $Count).Trim([char[]]@([char]0, [char]0x20))
  if ($Raw -cnotmatch '^[0-7]+$') { throw "macOS tar header contains a noncanonical octal field." }
  try { return [Convert]::ToInt64($Raw, 8) }
  catch { throw "macOS tar octal field exceeds its numeric bound." }
}

function Assert-SafePaxPayload([byte[]]$Payload) {
  $AllowedKeys = @(
    "mtime", "LIBARCHIVE.xattr.com.apple.provenance", "SCHILY.xattr.com.apple.provenance"
  )
  $Offset = 0
  while ($Offset -lt $Payload.Length) {
    $Space = $Offset
    while ($Space -lt $Payload.Length -and $Payload[$Space] -ne 0x20) {
      if ($Payload[$Space] -lt 0x30 -or $Payload[$Space] -gt 0x39) {
        throw "macOS tar PAX record length is not canonical decimal."
      }
      $Space += 1
    }
    if ($Space -eq $Offset -or $Space -ge $Payload.Length) {
      throw "macOS tar PAX record lacks a bounded length prefix."
    }
    $LengthText = [Text.Encoding]::ASCII.GetString($Payload, $Offset, $Space - $Offset)
    if ($LengthText -cnotmatch '^[1-9][0-9]*$') { throw "macOS tar PAX record length is not canonical." }
    $RecordLength = [int64]$LengthText
    if ($RecordLength -lt 5 -or $RecordLength -gt ($Payload.Length - $Offset)) {
      throw "macOS tar PAX record exceeds its payload."
    }
    $RecordEnd = $Offset + [int]$RecordLength
    if ($Payload[$RecordEnd - 1] -ne 0x0a) { throw "macOS tar PAX record lacks terminal LF." }
    $Equals = $Space + 1
    while ($Equals -lt ($RecordEnd - 1) -and $Payload[$Equals] -ne 0x3d) { $Equals += 1 }
    if ($Equals -eq ($Space + 1) -or $Equals -ge ($RecordEnd - 1)) {
      throw "macOS tar PAX record lacks a key/value boundary."
    }
    for ($Index = $Space + 1; $Index -lt $Equals; $Index += 1) {
      if ($Payload[$Index] -lt 0x21 -or $Payload[$Index] -gt 0x7e) {
        throw "macOS tar PAX key is not printable ASCII."
      }
    }
    $Key = [Text.Encoding]::ASCII.GetString($Payload, $Space + 1, $Equals - $Space - 1)
    if ($AllowedKeys -cnotcontains $Key) {
      throw "macOS tar PAX metadata could alter extraction semantics."
    }
    $Offset = $RecordEnd
  }
}

function Read-TarEntryData([IO.Stream]$Stream, [int64]$Size, [bool]$Capture) {
  if ($Size -lt 0) { throw "macOS tar entry has a negative size." }
  if ($Capture -and $Size -gt 4194304) { throw "macOS tar metadata entry exceeds four MiB." }
  $Memory = $(if ($Capture) { [IO.MemoryStream]::new([int]$Size) } else { $null })
  $FirstFour = New-Object byte[] 4
  $FirstCount = 0
  $Buffer = New-Object byte[] 65536
  $Remaining = $Size
  try {
    while ($Remaining -gt 0) {
      $Wanted = [int][Math]::Min([int64]$Buffer.Length, $Remaining)
      $Read = $Stream.Read($Buffer, 0, $Wanted)
      if ($Read -eq 0) { throw "macOS tar entry ended before its declared size." }
      if ($FirstCount -lt 4) {
        $Copy = [Math]::Min(4 - $FirstCount, $Read)
        [Array]::Copy($Buffer, 0, $FirstFour, $FirstCount, $Copy)
        $FirstCount += $Copy
      }
      if ($Capture) { $Memory.Write($Buffer, 0, $Read) }
      $Remaining -= $Read
    }
    $Padding = [int]((512 - ($Size % 512)) % 512)
    if ($Padding -gt 0) {
      $PaddingBytes = Read-ExactStreamBytes $Stream $Padding
      if (-not (Test-ZeroBytes $PaddingBytes)) { throw "macOS tar entry padding is not zero-filled." }
    }
    return [pscustomobject]@{
      firstFour = $FirstFour
      firstCount = $FirstCount
      bytes = $(if ($Capture) { $Memory.ToArray() } else { $null })
    }
  }
  finally { if ($null -ne $Memory) { $Memory.Dispose() } }
}

function Assert-MacosArchive(
  [string]$ArchivePath,
  [string]$ExpectedVersion,
  [string]$SourceRoot,
  [int64]$MaximumBytes
) {
  $ExpectedEntries = @(
    "local-browser-bridge",
    "Local Computer Helper.app",
    "Local Computer Helper.app/Contents",
    "Local Computer Helper.app/Contents/Info.plist",
    "Local Computer Helper.app/Contents/MacOS",
    "Local Computer Helper.app/Contents/MacOS/local-computer-helper",
    "Local Computer Helper.app/Contents/_CodeSignature",
    "Local Computer Helper.app/Contents/_CodeSignature/CodeResources",
    "LICENSE", "THIRD_PARTY_LICENSES.txt"
  )
  $ExpectedDirectories = @(
    "Local Computer Helper.app", "Local Computer Helper.app/Contents",
    "Local Computer Helper.app/Contents/MacOS",
    "Local Computer Helper.app/Contents/_CodeSignature"
  )
  $Input = [IO.File]::Open($ArchivePath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
  $Gzip = [IO.Compression.GZipStream]::new($Input, [IO.Compression.CompressionMode]::Decompress)
  $LogicalEntries = New-Object Collections.Generic.List[string]
  $Captured = @{}
  $TotalEntryBytes = [int64]0
  $HeaderCount = 0
  $PendingPax = $false
  try {
    while ($true) {
      $Header = Read-ExactStreamBytes $Gzip 512 $true
      if ($null -eq $Header) { throw "macOS tar ended before its two zero terminators." }
      if (Test-ZeroBytes $Header) {
        $SecondZero = Read-ExactStreamBytes $Gzip 512
        if (-not (Test-ZeroBytes $SecondZero)) { throw "macOS tar has only one zero terminator." }
        $Trailing = New-Object byte[] 65536
        $TrailingBytes = [int64]0
        while (($Read = $Gzip.Read($Trailing, 0, $Trailing.Length)) -gt 0) {
          if ($TrailingBytes -gt (1048576 - $Read)) {
            throw "macOS tar contains more than one MiB of terminal padding."
          }
          $TrailingBytes += $Read
          for ($Index = 0; $Index -lt $Read; $Index += 1) {
            if ($Trailing[$Index] -ne 0) { throw "macOS tar contains nonzero data after its terminator." }
          }
        }
        break
      }
      $HeaderCount += 1
      if ($HeaderCount -gt 32) { throw "macOS tar contains too many physical headers." }
      $StoredChecksum = Get-TarOctal $Header 148 8
      $CalculatedChecksum = [int64]0
      for ($Index = 0; $Index -lt 512; $Index += 1) {
        $CalculatedChecksum += $(if ($Index -ge 148 -and $Index -lt 156) { 0x20 } else { $Header[$Index] })
      }
      if ($StoredChecksum -ne $CalculatedChecksum -or
          (Get-TarAscii $Header 257 6).Substring(0, 5) -cne "ustar") {
        throw "macOS tar header checksum or USTAR identity is invalid."
      }
      $Name = Get-TarAscii $Header 0 100
      $Prefix = Get-TarAscii $Header 345 155
      if (-not [String]::IsNullOrEmpty($Prefix)) { $Name = "$Prefix/$Name" }
      $Mode = Get-TarOctal $Header 100 8
      $Size = Get-TarOctal $Header 124 12
      $Type = [char]$Header[156]
      if ([int]$Type -eq 0) { $Type = [char]0x30 }
      $LinkName = Get-TarAscii $Header 157 100
      if (-not [String]::IsNullOrEmpty($LinkName) -or $Size -gt $MaximumBytes -or
          $TotalEntryBytes -gt ($MaximumBytes - $Size)) {
        throw "macOS tar entry link or size exceeds the candidate policy."
      }
      $TotalEntryBytes += $Size
      if ($Type -eq [char]0x78) {
        if ($PendingPax -or $Size -gt 65536) { throw "macOS tar PAX header is ambiguous or oversized." }
        $PaxData = Read-TarEntryData $Gzip $Size $true
        Assert-SafePaxPayload $PaxData.bytes
        $PendingPax = $true
        continue
      }
      $NormalizedName = $Name.TrimEnd('/')
      if ($ExpectedEntries -cnotcontains $NormalizedName -or
          @($LogicalEntries | Where-Object { $_ -ceq $NormalizedName }).Count -ne 0) {
        throw "macOS tar logical inventory is unexpected or duplicated."
      }
      $IsDirectory = $ExpectedDirectories -ccontains $NormalizedName
      if (($IsDirectory -and ($Type -ne [char]0x35 -or $Size -ne 0 -or -not $Name.EndsWith("/"))) -or
          (-not $IsDirectory -and ($Type -ne [char]0x30 -or $Name.EndsWith("/")))) {
        throw "macOS tar logical entry type does not match its exact path."
      }
      $Capture = $NormalizedName -in @("LICENSE", "THIRD_PARTY_LICENSES.txt", "Local Computer Helper.app/Contents/Info.plist")
      $Data = Read-TarEntryData $Gzip $Size $Capture
      if ($Capture) { $Captured[$NormalizedName] = $Data.bytes }
      if ($NormalizedName -in @("local-browser-bridge", "Local Computer Helper.app/Contents/MacOS/local-computer-helper")) {
        $Magic = ([BitConverter]::ToString($Data.firstFour)).Replace("-", "").ToLowerInvariant()
        if ($Data.firstCount -ne 4 -or $Magic -notin @("cafebabe", "cafebabf") -or ($Mode -band 0x49) -eq 0) {
          throw "macOS tar executable is not an executable universal Mach-O."
        }
      }
      $LogicalEntries.Add($NormalizedName)
      $PendingPax = $false
    }
  }
  finally {
    $Gzip.Dispose()
    $Input.Dispose()
  }
  if ($PendingPax -or $LogicalEntries.Count -ne $ExpectedEntries.Count) {
    throw "macOS tar did not terminate with its exact logical inventory."
  }
  foreach ($Name in $ExpectedEntries) {
    if (@($LogicalEntries | Where-Object { $_ -ceq $Name }).Count -ne 1) {
      throw "macOS tar is missing an exact logical entry."
    }
  }
  foreach ($Notice in @("LICENSE", "THIRD_PARTY_LICENSES.txt")) {
    if ((Get-ByteArraySha256 $Captured[$Notice]) -cne (Get-TrustedSha256 (Join-Path $SourceRoot $Notice))) {
      throw "macOS tar notice differs from the exact source notice."
    }
  }
  try { $PlistText = [Text.UTF8Encoding]::new($false, $true).GetString($Captured["Local Computer Helper.app/Contents/Info.plist"]) }
  catch { throw "macOS helper Info.plist is not valid UTF-8." }
  if ([regex]::Matches($PlistText, [regex]::Escape("<string>$ExpectedVersion</string>")).Count -lt 2) {
    throw "macOS helper Info.plist does not bind the release version twice."
  }
}

function Write-CreateOnceUtf8Json([string]$Path, [object]$Value) {
  $Json = ($Value | ConvertTo-Json -Depth 8 -Compress) + "`n"
  $Bytes = [Text.UTF8Encoding]::new($false).GetBytes($Json)
  $Stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try { $Stream.Write($Bytes, 0, $Bytes.Length) }
  finally { $Stream.Dispose() }
}

# GitHub can return more than one valid attestation when a workflow rerun
# reproduces byte-identical assets. Historical entries are admissible only
# when they remain fully valid for the same run and exact subject. Exactly one
# entry must bind the expected attempt.
# BEGIN EXACT_ATTEMPT_ATTESTATION_SELECTOR
function Assert-ExactAttemptAttestationSet(
  [object[]]$Attestations,
  [string]$ExpectedInvocationUri,
  [string]$WorkflowRunId,
  [string]$WorkflowPath,
  [string]$Repository,
  [string]$TagRef,
  [string]$SourceSha,
  [string]$SubjectName,
  [string]$SubjectSha256
) {
  # Windows PowerShell 5.1 emits a top-level JSON array as one non-enumerated
  # Object[]; @(... | ConvertFrom-Json) consequently adds one wrapper layer.
  # Remove exactly that compatibility wrapper while retaining strict nested
  # array validation below.
  if ($null -ne $Attestations -and $Attestations.Count -eq 1 -and
      $Attestations[0] -is [Array]) {
    $Attestations = [object[]]$Attestations[0]
  }
  $SameRunInvocationPrefix = "https://github.com/$Repository/actions/runs/$WorkflowRunId/attempts/"
  $ExpectedAttemptSuffix = $(if ($null -ne $ExpectedInvocationUri -and
      $ExpectedInvocationUri.StartsWith($SameRunInvocationPrefix, [StringComparison]::Ordinal)) {
    $ExpectedInvocationUri.Substring($SameRunInvocationPrefix.Length)
  } else { "" })
  if ($null -eq $Attestations -or $Attestations.Count -lt 1 -or
      $ExpectedAttemptSuffix -cnotmatch '^[1-9][0-9]*$' -or
      [String]::IsNullOrWhiteSpace($WorkflowPath) -or
      [String]::IsNullOrWhiteSpace($SubjectName) -or
      $SourceSha -cnotmatch '^[0-9a-f]{40}$' -or
      $SubjectSha256 -cnotmatch '^[0-9a-f]{64}$') {
    throw "GitHub attestation selection inputs are invalid."
  }
  $CurrentAttemptCount = 0
  foreach ($Attestation in $Attestations) {
    $EntryIsValid = $false
    $EntryInvocation = $null
    try {
      if ($null -eq $Attestation -or $Attestation -is [Array] -or
          $Attestation -is [string] -or
          $Attestation -is [ValueType]) {
        throw "Attestation entry is not an object."
      }
      $Verification = $Attestation.verificationResult
      $Statement = $Verification.statement
      $Predicate = $Statement.predicate
      $BuildDefinition = $Predicate.buildDefinition
      $Workflow = $BuildDefinition.externalParameters.workflow
      $Certificate = $Verification.signature.certificate
      if ($null -eq $Verification -or $null -eq $Statement -or
          $null -eq $Predicate -or $null -eq $BuildDefinition -or
          $null -eq $Workflow -or $null -eq $Certificate -or
          $Statement.subject -isnot [Array]) {
        throw "Attestation entry is missing a required object or array."
      }
      $AllSubjects = @($Statement.subject)
      if ($AllSubjects.Count -lt 1) { throw "Attestation subject array is empty." }
      foreach ($Subject in $AllSubjects) {
        if ($null -eq $Subject -or $Subject -is [string] -or
            $Subject -is [ValueType] -or $Subject.name -isnot [string] -or
            $null -eq $Subject.digest -or $Subject.digest.sha256 -isnot [string] -or
            [string]$Subject.digest.sha256 -cnotmatch '^[0-9a-f]{64}$') {
          throw "Attestation subject shape is invalid."
        }
      }
      $MatchingSubjects = @($AllSubjects | Where-Object {
        $_.name -ceq $SubjectName -and $_.digest.sha256 -ceq $SubjectSha256
      })
      $EntryInvocation = $Predicate.runDetails.metadata.invocationId
      $CertificateInvocation = $Certificate.runInvocationURI
      $EntryAttemptSuffix = $(if ($EntryInvocation -is [string] -and
          $EntryInvocation.StartsWith($SameRunInvocationPrefix, [StringComparison]::Ordinal)) {
        $EntryInvocation.Substring($SameRunInvocationPrefix.Length)
      } else { "" })
      if ($EntryInvocation -isnot [string] -or $CertificateInvocation -isnot [string] -or
          $EntryInvocation -cne $CertificateInvocation -or
          $EntryAttemptSuffix -cnotmatch '^[1-9][0-9]*$' -or
          $Statement.predicateType -cne "https://slsa.dev/provenance/v1" -or
          $BuildDefinition.buildType -cne "https://actions.github.io/buildtypes/workflow/v1" -or
          $Workflow.path -cne $WorkflowPath -or $Workflow.ref -cne $TagRef -or
          $Workflow.repository -cne "https://github.com/$Repository" -or
          $Certificate.githubWorkflowSHA -cne $SourceSha -or
          $Certificate.githubWorkflowRepository -cne $Repository -or
          $Certificate.githubWorkflowRef -cne $TagRef -or
          $Certificate.runnerEnvironment -cne "github-hosted" -or
          $Certificate.sourceRepositoryDigest -cne $SourceSha -or
          $Certificate.sourceRepositoryRef -cne $TagRef -or
          $MatchingSubjects.Count -ne 1) {
        throw "Attestation entry is not exact and unambiguous."
      }
      $EntryIsValid = $true
    }
    catch { $EntryIsValid = $false }
    if (-not $EntryIsValid) {
      throw "GitHub attestation set contains a malformed, unrelated, or ambiguous statement."
    }
    if ($EntryInvocation -ceq $ExpectedInvocationUri) { $CurrentAttemptCount += 1 }
  }
  if ($CurrentAttemptCount -ne 1) {
    throw "GitHub attestation set does not contain exactly one current-attempt statement."
  }
}
# END EXACT_ATTEMPT_ATTESTATION_SELECTOR

function New-AttestationSelectionSelfTestEntry(
  [string]$Invocation,
  [string]$Repository,
  [string]$WorkflowPath,
  [string]$TagRef,
  [string]$SourceSha,
  [string]$SubjectName,
  [string]$SubjectSha256
) {
  return [pscustomobject]@{
    verificationResult = [pscustomobject]@{
      statement = [pscustomobject]@{
        predicateType = "https://slsa.dev/provenance/v1"
        subject = @([pscustomobject]@{
          name = $SubjectName
          digest = [pscustomobject]@{ sha256 = $SubjectSha256 }
        })
        predicate = [pscustomobject]@{
          buildDefinition = [pscustomobject]@{
            buildType = "https://actions.github.io/buildtypes/workflow/v1"
            externalParameters = [pscustomobject]@{ workflow = [pscustomobject]@{
              path = $WorkflowPath
              ref = $TagRef
              repository = "https://github.com/$Repository"
            }}
          }
          runDetails = [pscustomobject]@{
            metadata = [pscustomobject]@{ invocationId = $Invocation }
          }
        }
      }
      signature = [pscustomobject]@{ certificate = [pscustomobject]@{
        runInvocationURI = $Invocation
        githubWorkflowSHA = $SourceSha
        githubWorkflowRepository = $Repository
        githubWorkflowRef = $TagRef
        runnerEnvironment = "github-hosted"
        sourceRepositoryDigest = $SourceSha
        sourceRepositoryRef = $TagRef
      }}
    }
  }
}

function Invoke-AttestationSelectionSelfTest {
  $TestRepository = "flrngel/local-browser-bridge"
  $TestRunId = "123456789"
  $TestWorkflow = ".github/workflows/deploy.yml"
  $TestTagRef = "refs/heads/main"
  $TestSource = "1111111111111111111111111111111111111111"
  $TestSubject = "fixture.bin"
  $TestSubjectSha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  $OldInvocation = "https://github.com/$TestRepository/actions/runs/$TestRunId/attempts/1"
  $CurrentInvocation = "https://github.com/$TestRepository/actions/runs/$TestRunId/attempts/2"
  $Old = New-AttestationSelectionSelfTestEntry `
    $OldInvocation $TestRepository $TestWorkflow $TestTagRef $TestSource $TestSubject $TestSubjectSha
  $Current = New-AttestationSelectionSelfTestEntry `
    $CurrentInvocation $TestRepository $TestWorkflow $TestTagRef $TestSource $TestSubject $TestSubjectSha
  $Arguments = @{
    ExpectedInvocationUri = $CurrentInvocation
    WorkflowRunId = $TestRunId
    WorkflowPath = $TestWorkflow
    Repository = $TestRepository
    TagRef = $TestTagRef
    SourceSha = $TestSource
    SubjectName = $TestSubject
    SubjectSha256 = $TestSubjectSha
  }
  Assert-ExactAttemptAttestationSet -Attestations @($Old, $Current) @Arguments
  $Serialized = ConvertTo-Json -InputObject @($Old, $Current) -Depth 20 -Compress
  $RoundTripped = @(
    Microsoft.PowerShell.Utility\ConvertFrom-Json -InputObject $Serialized
  )
  Assert-ExactAttemptAttestationSet -Attestations $RoundTripped @Arguments
  $Ps51WrappedRoundTrip = New-Object object[] 1
  $Ps51WrappedRoundTrip[0] = [object[]]@($Old, $Current)
  Assert-ExactAttemptAttestationSet -Attestations $Ps51WrappedRoundTrip @Arguments
  $NestedWrappedRoundTrip = New-Object object[] 1
  $NestedWrappedRoundTrip[0] = $Ps51WrappedRoundTrip
  $NestedWrapperRejected = $false
  try {
    Assert-ExactAttemptAttestationSet -Attestations $NestedWrappedRoundTrip @Arguments
  }
  catch { $NestedWrapperRejected = $true }
  if (-not $NestedWrapperRejected) {
    throw "Attestation selection self-test accepted nested-attestation-array."
  }

  $MalformedCurrent = New-AttestationSelectionSelfTestEntry `
    $CurrentInvocation $TestRepository $TestWorkflow $TestTagRef $TestSource $TestSubject $TestSubjectSha
  $MalformedCurrent.verificationResult.signature = [pscustomobject]@{}
  $WrongSubjectCurrent = New-AttestationSelectionSelfTestEntry `
    $CurrentInvocation $TestRepository $TestWorkflow $TestTagRef $TestSource "wrong.bin" $TestSubjectSha
  $ScalarSubjectCurrent = New-AttestationSelectionSelfTestEntry `
    $CurrentInvocation $TestRepository $TestWorkflow $TestTagRef $TestSource $TestSubject $TestSubjectSha
  $ScalarSubjectCurrent.verificationResult.statement.subject = [pscustomobject]@{
    name = $TestSubject
    digest = [pscustomobject]@{ sha256 = $TestSubjectSha }
  }
  $MalformedSubjectCurrent = New-AttestationSelectionSelfTestEntry `
    $CurrentInvocation $TestRepository $TestWorkflow $TestTagRef $TestSource $TestSubject $TestSubjectSha
  $MalformedSubjectCurrent.verificationResult.statement.subject = @([pscustomobject]@{
    name = $TestSubject
    digest = [pscustomobject]@{ sha256 = "not-a-sha256" }
  })
  foreach ($RejectedCase in @(
    [pscustomobject]@{ Label = "old-only"; Entries = @($Old) },
    [pscustomobject]@{ Label = "duplicate-current"; Entries = @($Current, $Current) },
    [pscustomobject]@{ Label = "malformed-current"; Entries = @($Old, $MalformedCurrent) },
    [pscustomobject]@{ Label = "wrong-current-subject"; Entries = @($Old, $WrongSubjectCurrent) },
    [pscustomobject]@{ Label = "scalar-current-subject"; Entries = @($Old, $ScalarSubjectCurrent) },
    [pscustomobject]@{ Label = "malformed-current-subject"; Entries = @($Old, $MalformedSubjectCurrent) }
  )) {
    $Rejected = $false
    try {
      Assert-ExactAttemptAttestationSet -Attestations @($RejectedCase.Entries) @Arguments
    }
    catch { $Rejected = $true }
    if (-not $Rejected) {
      throw "Attestation selection self-test accepted $($RejectedCase.Label)."
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

function Assert-PrivateDirectoryAcl([string]$Path) {
  $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  try {
    if ($null -eq $Identity.User) { throw "The current Windows identity has no SID." }
    $Observed = Get-DirectoryAccessControlPortable $Path
    $Owner = $Observed.GetOwner([Security.Principal.SecurityIdentifier])
    $Rules = @($Observed.GetAccessRules(
      $true,
      $true,
      [Security.Principal.SecurityIdentifier]
    ))
    $ExpectedInheritance = [Security.AccessControl.InheritanceFlags]::ContainerInherit -bor
      [Security.AccessControl.InheritanceFlags]::ObjectInherit
    $OwnerRule = @($Rules | Where-Object {
      -not $_.IsInherited -and
      $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
      $_.IdentityReference.Value -ceq $Identity.User.Value -and
      $_.FileSystemRights -eq [Security.AccessControl.FileSystemRights]::FullControl -and
      $_.InheritanceFlags -eq $ExpectedInheritance -and
      $_.PropagationFlags -eq [Security.AccessControl.PropagationFlags]::None
    })
    if ($Owner.Value -cne $Identity.User.Value -or
        -not $Observed.AreAccessRulesProtected -or
        -not $Observed.AreAccessRulesCanonical -or
        $Rules.Count -ne 1 -or $OwnerRule.Count -ne 1) {
      throw "Fresh destination ACL is not protected and private to the current user."
    }
  }
  finally { $Identity.Dispose() }
}

function New-PrivateDirectory([string]$Path) {
  if ([IO.File]::Exists($Path) -or [IO.Directory]::Exists($Path)) {
    throw "Private directory creation requires a fresh path."
  }
  $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  try {
    if ($null -eq $Identity.User) { throw "The current Windows identity has no SID." }
    $Security = [Security.AccessControl.DirectorySecurity]::new()
    $Security.SetOwner($Identity.User)
    $Security.SetAccessRuleProtection($true, $false)
    $Inheritance = [Security.AccessControl.InheritanceFlags]::ContainerInherit -bor
      [Security.AccessControl.InheritanceFlags]::ObjectInherit
    $Rule = [Security.AccessControl.FileSystemAccessRule]::new(
      $Identity.User,
      [Security.AccessControl.FileSystemRights]::FullControl,
      $Inheritance,
      [Security.AccessControl.PropagationFlags]::None,
      [Security.AccessControl.AccessControlType]::Allow
    )
    [void]$Security.AddAccessRule($Rule)

    # Apply the protected owner-only DACL as part of directory creation. A
    # create-then-rewrite sequence briefly inherits the parent DACL and can
    # require a separate WRITE_OWNER operation on otherwise valid hosts.
    if ($PSVersionTable.PSEdition -ceq "Core") {
      [IO.FileSystemAclExtensions]::Create(
        [IO.DirectoryInfo]::new($Path), $Security
      )
    }
    else {
      [IO.Directory]::CreateDirectory($Path, $Security) | Out-Null
    }
  }
  finally { $Identity.Dispose() }
  Assert-PrivateDirectoryAcl $Path
}

function Invoke-TrustSelfTest {
  $Root = [IO.Path]::Combine([IO.Path]::GetTempPath(), "lbb-win-trust-self-test-" + [Guid]::NewGuid().ToString("N"))
  $AllowedSelfTestFiles = @("fixture.exe", "SHA256SUMS.txt", "create-once.json")
  New-PrivateDirectory $Root
  try {
    Assert-PrivateDirectoryAcl $Root
    $BeforeCollision = (Get-DirectoryAccessControlPortable $Root).GetSecurityDescriptorSddlForm(
      [Security.AccessControl.AccessControlSections]::Access -bor
        [Security.AccessControl.AccessControlSections]::Owner
    )
    $CollisionRefused = $false
    try { New-PrivateDirectory $Root }
    catch {
      if ($_.Exception.Message -cne "Private directory creation requires a fresh path.") {
        throw
      }
      $CollisionRefused = $true
    }
    $AfterCollision = (Get-DirectoryAccessControlPortable $Root).GetSecurityDescriptorSddlForm(
      [Security.AccessControl.AccessControlSections]::Access -bor
        [Security.AccessControl.AccessControlSections]::Owner
    )
    if (-not $CollisionRefused -or $AfterCollision -cne $BeforeCollision) {
      throw "Private directory collision self-test failed."
    }

    $InheritedChild = Join-Path $Root "inherited-child"
    [IO.Directory]::CreateDirectory($InheritedChild) | Out-Null
    try {
      $InheritedRefused = $false
      try { Assert-PrivateDirectoryAcl $InheritedChild }
      catch {
        if ($_.Exception.Message -cne
            "Fresh destination ACL is not protected and private to the current user.") {
          throw
        }
        $InheritedRefused = $true
      }
      if (-not $InheritedRefused) {
        throw "Inherited directory ACL self-test failed."
      }
    }
    finally {
      if ([IO.Directory]::Exists($InheritedChild)) {
        $InheritedInfo = [IO.DirectoryInfo]::new($InheritedChild)
        if (($InheritedInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $InheritedInfo.GetFileSystemInfos().Count -ne 0) {
          throw "Inherited directory ACL self-test cleanup refused an unexpected target."
        }
        [IO.Directory]::Delete($InheritedChild, $false)
      }
    }

    $Exe = Join-Path $Root "fixture.exe"
    $Pe = New-Object byte[] 512
    $Pe[0] = 0x4d; $Pe[1] = 0x5a; $Pe[0x3c] = 0x80
    $Pe[0x80] = 0x50; $Pe[0x81] = 0x45
    $Pe[0x84] = 0x64; $Pe[0x85] = 0x86
    $Pe[0x86] = 0x01
    $Pe[0x94] = 0xf0
    $Pe[0x98] = 0x0b; $Pe[0x99] = 0x02
    [IO.File]::WriteAllBytes($Exe, $Pe)
    Assert-Pe32PlusX64 $Exe

    $Assets = @("one.exe", "two.exe", "three.tar.gz", "four.zip")
    $Manifest = Join-Path $Root "SHA256SUMS.txt"
    $Rows = for ($Index = 0; $Index -lt 4; $Index += 1) {
      (([char](97 + $Index)).ToString() * 64) + "  " + $Assets[$Index]
    }
    [IO.File]::WriteAllBytes(
      $Manifest,
      [Text.Encoding]::ASCII.GetBytes(($Rows -join "`n") + "`n")
    )
    $Parsed = @(Get-CanonicalManifestRows $Manifest $Assets)
    if ($Parsed.Count -ne 4 -or $Parsed[3].file -cne "four.zip") {
      throw "Canonical manifest self-test failed."
    }
    $Record = [ordered]@{ schemaVersion = 1; passed = $true }
    $RecordPath = Join-Path $Root "create-once.json"
    Write-CreateOnceUtf8Json $RecordPath $Record
    $RefusedOverwrite = $false
    try { Write-CreateOnceUtf8Json $RecordPath $Record }
    catch [IO.IOException] { $RefusedOverwrite = $true }
    if (-not $RefusedOverwrite) { throw "Create-once JSON self-test failed." }
    if ((ConvertTo-WindowsCommandLineArgument 'C:\path with space\') -cne '"C:\path with space\\"') {
      throw "Windows command-line quoting self-test failed."
    }
    Assert-SafePaxPayload ([Text.Encoding]::ASCII.GetBytes("11 mtime=1`n"))
    $UnsafePaxRefused = $false
    try { Assert-SafePaxPayload ([Text.Encoding]::ASCII.GetBytes("13 path=../x`n")) }
    catch { $UnsafePaxRefused = $true }
    if (-not $UnsafePaxRefused) { throw "Unsafe PAX metadata self-test failed." }
    Invoke-AttestationSelectionSelfTest
  }
  finally {
    if ([IO.Directory]::Exists($Root)) {
      $RootInfo = [IO.DirectoryInfo]::new($Root)
      $Entries = @($RootInfo.GetFileSystemInfos())
      if (($RootInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
          @($Entries | Where-Object {
            $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $AllowedSelfTestFiles -cnotcontains $_.Name
          }).Count -ne 0 -or @($Entries.Name | Sort-Object -Unique).Count -ne $Entries.Count) {
        throw "Self-test cleanup refused an unexpected or linked inventory."
      }
      foreach ($Entry in $Entries) { [IO.File]::Delete($Entry.FullName) }
      [IO.Directory]::Delete($Root, $false)
    }
  }
  Write-Output "Windows release-candidate trust wrapper self-test passed."
}

# Self-test stops here: no candidate path, network client, token, archive, or
# repository operation is consulted or executed.
if ($SelfTestRequested) {
  Invoke-TrustSelfTest
  exit 0
}

if ($Version -cne $ProductVersion -or $Version -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+$' -or
    $WorkflowRunId -cnotmatch '^[1-9][0-9]*$' -or
    $WorkflowRunAttempt -cnotmatch '^[1-9][0-9]*$' -or
    $ArtifactId -cnotmatch '^[1-9][0-9]*$' -or
    $SourceSha -cnotmatch '^[0-9a-f]{40}$') {
  throw "Candidate identifiers are not canonical v0.12.40 identifiers."
}
$Tag = "v$Version"
$ExpectedInvocationUri = "https://github.com/$Repository/actions/runs/$WorkflowRunId/attempts/$WorkflowRunAttempt"

$TrustedGit = Assert-OrdinaryAbsolutePath $TrustedGit $true "Trusted git.exe"
$TrustedGh = Assert-OrdinaryAbsolutePath $TrustedGh $true "Trusted gh.exe"
if (-not [String]::Equals([IO.Path]::GetFileName($TrustedGit), "git.exe", [StringComparison]::OrdinalIgnoreCase) -or
    -not [String]::Equals([IO.Path]::GetFileName($TrustedGh), "gh.exe", [StringComparison]::OrdinalIgnoreCase)) {
  throw "TrustedGit and TrustedGh must name git.exe and gh.exe."
}
if ([String]::IsNullOrWhiteSpace($Destination) -or -not [IO.Path]::IsPathRooted($Destination)) {
  throw "Destination must be a fresh absolute path."
}
$Destination = [IO.Path]::GetFullPath($Destination).TrimEnd('\')
if ($Destination.Length -gt 90 -or [IO.File]::Exists($Destination) -or [IO.Directory]::Exists($Destination)) {
  throw "Destination must be a fresh short path of at most 90 characters."
}
$DestinationParent = [IO.Path]::GetDirectoryName($Destination)
$null = Assert-OrdinaryAbsolutePath $DestinationParent $false "Destination parent"

New-PrivateDirectory $Destination
$DestinationInfo = [IO.DirectoryInfo]::new($Destination)
if (($DestinationInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
    $DestinationInfo.GetFileSystemInfos().Count -ne 0) {
  throw "Fresh destination is not an empty ordinary directory."
}

$PayloadDirectory = Join-Path $Destination "payload"
$SourceDirectory = Join-Path $Destination "source"
$TrustDirectory = Join-Path $Destination ".trust"
$IsolatedGh = Join-Path $TrustDirectory "gh"
$EmptyTemplates = Join-Path $TrustDirectory "templates"
$EmptyHooks = Join-Path $TrustDirectory "hooks"
foreach ($Directory in @($PayloadDirectory, $TrustDirectory, $IsolatedGh, $EmptyTemplates, $EmptyHooks)) {
  [IO.Directory]::CreateDirectory($Directory) | Out-Null
  if ([IO.DirectoryInfo]::new($Directory).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw "Trust workspace contains a reparse-point directory."
  }
}
if ($SourceDirectory.Length -gt 98) { throw "Detached source checkout root is not short enough." }

$InheritedToken = [Environment]::GetEnvironmentVariable("GH_TOKEN", "Process")
$SecureGhToken = $null
if (-not [String]::IsNullOrWhiteSpace($InheritedToken)) {
  $SecureGhToken = [Security.SecureString]::new()
  foreach ($Character in $InheritedToken.ToCharArray()) { $SecureGhToken.AppendChar($Character) }
  $SecureGhToken.MakeReadOnly()
}
[Environment]::SetEnvironmentVariable("GH_TOKEN", $null, "Process")
$InheritedToken = $null

foreach ($EnvironmentName in @([Environment]::GetEnvironmentVariables("Process").Keys)) {
  $Name = [string]$EnvironmentName
  if ($Name -match '^(?:GIT_|GH_|GITHUB_)' -or $Name -in @("SSH_ASKPASS", "XDG_CONFIG_HOME")) {
    [Environment]::SetEnvironmentVariable($Name, $null, "Process")
  }
}
$env:GIT_CONFIG_NOSYSTEM = "1"
$env:GIT_CONFIG_GLOBAL = "NUL"
$env:GIT_CONFIG_COUNT = "0"
$env:GIT_ATTR_NOSYSTEM = "1"
$env:GIT_ALLOW_PROTOCOL = "https"
$env:GIT_TERMINAL_PROMPT = "0"
$env:GH_CONFIG_DIR = $IsolatedGh
$env:GH_PROMPT_DISABLED = "1"
$env:GH_NO_UPDATE_NOTIFIER = "1"

function Invoke-TrustedProcessText(
  [string]$Executable,
  [string[]]$Arguments,
  [string]$WorkingDirectory,
  [string]$FailureLabel,
  [bool]$IncludeGhToken,
  [int]$TimeoutMilliseconds = 120000
) {
  $Info = [Diagnostics.ProcessStartInfo]::new()
  $Info.FileName = $Executable
  $Info.WorkingDirectory = $WorkingDirectory
  $Info.Arguments = Join-TrustedArguments $Arguments
  $Info.UseShellExecute = $false
  $Info.CreateNoWindow = $true
  $Info.RedirectStandardOutput = $true
  $Info.RedirectStandardError = $true
  $Bstr = [IntPtr]::Zero
  $ChildToken = $null
  $Process = $null
  try {
    if ($IncludeGhToken) {
      $Bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($SecureGhToken)
      $ChildToken = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($Bstr)
      $Info.EnvironmentVariables["GH_TOKEN"] = $ChildToken
    }
    $Process = [Diagnostics.Process]::new()
    $Process.StartInfo = $Info
    if (-not $Process.Start()) { throw "$FailureLabel child did not start." }
    if ($IncludeGhToken) { [void]$Info.EnvironmentVariables.Remove("GH_TOKEN") }
    $ChildToken = $null
    $OutputTask = $Process.StandardOutput.ReadToEndAsync()
    $ErrorTask = $Process.StandardError.ReadToEndAsync()
    if (-not $Process.WaitForExit($TimeoutMilliseconds)) {
      $Process.Kill()
      throw "$FailureLabel exceeded its bounded runtime."
    }
    $Output = $OutputTask.GetAwaiter().GetResult()
    [void]$ErrorTask.GetAwaiter().GetResult()
    if ($Process.ExitCode -ne 0) { throw "$FailureLabel failed with a nonzero exit code." }
    return $Output
  }
  finally {
    if ($IncludeGhToken) { [void]$Info.EnvironmentVariables.Remove("GH_TOKEN") }
    $ChildToken = $null
    if ($Bstr -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($Bstr) }
    if ($null -ne $Process) { $Process.Dispose() }
  }
}

function Invoke-TrustedGhJson([string[]]$Arguments, [string]$FailureLabel) {
  $Text = Invoke-TrustedProcessText $TrustedGh $Arguments $Destination $FailureLabel $true
  try { return ($Text | ConvertFrom-Json) }
  catch { throw "$FailureLabel did not return valid JSON." }
}

function Invoke-TrustedGhBinary(
  [string[]]$Arguments,
  [string]$OutputPath,
  [string]$FailureLabel
) {
  $Info = [Diagnostics.ProcessStartInfo]::new()
  $Info.FileName = $TrustedGh
  $Info.WorkingDirectory = $Destination
  $Info.Arguments = Join-TrustedArguments $Arguments
  $Info.UseShellExecute = $false
  $Info.CreateNoWindow = $true
  $Info.RedirectStandardOutput = $true
  $Info.RedirectStandardError = $true
  $Bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($SecureGhToken)
  $ChildToken = $null
  $Process = $null
  $Output = $null
  try {
    $ChildToken = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($Bstr)
    $Info.EnvironmentVariables["GH_TOKEN"] = $ChildToken
    $Process = [Diagnostics.Process]::new()
    $Process.StartInfo = $Info
    $Output = [IO.File]::Open($OutputPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    if (-not $Process.Start()) { throw "$FailureLabel child did not start." }
    [void]$Info.EnvironmentVariables.Remove("GH_TOKEN")
    $ChildToken = $null
    $CopyTask = $Process.StandardOutput.BaseStream.CopyToAsync($Output)
    $ErrorTask = $Process.StandardError.ReadToEndAsync()
    if (-not $Process.WaitForExit(300000)) {
      $Process.Kill()
      throw "$FailureLabel exceeded its five-minute bound."
    }
    $CopyTask.GetAwaiter().GetResult()
    [void]$ErrorTask.GetAwaiter().GetResult()
    $Output.Flush()
    if ($Process.ExitCode -ne 0) { throw "$FailureLabel failed with a nonzero exit code." }
  }
  catch {
    if ($null -ne $Output) { $Output.Dispose(); $Output = $null }
    if ([IO.File]::Exists($OutputPath)) { [IO.File]::Delete($OutputPath) }
    throw
  }
  finally {
    [void]$Info.EnvironmentVariables.Remove("GH_TOKEN")
    $ChildToken = $null
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($Bstr)
    if ($null -ne $Output) { $Output.Dispose() }
    if ($null -ne $Process) { $Process.Dispose() }
  }
}

function Invoke-TrustedGit([string[]]$Arguments, [string]$WorkingDirectory, [string]$FailureLabel) {
  return (Invoke-TrustedProcessText $TrustedGit $Arguments $WorkingDirectory $FailureLabel $false 300000)
}

$ExpectedAssets = @(
  "local-browser-bridge-v$Version-windows-x86_64.exe",
  "local-computer-helper-v$Version-windows-x86_64.exe",
  "local-browser-bridge-v$Version-macos-universal.tar.gz",
  "local-browser-bridge-extension-v$Version.zip"
)
$ExpectedFiles = @($ExpectedAssets) + "SHA256SUMS.txt"

# Bootstrap the trust program, acceptance runner, and fixture from a clean,
# exact, short detached checkout before asking for a token or reading any
# release-candidate bytes.
$GitCommon = @(
  "--no-replace-objects", "-c", "core.fsmonitor=false", "-c", "core.hooksPath=$EmptyHooks",
  "-c", "core.autocrlf=false", "-c", "core.longpaths=true", "-c", "core.symlinks=false"
)
$null = Invoke-TrustedGit (@($GitCommon) + @(
  "clone", "--no-checkout", "--no-local", "--origin", "origin", "--template=$EmptyTemplates", $Origin, $SourceDirectory
)) $Destination "Fixed-origin fresh source clone"
$null = Invoke-TrustedGit (@($GitCommon) + @("-C", $SourceDirectory, "config", "--local", "core.longpaths", "true")) $Destination "Long-path source configuration"
$null = Invoke-TrustedGit (@($GitCommon) + @("-C", $SourceDirectory, "fetch", "--force", "origin", $SourceSha)) $Destination "Exact source fetch"
$null = Invoke-TrustedGit (@($GitCommon) + @("-C", $SourceDirectory, "checkout", "--detach", "--force", $SourceSha)) $Destination "Exact detached source checkout"

function Invoke-SourceGit([string[]]$Arguments, [string]$FailureLabel) {
  return ([string](Invoke-TrustedGit (@($GitCommon) + @("-C", $SourceDirectory) + $Arguments) $Destination $FailureLabel)).Trim()
}

$OriginFetch = Invoke-SourceGit @("remote", "get-url", "origin") "Source origin inspection"
$OriginPush = @(Invoke-SourceGit @("remote", "get-url", "--push", "--all", "origin") "Source push-origin inspection")
$ObservedHead = Invoke-SourceGit @("rev-parse", "--verify", "HEAD") "Source HEAD inspection"
if ($OriginFetch -cne $Origin -or $OriginPush.Count -ne 1 -or $OriginPush[0] -cne $Origin -or
    $ObservedHead -cne $SourceSha) {
  throw "Fresh source origin or exact commit binding mismatch."
}
$SymbolicInfo = [Diagnostics.ProcessStartInfo]::new()
$SymbolicInfo.FileName = $TrustedGit
$SymbolicInfo.WorkingDirectory = $Destination
$SymbolicInfo.Arguments = Join-TrustedArguments (@($GitCommon) + @("-C", $SourceDirectory, "symbolic-ref", "-q", "HEAD"))
$SymbolicInfo.UseShellExecute = $false
$SymbolicInfo.CreateNoWindow = $true
$SymbolicInfo.RedirectStandardOutput = $true
$SymbolicInfo.RedirectStandardError = $true
$SymbolicProcess = [Diagnostics.Process]::new()
$SymbolicProcess.StartInfo = $SymbolicInfo
try {
  if (-not $SymbolicProcess.Start()) { throw "Detached-HEAD inspection child did not start." }
  $SymbolicOutput = $SymbolicProcess.StandardOutput.ReadToEnd()
  [void]$SymbolicProcess.StandardError.ReadToEnd()
  if (-not $SymbolicProcess.WaitForExit(30000)) {
    $SymbolicProcess.Kill()
    throw "Detached-HEAD inspection exceeded its bounded runtime."
  }
  if ($SymbolicProcess.ExitCode -ne 1 -or -not [String]::IsNullOrEmpty($SymbolicOutput)) {
    throw "Fresh source checkout is not detached."
  }
}
finally { $SymbolicProcess.Dispose() }

$Status = Invoke-SourceGit @("status", "--porcelain=v2", "--untracked-files=all") "Source cleanliness inspection"
$Deleted = Invoke-SourceGit @("ls-files", "--deleted") "Source missing-file inspection"
$Others = Invoke-SourceGit @("ls-files", "--others", "--exclude-standard") "Source untracked-file inspection"
$Ignored = Invoke-SourceGit @("ls-files", "--others", "--ignored", "--exclude-standard") "Source ignored-file inspection"
if ($Status.Length -ne 0 -or $Deleted.Length -ne 0 -or $Others.Length -ne 0 -or $Ignored.Length -ne 0) {
  throw "Fresh detached source checkout is not completely clean and materialized."
}
$null = Invoke-SourceGit @("diff", "--quiet", "HEAD", "--") "Source worktree diff inspection"
$null = Invoke-SourceGit @("diff", "--cached", "--quiet") "Source index diff inspection"
$null = Invoke-SourceGit @("fsck", "--full") "Source object-database inspection"

$TrustedRelativeFiles = @(
  "scripts/verify-windows-release-candidate.ps1",
  "scripts/test-windows-computer-use.ps1",
  "tests/fixtures/windows/WindowsComputerUseFixture.ps1"
)
foreach ($Relative in $TrustedRelativeFiles) {
  $Materialized = [IO.Path]::GetFullPath((Join-Path $SourceDirectory $Relative))
  $SourcePrefix = [IO.Path]::GetFullPath($SourceDirectory).TrimEnd('\') + '\'
  if (-not $Materialized.StartsWith($SourcePrefix, [StringComparison]::OrdinalIgnoreCase) -or
      -not [IO.File]::Exists($Materialized)) {
    throw "Required wrapper, runner, or fixture source is not an ordinary tracked file."
  }
  $null = Assert-OrdinaryAbsolutePath $Materialized $true "Required trusted source file"
  $Blob = Invoke-SourceGit @("rev-parse", "--verify", "HEAD:$Relative") "Trusted source blob inspection"
  $WorktreeBlob = Invoke-SourceGit @("hash-object", "--no-filters", "--", $Relative) "Materialized source blob inspection"
  if ($Blob -cnotmatch '^[0-9a-f]{40}$' -or $WorktreeBlob -cne $Blob) {
    throw "Required wrapper, runner, or fixture does not match its exact source blob."
  }
}
$FreshWrapper = Join-Path $SourceDirectory "scripts/verify-windows-release-candidate.ps1"
if ((Get-TrustedSha256 $FreshWrapper) -cne (Get-TrustedSha256 $PSCommandPath)) {
  throw "Executing trust wrapper does not match the exact source wrapper blob."
}

if ($null -eq $SecureGhToken) {
  $SecureGhToken = Read-Host "Least-privilege GitHub candidate-verification token" -AsSecureString
}
if ($null -eq $SecureGhToken -or $SecureGhToken.Length -lt 1) {
  throw "A non-empty GitHub token is required."
}

$Run = Invoke-TrustedGhJson @(
  "api", "--hostname", "github.com",
  "repos/$Repository/actions/runs/$WorkflowRunId/attempts/$WorkflowRunAttempt"
) "Exact-attempt workflow run API"
if ($Run.event -cne "workflow_dispatch" -or $Run.head_sha -cne $SourceSha -or
    $Run.head_branch -cne "main" -or [string]$Run.run_attempt -cne $WorkflowRunAttempt -or
    $Run.path -cne $WorkflowPath -or $Run.repository.full_name -cne $Repository -or
    $Run.status -cne "completed" -or $Run.conclusion -cne "success") {
  throw "Workflow run API binding mismatch."
}
$JobsResponse = Invoke-TrustedGhJson @(
  "api", "--hostname", "github.com",
  "repos/$Repository/actions/runs/$WorkflowRunId/attempts/$WorkflowRunAttempt/jobs?per_page=100"
) "Exact-attempt workflow jobs API"
$ReturnedJobs = @($JobsResponse.jobs)
if ([int64]$JobsResponse.total_count -lt 1 -or [int64]$JobsResponse.total_count -ge 100 -or
    $ReturnedJobs.Count -ne [int]$JobsResponse.total_count) {
  throw "Exact-attempt workflow jobs API response is empty, paginated, or incomplete."
}
$AssembleJobs = @($ReturnedJobs | Where-Object { $_.name -ceq "Assemble frozen release candidate" })
if ($AssembleJobs.Count -ne 1 -or $AssembleJobs[0].conclusion -cne "success" -or
    [string]$AssembleJobs[0].started_at -cnotmatch '^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$' -or
    [string]$AssembleJobs[0].completed_at -cnotmatch '^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$') {
  throw "Exact-attempt workflow does not contain one successful bounded assembly job."
}
$AssembleStartedAt = [DateTimeOffset]::MinValue
$AssembleCompletedAt = [DateTimeOffset]::MinValue
if (-not [DateTimeOffset]::TryParse(
      [string]$AssembleJobs[0].started_at,
      [Globalization.CultureInfo]::InvariantCulture,
      [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal,
      [ref]$AssembleStartedAt
    ) -or
    -not [DateTimeOffset]::TryParse(
      [string]$AssembleJobs[0].completed_at,
      [Globalization.CultureInfo]::InvariantCulture,
      [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal,
      [ref]$AssembleCompletedAt
    ) -or
    $AssembleStartedAt -gt $AssembleCompletedAt) {
  throw "Exact-attempt assembly job timestamps are invalid."
}
$ArtifactsResponse = Invoke-TrustedGhJson @(
  "api", "--hostname", "github.com", "repos/$Repository/actions/runs/$WorkflowRunId/artifacts?per_page=100"
) "Workflow artifact API"
$ReturnedArtifacts = @($ArtifactsResponse.artifacts)
if ([int64]$ArtifactsResponse.total_count -lt 1 -or [int64]$ArtifactsResponse.total_count -ge 100 -or
    $ReturnedArtifacts.Count -ne [int]$ArtifactsResponse.total_count) {
  throw "Workflow artifact API response is empty, paginated, or incomplete."
}
$ReleaseCandidates = @($ReturnedArtifacts | Where-Object {
  $_.name -ceq "release-candidate" -and $_.expired -eq $false
})
if ($ReleaseCandidates.Count -ne 1 -or [string]$ReleaseCandidates[0].id -cne $ArtifactId) {
  throw "Workflow run does not contain exactly one expected nonexpired release-candidate artifact."
}
$Artifact = $ReleaseCandidates[0]
if ($Artifact.name -cne "release-candidate" -or $Artifact.expired -ne $false -or
    [int64]$Artifact.size_in_bytes -lt 1 -or
    [int64]$Artifact.size_in_bytes -gt $MaximumCandidateBytes -or
    $Artifact.digest -cnotmatch '^sha256:[0-9a-f]{64}$' -or
    [string]$Artifact.workflow_run.id -cne $WorkflowRunId -or
    $Artifact.workflow_run.head_sha -cne $SourceSha -or $Artifact.workflow_run.head_branch -cne "main") {
  throw "Release-candidate artifact API binding mismatch."
}
$DirectArtifact = Invoke-TrustedGhJson @(
  "api", "--hostname", "github.com", "repos/$Repository/actions/artifacts/$ArtifactId"
) "Direct release-candidate artifact API"
if ([string]$DirectArtifact.id -cne $ArtifactId -or
    $DirectArtifact.name -cne "release-candidate" -or $DirectArtifact.expired -ne $false -or
    [int64]$DirectArtifact.size_in_bytes -lt 1 -or
    [int64]$DirectArtifact.size_in_bytes -gt $MaximumCandidateBytes -or
    $DirectArtifact.digest -cnotmatch '^sha256:[0-9a-f]{64}$' -or
    [string]$DirectArtifact.workflow_run.id -cne $WorkflowRunId -or
    $DirectArtifact.workflow_run.head_sha -cne $SourceSha -or
    $DirectArtifact.workflow_run.head_branch -cne "main" -or
    [string]$DirectArtifact.id -cne [string]$Artifact.id -or
    [int64]$DirectArtifact.size_in_bytes -ne [int64]$Artifact.size_in_bytes -or
    $DirectArtifact.digest -cne $Artifact.digest -or
    $DirectArtifact.created_at -cne $Artifact.created_at -or
    [string]$DirectArtifact.created_at -cnotmatch '^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$') {
  throw "Direct release-candidate artifact metadata mismatch."
}
$ArtifactCreatedAt = [DateTimeOffset]::MinValue
if (-not [DateTimeOffset]::TryParse(
      [string]$DirectArtifact.created_at,
      [Globalization.CultureInfo]::InvariantCulture,
      [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal,
      [ref]$ArtifactCreatedAt
    ) -or
    $ArtifactCreatedAt -lt $AssembleStartedAt -or $ArtifactCreatedAt -gt $AssembleCompletedAt) {
  throw "Direct artifact metadata is not bound to the successful current-attempt assembly job."
}
$ArtifactZip = Join-Path $Destination "release-candidate-artifact-$ArtifactId.zip"
$ArtifactPartial = "$ArtifactZip.partial"
Invoke-TrustedGhBinary @(
  "api", "--hostname", "github.com", "-H", "Accept: application/vnd.github+json",
  "repos/$Repository/actions/artifacts/$ArtifactId/zip"
) $ArtifactPartial "Raw workflow artifact download"
$ExpectedArtifactBytes = [int64]$DirectArtifact.size_in_bytes
$ExpectedArtifactSha256 = ([string]$DirectArtifact.digest).Substring(7)
$ObservedArtifact = [IO.FileInfo]::new($ArtifactPartial)
if ($ObservedArtifact.Length -ne $ExpectedArtifactBytes) { throw "Raw artifact ZIP size mismatch." }
$ArtifactZipSha256 = Get-TrustedSha256 $ArtifactPartial
if ($ArtifactZipSha256 -cne $ExpectedArtifactSha256) { throw "Raw artifact ZIP SHA-256 mismatch." }
[IO.File]::Move($ArtifactPartial, $ArtifactZip)

Add-Type -AssemblyName System.IO.Compression.FileSystem
$Archive = [IO.Compression.ZipFile]::OpenRead($ArtifactZip)
try {
  $Entries = @($Archive.Entries)
  $Names = @($Entries | ForEach-Object { $_.FullName })
  $TotalUncompressedBytes = [int64]0
  foreach ($Entry in $Entries) {
    if ($Entry.Length -lt 1 -or $Entry.Length -gt $MaximumCandidateBytes -or
        $TotalUncompressedBytes -gt ($MaximumCandidateBytes - $Entry.Length)) {
      throw "Outer artifact ZIP exceeds the bounded uncompressed candidate size."
    }
    $TotalUncompressedBytes += $Entry.Length
  }
  if ($Entries.Count -ne 5 -or
      @($Entries | Where-Object { $ExpectedFiles -cnotcontains $_.FullName }).Count -ne 0 -or
      @($Entries | Where-Object {
        $_.Name -cne $_.FullName -or $_.Length -lt 1 -or $_.FullName.Contains("/") -or $_.FullName.Contains("\")
      }).Count -ne 0) {
    throw "Outer artifact ZIP is not the exact flat five-file candidate."
  }
  foreach ($Name in $ExpectedFiles) {
    $Entry = @($Entries | Where-Object { $_.FullName -ceq $Name })
    if ($Entry.Count -ne 1) { throw "Candidate ZIP contains an ambiguous entry." }
    $OutputPath = Join-Path $PayloadDirectory $Name
    $OutputStream = [IO.File]::Open($OutputPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { Copy-ZipEntryBounded $Entry[0] $OutputStream $MaximumCandidateBytes }
    finally { $OutputStream.Dispose() }
  }
}
finally { $Archive.Dispose() }

$PayloadEntries = @([IO.DirectoryInfo]::new($PayloadDirectory).GetFileSystemInfos())
if ($PayloadEntries.Count -ne 5 -or
    @($PayloadEntries | Where-Object { $ExpectedFiles -cnotcontains $_.Name }).Count -ne 0 -or
    @($PayloadEntries | Where-Object {
      $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $_.Length -lt 1
    }).Count -ne 0) {
  throw "Extracted payload is not the exact flat five-file candidate."
}
$Manifest = Join-Path $PayloadDirectory "SHA256SUMS.txt"
$ManifestRows = @(Get-CanonicalManifestRows $Manifest $ExpectedAssets)
foreach ($Row in $ManifestRows) {
  if ((Get-TrustedSha256 (Join-Path $PayloadDirectory $Row.file)) -cne $Row.sha256) {
    throw "Candidate payload SHA-256 mismatch."
  }
}
$ManifestSha256 = Get-TrustedSha256 $Manifest
Assert-Pe32PlusX64 (Join-Path $PayloadDirectory $ExpectedAssets[0])
Assert-Pe32PlusX64 (Join-Path $PayloadDirectory $ExpectedAssets[1])
Assert-ExtensionArchive `
  (Join-Path $PayloadDirectory $ExpectedAssets[3]) `
  $Version `
  (Join-Path $SourceDirectory "LICENSE") `
  $MaximumCandidateBytes
Assert-MacosArchive `
  (Join-Path $PayloadDirectory $ExpectedAssets[2]) `
  $Version `
  $SourceDirectory `
  $MaximumCandidateBytes

foreach ($Name in $ExpectedFiles) {
  $AttestationText = Invoke-TrustedProcessText $TrustedGh @(
    "attestation", "verify", (Join-Path $PayloadDirectory $Name),
    "--hostname", "github.com", "--repo", $Repository,
    "--source-ref", $WorkflowRef, "--source-digest", $SourceSha,
    "--signer-workflow", "$Repository/$WorkflowPath",
    "--deny-self-hosted-runners", "--format", "json"
  ) $Destination "GitHub attestation verification" $true 120000
  $TrimmedAttestationText = $AttestationText.Trim()
  if (-not $TrimmedAttestationText.StartsWith("[", [StringComparison]::Ordinal) -or
      -not $TrimmedAttestationText.EndsWith("]", [StringComparison]::Ordinal)) {
    throw "GitHub attestation verification did not return a JSON array."
  }
  try { $Attestations = @($AttestationText | ConvertFrom-Json) }
  catch { throw "GitHub attestation verification did not return valid JSON." }
  $SubjectSha256 = Get-TrustedSha256 (Join-Path $PayloadDirectory $Name)
  Assert-ExactAttemptAttestationSet `
    -Attestations $Attestations `
    -ExpectedInvocationUri $ExpectedInvocationUri `
    -WorkflowRunId $WorkflowRunId `
    -WorkflowPath $WorkflowPath `
    -Repository $Repository `
    -TagRef $WorkflowRef `
    -SourceSha $SourceSha `
    -SubjectName $Name `
    -SubjectSha256 $SubjectSha256
}

$Assets = New-Object Collections.Generic.List[object]
foreach ($Name in $ExpectedFiles) {
  $File = [IO.FileInfo]::new((Join-Path $PayloadDirectory $Name))
  $Assets.Add([ordered]@{
    file = $Name
    bytes = [int64]$File.Length
    sha256 = Get-TrustedSha256 $File.FullName
  })
}
$Binding = [ordered]@{
  schemaVersion = 3
  version = $Version
  releaseTag = $Tag
  repository = $Repository
  sourceSha = $SourceSha
  workflowRunId = $WorkflowRunId
  workflowRunAttempt = $WorkflowRunAttempt
  workflowEvent = "workflow_dispatch"
  workflowRef = $WorkflowRef
  workflowPath = $WorkflowPath
  artifactId = $ArtifactId
  artifactName = "release-candidate"
  artifactZipBytes = $ExpectedArtifactBytes
  artifactZipSha256 = $ArtifactZipSha256
  checksumManifestSha256 = $ManifestSha256
  attestationInvocationUri = $ExpectedInvocationUri
  attestedAssetCount = 5
  githubHostedRunner = $true
  assets = @($Assets | ForEach-Object { $_ })
  passed = $true
}
$BindingPath = Join-Path $Destination "candidate-binding.json"
Write-CreateOnceUtf8Json $BindingPath $Binding

$SecureGhToken.Dispose()
$SecureGhToken = $null
Write-Output "Windows release-candidate trust gate passed."
Write-Output "Binding: $BindingPath"
Write-Output "Payload: $PayloadDirectory"
Write-Output "Source: $SourceDirectory"
