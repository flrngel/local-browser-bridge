#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage: scripts/fetch-verify-release-candidate.sh VERSION RUN_ID RUN_ATTEMPT ARTIFACT_ID SOURCE_SHA TAG_OBJECT_SHA DESTINATION
       scripts/fetch-verify-release-candidate.sh --self-test

Downloads one GitHub Actions release-candidate artifact into a new private
directory, binds it to the exact tagged workflow attempt, verifies the flat
five-file payload and every GitHub build attestation, and writes a sanitized
candidate-binding.json. It never executes candidate bytes.
EOF
  exit 2
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

run_attestation_selection_self_test() {
  command -v jq >/dev/null || {
    echo "Required command is unavailable: jq" >&2
    return 1
  }
  local repository="flrngel/local-browser-bridge"
  local run_id="123456789"
  local source="1111111111111111111111111111111111111111"
  local tag_ref="refs/tags/v0.0.0"
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
    echo "Attestation selection self-test rejected one old plus one current result." >&2
    return 1
  fi
  if jq -cn --argjson old "$old" '[$old]' |
    verify_exact_attempt_attestation_set - "$current_invocation" "$repository" "$run_id" \
      "$source" "$tag_ref" "$workflow" "$subject_name" "$subject_sha256"; then
    echo "Attestation selection self-test accepted an old-only result." >&2
    return 1
  fi
  if jq -cn --argjson current "$current" '[$current,$current]' |
    verify_exact_attempt_attestation_set - "$current_invocation" "$repository" "$run_id" \
      "$source" "$tag_ref" "$workflow" "$subject_name" "$subject_sha256"; then
    echo "Attestation selection self-test accepted duplicate current results." >&2
    return 1
  fi
  if jq -cn --argjson old "$old" --argjson current "$current" \
      '[$old,($current | del(.verificationResult.signature.certificate))]' |
    verify_exact_attempt_attestation_set - "$current_invocation" "$repository" "$run_id" \
      "$source" "$tag_ref" "$workflow" "$subject_name" "$subject_sha256"; then
    echo "Attestation selection self-test accepted a malformed current result." >&2
    return 1
  fi
  if jq -cn --argjson old "$old" --argjson current "$current" \
      '[$old,($current | .verificationResult.statement.subject[0].name = "wrong.bin")]' |
    verify_exact_attempt_attestation_set - "$current_invocation" "$repository" "$run_id" \
      "$source" "$tag_ref" "$workflow" "$subject_name" "$subject_sha256"; then
    echo "Attestation selection self-test accepted a wrong current subject." >&2
    return 1
  fi
  echo "Release-candidate attestation selection self-test passed."
}

if [[ $# -eq 1 && $1 == --self-test ]]; then
  run_attestation_selection_self_test
  exit $?
fi

[[ $# -eq 7 ]] || usage
VERSION=$1
RUN_ID=$2
RUN_ATTEMPT=$3
ARTIFACT_ID=$4
SOURCE_SHA=$5
TAG_OBJECT_SHA=$6
DESTINATION=$7

[[ $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
[[ $RUN_ID =~ ^[1-9][0-9]*$ ]] || usage
[[ $RUN_ATTEMPT =~ ^[1-9][0-9]*$ ]] || usage
[[ $ARTIFACT_ID =~ ^[1-9][0-9]*$ ]] || usage
[[ $SOURCE_SHA =~ ^[0-9a-f]{40}$ ]] || usage
[[ $TAG_OBJECT_SHA =~ ^[0-9a-f]{40}$ ]] || usage
[[ $DESTINATION == /* ]] || { echo "DESTINATION must be absolute." >&2; exit 2; }

for command_name in gh git jq shasum file wc find awk sort cmp chmod mkdir \
  tail od sed grep tr mv dirname basename python3; do
  command -v "$command_name" >/dev/null || {
    echo "Required command is unavailable: $command_name" >&2
    exit 1
  }
done

umask 077
REPOSITORY="flrngel/local-browser-bridge"
TAG="v$VERSION"
SCRIPT_PATH=$(python3 - "${BASH_SOURCE[0]}" <<'PY'
import os
import stat
import sys

supplied = os.path.abspath(sys.argv[1])
canonical = os.path.realpath(supplied)
if supplied != canonical:
    raise SystemExit("candidate trust script must be executed through its canonical path without symlink traversal")
info = os.lstat(canonical)
if not stat.S_ISREG(info.st_mode) or stat.S_ISLNK(info.st_mode) or info.st_nlink != 1:
    raise SystemExit("candidate trust script must be one ordinary, singly linked file")
if info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) & 0o022:
    raise SystemExit("candidate trust script must be current-user owned and not group/other writable")
print(canonical)
PY
) || exit 1
SCRIPT_DIRECTORY=$(dirname "$SCRIPT_PATH")
SOURCE_ROOT=$(git -C "$SCRIPT_DIRECTORY" rev-parse --show-toplevel)
SOURCE_ROOT=$(python3 - "$SOURCE_ROOT" <<'PY'
import os
import sys

supplied = os.path.abspath(sys.argv[1])
canonical = os.path.realpath(supplied)
if supplied != canonical:
    raise SystemExit("candidate source checkout must use its canonical path without symlink traversal")
print(canonical)
PY
) || exit 1
[[ $SCRIPT_PATH == "$SOURCE_ROOT/scripts/fetch-verify-release-candidate.sh" ]] || {
  echo "Candidate trust script is not executing from its canonical source-tree location." >&2
  exit 1
}
DESTINATION_PARENT=$(dirname "$DESTINATION")
DESTINATION_BASENAME=$(basename "$DESTINATION")
[[ $DESTINATION_BASENAME != . && $DESTINATION_BASENAME != .. && $DESTINATION_BASENAME != */* ]] || usage
DESTINATION_PARENT_IDENTITY=$(python3 - "$DESTINATION_PARENT" "$DESTINATION" <<'PY'
import os
import stat
import sys

parent, destination = sys.argv[1:]
if not os.path.isabs(parent) or not os.path.isabs(destination):
    raise SystemExit("candidate destination paths must be absolute")
canonical_parent = os.path.realpath(parent)
canonical_destination = os.path.join(canonical_parent, os.path.basename(destination))
if parent != canonical_parent or destination != canonical_destination:
    raise SystemExit("candidate destination and parent must use canonical paths without symlink traversal")
if os.path.lexists(destination):
    raise SystemExit("candidate destination must not already exist")
uid = os.geteuid()
current = canonical_parent
first = True
while True:
    info = os.lstat(current)
    if not stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode):
        raise SystemExit("candidate destination ancestry must contain ordinary directories only")
    mode = stat.S_IMODE(info.st_mode)
    if first and (info.st_uid != uid or mode != 0o700):
        raise SystemExit("candidate destination parent must be owned by the current user with mode 0700")
    if mode & 0o022 and not mode & stat.S_ISVTX:
        raise SystemExit("candidate destination ancestry contains an unprotected writable directory")
    parent_path = os.path.dirname(current)
    if parent_path == current:
        break
    current = parent_path
    first = False
info = os.stat(canonical_parent, follow_symlinks=False)
print(f"{info.st_dev}:{info.st_ino}")
PY
) || exit 1
mkdir -m 700 "$DESTINATION"
PAYLOAD_DIRECTORY="$DESTINATION/payload"
ATTESTATION_DIRECTORY="$DESTINATION/attestations"
mkdir -m 700 "$PAYLOAD_DIRECTORY" "$ATTESTATION_DIRECTORY"

assert_destination_identity() {
  local observed
  observed=$(python3 - "$DESTINATION_PARENT" "$DESTINATION" <<'PY'
import os
import stat
import sys

parent, destination = sys.argv[1:]
parent_info = os.stat(parent, follow_symlinks=False)
destination_info = os.stat(destination, follow_symlinks=False)
if not stat.S_ISDIR(parent_info.st_mode) or not stat.S_ISDIR(destination_info.st_mode):
    raise SystemExit("candidate trust directories changed type")
if parent_info.st_uid != os.geteuid() or stat.S_IMODE(parent_info.st_mode) != 0o700:
    raise SystemExit("candidate destination parent is no longer owner-private")
if destination_info.st_uid != os.geteuid() or stat.S_IMODE(destination_info.st_mode) != 0o700:
    raise SystemExit("candidate destination is not owner-private")
print(f"{parent_info.st_dev}:{parent_info.st_ino}")
PY
  ) || return 1
  [[ $observed == "$DESTINATION_PARENT_IDENTITY" ]]
}
assert_destination_identity || {
  echo "Candidate destination identity changed during initialization." >&2
  exit 1
}

fail() {
  echo "Candidate trust gate failed: $*" >&2
  exit 1
}

sha256_file() {
  shasum -a 256 "$1" | awk '{ print $1 }'
}

byte_count() {
  wc -c < "$1" | tr -d ' '
}

# The trust program itself must be an exact tracked blob in a clean detached
# checkout of the independently supplied source and annotated tag.
[[ $(git -C "$SOURCE_ROOT" rev-parse HEAD) == "$SOURCE_SHA" ]] || fail "source HEAD mismatch"
[[ $(git -C "$SOURCE_ROOT" rev-parse --abbrev-ref HEAD) == HEAD ]] || fail "source checkout is not detached"
[[ $(git -C "$SOURCE_ROOT" cat-file -t "$TAG_OBJECT_SHA") == tag ]] || fail "tag object is not annotated"
[[ $(git -C "$SOURCE_ROOT" rev-parse "$TAG") == "$TAG_OBJECT_SHA" ]] || fail "tag object mismatch"
[[ $(git -C "$SOURCE_ROOT" rev-parse "$TAG^{}") == "$SOURCE_SHA" ]] || fail "tag peel mismatch"
[[ -z $(git -C "$SOURCE_ROOT" status --porcelain=v2 --untracked-files=all) ]] || fail "source checkout is dirty"
git -C "$SOURCE_ROOT" diff --quiet HEAD -- || fail "source worktree diff is nonempty"
git -C "$SOURCE_ROOT" diff --cached --quiet || fail "source index diff is nonempty"
[[ -z $(git -C "$SOURCE_ROOT" ls-files --deleted) ]] || fail "source checkout has missing tracked files"
[[ -z $(git -C "$SOURCE_ROOT" ls-files --others --exclude-standard) ]] || fail "source checkout has untracked files"
git -C "$SOURCE_ROOT" fsck --full >/dev/null || fail "source object database failed fsck"
SCRIPT_RELATIVE=${SCRIPT_PATH#"$SOURCE_ROOT/"}
[[ $SCRIPT_RELATIVE != "$SCRIPT_PATH" ]] || fail "trust script is outside source checkout"
[[ $(git -C "$SOURCE_ROOT" rev-parse "HEAD:$SCRIPT_RELATIVE") == $(git -C "$SOURCE_ROOT" hash-object -- "$SCRIPT_PATH") ]] ||
  fail "trust script does not match the tagged source blob"

RUN_JSON="$DESTINATION/workflow-run.json"
JOBS_JSON="$DESTINATION/workflow-jobs.json"
ARTIFACTS_JSON="$DESTINATION/workflow-artifacts.json"
ARTIFACT_API_JSON="$DESTINATION/release-candidate-artifact-api.json"
REMOTE_REF_JSON="$DESTINATION/remote-tag-ref.json"
REMOTE_TAG_JSON="$DESTINATION/remote-tag-object.json"
gh api "repos/$REPOSITORY/actions/runs/$RUN_ID/attempts/$RUN_ATTEMPT" > "$RUN_JSON"
gh api "repos/$REPOSITORY/actions/runs/$RUN_ID/attempts/$RUN_ATTEMPT/jobs?per_page=100" > "$JOBS_JSON"
gh api "repos/$REPOSITORY/actions/runs/$RUN_ID/artifacts?per_page=100" > "$ARTIFACTS_JSON"
gh api "repos/$REPOSITORY/actions/artifacts/$ARTIFACT_ID" > "$ARTIFACT_API_JSON"
gh api "repos/$REPOSITORY/git/ref/tags/$TAG" > "$REMOTE_REF_JSON"
gh api "repos/$REPOSITORY/git/tags/$TAG_OBJECT_SHA" > "$REMOTE_TAG_JSON"

jq -e \
  --arg source "$SOURCE_SHA" \
  --arg tag "$TAG" \
  --argjson attempt "$RUN_ATTEMPT" '
    .event == "push" and .head_sha == $source and .head_branch == $tag and
    .run_attempt == $attempt and .path == ".github/workflows/deploy.yml"
  ' "$RUN_JSON" >/dev/null || fail "workflow run binding mismatch"
jq -e '
    (.total_count < 100) and (.jobs | length) == .total_count and
    [.jobs[] | select(.name == "Assemble frozen release candidate")] as $matched |
    ($matched | length) == 1 and $matched[0].conclusion == "success" and
    ($matched[0].started_at | type) == "string" and
    ($matched[0].completed_at | type) == "string"
  ' "$JOBS_JSON" >/dev/null || fail "workflow assemble-job binding mismatch"
jq -e --arg tag_object "$TAG_OBJECT_SHA" '
  .object.type == "tag" and .object.sha == $tag_object
  ' "$REMOTE_REF_JSON" >/dev/null || fail "remote annotated tag ref mismatch"
jq -e --arg source "$SOURCE_SHA" --arg tag "$TAG" '
  .tag == $tag and .object.type == "commit" and .object.sha == $source
  ' "$REMOTE_TAG_JSON" >/dev/null || fail "remote annotated tag object mismatch"

ARTIFACT_JSON="$DESTINATION/release-candidate-artifact.json"
jq -e \
  --argjson artifact_id "$ARTIFACT_ID" \
  --argjson run_id "$RUN_ID" \
  --arg source "$SOURCE_SHA" '
    (.total_count < 100) and
    [.artifacts[] | select(.id == $artifact_id)] as $matched |
    ($matched | length) == 1 and
    $matched[0].name == "release-candidate" and
    $matched[0].expired == false and
    ($matched[0].size_in_bytes > 0 and $matched[0].size_in_bytes <= 536870912) and
    ($matched[0].digest | test("^sha256:[0-9a-f]{64}$")) and
    $matched[0].workflow_run.id == $run_id and
    $matched[0].workflow_run.head_sha == $source and
    ([.artifacts[] | select(.name == "release-candidate" and .expired == false)] | length) == 1
  ' "$ARTIFACTS_JSON" >/dev/null || fail "release-candidate artifact binding mismatch"
jq --argjson artifact_id "$ARTIFACT_ID" \
  '.artifacts[] | select(.id == $artifact_id)' "$ARTIFACTS_JSON" > "$ARTIFACT_JSON"
jq -e \
  --argjson artifact_id "$ARTIFACT_ID" \
  --argjson run_id "$RUN_ID" \
  --arg source "$SOURCE_SHA" \
  --slurpfile listing "$ARTIFACT_JSON" \
  --slurpfile jobs "$JOBS_JSON" '
    .id == $artifact_id and .name == "release-candidate" and .expired == false and
    (.size_in_bytes > 0 and .size_in_bytes <= 536870912) and
    (.digest | test("^sha256:[0-9a-f]{64}$")) and
    .workflow_run.id == $run_id and .workflow_run.head_sha == $source and
    .id == $listing[0].id and .size_in_bytes == $listing[0].size_in_bytes and
    .digest == $listing[0].digest and .created_at == $listing[0].created_at and
    .created_at >= ($jobs[0].jobs[] | select(.name == "Assemble frozen release candidate") | .started_at) and
    .created_at <= ($jobs[0].jobs[] | select(.name == "Assemble frozen release candidate") | .completed_at)
  ' "$ARTIFACT_API_JSON" >/dev/null || fail "direct artifact metadata is not bound to the successful current-attempt assemble job"

ARTIFACT_ZIP="$DESTINATION/release-candidate-artifact-$ARTIFACT_ID.zip"
ARTIFACT_ZIP_TEMP="$ARTIFACT_ZIP.partial"
gh api -H "Accept: application/vnd.github+json" \
  "repos/$REPOSITORY/actions/artifacts/$ARTIFACT_ID/zip" > "$ARTIFACT_ZIP_TEMP"
chmod 600 "$ARTIFACT_ZIP_TEMP"
EXPECTED_ARTIFACT_BYTES=$(jq -r '.size_in_bytes' "$ARTIFACT_API_JSON")
EXPECTED_ARTIFACT_SHA256=$(jq -r '.digest | sub("^sha256:"; "")' "$ARTIFACT_API_JSON")
[[ $(byte_count "$ARTIFACT_ZIP_TEMP") == "$EXPECTED_ARTIFACT_BYTES" ]] || fail "raw artifact ZIP size mismatch"
ARTIFACT_ZIP_SHA256=$(sha256_file "$ARTIFACT_ZIP_TEMP")
[[ $ARTIFACT_ZIP_SHA256 == "$EXPECTED_ARTIFACT_SHA256" ]] || fail "raw artifact ZIP digest mismatch"
mv "$ARTIFACT_ZIP_TEMP" "$ARTIFACT_ZIP"

ASSETS=(
  "local-browser-bridge-v$VERSION-windows-x86_64.exe"
  "local-computer-helper-v$VERSION-windows-x86_64.exe"
  "local-browser-bridge-v$VERSION-macos-universal.tar.gz"
  "local-browser-bridge-extension-v$VERSION.zip"
)
RELEASE_FILES=("${ASSETS[@]}" "SHA256SUMS.txt")
EXPECTED_LISTING=$(printf '%s\n' "${RELEASE_FILES[@]}" | LC_ALL=C sort)
python3 - "$ARTIFACT_ZIP" "$PAYLOAD_DIRECTORY" "${RELEASE_FILES[@]}" <<'PY'
import os
import stat
import sys
import zipfile

archive_path, destination, *expected_names = sys.argv[1:]
maximum_entry_bytes = 256 * 1024 * 1024
maximum_total_bytes = 512 * 1024 * 1024
with zipfile.ZipFile(archive_path, "r") as archive:
    entries = archive.infolist()
    if len(entries) != len(expected_names) or sorted(item.filename for item in entries) != sorted(expected_names):
        raise SystemExit("outer artifact ZIP inventory changed before bounded extraction")
    if len({item.filename for item in entries}) != len(entries):
        raise SystemExit("outer artifact ZIP contains duplicate entries")
    total = 0
    for item in entries:
        if (
            item.filename not in expected_names
            or "/" in item.filename
            or "\\" in item.filename
            or item.is_dir()
            or item.flag_bits & 0x1
            or item.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
            or item.file_size < 1
            or item.file_size > maximum_entry_bytes
            or item.compress_size < 1
        ):
            raise SystemExit(f"unsafe outer artifact ZIP entry: {item.filename}")
        unix_mode = (item.external_attr >> 16) & 0xffff
        if unix_mode and not stat.S_ISREG(unix_mode):
            raise SystemExit(f"non-regular outer artifact ZIP entry: {item.filename}")
        total += item.file_size
        if total > maximum_total_bytes:
            raise SystemExit("outer artifact ZIP exceeds the bounded uncompressed candidate size")

    for item in entries:
        output_path = os.path.join(destination, item.filename)
        descriptor = os.open(
            output_path,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
            0o600,
        )
        written = 0
        try:
            with archive.open(item, "r") as source, os.fdopen(descriptor, "wb", closefd=False) as output:
                while written < item.file_size:
                    chunk = source.read(min(1024 * 1024, item.file_size - written))
                    if not chunk:
                        raise SystemExit(f"truncated outer artifact ZIP entry: {item.filename}")
                    output.write(chunk)
                    written += len(chunk)
                if source.read(1):
                    raise SystemExit(f"outer artifact ZIP entry expanded beyond its declared size: {item.filename}")
                output.flush()
                os.fsync(output.fileno())
        finally:
            os.close(descriptor)
        if written != item.file_size:
            raise SystemExit(f"outer artifact ZIP entry length mismatch: {item.filename}")
PY
ACTUAL_LISTING=$(find "$PAYLOAD_DIRECTORY" -mindepth 1 -maxdepth 1 -exec basename {} \; | LC_ALL=C sort)
[[ $ACTUAL_LISTING == "$EXPECTED_LISTING" ]] || fail "extracted candidate inventory mismatch"
for name in "${RELEASE_FILES[@]}"; do
  [[ -f "$PAYLOAD_DIRECTORY/$name" && ! -L "$PAYLOAD_DIRECTORY/$name" && -s "$PAYLOAD_DIRECTORY/$name" ]] ||
    fail "invalid candidate payload: $name"
done
[[ -z $(find "$PAYLOAD_DIRECTORY" -mindepth 1 -maxdepth 1 ! -type f -print) ]] || fail "non-regular candidate payload"

MANIFEST="$PAYLOAD_DIRECTORY/SHA256SUMS.txt"
[[ $(byte_count "$MANIFEST") -gt 0 ]] || fail "empty checksum manifest"
[[ $(wc -l < "$MANIFEST" | tr -d ' ') == 4 ]] || fail "checksum manifest line count mismatch"
[[ $(tail -c 1 "$MANIFEST" | od -An -t x1 | tr -d ' \n') == 0a ]] || fail "checksum manifest lacks terminal LF"
[[ -z $(LC_ALL=C tr -d '\11\12\15\40-\176' < "$MANIFEST") ]] || fail "checksum manifest is not ASCII"
[[ -z $(LC_ALL=C grep $'\r' "$MANIFEST" || true) ]] || fail "checksum manifest contains CR bytes"
for index in 0 1 2 3; do
  line=$(sed -n "$((index + 1))p" "$MANIFEST")
  expected_name=${ASSETS[$index]}
  [[ $line =~ ^[0-9a-f]{64}\ \  ]] || fail "noncanonical checksum row"
  [[ ${line:66} == "$expected_name" ]] || fail "checksum asset order mismatch"
done
(cd "$PAYLOAD_DIRECTORY" && shasum -a 256 -c SHA256SUMS.txt >/dev/null) || fail "candidate payload checksum mismatch"
MANIFEST_SHA256=$(sha256_file "$MANIFEST")

bash "$SOURCE_ROOT/scripts/verify-release-assets.sh" \
  "$VERSION" "$PAYLOAD_DIRECTORY" --static-only >/dev/null ||
  fail "release asset policy verification failed"

INVOCATION_URI="https://github.com/$REPOSITORY/actions/runs/$RUN_ID/attempts/$RUN_ATTEMPT"
for name in "${RELEASE_FILES[@]}"; do
  attestation_json="$ATTESTATION_DIRECTORY/$name.json"
  gh attestation verify "$PAYLOAD_DIRECTORY/$name" \
    --repo "$REPOSITORY" \
    --source-ref "refs/tags/$TAG" \
    --source-digest "$SOURCE_SHA" \
    --signer-workflow "$REPOSITORY/.github/workflows/deploy.yml" \
    --deny-self-hosted-runners \
    --format json > "$attestation_json"
  verify_exact_attempt_attestation_set \
    "$attestation_json" "$INVOCATION_URI" "$REPOSITORY" "$RUN_ID" \
    "$SOURCE_SHA" "refs/tags/$TAG" ".github/workflows/deploy.yml" \
    "$name" "$(sha256_file "$PAYLOAD_DIRECTORY/$name")" ||
    fail "exact-attempt attestation mismatch or ambiguity for $name"
done

ASSETS_JSON="$DESTINATION/assets.json"
printf '%s\n' "${RELEASE_FILES[@]}" | while IFS= read -r name; do
  jq -cn \
    --arg file "$name" \
    --argjson bytes "$(byte_count "$PAYLOAD_DIRECTORY/$name")" \
    --arg sha256 "$(sha256_file "$PAYLOAD_DIRECTORY/$name")" \
    '{file:$file,bytes:$bytes,sha256:$sha256}'
done | jq -s '.' > "$ASSETS_JSON"

BINDING="$DESTINATION/candidate-binding.json"
jq -cn \
  --arg version "$VERSION" \
  --arg repository "$REPOSITORY" \
  --arg tag "$TAG" \
  --arg source "$SOURCE_SHA" \
  --arg tag_object "$TAG_OBJECT_SHA" \
  --arg run_id "$RUN_ID" \
  --arg run_attempt "$RUN_ATTEMPT" \
  --arg artifact_id "$ARTIFACT_ID" \
  --argjson artifact_bytes "$EXPECTED_ARTIFACT_BYTES" \
  --arg artifact_sha256 "$ARTIFACT_ZIP_SHA256" \
  --arg manifest_sha256 "$MANIFEST_SHA256" \
  --arg invocation "$INVOCATION_URI" \
  --slurpfile assets "$ASSETS_JSON" \
  '{schemaVersion:1,productVersion:$version,repository:$repository,tag:$tag,sourceSha:$source,tagObjectSha:$tag_object,workflowRunId:$run_id,workflowRunAttempt:$run_attempt,artifactId:$artifact_id,artifactName:"release-candidate",artifactZipBytes:$artifact_bytes,artifactZipSha256:$artifact_sha256,checksumManifestSha256:$manifest_sha256,attestationInvocationUri:$invocation,attestedAssetCount:5,githubHostedRunner:true,assets:$assets[0],passed:true}' \
  > "$BINDING"

chmod 600 "$DESTINATION"/*.json "$ARTIFACT_ZIP" "$PAYLOAD_DIRECTORY"/* "$ATTESTATION_DIRECTORY"/*.json
assert_destination_identity || fail "candidate destination identity changed during verification"
printf 'Candidate trust gate passed.\n'
printf 'Binding: %s\n' "$BINDING"
printf 'Payload: %s\n' "$PAYLOAD_DIRECTORY"
