#requires -Version 5.1

param(
  [switch]$SelfTest,
  [string]$FinalSha,
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
  [string]$ExternalSurfacePreflightAttestation,
  [string]$ExternalSurfacePostflightAttestation,
  [string]$GitHubTokenPipeName,
  [string]$CleanCoordinatorNonce
)

function ConvertFrom-JsonPreservingStrings {
  param([Parameter(Mandatory = $true, ValueFromPipeline = $true)][string]$Json)
  process {
    $Command = Get-Command ConvertFrom-Json -CommandType Cmdlet -ErrorAction Stop
    if ($Command.Parameters.ContainsKey("DateKind")) {
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
    'FinalEntries.Count -ne 22',
    'Invoke-ExactPs51SelfTest',
    'Invoke-BoundedExactExtensionZipExtraction',
    'Invoke-IndependentReviewerExchange',
    'Assert-ReviewerCropDecision',
    'Assert-ReviewerSixCropDecision',
    'scripts/write-stock-chrome-operator-response.ps1',
    'ExternalSurfacePreflightAttestation',
    'ExternalSurfacePostflightAttestation',
    'EXTERNAL_SURFACE_POSTFLIGHT_REQUIRED',
    '-ScopedApprovalRecord',
    'independent-visual-review.json',
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
    ('[IO.Compression.ZipFile]::' + 'ExtractToDirectory'),
    ('Manual' + 'VisualReviewConfirmed'), ('Attest' + 'Review'),
    ('OPERATOR-' + 'RECORD-SAVED'), ('Read-' + 'CropInteger'),
    ('confirmationAcceptedBy' + 'Human'), ('human' + 'VisualReview')
  )) {
    if ($Source.Contains($Forbidden)) {
      throw "Stock-Chrome coordinator self-test found a forbidden primitive or system-home mutation."
    }
  }
  $ReadHostToken = 'Read' + '-Host'
  if ([regex]::Matches($Source, [regex]::Escape($ReadHostToken)).Count -ne 1 -or
      -not $Source.Contains('Independent least-privilege GitHub acceptance token')) {
    throw "Stock-Chrome coordinator self-test permits only the secure GitHub token prompt."
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
  Invoke-AttestationSelectionSelfTest

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

    $CreateOncePath = [IO.Path]::Combine($SelfTestRoot, "create-once.json")
    $CreateOnceDigest = $null
    Write-CreateOncePrivateJson $CreateOncePath ([ordered]@{ value = "bound" }) `
      "self-test record" ([ref]$CreateOnceDigest)
    $StableCreateOnce = Read-StablePrivateJson $CreateOncePath "self-test record"
    if ($StableCreateOnce.Value.value -cne "bound" -or
        $StableCreateOnce.Sha256 -cne $CreateOnceDigest) {
      throw "Stock-Chrome stable create-once JSON self-test failed."
    }
    $ReplayRejected = $false
    try { Write-CreateOncePrivateJson $CreateOncePath ([ordered]@{ value = "replay" }) "self-test replay" }
    catch { $ReplayRejected = $true }
    if (-not $ReplayRejected) { throw "Stock-Chrome create-once replay self-test failed." }

    $LedgerDirectory = New-SelfTestExtractionDirectory "durable-ledger"
    $LedgerBinding = [ordered]@{
      schemaVersion = 3
      version = "0.12.49"
      releaseTag = "v0.12.49"
      sourceSha = [String]::new([char]"1", 40)
      workflowRunId = "123"
      workflowRunAttempt = "1"
      workflowEvent = "workflow_dispatch"
      workflowRef = "refs/heads/main"
      workflowPath = ".github/workflows/deploy.yml"
      artifactId = "456"
      artifactZipSha256 = [String]::new([char]"3", 64)
    }
    $LedgerClaim = New-DurableCandidateExecutionClaim $LedgerDirectory $LedgerBinding
    $DuplicateClaimRejected = $false
    try { [void](New-DurableCandidateExecutionClaim $LedgerDirectory $LedgerBinding) }
    catch { $DuplicateClaimRejected = $true }
    if (-not $DuplicateClaimRejected) {
      throw "Stock-Chrome durable ledger accepted a duplicate frozen-candidate claim."
    }
    Write-DurableCandidateExecutionOutcome $LedgerClaim $false $false "self-test"
    if (@([IO.DirectoryInfo]::new($LedgerDirectory).GetFileSystemInfos()).Count -ne 2) {
      throw "Stock-Chrome durable ledger did not retain the exact claim and outcome pair."
    }

    $EnvelopeRequestId = [String]::new([char]"1", 32)
    $EnvelopeRequestSha = [String]::new([char]"2", 64)
    $EnvelopeCandidateSha = [String]::new([char]"3", 64)
    $EnvelopeInputSha = [String]::new([char]"4", 64)
    $EnvelopeExecutorRef = [String]::new([char]"5", 64)
    $EnvelopeReviewerRef = [String]::new([char]"6", 64)
    $GoodEnvelope = [pscustomobject][ordered]@{
      schemaVersion = 1
      evidenceType = "stock-user-chrome-reviewer-response"
      requestId = $EnvelopeRequestId
      requestSha256 = $EnvelopeRequestSha
      candidateBindingSha256 = $EnvelopeCandidateSha
      inputDigestSha256 = $EnvelopeInputSha
      responderKind = "independent-agent"
      responderSessionRef = $EnvelopeReviewerRef
      respondedAtUtc = "2026-08-24T00:00:01.0000000+00:00"
      decision = [pscustomobject]@{ value = "fixture" }
    }
    Assert-IndependentReviewerResponseEnvelope $GoodEnvelope $EnvelopeRequestId `
      $EnvelopeRequestSha $EnvelopeCandidateSha $EnvelopeInputSha `
      $EnvelopeExecutorRef $EnvelopeReviewerRef
    foreach ($EnvelopeMutation in @("request-replay", "input-swap", "same-session", "old-human-field")) {
      $InvalidEnvelope = ConvertFrom-JsonPreservingStrings ($GoodEnvelope | ConvertTo-Json -Depth 8)
      switch ($EnvelopeMutation) {
        "request-replay" { $InvalidEnvelope.requestSha256 = [String]::new([char]"7", 64) }
        "input-swap" { $InvalidEnvelope.inputDigestSha256 = [String]::new([char]"8", 64) }
        "same-session" { $InvalidEnvelope.responderSessionRef = $EnvelopeExecutorRef }
        "old-human-field" {
          $InvalidEnvelope | Add-Member -NotePropertyName humanReviewed -NotePropertyValue $true
        }
      }
      $EnvelopeRejected = $false
      try {
        Assert-IndependentReviewerResponseEnvelope $InvalidEnvelope $EnvelopeRequestId `
          $EnvelopeRequestSha $EnvelopeCandidateSha $EnvelopeInputSha `
          $EnvelopeExecutorRef $EnvelopeReviewerRef
      }
      catch { $EnvelopeRejected = $true }
      if (-not $EnvelopeRejected) {
        throw "Stock-Chrome reviewer envelope accepted $EnvelopeMutation."
      }
    }
    $TimestampStart = [DateTimeOffset]::ParseExact(
      "2026-08-24T00:00:00.0000000Z", "o", [Globalization.CultureInfo]::InvariantCulture,
      [Globalization.DateTimeStyles]::RoundtripKind
    )
    [void](Assert-FreshCanonicalResponseTimestamp `
      "2026-08-24T00:00:01.0000000Z" $TimestampStart ($TimestampStart.AddSeconds(2)) `
      "self-test canonical reviewer timestamp")
    $OffsetTimestampRejected = $false
    try {
      [void](Assert-FreshCanonicalResponseTimestamp `
        "2026-08-24T00:00:01.0000000+00:00" $TimestampStart ($TimestampStart.AddSeconds(2)) `
        "self-test offset reviewer timestamp")
    }
    catch { $OffsetTimestampRejected = $true }
    if (-not $OffsetTimestampRejected) {
      throw "Stock-Chrome reviewer exchange accepted a noncanonical +00:00 timestamp."
    }

    $SavedReviewDirectory = $script:ReviewExchangeDirectory
    $SavedReviewArtifacts = $script:ReviewExchangeArtifacts
    $SavedReviewReservations = $script:ReviewResponseReservations
    $SavedReviewExpectedTransients = $script:ReviewExpectedTransientArtifacts
    $PublicationDirectory = New-SelfTestExtractionDirectory "review-publication"
    $script:ReviewExchangeDirectory = $PublicationDirectory
    $script:ReviewExchangeArtifacts = New-Object Collections.Generic.List[string]
    $script:ReviewResponseReservations = New-Object Collections.Generic.List[object]
    $script:ReviewExpectedTransientArtifacts = New-Object Collections.Generic.List[string]
    try {
      $PublicationId = [String]::new([char]"a", 32)
      $PublicationResponse = Join-Path $PublicationDirectory "response-$PublicationId.json"
      $PublicationTemporary = "$PublicationResponse.new"
      $PublicationClaimed = Join-Path $PublicationDirectory "response-$PublicationId.claimed.json"
      $Partial = [IO.File]::Open(
        $PublicationTemporary, [IO.FileMode]::CreateNew,
        [IO.FileAccess]::Write, [IO.FileShare]::None
      )
      try {
        $PartialBytes = [Text.Encoding]::ASCII.GetBytes('{"partial":')
        $Partial.Write($PartialBytes, 0, $PartialBytes.Length)
        $Partial.Flush($true)
      }
      finally { $Partial.Dispose() }
      $PartialRejected = $false
      try {
        Claim-PublishedReviewerResponse `
          $PublicationResponse $PublicationClaimed ([DateTimeOffset]::UtcNow.AddMilliseconds(250))
      }
      catch { $PartialRejected = $true }
      if (-not $PartialRejected -or [IO.File]::Exists($PublicationClaimed)) {
        throw "Stock-Chrome reviewer exchange accepted a partial publication."
      }
      [IO.File]::Delete($PublicationTemporary)
      $ExpiredId = [String]::new([char]"b", 32)
      $ExpiredResponse = Join-Path $PublicationDirectory "response-$ExpiredId.json"
      $ExpiredClaimed = Join-Path $PublicationDirectory "response-$ExpiredId.claimed.json"
      [IO.File]::WriteAllText(
        $ExpiredResponse, "{}`n", [Text.UTF8Encoding]::new($false, $true)
      )
      $ExpiredPublicationRejected = $false
      try {
        Claim-PublishedReviewerResponse $ExpiredResponse $ExpiredClaimed `
          ([DateTimeOffset]::UtcNow.AddSeconds(-1))
      }
      catch { $ExpiredPublicationRejected = $true }
      if (-not $ExpiredPublicationRejected -or [IO.File]::Exists($ExpiredClaimed) -or
          -not [IO.File]::Exists($ExpiredResponse)) {
        throw "Stock-Chrome reviewer exchange accepted an already-published expired response."
      }
      [IO.File]::Delete($ExpiredResponse)
      $PublicationBytes = [Text.UTF8Encoding]::new($false).GetBytes("{}`n")
      $Published = [IO.File]::Open(
        $PublicationTemporary, [IO.FileMode]::CreateNew,
        [IO.FileAccess]::Write, [IO.FileShare]::None
      )
      try {
        $Published.Write($PublicationBytes, 0, $PublicationBytes.Length)
        $Published.Flush($true)
      }
      finally { $Published.Dispose(); [Array]::Clear($PublicationBytes, 0, $PublicationBytes.Length) }
      [IO.File]::Move($PublicationTemporary, $PublicationResponse)
      Claim-PublishedReviewerResponse `
        $PublicationResponse $PublicationClaimed ([DateTimeOffset]::UtcNow.AddSeconds(2))
      if (-not [IO.File]::Exists($PublicationClaimed)) {
        throw "Stock-Chrome reviewer exchange did not claim an atomic publication."
      }
      $CanonicalPublication = Read-StablePrivateJson `
        $PublicationClaimed "self-test canonical reviewer response"
      Assert-CanonicalCompactJsonResponse `
        $CanonicalPublication "self-test canonical reviewer response"
      foreach ($AmbiguousJson in @(
        '{"decision":{},"decision":{}}' + "`n",
        '{"decision":{},"Decision":{}}' + "`n",
        '{"decision":{}} trailing' + "`n",
        "{`n  `"decision`": {}`n}`n"
      )) {
        $AmbiguousPath = Join-Path $PublicationDirectory `
          ("ambiguous-" + [Guid]::NewGuid().ToString("N") + ".json")
        [IO.File]::WriteAllText(
          $AmbiguousPath, $AmbiguousJson, [Text.UTF8Encoding]::new($false, $true)
        )
        $AmbiguousRejected = $false
        try {
          $AmbiguousStable = Read-StablePrivateJson `
            $AmbiguousPath "self-test ambiguous reviewer response"
          Assert-CanonicalCompactJsonResponse `
            $AmbiguousStable "self-test ambiguous reviewer response"
        }
        catch { $AmbiguousRejected = $true }
        finally { if ([IO.File]::Exists($AmbiguousPath)) { [IO.File]::Delete($AmbiguousPath) } }
        if (-not $AmbiguousRejected) {
          throw "Stock-Chrome reviewer exchange accepted a duplicate, case-colliding, trailing, or noncanonical response."
        }
      }
      $ReservationReplayRejected = $false
      try { [IO.File]::WriteAllText($PublicationResponse, '{"replay":true}') }
      catch { $ReservationReplayRejected = $true }
      if (-not $ReservationReplayRejected) {
        throw "Stock-Chrome reviewer exchange allowed a reserved response replay."
      }
      $InputPath = Join-Path $PublicationDirectory "digest-input.png"
      $InputBytes = New-Object byte[] 25
      [byte[]]$InputSignature = @(137, 80, 78, 71, 13, 10, 26, 10)
      $InputSignature.CopyTo($InputBytes, 0)
      [Text.Encoding]::ASCII.GetBytes("IHDR").CopyTo($InputBytes, 12)
      $InputBytes[19] = 120
      $InputBytes[23] = 32
      [IO.File]::WriteAllBytes($InputPath, $InputBytes)
      $InputFacts = Read-StablePrivatePng $InputPath "self-test reviewer digest input"
      $InputBytes[24] = 1
      [IO.File]::WriteAllBytes($InputPath, $InputBytes)
      $ChangedInputRejected = $false
      try {
        [void](Assert-UnchangedPrivatePng `
          $InputPath $InputFacts "self-test post-response reviewer input" -RequireBytes)
      }
      catch { $ChangedInputRejected = $true }
      [IO.File]::Delete($InputPath)
      [Array]::Clear($InputBytes, 0, $InputBytes.Length)
      [Array]::Clear($InputSignature, 0, $InputSignature.Length)
      if (-not $ChangedInputRejected) {
        throw "Stock-Chrome reviewer exchange accepted a changed post-response input."
      }
      Remove-RegisteredReviewExchangeArtifact $PublicationClaimed
      $ExtraPath = Join-Path $PublicationDirectory `
        ("request-" + [String]::new([char]"b", 32) + ".json")
      [IO.File]::WriteAllText($ExtraPath, "{}`n", [Text.UTF8Encoding]::new($false))
      $ExtraRejected = $false
      try { Remove-ExactReviewExchangeDirectory } catch { $ExtraRejected = $true }
      if (-not $ExtraRejected) {
        throw "Stock-Chrome reviewer exchange accepted an unregistered extra artifact."
      }
      $BestEffortReportedRemainder = $false
      try { Remove-KnownReviewExchangeArtifactsAfterFailure }
      catch { $BestEffortReportedRemainder = $true }
      if (-not $BestEffortReportedRemainder -or
          [IO.File]::Exists($PublicationResponse) -or
          [IO.File]::Exists($PublicationTemporary) -or
          -not [IO.File]::Exists($ExtraPath)) {
        throw "Stock-Chrome reviewer failure cleanup did not delete only its known ordinary scratch."
      }
      [IO.File]::Delete($ExtraPath)
      [IO.Directory]::Delete($PublicationDirectory, $false)
    }
    finally {
      foreach ($Held in $script:ReviewResponseReservations) {
        try { $Held.Stream.Dispose() } catch {}
      }
      $script:ReviewExchangeDirectory = $SavedReviewDirectory
      $script:ReviewExchangeArtifacts = $SavedReviewArtifacts
      $script:ReviewResponseReservations = $SavedReviewReservations
      $script:ReviewExpectedTransientArtifacts = $SavedReviewExpectedTransients
    }

    $GoodCrop = [pscustomobject]@{
      cropX = 0; cropY = 0; cropWidth = 240; cropHeight = 120
      requiredStateVisible = $true; sensitivePixelsInsideCrop = $false; uncertain = $false
    }
    $RawFacts = [pscustomobject]@{ width = 320; height = 180 }
    Assert-ReviewerCropDecision $GoodCrop $RawFacts
    if (-not (Test-UnableReviewerDecision ([pscustomobject]@{ unable = $true }))) {
      throw "Stock-Chrome coordinator did not recognize the universal unable decision."
    }
    foreach ($BadUnable in @(
      [pscustomobject]@{ unable = $false },
      [pscustomobject]@{ unable = $true; cropX = 0 }
    )) {
      $UnableRejected = $false
      try { [void](Test-UnableReviewerDecision $BadUnable) } catch { $UnableRejected = $true }
      if (-not $UnableRejected) {
        throw "Stock-Chrome coordinator accepted a malformed unable decision."
      }
    }
    foreach ($CropMutation in @(
      "extra-human", "uncertain", "sensitive", "bounds", "fractional", "string-bool"
    )) {
      $InvalidCrop = ConvertFrom-JsonPreservingStrings ($GoodCrop | ConvertTo-Json)
      switch ($CropMutation) {
        "extra-human" {
          $InvalidCrop | Add-Member -NotePropertyName humanReviewed -NotePropertyValue $true
        }
        "uncertain" { $InvalidCrop.uncertain = $true }
        "sensitive" { $InvalidCrop.sensitivePixelsInsideCrop = $true }
        "bounds" { $InvalidCrop.cropWidth = 321 }
        "fractional" { $InvalidCrop.cropX = 0.5 }
        "string-bool" { $InvalidCrop.requiredStateVisible = "True" }
      }
      $Rejected = $false
      try { Assert-ReviewerCropDecision $InvalidCrop $RawFacts } catch { $Rejected = $true }
      if (-not $Rejected) { throw "Stock-Chrome crop review accepted $CropMutation." }
    }

    $ExpectedReviewEntries = @()
    $ActualReviewEntries = @()
    for ($Index = 0; $Index -lt 6; $Index += 1) {
      $Sequence = $Index + 1
      $Digest = [String]::new([char](49 + $Index), 64)
      $ExpectedReviewEntries += [pscustomobject]@{
        purpose = "purpose-$Sequence"; image = "browser-0$Sequence.png"; sha256 = $Digest
        width = 240; height = 120; requiredVisibleStateSha256 = [String]::new([char]"a", 64)
      }
      $ActualReviewEntries += [pscustomobject]@{
        sequence = $Sequence; purpose = "purpose-$Sequence"; image = "browser-0$Sequence.png"
        sha256 = $Digest; width = 240; height = 120
        requiredVisibleStateSha256 = [String]::new([char]"a", 64)
        digestMatched = $true; requiredStateVerdict = "pass"
        sensitivePixelsObserved = $false; uncertain = $false
      }
    }
    $GoodReview = [pscustomobject]@{
      entries = $ActualReviewEntries
      aggregate = [pscustomobject]@{
        reviewedCropCount = 6; everySanitizedCropOpenedByReviewer = $true
        allImageDigestsMatched = $true; requiredVisibleStateConfirmedByReviewer = $true
        noSensitivePixelsObservedByReviewer = $true; noUncertaintyReported = $true
        visualJudgmentNotPixelSafetyProof = $true
      }
    }
    if (@(Assert-ReviewerSixCropDecision $GoodReview $ExpectedReviewEntries).Count -ne 6) {
      throw "Stock-Chrome independent six-crop review valid fixture failed."
    }
    foreach ($ReviewMutation in @(
      "reordered", "digest", "uncertain", "sensitive", "aggregate",
      "string-number", "string-bool", "aggregate-count"
    )) {
      $InvalidReview = ConvertFrom-JsonPreservingStrings ($GoodReview | ConvertTo-Json -Depth 8)
      switch ($ReviewMutation) {
        "reordered" { $InvalidReview.entries[0].sequence = 2 }
        "digest" { $InvalidReview.entries[0].sha256 = [String]::new([char]"f", 64) }
        "uncertain" { $InvalidReview.entries[0].uncertain = $true }
        "sensitive" { $InvalidReview.entries[0].sensitivePixelsObserved = $true }
        "aggregate" { $InvalidReview.aggregate.noUncertaintyReported = $false }
        "string-number" { $InvalidReview.entries[0].sequence = "1" }
        "string-bool" { $InvalidReview.entries[0].digestMatched = "True" }
        "aggregate-count" { $InvalidReview.aggregate.reviewedCropCount = "6" }
      }
      $Rejected = $false
      try { [void](Assert-ReviewerSixCropDecision $InvalidReview $ExpectedReviewEntries) }
      catch { $Rejected = $true }
      if (-not $Rejected) { throw "Stock-Chrome six-crop review accepted $ReviewMutation." }
    }
    $CleanupDisclosureRoot = Join-Path $SelfTestRoot "cleanup-disclosure"
    [IO.Directory]::CreateDirectory($CleanupDisclosureRoot) | Out-Null
    $UnknownDisclosure = New-FailureCleanupDisclosure `
      -PartialEvidenceDirectory $CleanupDisclosureRoot `
      -RawDirectory $CleanupDisclosureRoot `
      -ReviewDirectory $null `
      -OperatorDirectory $null `
      -CleanupIssueObserved $true
    if ($UnknownDisclosure.partialEvidenceDirectoryDeleted -ne $false -or
        $UnknownDisclosure.rawScreenshotScratchDeleted -ne $false -or
        $UnknownDisclosure.sensitiveScratchDisposition -cne "unknown" -or
        $UnknownDisclosure.wrongTargetMutationDisposition -cne "unknown" -or
        $UnknownDisclosure.tokenOrCredentialValuesWrittenToAttemptJson -ne $false) {
      throw "Stock-Chrome cleanup-failure disclosure self-test hid uncertain scratch or target state."
    }
    [IO.Directory]::Delete($CleanupDisclosureRoot, $false)
    $DeletedDisclosure = New-FailureCleanupDisclosure `
      -PartialEvidenceDirectory $null -RawDirectory $null -ReviewDirectory $null `
      -OperatorDirectory $null -CleanupIssueObserved $false
    if ($DeletedDisclosure.sensitiveScratchDisposition -cne "deleted") {
      throw "Stock-Chrome cleanup disclosure did not recognize measured scratch deletion."
    }
    $SensitiveReviewEvidenceRoot = Join-Path $SelfTestRoot "sensitive-review-partial-evidence"
    [IO.Directory]::CreateDirectory($SensitiveReviewEvidenceRoot) | Out-Null
    [void]$OwnedDirectories.Add([IO.Path]::GetFullPath($SensitiveReviewEvidenceRoot))
    foreach ($CaptureName in @(
      "browser-01-extension-loaded", "browser-02-api-action-result",
      "browser-03-computer-share-action", "browser-04-stop-paused",
      "browser-05-cancel-paused", "browser-06-post-handback-resume"
    )) {
      [IO.File]::WriteAllBytes(
        (Join-Path $SensitiveReviewEvidenceRoot ($CaptureName + ".png")), [byte[]](1)
      )
      [IO.File]::WriteAllText(
        (Join-Path $SensitiveReviewEvidenceRoot ($CaptureName + ".json")), "{}`n",
        [Text.UTF8Encoding]::new($false, $true)
      )
    }
    Remove-TestOwnedTree $SensitiveReviewEvidenceRoot
    $SensitiveReviewDisclosure = New-FailureCleanupDisclosure `
      -PartialEvidenceDirectory $SensitiveReviewEvidenceRoot `
      -RawDirectory $null -ReviewDirectory $null -OperatorDirectory $null `
      -CleanupIssueObserved $false
    if (-not $SensitiveReviewDisclosure.partialEvidenceDirectoryDeleted -or
        $SensitiveReviewDisclosure.sensitiveScratchDisposition -cne "deleted" -or
        [IO.Directory]::Exists($SensitiveReviewEvidenceRoot)) {
      throw "Stock-Chrome sensitive-review failure cleanup retained a PNG or sidecar."
    }
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
      $FinalSha, $WorkflowRunId, $WorkflowRunAttempt,
      $ReleaseCandidateArtifactId, $ReleaseCandidateArtifactZipSha256,
      $ManifestSha, $Candidate, $CandidateBinding, $PrivateParent,
      $TrustedGit, $TrustedGh, $ExternalSurfacePreflightAttestation,
      $ExternalSurfacePostflightAttestation
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
    $Info.EnvironmentVariables["LBB_COORDINATOR_EXTERNAL_SURFACE_PREFLIGHT"] = `
      $ExternalSurfacePreflightAttestation
    $Info.EnvironmentVariables["LBB_COORDINATOR_EXTERNAL_SURFACE_POSTFLIGHT"] = `
      $ExternalSurfacePostflightAttestation
    if (-not [String]::IsNullOrWhiteSpace($GitHubTokenPipeName)) {
      $Info.EnvironmentVariables["LBB_COORDINATOR_GH_TOKEN_PIPE"] = $GitHubTokenPipeName
    }
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
$ExternalSurfacePreflightAttestation = [Environment]::GetEnvironmentVariable(
  "LBB_COORDINATOR_EXTERNAL_SURFACE_PREFLIGHT", "Process"
)
$ExternalSurfacePostflightAttestation = [Environment]::GetEnvironmentVariable(
  "LBB_COORDINATOR_EXTERNAL_SURFACE_POSTFLIGHT", "Process"
)
$GitHubTokenPipeName = [Environment]::GetEnvironmentVariable(
  "LBB_COORDINATOR_GH_TOKEN_PIPE", "Process"
)
foreach ($HandoffName in @(
  "LBB_COORDINATOR_SELF_TEST",
  "LBB_COORDINATOR_FINAL_SHA",
  "LBB_COORDINATOR_WORKFLOW_RUN_ID", "LBB_COORDINATOR_WORKFLOW_RUN_ATTEMPT",
  "LBB_COORDINATOR_ARTIFACT_ID", "LBB_COORDINATOR_ARTIFACT_ZIP_SHA256",
  "LBB_COORDINATOR_MANIFEST_SHA256", "LBB_COORDINATOR_CANDIDATE",
  "LBB_COORDINATOR_CANDIDATE_BINDING", "LBB_COORDINATOR_PRIVATE_PARENT",
  "LBB_COORDINATOR_TRUSTED_GIT", "LBB_COORDINATOR_TRUSTED_GH",
  "LBB_COORDINATOR_GH_TOKEN_PIPE", "LBB_COORDINATOR_EXTERNAL_SURFACE_PREFLIGHT",
  "LBB_COORDINATOR_EXTERNAL_SURFACE_POSTFLIGHT"
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

$Version = "0.12.49"
$script:ReviewExchangeDirectory = $null
$script:ReviewExchangeArtifacts = New-Object Collections.Generic.List[string]
$script:ReviewResponseReservations = New-Object Collections.Generic.List[object]
$script:ReviewExpectedTransientArtifacts = New-Object Collections.Generic.List[string]
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
  if ($Version -cne "0.12.49" -or $FinalSha -cnotmatch '^[0-9a-f]{40}$' -or
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
  if (-not [String]::IsNullOrWhiteSpace($GitHubTokenPipeName) -and
      $GitHubTokenPipeName -cnotmatch '^lbb-gh-[0-9a-f]{32}$') {
    throw "GitHubTokenPipeName must be a fresh canonical non-secret pipe name."
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
    $Observed = Get-DirectoryAccessControlPortable $Full
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
    Set-DirectoryAccessControlPortable $Path $Security
  }
  finally { $Identity.Dispose() }
  Assert-OwnerPrivateDirectoryAcl $Path "Fresh acceptance directory"
}

function New-OpaqueSessionRef {
  $Bytes = New-Object byte[] 32
  $Rng = [Security.Cryptography.RandomNumberGenerator]::Create()
  try {
    $Rng.GetBytes($Bytes)
    $Hasher = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($Hasher.ComputeHash($Bytes))).Replace("-", "").ToLowerInvariant() }
    finally { $Hasher.Dispose() }
  }
  finally {
    [Array]::Clear($Bytes, 0, $Bytes.Length)
    $Rng.Dispose()
  }
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
  $Bytes = [Text.UTF8Encoding]::new($false, $true).GetBytes(
    ($Value | ConvertTo-Json -Depth 30 -Compress)
  )
  try { return Get-BytesSha256 $Bytes }
  finally { [Array]::Clear($Bytes, 0, $Bytes.Length) }
}

function Format-CanonicalUtc([DateTimeOffset]$Value) {
  return $Value.UtcDateTime.ToString("o", [Globalization.CultureInfo]::InvariantCulture)
}

function Assert-FreshCanonicalResponseTimestamp(
  [object]$Value,
  [DateTimeOffset]$CreatedAt,
  [DateTimeOffset]$ExpiresAt,
  [string]$Label
) {
  $Parsed = [DateTimeOffset]::MinValue
  if ($Value -isnot [string] -or
      [string]$Value -cnotmatch '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{7}Z$' -or
      -not [DateTimeOffset]::TryParseExact(
        [string]$Value, "o", [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::RoundtripKind, [ref]$Parsed
      ) -or $Parsed -lt $CreatedAt -or $Parsed -gt $ExpiresAt) {
    throw "$Label is stale, premature, noncanonical, or not Z-suffixed UTC."
  }
  return $Parsed
}

function Write-CreateOncePrivateJson(
  [string]$Path,
  [object]$Value,
  [string]$Label,
  [ref]$Sha256Out
) {
  if ([IO.File]::Exists($Path) -or [IO.Directory]::Exists($Path)) {
    throw "$Label must be create-once."
  }
  $Temporary = "$Path.new"
  if ([IO.File]::Exists($Temporary) -or [IO.Directory]::Exists($Temporary)) {
    throw "$Label has a stale temporary file."
  }
  $Bytes = [Text.UTF8Encoding]::new($false, $true).GetBytes(
    (($Value | ConvertTo-Json -Depth 30) + [Environment]::NewLine)
  )
  try {
    $Stream = [IO.File]::Open(
      $Temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None
    )
    try { $Stream.Write($Bytes, 0, $Bytes.Length); $Stream.Flush($true) }
    finally { $Stream.Dispose() }
    [IO.File]::Move($Temporary, $Path)
    if ($null -ne $Sha256Out) { $Sha256Out.Value = Get-BytesSha256 $Bytes }
  }
  finally {
    [Array]::Clear($Bytes, 0, $Bytes.Length)
    if ([IO.File]::Exists($Temporary)) { [IO.File]::Delete($Temporary) }
  }
}

function Read-StablePrivateJson([string]$Path, [string]$Label) {
  $Item = [IO.FileInfo]::new([IO.Path]::GetFullPath($Path))
  if (-not $Item.Exists -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
      $Item.Length -le 0 -or $Item.Length -gt 4MB) {
    throw "$Label is not an ordinary bounded file."
  }
  Assert-NoReparseAncestorChain $Item.FullName $Label
  $Stream = [IO.File]::Open($Item.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
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
    try {
      $Value = ConvertFrom-JsonPreservingStrings `
        ([Text.UTF8Encoding]::new($false, $true).GetString($Bytes))
    }
    catch { throw "$Label is not strict UTF-8 JSON." }
    $Hasher = [Security.Cryptography.SHA256]::Create()
    try { $Digest = ([BitConverter]::ToString($Hasher.ComputeHash($Bytes))).Replace("-", "").ToLowerInvariant() }
    finally { $Hasher.Dispose() }
    return [pscustomobject]@{ Value = $Value; Sha256 = $Digest; Bytes = $Bytes.Length }
  }
  finally {
    $Stream.Dispose()
    if ($null -ne $Bytes) { [Array]::Clear($Bytes, 0, $Bytes.Length) }
  }
}

function Assert-CanonicalCompactJsonResponse([object]$Stable, [string]$Label) {
  $Bytes = [Text.UTF8Encoding]::new($false, $true).GetBytes(
    (($Stable.Value | ConvertTo-Json -Depth 30 -Compress) + "`n")
  )
  try {
    if ($Bytes.Length -ne [int64]$Stable.Bytes -or
        (Get-BytesSha256 $Bytes) -cne [string]$Stable.Sha256) {
      throw "$Label must be canonical compact UTF-8 JSON followed by one LF; duplicate, case-colliding, reordered, or trailing data is forbidden."
    }
  }
  finally { [Array]::Clear($Bytes, 0, $Bytes.Length) }
}

function Test-UnableReviewerDecision([object]$Decision) {
  if ($null -eq $Decision.PSObject.Properties["unable"]) { return $false }
  if ((@($Decision.PSObject.Properties.Name) -join "`n") -cne "unable" -or
      $Decision.unable -isnot [bool] -or $Decision.unable -ne $true) {
    throw 'Unable reviewer decision must be exactly {"unable":true}.'
  }
  return $true
}

function Assert-ExternalSurfaceAttestationInput(
  [object]$Record,
  [object]$ReleaseBinding,
  [string]$ExpectedPhase,
  [DateTimeOffset]$Now
) {
  $ExpectedFields = @(
    "schemaVersion", "evidenceType", "phase", "releaseCandidateBinding",
    "releaseCandidateBindingSha256", "orchestrationSurface", "chromeMcpState",
    "computerUseState", "reviewerInputState", "attestorKind",
    "attestorSessionRef", "attestedAtUtc"
  )
  if ($ExpectedPhase -ceq "preflight") {
    $ExpectedChromeMcp = "not-used-before-candidate-execution"
    $ExpectedComputerUse = "released-before-candidate-execution"
    $ExpectedReviewerInput = "review-not-started"
  }
  elseif ($ExpectedPhase -ceq "postflight") {
    $ExpectedChromeMcp = "never-used-through-independent-review"
    $ExpectedComputerUse = "not-resumed-through-independent-review"
    $ExpectedReviewerInput = "exported-digest-bound-files-only"
  }
  else { throw "External-surface attestation phase is unsupported." }
  if ((@($Record.PSObject.Properties.Name) -join "`n") -cne ($ExpectedFields -join "`n") -or
      $Record.schemaVersion -ne 1 -or
      $Record.evidenceType -cne "stock-user-chrome-external-surface-attestation" -or
      $Record.phase -cne $ExpectedPhase -or
      $Record.orchestrationSurface -cne `
        "user-orchestrator-secured-ssh-exported-file-review" -or
      $Record.chromeMcpState -cne $ExpectedChromeMcp -or
      $Record.computerUseState -cne $ExpectedComputerUse -or
      $Record.reviewerInputState -cne $ExpectedReviewerInput -or
      $Record.attestorKind -cne "orchestrator-agent" -or
      [string]$Record.releaseCandidateBindingSha256 -cnotmatch '^[0-9a-f]{64}$' -or
      [string]$Record.attestorSessionRef -cnotmatch '^[0-9a-f]{64}$' -or
      $Record.releaseCandidateBindingSha256 -cne (Get-CanonicalObjectSha256 $ReleaseBinding) -or
      ($Record.releaseCandidateBinding | ConvertTo-Json -Depth 30 -Compress) -cne
        ($ReleaseBinding | ConvertTo-Json -Depth 30 -Compress)) {
    throw "External-surface attestation is not exact, phase-scoped, candidate-bound, and role-bound."
  }
  $AttestedAt = [DateTimeOffset]::MinValue
  if ([string]$Record.attestedAtUtc -cnotmatch `
      '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{7}Z$' -or
      -not [DateTimeOffset]::TryParseExact(
        [string]$Record.attestedAtUtc, "o", [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::RoundtripKind, [ref]$AttestedAt
      ) -or $AttestedAt -gt $Now -or $AttestedAt -lt $Now.AddMinutes(-30)) {
    throw "External-surface attestation is not fresh and canonically timestamped."
  }
  return [pscustomobject]@{
    Phase = $ExpectedPhase
    AttestorSessionRef = [string]$Record.attestorSessionRef
    AttestedAt = $AttestedAt
  }
}

function Resolve-ExternalSurfaceAttestationPath(
  [string]$Path,
  [string]$ExpectedPhase,
  [bool]$MustExist
) {
  $Full = [IO.Path]::GetFullPath($Path)
  $PrivateRoot = [IO.Path]::GetFullPath($PrivateParent).TrimEnd('\')
  if ([IO.Path]::GetFileName($Full) -cne "external-surface-$ExpectedPhase.json" -or
      -not [String]::Equals(
        [IO.Path]::GetDirectoryName($Full), $PrivateRoot,
        [StringComparison]::OrdinalIgnoreCase
      )) {
    throw "External-surface $ExpectedPhase attestation must use its exact leaf directly under PrivateParent."
  }
  Assert-NoReparseAncestorChain $Full "external-surface $ExpectedPhase attestation"
  Assert-OwnerPrivateDirectoryAcl $PrivateRoot "External-surface attestation parent"
  if ($MustExist) {
    $Item = [IO.FileInfo]::new($Full)
    if (-not $Item.Exists -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $Item.Length -le 0 -or $Item.Length -gt 1MB) {
      throw "External-surface $ExpectedPhase attestation is not one bounded ordinary file."
    }
  }
  elseif ([IO.File]::Exists($Full) -or [IO.Directory]::Exists($Full) -or
      [IO.File]::Exists("$Full.new") -or [IO.Directory]::Exists("$Full.new")) {
    throw "External-surface $ExpectedPhase attestation publication paths must be new."
  }
  return $Full
}

function Wait-ExternalSurfacePostflight(
  [string]$Path,
  [object]$ReleaseBinding,
  [string]$ExpectedAttestorSessionRef,
  [DateTimeOffset]$NotBefore
) {
  $Deadline = [DateTimeOffset]::UtcNow.AddMinutes(15)
  Write-Output "EXTERNAL_SURFACE_POSTFLIGHT_REQUIRED $Path"
  while ([DateTimeOffset]::UtcNow -lt $Deadline) {
    if ([IO.File]::Exists($Path)) {
      $Stable = Read-StablePrivateJson $Path "external-surface postflight attestation"
      Assert-CanonicalCompactJsonResponse $Stable "external-surface postflight attestation"
      $Facts = Assert-ExternalSurfaceAttestationInput `
        $Stable.Value $ReleaseBinding "postflight" ([DateTimeOffset]::UtcNow)
      if ($Facts.AttestorSessionRef -cne $ExpectedAttestorSessionRef -or
          $Facts.AttestedAt -lt $NotBefore) {
        throw "External-surface postflight is reordered or from a different orchestrator session."
      }
      return [pscustomobject]@{ Stable = $Stable; Facts = $Facts }
    }
    Start-Sleep -Milliseconds 100
  }
  throw "Timed out waiting for the create-once external-surface postflight attestation."
}

function Get-DurableAcceptanceLedgerDirectory {
  $LocalAppData = [Environment]::GetFolderPath(
    [Environment+SpecialFolder]::LocalApplicationData
  )
  if ([String]::IsNullOrWhiteSpace($LocalAppData) -or
      -not [IO.Path]::IsPathRooted($LocalAppData) -or
      -not [IO.Directory]::Exists($LocalAppData)) {
    throw "The fixed per-user LocalAppData root is unavailable for the acceptance ledger."
  }
  Assert-NoReparseAncestorChain $LocalAppData "acceptance-ledger parent"
  $Ledger = [IO.Path]::Combine(
    [IO.Path]::GetFullPath($LocalAppData), "LBB-Stock-Chrome-Acceptance-Ledger-v1"
  )
  if ([IO.File]::Exists($Ledger)) {
    throw "The fixed acceptance-ledger path is a file."
  }
  if (-not [IO.Directory]::Exists($Ledger)) {
    [IO.Directory]::CreateDirectory($Ledger) | Out-Null
    Set-OwnerPrivateDirectoryAcl $Ledger
  }
  Assert-NoReparseAncestorChain $Ledger "acceptance ledger"
  Assert-OwnerPrivateDirectoryAcl $Ledger "Acceptance ledger"
  foreach ($Entry in @([IO.DirectoryInfo]::new($Ledger).GetFileSystemInfos())) {
    if ($Entry -isnot [IO.FileInfo] -or
        ($Entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $Entry.Name -notmatch '^candidate-[0-9a-f]{64}\.(?:claim|outcome)\.json$') {
      throw "The durable acceptance ledger contains an unexpected or linked entry."
    }
  }
  return $Ledger
}

function New-DurableCandidateExecutionClaim(
  [string]$LedgerDirectory,
  [object]$ReleaseBinding
) {
  Assert-OwnerPrivateDirectoryAcl $LedgerDirectory "Acceptance ledger"
  $CandidateKey = Get-CanonicalObjectSha256 $ReleaseBinding
  $ClaimPath = Join-Path $LedgerDirectory "candidate-$CandidateKey.claim.json"
  $OutcomePath = Join-Path $LedgerDirectory "candidate-$CandidateKey.outcome.json"
  if ([IO.File]::Exists($ClaimPath) -or [IO.Directory]::Exists($ClaimPath) -or
      [IO.File]::Exists($OutcomePath) -or [IO.Directory]::Exists($OutcomePath)) {
    throw "This exact frozen release candidate already has a durable acceptance-attempt claim."
  }
  Write-CreateOncePrivateJson $ClaimPath ([ordered]@{
    schemaVersion = 1
    evidenceType = "stock-user-chrome-candidate-execution-claim"
    version = $Version
    candidateBindingSha256 = $CandidateKey
    sourceSha = [string]$ReleaseBinding.sourceSha
    releaseTag = [string]$ReleaseBinding.releaseTag
    workflowRef = [string]$ReleaseBinding.workflowRef
    workflowRunId = [string]$ReleaseBinding.workflowRunId
    workflowRunAttempt = [string]$ReleaseBinding.workflowRunAttempt
    artifactId = [string]$ReleaseBinding.artifactId
    artifactZipSha256 = [string]$ReleaseBinding.artifactZipSha256
    claimedAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
    coordinatorSha256 = Get-TrustedSha256 $PSCommandPath
  }) "durable candidate execution claim"
  return [pscustomobject]@{
    CandidateBindingSha256 = $CandidateKey
    ClaimPath = $ClaimPath
    OutcomePath = $OutcomePath
  }
}

function Write-DurableCandidateExecutionOutcome(
  [object]$Claim,
  [bool]$CandidateExecutionPassed,
  [bool]$CleanupIssuesObserved,
  [string]$FinalStage
) {
  Write-CreateOncePrivateJson $Claim.OutcomePath ([ordered]@{
    schemaVersion = 1
    evidenceType = "stock-user-chrome-candidate-execution-outcome"
    version = $Version
    candidateBindingSha256 = [string]$Claim.CandidateBindingSha256
    candidateExecutionPassed = $CandidateExecutionPassed
    cleanupIssuesObservedBeforeOutcome = $CleanupIssuesObserved
    finalStage = $FinalStage
    finishedAtUtc = Format-CanonicalUtc ([DateTimeOffset]::UtcNow)
  }) "durable candidate execution outcome"
}

function Copy-StablePrivateFileCreateOnce([string]$Source, [string]$Destination, [string]$Label) {
  if ([IO.File]::Exists($Destination) -or [IO.Directory]::Exists($Destination)) {
    throw "$Label destination is not new."
  }
  $SourceItem = [IO.FileInfo]::new([IO.Path]::GetFullPath($Source))
  if (-not $SourceItem.Exists -or ($SourceItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
      $SourceItem.Length -le 0 -or $SourceItem.Length -gt 20MB) {
    throw "$Label source is not an ordinary bounded file."
  }
  Assert-NoReparseAncestorChain $SourceItem.FullName "$Label source"
  $Input = [IO.File]::Open($SourceItem.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
  $Output = $null
  $Hasher = [Security.Cryptography.SHA256]::Create()
  $Buffer = New-Object byte[] 65536
  try {
    $Output = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $Total = 0L
    while (($Read = $Input.Read($Buffer, 0, $Buffer.Length)) -gt 0) {
      $Total += $Read
      if ($Total -gt 20MB) { throw "$Label exceeded its copy limit." }
      $Output.Write($Buffer, 0, $Read)
      [void]$Hasher.TransformBlock($Buffer, 0, $Read, $Buffer, 0)
    }
    [void]$Hasher.TransformFinalBlock((New-Object byte[] 0), 0, 0)
    if ($Total -ne $SourceItem.Length) { throw "$Label changed during its stable copy." }
    $Output.Flush($true)
    return [pscustomobject]@{
      name = [IO.Path]::GetFileName($Destination)
      bytes = $Total
      sha256 = ([BitConverter]::ToString($Hasher.Hash)).Replace("-", "").ToLowerInvariant()
    }
  }
  catch {
    if ($null -ne $Output) { $Output.Dispose(); $Output = $null }
    if ([IO.File]::Exists($Destination)) { [IO.File]::Delete($Destination) }
    throw
  }
  finally {
    if ($null -ne $Output) { $Output.Dispose() }
    $Input.Dispose(); $Hasher.Dispose(); [Array]::Clear($Buffer, 0, $Buffer.Length)
  }
}

function Read-StablePrivatePng([string]$Path, [string]$Label) {
  $Item = [IO.FileInfo]::new([IO.Path]::GetFullPath($Path))
  if (-not $Item.Exists -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
      $Item.Length -lt 24 -or $Item.Length -gt 20MB) {
    throw "$Label is not an ordinary bounded PNG file."
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
    if ($Stream.Position -ne $Stream.Length -or
        ([BitConverter]::ToString($Bytes, 0, 8)) -cne "89-50-4E-47-0D-0A-1A-0A" -or
        [Text.Encoding]::ASCII.GetString($Bytes, 12, 4) -cne "IHDR") {
      throw "$Label is not a canonical PNG."
    }
    $Width = ([uint32]$Bytes[16] -shl 24) -bor ([uint32]$Bytes[17] -shl 16) -bor
      ([uint32]$Bytes[18] -shl 8) -bor [uint32]$Bytes[19]
    $Height = ([uint32]$Bytes[20] -shl 24) -bor ([uint32]$Bytes[21] -shl 16) -bor
      ([uint32]$Bytes[22] -shl 8) -bor [uint32]$Bytes[23]
    if ($Width -lt 120 -or $Height -lt 32 -or $Width -gt 8192 -or $Height -gt 8192 -or
        ([uint64]$Width * [uint64]$Height) -gt 50MB) {
      throw "Reviewer exchange image dimensions are invalid."
    }
    return [pscustomobject]@{
      name = $Item.Name
      bytes = $Bytes.Length
      sha256 = Get-BytesSha256 $Bytes
      width = [int64]$Width
      height = [int64]$Height
    }
  }
  finally {
    $Stream.Dispose()
    if ($null -ne $Bytes) { [Array]::Clear($Bytes, 0, $Bytes.Length) }
  }
}

function Assert-UnchangedPrivatePng(
  [string]$Path,
  [object]$Expected,
  [string]$Label,
  [switch]$RequireBytes
) {
  $Observed = Read-StablePrivatePng $Path $Label
  if ($Observed.sha256 -cne [string]$Expected.sha256 -or
      $Observed.width -ne [int64]$Expected.width -or
      $Observed.height -ne [int64]$Expected.height -or
      ($RequireBytes -and $Observed.bytes -ne [int64]$Expected.bytes)) {
    throw "$Label changed before its digest-bound response was accepted."
  }
  return $Observed
}

function Register-ReviewExchangeArtifact([string]$Path) {
  $Full = [IO.Path]::GetFullPath($Path)
  $Prefix = [IO.Path]::GetFullPath($script:ReviewExchangeDirectory).TrimEnd('\') + '\'
  if (-not $Full.StartsWith($Prefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Reviewer exchange artifact registration escaped the exact private directory."
  }
  $Name = [IO.Path]::GetFileName($Full)
  if ($script:ReviewExchangeArtifacts.Contains($Name)) {
    throw "Reviewer exchange artifact registration was duplicated."
  }
  $script:ReviewExchangeArtifacts.Add($Name)
}

function Register-ExpectedReviewTransientArtifact([string]$Path) {
  $Full = [IO.Path]::GetFullPath($Path)
  $Prefix = [IO.Path]::GetFullPath($script:ReviewExchangeDirectory).TrimEnd('\') + '\'
  $Name = [IO.Path]::GetFileName($Full)
  if (-not $Full.StartsWith($Prefix, [StringComparison]::OrdinalIgnoreCase) -or
      $Name -notmatch '^response-[0-9a-f]{32}(?:\.claimed)?\.json(?:\.new)?$' -or
      $script:ReviewExchangeArtifacts.Contains($Name) -or
      $script:ReviewExpectedTransientArtifacts.Contains($Name)) {
    throw "Expected reviewer transient registration is invalid or duplicated."
  }
  $script:ReviewExpectedTransientArtifacts.Add($Name)
}

function Complete-ExpectedReviewTransientArtifacts([string[]]$Paths) {
  foreach ($Path in $Paths) {
    if (-not $script:ReviewExpectedTransientArtifacts.Remove(
      [IO.Path]::GetFileName([IO.Path]::GetFullPath($Path))
    )) {
      throw "An expected reviewer transient artifact was not registered."
    }
  }
}

function Remove-RegisteredReviewExchangeArtifact([string]$Path) {
  $Full = [IO.Path]::GetFullPath($Path)
  $Prefix = [IO.Path]::GetFullPath($script:ReviewExchangeDirectory).TrimEnd('\') + '\'
  if (-not $Full.StartsWith($Prefix, [StringComparison]::OrdinalIgnoreCase) -or
      -not [IO.File]::Exists($Full) -or
      ([IO.FileInfo]::new($Full).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "Reviewer exchange cleanup refused an unowned, missing, or linked artifact."
  }
  [IO.File]::Delete($Full)
  if (-not $script:ReviewExchangeArtifacts.Remove([IO.Path]::GetFileName($Full))) {
    throw "Reviewer exchange cleanup encountered an unregistered artifact."
  }
}

function Remove-ExactReviewExchangeDirectory {
  if ([String]::IsNullOrWhiteSpace([string]$script:ReviewExchangeDirectory) -or
      -not [IO.Directory]::Exists($script:ReviewExchangeDirectory)) { return }
  $Directory = [IO.DirectoryInfo]::new($script:ReviewExchangeDirectory)
  if ($Directory.Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw "Reviewer exchange cleanup refused a reparse-point directory."
  }
  $Entries = @($Directory.GetFileSystemInfos())
  $ActualNames = @($Entries | ForEach-Object { $_.Name } | Sort-Object)
  $RegisteredNames = @($script:ReviewExchangeArtifacts | Sort-Object)
  $TransientNames = @($script:ReviewExpectedTransientArtifacts | Sort-Object)
  $AllowedNames = @($RegisteredNames + $TransientNames | Sort-Object -Unique)
  $InventoryMatches = @($ActualNames | Where-Object { $AllowedNames -cnotcontains $_ }).Count -eq 0 -and
    @($RegisteredNames | Where-Object { $ActualNames -cnotcontains $_ }).Count -eq 0
  $ReservationsValid = $true
  try {
    foreach ($Held in $script:ReviewResponseReservations) {
      $Entry = $Entries | Where-Object { $_.Name -ceq $Held.Name } | Select-Object -First 1
      if ($Entry -isnot [IO.FileInfo] -or
          ($Entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
          $Entry.Length -ne 0 -or $Held.Stream.Length -ne 0) {
        $ReservationsValid = $false
      }
    }
  }
  finally {
    foreach ($Held in $script:ReviewResponseReservations) {
      try { $Held.Stream.Dispose() } catch { $ReservationsValid = $false }
    }
    $script:ReviewResponseReservations.Clear()
  }
  if (-not $InventoryMatches -or -not $ReservationsValid) {
    throw "Reviewer exchange cleanup found an unregistered, missing, extra, or replaced reservation artifact."
  }
  foreach ($Entry in $Entries) {
    if ($Entry -isnot [IO.FileInfo] -or
        ($Entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
      throw "Reviewer exchange cleanup found a non-file or linked artifact."
    }
    $Deadline = [DateTimeOffset]::UtcNow.AddSeconds(2)
    do {
      try { [IO.File]::Delete($Entry.FullName); break }
      catch [IO.IOException] {
        if ([DateTimeOffset]::UtcNow -ge $Deadline) { throw }
        Start-Sleep -Milliseconds 50
      }
    } while ($true)
  }
  if (@($Directory.GetFileSystemInfos()).Count -ne 0) {
    throw "Reviewer exchange cleanup did not reach an exact empty directory."
  }
  [IO.Directory]::Delete($Directory.FullName, $false)
  $script:ReviewExchangeArtifacts.Clear()
  $script:ReviewExpectedTransientArtifacts.Clear()
}

function Remove-KnownReviewExchangeArtifactsAfterFailure {
  if ([String]::IsNullOrWhiteSpace([string]$script:ReviewExchangeDirectory) -or
      -not [IO.Directory]::Exists($script:ReviewExchangeDirectory)) { return }
  $Directory = [IO.DirectoryInfo]::new($script:ReviewExchangeDirectory)
  if ($Directory.Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw "Failure cleanup refused a reparse-point reviewer exchange directory."
  }
  $Errors = New-Object Collections.Generic.List[string]
  foreach ($Held in $script:ReviewResponseReservations) {
    try { $Held.Stream.Dispose() }
    catch { $Errors.Add("reservation-close") }
  }
  $script:ReviewResponseReservations.Clear()
  $KnownNames = @(
    @($script:ReviewExchangeArtifacts) + @($script:ReviewExpectedTransientArtifacts) |
      Sort-Object -Unique
  )
  $Prefix = [IO.Path]::GetFullPath($Directory.FullName).TrimEnd('\') + '\'
  foreach ($Name in $KnownNames) {
    if ($Name -notmatch '^(?:request-[0-9a-f]{32}\.json|response-[0-9a-f]{32}(?:\.claimed)?\.json(?:\.new)?|browser-0[1-6]-[a-z0-9-]+(?:\.raw)?\.png)$' -or
        [IO.Path]::GetFileName([string]$Name) -cne [string]$Name) {
      $Errors.Add("noncanonical-registered-name")
      continue
    }
    $Path = [IO.Path]::GetFullPath((Join-Path $Directory.FullName $Name))
    if (-not $Path.StartsWith($Prefix, [StringComparison]::OrdinalIgnoreCase)) {
      $Errors.Add("registered-name-escaped")
      continue
    }
    if ([IO.Directory]::Exists($Path)) {
      $Errors.Add("registered-name-became-directory")
      continue
    }
    if (-not [IO.File]::Exists($Path)) { continue }
    $Item = [IO.FileInfo]::new($Path)
    if ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
      $Errors.Add("registered-name-became-link")
      continue
    }
    $Deadline = [DateTimeOffset]::UtcNow.AddSeconds(2)
    do {
      try { [IO.File]::Delete($Path); break }
      catch [IO.IOException] {
        if ([DateTimeOffset]::UtcNow -ge $Deadline) {
          $Errors.Add("registered-file-delete")
          break
        }
        Start-Sleep -Milliseconds 50
      }
    } while ($true)
  }
  $Remaining = @($Directory.GetFileSystemInfos())
  if ($Remaining.Count -eq 0 -and $Errors.Count -eq 0) {
    [IO.Directory]::Delete($Directory.FullName, $false)
    $script:ReviewExchangeArtifacts.Clear()
    $script:ReviewExpectedTransientArtifacts.Clear()
    return
  }
  throw "Failure cleanup deleted every reachable known ordinary reviewer artifact but retained an unknown or unsafe remainder."
}

function Claim-PublishedReviewerResponse(
  [string]$ResponsePath,
  [string]$ClaimedPath,
  [DateTimeOffset]$ExpiresAt
) {
  $TemporaryPath = "$ResponsePath.new"
  if ([IO.File]::Exists($ClaimedPath) -or [IO.Directory]::Exists($ClaimedPath)) {
    throw "Independent reviewer response claim path was not new."
  }
  while ([DateTimeOffset]::UtcNow -le $ExpiresAt) {
    if ([IO.Directory]::Exists($TemporaryPath) -or
        [IO.Directory]::Exists($ResponsePath) -or
        [IO.Directory]::Exists($ClaimedPath)) {
      throw "An independent reviewer response publication path became a directory."
    }
    if ([IO.File]::Exists($TemporaryPath)) {
      if ([IO.FileInfo]::new($TemporaryPath).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Independent reviewer response temporary file is a reparse point."
      }
    }
    elseif ([IO.File]::Exists($ResponsePath)) {
      if ([IO.FileInfo]::new($ResponsePath).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Independent reviewer response is a reparse point."
      }
      try {
        [IO.File]::Move($ResponsePath, $ClaimedPath)
        foreach ($ReservationPath in @($ResponsePath, $TemporaryPath)) {
          $Reservation = [IO.File]::Open(
            $ReservationPath, [IO.FileMode]::CreateNew,
            [IO.FileAccess]::ReadWrite, [IO.FileShare]::None
          )
          $Reservation.Flush($true)
          Register-ReviewExchangeArtifact $ReservationPath
          $script:ReviewResponseReservations.Add([pscustomobject]@{
            Name = [IO.Path]::GetFileName($ReservationPath)
            Stream = $Reservation
          })
        }
        Register-ReviewExchangeArtifact $ClaimedPath
        if ([DateTimeOffset]::UtcNow -gt $ExpiresAt) {
          throw "The independent reviewer response was claimed after request expiry."
        }
        return
      }
      catch [IO.IOException] {
        if ([IO.File]::Exists($ClaimedPath)) { throw }
      }
    }
    Start-Sleep -Milliseconds 100
  }
  throw "Timed out waiting for the atomically published create-once independent reviewer response."
}

function Get-InstalledStockChromeVersion {
  $Probes = @(
    [pscustomobject]@{ Hive = [Microsoft.Win32.RegistryHive]::CurrentUser; View = $RegistryView },
    [pscustomobject]@{ Hive = [Microsoft.Win32.RegistryHive]::LocalMachine; View = $RegistryView },
    [pscustomobject]@{ Hive = [Microsoft.Win32.RegistryHive]::LocalMachine; View = [Microsoft.Win32.RegistryView]::Registry32 }
  )
  foreach ($Probe in $Probes) {
    $Base = [Microsoft.Win32.RegistryKey]::OpenBaseKey($Probe.Hive, $Probe.View)
    $Key = $null
    try {
      $Key = $Base.OpenSubKey("SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe", $false)
      if ($null -eq $Key) { continue }
      $RawPath = [string]$Key.GetValue(
        "", $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
      )
      if ([String]::IsNullOrWhiteSpace($RawPath)) { continue }
      $ChromePath = [IO.Path]::GetFullPath($RawPath.Trim().Trim('"'))
      if (-not [IO.File]::Exists($ChromePath) -or
          ([IO.FileInfo]::new($ChromePath).Attributes -band [IO.FileAttributes]::ReparsePoint)) { continue }
      Assert-NoReparseAncestorChain $ChromePath "registered stock Chrome executable"
      $VersionInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo($ChromePath)
      $ObservedVersion = ([string]$VersionInfo.ProductVersion).Split(' ')[0]
      if ($VersionInfo.ProductName -cne "Google Chrome" -or
          $ObservedVersion -cnotmatch '^(1(?:4[0-9]|[5-9][0-9])|[2-9][0-9]{2})\.[0-9]{1,5}\.[0-9]{1,5}\.[0-9]{1,5}$') {
        continue
      }
      return $ObservedVersion
    }
    finally {
      if ($null -ne $Key) { $Key.Dispose() }
      $Base.Dispose()
    }
  }
  throw "A registered ordinary Google Chrome executable with a canonical supported version was not found."
}

function Assert-IndependentReviewerResponseEnvelope(
  [object]$Response,
  [string]$RequestId,
  [string]$RequestSha256,
  [string]$CandidateBindingSha256,
  [string]$InputDigestSha256,
  [string]$ExecutorSessionRef,
  [string]$ReviewerSessionRef
) {
  $ExpectedFields = @(
    "schemaVersion", "evidenceType", "requestId", "requestSha256", "candidateBindingSha256",
    "inputDigestSha256", "responderKind", "responderSessionRef", "respondedAtUtc", "decision"
  )
  if ((@($Response.PSObject.Properties.Name) -join "`n") -cne ($ExpectedFields -join "`n") -or
      $Response.schemaVersion -ne 1 -or
      $Response.evidenceType -cne "stock-user-chrome-reviewer-response" -or
      $Response.requestId -cne $RequestId -or $Response.requestSha256 -cne $RequestSha256 -or
      $Response.candidateBindingSha256 -cne $CandidateBindingSha256 -or
      $Response.inputDigestSha256 -cne $InputDigestSha256 -or
      $Response.responderKind -cne "independent-agent" -or
      [string]$Response.responderSessionRef -cne $ReviewerSessionRef -or
      $Response.responderSessionRef -ceq $ExecutorSessionRef) {
    throw "Independent reviewer response is not bound to the exact request, input, candidate, and separate session."
  }
}

function Invoke-IndependentReviewerExchange(
  [string]$Kind,
  [string]$ActionName,
  [object]$Context,
  [object]$AllowedResponse,
  [string]$Instruction
) {
  if ($Kind -notin @("screenshot-crop", "six-crop-review")) {
    throw "Reviewer exchange kind is invalid."
  }
  Assert-OwnerPrivateDirectoryAcl $script:ReviewExchangeDirectory "Reviewer exchange"
  $RequestId = [Guid]::NewGuid().ToString("N")
  $CreatedAt = [DateTimeOffset]::UtcNow
  $ExpiresAt = $CreatedAt.AddMinutes(15)
  $InputDigestSha256 = if ($Kind -ceq "screenshot-crop") {
    [string]$Context.source.sha256
  }
  else {
    Get-CanonicalObjectSha256 $Context
  }
  if ($InputDigestSha256 -cnotmatch '^[0-9a-f]{64}$') {
    throw "Reviewer request input digest is invalid."
  }
  $Request = [ordered]@{
    schemaVersion = 1
    evidenceType = "stock-user-chrome-reviewer-request"
    productVersion = $Version
    releaseCandidateBinding = $script:ReviewReleaseCandidateBinding
    candidateBinding = $script:ReviewCandidateBinding
    candidateBindingSha256 = $script:ReviewCandidateBindingSha256
    requestId = $RequestId
    sequence = $script:ReviewRequestCount + 1
    kind = $Kind
    actionName = $ActionName
    createdAtUtc = Format-CanonicalUtc $CreatedAt
    expiresAtUtc = Format-CanonicalUtc $ExpiresAt
    executorSessionRef = $script:ReviewExecutorSessionRef
    reviewerSessionRef = $script:ReviewReviewerSessionRef
    inputDigestSha256 = $InputDigestSha256
    context = $Context
    allowedResponse = $AllowedResponse
    instruction = $Instruction
  }
  $RequestPath = Join-Path $script:ReviewExchangeDirectory "request-$RequestId.json"
  $ResponsePath = Join-Path $script:ReviewExchangeDirectory "response-$RequestId.json"
  $TemporaryResponsePath = "$ResponsePath.new"
  $ClaimedPath = Join-Path $script:ReviewExchangeDirectory "response-$RequestId.claimed.json"
  foreach ($NewResponseArtifact in @($ResponsePath, $TemporaryResponsePath, $ClaimedPath)) {
    if ([IO.File]::Exists($NewResponseArtifact) -or [IO.Directory]::Exists($NewResponseArtifact)) {
      throw "Independent reviewer response publication paths were not all new."
    }
    Register-ExpectedReviewTransientArtifact $NewResponseArtifact
  }
  $RequestSha = $null
  Write-CreateOncePrivateJson $RequestPath $Request "reviewer request" ([ref]$RequestSha)
  Register-ReviewExchangeArtifact $RequestPath
  $script:ReviewRequestCount += 1
  Write-Host "REVIEWER_REQUEST $RequestPath"
  Claim-PublishedReviewerResponse $ResponsePath $ClaimedPath $ExpiresAt
  Complete-ExpectedReviewTransientArtifacts @(
    $ResponsePath, $TemporaryResponsePath, $ClaimedPath
  )
  $Stable = Read-StablePrivateJson $ClaimedPath "independent reviewer response"
  Assert-CanonicalCompactJsonResponse $Stable "independent reviewer response"
  $Response = $Stable.Value
  Assert-IndependentReviewerResponseEnvelope $Response $RequestId $RequestSha `
    $script:ReviewCandidateBindingSha256 $InputDigestSha256 `
    $script:ReviewExecutorSessionRef $script:ReviewReviewerSessionRef
  [void](Assert-FreshCanonicalResponseTimestamp $Response.respondedAtUtc `
    $CreatedAt $ExpiresAt "Independent reviewer response timestamp")
  $UnableDecision = Test-UnableReviewerDecision $Response.decision
  if ($UnableDecision) {
    # Re-hash the exact request and every selected image below before rejecting.
  }
  elseif ($Kind -ceq "screenshot-crop") {
    Assert-ReviewerCropDecision $Response.decision $Context.source
  }
  else {
    [void](Assert-ReviewerSixCropDecision $Response.decision @($Context.entries))
  }
  $StableRequest = Read-StablePrivateJson $RequestPath "reviewer request after response"
  if ($StableRequest.Sha256 -cne $RequestSha) {
    throw "The exact reviewer request changed before its response was accepted."
  }
  if ($Kind -ceq "screenshot-crop") {
    $InputPath = Join-Path $script:ReviewExchangeDirectory ([string]$Context.source.name)
    [void](Assert-UnchangedPrivatePng `
      $InputPath $Context.source "reviewer crop input after response" -RequireBytes)
  }
  else {
    if ((Get-CanonicalObjectSha256 $Context) -cne $InputDigestSha256) {
      throw "The ordered reviewer input manifest changed before its response was accepted."
    }
    foreach ($ExpectedEntry in @($Context.entries)) {
      $InputPath = Join-Path $script:ReviewExchangeDirectory ([string]$ExpectedEntry.image)
      [void](Assert-UnchangedPrivatePng `
        $InputPath $ExpectedEntry "six-crop reviewer input after response")
    }
  }
  if ($UnableDecision) {
    throw "The independent reviewer reported that the exact digest-bound input could not be interpreted safely."
  }
  Remove-RegisteredReviewExchangeArtifact $RequestPath
  Remove-RegisteredReviewExchangeArtifact $ClaimedPath
  return [pscustomobject]@{
    Decision = $Response.decision
    RequestSha256 = $RequestSha
    ResponseSha256 = $Stable.Sha256
    RespondedAtUtc = [string]$Response.respondedAtUtc
  }
}

function Test-ExactReviewerInteger([object]$Value) {
  return $Value -is [int] -or $Value -is [long]
}

function Assert-ReviewerCropDecision([object]$Decision, [object]$RawFacts) {
  $Fields = @(
    "cropX", "cropY", "cropWidth", "cropHeight",
    "requiredStateVisible", "sensitivePixelsInsideCrop", "uncertain"
  )
  if ((@($Decision.PSObject.Properties.Name) -join "`n") -cne ($Fields -join "`n") -or
      -not (Test-ExactReviewerInteger $Decision.cropX) -or
      -not (Test-ExactReviewerInteger $Decision.cropY) -or
      -not (Test-ExactReviewerInteger $Decision.cropWidth) -or
      -not (Test-ExactReviewerInteger $Decision.cropHeight) -or
      [int64]$Decision.cropX -lt 0 -or [int64]$Decision.cropY -lt 0 -or
      [int64]$Decision.cropWidth -lt 120 -or [int64]$Decision.cropHeight -lt 32 -or
      ([int64]$Decision.cropX + [int64]$Decision.cropWidth) -gt [int64]$RawFacts.width -or
      ([int64]$Decision.cropY + [int64]$Decision.cropHeight) -gt [int64]$RawFacts.height -or
      $Decision.requiredStateVisible -isnot [bool] -or
      $Decision.sensitivePixelsInsideCrop -isnot [bool] -or
      $Decision.uncertain -isnot [bool] -or
      $Decision.requiredStateVisible -ne $true -or
      $Decision.sensitivePixelsInsideCrop -ne $false -or $Decision.uncertain -ne $false) {
    throw "Independent reviewer returned an invalid, sensitive, uncertain, or out-of-bounds crop."
  }
}

function Assert-ReviewerSixCropDecision([object]$Decision, [object[]]$ExpectedEntries) {
  if ((@($Decision.PSObject.Properties.Name) -join "`n") -cne "entries`naggregate" -or
      @($Decision.entries).Count -ne 6 -or $ExpectedEntries.Count -ne 6) {
    throw "Independent six-crop review response shape is invalid."
  }
  $Bound = @()
  for ($Index = 0; $Index -lt 6; $Index += 1) {
    $Expected = $ExpectedEntries[$Index]
    $Actual = $Decision.entries[$Index]
    $EntryFields = @(
      "sequence", "purpose", "image", "sha256", "width", "height",
      "requiredVisibleStateSha256", "digestMatched", "requiredStateVerdict",
      "sensitivePixelsObserved", "uncertain"
    )
    if ((@($Actual.PSObject.Properties.Name) -join "`n") -cne ($EntryFields -join "`n") -or
        -not (Test-ExactReviewerInteger $Actual.sequence) -or
        -not (Test-ExactReviewerInteger $Actual.width) -or
        -not (Test-ExactReviewerInteger $Actual.height) -or
        [int64]$Actual.sequence -ne ($Index + 1) -or $Actual.purpose -cne $Expected.purpose -or
        $Actual.image -cne $Expected.image -or $Actual.sha256 -cne $Expected.sha256 -or
        [int64]$Actual.width -ne [int64]$Expected.width -or
        [int64]$Actual.height -ne [int64]$Expected.height -or
        $Actual.requiredVisibleStateSha256 -cne $Expected.requiredVisibleStateSha256 -or
        $Actual.digestMatched -isnot [bool] -or
        $Actual.sensitivePixelsObserved -isnot [bool] -or $Actual.uncertain -isnot [bool] -or
        $Actual.digestMatched -ne $true -or $Actual.requiredStateVerdict -cne "pass" -or
        $Actual.sensitivePixelsObserved -ne $false -or $Actual.uncertain -ne $false) {
      throw "Independent review contains a mismatched, reordered, failed, sensitive, or uncertain entry."
    }
    $Bound += [ordered]@{
      sequence = [int]$Actual.sequence; purpose = [string]$Actual.purpose; image = [string]$Actual.image
      sha256 = [string]$Actual.sha256; width = [int64]$Actual.width; height = [int64]$Actual.height
      requiredVisibleStateSha256 = [string]$Actual.requiredVisibleStateSha256
      digestMatched = $true; requiredStateVerdict = "pass"
      sensitivePixelsObserved = $false; uncertain = $false
    }
  }
  $AggregateFields = @(
    "reviewedCropCount", "everySanitizedCropOpenedByReviewer", "allImageDigestsMatched",
    "requiredVisibleStateConfirmedByReviewer", "noSensitivePixelsObservedByReviewer",
    "noUncertaintyReported", "visualJudgmentNotPixelSafetyProof"
  )
  if ((@($Decision.aggregate.PSObject.Properties.Name) -join "`n") -cne ($AggregateFields -join "`n") -or
      -not (Test-ExactReviewerInteger $Decision.aggregate.reviewedCropCount) -or
      [int64]$Decision.aggregate.reviewedCropCount -ne 6 -or
      @($AggregateFields[1..6] | Where-Object {
        $Decision.aggregate.$_ -isnot [bool] -or $Decision.aggregate.$_ -ne $true
      }).Count -ne 0) {
    throw "Independent review aggregate did not fail closed."
  }
  return $Bound
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

function New-FailureCleanupDisclosure(
  [string]$PartialEvidenceDirectory,
  [string]$RawDirectory,
  [string]$ReviewDirectory,
  [string]$OperatorDirectory,
  [bool]$CleanupIssueObserved
) {
  $PartialEvidenceDeleted = [String]::IsNullOrWhiteSpace($PartialEvidenceDirectory) -or
    -not [IO.Directory]::Exists($PartialEvidenceDirectory)
  $RawDeleted = [String]::IsNullOrWhiteSpace($RawDirectory) -or
    -not [IO.Directory]::Exists($RawDirectory)
  $ReviewDeleted = [String]::IsNullOrWhiteSpace($ReviewDirectory) -or
    -not [IO.Directory]::Exists($ReviewDirectory)
  $OperatorDeleted = [String]::IsNullOrWhiteSpace($OperatorDirectory) -or
    -not [IO.Directory]::Exists($OperatorDirectory)
  $SensitiveDisposition = if ($PartialEvidenceDeleted -and $RawDeleted -and
      $ReviewDeleted -and $OperatorDeleted -and -not $CleanupIssueObserved) {
    "deleted"
  } else { "unknown" }
  return [ordered]@{
    tokenOrCredentialValuesWrittenToAttemptJson = $false
    partialEvidenceDirectoryDeleted = $PartialEvidenceDeleted
    rawScreenshotScratchDeleted = $RawDeleted
    reviewExchangeDeleted = $ReviewDeleted
    operatorExchangeDeleted = $OperatorDeleted
    sensitiveScratchDisposition = $SensitiveDisposition
    wrongTargetMutationDisposition = "unknown"
  }
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

function Read-GitHubAcceptanceToken([string]$PipeName) {
  if ([String]::IsNullOrWhiteSpace($PipeName)) {
    return Read-Host "Independent least-privilege GitHub acceptance token" -AsSecureString
  }
  if ($PipeName -cnotmatch '^lbb-gh-[0-9a-f]{32}$') {
    throw "The GitHub credential pipe name is not canonical."
  }
  $Pipe = [IO.Pipes.NamedPipeClientStream]::new(
    ".", $PipeName, [IO.Pipes.PipeDirection]::In, [IO.Pipes.PipeOptions]::Asynchronous
  )
  $Secure = [Security.SecureString]::new()
  $Completed = $false
  $OneByte = New-Object byte[] 1
  $Deadline = [DateTimeOffset]::UtcNow.AddSeconds(30)
  try {
    $Pipe.Connect(30000)
    for ($Count = 0; $Count -le 4096; $Count += 1) {
      $Remaining = $Deadline - [DateTimeOffset]::UtcNow
      if ($Remaining.TotalMilliseconds -le 0) {
        throw "The GitHub credential pipe exceeded its absolute thirty-second deadline."
      }
      $ReadTask = $Pipe.ReadAsync($OneByte, 0, 1)
      if (-not $ReadTask.Wait(
          [Math]::Max(1, [Math]::Min(30000, [int]$Remaining.TotalMilliseconds)))) {
        throw "The GitHub credential pipe stalled before its line terminator."
      }
      $ReadCount = $ReadTask.GetAwaiter().GetResult()
      if ($ReadCount -ne 1) {
        throw "The GitHub credential pipe ended before its line terminator."
      }
      $Byte = [int]$OneByte[0]
      if ($Byte -eq 10) {
        if ($Count -lt 16) { throw "The GitHub credential supplied through the pipe is too short." }
        $Completed = $true
        break
      }
      if ($Byte -lt 33 -or $Byte -gt 126) {
        throw "The GitHub credential pipe must contain one bounded printable-ASCII line."
      }
      $Secure.AppendChar([char]$Byte)
    }
    if (-not $Completed) {
      throw "The GitHub credential pipe ended or exceeded its bound before the line terminator."
    }
    $Secure.MakeReadOnly()
    return $Secure
  }
  catch {
    $Secure.Dispose()
    throw
  }
  finally { [Array]::Clear($OneByte, 0, $OneByte.Length); $Pipe.Dispose() }
}

$PrimaryFailure = $null
$CleanupErrors = New-Object Collections.Generic.List[string]
$EvidenceDirectory = $null
$RawScreenshotDirectory = $null
$ExtensionDirectory = $null
$OperatorExchangeDirectory = $null
$ReviewExchangeDirectory = $null
$AttemptDirectory = $null
$SecureGhToken = $null
$StableCandidateBinding = $null
$StableExternalSurfacePreflightAttestation = $null
$StableExternalSurfacePostflightAttestation = $null
$ExternalSurfaceAttestorSessionRef = $null
$ExactReleaseCandidateBinding = $null
$DurableCandidateClaim = $null
$Stage = "initialize"
try {
$Repository = New-ShortSourceDirectory
$EvidenceDirectory = New-PrivateEmptyDirectory "lbb-evidence-"
$RawScreenshotDirectory = New-PrivateEmptyDirectory "lbb-raw-"
$ExtensionDirectory = New-PrivateEmptyDirectory "lbb-extension-"
$OperatorExchangeDirectory = New-PrivateEmptyDirectory "lbb-operator-exchange-"
$ReviewExchangeDirectory = New-PrivateEmptyDirectory "lbb-review-exchange-"
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
  $Binding = ConvertFrom-JsonPreservingStrings `
    ([Text.UTF8Encoding]::new($false, $true).GetString($BindingBytes))
}
finally { [Array]::Clear($BindingBytes, 0, $BindingBytes.Length) }
$BindingProperties = @($Binding.PSObject.Properties.Name)
$ExpectedBindingProperties = @(
  "schemaVersion", "version", "releaseTag", "repository", "sourceSha",
  "workflowRunId", "workflowRunAttempt", "workflowEvent", "workflowRef", "workflowPath", "artifactId",
  "artifactName", "artifactZipBytes", "artifactZipSha256",
  "checksumManifestSha256", "attestationInvocationUri", "attestedAssetCount",
  "githubHostedRunner", "assets", "passed"
)
if (($BindingProperties -join "`n") -cne ($ExpectedBindingProperties -join "`n") -or
    $Binding.schemaVersion -ne 3 -or $Binding.version -cne $Version -or
    $Binding.releaseTag -cne "v$Version" -or
    $Binding.repository -cne "flrngel/local-browser-bridge" -or
    $Binding.sourceSha -cne $FinalSha -or
    [string]$Binding.workflowRunId -cne $WorkflowRunId -or
    [string]$Binding.workflowRunAttempt -cne $WorkflowRunAttempt -or
    $Binding.workflowEvent -cne "workflow_dispatch" -or
    $Binding.workflowRef -cne "refs/heads/main" -or
    $Binding.workflowPath -cne ".github/workflows/deploy.yml" -or
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
  $StableBinding = ConvertFrom-JsonPreservingStrings `
    ([Text.UTF8Encoding]::new($false, $true).GetString($StableBindingBytes))
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
  schemaVersion = 3
  version = [string]$StableBinding.version
  releaseTag = [string]$StableBinding.releaseTag
  repository = [string]$StableBinding.repository
  sourceSha = [string]$StableBinding.sourceSha
  workflowRunId = [string]$StableBinding.workflowRunId
  workflowRunAttempt = [string]$StableBinding.workflowRunAttempt
  workflowEvent = [string]$StableBinding.workflowEvent
  workflowRef = [string]$StableBinding.workflowRef
  workflowPath = [string]$StableBinding.workflowPath
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
$ExternalSurfacePreflightPath = Resolve-ExternalSurfaceAttestationPath `
  $ExternalSurfacePreflightAttestation "preflight" $true
$ExternalSurfacePostflightPath = Resolve-ExternalSurfaceAttestationPath `
  $ExternalSurfacePostflightAttestation "postflight" $false
$ExternalSurfacePreflightRead = Read-StablePrivateJson `
  $ExternalSurfacePreflightPath "external-surface preflight attestation"
Assert-CanonicalCompactJsonResponse `
  $ExternalSurfacePreflightRead "external-surface preflight attestation"
$ExternalSurfacePreflightFacts = Assert-ExternalSurfaceAttestationInput `
  $ExternalSurfacePreflightRead.Value $ExactReleaseCandidateBinding `
  "preflight" ([DateTimeOffset]::UtcNow)
$ExternalSurfaceAttestorSessionRef = $ExternalSurfacePreflightFacts.AttestorSessionRef
$StableExternalSurfacePreflightAttestation = Join-Path `
  $EvidenceDirectory "external-surface-preflight.json"
$StableExternalSurfacePreflightCopy = Copy-StablePrivateFileCreateOnce `
  $ExternalSurfacePreflightPath $StableExternalSurfacePreflightAttestation `
  "external-surface preflight evidence copy"
if ($StableExternalSurfacePreflightCopy.sha256 -cne $ExternalSurfacePreflightRead.Sha256) {
  throw "Retained external-surface preflight differs from its exact stable input."
}
$ExternalSurfacePreflightRead = $null
$ExternalSurfacePreflightFacts = $null
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
$SecureGhToken = Read-GitHubAcceptanceToken $GitHubTokenPipeName

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
    $Info.Arguments = "attestation verify $Name --hostname github.com --repo flrngel/local-browser-bridge --signer-workflow flrngel/local-browser-bridge/.github/workflows/deploy.yml --source-ref refs/heads/main --source-digest $FinalSha --deny-self-hosted-runners --format json"
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
    $TrimmedOutput = $Output.Trim()
    if (-not $TrimmedOutput.StartsWith("[", [StringComparison]::Ordinal) -or
        -not $TrimmedOutput.EndsWith("]", [StringComparison]::Ordinal)) {
      throw "GitHub attestation verification did not return a JSON array."
    }
    $Attestations = @($Output | ConvertFrom-JsonPreservingStrings)
    $ExpectedAssetSha = Get-TrustedSha256 (Join-Path $Candidate $Name)
    Assert-ExactAttemptAttestationSet `
      -Attestations $Attestations `
      -ExpectedInvocationUri $ExpectedInvocationUri `
      -WorkflowRunId $WorkflowRunId `
      -WorkflowPath ".github/workflows/deploy.yml" `
      -Repository "flrngel/local-browser-bridge" `
      -TagRef "refs/heads/main" `
      -SourceSha $FinalSha `
      -SubjectName $Name `
      -SubjectSha256 $ExpectedAssetSha
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
& $TrustedGit @GitCommon -C $Repository fetch --force origin $FinalSha
if ($LASTEXITCODE -ne 0) { throw "Fresh clone could not fetch the exact source commit." }
& $TrustedGit @GitCommon -C $Repository checkout --detach --force $FinalSha
if ($LASTEXITCODE -ne 0) { throw "Exact detached checkout failed." }

$ObservedHead = (& $TrustedGit @GitCommon -C $Repository rev-parse --verify HEAD).Trim()
$SymbolicHead = @(& $TrustedGit @GitCommon -C $Repository symbolic-ref -q HEAD 2>$null)
$SymbolicHeadExit = $LASTEXITCODE
$Dirty = @(& $TrustedGit @GitCommon -C $Repository status --porcelain=v2 --untracked-files=all)
$Deleted = @(& $TrustedGit @GitCommon -C $Repository ls-files --deleted)
$Others = @(& $TrustedGit @GitCommon -C $Repository ls-files --others --exclude-standard)
$Ignored = @(& $TrustedGit @GitCommon -C $Repository ls-files --others --ignored --exclude-standard)
if ($ObservedHead -cne $FinalSha) { throw "Repository HEAD does not equal FINAL_SHA." }
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
  "scripts/write-stock-chrome-operator-response.ps1",
  "evidence/v0.12.49/browser/operator-results.template.json",
  "evidence/v0.12.49/browser/operator-results.schema.json",
  "evidence/v0.12.49/browser/computer-helper-chain.schema.json",
  "evidence/v0.12.49/browser/scoped-action-approval.schema.json",
  "evidence/v0.12.49/browser/independent-visual-review.schema.json",
  "evidence/v0.12.49/browser/external-surface-attestation.schema.json"
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
  [pscustomobject]@{ Relative = "scripts/write-stock-chrome-operator-response.ps1"; Arguments = @("-Mode", "SelfTest") }
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

$Stage = "durable-candidate-execution-claim"
$DurableLedger = Get-DurableAcceptanceLedgerDirectory
$DurableCandidateClaim = New-DurableCandidateExecutionClaim `
  $DurableLedger $ExactReleaseCandidateBinding
$Stage = "computer-helper-chain"
$ExecutorSessionRef = New-OpaqueSessionRef
& "$Scripts\record-computer-helper-chain.ps1" -Mode Run `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -ApiMatrixRunner "$Scripts\test-windows-browser-api.ps1" `
  -ApiMatrixRecord (Join-Path $EvidenceDirectory "browser-api-matrix.json") `
  -ServerExecutable (Join-Path $Candidate "local-browser-bridge-v$Version-windows-x86_64.exe") `
  -HelperExecutable (Join-Path $Candidate "local-computer-helper-v$Version-windows-x86_64.exe") `
  -ExtensionDirectory $ExtensionDirectory `
  -RawScreenshotDirectory $RawScreenshotDirectory `
  -OperatorExchangeDirectory $OperatorExchangeDirectory `
  -ScopedApprovalRecord (Join-Path $EvidenceDirectory "scoped-action-approval.json") `
  -ExecutorSessionRef $ExecutorSessionRef `
  -ExpectedOrchestratorSessionRef $ExternalSurfaceAttestorSessionRef `
  -OutputRecord (Join-Path $EvidenceDirectory "browser-computer-helper-chain.json")

$HelperPath = Join-Path $EvidenceDirectory "browser-computer-helper-chain.json"
$StableHelper = Read-StablePrivateJson $HelperPath "computer-helper chain"
$HelperRecord = $StableHelper.Value
if ($HelperRecord.schemaVersion -ne 2 -or $HelperRecord.passed -ne $true -or
    $HelperRecord.operatorExchange.executorSessionRef -cne $ExecutorSessionRef -or
    $HelperRecord.operatorExchange.reviewerSessionRef -cnotmatch '^[0-9a-f]{64}$' -or
    $HelperRecord.operatorExchange.reviewerSessionRef -ceq $ExecutorSessionRef -or
    $HelperRecord.operatorExchange.independentSessionBoundary -ne $true -or
    $HelperRecord.operatorExchange.scratchDeleted -ne $true) {
  throw "The helper record did not establish a separate, completed reviewer session."
}
$ScopedApprovalPath = Join-Path $EvidenceDirectory "scoped-action-approval.json"
$StableScopedApproval = Read-StablePrivateJson $ScopedApprovalPath "scoped action approval"
if ($StableScopedApproval.Value.response.orchestratorSessionRef -cne `
    $ExternalSurfaceAttestorSessionRef) {
  throw "The scoped approval was not delivered through the preflight attestor session."
}
$script:ReviewExchangeDirectory = $ReviewExchangeDirectory
$script:ReviewExchangeArtifacts = New-Object Collections.Generic.List[string]
$script:ReviewResponseReservations = New-Object Collections.Generic.List[object]
$script:ReviewExpectedTransientArtifacts = New-Object Collections.Generic.List[string]
$script:ReviewReleaseCandidateBinding = $HelperRecord.releaseCandidateBinding
$script:ReviewCandidateBinding = $HelperRecord.candidateBinding
$CandidateBindingText = $script:ReviewCandidateBinding | ConvertTo-Json -Depth 12 -Compress
$CandidateBindingBytes = [Text.UTF8Encoding]::new($false, $true).GetBytes($CandidateBindingText)
$CandidateBindingHasher = [Security.Cryptography.SHA256]::Create()
try {
  $script:ReviewCandidateBindingSha256 = (
    [BitConverter]::ToString($CandidateBindingHasher.ComputeHash($CandidateBindingBytes))
  ).Replace("-", "").ToLowerInvariant()
}
finally {
  $CandidateBindingHasher.Dispose()
  [Array]::Clear($CandidateBindingBytes, 0, $CandidateBindingBytes.Length)
}
$script:ReviewExecutorSessionRef = $ExecutorSessionRef
$script:ReviewReviewerSessionRef = [string]$HelperRecord.operatorExchange.reviewerSessionRef
$script:ReviewRequestCount = 0

$Captures = [ordered]@{
  "extension-loaded" = "browser-01-extension-loaded"
  "api-action-result" = "browser-02-api-action-result"
  "computer-share-action" = "browser-03-computer-share-action"
  "stop-paused" = "browser-04-stop-paused"
  "cancel-paused" = "browser-05-cancel-paused"
  "post-handback-resume" = "browser-06-post-handback-resume"
}
$RequiredVisibleStates = [ordered]@{
  "extension-loaded" = "stock Chrome chrome://extensions shows exactly one enabled unpacked Local Browser Bridge v0.12.49 card with no load errors and Chrome's debugger-use indicator during the active bridge lease"
  "api-action-result" = "the loopback demo visibly shows Hello, Bridge Matrix. blue selected. after the browser API action"
  "computer-share-action" = "the exact shared Chrome window visibly shows the post-click demo state and synthetic session pointer from a fresh helper frame"
  "stop-paused" = "the trusted extension popup visibly shows the human pause and Resume remote control after the in-page Stop handback"
  "cancel-paused" = "the trusted extension popup visibly shows the human pause and Resume remote control after Chrome's browser-owned Cancel handback"
  "post-handback-resume" = "the exact demo visibly shows the restored Chrome debugger-use indicator and page control pill after both trusted-popup recovery cycles"
}
$Stage = "independent-screenshot-cropping"
foreach ($Entry in $Captures.GetEnumerator()) {
  $RawPath = Join-Path $RawScreenshotDirectory ($Entry.Value + ".raw.png")
  $ReviewRawPath = Join-Path $ReviewExchangeDirectory ($Entry.Value + ".raw.png")
  $RawCopy = Copy-StablePrivateFileCreateOnce $RawPath $ReviewRawPath "raw screenshot review copy"
  Register-ReviewExchangeArtifact $ReviewRawPath
  $RawFacts = Read-StablePrivatePng $ReviewRawPath "raw screenshot review copy"
  if ($RawFacts.sha256 -cne $RawCopy.sha256 -or $RawFacts.bytes -ne $RawCopy.bytes) {
    throw "Raw screenshot review copy changed after its create-once stable copy."
  }
  $CropExchange = Invoke-IndependentReviewerExchange `
    "screenshot-crop" "crop-$($Entry.Key)" `
    ([ordered]@{
      purpose = $Entry.Key
      source = [ordered]@{
        name = $RawCopy.name; bytes = $RawCopy.bytes; sha256 = $RawCopy.sha256
        width = $RawFacts.width; height = $RawFacts.height
      }
      requiredVisibleState = $RequiredVisibleStates[$Entry.Key]
    }) `
    ([ordered]@{
      type = "tight-crop"
      minimumWidth = 120; minimumHeight = 32
      maximumWidth = $RawFacts.width; maximumHeight = $RawFacts.height
      requireVisibleState = $true; sensitivePixelsInsideCrop = $false; uncertain = $false
    }) `
    "Open the exact digest-bound raw screenshot, choose the tightest crop proving only the required visible state, and fail closed on uncertainty or sensitive pixels inside that crop."
  $CropDecision = $CropExchange.Decision
  Assert-ReviewerCropDecision $CropDecision $RawFacts
  Remove-RegisteredReviewExchangeArtifact $ReviewRawPath
  & "$Scripts\sanitize-browser-evidence-screenshot.ps1" -Mode Sanitize `
    -InputImage $RawPath `
    -OutputImage (Join-Path $EvidenceDirectory ($Entry.Value + ".png")) `
    -OutputRecord (Join-Path $RawScreenshotDirectory ($Entry.Value + ".pending.json")) `
    -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
    -Purpose $Entry.Key -CropX ([int]$CropDecision.cropX) -CropY ([int]$CropDecision.cropY) `
    -CropWidth ([int]$CropDecision.cropWidth) -CropHeight ([int]$CropDecision.cropHeight)
}

$Stage = "independent-six-crop-review"
$ReviewRequestEntries = @()
$Sequence = 0
foreach ($Entry in $Captures.GetEnumerator()) {
  $Sequence += 1
  $ReviewedImage = Join-Path $EvidenceDirectory ($Entry.Value + ".png")
  $ReviewCopyPath = Join-Path $ReviewExchangeDirectory ($Entry.Value + ".png")
  $ReviewCopy = Copy-StablePrivateFileCreateOnce $ReviewedImage $ReviewCopyPath "sanitized crop review copy"
  Register-ReviewExchangeArtifact $ReviewCopyPath
  $ReviewFacts = Read-StablePrivatePng $ReviewCopyPath "sanitized crop review copy"
  if ($ReviewFacts.sha256 -cne $ReviewCopy.sha256 -or $ReviewFacts.bytes -ne $ReviewCopy.bytes) {
    throw "Sanitized crop review copy changed after its create-once stable copy."
  }
  $CriterionBytes = [Text.UTF8Encoding]::new($false, $true).GetBytes($RequiredVisibleStates[$Entry.Key])
  $CriterionHasher = [Security.Cryptography.SHA256]::Create()
  try {
    $CriterionSha = ([BitConverter]::ToString($CriterionHasher.ComputeHash($CriterionBytes))).Replace("-", "").ToLowerInvariant()
  }
  finally { $CriterionHasher.Dispose(); [Array]::Clear($CriterionBytes, 0, $CriterionBytes.Length) }
  $ReviewRequestEntries += [ordered]@{
    sequence = $Sequence; purpose = $Entry.Key; image = $ReviewCopy.name
    sha256 = $ReviewCopy.sha256; width = $ReviewFacts.width; height = $ReviewFacts.height
    requiredVisibleState = $RequiredVisibleStates[$Entry.Key]
    requiredVisibleStateSha256 = $CriterionSha
  }
}
$ReviewExchange = Invoke-IndependentReviewerExchange `
  "six-crop-review" "review-six-sanitized-crops" `
  ([ordered]@{ entries = $ReviewRequestEntries }) `
  ([ordered]@{
    type = "ordered-six-crop-review"; everyDigestMustMatch = $true
    requiredStateVerdict = "pass"; sensitivePixelsObserved = $false; uncertain = $false
  }) `
  "Open all six exact digest-bound sanitized crops in order. Confirm each required visible state, report any sensitive pixel or uncertainty, and do not infer pixel safety from automation."
$ReviewDecision = $ReviewExchange.Decision
$BoundReviewEntries = @(Assert-ReviewerSixCropDecision $ReviewDecision $ReviewRequestEntries)
$IndependentReviewPath = Join-Path $EvidenceDirectory "independent-visual-review.json"
Write-CreateOncePrivateJson $IndependentReviewPath ([ordered]@{
  schemaVersion = 1
  evidenceType = "stock-user-chrome-independent-visual-review"
  releaseCandidateBinding = $script:ReviewReleaseCandidateBinding
  candidateBinding = $script:ReviewCandidateBinding
  executorSessionRef = $script:ReviewExecutorSessionRef
  reviewerSessionRef = $script:ReviewReviewerSessionRef
  independentSessionBoundary = $true
  requestSha256 = $ReviewExchange.RequestSha256
  reviewedAtUtc = $ReviewExchange.RespondedAtUtc
  entries = $BoundReviewEntries
  aggregate = [ordered]@{
    reviewedCropCount = 6; everySanitizedCropOpenedByReviewer = $true
    allImageDigestsMatched = $true; requiredVisibleStateConfirmedByReviewer = $true
    noSensitivePixelsObservedByReviewer = $true; noUncertaintyReported = $true
    visualJudgmentNotPixelSafetyProof = $true
  }
}) "independent visual review record"
Remove-ExactReviewExchangeDirectory

$Stage = "external-surface-postflight"
$ReviewCompletedAt = [DateTimeOffset]::MinValue
if (-not [DateTimeOffset]::TryParseExact(
    [string]$ReviewExchange.RespondedAtUtc, "o", [Globalization.CultureInfo]::InvariantCulture,
    [Globalization.DateTimeStyles]::RoundtripKind, [ref]$ReviewCompletedAt
  )) {
  throw "Independent-review completion time is invalid before external postflight."
}
$ExternalSurfacePostflight = Wait-ExternalSurfacePostflight `
  $ExternalSurfacePostflightPath $ExactReleaseCandidateBinding `
  $ExternalSurfaceAttestorSessionRef $ReviewCompletedAt
$StableExternalSurfacePostflightAttestation = Join-Path `
  $EvidenceDirectory "external-surface-postflight.json"
$StableExternalSurfacePostflightCopy = Copy-StablePrivateFileCreateOnce `
  $ExternalSurfacePostflightPath $StableExternalSurfacePostflightAttestation `
  "external-surface postflight evidence copy"
if ($StableExternalSurfacePostflightCopy.sha256 -cne `
    $ExternalSurfacePostflight.Stable.Sha256) {
  throw "Retained external-surface postflight differs from its exact stable input."
}
$ExternalSurfacePostflight = $null

$Stage = "bind-independent-review"
foreach ($Entry in $Captures.GetEnumerator()) {
  $ReviewedImage = Join-Path $EvidenceDirectory ($Entry.Value + ".png")
  & "$Scripts\sanitize-browser-evidence-screenshot.ps1" -Mode BindReview `
    -PendingRecord (Join-Path $RawScreenshotDirectory ($Entry.Value + ".pending.json")) `
    -ReviewedImage $ReviewedImage `
    -OutputRecord (Join-Path $EvidenceDirectory ($Entry.Value + ".json")) `
    -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
    -IndependentReviewRecord $IndependentReviewPath
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
$BrowserVersion = Get-InstalledStockChromeVersion
$Stage = "build-operator-record"
& "$Scripts\write-browser-evidence-record.ps1" -Mode BuildOperator `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -ComputerHelperRecord (Join-Path $EvidenceDirectory "browser-computer-helper-chain.json") `
  -ScopedApprovalRecord (Join-Path $EvidenceDirectory "scoped-action-approval.json") `
  -IndependentReviewRecord $IndependentReviewPath `
  -ExternalSurfacePreflightAttestation $StableExternalSurfacePreflightAttestation `
  -ExternalSurfacePostflightAttestation $StableExternalSurfacePostflightAttestation `
  -BrowserVersion $BrowserVersion `
  -OutputRecord $OperatorPath

$Sidecars = @($Captures.Values | ForEach-Object {
  Join-Path $EvidenceDirectory ($_ + ".json")
})
$Stage = "finalize"
& "$Scripts\write-browser-evidence-record.ps1" -Mode Finalize `
  -PreflightRecord (Join-Path $EvidenceDirectory "candidate-preflight.json") `
  -PostflightRecord (Join-Path $EvidenceDirectory "candidate-postflight.json") `
  -ApiMatrixRecord (Join-Path $EvidenceDirectory "browser-api-matrix.json") `
  -ComputerHelperRecord (Join-Path $EvidenceDirectory "browser-computer-helper-chain.json") `
  -ScopedApprovalRecord (Join-Path $EvidenceDirectory "scoped-action-approval.json") `
  -IndependentReviewRecord $IndependentReviewPath `
  -ExternalSurfacePreflightAttestation $StableExternalSurfacePreflightAttestation `
  -ExternalSurfacePostflightAttestation $StableExternalSurfacePostflightAttestation `
  -OperatorResults (Join-Path $EvidenceDirectory "operator-results.json") `
  -ScreenshotRecords $Sidecars `
  -OutputRecord (Join-Path $EvidenceDirectory "browser-acceptance.json")

$FinalEntries = @([IO.DirectoryInfo]::new($EvidenceDirectory).GetFileSystemInfos())
if ($FinalEntries.Count -ne 22 -or
    @($FinalEntries | Where-Object {
      $_ -isnot [IO.FileInfo] -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)
    }).Count -ne 0) {
  throw "The finalized evidence directory is not the exact twenty-two-file ordinary inventory."
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
    try {
      if ($Owned -ceq $ReviewExchangeDirectory -and
          [IO.Directory]::Exists($ReviewExchangeDirectory)) {
        try { Remove-ExactReviewExchangeDirectory }
        catch {
          $CleanupErrors.Add("review-exchange-strict: $($_.Exception.Message)")
          Remove-KnownReviewExchangeArtifactsAfterFailure
        }
      }
      else { Remove-TestOwnedTree $Owned }
    }
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
  if ($null -ne $DurableCandidateClaim) {
    try {
      Write-DurableCandidateExecutionOutcome `
        $DurableCandidateClaim `
        ($null -eq $PrimaryFailure -and $Stage -ceq "completed") `
        ($CleanupErrors.Count -ne 0) `
        $Stage
    }
    catch { $CleanupErrors.Add("durable-candidate-outcome: $($_.Exception.Message)") }
  }
  $FailedAttempt = $null -ne $PrimaryFailure -or $CleanupErrors.Count -ne 0
  if ($FailedAttempt -and -not [String]::IsNullOrWhiteSpace($EvidenceDirectory) -and
      [IO.Directory]::Exists($EvidenceDirectory)) {
    try { Remove-TestOwnedTree $EvidenceDirectory }
    catch { $CleanupErrors.Add("partial-evidence invalidation: $($_.Exception.Message)") }
  }
  if ($FailedAttempt -and -not [String]::IsNullOrWhiteSpace($AttemptDirectory) -and
      [IO.Directory]::Exists($AttemptDirectory)) {
    try {
      $CleanupDisclosure = New-FailureCleanupDisclosure `
        -PartialEvidenceDirectory $EvidenceDirectory `
        -RawDirectory $RawScreenshotDirectory `
        -ReviewDirectory $ReviewExchangeDirectory `
        -OperatorDirectory $OperatorExchangeDirectory `
        -CleanupIssueObserved ($CleanupErrors.Count -ne 0)
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
        tokenOrCredentialValuesWrittenToAttemptJson = `
          $CleanupDisclosure.tokenOrCredentialValuesWrittenToAttemptJson
        partialEvidenceDirectoryDeleted = `
          $CleanupDisclosure.partialEvidenceDirectoryDeleted
        rawScreenshotScratchDeleted = $CleanupDisclosure.rawScreenshotScratchDeleted
        reviewExchangeDeleted = $CleanupDisclosure.reviewExchangeDeleted
        operatorExchangeDeleted = $CleanupDisclosure.operatorExchangeDeleted
        sensitiveScratchDisposition = $CleanupDisclosure.sensitiveScratchDisposition
        wrongTargetMutationDisposition = $CleanupDisclosure.wrongTargetMutationDisposition
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
  if (-not [String]::IsNullOrWhiteSpace($EvidenceDirectory) -and
      [IO.Directory]::Exists($EvidenceDirectory)) {
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
