# Windows v0.12.11 stock-Chrome acceptance protocol

This directory defines the release gate for the v0.12.11 browser extension and
packaged Windows computer helper. It is protocol infrastructure, not passing
evidence. A release passes only when one frozen candidate run produces the
finalizer-owned `browser-acceptance.json`.

No v0.12.11 stock-Chrome acceptance result exists yet. This protocol requires a
fresh v0.12.11 candidate and output directory; it cannot reuse any prior
operator record, notification, image, or generated evidence byte. The exact
v0.12.10 macOS candidate completed 69 assertions and six fixture-only
screenshots, then timed out before its final action because no separately
authorized pointer movement arrived. Windows and stock-Chrome acceptance never
started, publication was canceled, and no v0.12.10 Release exists. The retained
negative record is
[`withdrawn-de59840-macos-deliberate-pointer-timeout`](../../v0.12.10/computer/attempts/withdrawn-de59840-macos-deliberate-pointer-timeout/README.md).
The exact v0.12.8 macOS package passed 187/187 assertions with six reviewed
screenshots, but its Windows run received no click or received marker after
publishing the foreground-arm request and timed out at `wait-foreground-arm`
before any product action. Chrome therefore never started, publication was
canceled, and no v0.12.8 Release exists.

The run is new-install-only. Use the user's installed, ordinary Google Chrome,
an existing user session, one new dedicated Chrome window, and a brand-new
empty test-owned extension directory. Do not relaunch Chrome with flags, use a
test profile, enable remote debugging, call CDP directly, or touch an existing
Local Browser Bridge identity.

## Product-control boundary

Local Browser Bridge attaches `chrome.debugger` during an active browser lease.
Chrome permits only one debugger client per tab, so Chrome MCP cannot coexist
with an active Local Browser Bridge lease. The passing record therefore fixes:

- `debuggerOwnerDuringBridgeLease:local-browser-bridge-extension`;
- `competingDebuggerAttachmentAllowed:false`;
- `chromeMcpUsedDuringBridgeLease:false`; and
- `chromeMcpReleaseEvidenceClaimed:false`.

The Local Browser Bridge API executes the browser method matrix. The exact
candidate-bound `local-computer-helper` uses the same authenticated loopback
server API for stock-Chrome UI, the native **Load unpacked** picker, the Local
Browser Bridge popup, exact-window sharing, native input, and all retained
screenshots. A human or Windows Computer Use application sharing may interpret
a helper frame, choose coordinates, and provide action-time consent, but it is
not credited as product control or screenshot capture. The simplest run never
invokes Chrome MCP and records `chromeMcpReleasedOrNotUsed:true`.

## Consent and live initial state

Consent cannot be collected as one blanket approval. Pause immediately before
installing the new identity, changing Developer Mode when required, using Full
Access, saving the ephemeral token, initiating token clearing, confirming token
clearing, and removing the exact test-owned identity.

The recorder does not accept caller-supplied Developer Mode or Full Access
values. It takes fresh helper observations and asks the human to reduce only the
visible state to `enabled` or `disabled` before any relevant mutation. It also
captures `saved token = Not configured` from an independently fresh popup
frame. Those reduced values, opaque frame references, and epochs are written to
`browser-computer-helper-chain.json`; the finalizer requires the operator record
to match them and requires restoration to the same values.

## Retained evidence

The retention claim is `acceptance-evidence-directory-only`. External tool,
Chrome, Windows, ChatGPT, or platform logs are `not-asserted`; they are not
imported or cited as release evidence.

Immediately before finalization, the private non-reparse evidence directory has
exactly eleven ordinary files:

- `candidate-preflight.json`, `candidate-postflight.json`,
  `browser-api-matrix.json`, `browser-computer-helper-chain.json`, and
  `operator-results.json`;
- three sanitized PNGs; and
- three post-review JSON sidecars.

The finalizer rejects every extra entry and creates the twelfth file,
`browser-acceptance.json`. Raw PNGs, pending-review records, credentials, API
response bodies, filesystem paths, and raw browser/helper identifiers are not
retained there.

## 1. Trust the exact candidate before extraction or execution

Save every PowerShell block below, in order, into one coordinator-owned `.ps1`
file, then run that file once in a fresh absolute 64-bit
`powershell.exe -NoProfile` process with a private working directory. The first
block opens the outer `try`; the final block closes it and performs cleanup. Do
not execute the blocks independently. The
coordinator independently supplies version `0.12.11`, lowercase `FINAL_SHA`,
the SHA-256 of `SHA256SUMS.txt`, absolute trusted `git.exe` and `gh.exe` paths,
and a least-privilege GitHub token. Never execute a script from a supplied
checkout.

The following is the canonical trust order. No release archive is extracted and
no candidate-controlled byte is executed before every asset hash, all five
attestations, the fresh fixed-origin clone, detached commit, clean checkout,
exact script-blob checks, and direct trusted-root materialization pass.

```powershell
param([string]$CleanCoordinatorNonce)

$SystemRoot = [Environment]::GetEnvironmentVariable("SystemRoot", "Machine")
$CleanPowerShell = [IO.Path]::GetFullPath([IO.Path]::Combine(
  $SystemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"
))
if ([String]::IsNullOrWhiteSpace($CleanCoordinatorNonce)) {
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
[Environment]::SetEnvironmentVariable("LBB_CLEAN_COORDINATOR_NONCE", $null, "Process")
$InheritedPSModulePath = [Environment]::GetEnvironmentVariable("PSModulePath", "Process")
$TrustedPSModulePath = [IO.Path]::GetFullPath([IO.Path]::Combine($PSHOME, "Modules"))
if (-not [IO.Directory]::Exists($TrustedPSModulePath) -or
    ([IO.DirectoryInfo]::new($TrustedPSModulePath).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
  throw "The exact Windows PowerShell system module root is unavailable or linked."
}
[Environment]::SetEnvironmentVariable("PSModulePath", $TrustedPSModulePath, "Process")
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Version = "0.12.11"
$FinalSha = "REPLACE_WITH_COORDINATOR_FINAL_SHA"
$ManifestSha = "REPLACE_WITH_COORDINATOR_MANIFEST_SHA256"
$Candidate = "REPLACE_WITH_PRIVATE_FIVE_FILE_DOWNLOAD_DIRECTORY"
$PrivateParent = "REPLACE_WITH_PRIVATE_TEST_OWNED_PARENT"
$ShortSourceParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$TrustedGit = "REPLACE_WITH_ABSOLUTE_TRUSTED_GIT_EXE"
$TrustedGh = "REPLACE_WITH_ABSOLUTE_TRUSTED_GH_EXE"
$Origin = "https://github.com/flrngel/local-browser-bridge.git"

if ($Version -cne "0.12.11" -or $FinalSha -cnotmatch '^[0-9a-f]{40}$' -or
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
if (-not [IO.Directory]::Exists($PrivateParent) -or
    ([IO.DirectoryInfo]::new($PrivateParent).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
  throw "PrivateParent must be an existing ordinary test-owned directory."
}
if (-not [IO.Directory]::Exists($ShortSourceParent) -or $ShortSourceParent.Length -gt 80 -or
    ([IO.DirectoryInfo]::new($ShortSourceParent).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
  throw "The system temporary directory must be an existing short ordinary source parent."
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
      (Compare-Object ($Entries.Name | Sort-Object) ($ExpectedNames | Sort-Object)).Count -ne 0) {
    throw "Flat cleanup refused an unexpected or linked inventory."
  }
  foreach ($Name in $ExpectedNames) { [IO.File]::Delete((Join-Path $Full $Name)) }
  [IO.Directory]::Delete($Full, $false)
}

$SavedEnvironment = @{}
$EnvironmentNames = @(
  @([Environment]::GetEnvironmentVariables("Process").Keys | Where-Object {
    [string]$_ -match '^(?:GIT_|GH_|GITHUB_)' -or [string]$_ -in @("HOME","USERPROFILE","SSH_ASKPASS")
  }) + @(
    "GIT_CONFIG_NOSYSTEM","GIT_CONFIG_GLOBAL","GIT_CONFIG_COUNT","GIT_ATTR_NOSYSTEM",
    "GIT_ALLOW_PROTOCOL","GIT_TERMINAL_PROMPT","HOME","USERPROFILE","SSH_ASKPASS",
    "GH_CONFIG_DIR","GH_PROMPT_DISABLED","GH_TOKEN","GH_HOST","GH_REPO",
    "GITHUB_TOKEN","GITHUB_ENTERPRISE_TOKEN"
  ) | Sort-Object -Unique
)
foreach ($Name in $EnvironmentNames) {
  $SavedEnvironment[$Name] = [pscustomobject]@{
    Present = Test-Path "Env:$Name"
    Value = [Environment]::GetEnvironmentVariable($Name, "Process")
  }
}
function Restore-CoordinatorEnvironment {
  foreach ($Name in @([Environment]::GetEnvironmentVariables("Process").Keys | Where-Object {
    [string]$_ -match '^(?:GIT_|GH_|GITHUB_)' -or [string]$_ -in @("HOME","USERPROFILE","SSH_ASKPASS")
  })) { [Environment]::SetEnvironmentVariable([string]$Name, $null, "Process") }
  foreach ($Name in $SavedEnvironment.Keys) {
    $Saved = $SavedEnvironment[$Name]
    [Environment]::SetEnvironmentVariable($Name, $(if ($Saved.Present) { $Saved.Value } else { $null }), "Process")
  }
}

$PrimaryFailure = $null
$CleanupErrors = New-Object Collections.Generic.List[string]
$RunSucceeded = $false
$EvidenceDirectory = $null
$RawScreenshotDirectory = $null
$ExtensionDirectory = $null
$SecureGhToken = $null
try {
$Repository = New-ShortSourceDirectory
$EvidenceDirectory = New-PrivateEmptyDirectory "lbb-evidence-"
$RawScreenshotDirectory = New-PrivateEmptyDirectory "lbb-raw-"
$ExtensionDirectory = New-PrivateEmptyDirectory "lbb-extension-"
$IsolatedHome = New-PrivateEmptyDirectory "lbb-home-"
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
    (Compare-Object ($ExpectedDownloads | Sort-Object) ($Downloads.Name | Sort-Object)).Count -ne 0) {
  throw "Candidate download inventory is not the exact five ordinary files."
}

$Manifest = Join-Path $Candidate "SHA256SUMS.txt"
$ObservedManifestSha = Get-TrustedSha256 $Manifest
if ($ObservedManifestSha -cne $ManifestSha) { throw "Independent manifest digest mismatch." }
$Lines = @([IO.File]::ReadAllLines($Manifest, [Text.Encoding]::ASCII))
if ($Lines.Count -ne 4) { throw "SHA256SUMS.txt is not the canonical four-line manifest." }
for ($Index = 0; $Index -lt 4; $Index += 1) {
  if ($Lines[$Index] -cnotmatch '^([0-9a-f]{64})  (.+)$' -or $Matches[2] -cne $ExpectedAssets[$Index]) {
    throw "SHA256SUMS.txt entry order or spelling is not canonical."
  }
  $ObservedAssetSha = Get-TrustedSha256 (Join-Path $Candidate $Matches[2])
  if ($ObservedAssetSha -cne $Matches[1]) { throw "Candidate asset digest mismatch." }
}

foreach ($Entry in [Environment]::GetEnvironmentVariables("Process").GetEnumerator()) {
  $Name = [string]$Entry.Key
  if ($Name -match '^(?:GIT_|GH_|GITHUB_)' -or $Name -in @("HOME","USERPROFILE","SSH_ASKPASS")) {
    [Environment]::SetEnvironmentVariable($Name, $null, "Process")
  }
}
$env:GIT_CONFIG_NOSYSTEM = "1"
$env:GIT_CONFIG_GLOBAL = "NUL"
$env:GIT_CONFIG_COUNT = "0"
$env:GIT_ATTR_NOSYSTEM = "1"
$env:GIT_ALLOW_PROTOCOL = "https"
$env:GIT_TERMINAL_PROMPT = "0"
$env:HOME = $IsolatedHome
$env:USERPROFILE = $IsolatedHome
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
    $Info.Arguments = "attestation verify $Name --hostname github.com --repo flrngel/local-browser-bridge --signer-workflow flrngel/local-browser-bridge/.github/workflows/deploy.yml --source-ref refs/tags/v$Version --source-digest $FinalSha --deny-self-hosted-runners"
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
    [void]$OutputTask.GetAwaiter().GetResult()
    [void]$ErrorTask.GetAwaiter().GetResult()
    if ($Process.ExitCode -ne 0) { throw "GitHub attestation verification failed." }
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
& $TrustedGit @GitCommon -C $Repository checkout --detach --force $FinalSha
if ($LASTEXITCODE -ne 0) { throw "Exact detached checkout failed." }

$ObservedHead = (& $TrustedGit @GitCommon -C $Repository rev-parse --verify HEAD).Trim()
$SymbolicHead = @(& $TrustedGit @GitCommon -C $Repository symbolic-ref -q HEAD 2>$null)
$SymbolicHeadExit = $LASTEXITCODE
$Dirty = @(& $TrustedGit @GitCommon -C $Repository status --porcelain=v1 --untracked-files=all)
$Ignored = @(& $TrustedGit @GitCommon -C $Repository ls-files --others --ignored --exclude-standard)
if ($ObservedHead -cne $FinalSha) { throw "Repository HEAD does not equal FINAL_SHA." }
if ($SymbolicHeadExit -ne 1 -or $SymbolicHead.Count -ne 0) { throw "Repository HEAD must be detached." }
if ($Dirty.Count -ne 0) { throw "Repository checkout must be clean, including untracked files." }
if ($Ignored.Count -ne 0) { throw "Repository checkout must not contain ignored files." }

$TrustedRelativeFiles = @(
  "scripts/browser-evidence-candidate.ps1",
  "scripts/write-browser-evidence-record.ps1",
  "scripts/test-windows-browser-api.ps1",
  "scripts/record-computer-helper-chain.ps1",
  "scripts/sanitize-browser-evidence-screenshot.ps1",
  "evidence/v0.12.11/browser/operator-results.template.json"
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
```

Only after that block passes, extract once into the already-new empty directory
and run the exact byte-verified binder:

```powershell
[void][Reflection.Assembly]::Load("System.IO.Compression.FileSystem")
[IO.Compression.ZipFile]::ExtractToDirectory(
  (Join-Path $Candidate "local-browser-bridge-extension-v$Version.zip"),
  $ExtensionDirectory
)

$Scripts = Join-Path $TrustedRoot "scripts"
& "$Scripts\browser-evidence-candidate.ps1" -Mode Preflight -Version $Version `
  -FinalSha $FinalSha -Repository $Repository -TrustedGitExecutable $TrustedGit `
  -TrustedEmptyHooksDirectory $EmptyHooks -ChecksumManifest $Manifest `
  -ChecksumManifestSha256 $ManifestSha `
  -ServerExecutable (Join-Path $Candidate "local-browser-bridge-v$Version-windows-x86_64.exe") `
  -ComputerHelperExecutable (Join-Path $Candidate "local-computer-helper-v$Version-windows-x86_64.exe") `
  -ExtensionZip (Join-Path $Candidate "local-browser-bridge-extension-v$Version.zip") `
  -ExtractedExtension $ExtensionDirectory `
  -OutputRecord (Join-Path $EvidenceDirectory "candidate-preflight.json")

& "$Scripts\write-browser-evidence-record.ps1" -Mode InitializeOperator `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -OutputRecord (Join-Path $EvidenceDirectory "operator-results.json")
```

## 2. Run the packaged helper chain

Keep stock Chrome open. The recorder generates the 256-bit Base64URL token in
memory, starts the exact server with `--no-update-check`, proves its sole
listener is `127.0.0.1:17373`, starts the exact packaged helper, and calls the
browser matrix with `-PassThruOwnedTarget`. It uses fresh frames and real
`computer.share.start`, `computer.observe`, `computer.click`,
`computer.typeText`, `computer.key`, and `computer.share.stop` calls.

```powershell
& "$Scripts\record-computer-helper-chain.ps1" -Mode Run `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -ApiMatrixRunner "$Scripts\test-windows-browser-api.ps1" `
  -ApiMatrixRecord (Join-Path $EvidenceDirectory "browser-api-matrix.json") `
  -ServerExecutable (Join-Path $Candidate "local-browser-bridge-v$Version-windows-x86_64.exe") `
  -HelperExecutable (Join-Path $Candidate "local-computer-helper-v$Version-windows-x86_64.exe") `
  -ExtensionDirectory $ExtensionDirectory `
  -RawScreenshotDirectory $RawScreenshotDirectory `
  -OutputRecord (Join-Path $EvidenceDirectory "browser-computer-helper-chain.json")
```

The canonical UI sequence is short and explicit:

1. Create one dedicated stock-Chrome window. The recorder compares fresh
   `computer.status` results before and after `Control+N`, requires the Chrome
   count to increase by exactly one with the baseline unchanged, binds that
   exact window in memory, and reuses it for every dedicated-window epoch.
   Navigate only that window to `chrome://extensions` and verify no candidate
   card exists before **Load unpacked**.
2. Capture Developer Mode from a fresh helper frame. Change it only when the
   captured state is disabled and consent is given.
3. Click **Load unpacked**. In the native picker the helper focuses the address
   with `Control+L`, types the exact candidate directory, presses Enter,
   obtains a fresh frame, and actually clicks **Select Folder**.
4. Verify exactly one enabled v0.12.11 card with no load error. Open the
   Extensions menu and choose Local Browser Bridge; do not assume it is pinned.
5. In the first popup frames, capture Full Access and verify the saved token is
   initially unconfigured. Enable Full Access only when required, then save the
   ephemeral credential with action-time consent.
6. Run all 25 browser methods. Start a fresh exact-tab lease and reproduce the
   deterministic result `Hello, Bridge Matrix. blue selected.`
7. In one dedicated-window share, use helper clicks to show the exact
   `chrome://extensions` tab, the exact demo tab, and the demo's Coordinate
   target. Capture the three raw screenshots below.
8. Stop the browser lease, open the popup through the Extensions menu, click
   **Clear saved token**, wait for the popup's native `confirm()` dialog, clear
   it with separate initiation and confirmation consents, and restore Full
   Access.
9. Explicitly helper-click the existing `chrome://extensions` tab before
   removing the exact test-owned card and restoring Developer Mode. Close only
   the dedicated test window.
10. Force-terminate the exact captured Windows helper supervisor. Its
    kill-on-close Job Object owns the worker, so the record requires a nonzero
    forced disposition, helper disconnect, no remaining child/listener, and
    unchanged helper bytes. Then force-stop the exact server and prove its
    listener and bytes.

The recorder retains no raw HWND, tab, frame, share, session, token, or API
response body. If it fails after a possible UI mutation, it attempts bounded
helper-driven rollback. If the exact helper/server transport is unavailable or
any state remains unresolved, it reports an incomplete rollback with nonsecret
operator guidance and creates no passing helper record.

## 3. Three retained visual proofs

| Purpose | Sanitized image | Human-visible state required |
|---|---|---|
| `extension-loaded` | `browser-01-extension-loaded.png` | Stock `chrome://extensions` shows exactly one enabled unpacked Local Browser Bridge v0.12.11 card, no load error, and Chrome's native debugger-use indicator while the bridge lease is active. |
| `api-action-result` | `browser-02-api-action-result.png` | The exact loopback demo visibly says `Hello, Bridge Matrix. blue selected.` after the browser API action. |
| `computer-share-action` | `browser-03-computer-share-action.png` | The exact shared demo visibly shows `coordinate:true` after the native helper click plus the synthetic session pointer. |

Sanitize first. `Sanitize` writes a PNG and a pending record with
`manualVisualReviewConfirmed:false`; it never pre-attests review.

```powershell
$Captures = [ordered]@{
  "extension-loaded" = "browser-01-extension-loaded"
  "api-action-result" = "browser-02-api-action-result"
  "computer-share-action" = "browser-03-computer-share-action"
}
function Read-CropInteger([string]$Label, [int]$Minimum) {
  $Value = 0
  if (-not [int]::TryParse((Read-Host $Label), [ref]$Value) -or $Value -lt $Minimum) {
    throw "$Label must be an integer greater than or equal to $Minimum."
  }
  return $Value
}
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
```

Automation must then pause. A human opens every sanitized PNG, verifies the
exact table state, and confirms no sensitive pixels are present. Only after
that review may the finalized sidecar be created:

```powershell
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
```

## 4. Postflight, disposal, and finalization

The recorder has already removed the Chrome card, cleared the token, restored
the two settings, closed the exact test window, and stopped its exact
processes. Run postflight while the extracted eleven-file directory still
exists so the binder can prove candidate immutability:

```powershell
& "$Scripts\browser-evidence-candidate.ps1" -Mode Postflight -Version $Version `
  -FinalSha $FinalSha -Repository $Repository -TrustedGitExecutable $TrustedGit `
  -TrustedEmptyHooksDirectory $EmptyHooks -ChecksumManifest $Manifest `
  -ChecksumManifestSha256 $ManifestSha `
  -ServerExecutable (Join-Path $Candidate "local-browser-bridge-v$Version-windows-x86_64.exe") `
  -ComputerHelperExecutable (Join-Path $Candidate "local-computer-helper-v$Version-windows-x86_64.exe") `
  -ExtensionZip (Join-Path $Candidate "local-browser-bridge-extension-v$Version.zip") `
  -ExtractedExtension $ExtensionDirectory `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -OutputRecord (Join-Path $EvidenceDirectory "candidate-postflight.json")
```

Before deleting scratch, require its exact six ordinary-file inventory. Before
deleting the extracted folder, require the exact eleven ordinary candidate
files. Delete names individually, then delete each now-empty directory
nonrecursively. Refusing to delete an unexpected extracted-extension inventory
is a cleanup failure, not permission to recurse.

The following performs those two exact ownership-bound deletions; it never
recurses through either flat candidate directory.

```powershell
$ExpectedRawNames = @($Captures.Values | ForEach-Object { $_ + ".raw.png" }) +
  @($Captures.Values | ForEach-Object { $_ + ".pending.json" })
$ExpectedExtensionNames = @(
  "background.js","content.js","dom-core.js","frame-agent.js","lib.js",
  "manifest.json","popup.css","popup.html","popup.js","stop-guard.js","LICENSE"
)
Remove-ExactFlatOwnedDirectory $RawScreenshotDirectory $ExpectedRawNames
Remove-ExactFlatOwnedDirectory $ExtensionDirectory $ExpectedExtensionNames
```

Now edit only the initialized operator record. Copy the live initial values
from the helper record, attest the action-time human checkpoints and three
visual reviews truthfully, and record exact restoration. Do not edit its
`candidateBinding`. Set `rawScreenshotScratchDeleted:true`,
`pendingReviewRecordsDeleted:true`, and
`extractedExtensionDirectoryDeleted:true` only because the exact deletions
above succeeded. The operator cleanup must also record
`confirmationAcceptedByHuman:true` for saved-token confirmation.

```powershell
$OperatorPath = Join-Path $EvidenceDirectory "operator-results.json"
Write-Host "Edit and save only $OperatorPath, using the helper record and human receipts."
if ((Read-Host "Type OPERATOR-RECORD-SAVED after the exact record is saved") -cne "OPERATOR-RECORD-SAVED") {
  throw "The operator record was not explicitly saved."
}
```

```powershell
$Sidecars = @($Captures.Values | ForEach-Object {
  Join-Path $EvidenceDirectory ($_ + ".json")
})
& "$Scripts\write-browser-evidence-record.ps1" -Mode Finalize `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -PostflightRecord (Join-Path $EvidenceDirectory "candidate-postflight.json") `
  -ApiMatrixRecord (Join-Path $EvidenceDirectory "browser-api-matrix.json") `
  -ComputerHelperRecord (Join-Path $EvidenceDirectory "browser-computer-helper-chain.json") `
  -OperatorResults (Join-Path $EvidenceDirectory "operator-results.json") `
  -ScreenshotRecords $Sidecars `
  -OutputRecord (Join-Path $EvidenceDirectory "browser-acceptance.json")

$FinalEntries = @([IO.DirectoryInfo]::new($EvidenceDirectory).GetFileSystemInfos())
if ($FinalEntries.Count -ne 12 -or
    @($FinalEntries | Where-Object {
      $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)
    }).Count -ne 0) {
  throw "The finalized evidence directory is not the exact twelve-file ordinary inventory."
}
$RunSucceeded = $true
}
catch {
  $PrimaryFailure = $_
}
finally {
  try { Restore-CoordinatorEnvironment }
  catch { $CleanupErrors.Add("environment: $($_.Exception.Message)") }
  try { [Environment]::SetEnvironmentVariable("PSModulePath", $InheritedPSModulePath, "Process") }
  catch { $CleanupErrors.Add("PSModulePath: $($_.Exception.Message)") }
  if ($null -ne $SecureGhToken) {
    try { $SecureGhToken.Dispose() }
    catch { $CleanupErrors.Add("GitHub token disposal: $($_.Exception.Message)") }
  }
  $SecureGhToken = $null
  foreach ($Owned in @($OwnedDirectories | Sort-Object { $_.Length } -Descending)) {
    if ($RunSucceeded -and $Owned -ceq $EvidenceDirectory) { continue }
    try { Remove-TestOwnedTree $Owned }
    catch { $CleanupErrors.Add("owned-directory ${Owned}: $($_.Exception.Message)") }
  }
  if ($CleanupErrors.Count -ne 0 -and -not [String]::IsNullOrWhiteSpace($EvidenceDirectory)) {
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
}
if ($null -ne $PrimaryFailure) {
  $Suffix = if ($CleanupErrors.Count -eq 0) { "Outer rollback completed." }
    else { "Outer rollback incomplete: " + ($CleanupErrors -join "; ") }
  throw "$($PrimaryFailure.Exception.Message) $Suffix"
}
if ($CleanupErrors.Count -ne 0) {
  throw "Acceptance cleanup failed and passing output was invalidated: $($CleanupErrors -join '; ')"
}
```

Finalization fails closed for cross-run bindings, a helper or server hash
mismatch, missing live initial-state receipts, a picker action without a
`computer.click` and closed-picker proof, cleanup without an explicit
`chrome://extensions` tab switch, a human/external-only product source, stale
frames, an open share, a failed API result, a screenshot digest mismatch,
missing consent, incomplete restoration, unreviewed pixels, or an unexpected
retained file.

The outer `finally` restores the coordinator environment, disposes only the
random test-owned clone/home/config/hook/template directories after an exact
ownership and non-reparse walk, and invalidates a passing output if that cleanup
fails. It never deletes `$PrivateParent`, `$ShortSourceParent`, the supplied
download directory, or an unverified path.
