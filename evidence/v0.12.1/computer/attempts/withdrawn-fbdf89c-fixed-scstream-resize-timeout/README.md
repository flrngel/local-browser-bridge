# Withdrawn macOS candidate: fixed-dimension SCStream resize failure

This directory preserves the sanitized negative result from candidate commit
`fbdf89cd11d1545704e63f3d1f552b47605b2e31`. It is not release evidence.

The packaged archive matched SHA-256
`43e244f48223c75e0748a838ef95a66bf0aad76f75f14d098d4e0b4481b09c13`.
The run passed all 55 checks before resize, and the deterministic fixture
confirmed exactly one requested resize with `lastAction=resize`. All four
non-interruption probe equalities remained true. The harness then failed closed
after waiting 20 seconds for a later persistent-share frame whose image aspect
matched the resized exact-window geometry.

The strengthened predicate was not relaxed. The candidate configures the
ScreenCaptureKit stream dimensions only at startup, so later observations can
combine resized window metadata with pixel buffers that retain the original
stream aspect. The five retained screenshots contain only the deterministic
fixture window. No sixth screenshot was accepted or saved; `helper-results.json`
and `helper-rig.log` preserve the complete sanitized failure.
