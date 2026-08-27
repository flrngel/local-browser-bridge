# Withdrawn v0.12.43 candidate attempt 1

This directory is immutable negative-history context, not release evidence.
The exact v0.12.43 candidate was bound to source
`32bdbc653c8206c1c6a8c3fc9f74e8abb3cab004`, workflow run `33119834863`,
attempt `1`, and release-candidate artifact `9666274762`.

The static trust gate passed. The raw artifact ZIP SHA-256 was
`c0bcab65910b8180bd34fd176721591baa4735fc12ce971208e4a7adaa066272`,
and the checksum-manifest SHA-256 was
`e27c3bdaf008a58598a5f4135961985a3fe9a9751023bff27efd58fdc967166a`.
Package preparation accepted the exact 17-entry macOS archive without
executing candidate bytes.

The one-shot quiet lane then passed 207 of 207 assertions. Its reviewed
`helper-results.json` SHA-256 was
`50cdbd61f5617229d124c632c4d2aa045d36964c298b6cf66505b5a055ee38b0`.
All six fixture-only screenshots passed their recorded hashes and visual
review:

- `computer-01-exact-window-observe.png`: `00f6b9ab6396171fdd377f21c02f2bfbaa3aeeaa873116c58e1c3c60538d021e`
- `computer-02-semantic-set-value.png`: `48e66a0db965344b02467ab9c5b746b397f40779aa982e073982990a591c65ec`
- `computer-03-semantic-invoke.png`: `b10a8bee9b4ae4e44409cd96feb4139f843ac207194f4d430ad2e30276997ac6`
- `computer-04-persistent-scstream-start.png`: `649f2219156c14eaa72e3ce746fa511e859ce852961c69d07171923802573fae`
- `computer-05-live-share-pixel-action.png`: `16d566ba04ff634a496573ee5d95a15c37f623aace2b74512f8374857f6cc852`
- `computer-06-persistent-share-resize.png`: `94ccc72bf62443fc5af0b3cf7cdf87055a3c76f5c74b4a0289e73fc0f0e6ec62`

Immediately before the deliberate-concurrency lane, the mandatory second
source-only quiet-readiness command returned nonzero. Because the command ran
inside a shell assignment with `set -e`, its ephemeral diagnostic JSON was not
retained. No deliberate runner or watcher was launched, no deliberate
candidate bytes were executed, no UI action occurred, and Windows and Chrome
acceptance never started.

The shell terminated at that boundary. Reusing the passing quiet result would
resume or combine a terminal attempt, so run `33119834863` attempt `1` and
artifact `9666274762` were withdrawn. Any later workflow attempt must use a new
artifact, new binding, and two entirely fresh macOS lanes.
