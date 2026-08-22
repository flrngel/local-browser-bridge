# Withdrawn macOS candidate: transitional resize frame

This directory preserves the sanitized diagnostic result from candidate commit
`ffd9f8bee7748dbc7255d7d95515e817cce374a6`. It is not release evidence.

The packaged archive matched SHA-256
`048d28336f8726110cd41260b7205d3dbcbf53eceade90e0b212fcb7e6966cd7`.
The automated harness reported 98/98 checks, but mandatory visual review
rejected the result. `computer-06-persistent-share-resize.png` was associated
with new source geometry while its captured pixels still showed
`last=click size=720x460`, the pre-resize fixture state. Its changed hash came
from animation and pointer pixels, so a hash-only assertion accepted a
transitional frame.

The run is retained to prove that the visual review gate caught a false
automated pass. The harness was strengthened to wait for a later acknowledged
native frame whose captured-image aspect ratio matches the resized exact
window, and to require the saved PNG dimensions to match that settled
observation. All six screenshots contain only the deterministic fixture
window; the JSON and log are sanitized.
