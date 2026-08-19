//! Target-neutral computer-helper protocol and cancellation primitives.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Map, Value, json};
use thiserror::Error;

pub const COMPUTER_HELPER_ORIGIN: &str = "lbb-computer-helper://local";
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

#[derive(Clone, Default)]
pub struct CommandCancellation {
    canceled: Arc<AtomicBool>,
    dispatched: Arc<AtomicBool>,
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

    #[cfg(any(target_os = "macos", target_os = "windows", test))]
    pub(crate) fn begin_side_effect(&self, boundary: &str) -> Result<(), ComputerError> {
        self.check(boundary)?;
        self.dispatched.store(true, Ordering::Release);
        self.check(boundary)
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
}
