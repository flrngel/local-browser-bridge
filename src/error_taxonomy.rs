//! Structured error taxonomy mapping legacy error codes to recovery coaching.
//!
//! Every error code emitted by the server, the browser extension, or the
//! computer helper classifies into one canonical taxonomy code with a fixed
//! retriability flag, a recovery hint, and one sentence of model coaching.
//! Unrecognized codes classify as `unknown` and are never retriable.

/// Canonical taxonomy codes exposed as `taxonomy.code` on failed responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaxonomyCode {
    StaleSnapshot,
    StaleRef,
    TargetChanged,
    OutOfBounds,
    NotInteractable,
    Obscured,
    DocumentChanged,
    LeaseLost,
    NeedsUser,
    BlockedByPolicy,
    BlockedByDialog,
    SensitiveField,
    OutcomeUnknown,
    Timeout,
    WaitTimeout,
    Overloaded,
    ProtocolMismatch,
    Unavailable,
    InvalidRequest,
    Unknown,
}

#[cfg(test)]
impl TaxonomyCode {
    /// Every canonical code, used to prove the taxonomy table stays exhaustive.
    pub const ALL: [TaxonomyCode; 20] = [
        TaxonomyCode::StaleSnapshot,
        TaxonomyCode::StaleRef,
        TaxonomyCode::TargetChanged,
        TaxonomyCode::OutOfBounds,
        TaxonomyCode::NotInteractable,
        TaxonomyCode::Obscured,
        TaxonomyCode::DocumentChanged,
        TaxonomyCode::LeaseLost,
        TaxonomyCode::NeedsUser,
        TaxonomyCode::BlockedByPolicy,
        TaxonomyCode::BlockedByDialog,
        TaxonomyCode::SensitiveField,
        TaxonomyCode::OutcomeUnknown,
        TaxonomyCode::Timeout,
        TaxonomyCode::WaitTimeout,
        TaxonomyCode::Overloaded,
        TaxonomyCode::ProtocolMismatch,
        TaxonomyCode::Unavailable,
        TaxonomyCode::InvalidRequest,
        TaxonomyCode::Unknown,
    ];
}

impl TaxonomyCode {
    pub fn as_str(self) -> &'static str {
        match self {
            TaxonomyCode::StaleSnapshot => "stale_snapshot",
            TaxonomyCode::StaleRef => "stale_ref",
            TaxonomyCode::TargetChanged => "target_changed",
            TaxonomyCode::OutOfBounds => "out_of_bounds",
            TaxonomyCode::NotInteractable => "not_interactable",
            TaxonomyCode::Obscured => "obscured",
            TaxonomyCode::DocumentChanged => "document_changed",
            TaxonomyCode::LeaseLost => "lease_lost",
            TaxonomyCode::NeedsUser => "needs_user",
            TaxonomyCode::BlockedByPolicy => "blocked_by_policy",
            TaxonomyCode::BlockedByDialog => "blocked_by_dialog",
            TaxonomyCode::SensitiveField => "sensitive_field",
            TaxonomyCode::OutcomeUnknown => "outcome_unknown",
            TaxonomyCode::Timeout => "timeout",
            TaxonomyCode::WaitTimeout => "wait_timeout",
            TaxonomyCode::Overloaded => "overloaded",
            TaxonomyCode::ProtocolMismatch => "protocol_mismatch",
            TaxonomyCode::Unavailable => "unavailable",
            TaxonomyCode::InvalidRequest => "invalid_request",
            TaxonomyCode::Unknown => "unknown",
        }
    }
}

/// The single next step a model should take after this class of failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryHint {
    Reobserve,
    Wait,
    Resume,
    Handback,
    Reconnect,
    None,
}

impl RecoveryHint {
    pub fn as_str(self) -> &'static str {
        match self {
            RecoveryHint::Reobserve => "reobserve",
            RecoveryHint::Wait => "wait",
            RecoveryHint::Resume => "resume",
            RecoveryHint::Handback => "handback",
            RecoveryHint::Reconnect => "reconnect",
            RecoveryHint::None => "none",
        }
    }
}

/// One taxonomy entry: canonical code plus its fixed recovery coaching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taxonomy {
    pub code: TaxonomyCode,
    pub retriable: bool,
    pub recovery_hint: RecoveryHint,
    pub prose: &'static str,
}

impl Taxonomy {
    /// The fixed entry for a canonical code; the match is exhaustive so a new
    /// code cannot ship without recovery coaching.
    pub fn for_code(code: TaxonomyCode) -> Taxonomy {
        let (retriable, recovery_hint, prose) = match code {
            TaxonomyCode::StaleSnapshot => (
                true,
                RecoveryHint::Reobserve,
                "The snapshot or frame you acted on is no longer current; observe again and retry with fresh identifiers.",
            ),
            TaxonomyCode::StaleRef => (
                true,
                RecoveryHint::Reobserve,
                "That element reference no longer resolves; observe again and use a ref from the new observation.",
            ),
            TaxonomyCode::TargetChanged => (
                true,
                RecoveryHint::Reobserve,
                "The target changed or disappeared after observation; observe again before retrying.",
            ),
            TaxonomyCode::OutOfBounds => (
                true,
                RecoveryHint::Reobserve,
                "The requested point is outside the observed area; observe again and pick coordinates inside the current bounds.",
            ),
            TaxonomyCode::NotInteractable => (
                true,
                RecoveryHint::Reobserve,
                "The target cannot be interacted with right now; observe again and choose an enabled, reachable element.",
            ),
            TaxonomyCode::Obscured => (
                true,
                RecoveryHint::Reobserve,
                "Another surface covers the target; observe again and act on what is actually on top.",
            ),
            TaxonomyCode::DocumentChanged => (
                true,
                RecoveryHint::Reobserve,
                "The document navigated or mutated during the action; observe again once the page settles.",
            ),
            TaxonomyCode::LeaseLost => (
                true,
                RecoveryHint::Resume,
                "The control lease is no longer yours; restart or resynchronize the control session before acting.",
            ),
            TaxonomyCode::NeedsUser => (
                false,
                RecoveryHint::Handback,
                "A human must complete this step; hand control back to the user instead of retrying.",
            ),
            TaxonomyCode::BlockedByPolicy => (
                false,
                RecoveryHint::None,
                "Bridge policy forbids this request; do not retry the same action.",
            ),
            TaxonomyCode::BlockedByDialog => (
                true,
                RecoveryHint::Resume,
                "A dialog is blocking the page; resolve it first, then retry the action.",
            ),
            TaxonomyCode::SensitiveField => (
                false,
                RecoveryHint::Handback,
                "This field is sensitive; ask the human to enter the value manually.",
            ),
            TaxonomyCode::OutcomeUnknown => (
                false,
                RecoveryHint::Reobserve,
                "The command's outcome is unknown; observe the current state before deciding whether to act again.",
            ),
            TaxonomyCode::Timeout => (
                true,
                RecoveryHint::Wait,
                "The operation timed out; wait briefly and try again.",
            ),
            TaxonomyCode::WaitTimeout => (
                false,
                RecoveryHint::Reobserve,
                "The awaited page condition did not occur within the timeout; this is a normal result, so observe the page and decide the next step instead of repeating the same wait.",
            ),
            TaxonomyCode::Overloaded => (
                true,
                RecoveryHint::Wait,
                "The bridge or its connector is busy; wait briefly and try again.",
            ),
            TaxonomyCode::ProtocolMismatch => (
                false,
                RecoveryHint::Reconnect,
                "The connector and server versions or capabilities do not match; reconnect with matching versions.",
            ),
            TaxonomyCode::Unavailable => (
                true,
                RecoveryHint::Reconnect,
                "The required connector is not available; reconnect it and try again.",
            ),
            TaxonomyCode::InvalidRequest => (
                false,
                RecoveryHint::None,
                "The request itself is invalid; fix the parameters instead of retrying.",
            ),
            TaxonomyCode::Unknown => (
                false,
                RecoveryHint::None,
                "The failure is unclassified; treat it as non-retriable without new information.",
            ),
        };
        Taxonomy {
            code,
            retriable,
            recovery_hint,
            prose,
        }
    }
}

/// Classifies a legacy error code string into its taxonomy entry.
///
/// Every code emitted by this codebase maps explicitly or via one of the
/// suffix families below; anything else is `unknown` and non-retriable.
pub fn classify(legacy_code: &str) -> Taxonomy {
    let code = match legacy_code {
        "STALE_SNAPSHOT"
        | "STALE_SCREENSHOT"
        | "COMPUTER_STALE_FRAME"
        | "COMPUTER_STALE_POINTER"
        | "NO_COMPUTER_FRAME"
        | "NO_BROWSER_OBSERVATION" => TaxonomyCode::StaleSnapshot,
        "STALE_REF" | "COMPUTER_STALE_ELEMENT" => TaxonomyCode::StaleRef,
        "TARGET_CHANGED" | "TARGET_MISSING" | "COMPUTER_NO_WINDOW" => TaxonomyCode::TargetChanged,
        "BAD_COORDINATES" | "TARGET_OUT_OF_VIEWPORT" => TaxonomyCode::OutOfBounds,
        "ELEMENT_DISABLED" | "POINTER_NOT_ARRIVED" => TaxonomyCode::NotInteractable,
        "TARGET_OCCLUDED" | "CONTROL_UI_OCCLUSION" => TaxonomyCode::Obscured,
        "DOCUMENT_CHANGED" | "NAVIGATION_PENDING" => TaxonomyCode::DocumentChanged,
        "CONTROL_REQUIRED"
        | "CONTROL_REVOKED"
        | "CONTROL_CANCELED"
        | "CONTROL_CLEANUP_PENDING"
        | "CONTROL_OWNER_MISMATCH"
        | "STALE_CONTROL_SESSION"
        | "STALE_CONTROL_TURN"
        | "STALE_MOVE_SEQUENCE"
        | "INPUT_RELEASE_FAILED" => TaxonomyCode::LeaseLost,
        "HUMAN_CONTROL_PAUSED"
        | "HUMAN_PAUSE_STATE_INVALID"
        | "HUMAN_PAUSE_PERSIST_FAILED"
        | "TRUSTED_POPUP_REQUIRED"
        | "COMPUTER_PERMISSION_REQUIRED"
        | "APPROVAL_REQUIRED" => TaxonomyCode::NeedsUser,
        "SITE_BLOCKED"
        | "FULL_ACCESS_REQUIRED"
        | "DOCUMENT_UNSAFE"
        | "PAGE_UNAVAILABLE"
        | "COMPUTER_BACKGROUND_CONTRACT_VIOLATION"
        | "UNAUTHORIZED"
        | "CSRF_REJECTED"
        | "ORIGIN_REJECTED"
        | "HOST_REJECTED"
        | "LEGACY_WEBSOCKET_CREDENTIAL_REJECTED" => TaxonomyCode::BlockedByPolicy,
        "SENSITIVE_FIELD" | "COMPUTER_SENSITIVE_ELEMENT" => TaxonomyCode::SensitiveField,
        "BLOCKED_BY_DIALOG" => TaxonomyCode::BlockedByDialog,
        "COMMAND_CANCELED" | "COMPUTER_CANCELED" | "COMPUTER_POSTCONDITION_FAILED" => {
            TaxonomyCode::OutcomeUnknown
        }
        // page.waitFor timeouts are a normal result path: never auto-retried,
        // never coached as a transient fault.
        "WAIT_TIMEOUT" => TaxonomyCode::WaitTimeout,
        "CALL_IN_PROGRESS" | "AUTH_BUSY" => TaxonomyCode::Overloaded,
        "EXTENSION_PROTOCOL_MISMATCH"
        | "COMPUTER_PROTOCOL_MISMATCH"
        | "EXTENSION_CAPABILITY_UNAVAILABLE"
        | "COMPUTER_CAPABILITY_UNAVAILABLE"
        | "COMPUTER_INVALID_OBSERVATION" => TaxonomyCode::ProtocolMismatch,
        "COMPUTER_UNSUPPORTED_PLATFORM"
        | "COMPUTER_BACKGROUND_UNAVAILABLE"
        | "COMPUTER_SEMANTIC_UNAVAILABLE"
        | "NO_SCREENSHOT"
        | "NO_COMPUTER_SCREENSHOT" => TaxonomyCode::Unavailable,
        "BAD_REQUEST"
        | "BAD_TAB"
        | "BAD_URL"
        | "BAD_BUTTON"
        | "BAD_CLICK_COUNT"
        | "BAD_KEY"
        | "BAD_MODIFIER"
        | "CALL_ID_REUSED"
        | "BODY_TOO_LARGE"
        | "UNSUPPORTED_MEDIA_TYPE"
        | "NOT_FOUND"
        | "UNKNOWN_COMMAND"
        | "NO_PENDING_DIALOG"
        | "NO_NAVIGATION_ENTRY"
        | "COMPUTER_INVALID_REQUEST"
        | "COMPUTER_UNSUPPORTED_ACTION"
        | "COMPUTER_INVALID_ACTION_RECORD"
        | "INVALID_SANITIZER_STATE" => TaxonomyCode::InvalidRequest,
        code if code.ends_with("_OUTCOME_UNKNOWN") => TaxonomyCode::OutcomeUnknown,
        code if code.ends_with("_TIMEOUT") => TaxonomyCode::Timeout,
        code if code.ends_with("_OVERLOADED") => TaxonomyCode::Overloaded,
        code if code.ends_with("_OFFLINE")
            || code.ends_with("_DISCONNECTED")
            || code.ends_with("_HANDSHAKE_PENDING") =>
        {
            TaxonomyCode::Unavailable
        }
        _ => TaxonomyCode::Unknown,
    };
    Taxonomy::for_code(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every legacy code emitted anywhere in the codebase, paired with its
    /// expected canonical taxonomy code.
    const LEGACY_CODES: &[(&str, TaxonomyCode)] = &[
        // Browser snapshot and target proofs (content.js, background.js).
        ("STALE_SNAPSHOT", TaxonomyCode::StaleSnapshot),
        ("STALE_SCREENSHOT", TaxonomyCode::StaleSnapshot),
        ("STALE_REF", TaxonomyCode::StaleRef),
        ("TARGET_CHANGED", TaxonomyCode::TargetChanged),
        ("TARGET_MISSING", TaxonomyCode::TargetChanged),
        ("TARGET_OCCLUDED", TaxonomyCode::Obscured),
        ("CONTROL_UI_OCCLUSION", TaxonomyCode::Obscured),
        ("TARGET_OUT_OF_VIEWPORT", TaxonomyCode::OutOfBounds),
        ("BAD_COORDINATES", TaxonomyCode::OutOfBounds),
        ("ELEMENT_DISABLED", TaxonomyCode::NotInteractable),
        ("POINTER_NOT_ARRIVED", TaxonomyCode::NotInteractable),
        ("DOCUMENT_CHANGED", TaxonomyCode::DocumentChanged),
        ("NAVIGATION_PENDING", TaxonomyCode::DocumentChanged),
        // Browser control lease (background.js, content.js).
        ("CONTROL_REQUIRED", TaxonomyCode::LeaseLost),
        ("CONTROL_REVOKED", TaxonomyCode::LeaseLost),
        ("CONTROL_CANCELED", TaxonomyCode::LeaseLost),
        ("CONTROL_CLEANUP_PENDING", TaxonomyCode::LeaseLost),
        ("CONTROL_OWNER_MISMATCH", TaxonomyCode::LeaseLost),
        ("STALE_CONTROL_SESSION", TaxonomyCode::LeaseLost),
        ("STALE_CONTROL_TURN", TaxonomyCode::LeaseLost),
        ("STALE_MOVE_SEQUENCE", TaxonomyCode::LeaseLost),
        ("INPUT_RELEASE_FAILED", TaxonomyCode::LeaseLost),
        // Human authority and policy boundaries.
        ("HUMAN_CONTROL_PAUSED", TaxonomyCode::NeedsUser),
        ("HUMAN_PAUSE_STATE_INVALID", TaxonomyCode::NeedsUser),
        ("HUMAN_PAUSE_PERSIST_FAILED", TaxonomyCode::NeedsUser),
        ("TRUSTED_POPUP_REQUIRED", TaxonomyCode::NeedsUser),
        ("COMPUTER_PERMISSION_REQUIRED", TaxonomyCode::NeedsUser),
        // A Safe-mode approval cannot be queued inside page.batch, so the
        // failed sub-action hands the risky step back to the human.
        ("APPROVAL_REQUIRED", TaxonomyCode::NeedsUser),
        ("SITE_BLOCKED", TaxonomyCode::BlockedByPolicy),
        ("FULL_ACCESS_REQUIRED", TaxonomyCode::BlockedByPolicy),
        ("DOCUMENT_UNSAFE", TaxonomyCode::BlockedByPolicy),
        ("PAGE_UNAVAILABLE", TaxonomyCode::BlockedByPolicy),
        (
            "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
            TaxonomyCode::BlockedByPolicy,
        ),
        ("UNAUTHORIZED", TaxonomyCode::BlockedByPolicy),
        ("CSRF_REJECTED", TaxonomyCode::BlockedByPolicy),
        ("ORIGIN_REJECTED", TaxonomyCode::BlockedByPolicy),
        ("HOST_REJECTED", TaxonomyCode::BlockedByPolicy),
        (
            "LEGACY_WEBSOCKET_CREDENTIAL_REJECTED",
            TaxonomyCode::BlockedByPolicy,
        ),
        ("SENSITIVE_FIELD", TaxonomyCode::SensitiveField),
        ("COMPUTER_SENSITIVE_ELEMENT", TaxonomyCode::SensitiveField),
        // The pending-dialog gate: mutations fail fast while a JavaScript
        // dialog blocks the controlled page (resolved via page.handleDialog).
        ("BLOCKED_BY_DIALOG", TaxonomyCode::BlockedByDialog),
        // Unknown-outcome boundaries (hub, background.js, computer helper).
        ("COMMAND_OUTCOME_UNKNOWN", TaxonomyCode::OutcomeUnknown),
        ("CDP_OUTCOME_UNKNOWN", TaxonomyCode::OutcomeUnknown),
        ("ACTION_OUTCOME_UNKNOWN", TaxonomyCode::OutcomeUnknown),
        ("COMPUTER_OUTCOME_UNKNOWN", TaxonomyCode::OutcomeUnknown),
        (
            "DEBUGGER_ATTACH_OUTCOME_UNKNOWN",
            TaxonomyCode::OutcomeUnknown,
        ),
        ("COMMAND_CANCELED", TaxonomyCode::OutcomeUnknown),
        ("COMPUTER_CANCELED", TaxonomyCode::OutcomeUnknown),
        (
            "COMPUTER_POSTCONDITION_FAILED",
            TaxonomyCode::OutcomeUnknown,
        ),
        // Condition waits that ran out of time (page.waitFor): a normal
        // result path, never auto-retried.
        ("WAIT_TIMEOUT", TaxonomyCode::WaitTimeout),
        // Timeouts.
        ("COMMAND_TIMEOUT", TaxonomyCode::Timeout),
        ("CONTENT_TIMEOUT", TaxonomyCode::Timeout),
        ("DEBUGGER_TIMEOUT", TaxonomyCode::Timeout),
        ("DEBUGGER_ATTACH_TIMEOUT", TaxonomyCode::Timeout),
        ("DEBUGGER_DETACH_TIMEOUT", TaxonomyCode::Timeout),
        ("DEBUGGER_RECOVERY_TIMEOUT", TaxonomyCode::Timeout),
        ("TAB_RECOVERY_TIMEOUT", TaxonomyCode::Timeout),
        ("POINTER_PRESENTATION_TIMEOUT", TaxonomyCode::Timeout),
        // Load shedding.
        ("EXTENSION_OVERLOADED", TaxonomyCode::Overloaded),
        ("COMPUTER_OVERLOADED", TaxonomyCode::Overloaded),
        ("AUTH_BUSY", TaxonomyCode::Overloaded),
        ("CALL_IN_PROGRESS", TaxonomyCode::Overloaded),
        // Protocol and capability mismatches.
        (
            "EXTENSION_PROTOCOL_MISMATCH",
            TaxonomyCode::ProtocolMismatch,
        ),
        ("COMPUTER_PROTOCOL_MISMATCH", TaxonomyCode::ProtocolMismatch),
        (
            "EXTENSION_CAPABILITY_UNAVAILABLE",
            TaxonomyCode::ProtocolMismatch,
        ),
        (
            "COMPUTER_CAPABILITY_UNAVAILABLE",
            TaxonomyCode::ProtocolMismatch,
        ),
        (
            "COMPUTER_INVALID_OBSERVATION",
            TaxonomyCode::ProtocolMismatch,
        ),
        // Connector availability.
        ("EXTENSION_OFFLINE", TaxonomyCode::Unavailable),
        ("EXTENSION_DISCONNECTED", TaxonomyCode::Unavailable),
        ("EXTENSION_HANDSHAKE_PENDING", TaxonomyCode::Unavailable),
        ("COMPUTER_OFFLINE", TaxonomyCode::Unavailable),
        ("COMPUTER_DISCONNECTED", TaxonomyCode::Unavailable),
        ("COMPUTER_HANDSHAKE_PENDING", TaxonomyCode::Unavailable),
        ("COMPUTER_UNSUPPORTED_PLATFORM", TaxonomyCode::Unavailable),
        ("COMPUTER_BACKGROUND_UNAVAILABLE", TaxonomyCode::Unavailable),
        ("COMPUTER_SEMANTIC_UNAVAILABLE", TaxonomyCode::Unavailable),
        ("NO_SCREENSHOT", TaxonomyCode::Unavailable),
        ("NO_COMPUTER_SCREENSHOT", TaxonomyCode::Unavailable),
        // Stale server-side observation prerequisites (coordinate contract).
        ("COMPUTER_STALE_FRAME", TaxonomyCode::StaleSnapshot),
        ("COMPUTER_STALE_POINTER", TaxonomyCode::StaleSnapshot),
        ("NO_COMPUTER_FRAME", TaxonomyCode::StaleSnapshot),
        ("NO_BROWSER_OBSERVATION", TaxonomyCode::StaleSnapshot),
        ("COMPUTER_STALE_ELEMENT", TaxonomyCode::StaleRef),
        ("COMPUTER_NO_WINDOW", TaxonomyCode::TargetChanged),
        // Invalid requests.
        ("BAD_REQUEST", TaxonomyCode::InvalidRequest),
        ("BAD_TAB", TaxonomyCode::InvalidRequest),
        ("BAD_URL", TaxonomyCode::InvalidRequest),
        ("BAD_BUTTON", TaxonomyCode::InvalidRequest),
        ("BAD_CLICK_COUNT", TaxonomyCode::InvalidRequest),
        ("BAD_KEY", TaxonomyCode::InvalidRequest),
        ("BAD_MODIFIER", TaxonomyCode::InvalidRequest),
        // A callId reused for a different command: the request itself is
        // wrong, so the fix is a fresh callId, never a retry.
        ("CALL_ID_REUSED", TaxonomyCode::InvalidRequest),
        ("BODY_TOO_LARGE", TaxonomyCode::InvalidRequest),
        ("UNSUPPORTED_MEDIA_TYPE", TaxonomyCode::InvalidRequest),
        ("NOT_FOUND", TaxonomyCode::InvalidRequest),
        ("UNKNOWN_COMMAND", TaxonomyCode::InvalidRequest),
        ("NO_PENDING_DIALOG", TaxonomyCode::InvalidRequest),
        ("NO_NAVIGATION_ENTRY", TaxonomyCode::InvalidRequest),
        ("COMPUTER_INVALID_REQUEST", TaxonomyCode::InvalidRequest),
        ("COMPUTER_UNSUPPORTED_ACTION", TaxonomyCode::InvalidRequest),
        (
            "COMPUTER_INVALID_ACTION_RECORD",
            TaxonomyCode::InvalidRequest,
        ),
        ("INVALID_SANITIZER_STATE", TaxonomyCode::InvalidRequest),
        // Failure classes with no better information than the message.
        ("COMMAND_FAILED", TaxonomyCode::Unknown),
        ("CONTENT_COMMAND_FAILED", TaxonomyCode::Unknown),
        ("EVALUATION_FAILED", TaxonomyCode::Unknown),
        ("EXTENSION_ERROR", TaxonomyCode::Unknown),
        ("COMPUTER_ERROR", TaxonomyCode::Unknown),
        ("COMPUTER_HELPER_FAILED", TaxonomyCode::Unknown),
        ("COMPUTER_INPUT_FAILED", TaxonomyCode::Unknown),
        ("COMPUTER_CAPTURE_FAILED", TaxonomyCode::Unknown),
        ("COMPUTER_SEMANTIC_ACTION_FAILED", TaxonomyCode::Unknown),
        ("SCREENSHOT_FAILED", TaxonomyCode::Unknown),
        ("CONTROL_UI_RENDER_FAILED", TaxonomyCode::Unknown),
    ];

    #[test]
    fn classifies_every_legacy_code_into_its_expected_entry() {
        for (legacy, expected) in LEGACY_CODES {
            let taxonomy = classify(legacy);
            assert_eq!(
                taxonomy.code,
                *expected,
                "{legacy} classified as {} instead of {}",
                taxonomy.code.as_str(),
                expected.as_str()
            );
            assert_eq!(taxonomy, Taxonomy::for_code(*expected));
        }
    }

    #[test]
    fn unmapped_codes_are_unknown_and_never_retriable() {
        for garbage in ["", "SOMETHING_NEW", "stale_snapshot", "STALE", "X"] {
            let taxonomy = classify(garbage);
            assert_eq!(taxonomy.code, TaxonomyCode::Unknown);
            assert!(!taxonomy.retriable);
            assert_eq!(taxonomy.recovery_hint, RecoveryHint::None);
        }
    }

    #[test]
    fn every_canonical_code_has_a_distinct_name_and_a_coaching_sentence() {
        let mut names = Vec::new();
        for code in TaxonomyCode::ALL {
            let taxonomy = Taxonomy::for_code(code);
            assert_eq!(taxonomy.code, code);
            assert!(
                taxonomy.prose.ends_with('.') && taxonomy.prose.len() > 20,
                "{} needs one full coaching sentence",
                code.as_str()
            );
            assert!(
                !names.contains(&code.as_str()),
                "duplicate taxonomy name {}",
                code.as_str()
            );
            names.push(code.as_str());
        }
        assert_eq!(names.len(), TaxonomyCode::ALL.len());
    }

    #[test]
    fn recovery_hints_stay_within_the_published_vocabulary() {
        let allowed = [
            "reobserve",
            "wait",
            "resume",
            "handback",
            "reconnect",
            "none",
        ];
        for code in TaxonomyCode::ALL {
            let taxonomy = Taxonomy::for_code(code);
            assert!(allowed.contains(&taxonomy.recovery_hint.as_str()));
        }
    }
}
