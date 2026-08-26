#!/usr/bin/env bash

set -euo pipefail
export LC_ALL=C

readonly SCRIPT_NAME="$(basename "$0")"
readonly API_VERSION="2026-03-10"
readonly RECEIPT_SCHEMA_VERSION="3"
readonly EVIDENCE_PRODUCT_VERSION="0.12.33"
readonly EVIDENCE_MAX_BLOB_BYTES=20971520
readonly EVIDENCE_MAX_TOTAL_BYTES=209715200
readonly EVIDENCE_MAX_PATH_BYTES=1024
readonly EVIDENCE_MAX_PATH_COMPONENT_BYTES=255
readonly EVIDENCE_MAX_DIFF_STATUS_BYTES=8
readonly EVIDENCE_MAX_TREE_RECORD_BYTES=1077

EXPECTED_RELEASE_CANDIDATE_BINDING=""
MACOS_CANDIDATE_FACTS=""

die() {
  printf '%s: %s\n' "$SCRIPT_NAME" "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command is unavailable: $1"
}

sha256_file() {
  sha256sum "$1" | awk '{ print $1 }'
}

is_sha1() {
  [[ "$1" =~ ^[0-9a-f]{40}$ ]]
}

is_sha256() {
  [[ "$1" =~ ^[0-9a-f]{64}$ ]]
}

is_positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

# Keep this filter as one production and self-test contract. GitHub can return
# more than one valid attestation when a rerun reproduces byte-identical assets.
# Every result must still be a well-formed, same-run, GitHub-hosted provenance
# statement for the exact subject; exactly one result must name this attempt.
# BEGIN EXACT_ATTEMPT_ATTESTATION_FILTER
EXACT_ATTEMPT_ATTESTATION_FILTER='
  def canonical_sha256:
    type == "string" and test("^[0-9a-f]{64}$");
  def valid_subjects:
    try (
      type == "array" and length >= 1 and
      all(.[];
        type == "object" and
        (.name | type) == "string" and
        (.digest | type) == "object" and
        (.digest.sha256 | canonical_sha256)
      ) and
      ([.[] | select(
        .name == $subject_name and .digest.sha256 == $subject_sha256
      )] | length) == 1
    ) catch false;
  def valid_attestation:
    try (
      type == "object" and
      (.verificationResult | type) == "object" and
      (.verificationResult.statement | type) == "object" and
      (.verificationResult.statement.predicate | type) == "object" and
      (.verificationResult.statement.predicate.buildDefinition | type) == "object" and
      (.verificationResult.statement.predicate.buildDefinition.externalParameters | type) == "object" and
      (.verificationResult.statement.predicate.buildDefinition.externalParameters.workflow | type) == "object" and
      (.verificationResult.statement.predicate.runDetails | type) == "object" and
      (.verificationResult.statement.predicate.runDetails.metadata | type) == "object" and
      (.verificationResult.signature | type) == "object" and
      (.verificationResult.signature.certificate | type) == "object" and
      .verificationResult.statement.predicateType == "https://slsa.dev/provenance/v1" and
      .verificationResult.statement.predicate.buildDefinition.buildType == "https://actions.github.io/buildtypes/workflow/v1" and
      .verificationResult.statement.predicate.buildDefinition.externalParameters.workflow.path == $workflow and
      .verificationResult.statement.predicate.buildDefinition.externalParameters.workflow.ref == $tag_ref and
      .verificationResult.statement.predicate.buildDefinition.externalParameters.workflow.repository == ("https://github.com/" + $repository) and
      (.verificationResult.statement.predicate.runDetails.metadata.invocationId | type) == "string" and
      (.verificationResult.signature.certificate.runInvocationURI | type) == "string" and
      .verificationResult.statement.predicate.runDetails.metadata.invocationId ==
        .verificationResult.signature.certificate.runInvocationURI and
      (.verificationResult.statement.predicate.runDetails.metadata.invocationId as $entry_invocation |
        $entry_invocation | startswith($same_run_invocation_prefix)) and
      (.verificationResult.statement.predicate.runDetails.metadata.invocationId as $entry_invocation |
        $entry_invocation[($same_run_invocation_prefix | length):] |
        test("^[1-9][0-9]*$")) and
      .verificationResult.signature.certificate.githubWorkflowSHA == $source and
      .verificationResult.signature.certificate.githubWorkflowRepository == $repository and
      .verificationResult.signature.certificate.githubWorkflowRef == $tag_ref and
      .verificationResult.signature.certificate.runnerEnvironment == "github-hosted" and
      .verificationResult.signature.certificate.sourceRepositoryDigest == $source and
      .verificationResult.signature.certificate.sourceRepositoryRef == $tag_ref and
      (.verificationResult.statement.subject | valid_subjects)
    ) catch false;
  try (
    type == "array" and length >= 1 and
    all(.[]; valid_attestation) and
    ([.[] | select(
      .verificationResult.statement.predicate.runDetails.metadata.invocationId == $invocation and
      .verificationResult.signature.certificate.runInvocationURI == $invocation
    )] | length) == 1
  ) catch false
'
# END EXACT_ATTEMPT_ATTESTATION_FILTER

verify_exact_attempt_attestation_set() {
  local input_path=$1
  local invocation=$2
  local repository=$3
  local run_id=$4
  local source=$5
  local tag_ref=$6
  local workflow=$7
  local subject_name=$8
  local subject_sha256=$9
  local same_run_invocation_prefix
  same_run_invocation_prefix="https://github.com/${repository}/actions/runs/${run_id}/attempts/"
  jq -e \
    --arg invocation "$invocation" \
    --arg same_run_invocation_prefix "$same_run_invocation_prefix" \
    --arg repository "$repository" \
    --arg source "$source" \
    --arg tag_ref "$tag_ref" \
    --arg workflow "$workflow" \
    --arg subject_name "$subject_name" \
    --arg subject_sha256 "$subject_sha256" \
    "$EXACT_ATTEMPT_ATTESTATION_FILTER" "$input_path" >/dev/null
}

self_test_attestation_selection() {
  local repository="flrngel/local-browser-bridge"
  local run_id="123456789"
  local source="1111111111111111111111111111111111111111"
  local tag_ref="refs/heads/main"
  local workflow=".github/workflows/deploy.yml"
  local subject_name="fixture.bin"
  local subject_sha256="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  local old_invocation="https://github.com/$repository/actions/runs/$run_id/attempts/1"
  local current_invocation="https://github.com/$repository/actions/runs/$run_id/attempts/2"
  local old current
  old=$(jq -cn \
    --arg invocation "$old_invocation" --arg repository "$repository" \
    --arg source "$source" --arg tag_ref "$tag_ref" --arg workflow "$workflow" \
    --arg subject_name "$subject_name" --arg subject_sha256 "$subject_sha256" '
      {verificationResult:{statement:{predicateType:"https://slsa.dev/provenance/v1",
      subject:[{name:$subject_name,digest:{sha256:$subject_sha256}}],predicate:{
      buildDefinition:{buildType:"https://actions.github.io/buildtypes/workflow/v1",
      externalParameters:{workflow:{path:$workflow,ref:$tag_ref,
      repository:("https://github.com/" + $repository)}}},
      runDetails:{metadata:{invocationId:$invocation}}}},signature:{certificate:{
      runInvocationURI:$invocation,githubWorkflowSHA:$source,
      githubWorkflowRepository:$repository,githubWorkflowRef:$tag_ref,
      runnerEnvironment:"github-hosted",sourceRepositoryDigest:$source,
      sourceRepositoryRef:$tag_ref}}}}')
  current=$(jq -cn --argjson base "$old" --arg invocation "$current_invocation" '
    $base |
    .verificationResult.statement.predicate.runDetails.metadata.invocationId = $invocation |
    .verificationResult.signature.certificate.runInvocationURI = $invocation')

  if ! jq -cn --argjson old "$old" --argjson current "$current" '[$old,$current]' |
    verify_exact_attempt_attestation_set - "$current_invocation" "$repository" "$run_id" \
      "$source" "$tag_ref" "$workflow" "$subject_name" "$subject_sha256"; then
    die "attestation selection self-test rejected one old plus one current result"
  fi
  if jq -cn --argjson old "$old" '[$old]' |
    verify_exact_attempt_attestation_set - "$current_invocation" "$repository" "$run_id" \
      "$source" "$tag_ref" "$workflow" "$subject_name" "$subject_sha256"; then
    die "attestation selection self-test accepted an old-only result"
  fi
  if jq -cn --argjson current "$current" '[$current,$current]' |
    verify_exact_attempt_attestation_set - "$current_invocation" "$repository" "$run_id" \
      "$source" "$tag_ref" "$workflow" "$subject_name" "$subject_sha256"; then
    die "attestation selection self-test accepted duplicate current results"
  fi
  if jq -cn --argjson old "$old" --argjson current "$current" \
      '[$old,($current | del(.verificationResult.signature.certificate))]' |
    verify_exact_attempt_attestation_set - "$current_invocation" "$repository" "$run_id" \
      "$source" "$tag_ref" "$workflow" "$subject_name" "$subject_sha256"; then
    die "attestation selection self-test accepted a malformed current result"
  fi
  if jq -cn --argjson old "$old" --argjson current "$current" \
      '[$old,($current | .verificationResult.statement.subject[0].name = "wrong.bin")]' |
    verify_exact_attempt_attestation_set - "$current_invocation" "$repository" "$run_id" \
      "$source" "$tag_ref" "$workflow" "$subject_name" "$subject_sha256"; then
    die "attestation selection self-test accepted a wrong current subject"
  fi
}

is_canonical_decimal_at_most() {
  local value="$1"
  local maximum="$2"
  [[ "$value" =~ ^(0|[1-9][0-9]*)$ ]] || return 1
  if ((${#value} < ${#maximum})); then
    return 0
  fi
  if ((${#value} > ${#maximum})); then
    return 1
  fi
  ((10#$value <= 10#$maximum))
}

is_safe_evidence_path() {
  local path="$1"
  local canonical_root="$2"
  local remainder component
  [[ "$path" =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] || return 1
  ((${#path} <= EVIDENCE_MAX_PATH_BYTES)) || return 1
  [[ "$path" == "$canonical_root/"* ]] || return 1
  remainder="${path#"$canonical_root/"}"
  test -n "$remainder" || return 1
  [[ "$remainder" != /* && "$remainder" != */ && "$remainder" != *//* ]] || return 1
  while [[ "$remainder" == */* ]]; do
    component="${remainder%%/*}"
    test "$component" != . && test "$component" != .. \
      && ((${#component} <= EVIDENCE_MAX_PATH_COMPONENT_BYTES)) || return 1
    remainder="${remainder#*/}"
  done
  test "$remainder" != . && test "$remainder" != .. \
    && ((${#remainder} <= EVIDENCE_MAX_PATH_COMPONENT_BYTES))
}

line_inventory_contains_exact() {
  local expected="$1"
  local inventory_file="$2"
  local candidate
  while IFS= read -r candidate; do
    test "$candidate" = "$expected" && return 0
  done < "$inventory_file"
  return 1
}

assert_release_candidate_binding() {
  local record="$1"
  local selector="$2"
  test -f "$EXPECTED_RELEASE_CANDIDATE_BINDING" \
    || die "the independently reconstructed release-candidate binding is unavailable"
  jq -e \
    --arg selector "$selector" \
    --slurpfile expected "$EXPECTED_RELEASE_CANDIDATE_BINDING" '
      def selected($path): getpath($path | split(".") | map(select(length > 0)));
      selected($selector) as $actual
      | ($expected[0]) as $want
      | ($want | del(.assets)) as $base
      | ($base | keys_unsorted) as $base_keys
      | ($want | keys_unsorted) as $asset_keys
      | ($actual | type) == "object"
        and (($actual | keys_unsorted) == $base_keys or ($actual | keys_unsorted) == $asset_keys)
        and (if ($actual | has("assets")) then $actual == $want else $actual == $base end)
    ' "$record" >/dev/null \
    || die "evidence releaseCandidateBinding is not the exact current receipt/workflow-attempt/artifact binding: $record"
}

assert_utc_interval() {
  local start="$1"
  local finish="$2"
  local maximum_seconds="$3"
  local label="$4"
  python3 - "$start" "$finish" "$maximum_seconds" "$label" <<'PY'
import datetime
import re
import sys

start, finish, maximum, label = sys.argv[1:]
canonical = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{7}Z$")
def parse(value):
    if not canonical.fullmatch(value):
        raise SystemExit(f"{label} timestamp is not canonical UTC")
    return datetime.datetime.fromisoformat(value[:-1] + "+00:00")
elapsed = (parse(finish) - parse(start)).total_seconds()
if elapsed < 0 or elapsed > int(maximum):
    raise SystemExit(f"{label} interval is negative or exceeds its bound")
PY
}

validate_receipt() {
  local receipt_file="$1"
  local expected_version="$2"
  local expected_tag="$3"
  local expected_source_sha="$4"
  local expected_run_id="$5"
  local expected_run_attempt="$6"
  local expected_manifest_sha256="$7"

  test -f "$receipt_file" && test ! -L "$receipt_file" || return 1
  test "$(wc -c < "$receipt_file" | tr -d ' ')" -le 4096 || return 1
  test "$(wc -l < "$receipt_file" | tr -d ' ')" = 0 || return 1
  test "$(jq -c . "$receipt_file")" = "$(<"$receipt_file")" || return 1

  jq -e \
    --arg version "$expected_version" \
    --arg release_tag "$expected_tag" \
    --arg source_sha "$expected_source_sha" \
    --arg run_id "$expected_run_id" \
    --arg run_attempt "$expected_run_attempt" \
    --arg manifest_sha256 "$expected_manifest_sha256" '
      (type == "object")
      and (keys_unsorted == [
        "schemaVersion", "version", "releaseTag", "sourceSha",
        "workflowRunId", "workflowRunAttempt",
        "releaseCandidateArtifactId", "releaseCandidateArtifactZipSha256",
        "checksumManifestSha256", "evidenceRef", "evidenceCommitSha",
        "macosPassed", "macosAcceptanceSha256", "macosQuietResultSha256",
        "macosDeliberateConcurrencyResultSha256",
        "windowsPassed", "windowsResultSha256",
        "stockChromePassed", "stockChrome", "stockChromeResultSha256"
      ])
      and ((.schemaVersion | type) == "number") and (.schemaVersion == 3)
      and ((.version | type) == "string")
      and (.version | test("^[0-9]+\\.[0-9]+\\.[0-9]+$")) and (.version == $version)
      and ((.releaseTag | type) == "string")
      and (.releaseTag == ("v" + .version)) and (.releaseTag == $release_tag)
      and ((.sourceSha | type) == "string")
      and (.sourceSha | test("^[0-9a-f]{40}$")) and (.sourceSha == $source_sha)
      and ((.workflowRunId | type) == "string")
      and (.workflowRunId | test("^[1-9][0-9]*$")) and (.workflowRunId == $run_id)
      and ((.workflowRunAttempt | type) == "string")
      and (.workflowRunAttempt | test("^[1-9][0-9]*$")) and (.workflowRunAttempt == $run_attempt)
      and ((.releaseCandidateArtifactId | type) == "string")
      and (.releaseCandidateArtifactId | test("^[1-9][0-9]*$"))
      and ((.releaseCandidateArtifactZipSha256 | type) == "string")
      and (.releaseCandidateArtifactZipSha256 | test("^[0-9a-f]{64}$"))
      and ((.checksumManifestSha256 | type) == "string")
      and (.checksumManifestSha256 | test("^[0-9a-f]{64}$"))
      and (.checksumManifestSha256 == $manifest_sha256)
      and ((.evidenceRef | type) == "string")
      and (.evidenceRef == ("refs/heads/evidence/" + .releaseTag + "-release-run-" + .workflowRunId + "-attempt-" + .workflowRunAttempt))
      and ((.evidenceCommitSha | type) == "string")
      and (.evidenceCommitSha | test("^[0-9a-f]{40}$"))
      and ((.macosPassed | type) == "boolean") and (.macosPassed == true)
      and ((.macosAcceptanceSha256 | type) == "string")
      and (.macosAcceptanceSha256 | test("^[0-9a-f]{64}$"))
      and ((.macosQuietResultSha256 | type) == "string")
      and (.macosQuietResultSha256 | test("^[0-9a-f]{64}$"))
      and ((.macosDeliberateConcurrencyResultSha256 | type) == "string")
      and (.macosDeliberateConcurrencyResultSha256 | test("^[0-9a-f]{64}$"))
      and (.macosQuietResultSha256 != .macosDeliberateConcurrencyResultSha256)
      and ((.windowsPassed | type) == "boolean") and (.windowsPassed == true)
      and ((.windowsResultSha256 | type) == "string")
      and (.windowsResultSha256 | test("^[0-9a-f]{64}$"))
      and ((.stockChromePassed | type) == "boolean") and (.stockChromePassed == true)
      and ((.stockChrome | type) == "boolean") and (.stockChrome == true)
      and ((.stockChromeResultSha256 | type) == "string")
      and (.stockChromeResultSha256 | test("^[0-9a-f]{64}$"))
      and ([
        .macosAcceptanceSha256, .macosQuietResultSha256,
        .macosDeliberateConcurrencyResultSha256, .windowsResultSha256,
        .stockChromeResultSha256
      ] | unique | length == 5)
    ' "$receipt_file" >/dev/null
}

assert_json_hash() {
  local path="$1"
  local expected_sha256="$2"
  test -f "$path" && test ! -L "$path" || die "required JSON blob is absent: $path"
  jq -e . "$path" >/dev/null || die "required JSON blob is invalid: $path"
  test "$(sha256_file "$path")" = "$expected_sha256" || die "evidence blob SHA-256 mismatch: $path"
}

manifest_asset_sha256() {
  local manifest="$1"
  local asset_name="$2"
  awk -v expected="$asset_name" '
    $2 == expected { if (seen++) exit 2; value=$1 }
    END { if (seen != 1) exit 1; print value }
  ' "$manifest"
}

extract_macos_candidate_facts() {
  local archive="$1"
  local output="$2"
  local version="$3"
  python3 - "$archive" "$output" "$version" <<'PY'
import hashlib
import json
import plistlib
import struct
import sys
import tarfile

archive, output, version = sys.argv[1:]
expected_modes = {
    "local-browser-bridge": 0o755,
    "LICENSE": 0o644,
    "THIRD_PARTY_LICENSES.txt": 0o644,
    "Local Computer Helper.app": 0o755,
    "Local Computer Helper.app/Contents": 0o755,
    "Local Computer Helper.app/Contents/Info.plist": 0o644,
    "Local Computer Helper.app/Contents/MacOS": 0o755,
    "Local Computer Helper.app/Contents/MacOS/local-computer-helper": 0o755,
    "Local Computer Helper.app/Contents/_CodeSignature": 0o755,
    "Local Computer Helper.app/Contents/_CodeSignature/CodeResources": 0o644,
}
directories = {
    "Local Computer Helper.app",
    "Local Computer Helper.app/Contents",
    "Local Computer Helper.app/Contents/MacOS",
    "Local Computer Helper.app/Contents/_CodeSignature",
}
wanted = {
    "local-browser-bridge",
    "Local Computer Helper.app/Contents/MacOS/local-computer-helper",
    "Local Computer Helper.app/Contents/Info.plist",
    "Local Computer Helper.app/Contents/_CodeSignature/CodeResources",
}
maximum_member_bytes = 128 * 1024 * 1024
maximum_total_bytes = 256 * 1024 * 1024
payloads = {}
seen = set()
total = 0
with tarfile.open(archive, "r:gz") as bundle:
    if bundle.pax_headers:
        raise SystemExit("macOS package contains global PAX metadata")
    for member in bundle:
        name = member.name.removesuffix("/")
        if name not in expected_modes or name in seen or member.pax_headers:
            raise SystemExit(f"macOS package path is duplicated, unexpected, or PAX-overridden: {name}")
        seen.add(name)
        if member.mode != expected_modes[name]:
            raise SystemExit(f"macOS package member mode is noncanonical: {name}")
        if name in directories:
            if not member.isdir() or member.size != 0:
                raise SystemExit(f"macOS package directory type is invalid: {name}")
            continue
        if not member.isfile() or member.issym() or member.islnk() or member.size < 1 or member.size > maximum_member_bytes:
            raise SystemExit(f"unsafe macOS package member: {name}")
        total += member.size
        if total > maximum_total_bytes:
            raise SystemExit("macOS package exceeds its bounded uncompressed size")
        if name not in wanted:
            stream = bundle.extractfile(member)
            if stream is None:
                raise SystemExit(f"unreadable macOS package member: {name}")
            observed = 0
            with stream:
                while observed < member.size:
                    chunk = stream.read(min(1024 * 1024, member.size - observed))
                    if not chunk:
                        raise SystemExit(f"macOS package member length mismatch: {name}")
                    observed += len(chunk)
                if stream.read(1):
                    raise SystemExit(f"macOS package member exceeds its declared size: {name}")
            continue
        stream = bundle.extractfile(member)
        if stream is None:
            raise SystemExit(f"unreadable required macOS package member: {name}")
        data = bytearray()
        with stream:
            while len(data) < member.size:
                chunk = stream.read(min(1024 * 1024, member.size - len(data)))
                if not chunk:
                    raise SystemExit(f"macOS package member length mismatch: {name}")
                data.extend(chunk)
            if stream.read(1):
                raise SystemExit(f"macOS package member exceeds its declared size: {name}")
        if name in payloads:
            raise SystemExit(f"duplicate required macOS package member: {name}")
        payloads[name] = bytes(data)
if seen != set(expected_modes) or set(payloads) != wanted:
    raise SystemExit("macOS package does not have the exact independently inspected inventory")

CPU_X86_64 = 0x01000007
CPU_ARM64 = 0x0100000C
LC_CODE_SIGNATURE = 0x1D

def slices(data):
    if len(data) < 8:
        raise SystemExit("truncated macOS universal binary")
    magic = data[:4]
    if magic not in (b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"):
        raise SystemExit("macOS candidate is not a big-endian universal Mach-O")
    is64 = magic == b"\xca\xfe\xba\xbf"
    count = struct.unpack_from(">I", data, 4)[0]
    entry_size = 32 if is64 else 20
    if count != 2 or len(data) < 8 + count * entry_size:
        raise SystemExit("macOS candidate does not contain exactly two universal slices")
    result = []
    for index in range(count):
        offset = 8 + index * entry_size
        cpu = struct.unpack_from(">I", data, offset)[0]
        if is64:
            start, size = struct.unpack_from(">QQ", data, offset + 8)
        else:
            start, size = struct.unpack_from(">II", data, offset + 8)
        if size < 32 or start + size > len(data):
            raise SystemExit("macOS universal slice range is invalid")
        result.append((cpu, data[start:start + size]))
    if {cpu for cpu, _ in result} != {CPU_X86_64, CPU_ARM64}:
        raise SystemExit("macOS candidate architecture set is not exactly arm64+x86_64")
    return result

def has_code_signature(slice_data):
    magic = slice_data[:4]
    if magic == b"\xcf\xfa\xed\xfe":
        endian, header_size = "<", 32
    elif magic == b"\xfe\xed\xfa\xcf":
        endian, header_size = ">", 32
    else:
        raise SystemExit("universal slice is not a 64-bit Mach-O")
    ncmds, sizeofcmds = struct.unpack_from(endian + "II", slice_data, 16)
    if ncmds < 1 or ncmds > 4096 or header_size + sizeofcmds > len(slice_data):
        raise SystemExit("Mach-O load-command table is invalid")
    cursor = header_size
    found = False
    for _ in range(ncmds):
        if cursor + 8 > header_size + sizeofcmds:
            raise SystemExit("truncated Mach-O load command")
        command, size = struct.unpack_from(endian + "II", slice_data, cursor)
        if size < 8 or cursor + size > header_size + sizeofcmds:
            raise SystemExit("invalid Mach-O load command size")
        if (command & 0x7fffffff) == LC_CODE_SIGNATURE:
            if size < 16:
                raise SystemExit("truncated Mach-O code-signature command")
            data_offset, data_size = struct.unpack_from(endian + "II", slice_data, cursor + 8)
            if data_size < 12 or data_offset + data_size > len(slice_data) or \
               slice_data[data_offset:data_offset + 4] != b"\xfa\xde\x0c\xc0":
                raise SystemExit("Mach-O embedded code-signature superblob is invalid")
            found = True
        cursor += size
    return found

server = payloads["local-browser-bridge"]
helper = payloads["Local Computer Helper.app/Contents/MacOS/local-computer-helper"]
for label, data, product_name in (
    ("server", server, b"local-browser-bridge"),
    ("helper", helper, b"local-computer-helper"),
):
    binary_slices = slices(data)
    if not all(has_code_signature(item) for _, item in binary_slices):
        raise SystemExit(f"macOS {label} lacks an LC_CODE_SIGNATURE in every slice")
    if product_name not in data or version.encode() not in data:
        raise SystemExit(f"macOS {label} does not contain its independently checked product/version markers")

plist = plistlib.loads(payloads["Local Computer Helper.app/Contents/Info.plist"])
if plist.get("CFBundleExecutable") != "local-computer-helper" or \
   plist.get("CFBundleIdentifier") != "dev.flrngel.local-browser-bridge.computer-helper" or \
   plist.get("CFBundleShortVersionString") != version or plist.get("CFBundleVersion") != version or \
   plist.get("LSMinimumSystemVersion") != "13.0":
    raise SystemExit("macOS helper bundle metadata is not candidate-version bound")
json.dump({
    "serverSha256": hashlib.sha256(server).hexdigest(),
    "helperSha256": hashlib.sha256(helper).hexdigest(),
    "serverVersion": version,
    "helperVersion": version,
    "serverArchitectures": ["arm64", "x86_64"],
    "helperArchitectures": ["arm64", "x86_64"],
    "serverSignature": "ad-hoc",
    "helperSignature": "ad-hoc",
    "bundleCodeResourcesSha256": hashlib.sha256(
        payloads["Local Computer Helper.app/Contents/_CodeSignature/CodeResources"]
    ).hexdigest(),
}, open(output, "x", encoding="utf-8"), separators=(",", ":"))
PY
}

verify_raw_release_candidate() {
  local receipt_file="$1"
  local candidate_dir="$2"
  local scratch_root="$3"
  local artifact_id artifact_zip_sha256 artifact_json artifacts_json run_json jobs_json
  artifact_id="$(jq -er '.releaseCandidateArtifactId' "$receipt_file")"
  artifact_zip_sha256="$(jq -er '.releaseCandidateArtifactZipSha256' "$receipt_file")"
  is_positive_integer "$artifact_id" || die "release-candidate artifact ID is invalid"
  is_sha256 "$artifact_zip_sha256" || die "release-candidate artifact ZIP SHA-256 is invalid"

  run_json="$scratch_root/workflow-run.json"
  jobs_json="$scratch_root/workflow-jobs.json"
  artifacts_json="$scratch_root/workflow-artifacts.json"
  artifact_json="$scratch_root/artifact.json"
  gh api \
    -H 'Accept: application/vnd.github+json' \
    -H "X-GitHub-Api-Version: $API_VERSION" \
    "repos/$GITHUB_REPOSITORY/actions/runs/$CANDIDATE_RUN_ID/attempts/$CANDIDATE_RUN_ATTEMPT" \
    > "$run_json"
  jq -e \
    --argjson run_id "$CANDIDATE_RUN_ID" \
    --argjson run_attempt "$CANDIDATE_RUN_ATTEMPT" \
    --arg source_sha "$VERIFIED_SOURCE_SHA" \
    --arg workflow_path ".github/workflows/deploy.yml" '
      (.id == $run_id)
      and (.run_attempt == $run_attempt)
      and (.head_sha == $source_sha)
      and (.head_branch == "main")
      and (.event == "workflow_dispatch")
      and (.path == $workflow_path)
      and (.status == "completed")
      and (.conclusion == "success")
    ' "$run_json" >/dev/null || die "current workflow attempt identity did not match the receipt"

  gh api \
    -H 'Accept: application/vnd.github+json' \
    -H "X-GitHub-Api-Version: $API_VERSION" \
    "repos/$GITHUB_REPOSITORY/actions/runs/$CANDIDATE_RUN_ID/attempts/$CANDIDATE_RUN_ATTEMPT/jobs?per_page=100" \
    > "$jobs_json"
  jq -e '
      (.total_count < 100)
      and ([.jobs[] | select(.name == "Assemble frozen release candidate")] | length == 1)
      and ([.jobs[] | select(.name == "Assemble frozen release candidate")][0]
        | .conclusion == "success"
        and (.started_at | type) == "string"
        and (.completed_at | type) == "string")
    ' "$jobs_json" >/dev/null || die "the frozen candidate was not produced by the current successful assemble job"

  gh api \
    -H 'Accept: application/vnd.github+json' \
    -H "X-GitHub-Api-Version: $API_VERSION" \
    "repos/$GITHUB_REPOSITORY/actions/runs/$CANDIDATE_RUN_ID/artifacts?per_page=100" \
    > "$artifacts_json"
  jq -e \
    --argjson artifact_id "$artifact_id" \
    --argjson run_id "$CANDIDATE_RUN_ID" \
    --arg source_sha "$VERIFIED_SOURCE_SHA" '
      (.total_count < 100)
      and ([.artifacts[] | select(.id == $artifact_id)] | length == 1)
      and ([.artifacts[] | select(.id == $artifact_id)][0]
        | .name == "release-candidate"
        and .expired == false
        and (.size_in_bytes > 0 and .size_in_bytes <= 536870912)
        and (.digest | type == "string")
        and (.digest | test("^sha256:[0-9a-f]{64}$"))
        and (.workflow_run.id == $run_id)
        and (.workflow_run.head_sha == $source_sha))
      and ([.artifacts[] | select(.name == "release-candidate" and .expired == false)] | length == 1)
    ' "$artifacts_json" >/dev/null || die "receipt artifact ID is not the sole live release-candidate artifact for this run"

  gh api \
    -H 'Accept: application/vnd.github+json' \
    -H "X-GitHub-Api-Version: $API_VERSION" \
    "repos/$GITHUB_REPOSITORY/actions/artifacts/$artifact_id" \
    > "$artifact_json"
  jq -e \
    --argjson artifact_id "$artifact_id" \
    --argjson run_id "$CANDIDATE_RUN_ID" \
    --arg source_sha "$VERIFIED_SOURCE_SHA" \
    --slurpfile jobs "$jobs_json" '
      .id == $artifact_id
      and .name == "release-candidate"
      and .expired == false
      and (.size_in_bytes > 0 and .size_in_bytes <= 536870912)
      and (.digest | type == "string")
      and (.digest | test("^sha256:[0-9a-f]{64}$"))
      and (.workflow_run.id == $run_id)
      and (.workflow_run.head_sha == $source_sha)
      and (.created_at >= ($jobs[0].jobs[] | select(.name == "Assemble frozen release candidate") | .started_at))
      and (.created_at <= ($jobs[0].jobs[] | select(.name == "Assemble frozen release candidate") | .completed_at))
    ' "$artifact_json" >/dev/null || die "artifact metadata is not time-bound to the current assemble job"

  local raw_zip="$scratch_root/release-candidate-artifact.zip"
  local extracted="$scratch_root/release-candidate"
  gh api \
    -H "X-GitHub-Api-Version: $API_VERSION" \
    "repos/$GITHUB_REPOSITORY/actions/artifacts/$artifact_id/zip" \
    > "$raw_zip"
  test "$(wc -c < "$raw_zip" | tr -d ' ')" = "$(jq -er '.size_in_bytes' "$artifact_json")" \
    || die "raw release-candidate artifact ZIP byte count differs from GitHub metadata"
  test "$(sha256_file "$raw_zip")" = "$artifact_zip_sha256" || die "raw release-candidate artifact ZIP SHA-256 mismatch"
  test "$(jq -er '.digest | sub("^sha256:"; "")' "$artifact_json")" = "$artifact_zip_sha256" \
    || die "receipt artifact ZIP SHA-256 differs from GitHub metadata"

  mkdir "$extracted"
  python3 - "$raw_zip" "$extracted" "$RELEASE_TAG" <<'PY'
import os
import stat
import sys
import zipfile

archive, destination, tag = sys.argv[1:]
version = tag.removeprefix("v")
expected = [
    f"local-browser-bridge-v{version}-windows-x86_64.exe",
    f"local-computer-helper-v{version}-windows-x86_64.exe",
    f"local-browser-bridge-v{version}-macos-universal.tar.gz",
    f"local-browser-bridge-extension-v{version}.zip",
    "SHA256SUMS.txt",
]
with zipfile.ZipFile(archive) as bundle:
    infos = bundle.infolist()
    names = [item.filename for item in infos]
    if sorted(names) != sorted(expected) or len(set(names)) != len(expected):
        raise SystemExit("release-candidate artifact ZIP inventory is not exact")
    maximum_entry_bytes = 256 * 1024 * 1024
    maximum_total_bytes = 512 * 1024 * 1024
    total = 0
    for item in infos:
        mode = (item.external_attr >> 16) & 0xFFFF
        if (
            item.is_dir()
            or "/" in item.filename
            or "\\" in item.filename
            or item.flag_bits & 0x1
            or item.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
            or item.file_size < 1
            or item.file_size > maximum_entry_bytes
            or item.compress_size < 1
        ):
            raise SystemExit("release-candidate artifact ZIP has an unsafe path")
        if mode and stat.S_IFMT(mode) not in (0, stat.S_IFREG):
            raise SystemExit("release-candidate artifact ZIP has a non-regular entry")
        total += item.file_size
        if total > maximum_total_bytes:
            raise SystemExit("release-candidate artifact ZIP exceeds its bounded uncompressed size")
        target = os.path.join(destination, item.filename)
        descriptor = os.open(
            target,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
            0o600,
        )
        observed = 0
        try:
            with bundle.open(item, "r") as source, os.fdopen(descriptor, "wb", closefd=False) as output:
                while observed < item.file_size:
                    chunk = source.read(min(1024 * 1024, item.file_size - observed))
                    if not chunk:
                        raise SystemExit(f"release-candidate artifact ZIP entry is truncated: {item.filename}")
                    output.write(chunk)
                    observed += len(chunk)
                if source.read(1):
                    raise SystemExit(f"release-candidate artifact ZIP entry exceeds its declared size: {item.filename}")
                output.flush()
                os.fsync(output.fileno())
        finally:
            os.close(descriptor)
PY

  local version="${RELEASE_TAG#v}"
  bash scripts/verify-release-assets.sh "$version" "$extracted" --static-only
  local expected_invocation_uri="https://github.com/$GITHUB_REPOSITORY/actions/runs/$CANDIDATE_RUN_ID/attempts/$CANDIDATE_RUN_ATTEMPT"
  local attested_asset attestation_json attested_sha256
  for attested_asset in \
    "local-browser-bridge-v${version}-windows-x86_64.exe" \
    "local-computer-helper-v${version}-windows-x86_64.exe" \
    "local-browser-bridge-v${version}-macos-universal.tar.gz" \
    "local-browser-bridge-extension-v${version}.zip" \
    "SHA256SUMS.txt"; do
    attestation_json="$scratch_root/attestation-$attested_asset.json"
    attested_sha256="$(sha256_file "$extracted/$attested_asset")"
    gh attestation verify "$extracted/$attested_asset" \
      --repo "$GITHUB_REPOSITORY" \
      --source-ref "refs/heads/main" \
      --source-digest "$VERIFIED_SOURCE_SHA" \
      --signer-workflow "$GITHUB_REPOSITORY/.github/workflows/deploy.yml" \
      --deny-self-hosted-runners \
      --format json > "$attestation_json"
    verify_exact_attempt_attestation_set \
      "$attestation_json" "$expected_invocation_uri" "$GITHUB_REPOSITORY" \
      "$CANDIDATE_RUN_ID" "$VERIFIED_SOURCE_SHA" "refs/heads/main" \
      ".github/workflows/deploy.yml" "$attested_asset" "$attested_sha256" ||
      die "candidate provenance did not bind one exact workflow-attempt attestation: $attested_asset"
  done
  test "$(sha256_file "$extracted/SHA256SUMS.txt")" = "$(jq -er '.checksumManifestSha256' "$receipt_file")" \
    || die "raw artifact checksum manifest does not match the receipt"
  EXPECTED_RELEASE_CANDIDATE_BINDING="$scratch_root/expected-release-candidate-binding.json"
  local binding_assets="$scratch_root/expected-release-candidate-assets.json"
  python3 - "$extracted" "$binding_assets" <<'PY'
import hashlib
import json
import os
import sys

root, output = sys.argv[1:]
assets = []
version = next(name.split("-v", 1)[1].split("-windows", 1)[0] for name in os.listdir(root) if name.startswith("local-browser-bridge-v") and name.endswith("-windows-x86_64.exe"))
for name in [
    f"local-browser-bridge-v{version}-windows-x86_64.exe",
    f"local-computer-helper-v{version}-windows-x86_64.exe",
    f"local-browser-bridge-v{version}-macos-universal.tar.gz",
    f"local-browser-bridge-extension-v{version}.zip",
    "SHA256SUMS.txt",
]:
    path = os.path.join(root, name)
    with open(path, "rb") as stream:
        data = stream.read()
    assets.append({"file": name, "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)})
json.dump(assets, open(output, "x", encoding="utf-8"), separators=(",", ":"))
PY
  jq -cn \
    --arg version "$version" \
    --arg release_tag "$RELEASE_TAG" \
    --arg repository "$GITHUB_REPOSITORY" \
    --arg source_sha "$VERIFIED_SOURCE_SHA" \
    --arg workflow_run_id "$CANDIDATE_RUN_ID" \
    --arg workflow_run_attempt "$CANDIDATE_RUN_ATTEMPT" \
    --arg artifact_id "$artifact_id" \
    --arg artifact_name "release-candidate" \
    --argjson artifact_zip_bytes "$(wc -c < "$raw_zip" | tr -d ' ')" \
    --arg artifact_zip_sha256 "$artifact_zip_sha256" \
    --arg checksum_manifest_sha256 "$(jq -er '.checksumManifestSha256' "$receipt_file")" \
    --arg attestation_invocation_uri "$expected_invocation_uri" \
    --slurpfile assets "$binding_assets" '
      {
        schemaVersion: 3,
        version: $version,
        releaseTag: $release_tag,
        repository: $repository,
        sourceSha: $source_sha,
        workflowRunId: $workflow_run_id,
        workflowRunAttempt: $workflow_run_attempt,
        workflowEvent: "workflow_dispatch",
        workflowRef: "refs/heads/main",
        workflowPath: ".github/workflows/deploy.yml",
        artifactId: $artifact_id,
        artifactName: $artifact_name,
        artifactZipBytes: $artifact_zip_bytes,
        artifactZipSha256: $artifact_zip_sha256,
        checksumManifestSha256: $checksum_manifest_sha256,
        attestationInvocationUri: $attestation_invocation_uri,
        attestedAssetCount: 5,
        githubHostedRunner: true,
        assets: $assets[0]
      }
    ' > "$EXPECTED_RELEASE_CANDIDATE_BINDING"
  MACOS_CANDIDATE_FACTS="$scratch_root/macos-candidate-facts.json"
  extract_macos_candidate_facts \
    "$extracted/local-browser-bridge-v${version}-macos-universal.tar.gz" \
    "$MACOS_CANDIDATE_FACTS" "$version"
  local asset
  for asset in \
    "local-browser-bridge-v${version}-windows-x86_64.exe" \
    "local-computer-helper-v${version}-windows-x86_64.exe" \
    "local-browser-bridge-v${version}-macos-universal.tar.gz" \
    "local-browser-bridge-extension-v${version}.zip" \
    "SHA256SUMS.txt"; do
    test -f "$candidate_dir/$asset" && test ! -L "$candidate_dir/$asset" \
      || die "Actions download is missing candidate asset: $asset"
    cmp -s "$extracted/$asset" "$candidate_dir/$asset" \
      || die "Actions download differs from the independently downloaded artifact: $asset"
  done
}

add_allowed() {
  local relative="$1"
  [[ "$relative" =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] || die "unsafe evidence path: $relative"
  [[ "$relative" != *"//"* && "$relative" != *"/./"* && "$relative" != *"../"* ]] \
    || die "unsafe evidence path: $relative"
  ALLOWED_PATHS["$relative"]=1
}

verify_file_fact() {
  local path="$1"
  local expected_sha256="$2"
  local expected_bytes="$3"
  is_sha256 "$expected_sha256" || die "invalid sidecar SHA-256 for $path"
  [[ "$expected_bytes" =~ ^[1-9][0-9]*$ ]] || die "invalid sidecar byte count for $path"
  test -f "$path" && test ! -L "$path" || die "referenced sidecar is absent: $path"
  test "$(sha256_file "$path")" = "$expected_sha256" || die "referenced sidecar SHA-256 mismatch: $path"
  test "$(wc -c < "$path" | tr -d ' ')" = "$expected_bytes" || die "referenced sidecar byte count mismatch: $path"
}

verify_png_dimensions() {
  local path="$1"
  local expected_width="$2"
  local expected_height="$3"
  local expected_pixel_sha256="${4:-}"
  [[ "$expected_width" =~ ^[1-9][0-9]*$ && "$expected_height" =~ ^[1-9][0-9]*$ ]] \
    || die "claimed PNG dimensions are invalid: $path"
  if [[ -n "$expected_pixel_sha256" ]]; then
    is_sha256 "$expected_pixel_sha256" || die "claimed PNG decoded-pixel SHA-256 is invalid: $path"
  fi
  python3 - "$path" "$expected_width" "$expected_height" "$expected_pixel_sha256" <<'PY'
import hashlib
import struct
import sys
import zlib

path, expected_width, expected_height, expected_pixel_sha256 = sys.argv[1:]
expected = (int(expected_width), int(expected_height))
data = open(path, "rb").read()
if len(data) < 57 or len(data) > 100 * 1024 * 1024 or data[:8] != b"\x89PNG\r\n\x1a\n":
    raise SystemExit(f"referenced image is not a bounded PNG: {path}")
offset = 8
chunks = []
idat = bytearray()
while offset < len(data):
    if offset + 12 > len(data):
        raise SystemExit(f"truncated PNG chunk: {path}")
    length = struct.unpack_from(">I", data, offset)[0]
    kind = data[offset + 4:offset + 8]
    end = offset + 12 + length
    if length > 64 * 1024 * 1024 or end > len(data):
        raise SystemExit(f"invalid PNG chunk length: {path}")
    payload = data[offset + 8:offset + 8 + length]
    expected_crc = struct.unpack_from(">I", data, offset + 8 + length)[0]
    if zlib.crc32(kind + payload) & 0xffffffff != expected_crc:
        raise SystemExit(f"PNG CRC mismatch: {path}")
    chunks.append(kind)
    if kind == b"IDAT":
        idat.extend(payload)
    if kind == b"IEND" and payload:
        raise SystemExit(f"PNG IEND payload is invalid: {path}")
    offset = end
    if kind == b"IEND":
        break
if offset != len(data) or not chunks or chunks[0] != b"IHDR" or chunks[-1] != b"IEND" or chunks.count(b"IHDR") != 1 or chunks.count(b"IEND") != 1:
    raise SystemExit(f"PNG chunk sequence is noncanonical: {path}")
idat_indexes = [index for index, kind in enumerate(chunks) if kind == b"IDAT"]
if not idat_indexes or idat_indexes != list(range(idat_indexes[0], idat_indexes[-1] + 1)):
    raise SystemExit(f"PNG IDAT chunks are absent or noncontiguous: {path}")
ihdr_length = struct.unpack_from(">I", data, 8)[0]
if ihdr_length != 13:
    raise SystemExit(f"PNG IHDR length is invalid: {path}")
width, height, bit_depth, color_type, compression, filtering, interlace = struct.unpack_from(">IIBBBBB", data, 16)
if (width, height) != expected or width * height > 50_000_000:
    raise SystemExit(f"referenced PNG dimensions differ or exceed the pixel limit: {path}")
legal = {0: {1,2,4,8,16}, 2: {8,16}, 3: {1,2,4,8}, 4: {8,16}, 6: {8,16}}
channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
if color_type not in legal or bit_depth not in legal[color_type] or compression != 0 or filtering != 0 or interlace != 0:
    raise SystemExit(f"unsupported or invalid PNG IHDR semantics: {path}")
if color_type == 3 and b"PLTE" not in chunks:
    raise SystemExit(f"indexed PNG has no palette: {path}")
if not idat:
    raise SystemExit(f"PNG has no compressed pixel payload: {path}")
row_bytes = (width * channels[color_type] * bit_depth + 7) // 8
decoded_limit = height * (row_bytes + 1)
if decoded_limit > 256 * 1024 * 1024:
    raise SystemExit(f"PNG decoded payload exceeds the memory bound: {path}")
decoder = zlib.decompressobj()
decoded = decoder.decompress(bytes(idat), decoded_limit + 1)
if decoder.unconsumed_tail:
    raise SystemExit(f"PNG IDAT expands beyond the claimed raster: {path}")
decoded += decoder.flush(max(1, decoded_limit + 1 - len(decoded)))
if not decoder.eof or decoder.unused_data or len(decoded) != decoded_limit:
    raise SystemExit(f"PNG IDAT does not decode to the claimed raster: {path}")
if any(decoded[row * (row_bytes + 1)] > 4 for row in range(height)):
    raise SystemExit(f"PNG scanline has an invalid filter method: {path}")
if expected_pixel_sha256:
    if bit_depth != 8 or color_type != 6:
        raise SystemExit(f"pixel-bound PNG must be noninterlaced RGBA8: {path}")
    if any(kind not in (b"IHDR", b"IDAT", b"IEND") for kind in chunks):
        raise SystemExit(f"pixel-bound PNG contains unexpected metadata or ancillary chunks: {path}")
    pixels = bytearray(width * height * 4)
    def paeth(left, above, upper_left):
        estimate = left + above - upper_left
        distances = (abs(estimate - left), abs(estimate - above), abs(estimate - upper_left))
        if distances[0] <= distances[1] and distances[0] <= distances[2]:
            return left
        return above if distances[1] <= distances[2] else upper_left
    for row in range(height):
        encoded_offset = row * (row_bytes + 1)
        filter_method = decoded[encoded_offset]
        pixel_offset = row * row_bytes
        for column in range(row_bytes):
            encoded = decoded[encoded_offset + 1 + column]
            left = pixels[pixel_offset + column - 4] if column >= 4 else 0
            above = pixels[pixel_offset - row_bytes + column] if row else 0
            upper_left = pixels[pixel_offset - row_bytes + column - 4] if row and column >= 4 else 0
            predictor = 0
            if filter_method == 1:
                predictor = left
            elif filter_method == 2:
                predictor = above
            elif filter_method == 3:
                predictor = (left + above) // 2
            elif filter_method == 4:
                predictor = paeth(left, above, upper_left)
            pixels[pixel_offset + column] = (encoded + predictor) & 0xff
    digest = hashlib.sha256(f"{width}x{height}\0".encode("ascii") + pixels).hexdigest()
    if digest != expected_pixel_sha256:
        raise SystemExit(f"PNG decoded pixels do not match the aggregate hash: {path}")
PY
}

validate_mac_result_schema_binding() {
  local aggregate="$1"
  local quiet_result="$2"
  local deliberate_result="$3"
  jq -e \
    --argjson quiet_result_schema_version "$(jq -er '.schemaVersion | select(type == "number" and . == floor)' "$quiet_result")" \
    --argjson deliberate_result_schema_version "$(jq -er '.schemaVersion | select(type == "number" and . == floor)' "$deliberate_result")" '
      (.aggregateChecks | type) == "object"
      and .aggregateChecks.passingResultSchemaVersion == $quiet_result_schema_version
      and $quiet_result_schema_version == $deliberate_result_schema_version
    ' "$aggregate" >/dev/null
}

write_mac_harness_source_binding() {
  local output="$1"
  local harness_root="evidence/v${EVIDENCE_PRODUCT_VERSION}/computer"
  local runner="$harness_root/helper-evidence-rig.mjs"
  local fixture="$harness_root/HelperEvidenceFixture.swift"
  local system_probe="$harness_root/SystemProbe.swift"
  local app_share_handoff="$harness_root/AppShareHandoff.swift"
  local physical_pointer_handoff="$harness_root/PhysicalPointerHandoff.swift"
  local finalizer="scripts/finalize-macos-acceptance.mjs"
  local source_path
  local runner_sha256 fixture_sha256 system_probe_sha256 app_share_handoff_sha256
  local physical_pointer_handoff_sha256 finalizer_sha256

  test ! -e "$output" && test ! -L "$output" || return 1
  for source_path in \
    "$runner" "$fixture" "$system_probe" "$app_share_handoff" \
    "$physical_pointer_handoff" "$finalizer"; do
    test -f "$source_path" && test ! -L "$source_path" || return 1
  done
  runner_sha256="$(sha256_file "$runner")"
  fixture_sha256="$(sha256_file "$fixture")"
  system_probe_sha256="$(sha256_file "$system_probe")"
  app_share_handoff_sha256="$(sha256_file "$app_share_handoff")"
  physical_pointer_handoff_sha256="$(sha256_file "$physical_pointer_handoff")"
  finalizer_sha256="$(sha256_file "$finalizer")"
  for source_path in \
    "$runner_sha256" "$fixture_sha256" "$system_probe_sha256" \
    "$app_share_handoff_sha256" "$physical_pointer_handoff_sha256" \
    "$finalizer_sha256"; do
    is_sha256 "$source_path" || return 1
  done

  (umask 077; jq -cn \
    --arg runner_sha256 "$runner_sha256" \
    --arg fixture_sha256 "$fixture_sha256" \
    --arg system_probe_sha256 "$system_probe_sha256" \
    --arg app_share_handoff_sha256 "$app_share_handoff_sha256" \
    --arg physical_pointer_handoff_sha256 "$physical_pointer_handoff_sha256" \
    --arg finalizer_sha256 "$finalizer_sha256" '
      {
        runnerSha256: $runner_sha256,
        fixtureSha256: $fixture_sha256,
        systemProbeSha256: $system_probe_sha256,
        appShareHandoffSha256: $app_share_handoff_sha256,
        physicalPointerHandoffSha256: $physical_pointer_handoff_sha256,
        acceptanceFinalizerSha256: $finalizer_sha256,
        packagedHelperSpawnCount: 1
      }
    ' > "$output")
}

validate_mac_harness_source_binding() {
  local expected="$1"
  local aggregate="$2"
  local quiet_result="$3"
  local deliberate_result="$4"

  test -f "$expected" && test ! -L "$expected" || return 1
  jq -e '
      (keys_unsorted == [
        "runnerSha256", "fixtureSha256", "systemProbeSha256",
        "appShareHandoffSha256", "physicalPointerHandoffSha256",
        "acceptanceFinalizerSha256",
        "packagedHelperSpawnCount"
      ])
      and ([
        .runnerSha256, .fixtureSha256, .systemProbeSha256,
        .appShareHandoffSha256, .physicalPointerHandoffSha256,
        .acceptanceFinalizerSha256
      ] | all(type == "string" and test("^[0-9a-f]{64}$")))
      and .packagedHelperSpawnCount == 1
    ' "$expected" >/dev/null || return 1
  jq -e --slurpfile expected "$expected" '
      .bindings.harness == $expected[0]
    ' "$aggregate" >/dev/null || return 1
  jq -se --slurpfile expected "$expected" '
      length == 2 and all(.[]; .harness == $expected[0])
    ' "$quiet_result" "$deliberate_result" >/dev/null
}

validate_macos_pointer_app_share_contract() {
  local result="$1"
  local lane="$2"
  [[ "$lane" == quiet || "$lane" == deliberate-concurrency ]] || return 1
  jq -e --arg lane "$lane" '
      (has("operatorHandoff") | not)
      and (.pointerEvidence | keys_unsorted == [
        "requestedLane", "quietObserved", "concurrentSharedSeatActivityObserved",
        "unknownObserved", "rawCursorPositionsRetained",
        "rawPlatformActivityCountersRetained", "rawHidSystemCountersRetained",
        "hidSystemActivityClaimedAsPhysical"
      ])
      and .pointerEvidence.requestedLane == $lane
      and .pointerEvidence.quietObserved == true
      and .pointerEvidence.concurrentSharedSeatActivityObserved == false
      and .pointerEvidence.unknownObserved == false
      and .pointerEvidence.rawCursorPositionsRetained == false
      and .pointerEvidence.rawPlatformActivityCountersRetained == false
      and .pointerEvidence.rawHidSystemCountersRetained == false
      and .pointerEvidence.hidSystemActivityClaimedAsPhysical == false
      and (.appShareHandoff | keys_unsorted == [
        "requested", "requestPublicationAcknowledged", "startReceiptAcknowledged",
        "completePublicationAcknowledged", "promptClosed", "exactAppBundleObserved",
        "exactWindowObserved", "exactButtonObserved", "buttonDisabledAfterAction",
        "acceptanceButtonActionObserved", "appShareSurfaceObservedAtProductBoundaries",
        "sharedHidInputObserved", "sampledSharedContextUnchanged",
        "authorityRefreshedAfterReceipt", "authorityFreshAtDispatch", "actionDispatched",
        "targetPostconditionObserved", "productBoundaryQuiet", "independentBoundaryQuiet",
        "physicalHumanProvenanceClaimed", "cryptographicToolIdentityClaimed",
        "orchestrationNotProductControl", "markerNotificationOnly",
        "markerAcceptedAsProductAuthority", "rawAppIdentityRetainedInResult",
        "rawPointerDataRetained"
      ])
      and (if $lane == "quiet" then
        .appShareHandoff == {
          requested: false,
          requestPublicationAcknowledged: false,
          startReceiptAcknowledged: false,
          completePublicationAcknowledged: false,
          promptClosed: false,
          exactAppBundleObserved: false,
          exactWindowObserved: false,
          exactButtonObserved: false,
          buttonDisabledAfterAction: false,
          acceptanceButtonActionObserved: false,
          appShareSurfaceObservedAtProductBoundaries: false,
          sharedHidInputObserved: null,
          sampledSharedContextUnchanged: false,
          authorityRefreshedAfterReceipt: false,
          authorityFreshAtDispatch: false,
          actionDispatched: false,
          targetPostconditionObserved: false,
          productBoundaryQuiet: false,
          independentBoundaryQuiet: false,
          physicalHumanProvenanceClaimed: false,
          cryptographicToolIdentityClaimed: false,
          orchestrationNotProductControl: true,
          markerNotificationOnly: false,
          markerAcceptedAsProductAuthority: false,
          rawAppIdentityRetainedInResult: false,
          rawPointerDataRetained: false
        }
      else
        .appShareHandoff == {
          requested: true,
          requestPublicationAcknowledged: true,
          startReceiptAcknowledged: true,
          completePublicationAcknowledged: true,
          promptClosed: true,
          exactAppBundleObserved: true,
          exactWindowObserved: true,
          exactButtonObserved: true,
          buttonDisabledAfterAction: true,
          acceptanceButtonActionObserved: true,
          appShareSurfaceObservedAtProductBoundaries: true,
          sharedHidInputObserved: false,
          sampledSharedContextUnchanged: true,
          authorityRefreshedAfterReceipt: true,
          authorityFreshAtDispatch: true,
          actionDispatched: true,
          targetPostconditionObserved: true,
          productBoundaryQuiet: true,
          independentBoundaryQuiet: true,
          physicalHumanProvenanceClaimed: false,
          cryptographicToolIdentityClaimed: false,
          orchestrationNotProductControl: true,
          markerNotificationOnly: false,
          markerAcceptedAsProductAuthority: false,
          rawAppIdentityRetainedInResult: false,
          rawPointerDataRetained: false
        }
      end)
    ' "$result" >/dev/null
}

validate_macos_authority_assertion_contract() {
  local result="$1"
  local lane="$2"
  [[ "$lane" == quiet || "$lane" == deliberate-concurrency ]] || return 1
  jq -e --arg lane "$lane" '
      (.assertions.details | type) == "array"
      and ([.assertions.details[].name] | all(type == "string" and length > 0))
      and ([.assertions.details[].name] | length) ==
        ([.assertions.details[].name] | unique | length)
      and (if $lane == "deliberate-concurrency" then
        ([
          "app-share receipt retained the exact persistent share",
          "post-handoff share action authority is fresh and exact",
          "app-share handoff and frame refresh caused no target mutation",
          "post-handoff share action authority remained fresh at dispatch"
        ] - [.assertions.details[].name] | length == 0)
      else
        ([.assertions.details[].name] | map(select(
          . == "app-share receipt retained the exact persistent share" or
          . == "post-handoff share action authority is fresh and exact" or
          . == "app-share handoff and frame refresh caused no target mutation" or
          . == "post-handoff share action authority remained fresh at dispatch"
        )) | length == 0)
      end)
    ' "$result" >/dev/null
}

validate_macos_app_share_marker_chain() {
  local request_marker="$1"
  local start_marker="$2"
  local complete_marker="$3"
  local expected_request_sha256="$4"
  local expected_start_sha256="$5"
  local expected_complete_sha256="$6"
  local lane_result="$7"
  local marker

  for marker in "$request_marker" "$start_marker" "$complete_marker"; do
    test -f "$marker" && test ! -L "$marker" || return 1
    test "$(wc -c < "$marker" | tr -d ' ')" -ge 1 || return 1
    test "$(wc -c < "$marker" | tr -d ' ')" -le 16384 || return 1
  done
  test -f "$lane_result" && test ! -L "$lane_result" || return 1
  is_sha256 "$expected_request_sha256" \
    && is_sha256 "$expected_start_sha256" \
    && is_sha256 "$expected_complete_sha256" || return 1
  test "$(sha256_file "$request_marker")" = "$expected_request_sha256" \
    && test "$(sha256_file "$start_marker")" = "$expected_start_sha256" \
    && test "$(sha256_file "$complete_marker")" = "$expected_complete_sha256" \
    || return 1

  python3 - \
    "$request_marker" "$start_marker" "$complete_marker" \
    "$expected_request_sha256" "$expected_start_sha256" \
    "$expected_complete_sha256" "$EVIDENCE_PRODUCT_VERSION" "$lane_result" <<'PY'
import datetime
import hashlib
import json
import re
import sys

request_path, start_path, complete_path, request_sha, start_sha, complete_sha, version, result_path = sys.argv[1:]
request_fields = [
    "schemaVersion", "kind", "productVersion", "requestId", "createdAt", "expiresAt",
    "runnerPid", "promptPid", "expectedBundleIdentifier", "expectedWindowTitle",
    "expectedButtonText", "expectedButtonAccessibilityIdentifier",
    "expectedButtonEnabledAfterDelivery", "exactAppObserved", "exactWindowObserved",
    "requestDelivered", "panelOnScreen", "panelNonactivating", "notificationOnly",
    "exactAppShareRequired", "physicalHumanProvenanceRequired",
    "acceptedAsProductAuthority",
]
start_fields = [
    "acceptedAsAuthority", "buttonAccepted", "buttonActionObserved", "createdAt",
    "cryptographicToolIdentityClaimed", "kind", "physicalHumanProvenanceClaimed",
    "productVersion", "promptPid", "requestId", "requestSha256", "schemaVersion",
]
complete_fields = [
    "acceptedAsAuthority", "buttonRemainedDisabledDuringProductAction", "createdAt",
    "cryptographicToolIdentityClaimed", "handoffStateSequenceBound", "kind",
    "physicalHumanProvenanceClaimed", "productActionCompletedAt", "productActionStartedAt",
    "productVersion", "promptPid", "requestId", "requestSha256", "schemaVersion",
    "startReceiptSha256",
]
timestamp_pattern = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$")

def fail(message):
    raise SystemExit(message)

def read_exact(path, expected_fields, label, expected_sha):
    raw = open(path, "rb").read()
    if hashlib.sha256(raw).hexdigest() != expected_sha:
        fail(f"{label} SHA-256 mismatch")
    try:
        text = raw.decode("utf-8", "strict")
    except UnicodeDecodeError:
        fail(f"{label} is not strict UTF-8")
    if not text.endswith("\n") or "\x00" in text or text.startswith("\ufeff"):
        fail(f"{label} bytes are noncanonical")
    pairs_seen = []
    def exact_object(pairs):
        keys = [key for key, _ in pairs]
        if len(keys) != len(set(keys)):
            fail(f"{label} contains a duplicate key")
        pairs_seen.append(keys)
        return dict(pairs)
    try:
        value = json.loads(text, object_pairs_hook=exact_object)
    except json.JSONDecodeError:
        fail(f"{label} is invalid JSON")
    if not isinstance(value, dict) or not pairs_seen or pairs_seen[-1] != expected_fields:
        fail(f"{label} fields are not exact and ordered")
    if json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n" != text:
        fail(f"{label} is not canonical compact JSON")
    return value

def instant(value, label):
    if not isinstance(value, str) or not timestamp_pattern.fullmatch(value):
        fail(f"{label} is not canonical UTC")
    try:
        parsed = datetime.datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError:
        fail(f"{label} is not a real timestamp")
    if parsed.isoformat(timespec="milliseconds").replace("+00:00", "Z") != value:
        fail(f"{label} is not a real canonical timestamp")
    return parsed

def exact_bool(value, expected, label):
    if type(value) is not bool or value is not expected:
        fail(f"{label} must be {expected}")

request = read_exact(request_path, request_fields, "request marker", request_sha)
start = read_exact(start_path, start_fields, "start receipt", start_sha)
complete = read_exact(complete_path, complete_fields, "complete receipt", complete_sha)

if request["schemaVersion"] != 2 or type(request["schemaVersion"]) is not int:
    fail("request schemaVersion is invalid")
if request["kind"] != "macos-app-share-concurrency-handoff-request" or request["productVersion"] != version:
    fail("request kind or productVersion is invalid")
if not isinstance(request["requestId"], str) or not re.fullmatch(r"[0-9a-f]{32}", request["requestId"]):
    fail("requestId is invalid")
for field in ("runnerPid", "promptPid"):
    if type(request[field]) is not int or not 1 <= request[field] <= 2_147_483_647:
        fail(f"request {field} is invalid")
if request["runnerPid"] == request["promptPid"]:
    fail("request process identities are not distinct")
if (
    request["expectedBundleIdentifier"] != "dev.flrngel.local-browser-bridge.acceptance.app-share"
    or request["expectedWindowTitle"] != "LBB macOS Acceptance App Share"
    or request["expectedButtonText"] != "START APP-SHARE CHECK"
    or request["expectedButtonAccessibilityIdentifier"] != "lbb-app-share-start"
):
    fail("request exact-app surface binding is invalid")
for field, expected in (
    ("expectedButtonEnabledAfterDelivery", True), ("exactAppObserved", True),
    ("exactWindowObserved", True), ("requestDelivered", True), ("panelOnScreen", True),
    ("panelNonactivating", True), ("notificationOnly", False),
    ("exactAppShareRequired", True), ("physicalHumanProvenanceRequired", False),
    ("acceptedAsProductAuthority", False),
):
    exact_bool(request[field], expected, f"request {field}")
request_created = instant(request["createdAt"], "request createdAt")
request_expires = instant(request["expiresAt"], "request expiresAt")
request_lifetime = (request_expires - request_created).total_seconds()
if request_lifetime <= 0 or request_lifetime > 300:
    fail("request lifetime is outside the bounded interval")

if start["schemaVersion"] != 2 or type(start["schemaVersion"]) is not int:
    fail("start schemaVersion is invalid")
if (
    start["kind"] != "macos-app-share-concurrency-handoff-start"
    or start["productVersion"] != version
    or start["requestId"] != request["requestId"]
    or start["requestSha256"] != request_sha
    or start["promptPid"] != request["promptPid"]
):
    fail("start receipt request binding is invalid")
for field, expected in (
    ("acceptedAsAuthority", False), ("buttonAccepted", True),
    ("buttonActionObserved", True), ("cryptographicToolIdentityClaimed", False),
    ("physicalHumanProvenanceClaimed", False),
):
    exact_bool(start[field], expected, f"start {field}")
start_created = instant(start["createdAt"], "start createdAt")
if start_created < request_created or start_created > request_expires:
    fail("start receipt is outside its request interval")

if complete["schemaVersion"] != 2 or type(complete["schemaVersion"]) is not int:
    fail("complete schemaVersion is invalid")
if (
    complete["kind"] != "macos-app-share-concurrency-handoff-complete"
    or complete["productVersion"] != version
    or complete["requestId"] != request["requestId"]
    or complete["requestSha256"] != request_sha
    or complete["startReceiptSha256"] != start_sha
    or complete["promptPid"] != request["promptPid"]
):
    fail("complete receipt request/start binding is invalid")
for field, expected in (
    ("acceptedAsAuthority", False),
    ("buttonRemainedDisabledDuringProductAction", True),
    ("cryptographicToolIdentityClaimed", False),
    ("handoffStateSequenceBound", True),
    ("physicalHumanProvenanceClaimed", False),
):
    exact_bool(complete[field], expected, f"complete {field}")
action_started = instant(complete["productActionStartedAt"], "product action start")
action_completed = instant(complete["productActionCompletedAt"], "product action completion")
complete_created = instant(complete["createdAt"], "complete createdAt")
if (
    action_started < start_created
    or (action_started - start_created).total_seconds() > 10
    or action_completed < action_started
    or (action_completed - action_started).total_seconds() > 10
    or complete_created < action_completed
    or (complete_created - action_started).total_seconds() > 10
    or (complete_created - start_created).total_seconds() > 10
    or (complete_created - request_created).total_seconds() > 310
):
    fail("complete receipt timestamps are outside the bounded action interval")
with open(result_path, encoding="utf-8") as source:
    result = json.load(source)
lane_started = instant(result.get("startedAt"), "lane startedAt")
lane_captured = instant(result.get("capturedAt"), "lane capturedAt")
if request_created < lane_started or complete_created > lane_captured:
    fail("app-share marker chain falls outside the deliberate lane interval")
PY
}

validate_macos_app_share_operator_inventory() {
  local aggregate="$1"
  local aggregate_lane_key="$2"
  jq -e --arg key "$aggregate_lane_key" '
      (.lanes[$key].operatorMarkers | length == 3)
      and ([.lanes[$key].operatorMarkers[] | keys_unsorted]
        | all(. == ["file", "sha256"]))
      and [.lanes[$key].operatorMarkers[].file] == [
        "operator/macos-app-share-concurrency-handoff-request.json",
        "operator/macos-app-share-concurrency-handoff-start.json",
        "operator/macos-app-share-concurrency-handoff-complete.json"
      ]
      and ([.lanes[$key].operatorMarkers[].sha256]
        | all(test("^[0-9a-f]{64}$")))
      and ([.lanes[$key].operatorMarkers[].sha256] | unique | length == 3)
    ' "$aggregate" >/dev/null
}

verify_mac_lane() {
  local evidence_root="$1"
  local aggregate="$2"
  local lane_name="$3"
  local aggregate_lane_key="$4"
  local result_sha256="$5"
  local lane_root="$evidence_root/macos/$lane_name"
  local result="$lane_root/helper-results.json"
  local log_file log_sha256 expected_inventory_count actual_inventory_count

  expected_inventory_count=8
  [[ "$lane_name" == quiet ]] || expected_inventory_count=11
  actual_inventory_count="$(find "$lane_root" -type f | wc -l | tr -d ' ')"
  test "$actual_inventory_count" = "$expected_inventory_count" \
    || die "macOS $lane_name lane does not have its exact $expected_inventory_count-file inventory"

  assert_json_hash "$result" "$result_sha256"
  add_allowed "macos/$lane_name/helper-results.json"
  test -f "$MACOS_CANDIDATE_FACTS" || die "independent macOS package facts are unavailable"
  jq -e \
    --arg version "$EVIDENCE_PRODUCT_VERSION" \
    --arg release_tag "$RELEASE_TAG" \
    --arg lane "$lane_name" \
    --arg source_sha "$VERIFIED_SOURCE_SHA" \
    --arg run_id "$CANDIDATE_RUN_ID" \
    --arg run_attempt "$CANDIDATE_RUN_ATTEMPT" \
    --arg artifact_id "$(jq -er '.releaseCandidateArtifactId' "$RECEIPT_FILE")" \
    --arg artifact_zip_sha256 "$(jq -er '.releaseCandidateArtifactZipSha256' "$RECEIPT_FILE")" \
    --arg manifest_sha256 "$(jq -er '.checksumManifestSha256' "$RECEIPT_FILE")" \
    --arg archive_name "local-browser-bridge-v${EVIDENCE_PRODUCT_VERSION}-macos-universal.tar.gz" \
    --arg archive_sha256 "$(manifest_asset_sha256 "$CANDIDATE_DIR/SHA256SUMS.txt" "local-browser-bridge-v${EVIDENCE_PRODUCT_VERSION}-macos-universal.tar.gz")" \
    --arg acceptance_finalizer_sha256 "$(sha256_file scripts/finalize-macos-acceptance.mjs)" \
    --slurpfile package_facts "$MACOS_CANDIDATE_FACTS" '
      .schemaVersion == 8
      and .productVersion == $version
      and .status == "passed-release-candidate"
      and .evidenceClass == "exact-release-candidate-package-live-observation"
      and (.startedAt | type) == "string"
      and (.capturedAt | type) == "string"
      and (.releaseCandidateBinding == {
        schemaVersion: 3,
        version: $version,
        releaseTag: $release_tag,
        repository: "flrngel/local-browser-bridge",
        sourceSha: $source_sha,
        workflowRunId: $run_id,
        workflowRunAttempt: $run_attempt,
        workflowEvent: "workflow_dispatch",
        workflowRef: "refs/heads/main",
        workflowPath: ".github/workflows/deploy.yml",
        artifactId: $artifact_id,
        artifactZipSha256: $artifact_zip_sha256,
        checksumManifestSha256: $manifest_sha256
      })
      and .harnessSourceBinding.sourceSha == $source_sha
      and .harnessSourceBinding.detachedHead == true
      and .harnessSourceBinding.cleanTrackedAndUntracked == true
      and .harnessSourceBinding.fsckPassed == true
      and .harnessSourceBinding.exactTrackedHarnessBlobs == true
      and .harness.acceptanceFinalizerSha256 == $acceptance_finalizer_sha256
      and .capabilityBinding.inputDeliveryProvenanceV1 == true
      and .capabilityBinding.pointerActivityMonitorV1 == true
      and .package.checksumManifest.expectedSha256Matched == true
      and .package.checksumManifest.actualSha256 == $manifest_sha256
      and .package.archive.file == $archive_name
      and .package.archive.sha256 == $archive_sha256
      and .package.archive.checksumManifestMatched == true
      and .package.archive.checksumManifestSha256 == $manifest_sha256
      and .package.archive.canonicalEntryMatched == true
      and .package.archive.extractedInputsMatched == true
      and .package.serverVersion == $package_facts[0].serverVersion
      and .package.helperVersion == $package_facts[0].helperVersion
      and .package.helperBundleVersion == $package_facts[0].helperVersion
      and .package.helperBundleBuildVersion == $package_facts[0].helperVersion
      and .package.serverSha256 == $package_facts[0].serverSha256
      and .package.helperSha256 == $package_facts[0].helperSha256
      and .package.serverArchitectures == $package_facts[0].serverArchitectures
      and .package.helperArchitectures == $package_facts[0].helperArchitectures
      and .package.strictCodeSignatureVerification == "passed"
      and .quietSeatStabilization.requiredStableMilliseconds == 30000
      and .quietSeatStabilization.maximumWaitMilliseconds == 1800000
      and .quietSeatStabilization.sampleIntervalMilliseconds == 500
      and .quietSeatStabilization.requiredStableTransitions == 60
      and .quietSeatStabilization.required == true
      and .quietSeatStabilization.completed == true
      and .quietSeatStabilization.completedBeforeCandidateExecution == true
      and .quietSeatStabilization.stableDurationMilliseconds >= 30000
      and .quietSeatStabilization.stableDurationMilliseconds <= 1800000
      and .quietSeatStabilization.observedSamples >= 61
      and .quietSeatStabilization.stableTransitions >= 60
      and .quietSeatStabilization.resetCount >= 0
      and .quietSeatStabilization.monitoringUnknown == false
      and .quietSeatStabilization.rawPointerDataRetained == false
      and .assertions.failed == 0
      and .assertions.passed == .assertions.total
      and .assertions.total > 0
      and ([.assertions.details[] | select(.passed != true)] | length == 0)
      and (.screenshots | type == "array" and length == 6)
    ' "$result" >/dev/null || die "macOS $lane_name result failed its pass, binding, or lane invariants"
  validate_macos_pointer_app_share_contract "$result" "$lane_name" \
    || die "macOS $lane_name pointer/app-share contract is noncanonical"
  validate_macos_authority_assertion_contract "$result" "$lane_name" \
    || die "macOS $lane_name authority assertion contract is noncanonical"

  log_file="$(jq -er --arg key "$aggregate_lane_key" '.lanes[$key].logFile' "$aggregate")"
  log_sha256="$(jq -er --arg key "$aggregate_lane_key" '.lanes[$key].logSha256' "$aggregate")"
  [[ "$log_file" =~ ^helper-(rig|evidence)\.log$ ]] || die "macOS $lane_name aggregate has a noncanonical log filename"
  verify_file_fact "$lane_root/$log_file" "$log_sha256" "$(wc -c < "$lane_root/$log_file" | tr -d ' ')"
  add_allowed "macos/$lane_name/$log_file"

  local screenshot_rows screenshot_count=0
  screenshot_rows="$(jq -er --arg key "$aggregate_lane_key" '.lanes[$key].screenshots[] | [.file, .sha256, .pixelSha256, (.bytes|tostring), (.width|tostring), (.height|tostring)] | @tsv' "$aggregate")"
  while IFS=$'\t' read -r filename expected_sha256 expected_pixel_sha256 expected_bytes width height; do
    test -n "$filename" || continue
    screenshot_count=$((screenshot_count + 1))
    [[ "$filename" =~ ^computer-0[1-6]-[a-z0-9-]+\.png$ ]] || die "macOS screenshot filename is noncanonical: $filename"
    [[ "$width" =~ ^[1-9][0-9]*$ && "$height" =~ ^[1-9][0-9]*$ ]] || die "macOS screenshot dimensions are invalid"
    is_sha256 "$expected_pixel_sha256" || die "macOS screenshot decoded-pixel hash is invalid"
    verify_file_fact "$lane_root/$filename" "$expected_sha256" "$expected_bytes"
    verify_png_dimensions "$lane_root/$filename" "$width" "$height" "$expected_pixel_sha256"
    jq -e \
      --arg file "$filename" \
      --arg sha256 "$expected_sha256" \
      --argjson bytes "$expected_bytes" \
      --argjson width "$width" \
      --argjson height "$height" '
        [.screenshots[] | select(
          .file == $file and .sha256 == $sha256 and .bytes == $bytes
          and .width == $width and .height == $height
        )] | length == 1
      ' "$result" >/dev/null || die "macOS aggregate screenshot does not match its lane result: $filename"
    add_allowed "macos/$lane_name/$filename"
  done <<< "$screenshot_rows"
  test "$screenshot_count" = 6 || die "macOS $lane_name aggregate must bind exactly six screenshots"
  jq -e --arg key "$aggregate_lane_key" '[.lanes[$key].screenshots[].sha256] | unique | length == 6' "$aggregate" >/dev/null \
    || die "macOS $lane_name screenshots are not six byte-distinct captures"
  test "$(jq -r --arg key "$aggregate_lane_key" '.lanes[$key].resultFile' "$aggregate")" = helper-results.json \
    || die "macOS aggregate result filename is noncanonical"
  test "$(jq -r --arg key "$aggregate_lane_key" '.lanes[$key].resultSha256' "$aggregate")" = "$result_sha256" \
    || die "macOS aggregate lane hash does not match the receipt"
  test "$(jq -r --arg key "$aggregate_lane_key" '.lanes[$key].startedAt' "$aggregate")" = "$(jq -r '.startedAt' "$result")" \
    || die "macOS aggregate lane start timestamp differs from its raw result"
  test "$(jq -r --arg key "$aggregate_lane_key" '.lanes[$key].capturedAt' "$aggregate")" = "$(jq -r '.capturedAt' "$result")" \
    || die "macOS aggregate lane completion timestamp differs from its raw result"

  if [[ "$lane_name" == quiet ]]; then
    jq -e --arg key "$aggregate_lane_key" '.lanes[$key].operatorMarkers == []' "$aggregate" >/dev/null \
      || die "quiet macOS lane must not retain operator markers"
  else
    local request_relative="operator/macos-app-share-concurrency-handoff-request.json"
    local start_relative="operator/macos-app-share-concurrency-handoff-start.json"
    local complete_relative="operator/macos-app-share-concurrency-handoff-complete.json"
    local request_marker="$lane_root/$request_relative"
    local start_marker="$lane_root/$start_relative"
    local complete_marker="$lane_root/$complete_relative"
    local request_sha256 start_sha256 complete_sha256
    validate_macos_app_share_operator_inventory "$aggregate" "$aggregate_lane_key" \
      || die "deliberate macOS lane exact-app-share marker inventory is noncanonical"
    request_sha256="$(jq -er --arg key "$aggregate_lane_key" '.lanes[$key].operatorMarkers[0].sha256' "$aggregate")"
    start_sha256="$(jq -er --arg key "$aggregate_lane_key" '.lanes[$key].operatorMarkers[1].sha256' "$aggregate")"
    complete_sha256="$(jq -er --arg key "$aggregate_lane_key" '.lanes[$key].operatorMarkers[2].sha256' "$aggregate")"
    validate_macos_app_share_marker_chain \
      "$request_marker" "$start_marker" "$complete_marker" \
      "$request_sha256" "$start_sha256" "$complete_sha256" "$result" \
      || die "macOS exact-app-share request/start/complete chain failed its exact schema, hash, or timestamp binding"
    add_allowed "macos/$lane_name/$request_relative"
    add_allowed "macos/$lane_name/$start_relative"
    add_allowed "macos/$lane_name/$complete_relative"
  fi
}

validate_windows_step_inventory() {
  local summary="$1"
  local expected="$2"
  jq -e --argjson expected "$expected" '
      (.steps | type == "array" and length == 62)
      and ([.steps[].evidence] == $expected)
      and ([.steps[] | select(.passed != true)] | length == 0)
      and ([.steps[].evidence] | unique | length == 62)
    ' "$summary" >/dev/null
}

validate_windows_arm_pair_identity() {
  local request="$1"
  local received="$2"
  jq -e '
      (keys_unsorted | length) == 33
      and (.requestId | type == "string" and test("^[0-9a-f]{32}$"))
      and .status == "action-required" and .maximumClickAttempts == 1
      and .inputStateAtPublication == "not-started"
    ' "$request" >/dev/null \
    && jq -e --slurpfile request "$request" '
      (keys_unsorted | length) == 20
      and .requestId == $request[0].requestId
      and .stableSamplesRequired == 3
      and (.stableSamplesObserved | type == "number" and . >= 3 and . <= 1000)
    ' "$received" >/dev/null
}

validate_global_mac_screenshot_hashes() {
  jq -e '
      ([.lanes.quiet.screenshots[].sha256] + [.lanes.deliberateConcurrency.screenshots[].sha256]) as $hashes
      | ([.lanes.quiet.screenshots[].pixelSha256] + [.lanes.deliberateConcurrency.screenshots[].pixelSha256]) as $pixel_hashes
      | (($hashes | length) == 12 and ($hashes | unique | length) == 12
          and ($pixel_hashes | length) == 12 and ($pixel_hashes | unique | length) == 12)
    ' "$1" >/dev/null
}

validate_stock_chrome_matrix_identity() {
  jq -e '
      .methodCount == 25 and (.methods | length) == 25
      and ([.methods[].name] == [
        "status","browser.control.start","browser.control.status","browser.control.stop",
        "tabs.list","tabs.activate","tabs.new","tabs.close","page.observe","page.navigate",
        "page.back","page.forward","page.reload","page.click","page.fill","page.select",
        "page.key","page.scroll","page.clickAt","page.typeText","page.evaluate","page.waitFor",
        "page.hover","page.batch","page.handleDialog"
      ])
      and ([.methods[] | select(.passed != true or .commandInvoked != true or .resultVerified != true or .postconditionVerified != true)] | length == 0)
    ' "$1" >/dev/null
}

verify_windows_computer() {
  local evidence_root="$1"
  local manifest="$2"
  local summary="$evidence_root/windows/computer/summary.json"
  local expected_sha256
  declare -A windows_screenshots=()
  expected_sha256="$(jq -er '.windowsResultSha256' "$RECEIPT_FILE")"
  assert_json_hash "$summary" "$expected_sha256"
  add_allowed "windows/computer/summary.json"

  local version="$EVIDENCE_PRODUCT_VERSION"
  local server_name="local-browser-bridge-v${version}-windows-x86_64.exe"
  local helper_name="local-computer-helper-v${version}-windows-x86_64.exe"
  local expected_steps expected_screenshots fixture_source_sha256
  fixture_source_sha256="$(sha256_file tests/fixtures/windows/WindowsComputerUseFixture.ps1)"
  expected_steps="$(printf '%s\n' \
    01-protocol-bound-helper-readiness.json \
    02-foreground-arm-request-delivery.json \
    03-foreground-arm-proof.json \
    04-post-arm-protocol-bound-helper-continuity.json \
    05-baseline-exact-window-observe.json \
    06-baseline-screenshot.json \
    07-one-shot-share-pump-stall-start.json \
    08-share-pump-watchdog-causality-proof.json \
    09-replacement-protocol-bound-helper-readiness.json \
    10-replacement-worker-fresh-observe.json \
    11-replacement-worker-fresh-observe-screenshot.json \
    12-replacement-worker-fresh-share-start.json \
    13-replacement-worker-fresh-share-screenshot.json \
    14-replacement-worker-fresh-share-stop.json \
    15-disposable-worker-recovery-proof.json \
    16-semantic-set-value.json \
    17-semantic-set-value-screenshot.json \
    18-semantic-invoke.json \
    19-semantic-invoke-screenshot.json \
    20-background-type-text.json \
    21-background-type-text-screenshot.json \
    22-background-key.json \
    23-background-key-f6-screenshot.json \
    24-background-system-key.json \
    25-background-system-key-alt-a-screenshot.json \
    26-key-message-lparam-proof.json \
    27-pixel-move.json \
    28-pixel-move-screenshot.json \
    29-pixel-click.json \
    30-pixel-click-screenshot.json \
    31-pixel-double-click.json \
    32-pixel-double-click-screenshot.json \
    33-pixel-drag.json \
    34-pixel-drag-screenshot.json \
    35-pixel-scroll.json \
    36-pixel-scroll-screenshot.json \
    37-sanitized-desktop-crop-before-share.json \
    38-native-share-start.json \
    39-windows-capture-indicator-and-host-provenance.json \
    40-native-share-frame-progression.json \
    41-native-share-action.json \
    42-native-share-after-action-screenshot.json \
    43-native-share-stop.json \
    44-cancellation-native-share-start.json \
    45-explicit-cancellation-live-frame-screenshot.json \
    46-pre-cancellation-protocol-bound-helper-readiness.json \
    47-explicit-cancellation-in-progress-duplicate.json \
    48-explicit-cancellation-accepted.json \
    49-explicit-cancellation-original-outcome.json \
    50-explicit-cancellation-duplicate-refused.json \
    51-explicit-cancellation-cached-replay.json \
    52-explicit-cancellation-changed-request-refused.json \
    53-explicit-cancellation-screenshot-removed.json \
    54-post-cancellation-protocol-bound-helper-readiness.json \
    55-explicit-cancellation-idempotent-share-stop.json \
    56-explicit-cancellation-replacement-has-no-frame-before-observe.json \
    57-explicit-cancellation-fresh-recovery-observe.json \
    58-explicit-cancellation-stale-frame-after-recovery.json \
    59-explicit-cancellation-recovered-action.json \
    60-explicit-cancellation-recovered-action-screenshot.json \
    61-explicit-cancellation-authority-and-recovery-proof.json \
    62-foreground-cursor-focus-desktop-invariants.json | jq -Rsc 'split("\n")[:-1]')"
  expected_screenshots="$(printf '%s\n' \
    00-baseline-observe.png 05-recovery-fresh-observe.png 06-recovery-fresh-share.png \
    10-semantic-set-value.png 11-semantic-invoke.png 20-background-type-text.png \
    21-background-key-f6.png 22-background-system-key-alt-a.png 30-pixel-move.png \
    31-pixel-click.png 32-pixel-double-click.png 33-pixel-drag.png 34-pixel-scroll.png \
    39-desktop-crop-before-share.png 40-native-share-frame-1.png 41-native-share-frame-2.png \
    41-desktop-crop-during-share.png 42-native-share-after-action.png \
    50-explicit-cancel-live-frame.png 51-explicit-cancel-recovered-action.png \
    | jq -Rsc 'split("\n")[:-1]')"
  jq -e \
    --arg version "$version" \
    --arg manifest_sha256 "$(jq -er '.checksumManifestSha256' "$RECEIPT_FILE")" \
    --arg server_name "$server_name" \
    --arg server_sha256 "$(manifest_asset_sha256 "$manifest" "$server_name")" \
    --arg helper_name "$helper_name" \
    --arg helper_sha256 "$(manifest_asset_sha256 "$manifest" "$helper_name")" \
    --arg fixture_source_sha256 "$fixture_source_sha256" \
    --argjson expected_steps "$expected_steps" '
      .schemaVersion == 2
      and .passed == true
      and .failure == null
      and .failureDetails == null
      and (.cleanupIssues == [])
      and .tokenPersistenceVerified == true
      and .tokenPersisted == false
      and .tokenBearingEvidenceRemoved == 0
      and .unrelatedProcessesTerminated == false
      and .recoveryEventReleased == true
      and (.suites == ["Smoke", "Recovery", "Semantic", "Keyboard", "Pixel", "Capture", "Cancellation"])
      and (.startedAtUtc | type == "string")
      and (.finishedAtUtc | type == "string")
      and (.steps | type == "array" and length == 62)
      and ([.steps[].evidence] == $expected_steps)
      and ([.steps[] | select(.passed != true)] | length == 0)
      and ([.steps[].evidence] | unique | length == (.steps | length))
      and .foregroundArmProof.completed == true
      and .foregroundArmProof.fixtureRequestCount == 1
      and .foregroundArmProof.fixtureAcknowledgementCount == 1
      and .foregroundArmProof.fixtureLeftMouseDownCount == 1
      and .foregroundArmProof.fixtureLeftMouseUpCount == 1
      and .foregroundArmProof.armAndNativeMatched == true
      and .foregroundArmProof.baselineContinuityMatched == true
      and .fixtureProcessBinding.executionMode == "dedicated-windows-application"
      and .fixtureProcessBinding.appUserModelId == "LocalBrowserBridge.WindowsAcceptance"
      and .fixtureProcessBinding.sourceScriptSha256 == $fixture_source_sha256
      and .fixtureProcessBinding.sourceStableAcrossBuild == true
      and (.fixtureProcessBinding.executableBytes | type == "number" and . > 0 and . <= 20971520)
      and (.fixtureProcessBinding.executableSha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and .fixtureProcessBinding.executableStableAcrossLaunch == true
      and .fixtureProcessBinding.entryPointSelfTestPassed == true
      and .fixtureProcessBinding.directChildMatched == true
      and .fixtureProcessBinding.exactImageMatched == true
      and .fixtureProcessBinding.interactiveSessionMatched == true
      and .fixtureProcessBinding.readyPidMatched == true
      and .fixtureProcessBinding.executableRemoved == true
      and .fixtureProcessBinding.terminalHostUsed == false
      and .fixtureProcessBinding.pathsRecorded == false
      and .candidateBinding.version == $version
      and .candidateBinding.checksumManifestMatched == true
      and .candidateBinding.exactAssetSetMatched == true
      and .candidateBinding.checksumManifest.sha256 == $manifest_sha256
      and .candidateBinding.checksumManifest.expectedSha256 == $manifest_sha256
      and .candidateBinding.server.name == $server_name
      and .candidateBinding.server.sha256 == $server_sha256
      and .candidateBinding.server.checksumManifestMatched == true
      and .candidateBinding.server.versionMatched == true
      and .candidateBinding.helper.name == $helper_name
      and .candidateBinding.helper.sha256 == $helper_sha256
      and .candidateBinding.helper.checksumManifestMatched == true
      and .candidateBinding.helper.versionMatched == true
    ' "$summary" >/dev/null || die "Windows computer result failed its pass, binding, cleanup, or arm invariants"
  validate_windows_step_inventory "$summary" "$expected_steps" \
    || die "Windows computer result does not contain the exact ordered Suite=All step inventory"
  assert_release_candidate_binding "$summary" '.releaseCandidateBinding'
  assert_utc_interval "$(jq -er '.startedAtUtc' "$summary")" "$(jq -er '.finishedAtUtc' "$summary")" 7200 \
    "Windows computer acceptance"

  local fixed
  for fixed in fixture/fixture-events.ndjson fixture/fixture-ready.json fixture/fixture-state.json \
    operator/foreground-arm-request.json operator/foreground-arm-received.json; do
    test -f "$evidence_root/windows/computer/$fixed" || die "Windows evidence sidecar is absent: $fixed"
    add_allowed "windows/computer/$fixed"
  done
  jq -e '
      (keys_unsorted == ["schemaVersion","processId","targetHwnd","surfaceHwnd","sentinelHwnd","armButtonHwnd","occluderHwnd","backdropHwnd","occluderEnabled"])
      and .schemaVersion == 1 and (.processId | type == "number" and . > 0)
      and (([.targetHwnd,.surfaceHwnd,.sentinelHwnd,.armButtonHwnd,.occluderHwnd,.backdropHwnd]
        | all(type == "string" and test("^[1-9][0-9]*$"))))
      and (([.targetHwnd,.surfaceHwnd,.sentinelHwnd,.armButtonHwnd,.occluderHwnd,.backdropHwnd]
        | unique | length) == 6)
      and .occluderEnabled == true
    ' "$evidence_root/windows/computer/fixture/fixture-ready.json" >/dev/null \
    || die "Windows fixture ready sidecar is noncanonical"
  jq -e '
      (keys_unsorted == [
        "schemaVersion","statePublicationGeneration","utc","processId","uptimeMs","ready",
        "targetHwnd","surfaceHwnd","sentinelHwnd","armButtonHwnd","occluderHwnd","backdropHwnd",
        "foregroundHwnd","targetBounds","surfaceScreenBounds","cursor","animationFrame","invokeCount",
        "semanticValue","focusedText","messageCounters","targetActivatedCount","sentinelActivatedCount",
        "sentinelDeactivatedCount","foregroundArmRequestedGeneration","foregroundArmAcknowledgedGeneration",
        "foregroundArmRequestCount","foregroundArmAcknowledgementCount","foregroundArmLeftMouseDownCount",
        "foregroundArmLeftMouseUpCount","foregroundArmButtonEnabled","eventSequence","occluderEnabled"
      ])
      and .schemaVersion == 1 and .ready == true and .occluderEnabled == true
      and (.statePublicationGeneration | type == "number" and . > 0)
      and (.utc | type == "string")
      and .foregroundArmRequestedGeneration > 0
      and .foregroundArmAcknowledgedGeneration == .foregroundArmRequestedGeneration
      and .foregroundArmRequestCount == 1 and .foregroundArmAcknowledgementCount == 1
      and .foregroundArmLeftMouseDownCount == 1 and .foregroundArmLeftMouseUpCount == 1
      and .foregroundArmButtonEnabled == true
      and (.eventSequence | type == "number" and . > 0)
    ' "$evidence_root/windows/computer/fixture/fixture-state.json" >/dev/null \
    || die "Windows fixture final state does not prove the exact one-shot foreground arm"
  jq -e -s '
      length >= 8
      and ([range(0; length) as $i | .[$i].sequence == ($i + 1)] | all)
      and ([.[] | select(.schemaVersion != 1 or (.utc | type) != "string")] | length == 0)
      and ([.[] | select(.event == "fatalError")] | length == 0)
      and ([.[] | select(.source == "fixture" and .event == "ready")] | length == 1)
      and ([.[] | select(.source == "sentinel" and .event == "foregroundArmRequested")] | length == 1)
      and ([.[] | select(.source == "sentinel" and .event == "foregroundArmAcknowledged")] | length == 1)
    ' "$evidence_root/windows/computer/fixture/fixture-events.ndjson" >/dev/null \
    || die "Windows fixture event stream is incomplete, reordered, or contains a fatal event"
  jq -e --arg version "$version" '
      (keys_unsorted == [
        "schemaVersion","productVersion","kind","status","requestId","publishedAtUtc","timeoutSeconds",
        "operatorActionRequired","preferredRelaySurface","fallbackRelaySurface","expectedVisibleWindowTitle",
        "expectedVisibleButtonText","expectedAccessibleName","action","stopUiAfterAction",
        "requiresSeparateAuthorization","markerGrantsAuthorization","markerGrantsConsent",
        "externalOneShotConsentRequired","visualConfirmationRequired","maximumClickAttempts",
        "retryOnUnknownOutcome","instruction","requestDelivered","buttonEnabled","nativeTopologyMatched",
        "inputStateAtPublication","notificationOnly","acceptedAsAuthority","rawWindowHandlesRecorded",
        "rawCursorCoordinatesRecorded","pathsRecorded","secretsRecorded"
      ])
      and .schemaVersion == 2 and .productVersion == $version and .kind == "foreground-arm"
      and .status == "action-required"
      and (.requestId | type == "string" and test("^[0-9a-f]{32}$"))
      and (.publishedAtUtc | type == "string")
      and (.timeoutSeconds | type == "number" and . >= 15 and . <= 300)
      and .operatorActionRequired == true
      and .preferredRelaySurface == "windows-computer-use-app-share"
      and .fallbackRelaySurface == "human-on-windows-session"
      and .expectedVisibleWindowTitle == "LBB Foreground Sentinel"
      and .expectedVisibleButtonText == "CLICK TO ARM"
      and .expectedAccessibleName == "Click to arm Windows acceptance"
      and .action == "single-left-click"
      and .stopUiAfterAction == true and .requiresSeparateAuthorization == true
      and .maximumClickAttempts == 1
      and .retryOnUnknownOutcome == false
      and .instruction == "Use a separately authorized Windows Computer Use app share to visually confirm this exact window and button, click it once, then stop all UI use. If it already says ARMED or the outcome is uncertain, do not click or retry."
      and .requestDelivered == true and .buttonEnabled == true and .nativeTopologyMatched == true
      and .inputStateAtPublication == "not-started"
      and .notificationOnly == true and .acceptedAsAuthority == false
      and .markerGrantsAuthorization == false and .markerGrantsConsent == false
      and .externalOneShotConsentRequired == true and .visualConfirmationRequired == true
      and .rawWindowHandlesRecorded == false and .rawCursorCoordinatesRecorded == false
      and .secretsRecorded == false and .pathsRecorded == false
    ' "$evidence_root/windows/computer/operator/foreground-arm-request.json" >/dev/null
  jq -e --arg version "$version" \
    --slurpfile request "$evidence_root/windows/computer/operator/foreground-arm-request.json" '
      (keys_unsorted == [
        "schemaVersion","productVersion","kind","status","requestId","receivedAtUtc",
        "exactClickCountsMatched","stableSamplesObserved","stableSamplesRequired","nativeTopologyMatched",
        "foregroundMatched","focusMatched","cursorStable","inputDesktopStable","notificationOnly",
        "acceptedAsAuthority","rawWindowHandlesRecorded","rawCursorCoordinatesRecorded","pathsRecorded","secretsRecorded"
      ])
      and .schemaVersion == 2 and .productVersion == $version and .kind == "foreground-arm"
      and .status == "received" and .exactClickCountsMatched == true
      and .requestId == $request[0].requestId
      and (.receivedAtUtc | type == "string")
      and .stableSamplesRequired == 3
      and (.stableSamplesObserved | type == "number" and . >= 3 and . <= 1000)
      and .nativeTopologyMatched == true and .foregroundMatched == true and .focusMatched == true
      and .cursorStable == true and .inputDesktopStable == true
      and .notificationOnly == true and .acceptedAsAuthority == false
      and .rawWindowHandlesRecorded == false and .rawCursorCoordinatesRecorded == false
      and .secretsRecorded == false and .pathsRecorded == false
    ' "$evidence_root/windows/computer/operator/foreground-arm-received.json" >/dev/null
  validate_windows_arm_pair_identity \
    "$evidence_root/windows/computer/operator/foreground-arm-request.json" \
    "$evidence_root/windows/computer/operator/foreground-arm-received.json" \
    || die "Windows foreground-arm request/receipt pair identity is incomplete or replayable"
  local arm_published arm_received arm_timeout
  arm_published="$(jq -er '.publishedAtUtc' "$evidence_root/windows/computer/operator/foreground-arm-request.json")"
  arm_received="$(jq -er '.receivedAtUtc' "$evidence_root/windows/computer/operator/foreground-arm-received.json")"
  arm_timeout="$(jq -er '.timeoutSeconds' "$evidence_root/windows/computer/operator/foreground-arm-request.json")"
  assert_utc_interval "$arm_published" "$arm_received" "$arm_timeout" "Windows foreground-arm marker"
  python3 - "$summary" "$evidence_root/windows/computer/fixture/fixture-events.ndjson" \
    "$arm_published" "$arm_received" <<'PY'
import datetime, hashlib, json, sys
summary_path, events_path, published, received = sys.argv[1:]
def instant(value):
    if not isinstance(value, str) or not value.endswith("Z"):
        raise SystemExit("Windows evidence timestamp is not canonical UTC")
    return datetime.datetime.fromisoformat(value[:-1] + "+00:00")
summary = json.load(open(summary_path, encoding="utf-8"))
started, finished = instant(summary["startedAtUtc"]), instant(summary["finishedAtUtc"])
published_at, received_at = instant(published), instant(received)
if not (started <= published_at <= received_at <= finished):
    raise SystemExit("Windows foreground-arm markers are outside the current acceptance run")
events = [json.loads(line) for line in open(events_path, encoding="utf-8") if line.strip()]
event_times = [instant(item["utc"]) for item in events]
if event_times != sorted(event_times) or event_times[0] < started or event_times[-1] > finished + datetime.timedelta(seconds=30):
    raise SystemExit("Windows fixture events are not ordered and bounded to the current run")
PY

  local step_name step_path screenshot_rows filename screenshot_sha screenshot_bytes
  while IFS= read -r step_name; do
    [[ "$step_name" =~ ^[0-9][0-9]-[a-z0-9-]+\.json$ ]] || die "Windows step filename is noncanonical: $step_name"
    step_path="$evidence_root/windows/computer/steps/$step_name"
    test -f "$step_path" || die "Windows summary references an absent step: $step_name"
    jq -e . "$step_path" >/dev/null || die "Windows step is invalid JSON: $step_name"
    add_allowed "windows/computer/steps/$step_name"
    screenshot_rows="$(jq -r '.. | objects | select((.file? | type) == "string" and (.sha256? | type) == "string" and (.bytes? | type) == "number") | [.file, .sha256, (.bytes|tostring)] | @tsv' "$step_path")"
    while IFS=$'\t' read -r filename screenshot_sha screenshot_bytes; do
      test -n "$filename" || continue
      [[ "$filename" =~ ^[0-9][0-9]-[a-z0-9-]+\.png$ ]] || die "Windows screenshot filename is noncanonical: $filename"
      test -z "${windows_screenshots[$filename]:-}" || die "Windows step evidence reuses a screenshot sidecar: $filename"
      windows_screenshots["$filename"]=1
      verify_file_fact "$evidence_root/windows/computer/screenshots/$filename" "$screenshot_sha" "$screenshot_bytes"
      add_allowed "windows/computer/screenshots/$filename"
    done <<< "$screenshot_rows"
  done < <(jq -er '.steps[].evidence' "$summary")
  test "${#windows_screenshots[@]}" = 20 || die "Windows acceptance did not bind exactly twenty required screenshots"
  local expected_name
  while IFS= read -r expected_name; do
    test -n "${windows_screenshots[$expected_name]:-}" \
      || die "Windows acceptance omitted required screenshot: $expected_name"
  done < <(jq -r '.[]' <<< "$expected_screenshots")
}

verify_stock_chrome() {
  local evidence_root="$1"
  local manifest="$2"
  local browser_root="$evidence_root/windows/browser"
  local final="$browser_root/browser-acceptance.json"
  local expected_sha256
  expected_sha256="$(jq -er '.stockChromeResultSha256' "$RECEIPT_FILE")"
  assert_json_hash "$final" "$expected_sha256"

  local fixed_names=(
    browser-acceptance.json
    candidate-preflight.json
    candidate-postflight.json
    browser-api-matrix.json
    browser-computer-helper-chain.json
    scoped-action-approval.json
    independent-visual-review.json
    external-surface-preflight.json
    external-surface-postflight.json
    operator-results.json
    browser-01-extension-loaded.json
    browser-01-extension-loaded.png
    browser-02-api-action-result.json
    browser-02-api-action-result.png
    browser-03-computer-share-action.json
    browser-03-computer-share-action.png
    browser-04-stop-paused.json
    browser-04-stop-paused.png
    browser-05-cancel-paused.json
    browser-05-cancel-paused.png
    browser-06-post-handback-resume.json
    browser-06-post-handback-resume.png
  )
  local name
  for name in "${fixed_names[@]}"; do
    test -f "$browser_root/$name" || die "stock-Chrome evidence file is absent: $name"
    add_allowed "windows/browser/$name"
  done

  local version="$EVIDENCE_PRODUCT_VERSION"
  local server_name="local-browser-bridge-v${version}-windows-x86_64.exe"
  local helper_name="local-computer-helper-v${version}-windows-x86_64.exe"
  local extension_name="local-browser-bridge-extension-v${version}.zip"
  jq -e \
    --arg version "$version" \
    --arg source_sha "$VERIFIED_SOURCE_SHA" \
    --arg manifest_sha256 "$(jq -er '.checksumManifestSha256' "$RECEIPT_FILE")" \
    --arg server_name "$server_name" \
    --arg server_sha256 "$(manifest_asset_sha256 "$manifest" "$server_name")" \
    --arg helper_name "$helper_name" \
    --arg helper_sha256 "$(manifest_asset_sha256 "$manifest" "$helper_name")" \
    --arg extension_name "$extension_name" \
    --arg extension_sha256 "$(manifest_asset_sha256 "$manifest" "$extension_name")" '
      .schemaVersion == 3
      and .evidenceType == "stock-user-chrome-acceptance"
      and .passed == true
      and .candidateBinding.finalSha == $source_sha
      and .candidateBinding.checksumManifestSha256 == $manifest_sha256
      and .candidate.version == $version
      and .candidate.finalSha == $source_sha
      and .candidate.checksumManifestSha256 == $manifest_sha256
      and .candidate.serverName == $server_name
      and .candidate.serverSha256 == $server_sha256
      and .candidate.computerHelperName == $helper_name
      and .candidate.computerHelperSha256 == $helper_sha256
      and .candidate.extensionZipName == $extension_name
      and .candidate.extensionZipSha256 == $extension_sha256
      and .apiMatrix.passed == true
      and .computerHelper.passed == true
      and (.screenshots | type == "array" and length == 6)
      and .scopedActionApproval.response.approvedBy == "user"
      and .scopedActionApproval.response.deliveredBy == "user-via-orchestrator"
      and .scopedActionApproval.response.confirmationMode == "batched-action-time"
      and .scopedActionApproval.response.singleCandidateRun == true
      and .scopedActionApproval.consumption.consumedBeforeFirstCoveredAction == true
      and .scopedActionApproval.consumption.consumedBeforeExpiry == true
      and .scopedActionApproval.consumption.freshStateRevalidatedAfterApproval == true
      and .scopedActionApproval.consumption.scopeUnchangedThroughRun == true
      and .scopedActionApproval.consumption.replayed == false
      and .scopedActionApproval.consumption.cleanupAuthoritySurvivesFailure == true
      and .independentVisualReview.independentSessionBoundary == true
      and (.independentVisualReview.entries | length) == 6
      and .independentVisualReview.aggregate.reviewedCropCount == 6
      and .independentVisualReview.aggregate.everySanitizedCropOpenedByReviewer == true
      and .independentVisualReview.aggregate.allImageDigestsMatched == true
      and .independentVisualReview.aggregate.requiredVisibleStateConfirmedByReviewer == true
      and .independentVisualReview.aggregate.noSensitivePixelsObservedByReviewer == true
      and .independentVisualReview.aggregate.noUncertaintyReported == true
      and .independentVisualReview.aggregate.visualJudgmentNotPixelSafetyProof == true
      and .restoration.candidateExtensionPresence.matchesInitial == true
      and .restoration.candidateExtensionPresence.finalPresent == false
      and .cleanup.controlReleased == true
      and .cleanup.testTabsClosed == true
      and .cleanup.testWindowClosed == true
      and .cleanup.serverStopped == true
      and .cleanup.portReleased == true
      and .cleanup.acceptanceCredentialClearedFromShell == true
      and .privacy.allowlistedSchemaOnly == true
      and .privacy.rawApiResponsesPresentInRetainedEvidence == false
      and .privacy.chromeMcpTranscriptPresentInRetainedEvidence == false
      and .privacy.computerUseTranscriptPresentInRetainedEvidence == false
      and .privacy.filesystemLocationsPresentInRetainedEvidence == false
      and .privacy.browserAccountDetailsPresentInRetainedEvidence == false
    ' "$final" >/dev/null || die "stock-Chrome result failed its pass, binding, review, restoration, or cleanup invariants"
  assert_release_candidate_binding "$final" '.releaseCandidateBinding'

  local preflight="$browser_root/candidate-preflight.json"
  local postflight="$browser_root/candidate-postflight.json"
  local matrix="$browser_root/browser-api-matrix.json"
  local helper="$browser_root/browser-computer-helper-chain.json"
  local approval="$browser_root/scoped-action-approval.json"
  local review="$browser_root/independent-visual-review.json"
  local external_preflight="$browser_root/external-surface-preflight.json"
  local external_postflight="$browser_root/external-surface-postflight.json"
  local operator="$browser_root/operator-results.json"
  for name in "$preflight" "$postflight" "$matrix" "$helper" "$approval" "$review" \
    "$external_preflight" "$external_postflight" "$operator"; do jq -e . "$name" >/dev/null; done
  assert_release_candidate_binding "$preflight" '.releaseCandidateBinding'
  assert_release_candidate_binding "$postflight" '.releaseCandidateBinding'
  assert_release_candidate_binding "$matrix" '.releaseCandidateBinding'
  assert_release_candidate_binding "$helper" '.releaseCandidateBinding'
  assert_release_candidate_binding "$approval" '.releaseCandidateBinding'
  assert_release_candidate_binding "$review" '.releaseCandidateBinding'
  assert_release_candidate_binding "$external_preflight" '.releaseCandidateBinding'
  assert_release_candidate_binding "$external_postflight" '.releaseCandidateBinding'
  assert_release_candidate_binding "$operator" '.releaseCandidateBinding'
  test "$(sha256_file "$preflight")" = "$(jq -er '.candidate.preflightRecordSha256' "$final")" || die "stock-Chrome preflight hash mismatch"
  test "$(sha256_file "$postflight")" = "$(jq -er '.candidate.postflightRecordSha256' "$final")" || die "stock-Chrome postflight hash mismatch"
  test "$(sha256_file "$matrix")" = "$(jq -er '.apiMatrixRecordSha256' "$final")" || die "stock-Chrome API matrix hash mismatch"
  test "$(sha256_file "$helper")" = "$(jq -er '.computerHelperRecordSha256' "$final")" || die "stock-Chrome helper-chain hash mismatch"
  test "$(sha256_file "$approval")" = "$(jq -er '.scopedApprovalRecordSha256' "$final")" || die "stock-Chrome scoped-approval hash mismatch"
  test "$(sha256_file "$review")" = "$(jq -er '.independentReviewRecordSha256' "$final")" || die "stock-Chrome independent-review hash mismatch"
  test "$(sha256_file "$external_preflight")" = "$(jq -er '.cleanup.externalSurfacePreflightAttestationSha256' "$operator")" || die "stock-Chrome external-surface preflight hash mismatch"
  test "$(sha256_file "$external_postflight")" = "$(jq -er '.cleanup.externalSurfacePostflightAttestationSha256' "$operator")" || die "stock-Chrome external-surface postflight hash mismatch"
  test "$(sha256_file "$operator")" = "$(jq -er '.operatorRecordSha256' "$final")" || die "stock-Chrome operator-results hash mismatch"
  test "$(jq -cS . "$matrix")" = "$(jq -cS '.apiMatrix' "$final")" || die "embedded stock-Chrome API matrix differs from its sidecar"
  test "$(jq -cS . "$helper")" = "$(jq -cS '.computerHelper' "$final")" || die "embedded stock-Chrome helper record differs from its sidecar"
  test "$(jq -cS . "$approval")" = "$(jq -cS '.scopedActionApproval' "$final")" || die "embedded stock-Chrome scoped approval differs from its sidecar"
  test "$(jq -cS . "$review")" = "$(jq -cS '.independentVisualReview' "$final")" || die "embedded stock-Chrome independent review differs from its sidecar"
  jq -e --arg source_sha "$VERIFIED_SOURCE_SHA" --arg manifest_sha256 "$(jq -er '.checksumManifestSha256' "$RECEIPT_FILE")" \
    '.passed == true and .candidate.finalSha == $source_sha and .candidate.checksumManifest.sha256 == $manifest_sha256' "$preflight" >/dev/null
  jq -e '.passed == true and ([.unchanged[]] | all)' "$postflight" >/dev/null
  assert_utc_interval \
    "$(jq -er '.request.createdAtUtc' "$approval")" \
    "$(jq -er '.request.expiresAtUtc' "$approval")" 1800 \
    "stock-Chrome scoped approval request"
  assert_utc_interval \
    "$(jq -er '.response.confirmedAtUtc' "$approval")" \
    "$(jq -er '.consumption.preDispatchVerifiedAtUtc' "$approval")" 1800 \
    "stock-Chrome approval-before-fresh-state-revalidation"
  assert_utc_interval \
    "$(jq -er '.consumption.preDispatchVerifiedAtUtc' "$approval")" \
    "$(jq -er '.scopedActionApproval.firstCoveredActionDispatchedAtUtc' "$helper")" 1800 \
    "stock-Chrome fresh-state-revalidation-before-first-covered-dispatch"
  assert_utc_interval \
    "$(jq -er '.scopedActionApproval.firstCoveredActionDispatchedAtUtc' "$helper")" \
    "$(jq -er '.request.expiresAtUtc' "$approval")" 1800 \
    "stock-Chrome approval consumption-before-expiry"
  python3 - "$preflight" "$external_preflight" "$helper" "$approval" "$review" "$external_postflight" "$postflight" "$final" <<'PY'
import datetime, hashlib, json, sys
records = [json.load(open(path, encoding="utf-8")) for path in sys.argv[1:]]
def instant(value):
    if not isinstance(value, str) or not __import__("re").fullmatch(
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{7}Z", value
    ):
        raise SystemExit("stock-Chrome timestamp is not canonical UTC")
    return datetime.datetime.fromisoformat(value[:-1] + "+00:00")
preflight, external_preflight, helper, approval, review, external_postflight, postflight, final = records
times = [
    instant(external_preflight["attestedAtUtc"]),
    instant(preflight["recordedAtUtc"]),
    instant(helper["run"]["startedAtUtc"]),
    instant(approval["response"]["confirmedAtUtc"]),
    instant(approval["consumption"]["preDispatchVerifiedAtUtc"]),
    instant(helper["scopedActionApproval"]["firstCoveredActionDispatchedAtUtc"]),
    instant(helper["run"]["finishedAtUtc"]),
    instant(review["reviewedAtUtc"]),
    instant(external_postflight["attestedAtUtc"]),
    instant(postflight["recordedAtUtc"]),
    instant(final["recordedAtUtc"]),
]
approval_expires = instant(approval["request"]["expiresAtUtc"])
if times != sorted(times) or times[3] == times[4] or times[4] == times[5] or (times[-1] - times[0]).total_seconds() > 8 * 60 * 60:
    raise SystemExit("stock-Chrome records are reordered or exceed the bounded acceptance interval")
if times[5] > approval_expires:
    raise SystemExit("stock-Chrome scoped approval expired before first covered dispatch")
if (times[-1] - times[-2]).total_seconds() > 2 * 60 * 60:
    raise SystemExit("stock-Chrome finalization was not prompt after candidate postflight")
for record in (external_preflight, external_postflight):
    canonical = json.dumps(
        record["releaseCandidateBinding"], separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    if hashlib.sha256(canonical).hexdigest() != record["releaseCandidateBindingSha256"]:
        raise SystemExit("stock-Chrome external-surface candidate-binding digest mismatch")
if external_preflight["attestorSessionRef"] != approval["response"]["orchestratorSessionRef"]:
    raise SystemExit("stock-Chrome external-surface attestor does not match approval orchestrator")
PY
  jq -e '
      .schemaVersion == 1
      and .evidenceType == "stock-user-chrome-api-matrix"
      and .version == "0.12.33" and .target == "loopback-demo" and .passed == true
      and .methodCount == 25 and (.methods | length) == 25
      and ([.methods[].name] == [
        "status","browser.control.start","browser.control.status","browser.control.stop",
        "tabs.list","tabs.activate","tabs.new","tabs.close","page.observe","page.navigate",
        "page.back","page.forward","page.reload","page.click","page.fill","page.select",
        "page.key","page.scroll","page.clickAt","page.typeText","page.evaluate","page.waitFor",
        "page.hover","page.batch","page.handleDialog"
      ])
      and ([.methods[].stage] == [
        "preflight","control","control","cleanup","tab-lifecycle","tab-lifecycle","tab-lifecycle",
        "cleanup","freshness","navigation","navigation","navigation","navigation","interaction",
        "interaction","interaction","interaction","interaction","interaction","interaction",
        "inspection","inspection","interaction","interaction","dialog"
      ])
      and ([.methods[].screenshot] == [
        "N/A","browser-01-extension-loaded.png","N/A","N/A","N/A","N/A","N/A","N/A","N/A",
        "N/A","N/A","N/A","N/A","browser-02-api-action-result.png",
        "browser-02-api-action-result.png","browser-02-api-action-result.png","N/A","N/A","N/A","N/A",
        "browser-02-api-action-result.png","N/A","N/A","N/A","N/A"
      ])
      and ([.methods[] | select(
        (keys_unsorted != ["name","passed","stage","commandInvoked","resultVerified","postconditionVerified","screenshot","machineProof"])
        or .passed != true or .commandInvoked != true or .resultVerified != true
        or .postconditionVerified != true or .machineProof != "machine-command-result-postcondition"
      )] | length == 0)
      and (.assertions | keys_unsorted) == [
        "serverVersionMatched","extensionVersionMatched","browserFloorMet","realExtensionConnected",
        "fullAccessEnabled","capabilitiesComplete","freshCommandIdentity","freshObservationAfterPageMutation",
        "dynamicTargetDiscovery","testOwnedTabsOnly","topLayerControlUiIntegrity","dialogLifecycle","cleanupComplete"
      ]
      and ([.assertions[]] | all(. == true))
    ' "$matrix" >/dev/null || die "stock-Chrome API matrix is incomplete, reordered, or lacks a required proof"
  validate_stock_chrome_matrix_identity "$matrix" \
    || die "stock-Chrome API matrix is missing or reorders one of the exact 25 methods"
  jq -e '
      . as $root
      |
      .schemaVersion == 2 and .evidenceType == "stock-user-chrome-computer-helper-chain"
      and .version == "0.12.33" and .passed == true
      and .server.soleListener == "127.0.0.1:17373" and .server.updateCheckDisabled == true
      and .helper.connectedThroughLoopbackServer == true and .helper.serverApiOnly == true
      and .extensionPayload.fileCount == 11
      and (.lifecycle | length) == 8 and (.windowEpochs | length) == 13 and (.actions | length) == 27
      and (.screenshots | length) == 6
      and ([.screenshots[].purpose] == ["extension-loaded","api-action-result","computer-share-action","stop-paused","cancel-paused","post-handback-resume"])
      and ([.screenshots[] | select(.exactWindowFrame != true or .shareFrameFresh != true or .rawImageRetained != false)] | length == 0)
      and .operatorExchange.protocolVersion == 1
      and .operatorExchange.executorSessionRef != .operatorExchange.reviewerSessionRef
      and .operatorExchange.independentSessionBoundary == true
      and .operatorExchange.requestCount == (.operatorExchange.statusDecisionCount + .operatorExchange.freshFrameDecisionCount + 1)
      and .operatorExchange.allRequestsCreateOnce == true and .operatorExchange.allResponsesCreateOnce == true
      and .operatorExchange.everyFrameDependentDecisionBoundToFreshFrame == true
      and .operatorExchange.everyStatusDecisionBoundToFreshStatus == true
      and .operatorExchange.scratchDeleted == true
      and .scopedActionApproval.executorSessionRef == .operatorExchange.executorSessionRef
      and .scopedActionApproval.consumedBeforeFirstCoveredAction == true
      and .scopedActionApproval.consumedBeforeExpiry == true
      and .scopedActionApproval.freshStateRevalidatedAfterApproval == true
      and .scopedActionApproval.approvalChallengeFrameRef != .scopedActionApproval.preDispatchFrameRef
      and .scopedActionApproval.scopeUnchangedThroughRun == true
      and ([.actions[].operatorDecisionRef] | unique | length) == 27
      and ([.actions[] | select(
        (if .riskRef == "none" then .approvalRef != "none"
         else .approvalRef != $root.scopedActionApproval.approvalId end)
      ] | length) == 0
      and .actions[.scopedActionApproval.firstCoveredActionSequence - 1].dispatchedAtUtc == .scopedActionApproval.firstCoveredActionDispatchedAtUtc
      and .cleanup.rawIdentifiersCleared == true and .privacy.credentialRetained == false
      and .privacy.opaqueReferenceMapDiscarded == true
    ' "$helper" >/dev/null || die "stock-Chrome computer-helper chain omits required lifecycle, action, frame, cleanup, or privacy proof"
  jq -e '
      .schemaVersion == 1 and .evidenceType == "stock-user-chrome-scoped-action-approval"
      and (.approvalId | test("^[0-9a-f]{64}$"))
      and (.request.scopeSha256 | test("^[0-9a-f]{64}$"))
      and (.request.challengeFrameRef | test("^[0-9a-f]{64}$"))
      and .request.coveredActions == [
        "conditional-developer-mode-change", "load-and-run-exact-unpacked-candidate",
        "conditional-full-access-change", "save-ephemeral-loopback-credential",
        "clear-ephemeral-loopback-credential", "remove-exact-test-owned-extension",
        "restore-captured-browser-settings", "failure-rollback"
      ]
      and .request.loopbackOnly == true and .request.dedicatedWindowOnly == true
      and .request.restoreCapturedState == true and .request.noUnrelatedExtensionMutation == true
      and .response.approvedBy == "user" and .response.deliveredBy == "user-via-orchestrator"
      and .response.confirmationMode == "batched-action-time" and .response.singleCandidateRun == true
      and .consumption.consumedBeforeFirstCoveredAction == true
      and .consumption.consumedBeforeExpiry == true
      and (.consumption.preDispatchFrameRef | test("^[0-9a-f]{64}$"))
      and (.consumption.preDispatchDecisionRef | test("^[0-9a-f]{64}$"))
      and .consumption.freshStateRevalidatedAfterApproval == true
      and .request.challengeFrameRef != .consumption.preDispatchFrameRef
      and .consumption.scopeUnchangedThroughRun == true and .consumption.replayed == false
      and .consumption.cleanupAuthoritySurvivesFailure == true
    ' "$approval" >/dev/null || die "stock-Chrome scoped action approval is incomplete, replayed, or not user-delivered"
  local approval_sha review_sha
  approval_sha="$(sha256_file "$approval")"
  review_sha="$(sha256_file "$review")"
  jq -e --arg approval_sha "$approval_sha" --slurpfile approval "$approval" '
      .scopedActionApproval.recordSha256 == $approval_sha
      and .scopedActionApproval.approvalId == $approval[0].approvalId
      and .scopedActionApproval.scopeSha256 == $approval[0].request.scopeSha256
      and .scopedActionApproval.approvalConfirmedAtUtc == $approval[0].response.confirmedAtUtc
      and .scopedActionApproval.approvalExpiresAtUtc == $approval[0].request.expiresAtUtc
      and .scopedActionApproval.approvalChallengeFrameRef == $approval[0].request.challengeFrameRef
      and .scopedActionApproval.preDispatchFrameRef == $approval[0].consumption.preDispatchFrameRef
      and .scopedActionApproval.preDispatchDecisionRef == $approval[0].consumption.preDispatchDecisionRef
      and .scopedActionApproval.preDispatchVerifiedAtUtc == $approval[0].consumption.preDispatchVerifiedAtUtc
      and .scopedActionApproval.consumedBeforeExpiry == $approval[0].consumption.consumedBeforeExpiry
      and $approval[0].response.orchestratorSessionRef != .operatorExchange.executorSessionRef
      and $approval[0].response.orchestratorSessionRef != .operatorExchange.reviewerSessionRef
    ' "$helper" >/dev/null || die "stock-Chrome scoped approval does not bind the helper or uses an executor/reviewer session"
  jq -e --slurpfile helper "$helper" '
      .schemaVersion == 1 and .evidenceType == "stock-user-chrome-independent-visual-review"
      and .executorSessionRef == $helper[0].operatorExchange.executorSessionRef
      and .reviewerSessionRef == $helper[0].operatorExchange.reviewerSessionRef
      and .executorSessionRef != .reviewerSessionRef and .independentSessionBoundary == true
      and ([.entries[].sequence] == [1,2,3,4,5,6])
      and ([.entries[].purpose] == ["extension-loaded","api-action-result","computer-share-action","stop-paused","cancel-paused","post-handback-resume"])
      and ([.entries[].image] == ["browser-01-extension-loaded.png","browser-02-api-action-result.png","browser-03-computer-share-action.png","browser-04-stop-paused.png","browser-05-cancel-paused.png","browser-06-post-handback-resume.png"])
      and ([.entries[] | select(.digestMatched != true or .requiredStateVerdict != "pass" or .sensitivePixelsObserved != false or .uncertain != false)] | length == 0)
      and .aggregate.reviewedCropCount == 6
      and .aggregate.everySanitizedCropOpenedByReviewer == true
      and .aggregate.allImageDigestsMatched == true
      and .aggregate.requiredVisibleStateConfirmedByReviewer == true
      and .aggregate.noSensitivePixelsObservedByReviewer == true
      and .aggregate.noUncertaintyReported == true
      and .aggregate.visualJudgmentNotPixelSafetyProof == true
    ' "$review" >/dev/null || die "stock-Chrome independent visual review is same-session, reordered, uncertain, sensitive, or incomplete"
  jq -e '
      (keys_unsorted == ["schemaVersion","evidenceType","phase","releaseCandidateBinding","releaseCandidateBindingSha256","orchestrationSurface","chromeMcpState","computerUseState","reviewerInputState","attestorKind","attestorSessionRef","attestedAtUtc"])
      and .schemaVersion == 1
      and .evidenceType == "stock-user-chrome-external-surface-attestation"
      and .phase == "preflight"
      and .orchestrationSurface == "user-orchestrator-secured-ssh-exported-file-review"
      and .chromeMcpState == "not-used-before-candidate-execution"
      and .computerUseState == "released-before-candidate-execution"
      and .reviewerInputState == "review-not-started"
      and .attestorKind == "orchestrator-agent"
      and (.releaseCandidateBindingSha256 | test("^[0-9a-f]{64}$"))
      and (.attestorSessionRef | test("^[0-9a-f]{64}$"))
    ' "$external_preflight" >/dev/null || die "stock-Chrome external-surface preflight is not exact and phase-scoped"
  jq -e --slurpfile pre "$external_preflight" --slurpfile approval "$approval" --slurpfile helper "$helper" '
      (keys_unsorted == ["schemaVersion","evidenceType","phase","releaseCandidateBinding","releaseCandidateBindingSha256","orchestrationSurface","chromeMcpState","computerUseState","reviewerInputState","attestorKind","attestorSessionRef","attestedAtUtc"])
      and .schemaVersion == 1
      and .evidenceType == "stock-user-chrome-external-surface-attestation"
      and .phase == "postflight"
      and .orchestrationSurface == "user-orchestrator-secured-ssh-exported-file-review"
      and .chromeMcpState == "never-used-through-independent-review"
      and .computerUseState == "not-resumed-through-independent-review"
      and .reviewerInputState == "exported-digest-bound-files-only"
      and .attestorKind == "orchestrator-agent"
      and .releaseCandidateBinding == $pre[0].releaseCandidateBinding
      and .releaseCandidateBindingSha256 == $pre[0].releaseCandidateBindingSha256
      and .attestorSessionRef == $pre[0].attestorSessionRef
      and .attestorSessionRef == $approval[0].response.orchestratorSessionRef
      and .attestorSessionRef != $helper[0].operatorExchange.executorSessionRef
      and .attestorSessionRef != $helper[0].operatorExchange.reviewerSessionRef
    ' "$external_postflight" >/dev/null || die "stock-Chrome external-surface postflight does not close the exact review interval"
  jq -e '
      .schemaVersion == 3 and .evidenceType == "stock-user-chrome-operator-observations"
      and .environment.platform == "windows-x86_64" and .environment.browserProduct == "Google Chrome"
      and (.environment.browserVersion | test("^[0-9]{1,3}\\.[0-9]{1,5}\\.[0-9]{1,5}\\.[0-9]{1,5}$"))
      and ((.environment.browserVersion | split(".")[0] | tonumber) >= 140)
      and .environment.stockUserChrome == true and .environment.existingUserSession == true
      and .environment.dedicatedTemporaryWindow == true and .environment.localDemoOnly == true
      and .environment.browserLaunchFlagsUsed == false and .environment.directCdpUsed == false
      and .environment.automationTestProfileUsed == false
      and .actionSurfaces.bridgeApiMatrix == "local-browser-bridge-api"
      and .actionSurfaces.computerHelperApi == "local-browser-bridge-computer-api"
      and .actionSurfaces.debuggerOwnerDuringBridgeLease == "local-browser-bridge-extension"
      and .actionSurfaces.competingDebuggerAttachmentAllowed == false
      and .actionSurfaces.chromeMcpUsedDuringBridgeLease == false
      and .actionSurfaces.chromeMcpReleaseEvidenceClaimed == false
      and .computerHelperChain.screenshotEndpoint == "/api/computer/screenshot"
      and .computerHelperChain.rawScreenshotCount == 6
      and ([.computerHelperChain[] | select(type == "boolean")] | all(. == true))
      and .initialState.capturedBeforeRelevantMutation == true
      and .initialState.candidateExtensionPresent == false and .initialState.savedTokenConfigured == false
      and .extension.cardCount == 1 and .extension.version == "0.12.33" and .extension.enabled == true
      and .extension.loadErrors == 0 and .extension.loadedVia == "chrome://extensions-load-unpacked"
      and .extension.loadedDirectoryByteMatchesCandidateZip == true and .extension.popupConnected == true
      and .extension.debuggerLeaseActiveAtFirstCapture == true and .extension.nativeDebuggerUseIndicatorSeen == true
      and ([.screenshotCaptures[].purpose] == ["extension-loaded","api-action-result","computer-share-action","stop-paused","cancel-paused","post-handback-resume"])
      and ([.screenshotCaptures[].image] == ["browser-01-extension-loaded.png","browser-02-api-action-result.png","browser-03-computer-share-action.png","browser-04-stop-paused.png","browser-05-cancel-paused.png","browser-06-post-handback-resume.png"])
      and ([.screenshotCaptures[].captureSurface] | all(. == "local-browser-bridge-computer-helper"))
      and .consentCheckpoints.scopedActionTimeApproval.obtainedAtActionTime == true
      and .consentCheckpoints.scopedActionTimeApproval.consumedBeforeFirstCoveredAction == true
      and .consentCheckpoints.scopedActionTimeApproval.consumedBeforeExpiry == true
      and .consentCheckpoints.scopedActionTimeApproval.freshStateRevalidatedAfterApproval == true
      and .consentCheckpoints.scopedActionTimeApproval.scopeUnchangedThroughRun == true
      and .consentCheckpoints.scopedActionTimeApproval.singleCandidateRun == true
      and .independentVisualReview.reviewedCropCount == 6
      and .independentVisualReview.independentSessionBoundary == true
      and .independentVisualReview.allImageDigestsMatched == true
      and .independentVisualReview.noSensitivePixelsObservedByReviewer == true
      and .independentVisualReview.noUncertaintyReported == true
      and .independentVisualReview.visualJudgmentNotPixelSafetyProof == true
      and .restoration.candidateExtensionPresence.finalPresent == false
      and .restoration.candidateExtensionPresence.matchesInitial == true
      and .retainedEvidence.exactAllowlistVerified == true
      and .retainedEvidence.inputFileCount == 21 and .retainedEvidence.finalFileCount == 22
      and (.cleanup.externalSurfacePreflightAttestationSha256 | test("^[0-9a-f]{64}$"))
      and (.cleanup.externalSurfacePostflightAttestationSha256 | test("^[0-9a-f]{64}$"))
      and .cleanup.chromeMcpDisposition == "never-used-through-independent-review"
      and .cleanup.computerUseDisposition == "not-resumed-through-independent-review"
      and .cleanup.reviewerInputDisposition == "exported-digest-bound-files-only"
      and .retainedEvidence.rawScreenshotsPresent == false
      and .cleanup.controlReleased == true and .cleanup.testTabsClosed == true
      and .cleanup.testWindowClosed == true and .cleanup.serverStopped == true
      and .cleanup.portReleased == true and .cleanup.computerHelperStopped == true
      and .cleanup.extractedExtensionDirectoryDeleted == true
      and .cleanup.unrelatedTargetMutationCommandsIssued == false
      and .cleanup.unrelatedExtensionMutationCommandsIssued == false
    ' "$operator" >/dev/null || die "stock-Chrome operator record omits required stock-session, consent, debugger, helper, restoration, or cleanup invariants"
  jq -e --arg approval_sha "$approval_sha" --arg review_sha "$review_sha" --slurpfile approval "$approval" --slurpfile review "$review" --slurpfile helper "$helper" '
      .consentCheckpoints.scopedActionTimeApproval.recordSha256 == $approval_sha
      and .consentCheckpoints.scopedActionTimeApproval.approvalId == $approval[0].approvalId
      and .independentVisualReview.recordSha256 == $review_sha
      and .independentVisualReview.executorSessionRef == $review[0].executorSessionRef
      and .independentVisualReview.reviewerSessionRef == $review[0].reviewerSessionRef
      and .independentVisualReview.executorSessionRef == $helper[0].operatorExchange.executorSessionRef
      and .independentVisualReview.reviewerSessionRef == $helper[0].operatorExchange.reviewerSessionRef
    ' "$operator" >/dev/null || die "stock-Chrome operator approval/review summaries do not bind the retained records and helper sessions"
  test "$(jq -cS '.handback' "$helper")" = "$(jq -cS '.handback' "$operator")" \
    && test "$(jq -cS '.handback' "$helper")" = "$(jq -cS '.handback' "$final")" \
    || die "stock-Chrome Stop/Cancel handback proofs differ across helper, operator, and final records"
  jq -e '
      def refusal:
        .httpStatus == 423 and .errorCode == "HUMAN_CONTROL_PAUSED"
        and .taxonomyState == "needs_user" and .taxonomyAction == "handback" and .retriable == false;
      def valid_case($trigger; $reason):
        .trigger == $trigger and .operatorSurface == "local-browser-bridge-computer-helper"
        and .statusPollMethod == "browser.control.status" and .statusPolledAfterTrigger == true
        and .reducedStatus.active == false and .reducedStatus.humanPaused == true
        and .reducedStatus.reason == $reason and .reducedStatus.revocationPending == false
        and (.controlStartRefusal | refusal) and (.tabMutationRefusal | refusal)
        and .indicatorsRemoved == true
        and .resume.trustedPopupClick == true
        and .resume.operatorSurface == "local-browser-bridge-computer-helper"
        and .resume.statusPollMethod == "browser.control.status"
        and .resume.statusPolledAfterResume == true
        and .resume.reducedStatus == {active:false,humanPaused:false,revocationPending:false}
        and .resume.postResumeStartSucceeded == true and .resume.activeStatusPolled == true
        and .resume.activeStatus == {active:true,humanPaused:false,revocationPending:false};
      (keys_unsorted == ["stop","cancel"])
      and (.stop | valid_case("in-page-stop"; "released_by_user"))
      and (.cancel | valid_case("chrome-native-cancel"; "canceled_by_user"))
    ' < <(jq '.handback' "$helper") >/dev/null \
    || die "stock-Chrome Stop/Cancel/trusted-popup Resume state machine proof is incomplete"

  local sidecar image image_sha image_bytes image_width image_height
  for name in browser-01-extension-loaded browser-02-api-action-result browser-03-computer-share-action \
    browser-04-stop-paused browser-05-cancel-paused browser-06-post-handback-resume; do
    sidecar="$browser_root/$name.json"
    image="$browser_root/$name.png"
    image_sha="$(jq -er '.image.sha256' "$sidecar")"
    image_bytes="$(jq -er '.image.bytes' "$sidecar")"
    image_width="$(jq -er '.image.width' "$sidecar")"
    image_height="$(jq -er '.image.height' "$sidecar")"
    assert_release_candidate_binding "$sidecar" '.releaseCandidateBinding'
    verify_file_fact "$image" "$image_sha" "$image_bytes"
    verify_png_dimensions "$image" "$image_width" "$image_height"
    test "$(jq -cS . "$sidecar")" = "$(jq -cS --arg image "$name.png" '.screenshots[] | select(.image.name == $image)' "$final")" \
      || die "stock-Chrome screenshot sidecar differs from its embedded record: $name"
    jq -e '
        .automatedTextInspectionPerformed == false
        and .independentVisualReviewRequired == true
        and .independentVisualReviewCompleted == true
        and (.reviewRecordSha256 | test("^[0-9a-f]{64}$"))
        and (.reviewEntryRef | test("^[0-9a-f]{64}$"))
        and .automaticPixelRedactionPerformed == false
        and .unknownPixelSafetyClaimed == false
        and .forbiddenMetadataChunksPresent == false
        and (has("manualVisualReviewRequired") | not)
        and (has("manualVisualReviewConfirmed") | not)
        and (has("humanVisualReview") | not)
        and (has("ocrAvailable") | not)
        and (has("ocrDenylistChecked") | not)
        and (has("ocrDenylistMatches") | not)
      ' "$sidecar" >/dev/null \
      || die "stock-Chrome screenshot sidecar is not the exact independent digest-bound review schema: $name"
    test "$(jq -er '.reviewRecordSha256' "$sidecar")" = "$review_sha" \
      || die "stock-Chrome screenshot sidecar does not bind the exact independent review: $name"
  done
  jq -e '
      ([.screenshots[].purpose] == ["extension-loaded","api-action-result","computer-share-action","stop-paused","cancel-paused","post-handback-resume"])
    ' "$final" >/dev/null \
    || die "stock-Chrome retained screenshots are missing or reordered across required states"

  local contract_root
  contract_root="$SCRATCH_ROOT/stock-chrome-contract"
  mkdir "$contract_root"
  chmod 700 "$contract_root"
  for name in candidate-preflight.json candidate-postflight.json browser-api-matrix.json \
    browser-computer-helper-chain.json scoped-action-approval.json \
    independent-visual-review.json external-surface-preflight.json \
    external-surface-postflight.json operator-results.json \
    browser-01-extension-loaded.json browser-01-extension-loaded.png \
    browser-02-api-action-result.json browser-02-api-action-result.png \
    browser-03-computer-share-action.json browser-03-computer-share-action.png \
    browser-04-stop-paused.json browser-04-stop-paused.png \
    browser-05-cancel-paused.json browser-05-cancel-paused.png \
    browser-06-post-handback-resume.json browser-06-post-handback-resume.png; do
    cp "$browser_root/$name" "$contract_root/$name"
  done
  LBB_CONTRACT_ROOT="$contract_root" env -u GH_TOKEN -u GITHUB_TOKEN \
    pwsh -NoLogo -NoProfile -NonInteractive -Command '
    & ./scripts/write-browser-evidence-record.ps1 -Mode Finalize `
      -PreflightRecord (Join-Path $env:LBB_CONTRACT_ROOT "candidate-preflight.json") `
      -PostflightRecord (Join-Path $env:LBB_CONTRACT_ROOT "candidate-postflight.json") `
      -ApiMatrixRecord (Join-Path $env:LBB_CONTRACT_ROOT "browser-api-matrix.json") `
      -ComputerHelperRecord (Join-Path $env:LBB_CONTRACT_ROOT "browser-computer-helper-chain.json") `
      -ScopedApprovalRecord (Join-Path $env:LBB_CONTRACT_ROOT "scoped-action-approval.json") `
      -IndependentReviewRecord (Join-Path $env:LBB_CONTRACT_ROOT "independent-visual-review.json") `
      -ExternalSurfacePreflightAttestation (Join-Path $env:LBB_CONTRACT_ROOT "external-surface-preflight.json") `
      -ExternalSurfacePostflightAttestation (Join-Path $env:LBB_CONTRACT_ROOT "external-surface-postflight.json") `
      -OperatorResults (Join-Path $env:LBB_CONTRACT_ROOT "operator-results.json") `
      -ScreenshotRecords @(
        (Join-Path $env:LBB_CONTRACT_ROOT "browser-01-extension-loaded.json"),
        (Join-Path $env:LBB_CONTRACT_ROOT "browser-02-api-action-result.json"),
        (Join-Path $env:LBB_CONTRACT_ROOT "browser-03-computer-share-action.json"),
        (Join-Path $env:LBB_CONTRACT_ROOT "browser-04-stop-paused.json"),
        (Join-Path $env:LBB_CONTRACT_ROOT "browser-05-cancel-paused.json"),
        (Join-Path $env:LBB_CONTRACT_ROOT "browser-06-post-handback-resume.json")
      ) `
      -OutputRecord (Join-Path $env:LBB_CONTRACT_ROOT "browser-acceptance.json")
  ' >/dev/null || die "stock-Chrome evidence failed the source-schema complete contract replay"
  test "$(jq -cS 'del(.recordedAtUtc)' "$contract_root/browser-acceptance.json")" = \
    "$(jq -cS 'del(.recordedAtUtc)' "$final")" \
    || die "stock-Chrome final record differs from independently rebuilt source-contract output"
}

scan_evidence_for_leaks() {
  local evidence_root="$1"
  python3 - \
    "$evidence_root" "$EVIDENCE_MAX_BLOB_BYTES" "$EVIDENCE_MAX_TOTAL_BYTES" <<'PY'
import json
import os
import re
import struct
import sys
import zlib

root = os.path.realpath(sys.argv[1])
maximum_blob_bytes = int(sys.argv[2])
maximum_total_bytes = int(sys.argv[3])
email = re.compile(r"(?i)(?<![A-Z0-9._%+-])[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}(?![A-Z0-9.-])")
home_path = re.compile(r"(?i)(?:/Users/[^/\s]+|/home/[^/\s]+|[A-Z]:\\Users\\[^\\\s]+)")
bearer = re.compile(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]{12,}")
signed_url = re.compile(r"(?i)https?://\S+[?&](?:token|sig|signature|x-amz-credential|x-amz-signature|x-goog-signature|access_token)=")
credential_assignment = re.compile(r"(?i)\b(?:password|passwd|secret|access[_-]?token|refresh[_-]?token|authorization)\s*[:=]\s*[^\s,}\]]{4,}")
forbidden_png_chunks = {b"tEXt", b"zTXt", b"iTXt", b"eXIf", b"iCCP", b"tIME"}

def fail(message):
    raise SystemExit(message)

def scan_text(path, text):
    for pattern, label in ((email, "email"), (home_path, "home path"), (bearer, "bearer value"),
                           (signed_url, "signed URL"), (credential_assignment, "credential value")):
        if pattern.search(text):
            fail(f"retained evidence contains a forbidden {label}: {os.path.relpath(path, root)}")

def walk_json(value, path):
    if isinstance(value, dict):
        for key, child in value.items():
            folded = key.casefold().replace("_", "").replace("-", "")
            if folded in {"token", "bearer", "authorization", "password", "passwd", "secret", "credential",
                          "accesstoken", "refreshtoken", "sessiontoken", "commandline", "environmentidentifier",
                          "hostname", "username", "userhome", "email"}:
                if child not in (None, False, "", 0, [], {}):
                    fail(f"retained JSON contains a populated sensitive field: {os.path.relpath(path, root)}:{key}")
            if folded.endswith("title") and isinstance(child, str) and child and not child.startswith("LBB "):
                fail(f"retained JSON contains a non-fixture title: {os.path.relpath(path, root)}:{key}")
            walk_json(child, path)
    elif isinstance(value, list):
        for child in value:
            walk_json(child, path)
    elif isinstance(value, str):
        scan_text(path, value)

def parse_unique_json(text, path):
    def unique_object(pairs):
        value = {}
        for key, child in pairs:
            if key in value:
                fail(f"retained JSON contains a duplicate key: {os.path.relpath(path, root)}:{key}")
            value[key] = child
        return value
    return json.loads(text, object_pairs_hook=unique_object)

def validate_png(path):
    size = os.path.getsize(path)
    if size < 57 or size > maximum_blob_bytes:
        fail(f"retained PNG size is invalid: {os.path.relpath(path, root)}")
    with open(path, "rb") as stream:
        data = stream.read()
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        fail(f"retained image is not PNG: {os.path.relpath(path, root)}")
    offset = 8
    saw_iend = False
    saw_ihdr = False
    idat = bytearray()
    chunks = []
    chunk_index = 0
    while offset < len(data):
        if len(data) - offset < 12:
            fail(f"retained PNG framing is invalid: {os.path.relpath(path, root)}")
        length = struct.unpack(">I", data[offset:offset + 4])[0]
        chunk = data[offset + 4:offset + 8]
        end = offset + 12 + length
        if end > len(data) or chunk in forbidden_png_chunks:
            fail(f"retained PNG contains invalid or metadata-bearing chunks: {os.path.relpath(path, root)}")
        if not re.fullmatch(rb"[A-Za-z]{4}", chunk):
            fail(f"retained PNG has an invalid chunk type: {os.path.relpath(path, root)}")
        if chunk_index == 0 and (chunk != b"IHDR" or length != 13):
            fail(f"retained PNG has no canonical leading IHDR: {os.path.relpath(path, root)}")
        if chunk == b"IHDR":
            if saw_ihdr:
                fail(f"retained PNG has duplicate IHDR: {os.path.relpath(path, root)}")
            saw_ihdr = True
        chunks.append(chunk)
        expected_crc = struct.unpack(">I", data[offset + 8 + length:end])[0]
        actual_crc = zlib.crc32(data[offset + 4:offset + 8 + length]) & 0xFFFFFFFF
        if expected_crc != actual_crc:
            fail(f"retained PNG has an invalid chunk checksum: {os.path.relpath(path, root)}")
        if chunk == b"IDAT":
            idat.extend(data[offset + 8:offset + 8 + length])
        if chunk == b"IEND":
            saw_iend = length == 0 and end == len(data)
            break
        offset = end
        chunk_index += 1
    if not saw_ihdr or not idat or not saw_iend or chunks.count(b"IEND") != 1:
        fail(f"retained PNG is incomplete: {os.path.relpath(path, root)}")
    idat_indexes = [index for index, kind in enumerate(chunks) if kind == b"IDAT"]
    if idat_indexes != list(range(idat_indexes[0], idat_indexes[-1] + 1)):
        fail(f"retained PNG has noncontiguous IDAT chunks: {os.path.relpath(path, root)}")
    width, height, bit_depth, color_type, compression, filtering, interlace = struct.unpack(
        ">IIBBBBB", data[16:29]
    )
    legal = {0: {1,2,4,8,16}, 2: {8,16}, 3: {1,2,4,8}, 4: {8,16}, 6: {8,16}}
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
    if width < 1 or height < 1 or width * height > 50_000_000 or color_type not in legal or \
       bit_depth not in legal[color_type] or compression != 0 or filtering != 0 or interlace != 0:
        fail(f"retained PNG has invalid or unsupported IHDR semantics: {os.path.relpath(path, root)}")
    if color_type == 3 and b"PLTE" not in chunks:
        fail(f"retained indexed PNG omits its palette: {os.path.relpath(path, root)}")
    row_bytes = (width * channels[color_type] * bit_depth + 7) // 8
    decoded_size = height * (row_bytes + 1)
    if decoded_size > 256 * 1024 * 1024:
        fail(f"retained PNG decoded payload exceeds its bound: {os.path.relpath(path, root)}")
    decoder = zlib.decompressobj()
    decoded = decoder.decompress(bytes(idat), decoded_size + 1)
    if decoder.unconsumed_tail:
        fail(f"retained PNG expands beyond its claimed raster: {os.path.relpath(path, root)}")
    decoded += decoder.flush(max(1, decoded_size + 1 - len(decoded)))
    if not decoder.eof or decoder.unused_data or len(decoded) != decoded_size:
        fail(f"retained PNG IDAT does not decode to its claimed raster: {os.path.relpath(path, root)}")
    if any(decoded[row * (row_bytes + 1)] > 4 for row in range(height)):
        fail(f"retained PNG has an invalid scanline filter: {os.path.relpath(path, root)}")

total = 0
for directory, directories, files in os.walk(root, followlinks=False):
    if directories:
        for name in directories:
            if os.path.islink(os.path.join(directory, name)):
                fail("retained evidence contains a symlink")
    for name in files:
        path = os.path.join(directory, name)
        if os.path.islink(path) or not os.path.isfile(path):
            fail("retained evidence contains a non-regular file")
        size = os.path.getsize(path)
        total += size
        suffix = os.path.splitext(name)[1].lower()
        if suffix == ".png":
            validate_png(path)
            continue
        if suffix not in {".json", ".ndjson", ".log"} or size > 10 * 1024 * 1024:
            fail(f"retained evidence has an unsupported file: {os.path.relpath(path, root)}")
        with open(path, "r", encoding="utf-8") as stream:
            text = stream.read()
        if "\x00" in text:
            fail(f"retained evidence contains NUL: {os.path.relpath(path, root)}")
        scan_text(path, text)
        if suffix == ".json":
            value = parse_unique_json(text, path)
            walk_json(value, path)
        elif suffix == ".ndjson":
            rows = [parse_unique_json(line, path) for line in text.splitlines() if line]
            if not rows:
                fail(f"retained NDJSON is empty: {os.path.relpath(path, root)}")
            walk_json(rows, path)
if total > maximum_total_bytes:
    fail("retained evidence exceeds the size limit")
PY
}

validate_evidence_tree_blob_bounds() {
  local evidence_commit_sha="$1"
  local canonical_root="$2"
  local expected_count="$3"
  local expected_paths_file="$4"
  local inventory_fifo="$5"
  test -f "$expected_paths_file" && test ! -L "$expected_paths_file" \
    || die "evidence-only diff path inventory is invalid"
  test ! -e "$inventory_fifo" && test ! -L "$inventory_fifo" \
    || die "evidence tree object stream destination already exists"
  mkfifo -m 600 "$inventory_fifo" \
    || die "could not create the bounded evidence tree object stream"
  local tree_paths_file="$inventory_fifo.paths"
  test ! -e "$tree_paths_file" && test ! -L "$tree_paths_file" \
    || die "evidence tree path inventory destination already exists"
  (umask 077; : > "$tree_paths_file")

  local tree_entry mode type object tree_path
  local tree_count=0 aggregate_size=0 raw_blob_size blob_size tree_failure=""
  git ls-tree -r -z "$evidence_commit_sha" -- "$canonical_root" > "$inventory_fifo" &
  local tree_producer_pid=$!
  exec 7< "$inventory_fifo"
  while true; do
    tree_entry=""
    if IFS= read -r -d '' -n "$((EVIDENCE_MAX_TREE_RECORD_BYTES + 1))" tree_entry <&7; then
      if ((${#tree_entry} > EVIDENCE_MAX_TREE_RECORD_BYTES)); then
        tree_failure="evidence tree emitted an overlong object/path record"
        break
      fi
    elif test -n "$tree_entry"; then
      tree_failure="evidence tree emitted an incomplete object/path record"
      break
    else
      break
    fi
    tree_count=$((tree_count + 1))
    if ((tree_count > expected_count || tree_count > 200)); then
      tree_failure="evidence tree file count exceeds the bounded evidence-only diff inventory"
      break
    fi
    mode="${tree_entry%% *}"
    type="${tree_entry#* }"; type="${type%% *}"
    object="${tree_entry#* }"; object="${object#* }"; object="${object%%$'\t'*}"
    tree_path="${tree_entry#*$'\t'}"
    if ! { test "$mode" = 100644 && test "$type" = blob && is_sha1 "$object"; }; then
      tree_failure="evidence tree contains a symlink, executable, submodule, or non-blob"
      break
    fi
    if ! is_safe_evidence_path "$tree_path" "$canonical_root"; then
      tree_failure="evidence tree contains an unsafe or overlong path"
      break
    fi
    if ! line_inventory_contains_exact "$tree_path" "$expected_paths_file"; then
      tree_failure="evidence tree contains a path not added by the evidence commit"
      break
    fi
    if line_inventory_contains_exact "$tree_path" "$tree_paths_file"; then
      tree_failure="evidence tree contains a duplicate path"
      break
    fi
    printf '%s\n' "$tree_path" >> "$tree_paths_file"

    if ! raw_blob_size="$({ git cat-file -s "$object" && printf x; } 2>/dev/null)"; then
      tree_failure="could not read the exact evidence blob size: $object"
      break
    fi
    if [[ "$raw_blob_size" != *$'\n'x ]]; then
      tree_failure="evidence blob size output is noncanonical: $object"
      break
    fi
    blob_size="${raw_blob_size%$'\n'x}"
    if [[ "$blob_size" == *$'\n'* ]] || [[ ! "$blob_size" =~ ^(0|[1-9][0-9]*)$ ]]; then
      tree_failure="evidence blob size output is noncanonical: $object"
      break
    fi
    if ! is_canonical_decimal_at_most "$blob_size" "$EVIDENCE_MAX_BLOB_BYTES"; then
      tree_failure="evidence tree blob exceeds the $EVIDENCE_MAX_BLOB_BYTES-byte limit: $tree_path"
      break
    fi
    if ((aggregate_size > EVIDENCE_MAX_TOTAL_BYTES - blob_size)); then
      tree_failure="evidence tree aggregate blob size exceeds the $EVIDENCE_MAX_TOTAL_BYTES-byte limit"
      break
    fi
    aggregate_size=$((aggregate_size + blob_size))
  done
  exec 7<&-
  if test -n "$tree_failure"; then
    kill "$tree_producer_pid" >/dev/null 2>&1 || true
    wait "$tree_producer_pid" >/dev/null 2>&1 || true
    die "$tree_failure"
  fi
  wait "$tree_producer_pid" \
    || die "could not enumerate the exact evidence tree objects"
  test "$tree_count" = "$expected_count" \
    || die "evidence tree and evidence-only diff inventories differ"
}

verify_evidence_commit() {
  local receipt_file="$1"
  local candidate_dir="$2"
  local scratch_root="$3"
  local evidence_ref evidence_commit_sha remote_lines remote_sha parent_line commit parent extra
  evidence_ref="$(jq -er '.evidenceRef' "$receipt_file")"
  evidence_commit_sha="$(jq -er '.evidenceCommitSha' "$receipt_file")"
  local canonical_ref="refs/heads/evidence/${RELEASE_TAG}-release-run-${CANDIDATE_RUN_ID}-attempt-${CANDIDATE_RUN_ATTEMPT}"
  local canonical_root="evidence/${RELEASE_TAG}/release/run-${CANDIDATE_RUN_ID}-attempt-${CANDIDATE_RUN_ATTEMPT}"
  test "$evidence_ref" = "$canonical_ref" || die "evidence ref is not canonical for this workflow attempt"
  is_sha1 "$evidence_commit_sha" || die "evidence commit SHA is invalid"
  local origin_url
  origin_url="$(git remote get-url origin)"
  [[ "$origin_url" == "https://github.com/$GITHUB_REPOSITORY" || "$origin_url" == "https://github.com/$GITHUB_REPOSITORY.git" ]] \
    || die "origin is not the canonical GitHub repository"

  remote_lines="$(git ls-remote --refs origin "$evidence_ref")"
  test "$(printf '%s\n' "$remote_lines" | sed '/^$/d' | wc -l | tr -d ' ')" = 1 \
    || die "remote evidence ref did not resolve exactly once"
  remote_sha="$(printf '%s\n' "$remote_lines" | awk -v ref="$evidence_ref" '$2 == ref { print $1 }')"
  test "$remote_sha" = "$evidence_commit_sha" || die "remote evidence ref does not resolve to the receipt commit"
  git -c protocol.version=2 fetch --quiet --no-tags origin "$evidence_ref"
  test "$(git rev-parse FETCH_HEAD)" = "$evidence_commit_sha" || die "fetched evidence ref changed after resolution"
  test "$(git cat-file -t "$evidence_commit_sha")" = commit || die "evidence object is not a commit"
  parent_line="$(git rev-list --parents -n 1 "$evidence_commit_sha")"
  read -r commit parent extra <<< "$parent_line"
  test "$commit" = "$evidence_commit_sha" && test "$parent" = "$VERIFIED_SOURCE_SHA" && test -z "${extra:-}" \
    || die "evidence commit must have the verified source as its sole parent"

  local changed_fifo="$scratch_root/evidence-changed-raw.pipe"
  local changed_paths="$scratch_root/evidence-changed-paths.txt"
  test ! -e "$changed_fifo" && test ! -L "$changed_fifo" \
    && test ! -e "$changed_paths" && test ! -L "$changed_paths" \
    || die "evidence-only diff inventory destination already exists"
  mkfifo -m 600 "$changed_fifo" \
    || die "could not create the bounded evidence-only diff stream"
  (umask 077; : > "$changed_paths")
  local status path changed_count=0 changed_failure=""
  git diff-tree --no-commit-id --name-status -r -z \
    "$VERIFIED_SOURCE_SHA" "$evidence_commit_sha" > "$changed_fifo" &
  local changed_producer_pid=$!
  exec 8< "$changed_fifo"
  while true; do
    status=""
    if IFS= read -r -d '' -n "$((EVIDENCE_MAX_DIFF_STATUS_BYTES + 1))" status <&8; then
      if ((${#status} > EVIDENCE_MAX_DIFF_STATUS_BYTES)); then
        changed_failure="evidence-only diff emitted an overlong status record"
        break
      fi
    elif test -n "$status"; then
      changed_failure="evidence-only diff emitted an incomplete status record"
      break
    else
      break
    fi
    path=""
    if ! IFS= read -r -d '' -n "$((EVIDENCE_MAX_PATH_BYTES + 1))" path <&8; then
      changed_failure="evidence-only diff emitted an incomplete path record"
      break
    fi
    if ((${#path} > EVIDENCE_MAX_PATH_BYTES)); then
      changed_failure="evidence-only diff emitted an overlong path record"
      break
    fi
    changed_count=$((changed_count + 1))
    if ((changed_count > 200)); then
      changed_failure="evidence commit file count exceeds the allowlist bound"
      break
    fi
    if ! test "$status" = A; then
      changed_failure="evidence commit contains a non-addition: $status"
      break
    fi
    if ! is_safe_evidence_path "$path" "$canonical_root"; then
      changed_failure="evidence commit has an unsafe, noncanonical, or overlong path"
      break
    fi
    if line_inventory_contains_exact "$path" "$changed_paths"; then
      changed_failure="evidence-only diff contains a duplicate path"
      break
    fi
    printf '%s\n' "$path" >> "$changed_paths"
  done
  exec 8<&-
  if test -n "$changed_failure"; then
    kill "$changed_producer_pid" >/dev/null 2>&1 || true
    wait "$changed_producer_pid" >/dev/null 2>&1 || true
    die "$changed_failure"
  fi
  wait "$changed_producer_pid" \
    || die "could not enumerate the exact evidence-only diff"
  test "$changed_count" -ge 5 && test "$changed_count" -le 200 || die "evidence commit file count is outside the allowlist bound"

  local tree_inventory="$scratch_root/evidence-tree-objects.z"
  validate_evidence_tree_blob_bounds \
    "$evidence_commit_sha" "$canonical_root" "$changed_count" "$changed_paths" "$tree_inventory"

  local extracted_parent="$scratch_root/evidence-tree"
  mkdir "$extracted_parent"
  git archive --format=tar "$evidence_commit_sha" "$canonical_root" | tar -xf - -C "$extracted_parent"
  local evidence_root="$extracted_parent/$canonical_root"
  test -d "$evidence_root" && test ! -L "$evidence_root" || die "canonical evidence root was not materialized"

  declare -gA ALLOWED_PATHS=()
  local mac_aggregate="$evidence_root/macos/macos-acceptance.json"
  local quiet_result="$evidence_root/macos/quiet/helper-results.json"
  local deliberate_result="$evidence_root/macos/deliberate-concurrency/helper-results.json"
  local windows_result="$evidence_root/windows/computer/summary.json"
  local chrome_result="$evidence_root/windows/browser/browser-acceptance.json"
  assert_json_hash "$mac_aggregate" "$(jq -er '.macosAcceptanceSha256' "$receipt_file")"
  assert_json_hash "$quiet_result" "$(jq -er '.macosQuietResultSha256' "$receipt_file")"
  assert_json_hash "$deliberate_result" "$(jq -er '.macosDeliberateConcurrencyResultSha256' "$receipt_file")"
  assert_json_hash "$windows_result" "$(jq -er '.windowsResultSha256' "$receipt_file")"
  assert_json_hash "$chrome_result" "$(jq -er '.stockChromeResultSha256' "$receipt_file")"
  add_allowed "macos/macos-acceptance.json"

  local mac_harness_source_binding="$scratch_root/macos-harness-source-binding.json"
  write_mac_harness_source_binding "$mac_harness_source_binding" \
    || die "could not independently hash the exact macOS harness sources"
  validate_mac_harness_source_binding \
    "$mac_harness_source_binding" "$mac_aggregate" "$quiet_result" "$deliberate_result" \
    || die "macOS retained lanes or aggregate do not match the exact harness source hashes"
  validate_mac_result_schema_binding "$mac_aggregate" "$quiet_result" "$deliberate_result" \
    || die "macOS aggregate result schema does not match both retained lanes"
  jq -e \
    --arg version "$EVIDENCE_PRODUCT_VERSION" \
    --arg release_tag "$RELEASE_TAG" \
    --arg source_sha "$VERIFIED_SOURCE_SHA" \
    --arg run_id "$CANDIDATE_RUN_ID" \
    --arg run_attempt "$CANDIDATE_RUN_ATTEMPT" \
    --arg artifact_id "$(jq -er '.releaseCandidateArtifactId' "$receipt_file")" \
    --arg artifact_zip_sha256 "$(jq -er '.releaseCandidateArtifactZipSha256' "$receipt_file")" \
    --arg manifest_sha256 "$(jq -er '.checksumManifestSha256' "$receipt_file")" \
    --arg quiet_sha256 "$(jq -er '.macosQuietResultSha256' "$receipt_file")" \
    --arg deliberate_sha256 "$(jq -er '.macosDeliberateConcurrencyResultSha256' "$receipt_file")" \
    --arg acceptance_finalizer_sha256 "$(sha256_file scripts/finalize-macos-acceptance.mjs)" '
      .schemaVersion == 3
      and .productVersion == $version
      and .status == "passed-release-candidate"
      and .evidenceClass == "exact-release-candidate-macos-dual-lane-aggregate"
      and (.bindings.releaseCandidate == {
        schemaVersion: 3,
        version: $version,
        releaseTag: $release_tag,
        repository: "flrngel/local-browser-bridge",
        sourceSha: $source_sha,
        workflowRunId: $run_id,
        workflowRunAttempt: $run_attempt,
        workflowEvent: "workflow_dispatch",
        workflowRef: "refs/heads/main",
        workflowPath: ".github/workflows/deploy.yml",
        artifactId: $artifact_id,
        artifactZipSha256: $artifact_zip_sha256,
        checksumManifestSha256: $manifest_sha256
      })
      and .bindings.source.sourceSha == $source_sha
      and .bindings.harness.acceptanceFinalizerSha256 == $acceptance_finalizer_sha256
      and .lanes.quiet.resultFile == "helper-results.json"
      and .lanes.quiet.resultSha256 == $quiet_sha256
      and .lanes.deliberateConcurrency.resultFile == "helper-results.json"
      and .lanes.deliberateConcurrency.resultSha256 == $deliberate_sha256
      and .aggregateChecks.laneDirectoriesDisjoint == true
      and .aggregateChecks.exactInventories == true
      and .aggregateChecks.resultsByteDistinct == true
      and .aggregateChecks.passingResultSchemaVersion == 8
      and .aggregateChecks.inventoryFileCount == 19
      and .aggregateChecks.screenshotCount == 12
      and .aggregateChecks.screenshotHashesMatched == true
      and .aggregateChecks.screenshotPixelHashesMatched == true
      and .aggregateChecks.operatorMarkerHashesMatched == true
    ' "$mac_aggregate" >/dev/null || die "macOS aggregate failed its pass, binding, or dual-lane invariants"
  for binding_pair in \
    'releaseCandidate releaseCandidateBinding' \
    'source harnessSourceBinding' \
    'package package' \
    'harness harness'; do
    read -r aggregate_key result_key <<< "$binding_pair"
    test "$(jq -cS --arg key "$aggregate_key" '.bindings[$key]' "$mac_aggregate")" = \
      "$(jq -cS --arg key "$result_key" '.[$key]' "$quiet_result")" \
      || die "macOS aggregate $aggregate_key binding differs from the quiet result"
    test "$(jq -cS --arg key "$result_key" '.[$key]' "$quiet_result")" = \
      "$(jq -cS --arg key "$result_key" '.[$key]' "$deliberate_result")" \
      || die "macOS raw lane $result_key bindings are not identical"
  done
  test "$(jq -cS '.capabilityBinding' "$quiet_result")" = "$(jq -cS '.capabilityBinding' "$deliberate_result")" \
    || die "macOS raw lane capability bindings are not identical"
  python3 - "$mac_aggregate" <<'PY'
import datetime
import json
import sys

with open(sys.argv[1], encoding="utf-8") as source:
    aggregate = json.load(source)
def instant(value):
    if not isinstance(value, str) or not value.endswith("Z"):
        raise SystemExit("macOS aggregate timestamp is noncanonical")
    return datetime.datetime.fromisoformat(value[:-1] + "+00:00")
finalized = instant(aggregate.get("finalizedAt"))
quiet = aggregate["lanes"]["quiet"]
deliberate = aggregate["lanes"]["deliberateConcurrency"]
quiet_started, quiet_captured = instant(quiet["startedAt"]), instant(quiet["capturedAt"])
deliberate_started, deliberate_captured = instant(deliberate["startedAt"]), instant(deliberate["capturedAt"])
for started, captured in ((quiet_started, quiet_captured), (deliberate_started, deliberate_captured)):
    elapsed = (captured - started).total_seconds()
    if elapsed < 0 or elapsed > 7200:
        raise SystemExit("macOS aggregate lane timestamps are not bounded and forward-moving")
if deliberate_started <= quiet_captured:
    raise SystemExit("macOS deliberate-concurrency lane did not start after the quiet lane passed")
captured = [quiet_captured, deliberate_captured]
if finalized < max(captured) or (finalized - max(captured)).total_seconds() > 1800:
    raise SystemExit("macOS aggregate is not ordered within the bounded 30-minute review interval")
PY

  verify_mac_lane "$evidence_root" "$mac_aggregate" quiet quiet "$(jq -er '.macosQuietResultSha256' "$receipt_file")"
  verify_mac_lane "$evidence_root" "$mac_aggregate" deliberate-concurrency deliberateConcurrency "$(jq -er '.macosDeliberateConcurrencyResultSha256' "$receipt_file")"
  validate_global_mac_screenshot_hashes "$mac_aggregate" \
    || die "macOS quiet and deliberate-concurrency lanes must contain twelve globally file- and decoded-pixel-distinct screenshots"
  verify_windows_computer "$evidence_root" "$candidate_dir/SHA256SUMS.txt"
  verify_stock_chrome "$evidence_root" "$candidate_dir/SHA256SUMS.txt"

  local actual_relative actual_count=0
  while IFS= read -r -d '' path; do
    actual_relative="${path#"$evidence_root/"}"
    test -n "${ALLOWED_PATHS[$actual_relative]:-}" || die "evidence commit contains an unreferenced or unexpected sidecar: $actual_relative"
    actual_count=$((actual_count + 1))
  done < <(find "$evidence_root" -type f -print0)
  test "$actual_count" = "${#ALLOWED_PATHS[@]}" || die "evidence allowlist references an absent file"
  scan_evidence_for_leaks "$evidence_root"
}

self_test_evidence_blob_bounds() {
  local scratch="$1"
  local synthetic_repo="$scratch/oversized-evidence-repo"
  local synthetic_root="evidence-root"
  local aggregate_blob_file="$scratch/aggregate-evidence-blob"
  local oversized_blob_file="$scratch/per-blob-oversized-evidence-blob"
  local aggregate_blob_bytes=$((EVIDENCE_MAX_TOTAL_BYTES / 11 + 1))
  local oversized_blob_bytes=$((EVIDENCE_MAX_BLOB_BYTES + 1))
  local aggregate_blob oversized_blob aggregate_leaf_tree oversized_leaf_tree
  local aggregate_root_tree oversized_root_tree aggregate_commit oversized_commit
  local index path
  git init --quiet "$synthetic_repo"
  python3 - \
    "$aggregate_blob_file" "$aggregate_blob_bytes" \
    "$oversized_blob_file" "$oversized_blob_bytes" <<'PY'
import sys

for path, size_text in ((sys.argv[1], sys.argv[2]), (sys.argv[3], sys.argv[4])):
    size = int(size_text)
    with open(path, "wb") as stream:
        stream.seek(size - 1)
        stream.write(b"\0")
PY
  aggregate_blob="$(git -C "$synthetic_repo" hash-object -w "$aggregate_blob_file")"
  oversized_blob="$(git -C "$synthetic_repo" hash-object -w "$oversized_blob_file")"
  is_sha1 "$aggregate_blob" && is_sha1 "$oversized_blob" \
    || die "self-test could not create the bounded evidence blob objects"
  test "$(git -C "$synthetic_repo" cat-file -s "$aggregate_blob")" = "$aggregate_blob_bytes" \
    && test "$(git -C "$synthetic_repo" cat-file -s "$oversized_blob")" = "$oversized_blob_bytes" \
    || die "self-test evidence blobs have the wrong sizes"
  is_canonical_decimal_at_most "$aggregate_blob_bytes" "$EVIDENCE_MAX_BLOB_BYTES" \
    || die "self-test aggregate blob is not below the per-blob bound"
  if is_canonical_decimal_at_most "$oversized_blob_bytes" "$EVIDENCE_MAX_BLOB_BYTES"; then
    die "self-test numeric bound accepted an oversized blob"
  fi

  aggregate_leaf_tree="$({
    for index in 01 02 03 04 05 06 07 08 09 10 11; do
      printf '100644 blob %s\tpart-%s.png\0' "$aggregate_blob" "$index"
    done
  } | git -C "$synthetic_repo" mktree -z)"
  aggregate_root_tree="$(printf '040000 tree %s\t%s\0' "$aggregate_leaf_tree" "$synthetic_root" \
    | git -C "$synthetic_repo" mktree -z)"
  aggregate_commit="$(printf '%s\n' 'synthetic aggregate-oversized evidence commit' \
    | env \
      GIT_AUTHOR_NAME='LBB Evidence Self-Test' \
      GIT_AUTHOR_EMAIL='lbb-evidence-self-test@invalid.example' \
      GIT_AUTHOR_DATE='2000-01-01T00:00:00Z' \
      GIT_COMMITTER_NAME='LBB Evidence Self-Test' \
      GIT_COMMITTER_EMAIL='lbb-evidence-self-test@invalid.example' \
      GIT_COMMITTER_DATE='2000-01-01T00:00:00Z' \
      git -C "$synthetic_repo" commit-tree "$aggregate_root_tree")"
  oversized_leaf_tree="$(printf '100644 blob %s\toversized.png\0' "$oversized_blob" \
    | git -C "$synthetic_repo" mktree -z)"
  oversized_root_tree="$(printf '040000 tree %s\t%s\0' "$oversized_leaf_tree" "$synthetic_root" \
    | git -C "$synthetic_repo" mktree -z)"
  oversized_commit="$(printf '%s\n' 'synthetic per-blob-oversized evidence commit' \
    | env \
      GIT_AUTHOR_NAME='LBB Evidence Self-Test' \
      GIT_AUTHOR_EMAIL='lbb-evidence-self-test@invalid.example' \
      GIT_AUTHOR_DATE='2000-01-01T00:00:01Z' \
      GIT_COMMITTER_NAME='LBB Evidence Self-Test' \
      GIT_COMMITTER_EMAIL='lbb-evidence-self-test@invalid.example' \
      GIT_COMMITTER_DATE='2000-01-01T00:00:01Z' \
      git -C "$synthetic_repo" commit-tree "$oversized_root_tree")"
  test "$(git -C "$synthetic_repo" cat-file -t "$aggregate_commit")" = commit \
    && test "$(git -C "$synthetic_repo" cat-file -t "$oversized_commit")" = commit \
    || die "self-test could not create the oversized evidence commits"

  local aggregate_paths="$scratch/aggregate-evidence-paths.txt"
  local oversized_paths="$scratch/per-blob-oversized-evidence-paths.txt"
  (umask 077; : > "$aggregate_paths"; : > "$oversized_paths")
  for index in 01 02 03 04 05 06 07 08 09 10 11; do
    path="$synthetic_root/part-$index.png"
    printf '%s\n' "$path" >> "$aggregate_paths"
  done
  printf '%s\n' "$synthetic_root/oversized.png" > "$oversized_paths"

  is_safe_evidence_path "$synthetic_root/safe/path.json" "$synthetic_root" \
    || die "self-test rejected a safe canonical evidence path"
  for path in \
    "$synthetic_root/../escape.json" \
    "$synthetic_root/./alias.json" \
    "$synthetic_root//empty.json" \
    "$synthetic_root/"; do
    if is_safe_evidence_path "$path" "$synthetic_root"; then
      die "self-test accepted an empty, dot, or dotdot evidence path component"
    fi
  done
  local overlong_component bounded_component overlong_path
  overlong_component="$(python3 -c 'print("a" * 256, end="")')"
  bounded_component="$(python3 -c 'print("b" * 250, end="")')"
  overlong_path="$synthetic_root/$bounded_component/$bounded_component/$bounded_component/$bounded_component/$bounded_component.json"
  if is_safe_evidence_path "$synthetic_root/$overlong_component.json" "$synthetic_root" \
    || is_safe_evidence_path "$overlong_path" "$synthetic_root"; then
    die "self-test accepted an overlong evidence path or component"
  fi

  local aggregate_extraction="$scratch/aggregate-oversized-extraction"
  local aggregate_failure="$scratch/aggregate-oversized-failure.log"
  mkdir "$aggregate_extraction"
  if (
    cd "$synthetic_repo"
    validate_evidence_tree_blob_bounds \
      "$aggregate_commit" "$synthetic_root" 11 "$aggregate_paths" \
      "$scratch/aggregate-oversized-tree-objects.z"
    git archive --format=tar "$aggregate_commit" "$synthetic_root" | tar -xf - -C "$aggregate_extraction"
  ) > /dev/null 2> "$aggregate_failure"; then
    die "self-test accepted an evidence commit whose aggregate blob size exceeds the extraction bound"
  fi
  [[ "$(<"$aggregate_failure")" == *"evidence tree aggregate blob size exceeds the $EVIDENCE_MAX_TOTAL_BYTES-byte limit"* ]] \
    || die "self-test aggregate-oversized evidence commit failed for the wrong reason"
  test ! -e "$aggregate_extraction/$synthetic_root" \
    || die "self-test reached evidence extraction before rejecting aggregate blob size"

  local oversized_extraction="$scratch/per-blob-oversized-extraction"
  local oversized_failure="$scratch/per-blob-oversized-failure.log"
  mkdir "$oversized_extraction"
  if (
    cd "$synthetic_repo"
    validate_evidence_tree_blob_bounds \
      "$oversized_commit" "$synthetic_root" 1 "$oversized_paths" \
      "$scratch/per-blob-oversized-tree-objects.z"
    git archive --format=tar "$oversized_commit" "$synthetic_root" | tar -xf - -C "$oversized_extraction"
  ) > /dev/null 2> "$oversized_failure"; then
    die "self-test accepted an evidence commit containing an oversized blob"
  fi
  [[ "$(<"$oversized_failure")" == *"evidence tree blob exceeds the $EVIDENCE_MAX_BLOB_BYTES-byte limit"* ]] \
    || die "self-test per-blob-oversized evidence commit failed for the wrong reason"
  test ! -e "$oversized_extraction/$synthetic_root" \
    || die "self-test reached evidence extraction before rejecting per-blob size"
}

self_test() {
  require_command git
  require_command jq
  require_command mkfifo
  require_command python3
  require_command sha256sum
  require_command tar
  local scratch
  scratch="$(mktemp -d)"
  chmod 700 "$scratch"
  trap 'rm -rf -- "$scratch"' RETURN
  local sha1_a="1111111111111111111111111111111111111111"
  local sha256_a="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  local receipt="$scratch/receipt.json"
  self_test_attestation_selection
  printf '%s' '{"schemaVersion":3,"version":"0.12.33","releaseTag":"v0.12.33","sourceSha":"1111111111111111111111111111111111111111","workflowRunId":"123","workflowRunAttempt":"1","releaseCandidateArtifactId":"456","releaseCandidateArtifactZipSha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","checksumManifestSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","evidenceRef":"refs/heads/evidence/v0.12.33-release-run-123-attempt-1","evidenceCommitSha":"3333333333333333333333333333333333333333","macosPassed":true,"macosAcceptanceSha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","macosQuietResultSha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","macosDeliberateConcurrencyResultSha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","windowsPassed":true,"windowsResultSha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","stockChromePassed":true,"stockChrome":true,"stockChromeResultSha256":"9999999999999999999999999999999999999999999999999999999999999999"}' > "$receipt"
  validate_receipt "$receipt" 0.12.33 v0.12.33 "$sha1_a" 123 1 "$sha256_a" \
    || die "self-test rejected a valid canonical schema-3 receipt"
  jq -cS . "$receipt" > "$scratch/noncanonical.json"
  if validate_receipt "$scratch/noncanonical.json" 0.12.33 v0.12.33 "$sha1_a" 123 1 "$sha256_a"; then
    die "self-test accepted reordered receipt keys"
  fi
  jq -c '.schemaVersion = 2' "$receipt" > "$scratch/stale.json"
  if validate_receipt "$scratch/stale.json" 0.12.33 v0.12.33 "$sha1_a" 123 1 "$sha256_a"; then
    die "self-test accepted a stale schema-2 receipt"
  fi
  mkdir "$scratch/safe"
  printf '%s\n' '{"passed":true,"credentialRetained":false,"expectedVisibleWindowTitle":"LBB Windows Acceptance - ARMED"}' > "$scratch/safe/result.json"
  scan_evidence_for_leaks "$scratch/safe"
  printf '%s\n' '{"passed":true,"authorization":"Bearer leaked-value-123456"}' > "$scratch/safe/result.json"
  if scan_evidence_for_leaks "$scratch/safe" >/dev/null 2>&1; then
    die "self-test accepted a populated authorization field"
  fi
  printf '%s\n' '{"passed":true,"passed":false}' > "$scratch/safe/result.json"
  if scan_evidence_for_leaks "$scratch/safe" >/dev/null 2>&1; then
    die "self-test accepted duplicate JSON keys"
  fi
  printf '%s\n' '{"passed":true,"note":"C:\\Users\\Alice\\private.txt"}' > "$scratch/safe/result.json"
  if scan_evidence_for_leaks "$scratch/safe" >/dev/null 2>&1; then
    die "self-test accepted a decoded Windows home path hidden by JSON escaping"
  fi

  EXPECTED_RELEASE_CANDIDATE_BINDING="$scratch/expected-binding.json"
  printf '%s' '{"schemaVersion":3,"version":"0.12.33","releaseTag":"v0.12.33","repository":"flrngel/local-browser-bridge","sourceSha":"1111111111111111111111111111111111111111","workflowRunId":"123","workflowRunAttempt":"2","workflowEvent":"workflow_dispatch","workflowRef":"refs/heads/main","workflowPath":".github/workflows/deploy.yml","artifactId":"456","artifactName":"release-candidate","artifactZipBytes":789,"artifactZipSha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","checksumManifestSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","attestationInvocationUri":"https://github.com/flrngel/local-browser-bridge/actions/runs/123/attempts/2","attestedAssetCount":5,"githubHostedRunner":true,"assets":[]}' > "$EXPECTED_RELEASE_CANDIDATE_BINDING"
  printf '%s' "{\"releaseCandidateBinding\":$(<"$EXPECTED_RELEASE_CANDIDATE_BINDING")}" > "$scratch/current-binding.json"
  assert_release_candidate_binding "$scratch/current-binding.json" '.releaseCandidateBinding'
  jq -c '.releaseCandidateBinding.workflowRunAttempt = "1"' "$scratch/current-binding.json" > "$scratch/replayed-binding.json"
  if (assert_release_candidate_binding "$scratch/replayed-binding.json" '.releaseCandidateBinding') >/dev/null 2>&1; then
    die "self-test accepted release-candidate evidence replayed from another workflow attempt"
  fi
  assert_utc_interval \
    "2026-08-24T00:00:00.0000000Z" "2026-08-24T00:00:01.0000000Z" 2 \
    "self-test canonical timestamp" \
    || die "self-test rejected canonical Z-suffixed timestamps"
  if assert_utc_interval \
      "2026-08-24T00:00:00.0000000+00:00" "2026-08-24T00:00:01.0000000Z" 2 \
      "self-test offset timestamp" >/dev/null 2>&1; then
    die "self-test accepted a noncanonical +00:00 timestamp"
  fi
  if assert_utc_interval \
      "2026-08-24T00:00:02.0000000Z" "2026-08-24T00:00:01.0000000Z" 2 \
      "self-test approval expired before dispatch" >/dev/null 2>&1; then
    die "self-test accepted first covered dispatch after approval expiry"
  fi
  self_test_evidence_blob_bounds "$scratch"

  local synthetic_steps
  synthetic_steps="$(printf 'step-%02d.json\n' {1..62} | jq -Rsc 'split("\n")[:-1]')"
  jq -cn --argjson names "$synthetic_steps" '{steps: ($names | map({evidence: ., passed: true}))}' > "$scratch/windows-steps.json"
  validate_windows_step_inventory "$scratch/windows-steps.json" "$synthetic_steps" \
    || die "self-test rejected an exact ordered 62-step Windows inventory"
  jq -c '.steps |= .[:-1]' "$scratch/windows-steps.json" > "$scratch/windows-short.json"
  if validate_windows_step_inventory "$scratch/windows-short.json" "$synthetic_steps"; then
    die "self-test accepted a truncated Windows step inventory"
  fi
  jq -c '.steps[0:2] |= reverse' "$scratch/windows-steps.json" > "$scratch/windows-reordered.json"
  if validate_windows_step_inventory "$scratch/windows-reordered.json" "$synthetic_steps"; then
    die "self-test accepted a reordered Windows step inventory"
  fi
  jq -cn '
      {requestId:"0123456789abcdef0123456789abcdef",status:"action-required",maximumClickAttempts:1,inputStateAtPublication:"not-started"}
      + (reduce range(1;30) as $n ({}; .[("k" + ($n|tostring))] = false))
    ' > "$scratch/arm-request.json"
  jq -cn '
      {requestId:"0123456789abcdef0123456789abcdef",stableSamplesRequired:3,stableSamplesObserved:3}
      + (reduce range(1;18) as $n ({}; .[("k" + ($n|tostring))] = false))
    ' > "$scratch/arm-received.json"
  validate_windows_arm_pair_identity "$scratch/arm-request.json" "$scratch/arm-received.json" \
    || die "self-test rejected a complete one-shot arm identity"
  jq -c 'del(.requestId) | .missing = false' "$scratch/arm-received.json" > "$scratch/arm-missing-request-id.json"
  if validate_windows_arm_pair_identity "$scratch/arm-request.json" "$scratch/arm-missing-request-id.json"; then
    die "self-test accepted an arm receipt without the request ID"
  fi

  jq -cn '
      ["status","browser.control.start","browser.control.status","browser.control.stop","tabs.list","tabs.activate","tabs.new","tabs.close","page.observe","page.navigate","page.back","page.forward","page.reload","page.click","page.fill","page.select","page.key","page.scroll","page.clickAt","page.typeText","page.evaluate","page.waitFor","page.hover","page.batch","page.handleDialog"] as $names
      | {methodCount: 25, methods: ($names | map({name: ., passed: true, commandInvoked: true, resultVerified: true, postconditionVerified: true}))}
    ' > "$scratch/chrome-matrix.json"
  validate_stock_chrome_matrix_identity "$scratch/chrome-matrix.json" \
    || die "self-test rejected the exact ordered 25-method stock-Chrome matrix"
  jq -c '.methods = []' "$scratch/chrome-matrix.json" > "$scratch/chrome-empty.json"
  if validate_stock_chrome_matrix_identity "$scratch/chrome-empty.json"; then
    die "self-test accepted an empty stock-Chrome method matrix"
  fi

  jq -cn '{lanes:{quiet:{screenshots:[range(0;6)|{sha256:("file-" + tostring),pixelSha256:("pixel-" + tostring)}]},deliberateConcurrency:{screenshots:[range(6;12)|{sha256:("file-" + tostring),pixelSha256:("pixel-" + tostring)}]}}}' > "$scratch/mac-distinct.json"
  validate_global_mac_screenshot_hashes "$scratch/mac-distinct.json" \
    || die "self-test rejected twelve globally distinct macOS screenshot hashes"
  jq -c '.lanes.deliberateConcurrency.screenshots[0].sha256 = "file-0"' "$scratch/mac-distinct.json" > "$scratch/mac-overlap.json"
  if validate_global_mac_screenshot_hashes "$scratch/mac-overlap.json"; then
    die "self-test accepted a macOS screenshot hash replayed across lanes"
  fi
  jq -c '.lanes.deliberateConcurrency.screenshots[0].pixelSha256 = "pixel-0"' "$scratch/mac-distinct.json" > "$scratch/mac-pixel-overlap.json"
  if validate_global_mac_screenshot_hashes "$scratch/mac-pixel-overlap.json"; then
    die "self-test accepted decoded macOS pixels replayed across lanes"
  fi

  jq -cn --arg lane quiet --argjson deliberate false '
      {
        pointerEvidence: {
          requestedLane: $lane,
          quietObserved: true,
          concurrentSharedSeatActivityObserved: false,
          unknownObserved: false,
          rawCursorPositionsRetained: false,
          rawPlatformActivityCountersRetained: false,
          rawHidSystemCountersRetained: false,
          hidSystemActivityClaimedAsPhysical: false
        },
        appShareHandoff: {
          requested: $deliberate,
          requestPublicationAcknowledged: $deliberate,
          startReceiptAcknowledged: $deliberate,
          completePublicationAcknowledged: $deliberate,
          promptClosed: $deliberate,
          exactAppBundleObserved: $deliberate,
          exactWindowObserved: $deliberate,
          exactButtonObserved: $deliberate,
          buttonDisabledAfterAction: $deliberate,
          acceptanceButtonActionObserved: $deliberate,
          appShareSurfaceObservedAtProductBoundaries: $deliberate,
          sharedHidInputObserved: null,
          sampledSharedContextUnchanged: $deliberate,
          authorityRefreshedAfterReceipt: $deliberate,
          authorityFreshAtDispatch: $deliberate,
          actionDispatched: $deliberate,
          targetPostconditionObserved: $deliberate,
          productBoundaryQuiet: $deliberate,
          independentBoundaryQuiet: $deliberate,
          physicalHumanProvenanceClaimed: false,
          cryptographicToolIdentityClaimed: false,
          orchestrationNotProductControl: true,
          markerNotificationOnly: false,
          markerAcceptedAsProductAuthority: false,
          rawAppIdentityRetainedInResult: false,
          rawPointerDataRetained: false
        }
      }
    ' > "$scratch/mac-quiet-app-share-contract.json"
  jq -cn --arg lane deliberate-concurrency --argjson deliberate true '
      {
        pointerEvidence: {
          requestedLane: $lane,
          quietObserved: true,
          concurrentSharedSeatActivityObserved: false,
          unknownObserved: false,
          rawCursorPositionsRetained: false,
          rawPlatformActivityCountersRetained: false,
          rawHidSystemCountersRetained: false,
          hidSystemActivityClaimedAsPhysical: false
        },
        appShareHandoff: {
          requested: $deliberate,
          requestPublicationAcknowledged: $deliberate,
          startReceiptAcknowledged: $deliberate,
          completePublicationAcknowledged: $deliberate,
          promptClosed: $deliberate,
          exactAppBundleObserved: $deliberate,
          exactWindowObserved: $deliberate,
          exactButtonObserved: $deliberate,
          buttonDisabledAfterAction: $deliberate,
          acceptanceButtonActionObserved: $deliberate,
          appShareSurfaceObservedAtProductBoundaries: $deliberate,
          sharedHidInputObserved: false,
          sampledSharedContextUnchanged: $deliberate,
          authorityRefreshedAfterReceipt: $deliberate,
          authorityFreshAtDispatch: $deliberate,
          actionDispatched: $deliberate,
          targetPostconditionObserved: $deliberate,
          productBoundaryQuiet: $deliberate,
          independentBoundaryQuiet: $deliberate,
          physicalHumanProvenanceClaimed: false,
          cryptographicToolIdentityClaimed: false,
          orchestrationNotProductControl: true,
          markerNotificationOnly: false,
          markerAcceptedAsProductAuthority: false,
          rawAppIdentityRetainedInResult: false,
          rawPointerDataRetained: false
        }
      }
    ' > "$scratch/mac-deliberate-app-share-contract.json"
  validate_macos_pointer_app_share_contract "$scratch/mac-quiet-app-share-contract.json" quiet \
    || die "self-test rejected the exact quiet pointer/app-share contract"
  validate_macos_pointer_app_share_contract \
    "$scratch/mac-deliberate-app-share-contract.json" deliberate-concurrency \
    || die "self-test rejected the exact deliberate app-share contract"
  jq -c '.pointerEvidence.concurrentSharedSeatActivityObserved = true' \
    "$scratch/mac-deliberate-app-share-contract.json" \
    > "$scratch/mac-deliberate-contaminated-pointer-contract.json"
  if validate_macos_pointer_app_share_contract \
      "$scratch/mac-deliberate-contaminated-pointer-contract.json" deliberate-concurrency; then
    die "self-test accepted mandatory sustained-motion/contamination evidence"
  fi
  jq -c '.operatorHandoff = {legacyContract:true}' \
    "$scratch/mac-deliberate-app-share-contract.json" \
    > "$scratch/mac-deliberate-legacy-operator-contract.json"
  if validate_macos_pointer_app_share_contract \
      "$scratch/mac-deliberate-legacy-operator-contract.json" deliberate-concurrency; then
    die "self-test accepted the removed legacy pointer operator contract"
  fi
  jq -c '.appShareHandoff.authorityRefreshedAfterReceipt = false' \
    "$scratch/mac-deliberate-app-share-contract.json" \
    > "$scratch/mac-deliberate-unrefreshed-authority-contract.json"
  if validate_macos_pointer_app_share_contract \
      "$scratch/mac-deliberate-unrefreshed-authority-contract.json" deliberate-concurrency; then
    die "self-test accepted deliberate app-share authority that was not refreshed after receipt"
  fi
  jq -c '.appShareHandoff.authorityFreshAtDispatch = false' \
    "$scratch/mac-deliberate-app-share-contract.json" \
    > "$scratch/mac-deliberate-stale-dispatch-authority-contract.json"
  if validate_macos_pointer_app_share_contract \
      "$scratch/mac-deliberate-stale-dispatch-authority-contract.json" deliberate-concurrency; then
    die "self-test accepted deliberate app-share authority that was stale at dispatch"
  fi

  jq -cn '{assertions:{details:[{name:"self-test"}]}}' \
    > "$scratch/mac-quiet-authority-assertions.json"
  validate_macos_authority_assertion_contract \
    "$scratch/mac-quiet-authority-assertions.json" quiet \
    || die "self-test rejected canonical quiet authority assertions"
  jq -cn '{assertions:{details:[
      {name:"self-test"},
      {name:"app-share receipt retained the exact persistent share"},
      {name:"post-handoff share action authority is fresh and exact"},
      {name:"app-share handoff and frame refresh caused no target mutation"},
      {name:"post-handoff share action authority remained fresh at dispatch"}
    ]}}' > "$scratch/mac-deliberate-authority-assertions.json"
  validate_macos_authority_assertion_contract \
    "$scratch/mac-deliberate-authority-assertions.json" deliberate-concurrency \
    || die "self-test rejected canonical deliberate authority assertions"
  jq -c 'del(.assertions.details[2])' \
    "$scratch/mac-deliberate-authority-assertions.json" \
    > "$scratch/mac-deliberate-missing-authority-assertion.json"
  if validate_macos_authority_assertion_contract \
      "$scratch/mac-deliberate-missing-authority-assertion.json" deliberate-concurrency; then
    die "self-test accepted a missing deliberate authority assertion"
  fi
  jq -c '.assertions.details += [.assertions.details[0]]' \
    "$scratch/mac-deliberate-authority-assertions.json" \
    > "$scratch/mac-deliberate-duplicate-authority-assertion.json"
  if validate_macos_authority_assertion_contract \
      "$scratch/mac-deliberate-duplicate-authority-assertion.json" deliberate-concurrency; then
    die "self-test accepted a duplicate authority assertion name"
  fi
  jq -c '.assertions.details += [{name:"post-handoff share action authority is fresh and exact"}]' \
    "$scratch/mac-quiet-authority-assertions.json" \
    > "$scratch/mac-quiet-deliberate-authority-assertion.json"
  if validate_macos_authority_assertion_contract \
      "$scratch/mac-quiet-deliberate-authority-assertion.json" quiet; then
    die "self-test accepted a deliberate authority assertion in the quiet lane"
  fi

  local app_share_request="$scratch/macos-app-share-request.json"
  local app_share_start="$scratch/macos-app-share-start.json"
  local app_share_complete="$scratch/macos-app-share-complete.json"
  local app_share_lane_result="$scratch/macos-app-share-lane-result.json"
  local app_share_request_sha256 app_share_start_sha256 app_share_complete_sha256
  jq -cn --arg version "$EVIDENCE_PRODUCT_VERSION" '{
      schemaVersion: 2,
      kind: "macos-app-share-concurrency-handoff-request",
      productVersion: $version,
      requestId: "0123456789abcdef0123456789abcdef",
      createdAt: "2026-08-24T00:00:00.000Z",
      expiresAt: "2026-08-24T00:05:00.000Z",
      runnerPid: 101,
      promptPid: 202,
      expectedBundleIdentifier: "dev.flrngel.local-browser-bridge.acceptance.app-share",
      expectedWindowTitle: "LBB macOS Acceptance App Share",
      expectedButtonText: "START APP-SHARE CHECK",
      expectedButtonAccessibilityIdentifier: "lbb-app-share-start",
      expectedButtonEnabledAfterDelivery: true,
      exactAppObserved: true,
      exactWindowObserved: true,
      requestDelivered: true,
      panelOnScreen: true,
      panelNonactivating: true,
      notificationOnly: false,
      exactAppShareRequired: true,
      physicalHumanProvenanceRequired: false,
      acceptedAsProductAuthority: false
    }' > "$app_share_request"
  app_share_request_sha256="$(sha256_file "$app_share_request")"
  jq -cn \
    --arg version "$EVIDENCE_PRODUCT_VERSION" \
    --arg request_sha256 "$app_share_request_sha256" '{
      acceptedAsAuthority: false,
      buttonAccepted: true,
      buttonActionObserved: true,
      createdAt: "2026-08-24T00:00:01.000Z",
      cryptographicToolIdentityClaimed: false,
      kind: "macos-app-share-concurrency-handoff-start",
      physicalHumanProvenanceClaimed: false,
      productVersion: $version,
      promptPid: 202,
      requestId: "0123456789abcdef0123456789abcdef",
      requestSha256: $request_sha256,
      schemaVersion: 2
    }' > "$app_share_start"
  app_share_start_sha256="$(sha256_file "$app_share_start")"
  jq -cn \
    --arg version "$EVIDENCE_PRODUCT_VERSION" \
    --arg request_sha256 "$app_share_request_sha256" \
    --arg start_sha256 "$app_share_start_sha256" '{
      acceptedAsAuthority: false,
      buttonRemainedDisabledDuringProductAction: true,
      createdAt: "2026-08-24T00:00:03.500Z",
      cryptographicToolIdentityClaimed: false,
      handoffStateSequenceBound: true,
      kind: "macos-app-share-concurrency-handoff-complete",
      physicalHumanProvenanceClaimed: false,
      productActionCompletedAt: "2026-08-24T00:00:03.000Z",
      productActionStartedAt: "2026-08-24T00:00:02.000Z",
      productVersion: $version,
      promptPid: 202,
      requestId: "0123456789abcdef0123456789abcdef",
      requestSha256: $request_sha256,
      schemaVersion: 2,
      startReceiptSha256: $start_sha256
    }' > "$app_share_complete"
  app_share_complete_sha256="$(sha256_file "$app_share_complete")"
  jq -cn '{
      startedAt: "2026-08-23T23:59:59.000Z",
      capturedAt: "2026-08-24T00:00:04.000Z"
    }' > "$app_share_lane_result"
  validate_macos_app_share_marker_chain \
    "$app_share_request" "$app_share_start" "$app_share_complete" \
    "$app_share_request_sha256" "$app_share_start_sha256" "$app_share_complete_sha256" \
    "$app_share_lane_result" \
    || die "self-test rejected a valid exact-app-share request/start/complete chain"

  jq -c '.requestSha256 = "0000000000000000000000000000000000000000000000000000000000000000"' \
    "$app_share_start" > "$scratch/macos-app-share-start-replayed.json"
  if validate_macos_app_share_marker_chain \
      "$app_share_request" "$scratch/macos-app-share-start-replayed.json" "$app_share_complete" \
      "$app_share_request_sha256" \
      "$(sha256_file "$scratch/macos-app-share-start-replayed.json")" \
      "$app_share_complete_sha256" "$app_share_lane_result" >/dev/null 2>&1; then
    die "self-test accepted an exact-app-share start receipt with a replayed request hash"
  fi
  jq -c '.productActionStartedAt = "2026-08-24T00:00:04.000Z"' \
    "$app_share_complete" > "$scratch/macos-app-share-complete-reordered.json"
  if validate_macos_app_share_marker_chain \
      "$app_share_request" "$app_share_start" "$scratch/macos-app-share-complete-reordered.json" \
      "$app_share_request_sha256" "$app_share_start_sha256" \
      "$(sha256_file "$scratch/macos-app-share-complete-reordered.json")" \
      "$app_share_lane_result" >/dev/null 2>&1; then
    die "self-test accepted a non-forward exact-app-share product timestamp chain"
  fi
  jq -c '
      .productActionStartedAt = "2026-08-24T00:00:11.100Z"
      | .productActionCompletedAt = "2026-08-24T00:00:11.200Z"
      | .createdAt = "2026-08-24T00:00:11.300Z"
    ' "$app_share_complete" > "$scratch/macos-app-share-complete-late-after-start.json"
  jq -cn '{
      startedAt: "2026-08-23T23:59:59.000Z",
      capturedAt: "2026-08-24T00:00:12.000Z"
    }' > "$scratch/macos-app-share-late-lane-result.json"
  if validate_macos_app_share_marker_chain \
      "$app_share_request" "$app_share_start" \
      "$scratch/macos-app-share-complete-late-after-start.json" \
      "$app_share_request_sha256" "$app_share_start_sha256" \
      "$(sha256_file "$scratch/macos-app-share-complete-late-after-start.json")" \
      "$scratch/macos-app-share-late-lane-result.json" >/dev/null 2>&1; then
    die "self-test accepted an app-share completion more than ten seconds after its start receipt"
  fi

  jq -cn \
    --arg request_sha256 "$app_share_request_sha256" \
    --arg start_sha256 "$app_share_start_sha256" \
    --arg complete_sha256 "$app_share_complete_sha256" '{
      lanes: {deliberateConcurrency: {operatorMarkers: [
        {file: "operator/macos-app-share-concurrency-handoff-request.json", sha256: $request_sha256},
        {file: "operator/macos-app-share-concurrency-handoff-start.json", sha256: $start_sha256},
        {file: "operator/macos-app-share-concurrency-handoff-complete.json", sha256: $complete_sha256}
      ]}}
    }' > "$scratch/mac-app-share-marker-inventory.json"
  validate_macos_app_share_operator_inventory \
    "$scratch/mac-app-share-marker-inventory.json" deliberateConcurrency \
    || die "self-test rejected the exact three-file app-share marker inventory"
  jq -c '.lanes.deliberateConcurrency.operatorMarkers += [{
      file: "operator/macos-physical-pointer-concurrency-handoff-complete.json",
      sha256: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
    }]' "$scratch/mac-app-share-marker-inventory.json" \
    > "$scratch/mac-app-share-marker-inventory-with-physical.json"
  if validate_macos_app_share_operator_inventory \
      "$scratch/mac-app-share-marker-inventory-with-physical.json" deliberateConcurrency; then
    die "self-test allowed an optional physical-pointer artifact to satisfy the mandatory app-share allowlist"
  fi

  printf '%s' '{"aggregateChecks":{"passingResultSchemaVersion":8}}' > "$scratch/mac-schema-aggregate.json"
  printf '%s' '{"schemaVersion":8}' > "$scratch/mac-schema-quiet.json"
  printf '%s' '{"schemaVersion":8}' > "$scratch/mac-schema-deliberate.json"
  validate_mac_result_schema_binding \
    "$scratch/mac-schema-aggregate.json" \
    "$scratch/mac-schema-quiet.json" \
    "$scratch/mac-schema-deliberate.json" \
    || die "self-test rejected aligned macOS result schemas"
  jq -c '.aggregateChecks.passingResultSchemaVersion = 7' \
    "$scratch/mac-schema-aggregate.json" > "$scratch/mac-schema-stale-aggregate.json"
  if validate_mac_result_schema_binding \
      "$scratch/mac-schema-stale-aggregate.json" \
      "$scratch/mac-schema-quiet.json" \
      "$scratch/mac-schema-deliberate.json"; then
    die "self-test accepted a stale macOS aggregate result schema"
  fi
  jq -c '.schemaVersion = 7' \
    "$scratch/mac-schema-deliberate.json" > "$scratch/mac-schema-stale-deliberate.json"
  if validate_mac_result_schema_binding \
      "$scratch/mac-schema-aggregate.json" \
      "$scratch/mac-schema-quiet.json" \
      "$scratch/mac-schema-stale-deliberate.json"; then
    die "self-test accepted mismatched macOS lane result schemas"
  fi

  local mac_harness_source_binding="$scratch/mac-harness-source-binding.json"
  local mac_harness_aggregate="$scratch/mac-harness-aggregate.json"
  local mac_harness_quiet="$scratch/mac-harness-quiet.json"
  local mac_harness_deliberate="$scratch/mac-harness-deliberate.json"
  write_mac_harness_source_binding "$mac_harness_source_binding" \
    || die "self-test could not independently hash the exact macOS harness sources"
  jq -cn --slurpfile expected "$mac_harness_source_binding" \
    '{bindings:{harness:$expected[0]}}' > "$mac_harness_aggregate"
  jq -cn --slurpfile expected "$mac_harness_source_binding" \
    '{harness:$expected[0]}' > "$mac_harness_quiet"
  jq -cn --slurpfile expected "$mac_harness_source_binding" \
    '{harness:$expected[0]}' > "$mac_harness_deliberate"
  validate_mac_harness_source_binding \
    "$mac_harness_source_binding" "$mac_harness_aggregate" \
    "$mac_harness_quiet" "$mac_harness_deliberate" \
    || die "self-test rejected exact macOS harness source hashes"

  local harness_field
  local wrong_harness_sha256="0000000000000000000000000000000000000000000000000000000000000000"
  for harness_field in \
    runnerSha256 fixtureSha256 systemProbeSha256 \
    appShareHandoffSha256 physicalPointerHandoffSha256 \
    acceptanceFinalizerSha256; do
    jq -c --arg field "$harness_field" --arg wrong "$wrong_harness_sha256" \
      '.bindings.harness[$field] = $wrong' \
      "$mac_harness_aggregate" > "$scratch/mac-harness-tampered-aggregate.json"
    if validate_mac_harness_source_binding \
        "$mac_harness_source_binding" "$scratch/mac-harness-tampered-aggregate.json" \
        "$mac_harness_quiet" "$mac_harness_deliberate"; then
      die "self-test accepted a macOS aggregate harness hash mismatch: $harness_field"
    fi

    jq -c --arg field "$harness_field" --arg wrong "$wrong_harness_sha256" \
      '.harness[$field] = $wrong' \
      "$mac_harness_quiet" > "$scratch/mac-harness-tampered-quiet.json"
    if validate_mac_harness_source_binding \
        "$mac_harness_source_binding" "$mac_harness_aggregate" \
        "$scratch/mac-harness-tampered-quiet.json" "$mac_harness_deliberate"; then
      die "self-test accepted a quiet-lane harness hash mismatch: $harness_field"
    fi

    jq -c --arg field "$harness_field" --arg wrong "$wrong_harness_sha256" \
      '.harness[$field] = $wrong' \
      "$mac_harness_deliberate" > "$scratch/mac-harness-tampered-deliberate.json"
    if validate_mac_harness_source_binding \
        "$mac_harness_source_binding" "$mac_harness_aggregate" \
        "$mac_harness_quiet" "$scratch/mac-harness-tampered-deliberate.json"; then
      die "self-test accepted a deliberate-lane harness hash mismatch: $harness_field"
    fi
  done

  python3 - "$scratch/valid.png" "$scratch/empty-idat.png" "$scratch/encoding-replay.png" "$scratch/pixel-sha256.txt" <<'PY'
import hashlib, struct, sys, zlib
def chunk(kind, payload):
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload) & 0xffffffff)
ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
signature = b"\x89PNG\r\n\x1a\n"
scanline = b"\0\0\0\0\xff"
open(sys.argv[1], "wb").write(signature + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(scanline)) + chunk(b"IEND", b""))
open(sys.argv[2], "wb").write(signature + chunk(b"IHDR", ihdr) + chunk(b"IDAT", b"") + chunk(b"IEND", b""))
open(sys.argv[3], "wb").write(signature + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(scanline, level=0)) + chunk(b"IEND", b""))
open(sys.argv[4], "w", encoding="ascii").write(hashlib.sha256(b"1x1\0" + scanline[1:]).hexdigest())
PY
  local self_test_pixel_sha256
  self_test_pixel_sha256="$(<"$scratch/pixel-sha256.txt")"
  verify_png_dimensions "$scratch/valid.png" 1 1 "$self_test_pixel_sha256"
  verify_png_dimensions "$scratch/encoding-replay.png" 1 1 "$self_test_pixel_sha256"
  test "$(sha256_file "$scratch/valid.png")" != "$(sha256_file "$scratch/encoding-replay.png")" \
    || die "self-test failed to construct a byte-distinct PNG encoding replay"
  if (verify_png_dimensions "$scratch/empty-idat.png" 1 1) >/dev/null 2>&1; then
    die "self-test accepted an undecodable zero-length PNG IDAT"
  fi
  printf '%s\n' "Release acceptance evidence verifier self-test passed."
}

if [[ "${1:-}" == "--self-test" ]]; then
  test "$#" = 1 || die "--self-test does not accept additional arguments"
  self_test
  exit 0
fi

test "$#" = 2 || die "usage: $SCRIPT_NAME <canonical-receipt.json> <downloaded-candidate-directory>"
readonly RECEIPT_FILE="$1"
readonly CANDIDATE_DIR="$2"

for variable in GITHUB_REPOSITORY CANDIDATE_RUN_ID CANDIDATE_RUN_ATTEMPT RELEASE_VERSION RELEASE_TAG VERIFIED_SOURCE_SHA GH_TOKEN; do
  test -n "${!variable:-}" || die "required environment variable is empty: $variable"
done
for command_name in awk cmp cp env find gh git jq mkfifo pwsh python3 sed sha256sum tar; do require_command "$command_name"; done
test "$RELEASE_VERSION" = "$EVIDENCE_PRODUCT_VERSION" || die "this evidence gate is bound only to version $EVIDENCE_PRODUCT_VERSION"
test "$RELEASE_TAG" = "v$RELEASE_VERSION" || die "release tag does not match the intended release version"
is_sha1 "$VERIFIED_SOURCE_SHA" || die "verified source SHA is invalid"
is_positive_integer "$CANDIDATE_RUN_ID" || die "workflow run ID is invalid"
is_positive_integer "$CANDIDATE_RUN_ATTEMPT" || die "workflow run attempt is invalid"
test -d "$CANDIDATE_DIR" && test ! -L "$CANDIDATE_DIR" || die "candidate directory is invalid"
readonly FROZEN_MANIFEST_SHA256="$(sha256_file "$CANDIDATE_DIR/SHA256SUMS.txt")"
validate_receipt "$RECEIPT_FILE" "$RELEASE_VERSION" "$RELEASE_TAG" "$VERIFIED_SOURCE_SHA" \
  "$CANDIDATE_RUN_ID" "$CANDIDATE_RUN_ATTEMPT" "$FROZEN_MANIFEST_SHA256" \
  || die "protected schema-3 acceptance receipt is noncanonical, stale, or incorrectly bound"

SCRATCH_ROOT="$(mktemp -d)"
chmod 700 "$SCRATCH_ROOT"
trap 'rm -rf -- "$SCRATCH_ROOT"' EXIT
verify_raw_release_candidate "$RECEIPT_FILE" "$CANDIDATE_DIR" "$SCRATCH_ROOT"
verify_evidence_commit "$RECEIPT_FILE" "$CANDIDATE_DIR" "$SCRATCH_ROOT"
printf '%s\n' "Release acceptance evidence commit and exact workflow artifact passed schema-3 verification."
