# Withdrawn v0.12.43 candidate attempt 2

This directory is immutable negative-history context, not release evidence.
The exact v0.12.43 candidate was bound to source
`32bdbc653c8206c1c6a8c3fc9f74e8abb3cab004`, workflow run `33119834863`,
attempt `2`, and release-candidate artifact `9666950262`.

The static trust gate passed. The raw artifact ZIP SHA-256 was
`2aa942b3fe941f8a41a83a9fc27af9bf5d6095227feb1bf114215c0f5fa38fd1`,
the checksum-manifest SHA-256 was
`186b155b56aaf09b004d6fdf902f4130df906e27652ada91aa58bec458171409`,
and the macOS archive SHA-256 was
`14b1981079eca21a142783c1dd41e9affed1832c0211e9306f386da7cf3d7f8f`.
Package preparation accepted the exact 17-entry macOS archive without
executing candidate bytes.

The one-shot quiet lane passed 207 of 207 assertions. Its reviewed
`helper-results.json` SHA-256 was
`43a4cac29b42c04913b967faa9cb94504924d09b508b2b73a6b8eb5416dd59a2`.
All six fixture-only screenshots passed their recorded hashes and visual
review:

- `computer-01-exact-window-observe.png`: `00f6b9ab6396171fdd377f21c02f2bfbaa3aeeaa873116c58e1c3c60538d021e`
- `computer-02-semantic-set-value.png`: `a9adc4d3642c0f706fe4f58c1652aa52ec52932e331a8a606637b5cf0a432921`
- `computer-03-semantic-invoke.png`: `10257db0247c058191462cd64975dcae6c29ae66c47f2ca0017e783ab4ccb3e2`
- `computer-04-persistent-scstream-start.png`: `25403cc582ea1342046fd87e803520c04cfd2e4f3762dffebb694913e5ce8d6a`
- `computer-05-live-share-pixel-action.png`: `b3374ba0483db1106e12a68f1f30c58325b0aee3d0aece26f2c5519656521c64`
- `computer-06-persistent-share-resize.png`: `ba4d153f9cca9eefd5e61b389892ce327969fda0789c56793e4d7133ada8b469`

The mandatory second source-only readiness gate passed. The fresh
deliberate-concurrency lane then passed its pre-execution quiet-seat gate and
all product checks through the exact-app-share READY presentation. The exact
app-share button action was recorded once, but the post-action independent
input-seat check observed foreground and focus changes, a hardware cursor
change, HID pointer activity, HID keyboard activity, and a contaminated shared
input seat. The runner terminated immediately before accepting or finalizing
the deliberate lane. Windows and Chrome acceptance never started.

The failed deliberate output was not reviewed or promoted as release evidence.
Reusing either macOS lane would resume or combine a terminal attempt, so run
`33119834863` attempt `2` and artifact `9666950262` were withdrawn. Any later
workflow attempt must use a new artifact, new binding, and two entirely fresh
macOS lanes.
