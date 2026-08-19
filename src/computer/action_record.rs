use std::time::Instant;

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::{ComputerError, InvariantReport};

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
    dispatched: Option<Instant>,
}

impl ActionTimer {
    pub(crate) fn start() -> Self {
        Self {
            action_id: Uuid::new_v4().to_string(),
            started: Instant::now(),
            resolved: None,
            dispatched: None,
        }
    }

    pub(crate) fn resolved(&mut self) {
        self.resolved = Some(Instant::now());
    }

    pub(crate) fn dispatched(&mut self) {
        self.dispatched = Some(Instant::now());
    }

    pub(crate) fn finish(
        self,
        effect: ActionEffect,
        evidence: Vec<ActionEvidence>,
    ) -> Result<ActionRecord, ComputerError> {
        let finished = Instant::now();
        let resolved = self.resolved.unwrap_or(self.started);
        let dispatched = self.dispatched.unwrap_or(resolved);
        ActionRecord::new(
            self.action_id,
            effect,
            evidence,
            ActionTimings {
                resolve_ms: milliseconds(resolved.duration_since(self.started)),
                dispatch_ms: milliseconds(dispatched.duration_since(resolved)),
                verify_ms: milliseconds(finished.duration_since(dispatched)),
                total_ms: milliseconds(finished.duration_since(self.started)),
            },
        )
    }
}

pub(crate) fn invariant_evidence(report: &InvariantReport) -> Vec<ActionEvidence> {
    [
        (
            "foregroundUnchanged",
            report.foreground_unchanged,
            "The foreground application was unchanged across background delivery",
        ),
        (
            "userFocusUnchanged",
            report.user_focus_unchanged,
            "Keyboard focus was unchanged across background delivery",
        ),
        (
            "hardwareCursorUnchanged",
            report.cursor_unchanged,
            "The hardware cursor was unchanged across background delivery",
        ),
        (
            "desktopSpaceUnchanged",
            report.space_unchanged,
            "The active desktop or Space was unchanged across background delivery",
        ),
    ]
    .into_iter()
    .map(|(claim, observed, detail)| ActionEvidence::delivery_invariant(claim, observed, detail))
    .collect()
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
    let effect = if confirms_effect {
        ActionEffect::Confirmed
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
    !matches!(
        claim,
        "no-observable-change"
            | "value-not-confirmed"
            | "postcondition-not-reported"
            | "effect-not-observed"
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
}
