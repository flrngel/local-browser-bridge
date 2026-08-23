//! Target-neutral computer-helper protocol and cancellation primitives.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(any(test, target_os = "macos", target_os = "windows"))]
use std::time::Instant;

use serde_json::{Map, Value, json};
use thiserror::Error;

pub const COMPUTER_HELPER_ORIGIN: &str = "lbb-computer-helper://local";
/// Capability string a helper advertises in its hello to negotiate
/// server-acknowledged share-frame pacing. It is a feature flag, never a
/// dispatchable command method.
pub const COMPUTER_SHARE_ACK_CAPABILITY: &str = "computer.share.ack";
/// Capability string indicating that `computer.share.start` creates one
/// persistent OS exact-window capture stream instead of polling snapshot APIs.
/// It is metadata, not a dispatchable command method.
pub const COMPUTER_NATIVE_SHARE_CAPABILITY: &str = "computer.capture.native-stream.v1";
/// Capability string indicating that every native action reports a sealed
/// exact-target delivery route and separates API acceptance from target effect.
pub const COMPUTER_INPUT_DELIVERY_PROVENANCE_CAPABILITY: &str =
    "computer.input-delivery-provenance.v1";
/// Capability string indicating that the helper samples a platform pointer
/// activity monitor without retaining input contents, coordinates, or devices.
pub const COMPUTER_POINTER_ACTIVITY_MONITOR_CAPABILITY: &str =
    "computer.pointer-activity-monitor.v1";
/// Native text delivery is intentionally much smaller than browser or
/// accessibility value writes. Windows posts one message per UTF-16 code unit
/// and permits a per-queue minimum as low as 4,000 posted messages, so one
/// command may consume at most half of that minimum while paced delivery gives
/// the target time to drain its queue.
pub const COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS: usize = 2_000;
/// A native text action must finish well before the helper/server watchdogs.
/// Platform loops enforce this budget cooperatively between dispatched units.
pub const COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS: u64 = 2_500;
pub const COMPUTER_METHODS: &[&str] = &[
    "computer.status",
    "computer.share.start",
    "computer.share.status",
    "computer.share.stop",
    "computer.observe",
    "computer.move",
    "computer.click",
    "computer.drag",
    "computer.scroll",
    "computer.typeText",
    "computer.key",
    "computer.invoke",
    "computer.setValue",
];

/// Identity-bound acknowledgement for one emitted native share frame.
///
/// Transport sequences restart for each share lease, so the share ID is part
/// of the acknowledgement identity rather than optional metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareFrameAck {
    pub share_id: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct ComputerError {
    pub code: String,
    pub message: String,
}

impl ComputerError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Validates the cost of a native text command in the unit used by the
/// Windows delivery primitive. Counting UTF-16 also gives supplementary-plane
/// text the same cross-platform bound instead of under-counting surrogate
/// pairs as one Rust `char`.
pub fn validate_computer_type_text(text: &str) -> Result<usize, ComputerError> {
    if text.is_empty() {
        return Err(ComputerError::new(
            "COMPUTER_INVALID_REQUEST",
            "text must not be empty",
        ));
    }
    if text.contains('\0') {
        return Err(ComputerError::new(
            "COMPUTER_INVALID_REQUEST",
            "text must not contain U+0000",
        ));
    }
    let utf16_units = text.encode_utf16().count();
    if utf16_units > COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS {
        return Err(ComputerError::new(
            "COMPUTER_INVALID_REQUEST",
            format!(
                "text exceeds the {COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS} UTF-16 code-unit native delivery limit"
            ),
        ));
    }
    Ok(utf16_units)
}

#[derive(Clone, Default)]
pub struct CommandCancellation {
    canceled: Arc<AtomicBool>,
    dispatched: Arc<AtomicBool>,
    dispatch_phase: Arc<Mutex<DispatchPhase>>,
}

#[derive(Default)]
struct DispatchPhase {
    epoch: u64,
    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    verification_started: Option<(u64, Instant)>,
}

impl CommandCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::Release);
    }

    pub fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::Acquire)
    }

    pub fn was_dispatched(&self) -> bool {
        self.dispatched.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self, boundary: &str) -> Result<(), ComputerError> {
        if self.is_canceled() {
            Err(self.cancellation_error(boundary))
        } else {
            Ok(())
        }
    }

    /// Marks the first side-effect boundary so cancellation can distinguish a
    /// retry-safe refusal from an outcome that may already have changed state.
    pub fn begin_side_effect(&self, boundary: &str) -> Result<(), ComputerError> {
        self.check(boundary)?;
        {
            let mut phase = self
                .dispatch_phase
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            phase.epoch = phase.epoch.checked_add(1).ok_or_else(|| {
                ComputerError::new(
                    "COMPUTER_DISPATCH_EXHAUSTED",
                    "The command exceeded its side-effect accounting range",
                )
            })?;
            // A later side effect in a compound action supersedes an earlier
            // verification boundary. The final platform mutation owns the
            // action-level dispatch/verification split.
            #[cfg(any(test, target_os = "macos", target_os = "windows"))]
            {
                phase.verification_started = None;
            }
        }
        self.dispatched.store(true, Ordering::Release);
        self.check(boundary)
    }

    /// Records the transition from the final native mutation into target-side
    /// and non-interruption verification. Providers with an exact read-back
    /// call this immediately after the OS mutation; platform guards provide a
    /// fallback after their dispatch closure returns.
    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    pub(crate) fn mark_verification_started(&self) {
        if !self.was_dispatched() {
            return;
        }
        let mut phase = self
            .dispatch_phase
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if phase.verification_started.is_none() {
            let epoch = phase.epoch;
            phase.verification_started = Some((epoch, Instant::now()));
        }
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    pub(crate) fn verification_started_at(&self) -> Option<Instant> {
        let phase = self
            .dispatch_phase
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        phase
            .verification_started
            .filter(|(epoch, _)| *epoch == phase.epoch)
            .map(|(_, instant)| instant)
    }

    pub fn finish<T>(&self, result: Result<T, ComputerError>) -> Result<T, ComputerError> {
        if result
            .as_ref()
            .err()
            .is_some_and(|error| error.code == "COMPUTER_OUTCOME_UNKNOWN")
        {
            return result;
        }
        if self.was_dispatched() && (self.is_canceled() || result.is_err()) {
            let detail = result
                .as_ref()
                .err()
                .map(|error| format!(" {}: {}", error.code, error.message))
                .unwrap_or_default();
            Err(ComputerError::new(
                "COMPUTER_OUTCOME_UNKNOWN",
                format!(
                    "The command was canceled or failed after computer input dispatch; observe again and do not automatically retry.{detail}"
                ),
            ))
        } else {
            result
        }
    }

    fn cancellation_error(&self, boundary: &str) -> ComputerError {
        if self.was_dispatched() {
            ComputerError::new(
                "COMPUTER_OUTCOME_UNKNOWN",
                format!(
                    "The command was canceled after computer input dispatch at {boundary}; observe again and do not automatically retry"
                ),
            )
        } else {
            ComputerError::new(
                "COMPUTER_CANCELED",
                format!("The command was canceled before {boundary}"),
            )
        }
    }
}

/// Latest-frame-wins single-slot mailbox pacing `computer.share.frame` emissions.
///
/// The producer parks at most one captured frame; a newer capture replaces an
/// unemitted one and counts it as dropped, so the producer never buffers more
/// than the single slot. With ack pacing enabled the next emission waits until
/// the server acknowledged the previously emitted share sequence; without it
/// the mailbox degenerates to the legacy emit-on-capture timer behavior.
/// Sequences are assigned by the capture side and stay strictly increasing,
/// including across dropped frames.
#[derive(Debug, Clone, Default)]
pub struct ShareMailbox {
    ack_paced: bool,
    slot: Option<(u64, Value)>,
    last_sequence: u64,
    awaiting_ack: Option<u64>,
    dropped_frames: u64,
    last_acked_sequence: u64,
}

impl ShareMailbox {
    pub fn new(ack_paced: bool) -> Self {
        Self {
            ack_paced,
            ..Self::default()
        }
    }

    pub fn ack_paced(&self) -> bool {
        self.ack_paced
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    /// Share sequence of the last frame the server acknowledged; 0 before any ack.
    pub fn last_acked_sequence(&self) -> u64 {
        self.last_acked_sequence
    }

    /// Invalidates a captured frame that has not left the helper yet.
    ///
    /// Geometry-epoch changes use this before accepting a replacement native
    /// frame, so a locally queued old-geometry observation cannot be emitted
    /// after the exact window has moved or resized. An already emitted frame
    /// remains ack-paced; the controller separately revokes its action token.
    pub fn discard_pending(&mut self) -> bool {
        if self.slot.take().is_some() {
            self.dropped_frames = self.dropped_frames.saturating_add(1);
            true
        } else {
            false
        }
    }

    /// Parks the newest captured frame in the single slot.
    ///
    /// A capture that does not advance the strictly increasing share sequence
    /// is refused. Replacing a parked, still-unemitted frame counts it as
    /// dropped; the counter is monotonic for the mailbox lifetime (one share).
    pub fn produce(&mut self, sequence: u64, frame: Value) -> bool {
        if sequence <= self.last_sequence {
            return false;
        }
        self.last_sequence = sequence;
        if self.slot.replace((sequence, frame)).is_some() {
            self.dropped_frames = self.dropped_frames.saturating_add(1);
        }
        true
    }

    /// Takes the parked frame when pacing allows another emission.
    ///
    /// While an ack-paced emission is still unacknowledged, nothing is
    /// released; the producer keeps replacing the slot in the meantime.
    pub fn emit(&mut self) -> Option<(u64, Value)> {
        if self.ack_paced && self.awaiting_ack.is_some() {
            return None;
        }
        let (sequence, frame) = self.slot.take()?;
        if self.ack_paced {
            self.awaiting_ack = Some(sequence);
        }
        Some((sequence, frame))
    }

    /// Applies a server acknowledgement.
    ///
    /// Anything but the exact awaited share sequence — stale, unknown, or
    /// duplicate — is ignored.
    pub fn acknowledge(&mut self, sequence: u64) -> bool {
        if self.awaiting_ack != Some(sequence) {
            return false;
        }
        self.awaiting_ack = None;
        self.last_acked_sequence = sequence;
        true
    }
}

pub fn result_envelope(id: &str, result: Result<Value, ComputerError>) -> Value {
    match result {
        Ok(result) => json!({ "id": id, "type": "result", "ok": true, "result": result }),
        Err(error) => json!({
            "id": id,
            "type": "result",
            "ok": false,
            "error": { "code": error.code, "message": error.message },
        }),
    }
}

pub fn command_parts(message: &Value) -> Option<(&str, &str, Value)> {
    if message.get("type").and_then(Value::as_str) != Some("command") {
        return None;
    }
    Some((
        message.get("id")?.as_str()?,
        message.get("method")?.as_str()?,
        message
            .get("params")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_command_envelopes() {
        let message = json!({
            "id": "abc", "type": "command", "method": "computer.status", "params": {}
        });
        let (id, method, params) = command_parts(&message).unwrap();
        assert_eq!(id, "abc");
        assert_eq!(method, "computer.status");
        assert_eq!(params, json!({}));
        assert!(command_parts(&json!({ "type": "event" })).is_none());
    }

    #[test]
    fn cancellation_before_dispatch_is_retry_safe() {
        let cancellation = CommandCancellation::new();
        cancellation.cancel();
        let error = cancellation.begin_side_effect("test dispatch").unwrap_err();
        assert_eq!(error.code, "COMPUTER_CANCELED");
        assert!(!cancellation.was_dispatched());
    }

    #[test]
    fn native_text_budget_counts_utf16_and_rejects_empty_or_nul() {
        assert_eq!(
            validate_computer_type_text(&"a".repeat(COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS)).unwrap(),
            COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS
        );
        assert_eq!(
            validate_computer_type_text(&"😀".repeat(COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS / 2))
                .unwrap(),
            COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS
        );

        for invalid in [
            String::new(),
            "before\0after".to_owned(),
            "a".repeat(COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS + 1),
            "😀".repeat(COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS / 2 + 1),
        ] {
            let error = validate_computer_type_text(&invalid).unwrap_err();
            assert_eq!(error.code, "COMPUTER_INVALID_REQUEST");
        }
    }

    #[test]
    fn verification_boundary_tracks_the_final_compound_side_effect() {
        let cancellation = CommandCancellation::new();
        cancellation.begin_side_effect("pointer approach").unwrap();
        cancellation.mark_verification_started();
        let first = cancellation.verification_started_at().unwrap();

        cancellation.begin_side_effect("click dispatch").unwrap();
        assert!(cancellation.verification_started_at().is_none());
        cancellation.mark_verification_started();
        let final_boundary = cancellation.verification_started_at().unwrap();

        assert!(final_boundary >= first);
    }

    #[test]
    fn mailbox_emits_only_the_newest_frame_under_slow_acks() {
        let mut mailbox = ShareMailbox::new(true);
        assert!(mailbox.produce(1, json!({ "capture": 1 })));
        let (sequence, frame) = mailbox.emit().unwrap();
        assert_eq!(sequence, 1);
        assert_eq!(frame, json!({ "capture": 1 }));

        // The server is slow: three captures land before the first ack.
        assert!(mailbox.produce(2, json!({ "capture": 2 })));
        assert!(mailbox.produce(3, json!({ "capture": 3 })));
        assert!(mailbox.produce(4, json!({ "capture": 4 })));
        assert!(mailbox.emit().is_none());
        assert_eq!(mailbox.dropped_frames(), 2);

        assert!(mailbox.acknowledge(1));
        let (sequence, frame) = mailbox.emit().unwrap();
        assert_eq!(sequence, 4);
        assert_eq!(frame, json!({ "capture": 4 }));
        assert!(sequence > 1, "sequences stay increasing across drops");
        assert_eq!(mailbox.dropped_frames(), 2);
        assert!(mailbox.acknowledge(4));
        assert_eq!(mailbox.last_acked_sequence(), 4);
    }

    #[test]
    fn mailbox_ignores_stale_unknown_and_duplicate_acks() {
        let mut mailbox = ShareMailbox::new(true);
        assert!(!mailbox.acknowledge(1), "nothing was emitted yet");
        assert!(mailbox.produce(7, json!({})));
        assert_eq!(mailbox.emit().unwrap().0, 7);

        assert!(!mailbox.acknowledge(6), "stale sequence is ignored");
        assert!(!mailbox.acknowledge(8), "unknown sequence is ignored");
        assert_eq!(mailbox.last_acked_sequence(), 0);
        assert!(mailbox.produce(8, json!({})));
        assert!(mailbox.emit().is_none(), "still awaiting the exact ack");

        assert!(mailbox.acknowledge(7));
        assert!(!mailbox.acknowledge(7), "duplicate ack is ignored");
        assert_eq!(mailbox.emit().unwrap().0, 8);
        assert_eq!(mailbox.last_acked_sequence(), 7);
    }

    #[test]
    fn mailbox_without_ack_pacing_keeps_timer_behavior_and_monotonic_produce() {
        let mut mailbox = ShareMailbox::new(false);
        assert!(!mailbox.ack_paced());
        assert!(mailbox.produce(1, json!({})));
        assert_eq!(mailbox.emit().unwrap().0, 1);
        assert!(mailbox.produce(2, json!({})));
        assert_eq!(mailbox.emit().unwrap().0, 2, "never waits for an ack");
        assert_eq!(mailbox.dropped_frames(), 0);
        assert!(!mailbox.acknowledge(2), "no emission ever awaits an ack");

        assert!(!mailbox.produce(2, json!({})), "sequences must advance");
        assert!(!mailbox.produce(1, json!({})), "sequences never rewind");
        assert!(mailbox.emit().is_none());
        assert_eq!(mailbox.dropped_frames(), 0);
    }

    #[test]
    fn mailbox_discards_a_pending_old_geometry_frame_without_rewinding() {
        let mut mailbox = ShareMailbox::new(true);
        assert!(mailbox.produce(4, json!({ "geometry": "old" })));
        assert!(mailbox.discard_pending());
        assert!(!mailbox.discard_pending());
        assert!(mailbox.emit().is_none());
        assert_eq!(mailbox.dropped_frames(), 1);
        assert!(!mailbox.produce(4, json!({})), "sequence cannot rewind");
        assert!(mailbox.produce(5, json!({ "geometry": "new" })));
        assert_eq!(mailbox.emit().unwrap().0, 5);
    }
}
