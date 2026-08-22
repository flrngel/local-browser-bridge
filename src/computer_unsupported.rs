//! Fail-closed computer-helper surface for unsupported build targets.

use serde_json::{Value, json};

pub use crate::computer_protocol::{
    COMPUTER_HELPER_ORIGIN, COMPUTER_METHODS, COMPUTER_NATIVE_SHARE_CAPABILITY,
    COMPUTER_SHARE_ACK_CAPABILITY, CommandCancellation, ComputerError, ShareFrameAck, ShareMailbox,
    command_parts, result_envelope,
};

pub const NATIVE_COMPUTER_SUPPORTED: bool = false;

#[derive(Default)]
pub struct ComputerController;

impl ComputerController {
    pub fn new() -> Self {
        Self
    }

    /// Unsupported hosts never start a native capture whose shutdown can
    /// fail, so there is no process-fatal capture error to drain.
    #[doc(hidden)]
    pub fn take_fatal_capture_stop_error() -> Option<ComputerError> {
        None
    }

    pub fn has_active_share(&self) -> bool {
        false
    }

    pub fn hello(&mut self) -> Value {
        json!({
            "type": "hello",
            "version": crate::VERSION,
            "platform": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "backend": "unsupported-host",
            "sessionMode": "unavailable",
            "inputReady": false,
            "semanticReady": false,
            "capabilities": [],
            "windows": [],
            "share": { "active": false },
        })
    }

    pub fn execute(&mut self, method: &str, params: &Value) -> Result<Value, ComputerError> {
        self.execute_cancellable(method, params, &CommandCancellation::new())
    }

    pub fn execute_cancellable(
        &mut self,
        method: &str,
        _params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        cancellation.check("unsupported-host command dispatch")?;
        if !COMPUTER_METHODS.contains(&method) {
            return Err(ComputerError::new(
                "COMPUTER_UNSUPPORTED_ACTION",
                format!("Unsupported computer action: {method}"),
            ));
        }
        Err(unsupported_host())
    }

    pub fn request_permissions(&mut self) -> Value {
        json!({
            "platform": std::env::consts::OS,
            "screenCaptureReady": false,
            "inputReady": false,
            "semanticReady": false,
            "windowCount": 0,
            "sessionMode": "unavailable",
            "error": {
                "code": "COMPUTER_UNSUPPORTED_PLATFORM",
                "message": unsupported_host().message,
            },
        })
    }

    pub fn benchmark(&mut self, _iterations: usize) -> Result<Value, ComputerError> {
        Err(unsupported_host())
    }

    pub fn reset_transport_session(&mut self) {}

    pub fn set_share_ack_pacing(&mut self, _enabled: bool) {}

    pub fn revoke_command_authority(&mut self) {}

    pub fn pump_share_capture(&mut self) -> Option<ComputerError> {
        None
    }

    pub fn take_share_emission(&mut self) -> Option<(u64, Value)> {
        None
    }

    pub fn acknowledge_share_frame(&mut self, _share_id: &str, _sequence: u64) -> bool {
        false
    }
}

fn unsupported_host() -> ComputerError {
    ComputerError::new(
        "COMPUTER_UNSUPPORTED_PLATFORM",
        "Native computer control is available only on macOS and Windows",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_controller_fails_closed_without_dispatch() {
        let cancellation = CommandCancellation::new();
        let mut controller = ComputerController::new();
        assert!(!controller.has_active_share());
        assert!(ComputerController::take_fatal_capture_stop_error().is_none());
        let error = controller
            .execute_cancellable("computer.status", &json!({}), &cancellation)
            .unwrap_err();
        assert_eq!(error.code, "COMPUTER_UNSUPPORTED_PLATFORM");
        assert!(!cancellation.was_dispatched());
    }
}
