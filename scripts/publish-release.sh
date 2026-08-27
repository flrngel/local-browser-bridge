#!/usr/bin/env bash

set -euo pipefail
export LC_ALL=C
umask 077

readonly SCRIPT_NAME="$(basename "$0")"
readonly REPOSITORY="flrngel/local-browser-bridge"
readonly CANDIDATE_WORKFLOW=".github/workflows/deploy.yml"
readonly CANDIDATE_REF="refs/heads/main"

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

byte_count() {
  wc -c < "$1" | tr -d ' '
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

require_environment() {
  local name
  for name in "$@"; do
    test -n "${!name:-}" || die "required environment variable is empty: $name"
  done
}

assert_source_checkout() {
  local source_root source_script remote_url
  source_root="$(git rev-parse --show-toplevel)"
  source_root="$(cd "$source_root" && pwd -P)"
  source_script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/$(basename "${BASH_SOURCE[0]}")"
  test "$source_script" = "$source_root/scripts/$SCRIPT_NAME" \
    || die "publication script is outside its canonical source checkout"
  test "$(git rev-parse HEAD)" = "$VERIFIED_SOURCE_SHA" || die "source HEAD mismatch"
  test "$(git rev-parse --abbrev-ref HEAD)" = HEAD || die "source checkout is not detached"
  test -z "$(git status --porcelain=v2 --untracked-files=all)" || die "source checkout is dirty"
  git diff --quiet HEAD -- || die "source worktree diff is nonempty"
  git diff --cached --quiet || die "source index diff is nonempty"
  git fsck --full >/dev/null || die "source object database failed fsck"
  test "$(git rev-parse "HEAD:scripts/$SCRIPT_NAME")" = "$(git hash-object -- "$source_script")" \
    || die "publication script does not match its exact source blob"
  remote_url="$(git remote get-url origin)"
  test "$remote_url" = "https://github.com/$REPOSITORY.git" \
    || test "$remote_url" = "https://github.com/$REPOSITORY" \
    || die "origin does not name the canonical repository"
  git merge-base --is-ancestor "$VERIFIED_SOURCE_SHA" origin/main \
    || die "verified source is not contained in origin/main"
}

release_files() {
  printf '%s\n' \
    "local-browser-bridge-v${RELEASE_VERSION}-windows-x86_64.exe" \
    "local-computer-helper-v${RELEASE_VERSION}-windows-x86_64.exe" \
    "local-browser-bridge-v${RELEASE_VERSION}-macos-universal.tar.gz" \
    "local-browser-bridge-extension-v${RELEASE_VERSION}.zip" \
    "SHA256SUMS.txt"
}

assert_flat_inventory() {
  local directory="$1"
  shift
  local expected actual
  expected="$(printf '%s\n' "$@" | sort)"
  actual="$(find "$directory" -mindepth 1 -maxdepth 1 -exec basename {} \; | sort)"
  test "$actual" = "$expected" || die "directory has an unexpected inventory: $directory"
  test -z "$(find "$directory" -mindepth 1 -maxdepth 1 ! -type f -print)" \
    || die "directory contains a non-regular entry: $directory"
  local name
  for name in "$@"; do
    test -f "$directory/$name" && test ! -L "$directory/$name" && test -s "$directory/$name" \
      || die "required ordinary file is absent or empty: $name"
  done
}

write_release_notes() {
  local destination="$1"
  local manifest_sha256="$2"
  local receipt_sha256="$3"
  cat > "$destination" <<EOF
## Install

- [Windows installation guide](https://github.com/$REPOSITORY/blob/$RELEASE_TAG/docs/INSTALL_WINDOWS.md)
- [macOS installation guide](https://github.com/$REPOSITORY/blob/$RELEASE_TAG/docs/INSTALL_MACOS.md)
- [Safe one-command uninstall](https://github.com/$REPOSITORY/blob/$RELEASE_TAG/README.md#uninstall-in-one-command)
- [Build from source](https://github.com/$REPOSITORY/blob/$RELEASE_TAG/docs/BUILD.md)

Download the server and computer helper for your platform plus the Chromium extension ZIP. Verify every download with \`SHA256SUMS.txt\` before running it.

## Assets

- \`local-browser-bridge-v${RELEASE_VERSION}-windows-x86_64.exe\`
- \`local-computer-helper-v${RELEASE_VERSION}-windows-x86_64.exe\`
- \`local-browser-bridge-v${RELEASE_VERSION}-macos-universal.tar.gz\`
- \`local-browser-bridge-extension-v${RELEASE_VERSION}.zip\`
- \`SHA256SUMS.txt\`

All five files were built by the exact [release-candidate workflow run](https://github.com/$REPOSITORY/actions/runs/$CANDIDATE_RUN_ID/attempts/$CANDIDATE_RUN_ATTEMPT), verified against the accepted source commit, and carry GitHub build-provenance attestations.

## Trust record

- Accepted source: [\`$VERIFIED_SOURCE_SHA\`](https://github.com/$REPOSITORY/commit/$VERIFIED_SOURCE_SHA)
- Candidate workflow: run \`$CANDIDATE_RUN_ID\`, attempt \`$CANDIDATE_RUN_ATTEMPT\`
- \`SHA256SUMS.txt\` SHA-256: \`$manifest_sha256\`
- Canonical acceptance receipt SHA-256: \`$receipt_sha256\`
EOF
}

validate_candidate_binding() {
  local binding="$1"
  jq -e \
    --arg repository "$REPOSITORY" \
    --arg version "$RELEASE_VERSION" \
    --arg tag "$RELEASE_TAG" \
    --arg source "$VERIFIED_SOURCE_SHA" \
    --arg run_id "$CANDIDATE_RUN_ID" \
    --arg run_attempt "$CANDIDATE_RUN_ATTEMPT" '
      type == "object" and
      .schemaVersion == 3 and .passed == true and
      .repository == $repository and .version == $version and .releaseTag == $tag and
      .sourceSha == $source and .workflowRunId == $run_id and
      .workflowRunAttempt == $run_attempt and .workflowEvent == "workflow_dispatch" and
      .workflowRef == "refs/heads/main" and
      .workflowPath == ".github/workflows/deploy.yml" and
      (.artifactId | type == "string" and test("^[1-9][0-9]*$")) and
      .artifactName == "release-candidate" and
      (.artifactZipBytes | type == "number" and . > 0 and . <= 536870912) and
      (.artifactZipSha256 | type == "string" and test("^[0-9a-f]{64}$")) and
      (.checksumManifestSha256 | type == "string" and test("^[0-9a-f]{64}$")) and
      .attestationInvocationUri == ("https://github.com/" + $repository + "/actions/runs/" + $run_id + "/attempts/" + $run_attempt) and
      .attestedAssetCount == 5 and .githubHostedRunner == true and
      (.assets | type == "array" and length == 5) and
      all(.assets[];
        (keys_unsorted == ["file", "bytes", "sha256"]) and
        (.file | type == "string") and (.bytes | type == "number" and . > 0) and
        (.sha256 | type == "string" and test("^[0-9a-f]{64}$"))) and
      ([.assets[].file] | unique | length == 5)
    ' "$binding" >/dev/null || die "candidate binding is malformed or does not match this publication"
}

write_asset_records() {
  local directory="$1"
  local name
  while IFS= read -r name; do
    jq -cn \
      --arg file "$name" \
      --argjson bytes "$(byte_count "$directory/$name")" \
      --arg sha256 "$(sha256_file "$directory/$name")" \
      '{file:$file,bytes:$bytes,sha256:$sha256}'
  done < <(release_files)
}

validate_approval() {
  local directory="$1"
  local approval="$directory/publication-approval.json"
  local expected_receipt_sha="$2"
  jq -e \
    --arg repository "$REPOSITORY" \
    --arg version "$RELEASE_VERSION" \
    --arg tag "$RELEASE_TAG" \
    --arg source "$VERIFIED_SOURCE_SHA" \
    --arg run_id "$CANDIDATE_RUN_ID" \
    --arg run_attempt "$CANDIDATE_RUN_ATTEMPT" \
    --arg receipt_sha "$expected_receipt_sha" '
      type == "object" and
      (keys_unsorted == [
        "schemaVersion", "repository", "version", "releaseTag", "sourceSha",
        "candidateRunId", "candidateRunAttempt", "candidateArtifactId",
        "candidateArtifactZipSha256", "candidateBindingSha256",
        "acceptanceReceiptSha256", "checksumManifestSha256", "assets", "passed"
      ]) and
      .schemaVersion == 1 and .repository == $repository and .version == $version and
      .releaseTag == $tag and .sourceSha == $source and .candidateRunId == $run_id and
      .candidateRunAttempt == $run_attempt and
      (.candidateArtifactId | type == "string" and test("^[1-9][0-9]*$")) and
      (.candidateArtifactZipSha256 | type == "string" and test("^[0-9a-f]{64}$")) and
      (.candidateBindingSha256 | type == "string" and test("^[0-9a-f]{64}$")) and
      .acceptanceReceiptSha256 == $receipt_sha and
      (.checksumManifestSha256 | type == "string" and test("^[0-9a-f]{64}$")) and
      (.assets | type == "array" and length == 5) and
      all(.assets[];
        (keys_unsorted == ["file", "bytes", "sha256"]) and
        (.file | type == "string") and (.bytes | type == "number" and . > 0) and
        (.sha256 | type == "string" and test("^[0-9a-f]{64}$"))) and
      ([.assets[].file] | unique | length == 5) and .passed == true
    ' "$approval" >/dev/null || die "publication approval is malformed or incorrectly bound"

  test "$(sha256_file "$directory/acceptance-receipt.json")" = "$expected_receipt_sha" \
    || die "acceptance receipt digest changed after preflight"
  test "$(sha256_file "$directory/candidate-binding.json")" = "$(jq -r '.candidateBindingSha256' "$approval")" \
    || die "candidate binding digest changed after preflight"
  test "$(sha256_file "$directory/SHA256SUMS.txt")" = "$(jq -r '.checksumManifestSha256' "$approval")" \
    || die "checksum manifest digest changed after preflight"
  validate_candidate_binding "$directory/candidate-binding.json"

  local name expected_bytes expected_sha
  while IFS= read -r name; do
    expected_bytes="$(jq -er --arg name "$name" '.assets[] | select(.file == $name) | .bytes' "$approval")"
    expected_sha="$(jq -er --arg name "$name" '.assets[] | select(.file == $name) | .sha256' "$approval")"
    test "$(byte_count "$directory/$name")" = "$expected_bytes" || die "approved asset size changed: $name"
    test "$(sha256_file "$directory/$name")" = "$expected_sha" || die "approved asset digest changed: $name"
  done < <(release_files)

  test "$(jq -r '.candidateArtifactId' "$approval")" = "$(jq -r '.artifactId' "$directory/candidate-binding.json")" \
    || die "approval and candidate binding artifact IDs differ"
  test "$(jq -r '.candidateArtifactZipSha256' "$approval")" = "$(jq -r '.artifactZipSha256' "$directory/candidate-binding.json")" \
    || die "approval and candidate binding artifact digests differ"
  test "$(jq -r '.checksumManifestSha256' "$approval")" = "$(jq -r '.checksumManifestSha256' "$directory/candidate-binding.json")" \
    || die "approval and candidate binding manifest digests differ"
  jq -e --slurpfile approval "$approval" '.assets == $approval[0].assets' \
    "$directory/candidate-binding.json" >/dev/null \
    || die "approval and candidate binding asset inventories differ"
}

verify_release_assets_in_bundle() {
  local bundle="$1"
  local scratch name
  scratch="$(mktemp -d)"
  while IFS= read -r name; do
    cp -- "$bundle/$name" "$scratch/$name"
  done < <(release_files)
  bash scripts/verify-release-assets.sh "$RELEASE_VERSION" "$scratch" --static-only >/dev/null
  rm -rf -- "$scratch"
}

prepare_approval() {
  test "$#" = 3 || die "usage: $SCRIPT_NAME prepare RECEIPT CANDIDATE_ROOT APPROVED_DIRECTORY"
  local receipt="$1"
  local candidate_root="$2"
  local approved="$3"
  local payload="$candidate_root/payload"
  local binding="$candidate_root/candidate-binding.json"
  local receipt_sha assets_json name

  test ! -e "$approved" && test ! -L "$approved" || die "approved directory already exists"
  test -f "$receipt" && test ! -L "$receipt" || die "acceptance receipt is not one ordinary file"
  test -d "$payload" && test ! -L "$payload" || die "candidate payload directory is invalid"
  test -f "$binding" && test ! -L "$binding" || die "candidate binding is invalid"
  mapfile -t files < <(release_files)
  assert_flat_inventory "$payload" "${files[@]}"
  validate_candidate_binding "$binding"
  bash scripts/verify-release-assets.sh "$RELEASE_VERSION" "$payload" --static-only >/dev/null

  mkdir -m 700 "$approved"
  for name in "${files[@]}"; do
    cp -- "$payload/$name" "$approved/$name"
    chmod 600 "$approved/$name"
  done
  cp -- "$receipt" "$approved/acceptance-receipt.json"
  cp -- "$binding" "$approved/candidate-binding.json"
  chmod 600 "$approved/acceptance-receipt.json" "$approved/candidate-binding.json"
  receipt_sha="$(sha256_file "$approved/acceptance-receipt.json")"
  assets_json="$(mktemp)"
  write_asset_records "$approved" | jq -s '.' > "$assets_json"
  jq -cn \
    --arg repository "$REPOSITORY" \
    --arg version "$RELEASE_VERSION" \
    --arg tag "$RELEASE_TAG" \
    --arg source "$VERIFIED_SOURCE_SHA" \
    --arg run_id "$CANDIDATE_RUN_ID" \
    --arg run_attempt "$CANDIDATE_RUN_ATTEMPT" \
    --arg artifact_id "$(jq -r '.artifactId' "$binding")" \
    --arg artifact_sha "$(jq -r '.artifactZipSha256' "$binding")" \
    --arg binding_sha "$(sha256_file "$binding")" \
    --arg receipt_sha "$receipt_sha" \
    --arg manifest_sha "$(sha256_file "$approved/SHA256SUMS.txt")" \
    --slurpfile assets "$assets_json" \
    '{schemaVersion:1,repository:$repository,version:$version,releaseTag:$tag,sourceSha:$source,candidateRunId:$run_id,candidateRunAttempt:$run_attempt,candidateArtifactId:$artifact_id,candidateArtifactZipSha256:$artifact_sha,candidateBindingSha256:$binding_sha,acceptanceReceiptSha256:$receipt_sha,checksumManifestSha256:$manifest_sha,assets:$assets[0],passed:true}' \
    > "$approved/publication-approval.json"
  chmod 600 "$approved/publication-approval.json"
  rm -f -- "$assets_json"

  mapfile -t approved_files < <(release_files)
  approved_files+=(acceptance-receipt.json candidate-binding.json publication-approval.json)
  assert_flat_inventory "$approved" "${approved_files[@]}"
  validate_approval "$approved" "$receipt_sha"
  if test -n "${GITHUB_OUTPUT:-}"; then
    printf 'acceptance_receipt_sha256=%s\n' "$receipt_sha" >> "$GITHUB_OUTPUT"
    printf 'candidate_artifact_id=%s\n' "$(jq -r '.candidateArtifactId' "$approved/publication-approval.json")" >> "$GITHUB_OUTPUT"
  fi
  printf 'Publication approval prepared: %s\n' "$approved"
}

# GitHub can return older valid attestations for byte-identical reruns. Every
# result must still name this source/workflow/subject, and exactly one must name
# the accepted candidate attempt.
verify_attestation() {
  local asset="$1"
  local output="$2"
  local name sha invocation prefix
  name="$(basename "$asset")"
  sha="$(sha256_file "$asset")"
  invocation="https://github.com/$REPOSITORY/actions/runs/$CANDIDATE_RUN_ID/attempts/$CANDIDATE_RUN_ATTEMPT"
  prefix="https://github.com/$REPOSITORY/actions/runs/$CANDIDATE_RUN_ID/attempts/"
  gh attestation verify "$asset" \
    --repo "$REPOSITORY" \
    --source-ref "$CANDIDATE_REF" \
    --source-digest "$VERIFIED_SOURCE_SHA" \
    --signer-workflow "$REPOSITORY/$CANDIDATE_WORKFLOW" \
    --deny-self-hosted-runners \
    --format json > "$output"
  jq -e \
    --arg repository "$REPOSITORY" --arg source "$VERIFIED_SOURCE_SHA" \
    --arg workflow "$CANDIDATE_WORKFLOW" --arg ref "$CANDIDATE_REF" \
    --arg invocation "$invocation" --arg prefix "$prefix" \
    --arg name "$name" --arg sha "$sha" '
      def valid:
        (.verificationResult.statement.predicateType == "https://slsa.dev/provenance/v1") and
        (.verificationResult.statement.predicate.buildDefinition.buildType == "https://actions.github.io/buildtypes/workflow/v1") and
        (.verificationResult.statement.predicate.buildDefinition.externalParameters.workflow.path == $workflow) and
        (.verificationResult.statement.predicate.buildDefinition.externalParameters.workflow.ref == $ref) and
        (.verificationResult.statement.predicate.buildDefinition.externalParameters.workflow.repository == ("https://github.com/" + $repository)) and
        (.verificationResult.statement.predicate.runDetails.metadata.invocationId | type == "string") and
        (.verificationResult.statement.predicate.runDetails.metadata.invocationId | startswith($prefix)) and
        (.verificationResult.statement.predicate.runDetails.metadata.invocationId | ltrimstr($prefix) | test("^[1-9][0-9]*$")) and
        (.verificationResult.signature.certificate.runInvocationURI == .verificationResult.statement.predicate.runDetails.metadata.invocationId) and
        (.verificationResult.signature.certificate.githubWorkflowSHA == $source) and
        (.verificationResult.signature.certificate.githubWorkflowRepository == $repository) and
        (.verificationResult.signature.certificate.githubWorkflowRef == $ref) and
        (.verificationResult.signature.certificate.runnerEnvironment == "github-hosted") and
        (.verificationResult.signature.certificate.sourceRepositoryDigest == $source) and
        (.verificationResult.signature.certificate.sourceRepositoryRef == $ref) and
        (.verificationResult.statement.subject | type == "array") and
        ([.verificationResult.statement.subject[] | select(.name == $name and .digest.sha256 == $sha)] | length == 1);
      type == "array" and length >= 1 and all(.[]; valid) and
      ([.[] | select(.verificationResult.statement.predicate.runDetails.metadata.invocationId == $invocation)] | length == 1)
    ' "$output" >/dev/null || die "asset attestation is malformed, ambiguous, or not candidate-bound: $name"
}

assert_repository_release_policy() {
  local enabled pages matches ruleset_id ruleset
  enabled="$(gh api \
    -H 'Accept: application/vnd.github+json' \
    -H 'X-GitHub-Api-Version: 2026-03-10' \
    "repos/$REPOSITORY/immutable-releases" \
    --jq '.enabled')"
  test "$enabled" = true || die "repository immutable releases are not enabled"

  pages="$(mktemp)"
  matches="$(mktemp)"
  ruleset="$(mktemp)"
  gh api --paginate \
    -H 'Accept: application/vnd.github+json' \
    -H 'X-GitHub-Api-Version: 2026-03-10' \
    "repos/$REPOSITORY/rulesets?per_page=100" > "$pages"
  jq -s '[.[][] | select(.target == "tag" and .enforcement == "active")]' \
    "$pages" > "$matches"
  test "$(jq 'length' "$matches")" -ge 1 \
    || die "no active tag ruleset protects release tags"

  local protected_count=0 candidate_id
  while IFS= read -r candidate_id; do
    is_positive_integer "$candidate_id" || die "tag ruleset ID is invalid"
    gh api \
      -H 'Accept: application/vnd.github+json' \
      -H 'X-GitHub-Api-Version: 2026-03-10' \
      "repos/$REPOSITORY/rulesets/$candidate_id" > "$ruleset"
    if jq -e '
        .target == "tag" and .enforcement == "active" and
        .conditions.ref_name.include == ["refs/tags/v*"] and
        .conditions.ref_name.exclude == [] and
        ([.rules[].type] | index("update") != null and index("deletion") != null) and
        (.bypass_actors | type == "array" and length == 0) and
        .current_user_can_bypass == "never"
      ' "$ruleset" >/dev/null; then
      protected_count=$((protected_count + 1))
      ruleset_id="$candidate_id"
    fi
  done < <(jq -r '.[].id' "$matches")
  rm -f -- "$pages" "$matches" "$ruleset"
  test "$protected_count" = 1 \
    || die "release tags are not protected by one unbypassable update/deletion ruleset"
  is_positive_integer "$ruleset_id" || die "release tag ruleset binding is invalid"
}

assert_tag() {
  local ref_json tag_json
  ref_json="$(mktemp)"
  tag_json="$(mktemp)"
  fetch_tag_ref "$ref_json" || die "annotated release tag is absent"
  jq -e --arg ref "refs/tags/$RELEASE_TAG" '.ref == $ref and .object.type == "tag" and (.object.sha | test("^[0-9a-f]{40}$"))' \
    "$ref_json" >/dev/null || die "release ref is not one annotated tag"
  gh api "repos/$REPOSITORY/git/tags/$(jq -r '.object.sha' "$ref_json")" > "$tag_json"
  jq -e \
    --arg tag "$RELEASE_TAG" \
    --arg message "Local Browser Bridge $RELEASE_VERSION" \
    --arg source "$VERIFIED_SOURCE_SHA" '
      .tag == $tag and .message == $message and .object.type == "commit" and .object.sha == $source
    ' "$tag_json" >/dev/null || die "annotated release tag differs from the accepted source or message"
  rm -f -- "$ref_json" "$tag_json"
}

fetch_tag_ref() {
  local output="$1"
  local error_output
  error_output="$(mktemp)"
  if gh api "repos/$REPOSITORY/git/ref/tags/$RELEASE_TAG" > "$output" 2> "$error_output"; then
    rm -f -- "$error_output"
    return 0
  fi
  if grep -Eq 'HTTP 404|Not Found.*404' "$error_output"; then
    rm -f -- "$error_output"
    return 1
  fi
  cat "$error_output" >&2
  rm -f -- "$error_output"
  die "unable to determine whether the release tag exists"
}

tag_ref_exists() {
  local ref_json
  ref_json="$(mktemp)"
  if fetch_tag_ref "$ref_json"; then
    rm -f -- "$ref_json"
    return 0
  fi
  rm -f -- "$ref_json"
  return 1
}

create_or_verify_tag() {
  if tag_ref_exists; then
    assert_tag
    return 0
  fi
  local request tag_object ref_request
  request="$(mktemp)"
  tag_object="$(mktemp)"
  ref_request="$(mktemp)"
  jq -n \
    --arg tag "$RELEASE_TAG" \
    --arg message "Local Browser Bridge $RELEASE_VERSION" \
    --arg object "$VERIFIED_SOURCE_SHA" \
    '{tag:$tag,message:$message,object:$object,type:"commit"}' > "$request"
  gh api --method POST "repos/$REPOSITORY/git/tags" --input "$request" > "$tag_object"
  jq -e --arg tag "$RELEASE_TAG" --arg source "$VERIFIED_SOURCE_SHA" \
    '.tag == $tag and .object.type == "commit" and .object.sha == $source and (.sha | test("^[0-9a-f]{40}$"))' \
    "$tag_object" >/dev/null || die "GitHub returned an invalid annotated tag object"
  jq -n --arg ref "refs/tags/$RELEASE_TAG" --arg sha "$(jq -r '.sha' "$tag_object")" \
    '{ref:$ref,sha:$sha}' > "$ref_request"
  if ! gh api --method POST "repos/$REPOSITORY/git/refs" --input "$ref_request" >/dev/null; then
    assert_tag
  fi
  rm -f -- "$request" "$tag_object" "$ref_request"
  assert_tag
}

find_release() {
  local output="$1"
  local pages matches
  pages="$(mktemp)"
  matches="$(mktemp)"
  gh api --paginate "repos/$REPOSITORY/releases?per_page=100" > "$pages"
  jq -s --arg tag "$RELEASE_TAG" '[.[][] | select(.tag_name == $tag)]' "$pages" > "$matches"
  test "$(jq 'length' "$matches")" -le 1 || die "more than one GitHub Release uses the intended tag"
  if test "$(jq 'length' "$matches")" = 1; then
    jq '.[0]' "$matches" > "$output"
    rm -f -- "$pages" "$matches"
    return 0
  fi
  rm -f -- "$pages" "$matches"
  return 1
}

assert_release_identity() {
  local release_json="$1"
  local notes="$2"
  jq -e \
    --arg tag "$RELEASE_TAG" \
    --arg name "Local Browser Bridge $RELEASE_VERSION" \
    --rawfile body "$notes" '
      .tag_name == $tag and .name == $name and .body == $body and
      .prerelease == false and (.id | type == "number" and . > 0)
    ' "$release_json" >/dev/null || die "existing release metadata is not byte-exact"
}

assert_release_assets() {
  local release_json="$1"
  local directory="$2"
  local allow_missing="$3"
  local expected_names name count size digest
  expected_names="$(release_files | jq -Rsc 'split("\n")[:-1]')"
  jq -e --argjson expected "$expected_names" '
    (.assets | type == "array") and
    ([.assets[].name] | unique | length == (.assets | length)) and
    all(.assets[]; (.name as $name | $expected | index($name)) != null)
  ' "$release_json" >/dev/null || die "draft contains an unexpected or duplicate release asset"
  if test "$allow_missing" = false; then
    jq -e --argjson expected "$expected_names" '([.assets[].name] | sort) == ($expected | sort)' \
      "$release_json" >/dev/null || die "release does not contain the exact five assets"
  fi
  while IFS= read -r name; do
    count="$(jq --arg name "$name" '[.assets[] | select(.name == $name)] | length' "$release_json")"
    test "$count" = 0 && continue
    test "$count" = 1 || die "release asset is duplicated: $name"
    size="$(byte_count "$directory/$name")"
    digest="sha256:$(sha256_file "$directory/$name")"
    jq -e --arg name "$name" --argjson size "$size" --arg digest "$digest" '
      [.assets[] | select(.name == $name and .state == "uploaded" and .size == $size and .digest == $digest)] | length == 1
    ' "$release_json" >/dev/null || die "existing release asset is not byte-exact: $name"
  done < <(release_files)
}

create_or_recover_release() {
  local approved="$1"
  local notes="$2"
  local release_json request name
  release_json="$(mktemp)"
  if ! find_release "$release_json"; then
    request="$(mktemp)"
    jq -n \
      --arg tag "$RELEASE_TAG" \
      --arg source "$VERIFIED_SOURCE_SHA" \
      --arg name "Local Browser Bridge $RELEASE_VERSION" \
      --rawfile body "$notes" \
      '{tag_name:$tag,target_commitish:$source,name:$name,body:$body,draft:true,prerelease:false,make_latest:"legacy"}' \
      > "$request"
    if ! gh api --method POST "repos/$REPOSITORY/releases" --input "$request" > "$release_json"; then
      find_release "$release_json" || die "release creation failed without an exact recoverable draft"
    fi
    rm -f -- "$request"
  fi
  assert_release_identity "$release_json" "$notes"
  assert_release_assets "$release_json" "$approved" true

  if jq -e '.draft == false' "$release_json" >/dev/null; then
    jq -e '.immutable == true' "$release_json" >/dev/null \
      || die "an existing published release is not immutable"
    assert_release_assets "$release_json" "$approved" false
    rm -f -- "$release_json"
    return 0
  fi
  jq -e '.draft == true and .immutable == false' "$release_json" >/dev/null \
    || die "existing release is neither a recoverable draft nor an exact immutable release"
  while IFS= read -r name; do
    if jq -e --arg name "$name" 'any(.assets[]; .name == $name)' "$release_json" >/dev/null; then
      continue
    fi
    gh release upload "$RELEASE_TAG" "$approved/$name" --repo "$REPOSITORY"
    find_release "$release_json" || die "draft disappeared after an asset upload"
    assert_release_identity "$release_json" "$notes"
    assert_release_assets "$release_json" "$approved" true
  done < <(release_files)
  assert_release_assets "$release_json" "$approved" false

  request="$(mktemp)"
  printf '%s\n' '{"draft":false,"make_latest":"true"}' > "$request"
  assert_repository_release_policy
  gh api --method PATCH "repos/$REPOSITORY/releases/$(jq -r '.id' "$release_json")" \
    --input "$request" > "$release_json"
  rm -f -- "$request"
  local poll
  for poll in 1 2 3 4 5 6 7 8 9 10; do
    find_release "$release_json" || die "published release disappeared"
    if jq -e '.draft == false and .immutable == true' "$release_json" >/dev/null; then
      break
    fi
    sleep 2
  done
  jq -e '.draft == false and .prerelease == false and .immutable == true' "$release_json" >/dev/null \
    || die "published release did not become immutable"
  assert_release_identity "$release_json" "$notes"
  assert_release_assets "$release_json" "$approved" false
  rm -f -- "$release_json"
}

verify_published_release() {
  local approved="$1"
  local notes="$2"
  local scratch release_json name attestation tag_object_sha release_attestation attempt verified
  scratch="$(mktemp -d)"
  release_json="$scratch/release.json"
  find_release "$release_json" || die "published release is absent"
  jq -e '.draft == false and .prerelease == false and .immutable == true' "$release_json" >/dev/null \
    || die "release is not published and immutable"
  assert_release_identity "$release_json" "$notes"
  assert_release_assets "$release_json" "$approved" false
  assert_tag
  tag_object_sha="$(gh api "repos/$REPOSITORY/git/ref/tags/$RELEASE_TAG" --jq '.object.sha')"
  is_sha1 "$tag_object_sha" || die "annotated tag object SHA is invalid"
  mkdir -m 700 "$scratch/downloads" "$scratch/attestations"
  while IFS= read -r name; do
    gh release download "$RELEASE_TAG" --repo "$REPOSITORY" --pattern "$name" --dir "$scratch/downloads"
    test -f "$scratch/downloads/$name" && test ! -L "$scratch/downloads/$name" \
      || die "release redownload did not produce one ordinary asset: $name"
    cmp -s "$approved/$name" "$scratch/downloads/$name" || die "redownloaded release asset differs: $name"
    verified=false
    for attempt in 1 2 3 4 5 6; do
      if gh release verify-asset "$RELEASE_TAG" "$scratch/downloads/$name" \
          --repo "$REPOSITORY" --format json > "$scratch/attestations/release-$name.json" \
          && jq -e 'type == "object"' "$scratch/attestations/release-$name.json" >/dev/null; then
        verified=true
        break
      fi
      sleep 5
    done
    test "$verified" = true || die "GitHub immutable-release attestation rejected asset: $name"
    attestation="$scratch/attestations/$name.json"
    verify_attestation "$scratch/downloads/$name" "$attestation"
  done < <(release_files)
  mapfile -t files < <(release_files)
  assert_flat_inventory "$scratch/downloads" "${files[@]}"
  bash scripts/verify-release-assets.sh "$RELEASE_VERSION" "$scratch/downloads" --static-only >/dev/null
  release_attestation="$scratch/attestations/release.json"
  verified=false
  for attempt in 1 2 3 4 5 6; do
    if gh release verify "$RELEASE_TAG" --repo "$REPOSITORY" --format json > "$release_attestation" \
        && jq -e \
          --arg uri "pkg:github/$REPOSITORY@$RELEASE_TAG" \
          --arg repository "$REPOSITORY" \
          --arg tag "$RELEASE_TAG" \
          --arg tag_object_sha "$tag_object_sha" '
            type == "object" and
            .verificationResult.statement.predicate.repository == $repository and
            .verificationResult.statement.predicate.tag == $tag and
            (.verificationResult.statement.subject | type == "array") and
            ([.verificationResult.statement.subject[] |
              select(.uri == $uri and .digest.sha1 == $tag_object_sha)] | length == 1) and
            ([.verificationResult.statement.subject[] | select(has("uri"))] | length == 1)
          ' "$release_attestation" >/dev/null; then
      verified=true
      break
    fi
    sleep 5
  done
  test "$verified" = true \
    || die "GitHub immutable-release attestation does not bind the unique package subject to the annotated tag object"
  rm -rf -- "$scratch"
  printf 'Immutable GitHub Release verified: https://github.com/%s/releases/tag/%s\n' "$REPOSITORY" "$RELEASE_TAG"
}

check_remote_state() {
  test "$#" = 2 || die "usage: $SCRIPT_NAME check-remote APPROVED_DIRECTORY EXPECTED_RECEIPT_SHA256"
  local approved="$1"
  local expected_receipt_sha="$2"
  local notes release_json tag_exists=false
  is_sha256 "$expected_receipt_sha" || die "expected acceptance receipt digest is invalid"
  mapfile -t files < <(release_files)
  files+=(acceptance-receipt.json candidate-binding.json publication-approval.json)
  assert_flat_inventory "$approved" "${files[@]}"
  validate_approval "$approved" "$expected_receipt_sha"
  verify_release_assets_in_bundle "$approved"
  assert_repository_release_policy
  if tag_ref_exists; then
    assert_tag
    tag_exists=true
  fi
  notes="$(mktemp)"
  release_json="$(mktemp)"
  write_release_notes "$notes" \
    "$(jq -r '.checksumManifestSha256' "$approved/publication-approval.json")" \
    "$expected_receipt_sha"
  if find_release "$release_json"; then
    test "$tag_exists" = true || die "a release exists without the exact annotated tag"
    assert_release_identity "$release_json" "$notes"
    assert_release_assets "$release_json" "$approved" true
    jq -e '(.draft == true and .immutable == false) or (.draft == false and .immutable == true)' \
      "$release_json" >/dev/null || die "release is not an exact recoverable draft or immutable publication"
    if jq -e '.draft == false' "$release_json" >/dev/null; then
      assert_release_assets "$release_json" "$approved" false
    fi
  fi
  rm -f -- "$notes" "$release_json"
  printf '%s\n' "Remote publication state is empty or exactly recoverable."
}

publish_approved() {
  test "$#" = 2 || die "usage: $SCRIPT_NAME publish APPROVED_DIRECTORY EXPECTED_RECEIPT_SHA256"
  local approved="$1"
  local expected_receipt_sha="$2"
  local notes name attestation
  is_sha256 "$expected_receipt_sha" || die "expected acceptance receipt digest is invalid"
  mapfile -t files < <(release_files)
  files+=(acceptance-receipt.json candidate-binding.json publication-approval.json)
  assert_flat_inventory "$approved" "${files[@]}"
  validate_approval "$approved" "$expected_receipt_sha"
  verify_release_assets_in_bundle "$approved"
  assert_repository_release_policy
  local attestation_root
  attestation_root="$(mktemp -d)"
  while IFS= read -r name; do
    attestation="$attestation_root/$name.json"
    verify_attestation "$approved/$name" "$attestation"
  done < <(release_files)
  rm -rf -- "$attestation_root"

  assert_repository_release_policy
  create_or_verify_tag
  notes="$(mktemp)"
  write_release_notes "$notes" \
    "$(jq -r '.checksumManifestSha256' "$approved/publication-approval.json")" \
    "$expected_receipt_sha"
  create_or_recover_release "$approved" "$notes"
  verify_published_release "$approved" "$notes"
  rm -f -- "$notes"
}

self_test() {
  RELEASE_VERSION=0.0.0
  RELEASE_TAG=v0.0.0
  CANDIDATE_RUN_ID=123
  CANDIDATE_RUN_ATTEMPT=2
  VERIFIED_SOURCE_SHA=1111111111111111111111111111111111111111
  local notes
  notes="$(mktemp)"
  write_release_notes "$notes" \
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  grep -Fq "blob/v0.0.0/docs/INSTALL_WINDOWS.md" "$notes"
  grep -Fq "blob/v0.0.0/docs/INSTALL_MACOS.md" "$notes"
  grep -Fq "blob/v0.0.0/docs/BUILD.md" "$notes"
  grep -Fq "/actions/runs/123/attempts/2" "$notes"
  grep -Fq "$VERIFIED_SOURCE_SHA" "$notes"
  grep -Fq 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' "$notes"
  grep -Fq 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' "$notes"
  rm -f -- "$notes"
  printf '%s\n' "Release publication helper self-test passed."
}

if [[ "${1:-}" == --self-test ]]; then
  test "$#" = 1 || die "--self-test accepts no additional arguments"
  self_test
  exit 0
fi

test "$#" -ge 1 || die "usage: $SCRIPT_NAME prepare|publish ..."
readonly MODE="$1"
shift

for command_name in awk bash cmp cp find gh git grep jq sha256sum sort wc; do
  require_command "$command_name"
done
require_environment \
  GITHUB_REPOSITORY CANDIDATE_RUN_ID CANDIDATE_RUN_ATTEMPT \
  RELEASE_VERSION RELEASE_TAG VERIFIED_SOURCE_SHA GH_TOKEN
test "$GITHUB_REPOSITORY" = "$REPOSITORY" || die "repository identity mismatch"
[[ "$RELEASE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "release version is invalid"
test "$RELEASE_TAG" = "v$RELEASE_VERSION" || die "release tag/version mismatch"
is_sha1 "$VERIFIED_SOURCE_SHA" || die "verified source SHA is invalid"
is_positive_integer "$CANDIDATE_RUN_ID" || die "candidate run ID is invalid"
is_positive_integer "$CANDIDATE_RUN_ATTEMPT" || die "candidate run attempt is invalid"
assert_source_checkout
bash scripts/audit-versions.sh "$RELEASE_VERSION" >/dev/null

case "$MODE" in
  prepare) prepare_approval "$@" ;;
  check-remote) check_remote_state "$@" ;;
  publish) publish_approved "$@" ;;
  *) die "unsupported mode: $MODE" ;;
esac
