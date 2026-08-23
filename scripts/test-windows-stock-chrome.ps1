#requires -Version 5.1

param(
  [switch]$SelfTest,
  [string]$FinalSha,
  [string]$TagObjectSha,
  [string]$WorkflowRunId,
  [string]$WorkflowRunAttempt,
  [string]$ReleaseCandidateArtifactId,
  [string]$ReleaseCandidateArtifactZipSha256,
  [string]$ManifestSha,
  [string]$Candidate,
  [string]$CandidateBinding,
  [string]$PrivateParent,
  [string]$TrustedGit,
  [string]$TrustedGh,
  [string]$CleanCoordinatorNonce
)

function Invoke-CoordinatorSelfTest {
  $Source = [IO.File]::ReadAllText($PSCommandPath, [Text.UTF8Encoding]::new($false))
  foreach ($Required in @(
    'LBB_COORDINATOR_WORKFLOW_RUN_ATTEMPT',
    '.predicate.runDetails.metadata.invocationId',
    '$Certificate.runInvocationURI',
    'scripts/test-windows-stock-chrome.ps1',
    'browser-acceptance.json',
    '-ReleaseCandidateBinding $StableCandidateBinding',
    'browser-04-stop-paused',
    'browser-05-cancel-paused',
    'browser-06-post-handback-resume',
    'FinalEntries.Count -ne 18',
    'Invoke-ExactPs51SelfTest',
    'Invoke-BoundedExactExtensionZipExtraction',
    'Extension ZIP central directory is not the exact canonical eleven-file layout.',
    'Extension ZIP entry CRC-32 does not match its central-directory declaration.'
  )) {
    if (-not $Source.Contains($Required)) {
      throw "Stock-Chrome coordinator self-test is missing $Required."
    }
  }
  $PlaceholderPrefix = 'REPLACE' + '_WITH_'
  if ($Source.Contains($PlaceholderPrefix)) {
    throw "Stock-Chrome coordinator self-test found an unresolved placeholder."
  }
  foreach ($Forbidden in @(
    ('$env:' + 'HOME'), ('$env:' + 'USERPROFILE'), ('$env:' + 'CODEX_HOME'),
    ('SetEnvironmentVariable("' + 'HOME'), ('SetEnvironmentVariable("' + 'USERPROFILE'),
    ('SetEnvironmentVariable("' + 'CODEX_HOME'),
    ('[IO.Compression.ZipFile]::' + 'ExtractToDirectory')
  )) {
    if ($Source.Contains($Forbidden)) {
      throw "Stock-Chrome coordinator self-test found a forbidden primitive or system-home mutation."
    }
  }
  if ($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1) {
    throw "Stock-Chrome coordinator self-test must execute under Windows PowerShell 5.1."
  }
  $KnownCrcBytes = [Text.Encoding]::ASCII.GetBytes("123456789")
  $KnownCrcState = Update-Crc32 ([uint32]::MaxValue) $KnownCrcBytes $KnownCrcBytes.Length (New-Crc32Table)
  $KnownCrc32 = [uint32]($KnownCrcState -bxor [uint32]::MaxValue)
  if ($KnownCrc32 -ne [uint32]3421780262) {
    throw "CRC-32 known-answer self-test failed."
  }

  Add-Type -AssemblyName System.IO.Compression -ErrorAction Stop
  Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction SilentlyContinue
  $SelfTestParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
  $SelfTestRoot = [IO.Path]::Combine(
    $SelfTestParent, "lbb-stock-chrome-zip-self-test-" + [Guid]::NewGuid().ToString("N")
  )
  $OriginalPrivateParent = $script:PrivateParent
  $script:PrivateParent = $SelfTestParent
  try {
    [IO.Directory]::CreateDirectory($SelfTestRoot) | Out-Null
    $OwnedDirectories.Add($SelfTestRoot)
    Set-OwnerPrivateDirectoryAcl $SelfTestRoot
    function New-SelfTestZip(
      [string]$Path,
      [string[]]$Names,
      [switch]$OversizedFirstEntry
    ) {
      $FileStream = [IO.File]::Open(
        $Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None
      )
      try {
        $Zip = [IO.Compression.ZipArchive]::new(
          $FileStream, [IO.Compression.ZipArchiveMode]::Create, $true
        )
        try {
          for ($Index = 0; $Index -lt $Names.Count; $Index += 1) {
            $Entry = $Zip.CreateEntry($Names[$Index], [IO.Compression.CompressionLevel]::Optimal)
            $Entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
            $Entry.ExternalAttributes = 0
            [byte[]]$Payload = $(if ($OversizedFirstEntry -and $Index -eq 0) {
              New-Object byte[] ($MaximumExtensionEntryBytes + 1)
            } else {
              [Text.Encoding]::UTF8.GetBytes("fixture-${Index}:$($Names[$Index])")
            })
            $EntryStream = $Entry.Open()
            try { $EntryStream.Write($Payload, 0, $Payload.Length) }
            finally { $EntryStream.Dispose() }
          }
        }
        finally { $Zip.Dispose() }
        $FileStream.Flush($true)
      }
      finally { $FileStream.Dispose() }
    }
    function New-SelfTestExtractionDirectory([string]$Name) {
      $Path = [IO.Path]::Combine($SelfTestRoot, $Name)
      [IO.Directory]::CreateDirectory($Path) | Out-Null
      $OwnedDirectories.Add($Path)
      Set-OwnerPrivateDirectoryAcl $Path
      return $Path
    }
    function Assert-SelfTestExtractionRejected([string]$Archive, [string]$Label) {
      $Destination = New-SelfTestExtractionDirectory ("reject-" + $Label)
      $Rejected = $false
      try {
        Invoke-BoundedExactExtensionZipExtraction `
          $Archive $Destination $CanonicalExtensionEntries (Get-TrustedSha256 $Archive)
      }
      catch { $Rejected = $true }
      if (-not $Rejected) {
        throw "Stock-Chrome bounded extension ZIP self-test accepted $Label."
      }
    }

    $ValidArchive = [IO.Path]::Combine($SelfTestRoot, "valid.zip")
    New-SelfTestZip $ValidArchive $CanonicalExtensionEntries
    $ValidDestination = New-SelfTestExtractionDirectory "valid-output"
    Invoke-BoundedExactExtensionZipExtraction `
      $ValidArchive $ValidDestination $CanonicalExtensionEntries (Get-TrustedSha256 $ValidArchive)
    if ([IO.DirectoryInfo]::new($ValidDestination).GetFileSystemInfos().Count -ne 11) {
      throw "Stock-Chrome bounded extension ZIP valid fixture failed."
    }

    $CrcArchive = [IO.Path]::Combine($SelfTestRoot, "crc-mismatch.zip")
    $CrcBytes = [IO.File]::ReadAllBytes($ValidArchive)
    $EndOffset = $CrcBytes.Length - 22
    $CentralOffset = [int][BitConverter]::ToUInt32($CrcBytes, $EndOffset + 16)
    $CrcBytes[14] = [byte]($CrcBytes[14] -bxor 1)
    $CrcBytes[$CentralOffset + 16] = [byte]($CrcBytes[$CentralOffset + 16] -bxor 1)
    [IO.File]::WriteAllBytes($CrcArchive, $CrcBytes)
    Assert-SelfTestExtractionRejected $CrcArchive "a CRC-32 mismatch"

    $OversizedArchive = [IO.Path]::Combine($SelfTestRoot, "oversized.zip")
    New-SelfTestZip $OversizedArchive $CanonicalExtensionEntries -OversizedFirstEntry
    Assert-SelfTestExtractionRejected $OversizedArchive "an oversized declared entry"

    $TraversalNames = @($CanonicalExtensionEntries)
    $TraversalNames[0] = "../evil.js"
    $TraversalArchive = [IO.Path]::Combine($SelfTestRoot, "traversal.zip")
    New-SelfTestZip $TraversalArchive $TraversalNames
    Assert-SelfTestExtractionRejected $TraversalArchive "a traversal entry"

    $DuplicateNames = @($CanonicalExtensionEntries)
    $DuplicateNames[1] = $DuplicateNames[0]
    $DuplicateArchive = [IO.Path]::Combine($SelfTestRoot, "duplicate.zip")
    New-SelfTestZip $DuplicateArchive $DuplicateNames
    Assert-SelfTestExtractionRejected $DuplicateArchive "a duplicate entry"
  }
  finally {
    try {
      if ([IO.Directory]::Exists($SelfTestRoot)) { Remove-TestOwnedTree $SelfTestRoot }
    }
    finally { $script:PrivateParent = $OriginalPrivateParent }
  }
  Write-Output "Windows stock-Chrome coordinator self-test passed."
}

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
$CleanPowerShell = [IO.Path]::GetFullPath([IO.Path]::Combine(
  $MachineSystemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"
))
if ([String]::IsNullOrWhiteSpace($CleanCoordinatorNonce)) {
  if (-not $SelfTest) {
    foreach ($RequiredInput in @(
      $FinalSha, $TagObjectSha, $WorkflowRunId, $WorkflowRunAttempt,
      $ReleaseCandidateArtifactId, $ReleaseCandidateArtifactZipSha256,
      $ManifestSha, $Candidate, $CandidateBinding, $PrivateParent,
      $TrustedGit, $TrustedGh
    )) {
      if ([String]::IsNullOrWhiteSpace($RequiredInput)) {
        throw "Every exact-candidate coordinator parameter is required."
      }
    }
  }
  if ([String]::IsNullOrWhiteSpace($PSCommandPath) -or
      -not [IO.Path]::IsPathRooted($PSCommandPath) -or
      -not [IO.File]::Exists($PSCommandPath) -or
      ([IO.FileInfo]::new($PSCommandPath).Attributes -band [IO.FileAttributes]::ReparsePoint) -or
      -not [IO.File]::Exists($CleanPowerShell) -or
      ([IO.FileInfo]::new($CleanPowerShell).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "The clean coordinator child requires ordinary absolute script and powershell.exe paths."
  }
  $Ancestor = [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName($PSCommandPath))
  while ($null -ne $Ancestor) {
    if ($Ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) {
      throw "The coordinator script path must not traverse a reparse-point directory."
    }
    $Ancestor = $Ancestor.Parent
  }
  $Nonce = [Guid]::NewGuid().ToString("N")
  $Info = [Diagnostics.ProcessStartInfo]::new()
  $Info.FileName = $CleanPowerShell
  $Info.WorkingDirectory = [IO.Path]::GetDirectoryName($CleanPowerShell)
  $Info.Arguments = '-NoLogo -NoProfile -File "' + $PSCommandPath + '" -CleanCoordinatorNonce ' + $Nonce
  $Info.UseShellExecute = $false
  $Info.EnvironmentVariables["LBB_CLEAN_COORDINATOR_NONCE"] = $Nonce
  $Info.EnvironmentVariables["LBB_COORDINATOR_SELF_TEST"] = $(if ($SelfTest) { "1" } else { "0" })
  if (-not $SelfTest) {
    $Info.EnvironmentVariables["LBB_COORDINATOR_FINAL_SHA"] = $FinalSha
    $Info.EnvironmentVariables["LBB_COORDINATOR_TAG_OBJECT_SHA"] = $TagObjectSha
    $Info.EnvironmentVariables["LBB_COORDINATOR_WORKFLOW_RUN_ID"] = $WorkflowRunId
    $Info.EnvironmentVariables["LBB_COORDINATOR_WORKFLOW_RUN_ATTEMPT"] = $WorkflowRunAttempt
    $Info.EnvironmentVariables["LBB_COORDINATOR_ARTIFACT_ID"] = $ReleaseCandidateArtifactId
    $Info.EnvironmentVariables["LBB_COORDINATOR_ARTIFACT_ZIP_SHA256"] = $ReleaseCandidateArtifactZipSha256
    $Info.EnvironmentVariables["LBB_COORDINATOR_MANIFEST_SHA256"] = $ManifestSha
    $Info.EnvironmentVariables["LBB_COORDINATOR_CANDIDATE"] = $Candidate
    $Info.EnvironmentVariables["LBB_COORDINATOR_CANDIDATE_BINDING"] = $CandidateBinding
    $Info.EnvironmentVariables["LBB_COORDINATOR_PRIVATE_PARENT"] = $PrivateParent
    $Info.EnvironmentVariables["LBB_COORDINATOR_TRUSTED_GIT"] = $TrustedGit
    $Info.EnvironmentVariables["LBB_COORDINATOR_TRUSTED_GH"] = $TrustedGh
  }
  $Child = [Diagnostics.Process]::new()
  $Child.StartInfo = $Info
  try {
    if (-not $Child.Start()) { throw "The clean coordinator child did not start." }
    $Child.WaitForExit()
    $ChildExitCode = $Child.ExitCode
  }
  finally { $Child.Dispose() }
  exit $ChildExitCode
}
$ExpectedCleanNonce = [Environment]::GetEnvironmentVariable("LBB_CLEAN_COORDINATOR_NONCE", "Process")
if ($CleanCoordinatorNonce -cnotmatch '^[0-9a-f]{32}$' -or
    $CleanCoordinatorNonce -cne $ExpectedCleanNonce -or
    -not [Environment]::Is64BitProcess -or
    -not [String]::Equals(
      [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName,
      $CleanPowerShell,
      [StringComparison]::OrdinalIgnoreCase
    )) {
  throw "The acceptance body must run only in its exact self-spawned 64-bit -NoProfile child."
}
$SelfTestRequested = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_SELF_TEST", "Process") -ceq "1"
[Environment]::SetEnvironmentVariable("LBB_CLEAN_COORDINATOR_NONCE", $null, "Process")
$FinalSha = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_FINAL_SHA", "Process")
$TagObjectSha = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_TAG_OBJECT_SHA", "Process")
$WorkflowRunId = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_WORKFLOW_RUN_ID", "Process")
$WorkflowRunAttempt = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_WORKFLOW_RUN_ATTEMPT", "Process")
$ReleaseCandidateArtifactId = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_ARTIFACT_ID", "Process")
$ReleaseCandidateArtifactZipSha256 = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_ARTIFACT_ZIP_SHA256", "Process")
$ManifestSha = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_MANIFEST_SHA256", "Process")
$Candidate = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_CANDIDATE", "Process")
$CandidateBinding = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_CANDIDATE_BINDING", "Process")
$PrivateParent = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_PRIVATE_PARENT", "Process")
$TrustedGit = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_TRUSTED_GIT", "Process")
$TrustedGh = [Environment]::GetEnvironmentVariable("LBB_COORDINATOR_TRUSTED_GH", "Process")
foreach ($HandoffName in @(
  "LBB_COORDINATOR_SELF_TEST",
  "LBB_COORDINATOR_FINAL_SHA", "LBB_COORDINATOR_TAG_OBJECT_SHA",
  "LBB_COORDINATOR_WORKFLOW_RUN_ID", "LBB_COORDINATOR_WORKFLOW_RUN_ATTEMPT",
  "LBB_COORDINATOR_ARTIFACT_ID", "LBB_COORDINATOR_ARTIFACT_ZIP_SHA256",
  "LBB_COORDINATOR_MANIFEST_SHA256", "LBB_COORDINATOR_CANDIDATE",
  "LBB_COORDINATOR_CANDIDATE_BINDING", "LBB_COORDINATOR_PRIVATE_PARENT",
  "LBB_COORDINATOR_TRUSTED_GIT", "LBB_COORDINATOR_TRUSTED_GH"
)) {
  [Environment]::SetEnvironmentVariable($HandoffName, $null, "Process")
}
$TrustedPSModulePath = [IO.Path]::GetFullPath([IO.Path]::Combine($PSHOME, "Modules"))
if (-not [IO.Directory]::Exists($TrustedPSModulePath) -or
    ([IO.DirectoryInfo]::new($TrustedPSModulePath).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
  throw "The exact Windows PowerShell system module root is unavailable or linked."
}
[Environment]::SetEnvironmentVariable("PSModulePath", $TrustedPSModulePath, "Process")
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Version = "0.12.12"
$CanonicalExtensionEntries = @(
  "background.js", "content.js", "dom-core.js", "frame-agent.js", "lib.js",
  "manifest.json", "popup.css", "popup.html", "popup.js", "stop-guard.js", "LICENSE"
)
$MaximumExtensionEntryBytes = 2MB
$MaximumExtensionPayloadBytes = 8MB
$MaximumExtensionArchiveBytes = 8MB
$ShortSourceParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$Origin = "https://github.com/flrngel/local-browser-bridge.git"
$ExpectedInvocationUri = "https://github.com/flrngel/local-browser-bridge/actions/runs/$WorkflowRunId/attempts/$WorkflowRunAttempt"

if (-not $SelfTestRequested) {
  if ($Version -cne "0.12.12" -or $FinalSha -cnotmatch '^[0-9a-f]{40}$' -or
      $TagObjectSha -cnotmatch '^[0-9a-f]{40}$' -or
      $WorkflowRunId -cnotmatch '^[1-9][0-9]*$' -or
      $WorkflowRunAttempt -cnotmatch '^[1-9][0-9]*$' -or
      $ReleaseCandidateArtifactId -cnotmatch '^[1-9][0-9]*$' -or
      $ReleaseCandidateArtifactZipSha256 -cnotmatch '^[0-9a-f]{64}$' -or
      $ManifestSha -cnotmatch '^[0-9a-f]{64}$') {
    throw "Coordinator candidate identifiers are not canonical."
  }
  foreach ($Tool in @($TrustedGit, $TrustedGh)) {
    if (-not [IO.Path]::IsPathRooted($Tool) -or -not [IO.File]::Exists($Tool) -or
        ([IO.FileInfo]::new($Tool).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
      throw "Trusted Git and gh must be absolute ordinary executables."
    }
  }
  if (-not [IO.Directory]::Exists($Candidate) -or
      ([IO.DirectoryInfo]::new($Candidate).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "Candidate must be an existing ordinary private download directory."
  }
  if ([String]::IsNullOrWhiteSpace($CandidateBinding) -or
      -not [IO.Path]::IsPathRooted($CandidateBinding) -or
      -not [IO.File]::Exists($CandidateBinding) -or
      ([IO.FileInfo]::new($CandidateBinding).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "CandidateBinding must be an existing absolute ordinary file."
  }
  if (-not [IO.Directory]::Exists($PrivateParent) -or
      ([IO.DirectoryInfo]::new($PrivateParent).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "PrivateParent must be an existing ordinary test-owned directory."
  }
  if (-not [IO.Directory]::Exists($ShortSourceParent) -or $ShortSourceParent.Length -gt 80 -or
      ([IO.DirectoryInfo]::new($ShortSourceParent).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "The system temporary directory must be an existing short ordinary source parent."
  }
}

$OwnedDirectories = New-Object Collections.Generic.List[string]
function New-PrivateEmptyDirectory([string]$Prefix) {
  $Path = [IO.Path]::GetFullPath((Join-Path $PrivateParent ($Prefix + [Guid]::NewGuid().ToString("N"))))
  $ParentPrefix = [IO.Path]::GetFullPath($PrivateParent).TrimEnd('\') + '\'
  if (-not $Path.StartsWith($ParentPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "New directory path escaped PrivateParent."
  }
  $OwnedDirectories.Add($Path)
  [IO.Directory]::CreateDirectory($Path) | Out-Null
  Set-OwnerPrivateDirectoryAcl $Path
  $Item = [IO.DirectoryInfo]::new($Path)
  if (($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
      $Item.GetFileSystemInfos().Count -ne 0) { throw "New directory is not empty and ordinary." }
  return $Item.FullName
}

function New-ShortSourceDirectory {
  $Path = [IO.Path]::GetFullPath((Join-Path $ShortSourceParent ("lbb-src-" + [Guid]::NewGuid().ToString("N"))))
  $ParentPrefix = $ShortSourceParent + '\'
  if (-not $Path.StartsWith($ParentPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "New source directory escaped the short source parent."
  }
  $OwnedDirectories.Add($Path)
  [IO.Directory]::CreateDirectory($Path) | Out-Null
  Set-OwnerPrivateDirectoryAcl $Path
  $Item = [IO.DirectoryInfo]::new($Path)
  if (($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
      $Item.GetFileSystemInfos().Count -ne 0) { throw "New source directory is not empty and ordinary." }
  return $Item.FullName
}

function Get-TrustedSha256([string]$Path) {
  $Stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
  try {
    $Hasher = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($Hasher.ComputeHash($Stream))).Replace("-", "").ToLowerInvariant() }
    finally { $Hasher.Dispose() }
  }
  finally { $Stream.Dispose() }
}

function Read-ExactArchiveBytes([IO.Stream]$Stream, [int]$Count, [string]$Label) {
  if ($Count -lt 1) { throw "$Label has an invalid byte count." }
  $Buffer = New-Object byte[] $Count
  $Offset = 0
  while ($Offset -lt $Count) {
    $Read = $Stream.Read($Buffer, $Offset, $Count - $Offset)
    if ($Read -lt 1) { throw "Extension ZIP is truncated while reading $Label." }
    $Offset += $Read
  }
  return ,$Buffer
}

function Get-OpenStreamSha256([IO.Stream]$Stream) {
  if (-not $Stream.CanRead -or -not $Stream.CanSeek) {
    throw "Extension ZIP hashing requires one readable seekable stream."
  }
  $OriginalPosition = $Stream.Position
  $Hasher = [Security.Cryptography.SHA256]::Create()
  try {
    $Stream.Position = 0
    return ([BitConverter]::ToString($Hasher.ComputeHash($Stream))).Replace("-", "").ToLowerInvariant()
  }
  finally {
    $Hasher.Dispose()
    $Stream.Position = $OriginalPosition
  }
}

function New-Crc32Table {
  $Table = [uint32[]]::new(256)
  for ($Index = 0; $Index -lt 256; $Index += 1) {
    $Value = [uint32]$Index
    for ($Bit = 0; $Bit -lt 8; $Bit += 1) {
      if (($Value -band 1) -ne 0) {
        $Value = [uint32](([uint32]3988292384) -bxor ($Value -shr 1))
      }
      else { $Value = [uint32]($Value -shr 1) }
    }
    $Table[$Index] = $Value
  }
  return ,$Table
}

function Update-Crc32(
  [uint32]$State,
  [byte[]]$Buffer,
  [int]$Count,
  [uint32[]]$Table
) {
  if ($Count -lt 0 -or $Count -gt $Buffer.Length -or $Table.Length -ne 256) {
    throw "CRC-32 update received an invalid buffer range or table."
  }
  for ($Index = 0; $Index -lt $Count; $Index += 1) {
    $Lookup = [int](($State -bxor [uint32]$Buffer[$Index]) -band 0xff)
    $State = [uint32](($State -shr 8) -bxor $Table[$Lookup])
  }
  return $State
}

function Get-ValidatedExtensionZipCentralDirectory(
  [IO.FileStream]$Stream,
  [string[]]$ExpectedNames,
  [int64]$MaximumEntryBytes,
  [int64]$MaximumPayloadBytes
) {
  if ($Stream.Length -lt 22) { throw "Extension ZIP is shorter than one canonical end record." }
  $Stream.Position = $Stream.Length - 22
  $End = Read-ExactArchiveBytes $Stream 22 "the end-of-central-directory record"
  if ([BitConverter]::ToUInt32($End, 0) -ne 0x06054b50 -or
      [BitConverter]::ToUInt16($End, 4) -ne 0 -or
      [BitConverter]::ToUInt16($End, 6) -ne 0 -or
      [BitConverter]::ToUInt16($End, 8) -ne $ExpectedNames.Count -or
      [BitConverter]::ToUInt16($End, 10) -ne $ExpectedNames.Count -or
      [BitConverter]::ToUInt16($End, 20) -ne 0) {
    throw "Extension ZIP end record is not one single-disk, comment-free, exact-entry record."
  }
  $CentralSize = [int64][BitConverter]::ToUInt32($End, 12)
  $CentralOffset = [int64][BitConverter]::ToUInt32($End, 16)
  if ($CentralSize -lt 1 -or $CentralOffset -lt 1 -or
      $CentralOffset -gt ($Stream.Length - 22) -or
      $CentralSize -ne (($Stream.Length - 22) - $CentralOffset)) {
    throw "Extension ZIP central-directory bounds or trailing bytes are invalid."
  }

  $StrictAscii = [Text.Encoding]::GetEncoding(
    "us-ascii",
    [Text.EncoderFallback]::ExceptionFallback,
    [Text.DecoderFallback]::ExceptionFallback
  )
  $Records = New-Object Collections.Generic.List[object]
  $SeenNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
  $TotalPayloadBytes = [int64]0
  $Stream.Position = $CentralOffset
  for ($Index = 0; $Index -lt $ExpectedNames.Count; $Index += 1) {
    $Header = Read-ExactArchiveBytes $Stream 46 "a central-directory header"
    if ([BitConverter]::ToUInt32($Header, 0) -ne 0x02014b50) {
      throw "Extension ZIP central-directory signature is invalid."
    }
    $Flags = [int][BitConverter]::ToUInt16($Header, 8)
    $CompressionMethod = [int][BitConverter]::ToUInt16($Header, 10)
    $Crc32 = [uint32][BitConverter]::ToUInt32($Header, 16)
    $CompressedLength = [int64][BitConverter]::ToUInt32($Header, 20)
    $DeclaredLength = [int64][BitConverter]::ToUInt32($Header, 24)
    $NameLength = [int][BitConverter]::ToUInt16($Header, 28)
    $ExtraLength = [int][BitConverter]::ToUInt16($Header, 30)
    $CommentLength = [int][BitConverter]::ToUInt16($Header, 32)
    $DiskStart = [int][BitConverter]::ToUInt16($Header, 34)
    $ExternalAttributes = [uint32][BitConverter]::ToUInt32($Header, 38)
    $LocalHeaderOffset = [int64][BitConverter]::ToUInt32($Header, 42)
    if (($Flags -band 0x2041) -ne 0) {
      throw "Extension ZIP contains an encrypted entry."
    }
    if ($Flags -ne 0) { throw "Extension ZIP uses unsupported general-purpose flags." }
    if ($CompressionMethod -notin @(0, 8)) {
      throw "Extension ZIP uses an unsupported compression method."
    }
    if ($NameLength -lt 1 -or $ExtraLength -ne 0 -or $CommentLength -ne 0 -or $DiskStart -ne 0) {
      throw "Extension ZIP entry metadata is not canonical."
    }
    $NameBytes = Read-ExactArchiveBytes $Stream $NameLength "a central-directory name"
    try { $Name = $StrictAscii.GetString($NameBytes) }
    catch { throw "Extension ZIP entry name is not strict ASCII." }
    if ($Name -cne $ExpectedNames[$Index] -or $Name.Contains('/') -or $Name.Contains('\') -or
        [IO.Path]::IsPathRooted($Name) -or -not $SeenNames.Add($Name)) {
      throw "Extension ZIP contains a duplicate, traversal-capable, or noncanonical entry name."
    }
    $UnixType = ($ExternalAttributes -shr 16) -band 0xf000
    if (($ExternalAttributes -band 0x10) -ne 0 -or
        ($UnixType -ne 0 -and $UnixType -ne 0x8000)) {
      throw "Extension ZIP contains a directory, link, or special entry."
    }
    if ($DeclaredLength -lt 1 -or $DeclaredLength -gt $MaximumEntryBytes -or
        $CompressedLength -lt 1 -or $CompressedLength -gt $MaximumPayloadBytes -or
        ($CompressionMethod -eq 0 -and $CompressedLength -ne $DeclaredLength) -or
        $TotalPayloadBytes -gt ($MaximumPayloadBytes - $DeclaredLength)) {
      throw "Extension ZIP entry exceeds its declared per-entry or total byte bound."
    }
    $TotalPayloadBytes += $DeclaredLength
    $Records.Add([pscustomobject]@{
      Name = $Name
      Flags = $Flags
      CompressionMethod = $CompressionMethod
      Crc32 = $Crc32
      CompressedLength = $CompressedLength
      DeclaredLength = $DeclaredLength
      ExternalAttributes = $ExternalAttributes
      LocalHeaderOffset = $LocalHeaderOffset
    })
  }
  if ($Stream.Position -ne ($CentralOffset + $CentralSize) -or
      $Records.Count -ne 11 -or $SeenNames.Count -ne 11) {
    throw "Extension ZIP central directory is not the exact canonical eleven-file layout."
  }

  $ExpectedLocalOffset = [int64]0
  foreach ($Record in @($Records | Sort-Object LocalHeaderOffset)) {
    if ($Record.LocalHeaderOffset -ne $ExpectedLocalOffset -or
        $Record.LocalHeaderOffset -gt ($CentralOffset - 30)) {
      throw "Extension ZIP local records overlap, contain gaps, or escape the archive body."
    }
    $Stream.Position = $Record.LocalHeaderOffset
    $Local = Read-ExactArchiveBytes $Stream 30 "a local file header"
    $LocalNameLength = [int][BitConverter]::ToUInt16($Local, 26)
    $LocalExtraLength = [int][BitConverter]::ToUInt16($Local, 28)
    if ([BitConverter]::ToUInt32($Local, 0) -ne 0x04034b50 -or
        [BitConverter]::ToUInt16($Local, 6) -ne $Record.Flags -or
        [BitConverter]::ToUInt16($Local, 8) -ne $Record.CompressionMethod -or
        [BitConverter]::ToUInt32($Local, 14) -ne $Record.Crc32 -or
        [BitConverter]::ToUInt32($Local, 18) -ne $Record.CompressedLength -or
        [BitConverter]::ToUInt32($Local, 22) -ne $Record.DeclaredLength -or
        $LocalNameLength -lt 1 -or $LocalExtraLength -ne 0) {
      throw "Extension ZIP local and central metadata differ or use unsupported metadata."
    }
    $LocalNameBytes = Read-ExactArchiveBytes $Stream $LocalNameLength "a local file name"
    try { $LocalName = $StrictAscii.GetString($LocalNameBytes) }
    catch { throw "Extension ZIP local entry name is not strict ASCII." }
    if ($LocalName -cne $Record.Name) {
      throw "Extension ZIP local and central entry names differ."
    }
    $ExpectedLocalOffset = $Record.LocalHeaderOffset + 30 + $LocalNameLength + $Record.CompressedLength
    if ($ExpectedLocalOffset -gt $CentralOffset) {
      throw "Extension ZIP compressed data escapes the archive body."
    }
  }
  if ($ExpectedLocalOffset -ne $CentralOffset) {
    throw "Extension ZIP contains hidden bytes between local records and its central directory."
  }
  return @($Records | ForEach-Object { $_ })
}

function Invoke-BoundedExactExtensionZipExtraction(
  [string]$ArchivePath,
  [string]$Destination,
  [string[]]$ExpectedNames,
  [string]$ExpectedSha256
) {
  $ArchiveFull = [IO.Path]::GetFullPath($ArchivePath)
  $DestinationFull = [IO.Path]::GetFullPath($Destination)
  if ($ExpectedSha256 -cnotmatch '^[0-9a-f]{64}$' -or
      -not [IO.File]::Exists($ArchiveFull) -or
      ([IO.FileInfo]::new($ArchiveFull).Attributes -band [IO.FileAttributes]::ReparsePoint) -or
      -not [IO.Directory]::Exists($DestinationFull) -or
      ([IO.DirectoryInfo]::new($DestinationFull).Attributes -band [IO.FileAttributes]::ReparsePoint) -or
      -not $OwnedDirectories.Contains($DestinationFull)) {
    throw "Bounded extension extraction inputs are not exact ordinary run-owned paths."
  }
  Assert-NoReparseAncestorChain $ArchiveFull "extension ZIP"
  Assert-NoReparseAncestorChain $DestinationFull "extension extraction directory"
  Assert-OwnerPrivateDirectoryAcl $DestinationFull "Extension extraction directory"
  if ([IO.DirectoryInfo]::new($DestinationFull).GetFileSystemInfos().Count -ne 0) {
    throw "Extension extraction directory must be empty."
  }
  $ArchiveInfo = [IO.FileInfo]::new($ArchiveFull)
  if ($ArchiveInfo.Length -lt 1 -or $ArchiveInfo.Length -gt $MaximumExtensionArchiveBytes) {
    throw "Extension ZIP exceeds its compressed archive byte bound."
  }
  $BeforeLength = $ArchiveInfo.Length
  $BeforeCreationTicks = $ArchiveInfo.CreationTimeUtc.Ticks
  $BeforeWriteTicks = $ArchiveInfo.LastWriteTimeUtc.Ticks

  Add-Type -AssemblyName System.IO.Compression -ErrorAction Stop
  Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction SilentlyContinue
  $ArchiveStream = [IO.File]::Open(
    $ArchiveFull, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read
  )
  try {
    if ($ArchiveStream.Length -ne $BeforeLength) { throw "Extension ZIP identity changed before extraction." }
    $BeforeSha256 = Get-OpenStreamSha256 $ArchiveStream
    if ($BeforeSha256 -cne $ExpectedSha256) {
      throw "Extension ZIP does not match its already validated checksum-manifest entry."
    }
    $CentralRecords = @(Get-ValidatedExtensionZipCentralDirectory `
      $ArchiveStream $ExpectedNames $MaximumExtensionEntryBytes $MaximumExtensionPayloadBytes)
    $ArchiveStream.Position = 0
    $Zip = [IO.Compression.ZipArchive]::new(
      $ArchiveStream, [IO.Compression.ZipArchiveMode]::Read, $true
    )
    try {
      $Entries = @($Zip.Entries)
      if ($Entries.Count -ne $ExpectedNames.Count) {
        throw "Extension ZIP decompressor view has an unexpected entry count."
      }
      $ObservedTotal = [int64]0
      $Crc32Table = New-Crc32Table
      for ($Index = 0; $Index -lt $ExpectedNames.Count; $Index += 1) {
        $Entry = $Entries[$Index]
        $Record = $CentralRecords[$Index]
        $EntryExternalAttributes = [BitConverter]::ToUInt32(
          [BitConverter]::GetBytes([int32]$Entry.ExternalAttributes), 0
        )
        if ($Entry.FullName -cne $Record.Name -or $Entry.Name -cne $Record.Name -or
            $Entry.Length -ne $Record.DeclaredLength -or
            $Entry.CompressedLength -ne $Record.CompressedLength -or
            $EntryExternalAttributes -ne $Record.ExternalAttributes) {
          throw "Extension ZIP parser and decompressor metadata views differ."
        }
        $OutputPath = [IO.Path]::GetFullPath((Join-Path $DestinationFull $Record.Name))
        $DestinationPrefix = $DestinationFull.TrimEnd('\') + '\'
        if (-not $OutputPath.StartsWith($DestinationPrefix, [StringComparison]::OrdinalIgnoreCase)) {
          throw "Extension ZIP output path escaped its exact destination."
        }
        $InputStream = $Entry.Open()
        $OutputStream = [IO.File]::Open(
          $OutputPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
        )
        $ObservedEntry = [int64]0
        $Crc32State = [uint32]::MaxValue
        try {
          $Buffer = New-Object byte[] 65536
          while (($Read = $InputStream.Read($Buffer, 0, $Buffer.Length)) -gt 0) {
            if ($ObservedEntry -gt ($MaximumExtensionEntryBytes - $Read) -or
                $ObservedEntry -gt ($Record.DeclaredLength - $Read) -or
                $ObservedTotal -gt ($MaximumExtensionPayloadBytes - $Read)) {
              throw "Extension ZIP entry expanded beyond its declared or allowed byte bound."
            }
            $OutputStream.Write($Buffer, 0, $Read)
            $Crc32State = Update-Crc32 $Crc32State $Buffer $Read $Crc32Table
            $ObservedEntry += $Read
            $ObservedTotal += $Read
          }
          if ($ObservedEntry -ne $Record.DeclaredLength) {
            throw "Extension ZIP entry did not expand to its declared length."
          }
          $ObservedCrc32 = [uint32]($Crc32State -bxor [uint32]::MaxValue)
          if ($ObservedCrc32 -ne $Record.Crc32) {
            throw "Extension ZIP entry CRC-32 does not match its central-directory declaration."
          }
          $OutputStream.Flush($true)
        }
        finally {
          $OutputStream.Dispose()
          $InputStream.Dispose()
        }
        $OutputInfo = [IO.FileInfo]::new($OutputPath)
        if ($OutputInfo.Length -ne $Record.DeclaredLength -or
            ($OutputInfo.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
          throw "Extracted extension entry is not one exact ordinary declared-length file."
        }
      }
      if ($ObservedTotal -ne ($CentralRecords | Measure-Object DeclaredLength -Sum).Sum) {
        throw "Extension ZIP observed and declared total lengths differ."
      }
    }
    finally { $Zip.Dispose() }
    if ((Get-OpenStreamSha256 $ArchiveStream) -cne $BeforeSha256) {
      throw "Extension ZIP hash changed during bounded streaming extraction."
    }
  }
  finally { $ArchiveStream.Dispose() }

  $ArchiveInfo.Refresh()
  if ($ArchiveInfo.Length -ne $BeforeLength -or
      $ArchiveInfo.CreationTimeUtc.Ticks -ne $BeforeCreationTicks -or
      $ArchiveInfo.LastWriteTimeUtc.Ticks -ne $BeforeWriteTicks -or
      (Get-TrustedSha256 $ArchiveFull) -cne $ExpectedSha256) {
    throw "Extension ZIP path identity or hash changed across bounded extraction."
  }
  $Extracted = @([IO.DirectoryInfo]::new($DestinationFull).GetFileSystemInfos())
  if ($Extracted.Count -ne 11 -or
      @(Compare-Object ($Extracted.Name | Sort-Object) ($ExpectedNames | Sort-Object)).Count -ne 0 -or
      @($Extracted | Where-Object {
        $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)
      }).Count -ne 0) {
    throw "Bounded extension extraction did not produce the exact eleven ordinary files."
  }
}

function Assert-NoReparseAncestorChain([string]$Path, [string]$Label) {
  $Full = [IO.Path]::GetFullPath($Path)
  $Ancestor = $(if ([IO.Directory]::Exists($Full)) {
    [IO.DirectoryInfo]::new($Full)
  } else {
    [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName($Full))
  })
  while ($null -ne $Ancestor) {
    if ($Ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) {
      throw "$Label must not traverse a reparse-point directory."
    }
    $Ancestor = $Ancestor.Parent
  }
}

function Assert-OwnerPrivateDirectoryAcl([string]$Path, [string]$Label) {
  $Full = [IO.Path]::GetFullPath($Path)
  if (-not [IO.Directory]::Exists($Full)) { throw "$Label does not exist." }
  Assert-NoReparseAncestorChain $Full $Label
  $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  try {
    $Observed = [IO.Directory]::GetAccessControl(
      $Full,
      [Security.AccessControl.AccessControlSections]::Access -bor
        [Security.AccessControl.AccessControlSections]::Owner
    )
    $Owner = $Observed.GetOwner([Security.Principal.SecurityIdentifier])
    $Rules = @($Observed.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
    $FullControlRule = @($Rules | Where-Object {
      $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
      $_.IdentityReference.Value -ceq $Identity.User.Value -and
      ($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -eq
        [Security.AccessControl.FileSystemRights]::FullControl -and
      -not $_.IsInherited
    })
    if ($Owner.Value -cne $Identity.User.Value -or -not $Observed.AreAccessRulesProtected -or
        $Rules.Count -lt 1 -or $FullControlRule.Count -lt 1 -or
        @($Rules | Where-Object {
          $_.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
          $_.IdentityReference.Value -cne $Identity.User.Value -or $_.IsInherited
        }).Count -ne 0) {
      throw "$Label ACL is not protected and private to the current user."
    }
  }
  finally { $Identity.Dispose() }
}

function Set-OwnerPrivateDirectoryAcl([string]$Path) {
  $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  try {
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
    [IO.Directory]::SetAccessControl($Path, $Security)
  }
  finally { $Identity.Dispose() }
  Assert-OwnerPrivateDirectoryAcl $Path "Fresh acceptance directory"
}

function Remove-TestOwnedTree([string]$Path) {
  if ([String]::IsNullOrWhiteSpace($Path) -or -not [IO.Directory]::Exists($Path)) { return }
  $Full = [IO.Path]::GetFullPath($Path)
  $PrivateParentPrefix = [IO.Path]::GetFullPath($PrivateParent).TrimEnd('\') + '\'
  $ShortSourceParentPrefix = $ShortSourceParent + '\'
  $UnderApprovedParent = (
    $Full.StartsWith($PrivateParentPrefix, [StringComparison]::OrdinalIgnoreCase) -or
    $Full.StartsWith($ShortSourceParentPrefix, [StringComparison]::OrdinalIgnoreCase)
  )
  if (-not $OwnedDirectories.Contains($Full) -or
      -not $UnderApprovedParent) {
    throw "Cleanup refused a directory without exact random-run ownership."
  }
  function Remove-VerifiedNode([IO.DirectoryInfo]$Directory) {
    if ($Directory.Attributes -band [IO.FileAttributes]::ReparsePoint) {
      throw "Cleanup refused a reparse-point directory."
    }
    foreach ($Entry in $Directory.GetFileSystemInfos()) {
      if ($Entry.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Cleanup refused a reparse-point entry."
      }
      if ($Entry -is [IO.DirectoryInfo]) { Remove-VerifiedNode $Entry }
      elseif ($Entry -is [IO.FileInfo]) { [IO.File]::Delete($Entry.FullName) }
      else { throw "Cleanup refused an unknown filesystem entry." }
    }
    [IO.Directory]::Delete($Directory.FullName, $false)
  }
  Remove-VerifiedNode ([IO.DirectoryInfo]::new($Full))
}

function Remove-ExactFlatOwnedDirectory([string]$Path, [string[]]$ExpectedNames) {
  $Full = [IO.Path]::GetFullPath($Path)
  if (-not $OwnedDirectories.Contains($Full)) { throw "Flat cleanup lacks exact run ownership." }
  $Directory = [IO.DirectoryInfo]::new($Full)
  if ($Directory.Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw "Flat cleanup refused a reparse-point directory."
  }
  $Entries = @($Directory.GetFileSystemInfos())
  if ($Entries.Count -ne $ExpectedNames.Count -or
      @($Entries | Where-Object {
        $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)
      }).Count -ne 0 -or
      @(Compare-Object ($Entries.Name | Sort-Object) ($ExpectedNames | Sort-Object)).Count -ne 0) {
    throw "Flat cleanup refused an unexpected or linked inventory."
  }
  foreach ($Name in $ExpectedNames) { [IO.File]::Delete((Join-Path $Full $Name)) }
  [IO.Directory]::Delete($Full, $false)
}

function Write-CreateOnceAttemptRecord([string]$Directory, [string]$Name, [object]$Record) {
  if ($Name -notin @("attempt-start.json", "failure.json", "cleanup.json") -or
      -not $OwnedDirectories.Contains([IO.Path]::GetFullPath($Directory))) {
    throw "Attempt-record creation refused a noncanonical name or unowned directory."
  }
  $Path = [IO.Path]::GetFullPath((Join-Path $Directory $Name))
  $Prefix = [IO.Path]::GetFullPath($Directory).TrimEnd('\') + '\'
  if (-not $Path.StartsWith($Prefix, [StringComparison]::OrdinalIgnoreCase) -or
      [IO.File]::Exists($Path) -or [IO.Directory]::Exists($Path)) {
    throw "Attempt record must be a new file inside its exact owned directory."
  }
  $Bytes = [Text.UTF8Encoding]::new($false, $true).GetBytes(
    (($Record | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
  )
  $Stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try { $Stream.Write($Bytes, 0, $Bytes.Length); $Stream.Flush($true) }
  finally { $Stream.Dispose(); [Array]::Clear($Bytes, 0, $Bytes.Length) }
}

if ($SelfTestRequested) {
  Invoke-CoordinatorSelfTest
  return
}

foreach ($CheckedPath in @(
  [pscustomobject]@{ Path = $PSCommandPath; Label = "coordinator script" },
  [pscustomobject]@{ Path = $CleanPowerShell; Label = "Windows PowerShell 5.1 executable" },
  [pscustomobject]@{ Path = $TrustedGit; Label = "trusted Git executable" },
  [pscustomobject]@{ Path = $TrustedGh; Label = "trusted gh executable" },
  [pscustomobject]@{ Path = $Candidate; Label = "candidate directory" },
  [pscustomobject]@{ Path = $CandidateBinding; Label = "candidate-binding file" },
  [pscustomobject]@{ Path = $PrivateParent; Label = "private acceptance parent" },
  [pscustomobject]@{ Path = $ShortSourceParent; Label = "short source parent" }
)) {
  Assert-NoReparseAncestorChain ([string]$CheckedPath.Path) ([string]$CheckedPath.Label)
}
Assert-OwnerPrivateDirectoryAcl $Candidate "Candidate directory"
Assert-OwnerPrivateDirectoryAcl $PrivateParent "PrivateParent"
$CandidateBindingParent = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($CandidateBinding))
if (-not [String]::Equals(
    $CandidateBindingParent, [IO.Path]::GetFullPath($Candidate),
    [StringComparison]::OrdinalIgnoreCase
  )) {
  Assert-OwnerPrivateDirectoryAcl $CandidateBindingParent "CandidateBinding parent"
}

$EnvironmentNames = @(
  @([Environment]::GetEnvironmentVariables("Process").Keys | Where-Object {
    [string]$_ -match '^(?:GIT_|GH_|GITHUB_)' -or [string]$_ -eq "SSH_ASKPASS"
  }) + @(
    "GIT_CONFIG_NOSYSTEM","GIT_CONFIG_GLOBAL","GIT_CONFIG_COUNT","GIT_ATTR_NOSYSTEM",
    "GIT_ALLOW_PROTOCOL","GIT_TERMINAL_PROMPT","SSH_ASKPASS",
    "GH_CONFIG_DIR","GH_PROMPT_DISABLED","GH_TOKEN","GH_HOST","GH_REPO",
    "GITHUB_TOKEN","GITHUB_ENTERPRISE_TOKEN"
  ) | Sort-Object -Unique
)
foreach ($Name in $EnvironmentNames) {
  [Environment]::SetEnvironmentVariable($Name, $null, "Process")
}
function Restore-CoordinatorEnvironment {
  foreach ($Name in @([Environment]::GetEnvironmentVariables("Process").Keys | Where-Object {
    [string]$_ -match '^(?:GIT_|GH_|GITHUB_)' -or [string]$_ -eq "SSH_ASKPASS"
  })) { [Environment]::SetEnvironmentVariable([string]$Name, $null, "Process") }
}

$PrimaryFailure = $null
$CleanupErrors = New-Object Collections.Generic.List[string]
$EvidenceDirectory = $null
$RawScreenshotDirectory = $null
$ExtensionDirectory = $null
$AttemptDirectory = $null
$SecureGhToken = $null
$StableCandidateBinding = $null
$ExactReleaseCandidateBinding = $null
$Stage = "initialize"
try {
$Repository = New-ShortSourceDirectory
$EvidenceDirectory = New-PrivateEmptyDirectory "lbb-evidence-"
$RawScreenshotDirectory = New-PrivateEmptyDirectory "lbb-raw-"
$ExtensionDirectory = New-PrivateEmptyDirectory "lbb-extension-"
$AttemptDirectory = New-PrivateEmptyDirectory "lbb-attempt-"
$EmptyTemplates = New-PrivateEmptyDirectory "lbb-templates-"
$EmptyHooks = New-PrivateEmptyDirectory "lbb-hooks-"
$IsolatedGh = New-PrivateEmptyDirectory "lbb-gh-"
$TrustedRoot = New-PrivateEmptyDirectory "lbb-trusted-"

$ExpectedAssets = @(
  "local-browser-bridge-v$Version-windows-x86_64.exe",
  "local-computer-helper-v$Version-windows-x86_64.exe",
  "local-browser-bridge-v$Version-macos-universal.tar.gz",
  "local-browser-bridge-extension-v$Version.zip"
)
$ExpectedDownloads = @($ExpectedAssets) + "SHA256SUMS.txt"
$Downloads = @([IO.DirectoryInfo]::new($Candidate).GetFileSystemInfos())
if ($Downloads.Count -ne 5 -or @($Downloads | Where-Object {
      $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)
    }).Count -ne 0 -or
    @(Compare-Object ($ExpectedDownloads | Sort-Object) ($Downloads.Name | Sort-Object)).Count -ne 0) {
  throw "Candidate download inventory is not the exact five ordinary files."
}

$Manifest = Join-Path $Candidate "SHA256SUMS.txt"
$ObservedManifestSha = Get-TrustedSha256 $Manifest
if ($ObservedManifestSha -cne $ManifestSha) { throw "Independent manifest digest mismatch." }
$Lines = @([IO.File]::ReadAllLines($Manifest, [Text.Encoding]::ASCII))
if ($Lines.Count -ne 4) { throw "SHA256SUMS.txt is not the canonical four-line manifest." }
$ManifestAssetHashes = [ordered]@{}
for ($Index = 0; $Index -lt 4; $Index += 1) {
  if ($Lines[$Index] -cnotmatch '^([0-9a-f]{64})  (.+)$' -or $Matches[2] -cne $ExpectedAssets[$Index]) {
    throw "SHA256SUMS.txt entry order or spelling is not canonical."
  }
  $ManifestAssetName = [string]$Matches[2]
  $ManifestAssetSha = [string]$Matches[1]
  $ManifestAssetHashes.Add($ManifestAssetName, $ManifestAssetSha)
  $ObservedAssetSha = Get-TrustedSha256 (Join-Path $Candidate $ManifestAssetName)
  if ($ObservedAssetSha -cne $ManifestAssetSha) { throw "Candidate asset digest mismatch." }
}
$CandidateBindingInfo = [IO.FileInfo]::new([IO.Path]::GetFullPath($CandidateBinding))
if ($CandidateBindingInfo.Length -le 0 -or $CandidateBindingInfo.Length -gt 2MB) {
  throw "CandidateBinding has an invalid size."
}
$BindingBytes = [IO.File]::ReadAllBytes($CandidateBindingInfo.FullName)
try {
  $Binding = [Text.UTF8Encoding]::new($false, $true).GetString($BindingBytes) | ConvertFrom-Json
}
finally { [Array]::Clear($BindingBytes, 0, $BindingBytes.Length) }
$BindingProperties = @($Binding.PSObject.Properties.Name)
$ExpectedBindingProperties = @(
  "schemaVersion", "productVersion", "repository", "tag", "sourceSha",
  "tagObjectSha", "workflowRunId", "workflowRunAttempt", "artifactId",
  "artifactName", "artifactZipBytes", "artifactZipSha256",
  "checksumManifestSha256", "attestationInvocationUri", "attestedAssetCount",
  "githubHostedRunner", "assets", "passed"
)
if (($BindingProperties -join "`n") -cne ($ExpectedBindingProperties -join "`n") -or
    $Binding.schemaVersion -ne 1 -or $Binding.productVersion -cne $Version -or
    $Binding.repository -cne "flrngel/local-browser-bridge" -or
    $Binding.tag -cne "v$Version" -or $Binding.sourceSha -cne $FinalSha -or
    $Binding.tagObjectSha -cne $TagObjectSha -or
    [string]$Binding.workflowRunId -cne $WorkflowRunId -or
    [string]$Binding.workflowRunAttempt -cne $WorkflowRunAttempt -or
    [string]$Binding.artifactId -cne $ReleaseCandidateArtifactId -or
    $Binding.artifactName -cne "release-candidate" -or
    $Binding.artifactZipBytes -isnot [ValueType] -or [int64]$Binding.artifactZipBytes -le 0 -or
    $Binding.artifactZipSha256 -cne $ReleaseCandidateArtifactZipSha256 -or
    $Binding.checksumManifestSha256 -cne $ManifestSha -or
    $Binding.attestationInvocationUri -cne $ExpectedInvocationUri -or
    $Binding.attestedAssetCount -ne 5 -or $Binding.githubHostedRunner -ne $true -or
    $Binding.passed -ne $true -or @($Binding.assets).Count -ne 5) {
  throw "Candidate binding does not match the exact workflow artifact and attempt."
}
$BoundAssetNames = @($Binding.assets | ForEach-Object { [string]$_.file })
if (($BoundAssetNames -join "`n") -cne ($ExpectedDownloads -join "`n") -or
    @($BoundAssetNames | Sort-Object -Unique).Count -ne 5) {
  throw "Candidate binding does not contain the exact five-file inventory."
}
$StableCandidateBinding = Join-Path $TrustedRoot "candidate-binding.json"
$StableBindingOutput = [IO.File]::Open(
  $StableCandidateBinding, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
)
$StableBindingInput = [IO.File]::Open(
  $CandidateBindingInfo.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read
)
try { $StableBindingInput.CopyTo($StableBindingOutput) }
finally { $StableBindingInput.Dispose(); $StableBindingOutput.Dispose() }
if ((Get-TrustedSha256 $StableCandidateBinding) -cne (Get-TrustedSha256 $CandidateBindingInfo.FullName)) {
  throw "The private candidate-binding copy differs from its validated input."
}
$StableBindingBytes = [IO.File]::ReadAllBytes($StableCandidateBinding)
try {
  $StableBinding = [Text.UTF8Encoding]::new($false, $true).GetString($StableBindingBytes) | ConvertFrom-Json
}
finally { [Array]::Clear($StableBindingBytes, 0, $StableBindingBytes.Length) }
if (($StableBinding | ConvertTo-Json -Depth 20 -Compress) -cne
    ($Binding | ConvertTo-Json -Depth 20 -Compress)) {
  throw "The private candidate-binding copy is not the exact validated object."
}
$ExactReleaseAssets = @($StableBinding.assets | ForEach-Object {
  [ordered]@{ file = [string]$_.file; bytes = [int64]$_.bytes; sha256 = [string]$_.sha256 }
})
$ExactReleaseCandidateBinding = [ordered]@{
  productVersion = [string]$StableBinding.productVersion
  repository = [string]$StableBinding.repository
  tag = [string]$StableBinding.tag
  sourceSha = [string]$StableBinding.sourceSha
  tagObjectSha = [string]$StableBinding.tagObjectSha
  workflowRunId = [string]$StableBinding.workflowRunId
  workflowRunAttempt = [string]$StableBinding.workflowRunAttempt
  artifactId = [string]$StableBinding.artifactId
  artifactName = [string]$StableBinding.artifactName
  artifactZipBytes = [int64]$StableBinding.artifactZipBytes
  artifactZipSha256 = [string]$StableBinding.artifactZipSha256
  checksumManifestSha256 = [string]$StableBinding.checksumManifestSha256
  attestationInvocationUri = [string]$StableBinding.attestationInvocationUri
  attestedAssetCount = [int]$StableBinding.attestedAssetCount
  githubHostedRunner = [bool]$StableBinding.githubHostedRunner
  assets = $ExactReleaseAssets
}
$Stage = "candidate-bound"
Write-CreateOnceAttemptRecord $AttemptDirectory "attempt-start.json" ([ordered]@{
  schemaVersion = 1
  evidenceType = "stock-user-chrome-attempt-start"
  version = $Version
  stage = $Stage
  passed = $false
  releaseCandidateBinding = $ExactReleaseCandidateBinding
})
foreach ($BoundAsset in @($Binding.assets)) {
  if ((@($BoundAsset.PSObject.Properties.Name) -join "`n") -cne "file`nbytes`nsha256" -or
      $ExpectedDownloads -cnotcontains [string]$BoundAsset.file -or
      $BoundAsset.bytes -isnot [ValueType] -or [int64]$BoundAsset.bytes -le 0 -or
      [string]$BoundAsset.sha256 -cnotmatch '^[0-9a-f]{64}$' -or
      [string]$BoundAsset.sha256 -cne (Get-TrustedSha256 (Join-Path $Candidate ([string]$BoundAsset.file))) -or
      [int64]$BoundAsset.bytes -ne [IO.FileInfo]::new((Join-Path $Candidate ([string]$BoundAsset.file))).Length) {
    throw "Candidate binding asset inventory or digest mismatch."
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
$SecureGhToken = Read-Host "Independent least-privilege GitHub acceptance token" -AsSecureString

function Invoke-TrustedGhAttestation([string]$Name) {
  if ($ExpectedDownloads -cnotcontains $Name) { throw "Attestation refused an unexpected asset name." }
  $Bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($SecureGhToken)
  $ChildToken = $null
  $Process = $null
  $Info = $null
  try {
    $ChildToken = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($Bstr)
    $Info = [Diagnostics.ProcessStartInfo]::new()
    $Info.FileName = $TrustedGh
    $Info.WorkingDirectory = $Candidate
    $Info.Arguments = "attestation verify $Name --hostname github.com --repo flrngel/local-browser-bridge --signer-workflow flrngel/local-browser-bridge/.github/workflows/deploy.yml --source-ref refs/tags/v$Version --source-digest $FinalSha --deny-self-hosted-runners --format json"
    $Info.UseShellExecute = $false
    $Info.CreateNoWindow = $true
    $Info.RedirectStandardOutput = $true
    $Info.RedirectStandardError = $true
    $Info.EnvironmentVariables["GH_TOKEN"] = $ChildToken
    $Info.EnvironmentVariables["GH_PROMPT_DISABLED"] = "1"
    $Process = [Diagnostics.Process]::new()
    $Process.StartInfo = $Info
    if (-not $Process.Start()) { throw "Trusted gh attestation child did not start." }
    $OutputTask = $Process.StandardOutput.ReadToEndAsync()
    $ErrorTask = $Process.StandardError.ReadToEndAsync()
    if (-not $Process.WaitForExit(60000)) {
      $Process.Kill()
      throw "Trusted gh attestation child exceeded its sixty-second bound."
    }
    $Output = $OutputTask.GetAwaiter().GetResult()
    [void]$ErrorTask.GetAwaiter().GetResult()
    if ($Process.ExitCode -ne 0) { throw "GitHub attestation verification failed." }
    $Attestations = @($Output | ConvertFrom-Json)
    if ($Attestations.Count -lt 1) { throw "GitHub attestation verification returned no statement." }
    $ExpectedAssetSha = Get-TrustedSha256 (Join-Path $Candidate $Name)
    foreach ($Attestation in $Attestations) {
      $Verification = $Attestation.verificationResult
      $Certificate = $Verification.signature.certificate
      $Statement = $Verification.statement
      $Workflow = $Statement.predicate.buildDefinition.externalParameters.workflow
      $Subjects = @($Statement.subject | Where-Object {
        $_.name -ceq $Name -and $_.digest.sha256 -ceq $ExpectedAssetSha
      })
      if ($Statement.predicateType -cne "https://slsa.dev/provenance/v1" -or
          $Statement.predicate.buildDefinition.buildType -cne "https://actions.github.io/buildtypes/workflow/v1" -or
          $Workflow.path -cne ".github/workflows/deploy.yml" -or
          $Workflow.ref -cne "refs/tags/v$Version" -or
          $Workflow.repository -cne "https://github.com/flrngel/local-browser-bridge" -or
          $Statement.predicate.runDetails.metadata.invocationId -cne $ExpectedInvocationUri -or
          $Certificate.runInvocationURI -cne $ExpectedInvocationUri -or
          $Certificate.githubWorkflowSHA -cne $FinalSha -or
          $Certificate.githubWorkflowRepository -cne "flrngel/local-browser-bridge" -or
          $Certificate.githubWorkflowRef -cne "refs/tags/v$Version" -or
          $Certificate.runnerEnvironment -cne "github-hosted" -or
          $Certificate.sourceRepositoryDigest -cne $FinalSha -or
          $Certificate.sourceRepositoryRef -cne "refs/tags/v$Version" -or
          $Subjects.Count -lt 1) {
        throw "GitHub attestation did not bind the exact workflow attempt and subject."
      }
    }
  }
  finally {
    if ($null -ne $Info) { $Info.EnvironmentVariables.Remove("GH_TOKEN") }
    if ($null -ne $Process) { $Process.Dispose() }
    $ChildToken = $null
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($Bstr)
  }
}

foreach ($Name in $ExpectedDownloads) {
  Invoke-TrustedGhAttestation $Name
}
$SecureGhToken.Dispose()
$SecureGhToken = $null
$Stage = "attestations-verified"

$GitCommon = @(
  "--no-replace-objects", "--no-lazy-fetch", "-c", "core.fsmonitor=false",
  "-c", "core.hooksPath=$EmptyHooks", "-c", "core.autocrlf=false",
  "-c", "core.longpaths=true"
)
& $TrustedGit @GitCommon clone --no-checkout --no-local --origin origin `
  "--template=$EmptyTemplates" $Origin $Repository
if ($LASTEXITCODE -ne 0) { throw "Fixed-origin fresh clone failed." }
& $TrustedGit @GitCommon -C $Repository config --local core.longpaths true
if ($LASTEXITCODE -ne 0) { throw "Fresh clone could not persist long-path support." }
& $TrustedGit @GitCommon -C $Repository fetch --force --tags origin
if ($LASTEXITCODE -ne 0) { throw "Fresh clone could not fetch the exact annotated tag." }
& $TrustedGit @GitCommon -C $Repository checkout --detach --force $FinalSha
if ($LASTEXITCODE -ne 0) { throw "Exact detached checkout failed." }

$ObservedHead = (& $TrustedGit @GitCommon -C $Repository rev-parse --verify HEAD).Trim()
$ObservedTagObject = (& $TrustedGit @GitCommon -C $Repository rev-parse --verify "refs/tags/v$Version").Trim()
$ObservedTagType = (& $TrustedGit @GitCommon -C $Repository cat-file -t $ObservedTagObject).Trim()
$ObservedTagPeel = (& $TrustedGit @GitCommon -C $Repository rev-parse --verify "refs/tags/v$Version^{}").Trim()
$SymbolicHead = @(& $TrustedGit @GitCommon -C $Repository symbolic-ref -q HEAD 2>$null)
$SymbolicHeadExit = $LASTEXITCODE
$Dirty = @(& $TrustedGit @GitCommon -C $Repository status --porcelain=v2 --untracked-files=all)
$Deleted = @(& $TrustedGit @GitCommon -C $Repository ls-files --deleted)
$Others = @(& $TrustedGit @GitCommon -C $Repository ls-files --others --exclude-standard)
$Ignored = @(& $TrustedGit @GitCommon -C $Repository ls-files --others --ignored --exclude-standard)
if ($ObservedHead -cne $FinalSha) { throw "Repository HEAD does not equal FINAL_SHA." }
if ($ObservedTagType -cne "tag" -or $ObservedTagObject -cne $TagObjectSha -or
    $ObservedTagPeel -cne $FinalSha) {
  throw "Repository annotated tag object or peel does not match the coordinator binding."
}
if ($SymbolicHeadExit -ne 1 -or $SymbolicHead.Count -ne 0) { throw "Repository HEAD must be detached." }
if ($Dirty.Count -ne 0) { throw "Repository checkout must be clean, including untracked files." }
if ($Deleted.Count -ne 0 -or $Others.Count -ne 0) {
  throw "Repository checkout has missing or untracked files."
}
if ($Ignored.Count -ne 0) { throw "Repository checkout must not contain ignored files." }
& $TrustedGit @GitCommon -C $Repository diff --quiet HEAD --
if ($LASTEXITCODE -ne 0) { throw "Repository worktree diff must be empty." }
& $TrustedGit @GitCommon -C $Repository diff --cached --quiet
if ($LASTEXITCODE -ne 0) { throw "Repository index diff must be empty." }
& $TrustedGit @GitCommon -C $Repository fsck --full
if ($LASTEXITCODE -ne 0) { throw "Repository object database failed git fsck --full." }

$TrustedRelativeFiles = @(
  "scripts/test-windows-stock-chrome.ps1",
  "scripts/browser-evidence-candidate.ps1",
  "scripts/write-browser-evidence-record.ps1",
  "scripts/test-windows-browser-api.ps1",
  "scripts/record-computer-helper-chain.ps1",
  "scripts/sanitize-browser-evidence-screenshot.ps1",
  "evidence/v0.12.12/browser/operator-results.template.json"
)
function Export-ExactTrustedBlob([string]$ObjectId, [string]$Relative) {
  if ($ObjectId -cnotmatch '^[0-9a-f]{40}$' -or $TrustedRelativeFiles -cnotcontains $Relative) {
    throw "Trusted blob export refused an unbound object or path."
  }
  $Output = [IO.Path]::GetFullPath((Join-Path $TrustedRoot $Relative))
  $TrustedPrefix = [IO.Path]::GetFullPath($TrustedRoot).TrimEnd('\') + '\'
  if (-not $Output.StartsWith($TrustedPrefix, [StringComparison]::OrdinalIgnoreCase) -or
      [IO.File]::Exists($Output) -or [IO.Directory]::Exists($Output)) {
    throw "Trusted blob export path is not new and owned."
  }
  $OutputParent = [IO.Path]::GetDirectoryName($Output)
  [IO.Directory]::CreateDirectory($OutputParent) | Out-Null
  if ([IO.DirectoryInfo]::new($OutputParent).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw "Trusted blob export parent is a reparse point."
  }
  $Info = [Diagnostics.ProcessStartInfo]::new()
  $Info.FileName = $TrustedGit
  $Info.WorkingDirectory = $Repository
  $Info.Arguments = "--no-replace-objects --no-lazy-fetch -c core.fsmonitor=false cat-file blob $ObjectId"
  $Info.UseShellExecute = $false
  $Info.CreateNoWindow = $true
  $Info.RedirectStandardOutput = $true
  $Info.RedirectStandardError = $true
  $Process = [Diagnostics.Process]::new()
  $Process.StartInfo = $Info
  $Stream = [IO.File]::Open($Output, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try {
    if (-not $Process.Start()) { throw "Trusted git cat-file did not start." }
    $CopyTask = $Process.StandardOutput.BaseStream.CopyToAsync($Stream)
    $ErrorTask = $Process.StandardError.ReadToEndAsync()
    if (-not $Process.WaitForExit(20000)) {
      $Process.Kill()
      throw "Trusted git cat-file exceeded its twenty-second bound."
    }
    $CopyTask.GetAwaiter().GetResult()
    $ErrorText = $ErrorTask.GetAwaiter().GetResult()
    if ($Process.ExitCode -ne 0 -or -not [String]::IsNullOrWhiteSpace($ErrorText)) {
      throw "Trusted git cat-file failed."
    }
  }
  finally {
    $Stream.Dispose()
    $Process.Dispose()
  }
  $ObservedObject = (& $TrustedGit @GitCommon -C $Repository hash-object --no-filters -- $Output).Trim()
  if ($LASTEXITCODE -ne 0 -or $ObservedObject -cne $ObjectId -or
      ([IO.FileInfo]::new($Output).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "Materialized trusted blob does not byte-match FINAL_SHA."
  }
}
foreach ($Relative in $TrustedRelativeFiles) {
  $ExpectedBlob = (& $TrustedGit @GitCommon -C $Repository rev-parse "${FinalSha}:$Relative").Trim()
  $ObservedBlob = (& $TrustedGit @GitCommon -C $Repository hash-object --no-filters -- (Join-Path $Repository $Relative)).Trim()
  if ($LASTEXITCODE -ne 0 -or $ExpectedBlob -cnotmatch '^[0-9a-f]{40}$' -or
      $ObservedBlob -cne $ExpectedBlob) {
    throw "A repository evidence script does not byte-match FINAL_SHA."
  }
  Export-ExactTrustedBlob $ExpectedBlob $Relative
}

function Invoke-ExactPs51SelfTest([string]$ScriptPath, [string[]]$Arguments) {
  $Full = [IO.Path]::GetFullPath($ScriptPath)
  $TrustedPrefix = [IO.Path]::GetFullPath($TrustedRoot).TrimEnd('\') + '\'
  if (-not $Full.StartsWith($TrustedPrefix, [StringComparison]::OrdinalIgnoreCase) -or
      -not [IO.File]::Exists($Full) -or
      ([IO.FileInfo]::new($Full).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "A Windows PowerShell 5.1 producer self-test path is not an exact trusted blob."
  }
  Assert-NoReparseAncestorChain $Full "Windows PowerShell 5.1 producer self-test"
  & $CleanPowerShell -NoLogo -NoProfile -File $Full @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "A required Windows PowerShell 5.1 producer self-test failed."
  }
}

foreach ($SelfTestSpec in @(
  [pscustomobject]@{ Relative = "scripts/browser-evidence-candidate.ps1"; Arguments = @("-Mode", "SelfTest") },
  [pscustomobject]@{ Relative = "scripts/write-browser-evidence-record.ps1"; Arguments = @("-Mode", "SelfTest") },
  [pscustomobject]@{ Relative = "scripts/test-windows-browser-api.ps1"; Arguments = @("-SelfTest") },
  [pscustomobject]@{ Relative = "scripts/record-computer-helper-chain.ps1"; Arguments = @("-Mode", "SelfTest") },
  [pscustomobject]@{ Relative = "scripts/sanitize-browser-evidence-screenshot.ps1"; Arguments = @("-Mode", "SelfTest") }
)) {
  Invoke-ExactPs51SelfTest (Join-Path $TrustedRoot $SelfTestSpec.Relative) @($SelfTestSpec.Arguments)
}
$Stage = "producer-self-tests-passed"
$CoordinatorRelative = "scripts/test-windows-stock-chrome.ps1"
$ExpectedCoordinatorBlob = (& $TrustedGit @GitCommon -C $Repository rev-parse "${FinalSha}:$CoordinatorRelative").Trim()
$ObservedCoordinatorBlob = (& $TrustedGit @GitCommon -C $Repository hash-object --no-filters -- $PSCommandPath).Trim()
if ($LASTEXITCODE -ne 0 -or $ObservedCoordinatorBlob -cne $ExpectedCoordinatorBlob) {
  throw "The running stock-Chrome coordinator does not match the tagged source blob."
}

$ExtensionArchiveName = "local-browser-bridge-extension-v$Version.zip"
Invoke-BoundedExactExtensionZipExtraction `
  (Join-Path $Candidate $ExtensionArchiveName) `
  $ExtensionDirectory `
  $CanonicalExtensionEntries `
  ([string]$ManifestAssetHashes[$ExtensionArchiveName])

$Scripts = Join-Path $TrustedRoot "scripts"
$Stage = "candidate-preflight"
& "$Scripts\browser-evidence-candidate.ps1" -Mode Preflight -Version $Version `
  -FinalSha $FinalSha -Repository $Repository -TrustedGitExecutable $TrustedGit `
  -TrustedEmptyHooksDirectory $EmptyHooks -ChecksumManifest $Manifest `
  -ChecksumManifestSha256 $ManifestSha -ReleaseCandidateBinding $StableCandidateBinding `
  -ServerExecutable (Join-Path $Candidate "local-browser-bridge-v$Version-windows-x86_64.exe") `
  -ComputerHelperExecutable (Join-Path $Candidate "local-computer-helper-v$Version-windows-x86_64.exe") `
  -ExtensionZip (Join-Path $Candidate "local-browser-bridge-extension-v$Version.zip") `
  -ExtractedExtension $ExtensionDirectory `
  -OutputRecord (Join-Path $EvidenceDirectory "candidate-preflight.json")

& "$Scripts\write-browser-evidence-record.ps1" -Mode InitializeOperator `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -OutputRecord (Join-Path $EvidenceDirectory "operator-results.json")

$Stage = "computer-helper-chain"
& "$Scripts\record-computer-helper-chain.ps1" -Mode Run `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -ApiMatrixRunner "$Scripts\test-windows-browser-api.ps1" `
  -ApiMatrixRecord (Join-Path $EvidenceDirectory "browser-api-matrix.json") `
  -ServerExecutable (Join-Path $Candidate "local-browser-bridge-v$Version-windows-x86_64.exe") `
  -HelperExecutable (Join-Path $Candidate "local-computer-helper-v$Version-windows-x86_64.exe") `
  -ExtensionDirectory $ExtensionDirectory `
  -RawScreenshotDirectory $RawScreenshotDirectory `
  -OutputRecord (Join-Path $EvidenceDirectory "browser-computer-helper-chain.json")

$Captures = [ordered]@{
  "extension-loaded" = "browser-01-extension-loaded"
  "api-action-result" = "browser-02-api-action-result"
  "computer-share-action" = "browser-03-computer-share-action"
  "stop-paused" = "browser-04-stop-paused"
  "cancel-paused" = "browser-05-cancel-paused"
  "post-handback-resume" = "browser-06-post-handback-resume"
}
function Read-CropInteger([string]$Label, [int]$Minimum) {
  $Value = 0
  if (-not [int]::TryParse((Read-Host $Label), [ref]$Value) -or $Value -lt $Minimum) {
    throw "$Label must be an integer greater than or equal to $Minimum."
  }
  return $Value
}
$Stage = "screenshot-sanitization"
foreach ($Entry in $Captures.GetEnumerator()) {
  $RawPath = Join-Path $RawScreenshotDirectory ($Entry.Value + ".raw.png")
  Write-Host "Privately inspect $RawPath and enter a tight crop containing only the required visible proof."
  $CropX = Read-CropInteger "$($Entry.Key) CropX" 0
  $CropY = Read-CropInteger "$($Entry.Key) CropY" 0
  $CropWidth = Read-CropInteger "$($Entry.Key) CropWidth" 120
  $CropHeight = Read-CropInteger "$($Entry.Key) CropHeight" 32
  & "$Scripts\sanitize-browser-evidence-screenshot.ps1" -Mode Sanitize `
    -InputImage $RawPath `
    -OutputImage (Join-Path $EvidenceDirectory ($Entry.Value + ".png")) `
    -OutputRecord (Join-Path $RawScreenshotDirectory ($Entry.Value + ".pending.json")) `
    -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
    -Purpose $Entry.Key -CropX $CropX -CropY $CropY -CropWidth $CropWidth -CropHeight $CropHeight
}

$Stage = "screenshot-human-review"
foreach ($Entry in $Captures.GetEnumerator()) {
  $ReviewedImage = Join-Path $EvidenceDirectory ($Entry.Value + ".png")
  $ReviewSha = Get-TrustedSha256 $ReviewedImage
  $ExpectedReviewReceipt = "REVIEWED:$($Entry.Key):$ReviewSha"
  Write-Host "Open $ReviewedImage, verify the required visible state and absence of sensitive pixels, then type $ExpectedReviewReceipt"
  if ((Read-Host "Exact purpose-and-image-digest-bound human review receipt") -cne $ExpectedReviewReceipt) {
    throw "AttestReview refused a missing or mismatched per-image human receipt."
  }
  & "$Scripts\sanitize-browser-evidence-screenshot.ps1" -Mode AttestReview `
    -PendingRecord (Join-Path $RawScreenshotDirectory ($Entry.Value + ".pending.json")) `
    -ReviewedImage $ReviewedImage `
    -OutputRecord (Join-Path $EvidenceDirectory ($Entry.Value + ".json")) `
    -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
    -ManualVisualReviewConfirmed
}

$Stage = "candidate-postflight"
& "$Scripts\browser-evidence-candidate.ps1" -Mode Postflight -Version $Version `
  -FinalSha $FinalSha -Repository $Repository -TrustedGitExecutable $TrustedGit `
  -TrustedEmptyHooksDirectory $EmptyHooks -ChecksumManifest $Manifest `
  -ChecksumManifestSha256 $ManifestSha -ReleaseCandidateBinding $StableCandidateBinding `
  -ServerExecutable (Join-Path $Candidate "local-browser-bridge-v$Version-windows-x86_64.exe") `
  -ComputerHelperExecutable (Join-Path $Candidate "local-computer-helper-v$Version-windows-x86_64.exe") `
  -ExtensionZip (Join-Path $Candidate "local-browser-bridge-extension-v$Version.zip") `
  -ExtractedExtension $ExtensionDirectory `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -OutputRecord (Join-Path $EvidenceDirectory "candidate-postflight.json")

$ExpectedRawNames = @($Captures.Values | ForEach-Object { $_ + ".raw.png" }) +
  @($Captures.Values | ForEach-Object { $_ + ".pending.json" })
$ExpectedExtensionNames = @(
  "background.js","content.js","dom-core.js","frame-agent.js","lib.js",
  "manifest.json","popup.css","popup.html","popup.js","stop-guard.js","LICENSE"
)
Remove-ExactFlatOwnedDirectory $RawScreenshotDirectory $ExpectedRawNames
Remove-ExactFlatOwnedDirectory $ExtensionDirectory $ExpectedExtensionNames

$OperatorPath = Join-Path $EvidenceDirectory "operator-results.json"
$Stage = "operator-record"
Write-Host "Edit and save only $OperatorPath, using the helper record and human receipts."
if ((Read-Host "Type OPERATOR-RECORD-SAVED after the exact record is saved") -cne "OPERATOR-RECORD-SAVED") {
  throw "The operator record was not explicitly saved."
}

$Sidecars = @($Captures.Values | ForEach-Object {
  Join-Path $EvidenceDirectory ($_ + ".json")
})
$Stage = "finalize"
& "$Scripts\write-browser-evidence-record.ps1" -Mode Finalize `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -PostflightRecord (Join-Path $EvidenceDirectory "candidate-postflight.json") `
  -ApiMatrixRecord (Join-Path $EvidenceDirectory "browser-api-matrix.json") `
  -ComputerHelperRecord (Join-Path $EvidenceDirectory "browser-computer-helper-chain.json") `
  -OperatorResults (Join-Path $EvidenceDirectory "operator-results.json") `
  -ScreenshotRecords $Sidecars `
  -OutputRecord (Join-Path $EvidenceDirectory "browser-acceptance.json")

$FinalEntries = @([IO.DirectoryInfo]::new($EvidenceDirectory).GetFileSystemInfos())
if ($FinalEntries.Count -ne 18 -or
    @($FinalEntries | Where-Object {
      $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)
    }).Count -ne 0) {
  throw "The finalized evidence directory is not the exact eighteen-file ordinary inventory."
}
$Stage = "completed"
}
catch {
  $PrimaryFailure = $_
}
finally {
  try { Restore-CoordinatorEnvironment }
  catch { $CleanupErrors.Add("environment: $($_.Exception.Message)") }
  if ($null -ne $SecureGhToken) {
    try { $SecureGhToken.Dispose() }
    catch { $CleanupErrors.Add("GitHub token disposal: $($_.Exception.Message)") }
  }
  $SecureGhToken = $null
  foreach ($Owned in @($OwnedDirectories | Sort-Object { $_.Length } -Descending)) {
    if ($Owned -ceq $EvidenceDirectory -or $Owned -ceq $AttemptDirectory) { continue }
    try { Remove-TestOwnedTree $Owned }
    catch { $CleanupErrors.Add("owned-directory ${Owned}: $($_.Exception.Message)") }
  }
  if (($null -ne $PrimaryFailure -or $CleanupErrors.Count -ne 0) -and
      -not [String]::IsNullOrWhiteSpace($EvidenceDirectory)) {
    $PassingOutput = Join-Path $EvidenceDirectory "browser-acceptance.json"
    try {
      if ([IO.File]::Exists($PassingOutput)) {
        $PassingItem = [IO.FileInfo]::new($PassingOutput)
        if ($PassingItem.Attributes -band [IO.FileAttributes]::ReparsePoint) {
          throw "passing output is a reparse point"
        }
        [IO.File]::Delete($PassingOutput)
      }
    }
    catch { $CleanupErrors.Add("passing-output invalidation: $($_.Exception.Message)") }
  }
  $FailedAttempt = $null -ne $PrimaryFailure -or $CleanupErrors.Count -ne 0
  if ($FailedAttempt -and -not [String]::IsNullOrWhiteSpace($AttemptDirectory) -and
      [IO.Directory]::Exists($AttemptDirectory)) {
    try {
      Write-CreateOnceAttemptRecord $AttemptDirectory "failure.json" ([ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-failure"
        version = $Version
        stage = $Stage
        passed = $false
        releaseCandidateBinding = $ExactReleaseCandidateBinding
      })
      Write-CreateOnceAttemptRecord $AttemptDirectory "cleanup.json" ([ordered]@{
        schemaVersion = 1
        evidenceType = "stock-user-chrome-failure-cleanup"
        version = $Version
        passingOutputPresent = $(if ([String]::IsNullOrWhiteSpace($EvidenceDirectory)) {
          $false
        } else { [IO.File]::Exists((Join-Path $EvidenceDirectory "browser-acceptance.json")) })
        secretValuesRetained = $false
        rawFailureMessageRetained = $false
        cleanupIssuesPresent = $CleanupErrors.Count -ne 0
      })
    }
    catch { $CleanupErrors.Add("attempt-evidence: $($_.Exception.Message)") }
  }
  elseif (-not [String]::IsNullOrWhiteSpace($AttemptDirectory) -and
      [IO.Directory]::Exists($AttemptDirectory)) {
    try { Remove-TestOwnedTree $AttemptDirectory }
    catch { $CleanupErrors.Add("attempt-directory: $($_.Exception.Message)") }
  }
}
if ($null -ne $PrimaryFailure) {
  $Suffix = if ($CleanupErrors.Count -eq 0) { "Outer rollback completed." }
    else { "Outer rollback incomplete: " + ($CleanupErrors -join "; ") }
  if (-not [String]::IsNullOrWhiteSpace($EvidenceDirectory)) {
    Write-Output "Failed evidence directory: $EvidenceDirectory"
  }
  if (-not [String]::IsNullOrWhiteSpace($AttemptDirectory) -and [IO.Directory]::Exists($AttemptDirectory)) {
    Write-Output "Sanitized attempt evidence directory: $AttemptDirectory"
  }
  throw "$($PrimaryFailure.Exception.Message) $Suffix"
}
if ($CleanupErrors.Count -ne 0) {
  throw "Acceptance cleanup failed and passing output was invalidated: $($CleanupErrors -join '; ')"
}
Write-Output "Windows stock-Chrome acceptance passed."
Write-Output "Evidence directory: $EvidenceDirectory"
