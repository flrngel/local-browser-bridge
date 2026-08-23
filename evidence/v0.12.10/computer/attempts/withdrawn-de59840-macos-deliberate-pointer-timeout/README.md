# Withdrawn v0.12.10 macOS exact-candidate attempt

This directory preserves the failed macOS acceptance run for the exact v0.12.10
release candidate. The protected publication workflow was canceled without
approval, Windows and stock-Chrome acceptance were not started, and no v0.12.10
GitHub Release was created. Nothing in this directory is evidence for a shipped
release.

## Frozen candidate binding

- Source commit: `de5984076ccc3f71e253fba2c81a0c51bb03b0dc`
- Annotated tag object: `8254c504bb2450365cc7cc7df02d0eacadbc8dcd`
- Tag: `v0.12.10`
- Deploy workflow run: `32638488606`, attempt `1`
- Release-candidate artifact: `9493144770`
- Artifact ZIP bytes: `10432758`
- Artifact ZIP SHA-256:
  `86fa1bad3d5cd9b39cc5ffe5d13d1f372b5759a5315542c3210ca6c523a576ee`
- `SHA256SUMS.txt` SHA-256:
  `0568ec372cc2b396578d0d533c33c22dbdfbcb2907eaaa3ab89d3cd20dca8f48`
- macOS archive SHA-256:
  `eb1e85ec5825232390e38cb66013f95dff36d8cf6459e2e5fd87d6e870e78a4e`
- Packaged server SHA-256:
  `8435b0daecbc3de32a11620edfd5c46d6d512eb374d90448cc26373c1bba0a9d`
- Packaged helper SHA-256:
  `2611ef0692fd27066a2a61e7295363d2e014b983505b08aa3bb26d6a9288250c`

Before either executable ran, the coordinator verified the artifact API
binding, raw artifact digest, exact five-file inventory, canonical manifest,
every payload hash, source/tag/workflow/run-attempt identity, GitHub-hosted
runner identity, and all five candidate-file GitHub attestations. Both macOS
executables were universal `arm64`/`x86_64`, reported version 0.12.10, and
passed strict structural code-signature checks.

## Result and withdrawal reason

`helper-results.json` is a schema-4 `failed-release-candidate` record. The rig
recorded 69 passing assertions and zero assertion failures before the final
human-participation gate; that assertion count is not a passing result. The
single exact packaged run proved candidate binding, permission preflight,
one-helper topology, target/sibling discovery, exact-window observation,
semantic set-value and invoke, persistent ScreenCaptureKit streaming,
background pixel routing, live-share continuity, and exact-target resize.

At the deliberate-concurrency stage the rig requested continuous movement of
the shared hardware pointer without clicks. The pre-action independent pointer
probe observed no authorized activity during the 300-second window, so the
post-resize product action and its two-boundary concurrency proof were never
started. The rig therefore failed closed with
`separately authorized deliberate shared-pointer activity timed out`, stopped
before the post-resize action, and did not infer physical activity from quiet
samples or synthesize input. The candidate was not retried or reused.

This is an operator-handoff timeout, not evidence that the preceding product
assertions failed. A successor must make the human handoff visible and reliable
while preserving the requirement that neither the product nor its fixture can
self-attest independent shared-seat activity.

## Retained inventory

| File | Bytes | SHA-256 |
|---|---:|---|
| `computer-01-exact-window-observe.png` | 779690 | `ec4e34ef15cc8e2007523949dafaef1cf6ebe30bd5e309614a43b202eb5d436b` |
| `computer-02-semantic-set-value.png` | 796796 | `7d1111f4c65763be9769b8e5ff3e681de1ab44737251e8d1f938086f80e75d97` |
| `computer-03-semantic-invoke.png` | 798271 | `f3efb1d688aeb472efe9cdd2112030083eaf34dccca6bc5b11b22ee0235bc002` |
| `computer-04-persistent-scstream-start.png` | 664668 | `0f2fe8b49c242262cf0d1e2088924b712874fd3f8c9ca151c7f38ec2e5900d7b` |
| `computer-05-live-share-pixel-action.png` | 665513 | `5e05b1165715b234bd527c8039ed11f804ce9c7852e706fd30fa3216a2454e6a` |
| `computer-06-persistent-share-resize.png` | 534020 | `474c6f215288717e053d00a0cbfc60227733e1802d8b9b51467a00b214cf01ff` |
| `helper-results.json` | 19707 | `70794822e86c9749bac064c011377fc6c87ef387834c91250a92ec6c9f9e2fc2` |
| `helper-rig.log` | 10800 | `12db1c5278c7b6232c24aaf3a7d69b89cad5882d72d20468ef2373eb268dd50c` |

All six PNG hashes and dimensions match the screenshot records in
`helper-results.json`. Visual review confirmed that every image contains only
the deterministic fixture, with no desktop, unrelated window, personal
content, native-text payload, or secret. The PNGs are RGBA, have no color
profile, and retain no user-identifying metadata.

The result and log contain no bearer token, authorization value, user/home
path, hostname, email address, signed URL, command-line secret, or unrelated
content. The log records the owned helper, server, and fixture stopping and the
scratch directory being removed. An independent post-run check found no
related process or relevant loopback listener.
