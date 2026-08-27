# Withdrawn v0.12.39 macOS attempt 3

This immutable negative record belongs to workflow run `33051081816`, attempt
`3`, release-candidate artifact `9639189449`, and source
`f402c019edc1892ac2f040c5d6e4c60a8fe46e4a`.

The fresh quiet lane passed 206/206 assertions and its six reviewed screenshots
contained only the test fixture. The deliberate-concurrency lane reached the
exact app-share handoff, accepted one bound `START APP-SHARE CHECK` action, and
then failed closed before product dispatch. The independent system record kept
the foreground process, raw foreground identity, AX focused/main window,
frontmost state, cursor, HID counters, and active Space unchanged, but the
separate CGWindow front-window identifier changed. Consequently
`userFocusUnchanged` was false and the candidate stopped after 84 passing and
one failing assertion. `actionDispatched` remained false.

The exact run/attempt/artifact was withdrawn. Windows acceptance, stock-Chrome
acceptance, tagging, and release publication were not started from this
attempt. These bytes must not be resumed, merged into successful evidence, or
relabelled for another workflow attempt.

The retained files are the complete quiet result, log, and six screenshots,
plus the deliberate result, log, six screenshots, and two create-once operator
notifications. A sanitized leakage scan found no bearer or GitHub token,
user/home or temporary path, email, loopback endpoint, or OAuth value.
