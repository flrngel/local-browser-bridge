use std::time::Instant;

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::{CommandCancellation, ComputerError, InvariantReport};

/// The strongest conclusion that can be drawn about a requested mutation.
///
/// A successful platform call is only delivery evidence. `Confirmed` is
/// reserved for actions whose target-side postcondition was observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum ActionEffect {
    Confirmed,
    Partial,
    Unverifiable,
    SuspectedNoop,
    Refused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum EvidenceKind {
    DeliveryInvariant,
    DispatchAcknowledgement,
    Postcondition,
    DiagnosticObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionEvidence {
    pub kind: EvidenceKind,
    pub claim: String,
    pub observed: bool,
    pub supports_confirmation: bool,
    pub detail: String,
}

impl ActionEvidence {
    pub(crate) fn delivery_invariant(
        claim: impl Into<String>,
        observed: bool,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind: EvidenceKind::DeliveryInvariant,
            claim: claim.into(),
            observed,
            supports_confirmation: false,
            detail: detail.into(),
        }
    }

    pub(crate) fn postcondition(
        claim: impl Into<String>,
        observed: bool,
        detail: impl Into<String>,
    ) -> Self {
        let claim = claim.into();
        let supports_confirmation = observed && is_confirming_postcondition(&claim);
        Self {
            kind: EvidenceKind::Postcondition,
            claim,
            observed,
            supports_confirmation,
            detail: detail.into(),
        }
    }

    pub(crate) fn dispatch_acknowledgement(
        claim: impl Into<String>,
        observed: bool,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind: EvidenceKind::DispatchAcknowledgement,
            claim: claim.into(),
            observed,
            supports_confirmation: false,
            detail: detail.into(),
        }
    }

    pub(crate) fn diagnostic_observation(
        claim: impl Into<String>,
        observed: bool,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind: EvidenceKind::DiagnosticObservation,
            claim: claim.into(),
            observed,
            supports_confirmation: false,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionTimings {
    pub resolve_ms: f64,
    pub dispatch_ms: f64,
    pub verify_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionRecord {
    pub action_id: String,
    pub effect: ActionEffect,
    pub evidence: Vec<ActionEvidence>,
    pub timings: ActionTimings,
}

impl ActionRecord {
    pub(crate) fn new(
        action_id: String,
        effect: ActionEffect,
        evidence: Vec<ActionEvidence>,
        timings: ActionTimings,
    ) -> Result<Self, ComputerError> {
        let record = Self {
            action_id,
            effect,
            evidence,
            timings,
        };
        record.validate()?;
        Ok(record)
    }

    fn validate(&self) -> Result<(), ComputerError> {
        let has_confirming_postcondition = self.evidence.iter().any(|item| {
            item.kind == EvidenceKind::Postcondition && item.observed && item.supports_confirmation
        });
        if self.effect == ActionEffect::Confirmed && !has_confirming_postcondition {
            return Err(ComputerError::new(
                "COMPUTER_INVALID_ACTION_RECORD",
                "Confirmed requires at least one observed postcondition evidence item",
            ));
        }
        if self.evidence.iter().any(|item| {
            item.supports_confirmation
                && (item.kind != EvidenceKind::Postcondition
                    || !is_confirming_postcondition(&item.claim))
        }) {
            return Err(ComputerError::new(
                "COMPUTER_INVALID_ACTION_RECORD",
                "Delivery evidence cannot support effect confirmation",
            ));
        }
        Ok(())
    }

    pub(crate) fn insert_into(self, result: &mut Value) -> Result<(), ComputerError> {
        let target = result.as_object_mut().ok_or_else(|| {
            ComputerError::new(
                "COMPUTER_INVALID_ACTION_RECORD",
                "A computer action result must be a JSON object",
            )
        })?;
        let serialized = serde_json::to_value(self).map_err(|error| {
            ComputerError::new("COMPUTER_INVALID_ACTION_RECORD", error.to_string())
        })?;
        let record = serialized.as_object().ok_or_else(|| {
            ComputerError::new(
                "COMPUTER_INVALID_ACTION_RECORD",
                "The action record did not serialize as a JSON object",
            )
        })?;
        target.extend(record.clone());
        Ok(())
    }
}

pub(crate) struct ActionTimer {
    action_id: String,
    started: Instant,
    resolved: Option<Instant>,
    cancellation: CommandCancellation,
}

impl ActionTimer {
    pub(crate) fn start(cancellation: &CommandCancellation) -> Self {
        Self {
            action_id: Uuid::new_v4().to_string(),
            started: Instant::now(),
            resolved: None,
            cancellation: cancellation.clone(),
        }
    }

    pub(crate) fn resolved(&mut self) {
        self.resolved = Some(Instant::now());
    }

    pub(crate) fn finish(
        self,
        effect: ActionEffect,
        evidence: Vec<ActionEvidence>,
    ) -> Result<ActionRecord, ComputerError> {
        let finished = Instant::now();
        let resolved = self.resolved.unwrap_or(self.started);
        let verification_started = self
            .cancellation
            .verification_started_at()
            .filter(|instant| *instant >= resolved && *instant <= finished)
            .unwrap_or(resolved);
        ActionRecord::new(
            self.action_id,
            effect,
            evidence,
            ActionTimings {
                resolve_ms: milliseconds(resolved.duration_since(self.started)),
                dispatch_ms: milliseconds(verification_started.duration_since(resolved)),
                verify_ms: milliseconds(finished.duration_since(verification_started)),
                total_ms: milliseconds(finished.duration_since(self.started)),
            },
        )
    }
}

pub(crate) fn invariant_evidence(report: &InvariantReport) -> Vec<ActionEvidence> {
    let mut evidence = [
        (
            "foregroundUnchanged",
            report.foreground_unchanged,
            "The foreground application was unchanged across background delivery",
        ),
        (
            "userFocusUnchanged",
            report.user_focus_unchanged,
            "The platform-specific front-window or focused-window oracle was unchanged across background delivery",
        ),
        (
            "inputRouteTargetBound",
            report.input_delivery.is_target_bound(),
            "The runtime delivery route remained exact-target-bound and did not use the shared input seat, global HID input, or a hardware-cursor mutation API",
        ),
        (
            "hardwareCursorPreservedByHelper",
            report.hardware_cursor_preserved_by_helper,
            "The sealed exact-target route and corroborated shared-pointer boundary supported that this helper action requested no global pointer operation; release artifacts are separately checked for forbidden APIs",
        ),
        (
            "sharedPointerBoundaryCorroborated",
            report.shared_pointer_boundary_corroborated,
            "The cursor stayed at the sampled position, or a position delta coincided with platform pointer-monitor activity; this is boundary corroboration only and does not prove physical-device provenance or exclude a simultaneous eventless cursor warp",
        ),
        (
            "desktopSpaceUnchanged",
            report.space_unchanged,
            "The active desktop or Space was unchanged across background delivery",
        ),
    ]
    .into_iter()
    .map(|(claim, observed, detail)| ActionEvidence::delivery_invariant(claim, observed, detail))
    .collect::<Vec<_>>();
    evidence.push(ActionEvidence::diagnostic_observation(
        "cursorPositionUnchanged",
        report.cursor_position_unchanged,
        "The global hardware-cursor sample matched before and after delivery; concurrent shared-seat motion can change this diagnostic without identifying the action source",
    ));
    evidence.push(ActionEvidence::diagnostic_observation(
        "sharedPointerActivityObserved",
        report.shared_pointer_activity_observed,
        "A platform pointer monitor observed shared-session pointer activity across delivery; this is contamination evidence, not physical-device provenance or target-effect confirmation",
    ));
    evidence.push(ActionEvidence::diagnostic_observation(
        "hidSystemPointerActivityObserved",
        report.hid_system_pointer_activity_observed,
        "The platform's HID-system source recorded pointer activity across delivery; this can include physical, virtual-HID, or remote-session input and is never target-effect confirmation",
    ));
    evidence.push(ActionEvidence::diagnostic_observation(
        "rawInputPointerActivityObserved",
        report.raw_input_pointer_activity_observed,
        "The Windows Raw Input monitor observed mouse-device activity without retaining device identity, coordinates, or input contents",
    ));
    evidence.push(ActionEvidence::diagnostic_observation(
        "injectedPointerActivityObserved",
        report.injected_pointer_activity_observed,
        "The Windows low-level pointer monitor observed an injected-event flag; this cannot identify the injecting process or target effect",
    ));
    evidence.push(ActionEvidence::diagnostic_observation(
        "pointerActivityMonitorHealthy",
        report.pointer_activity_monitor_healthy,
        "The platform pointer monitor initialized and its sampled counter or epoch boundary remained readable; this does not prove continuous capture or identify an input source",
    ));
    evidence
}

/// Converts the semantic backend's effect report into conservative evidence.
/// The backend already performed the read-back; this layer merely records the
/// claim and prevents a delivery acknowledgement from becoming confirmation.
pub(crate) fn semantic_evidence(effect: &Value) -> (ActionEffect, Vec<ActionEvidence>) {
    let delivered = effect
        .get("delivered")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let observed = effect
        .get("effectObserved")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let postcondition = effect
        .get("postcondition")
        .and_then(Value::as_str)
        .unwrap_or("postcondition-not-reported");

    let mut evidence = vec![ActionEvidence::dispatch_acknowledgement(
        "semanticDispatchReturned",
        delivered,
        "The semantic backend returned after dispatch; this is not target-effect proof",
    )];
    let has_postcondition = postcondition != "postcondition-not-reported";
    if has_postcondition {
        evidence.push(ActionEvidence::postcondition(
            postcondition,
            observed,
            if observed {
                "The semantic backend observed this target-side postcondition"
            } else {
                "The semantic backend checked this postcondition but did not observe a change"
            },
        ));
    }

    let confirms_effect = evidence.iter().any(|item| item.supports_confirmation);
    let has_nonconfirming_observation = observed
        && !matches!(
            postcondition,
            "no-observable-change"
                | "value-not-confirmed"
                | "postcondition-not-reported"
                | "effect-not-observed"
        );
    let effect = if confirms_effect {
        ActionEffect::Confirmed
    } else if delivered && has_nonconfirming_observation {
        ActionEffect::Partial
    } else if delivered && has_postcondition {
        ActionEffect::SuspectedNoop
    } else if delivered {
        ActionEffect::Partial
    } else {
        ActionEffect::Refused
    };
    (effect, evidence)
}

fn milliseconds(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn is_confirming_postcondition(claim: &str) -> bool {
    matches!(
        claim,
        "value-confirmed"
            | "masked-length-confirmed"
            | "toggle-state-changed"
            | "selection-state-changed"
            | "expand-collapse-state-changed"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timings() -> ActionTimings {
        ActionTimings {
            resolve_ms: 1.0,
            dispatch_ms: 2.0,
            verify_ms: 3.0,
            total_ms: 6.0,
        }
    }

    #[test]
    fn confirmed_requires_observed_postcondition_evidence() {
        let delivery = ActionEvidence::delivery_invariant(
            "backendReturned",
            true,
            "Transport acknowledgement only",
        );
        let error = ActionRecord::new(
            "action-1".to_owned(),
            ActionEffect::Confirmed,
            vec![delivery],
            timings(),
        )
        .unwrap_err();
        assert_eq!(error.code, "COMPUTER_INVALID_ACTION_RECORD");
    }

    #[test]
    fn observed_postcondition_can_confirm_an_effect() {
        let record = ActionRecord::new(
            "action-1".to_owned(),
            ActionEffect::Confirmed,
            vec![ActionEvidence::postcondition(
                "value-confirmed",
                true,
                "Read-back matched",
            )],
            timings(),
        )
        .unwrap();
        assert_eq!(record.effect, ActionEffect::Confirmed);
    }

    #[test]
    fn semantic_delivery_ack_without_postcondition_is_partial() {
        let (effect, evidence) = semantic_evidence(&serde_json::json!({ "delivered": true }));
        assert_eq!(effect, ActionEffect::Partial);
        assert!(evidence.iter().all(|item| !item.supports_confirmation));
    }

    #[test]
    fn failed_semantic_postcondition_is_suspected_noop() {
        let (effect, evidence) = semantic_evidence(&serde_json::json!({
            "delivered": true,
            "effectObserved": false,
            "postcondition": "no-observable-change"
        }));
        assert_eq!(effect, ActionEffect::SuspectedNoop);
        assert!(
            evidence
                .iter()
                .any(|item| { item.kind == EvidenceKind::Postcondition && !item.observed })
        );
    }

    #[test]
    fn a_negative_postcondition_name_cannot_confirm_even_if_misreported_as_observed() {
        let (effect, evidence) = semantic_evidence(&serde_json::json!({
            "delivered": true,
            "effectObserved": true,
            "postcondition": "no-observable-change"
        }));
        assert_eq!(effect, ActionEffect::SuspectedNoop);
        assert!(evidence.iter().all(|item| !item.supports_confirmation));
    }

    #[test]
    fn an_unknown_positive_sounding_claim_cannot_confirm_an_effect() {
        let (effect, evidence) = semantic_evidence(&serde_json::json!({
            "delivered": true,
            "effectObserved": true,
            "postcondition": "window-state-changed"
        }));
        assert_eq!(effect, ActionEffect::Partial);
        assert!(evidence.iter().all(|item| !item.supports_confirmation));
    }

    #[test]
    fn semantic_confirmation_keeps_platform_invariants_as_nonconfirming_evidence() {
        let (effect, mut evidence) = semantic_evidence(&serde_json::json!({
            "delivered": true,
            "effectObserved": true,
            "postcondition": "value-confirmed"
        }));
        let invariants = InvariantReport {
            foreground_unchanged: true,
            user_focus_unchanged: true,
            cursor_position_unchanged: true,
            shared_pointer_activity_observed: false,
            hid_system_pointer_activity_observed: false,
            raw_input_pointer_activity_observed: false,
            injected_pointer_activity_observed: false,
            pointer_activity_monitor_healthy: true,
            shared_pointer_boundary_corroborated: true,
            shared_pointer_boundary_state:
                crate::computer::SharedPointerBoundaryState::Corroborated,
            hardware_cursor_preserved_by_helper: true,
            helper_global_pointer_preservation:
                crate::computer::HelperGlobalPointerPreservation::Confirmed,
            shared_pointer_activity_state: crate::computer::SharedPointerActivityState::Quiet,
            space_unchanged: true,
            input_delivery: crate::computer::InputDeliveryProvenance::target_bound_for_test(),
        };
        evidence.extend(invariant_evidence(&invariants));

        let record = ActionRecord::new("action-1".to_owned(), effect, evidence, timings()).unwrap();
        assert_eq!(record.effect, ActionEffect::Confirmed);
        let invariant_evidence = record
            .evidence
            .iter()
            .filter(|item| item.kind == EvidenceKind::DeliveryInvariant)
            .collect::<Vec<_>>();
        assert_eq!(invariant_evidence.len(), 6);
        assert!(
            invariant_evidence
                .iter()
                .all(|item| item.observed && !item.supports_confirmation)
        );
        let focus = invariant_evidence
            .iter()
            .find(|item| item.claim == "userFocusUnchanged")
            .unwrap();
        assert_eq!(
            focus.detail,
            "The platform-specific front-window or focused-window oracle was unchanged across background delivery"
        );
        let diagnostics = record
            .evidence
            .iter()
            .filter(|item| item.kind == EvidenceKind::DiagnosticObservation)
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 6);
        assert!(diagnostics.iter().all(|item| !item.supports_confirmation));
        for claim in [
            "cursorPositionUnchanged",
            "sharedPointerActivityObserved",
            "hidSystemPointerActivityObserved",
            "rawInputPointerActivityObserved",
            "injectedPointerActivityObserved",
            "pointerActivityMonitorHealthy",
        ] {
            assert!(diagnostics.iter().any(|item| item.claim == claim));
        }
    }
}
