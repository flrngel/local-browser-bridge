use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::Message;
use serde_json::{Value, json};
use thiserror::Error;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct HubError {
    pub code: String,
    pub message: String,
}

impl HubError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone)]
pub struct ExtensionHub {
    inner: Arc<HubInner>,
}

struct HubInner {
    connection: Mutex<Option<Connection>>,
    pending: Mutex<HashMap<String, PendingCall>>,
    call_timeout: Duration,
    connector_label: &'static str,
    error_prefix: &'static str,
}

#[derive(Clone)]
struct Connection {
    id: Uuid,
    sender: mpsc::Sender<Message>,
    sequence: Arc<AtomicU64>,
    ready: Arc<AtomicBool>,
}

struct PendingCall {
    connection_id: Uuid,
    sequence: u64,
    method: String,
    sender: oneshot::Sender<Result<Value, HubError>>,
}

impl ExtensionHub {
    pub fn new(call_timeout: Duration) -> Self {
        Self::connector(call_timeout, "Browser extension", "EXTENSION")
    }

    pub fn computer(call_timeout: Duration) -> Self {
        Self::connector(call_timeout, "Computer helper", "COMPUTER")
    }

    fn connector(
        call_timeout: Duration,
        connector_label: &'static str,
        error_prefix: &'static str,
    ) -> Self {
        Self {
            inner: Arc::new(HubInner {
                connection: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                call_timeout,
                connector_label,
                error_prefix,
            }),
        }
    }

    pub fn connected(&self) -> bool {
        self.inner
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|connection| connection.ready.load(Ordering::Acquire))
    }

    pub fn attach(&self) -> (Uuid, mpsc::Receiver<Message>) {
        let id = Uuid::new_v4();
        self.attach_with_id(id)
    }

    pub fn attach_with_id(&self, id: Uuid) -> (Uuid, mpsc::Receiver<Message>) {
        let (sender, receiver) = mpsc::channel(64);
        let previous = self.inner.connection.lock().unwrap().replace(Connection {
            id,
            sender,
            sequence: Arc::new(AtomicU64::new(0)),
            ready: Arc::new(AtomicBool::new(false)),
        });
        if let Some(previous) = previous {
            self.fail_enqueued_for(previous.id, Some(&previous.sender), "connection_replaced");
            let _ = previous.sender.try_send(Message::Close(None));
        }
        (id, receiver)
    }

    pub fn mark_ready(&self, connection_id: Uuid) -> bool {
        let connection = self.inner.connection.lock().unwrap();
        let Some(connection) = connection
            .as_ref()
            .filter(|connection| connection.id == connection_id)
        else {
            return false;
        };
        connection.ready.store(true, Ordering::Release);
        true
    }

    pub fn is_current_ready(&self, connection_id: Uuid) -> bool {
        self.inner
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|connection| {
                connection.id == connection_id && connection.ready.load(Ordering::Acquire)
            })
    }

    pub fn is_current(&self, connection_id: Uuid) -> bool {
        self.inner
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|connection| connection.id == connection_id)
    }

    fn fail_enqueued_for(
        &self,
        connection_id: Uuid,
        sender: Option<&mpsc::Sender<Message>>,
        reason: &str,
    ) {
        let pending = {
            let mut calls = self.inner.pending.lock().unwrap();
            let ids = calls
                .iter()
                .filter_map(|(id, call)| {
                    (call.connection_id == connection_id).then_some(id.clone())
                })
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| calls.remove(&id).map(|call| (id, call)))
                .collect::<Vec<_>>()
        };
        for (id, call) in pending {
            if let Some(sender) = sender {
                let cancel = json!({
                    "id": id,
                    "type": "cancel",
                    "protocolVersion": crate::PROTOCOL_VERSION,
                    "sessionId": connection_id.to_string(),
                    "sequence": call.sequence,
                    "reason": reason,
                });
                let _ = sender.try_send(Message::Text(cancel.to_string().into()));
            }
            let _ = call.sender.send(Err(HubError::new(
                "COMMAND_OUTCOME_UNKNOWN",
                format!(
                    "{} connection ended after the command was enqueued; its outcome is unknown: {}",
                    self.inner.connector_label, call.method
                ),
            )));
        }
    }

    pub fn detach(&self, connection_id: Uuid) -> bool {
        let detached = {
            let mut connection = self.inner.connection.lock().unwrap();
            if connection
                .as_ref()
                .is_some_and(|active| active.id == connection_id)
            {
                connection.take()
            } else {
                None
            }
        };
        self.fail_enqueued_for(
            connection_id,
            detached.as_ref().map(|connection| &connection.sender),
            "connection_disconnected",
        );
        detached.is_some()
    }

    pub fn resolve(&self, connection_id: Uuid, message: &Value) -> bool {
        if message.get("type").and_then(Value::as_str) != Some("result") {
            return false;
        }
        let Some(id) = message.get("id").and_then(Value::as_str) else {
            return false;
        };
        let Some(call) = self.inner.pending.lock().unwrap().remove(id) else {
            return false;
        };
        let expected_session = connection_id.to_string();
        let envelope_valid = call.connection_id == connection_id
            && message.get("protocolVersion").and_then(Value::as_u64)
                == Some(crate::PROTOCOL_VERSION)
            && message.get("sessionId").and_then(Value::as_str) == Some(expected_session.as_str())
            && message.get("sequence").and_then(Value::as_u64) == Some(call.sequence);
        if !envelope_valid {
            if let Some(connection) = self
                .inner
                .connection
                .lock()
                .unwrap()
                .as_ref()
                .filter(|connection| connection.id == connection_id)
            {
                connection.ready.store(false, Ordering::Release);
            }
            let _ = call.sender.send(Err(HubError::new(
                "COMMAND_OUTCOME_UNKNOWN",
                format!(
                    "{} returned a stale or mismatched result after the command was enqueued; its outcome is unknown",
                    self.inner.connector_label,
                ),
            )));
            return true;
        }
        let result = if message.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(message.get("result").cloned().unwrap_or(Value::Null))
        } else {
            let error = message.get("error").unwrap_or(&Value::Null);
            Err(HubError::new(
                error
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| {
                        if self.inner.error_prefix == "COMPUTER" {
                            "COMPUTER_ERROR"
                        } else {
                            "EXTENSION_ERROR"
                        }
                    }),
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown extension error"),
            ))
        };
        let _ = call.sender.send(result);
        false
    }

    pub fn send_to(&self, connection_id: Uuid, message: Value) -> Result<(), HubError> {
        self.send_message_to(connection_id, Message::Text(message.to_string().into()))
    }

    pub fn send_message_to(&self, connection_id: Uuid, message: Message) -> Result<(), HubError> {
        let connection = self
            .inner
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .filter(|connection| connection.id == connection_id)
            .cloned()
            .ok_or_else(|| {
                HubError::new(
                    format!("{}_DISCONNECTED", self.inner.error_prefix),
                    format!("{} disconnected", self.inner.connector_label),
                )
            })?;
        Self::try_send(&self.inner, &connection, message)
    }

    fn try_send(
        inner: &HubInner,
        connection: &Connection,
        message: Message,
    ) -> Result<(), HubError> {
        connection.sender.try_send(message).map_err(|error| {
            let saturated = matches!(error, TrySendError::Full(_));
            HubError::new(
                if saturated {
                    format!("{}_OVERLOADED", inner.error_prefix)
                } else {
                    format!("{}_DISCONNECTED", inner.error_prefix)
                },
                if saturated {
                    format!("{} outbound queue is full", inner.connector_label)
                } else {
                    format!("{} disconnected", inner.connector_label)
                },
            )
        })
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, HubError> {
        self.call_scoped(method, params)
            .await
            .map(|(_, result)| result)
    }

    pub async fn call_scoped(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(Uuid, Value), HubError> {
        let connection = self
            .inner
            .connection
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| {
                HubError::new(
                    format!("{}_OFFLINE", self.inner.error_prefix),
                    format!("{} is not connected", self.inner.connector_label),
                )
            })?;
        if !connection.ready.load(Ordering::Acquire) {
            return Err(HubError::new(
                format!("{}_HANDSHAKE_PENDING", self.inner.error_prefix),
                format!("{} handshake is not complete", self.inner.connector_label),
            ));
        }
        let id = Uuid::new_v4().to_string();
        let sequence = connection.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let (sender, receiver) = oneshot::channel();
        self.inner.pending.lock().unwrap().insert(
            id.clone(),
            PendingCall {
                connection_id: connection.id,
                sequence,
                method: method.to_owned(),
                sender,
            },
        );

        let command = json!({
            "id": id,
            "type": "command",
            "protocolVersion": crate::PROTOCOL_VERSION,
            "sessionId": connection.id.to_string(),
            "sequence": sequence,
            "method": method,
            "params": params,
        });
        if let Err(error) = connection
            .sender
            .try_send(Message::Text(command.to_string().into()))
        {
            self.inner.pending.lock().unwrap().remove(&id);
            let saturated = matches!(error, TrySendError::Full(_));
            return Err(HubError::new(
                if saturated {
                    format!("{}_OVERLOADED", self.inner.error_prefix)
                } else {
                    format!("{}_DISCONNECTED", self.inner.error_prefix)
                },
                if saturated {
                    format!("{} outbound queue is full", self.inner.connector_label)
                } else {
                    format!("{} disconnected", self.inner.connector_label)
                },
            ));
        }

        match tokio::time::timeout(self.inner.call_timeout, receiver).await {
            Ok(Ok(result)) => result.map(|result| (connection.id, result)),
            Ok(Err(_)) => Err(HubError::new(
                "COMMAND_OUTCOME_UNKNOWN",
                format!(
                    "{} connection ended after the command was enqueued; its outcome is unknown: {method}",
                    self.inner.connector_label
                ),
            )),
            Err(_) => {
                self.inner.pending.lock().unwrap().remove(&id);
                let cancel = json!({
                    "id": id,
                    "type": "cancel",
                    "protocolVersion": crate::PROTOCOL_VERSION,
                    "sessionId": connection.id.to_string(),
                    "sequence": sequence,
                    "reason": "command_timeout",
                });
                let _ = connection
                    .sender
                    .try_send(Message::Text(cancel.to_string().into()));
                Err(HubError::new(
                    "COMMAND_OUTCOME_UNKNOWN",
                    format!(
                        "{} command timed out and was canceled; its outcome is unknown: {method}",
                        self.inner.connector_label
                    ),
                ))
            }
        }
    }

    pub fn close(&self) {
        if let Some(connection) = self.inner.connection.lock().unwrap().take() {
            self.fail_enqueued_for(connection.id, Some(&connection.sender), "server_stopped");
            let _ = connection.sender.try_send(Message::Close(None));
        }
        let pending = self
            .inner
            .pending
            .lock()
            .unwrap()
            .drain()
            .map(|(_, call)| call)
            .collect::<Vec<_>>();
        for call in pending {
            let _ = call.sender.send(Err(HubError::new(
                "COMMAND_OUTCOME_UNKNOWN",
                format!(
                    "Bridge server stopped after the command was enqueued; its outcome is unknown: {}",
                    call.method
                ),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn next_json(receiver: &mut mpsc::Receiver<Message>) -> Value {
        let Message::Text(text) = receiver.recv().await.unwrap() else {
            panic!("expected a text command");
        };
        serde_json::from_str(text.as_str()).unwrap()
    }

    #[tokio::test]
    async fn binds_results_to_the_exact_connection_session_and_sequence() {
        let hub = ExtensionHub::new(Duration::from_secs(1));
        let (connection_id, mut receiver) = hub.attach();
        assert!(hub.mark_ready(connection_id));
        let caller = {
            let hub = hub.clone();
            tokio::spawn(async move { hub.call("tabs.list", json!({})).await })
        };
        let command = next_json(&mut receiver).await;
        assert_eq!(command["protocolVersion"], crate::PROTOCOL_VERSION);
        assert_eq!(command["sessionId"], connection_id.to_string());
        assert_eq!(command["sequence"], 1);

        hub.resolve(
            connection_id,
            &json!({
                "id": command["id"],
                "type": "cancel",
                "protocolVersion": crate::PROTOCOL_VERSION,
                "sessionId": connection_id.to_string(),
                "sequence": command["sequence"]
            }),
        );
        assert!(!caller.is_finished());

        hub.resolve(
            connection_id,
            &json!({
                "id": command["id"],
                "type": "result",
                "protocolVersion": crate::PROTOCOL_VERSION,
                "sessionId": connection_id.to_string(),
                "sequence": command["sequence"],
                "ok": true,
                "result": { "accepted": true }
            }),
        );
        assert_eq!(caller.await.unwrap().unwrap()["accepted"], true);
    }

    #[tokio::test]
    async fn quarantines_a_mismatched_result_and_reports_unknown_outcome() {
        let hub = ExtensionHub::new(Duration::from_secs(1));
        let (connection_id, mut receiver) = hub.attach();
        assert!(hub.mark_ready(connection_id));
        let caller = {
            let hub = hub.clone();
            tokio::spawn(async move { hub.call("tabs.list", json!({})).await })
        };
        let command = next_json(&mut receiver).await;
        assert!(hub.resolve(
            connection_id,
            &json!({
                "id": command["id"],
                "type": "result",
                "protocolVersion": crate::PROTOCOL_VERSION,
                "sessionId": connection_id.to_string(),
                "sequence": command["sequence"].as_u64().unwrap() + 1,
                "ok": true,
                "result": {}
            }),
        ));
        let error = caller.await.unwrap().unwrap_err();
        assert_eq!(error.code, "COMMAND_OUTCOME_UNKNOWN");
        assert!(!hub.connected());
        assert_eq!(
            hub.call("tabs.list", json!({})).await.unwrap_err().code,
            "EXTENSION_HANDSHAKE_PENDING"
        );
    }

    #[tokio::test]
    async fn refuses_calls_until_the_current_connection_is_ready() {
        let hub = ExtensionHub::new(Duration::from_secs(1));
        let (first, _first_receiver) = hub.attach();
        assert!(hub.mark_ready(first));
        let (_replacement, _replacement_receiver) = hub.attach();

        let error = hub.call("tabs.list", json!({})).await.unwrap_err();
        assert_eq!(error.code, "EXTENSION_HANDSHAKE_PENDING");
        assert!(!hub.connected());
    }

    #[tokio::test]
    async fn connection_scoped_replies_never_cross_to_a_replacement() {
        let hub = ExtensionHub::new(Duration::from_secs(1));
        let (first, mut first_receiver) = hub.attach();
        hub.send_to(first, json!({ "type": "welcome", "for": "first" }))
            .unwrap();
        assert_eq!(next_json(&mut first_receiver).await["for"], "first");

        let (second, mut second_receiver) = hub.attach();
        assert!(
            hub.send_to(first, json!({ "type": "pong", "for": "first" }))
                .is_err()
        );
        hub.send_to(second, json!({ "type": "welcome", "for": "second" }))
            .unwrap();
        assert_eq!(next_json(&mut second_receiver).await["for"], "second");
    }

    #[tokio::test]
    async fn scoped_call_identifies_the_connection_that_produced_a_resolved_result() {
        let hub = ExtensionHub::new(Duration::from_secs(1));
        let (first, mut first_receiver) = hub.attach();
        assert!(hub.mark_ready(first));
        let caller = {
            let hub = hub.clone();
            tokio::spawn(async move { hub.call_scoped("tabs.list", json!({})).await })
        };
        let command = next_json(&mut first_receiver).await;
        hub.resolve(
            first,
            &json!({
                "id": command["id"],
                "type": "result",
                "protocolVersion": crate::PROTOCOL_VERSION,
                "sessionId": first.to_string(),
                "sequence": command["sequence"],
                "ok": true,
                "result": { "producer": "first" }
            }),
        );

        let (replacement, _replacement_receiver) = hub.attach();
        assert_ne!(replacement, first);
        let (producer, result) = caller.await.unwrap().unwrap();
        assert_eq!(producer, first);
        assert_eq!(result["producer"], "first");
        assert!(!hub.is_current_ready(producer));
    }

    #[tokio::test]
    async fn timeout_emits_a_session_bound_cancel_and_reports_unknown_outcome() {
        let hub = ExtensionHub::new(Duration::from_millis(20));
        let (connection_id, mut receiver) = hub.attach();
        assert!(hub.mark_ready(connection_id));
        let caller = {
            let hub = hub.clone();
            tokio::spawn(async move { hub.call("page.click", json!({})).await })
        };
        let command = next_json(&mut receiver).await;
        let cancel = next_json(&mut receiver).await;
        assert_eq!(cancel["type"], "cancel");
        assert_eq!(cancel["id"], command["id"]);
        assert_eq!(cancel["sequence"], command["sequence"]);
        assert_eq!(cancel["sessionId"], connection_id.to_string());
        assert_eq!(
            caller.await.unwrap().unwrap_err().code,
            "COMMAND_OUTCOME_UNKNOWN"
        );
    }

    #[tokio::test]
    async fn replacing_extension_after_dequeue_cancels_and_reports_unknown_outcome() {
        let hub = ExtensionHub::new(Duration::from_secs(1));
        let (first, mut first_receiver) = hub.attach();
        assert!(hub.mark_ready(first));
        let caller = {
            let hub = hub.clone();
            tokio::spawn(async move { hub.call("page.click", json!({})).await })
        };
        let command = next_json(&mut first_receiver).await;

        let (replacement, _replacement_receiver) = hub.attach();
        assert_ne!(replacement, first);
        let cancel = next_json(&mut first_receiver).await;
        assert_eq!(cancel["type"], "cancel");
        assert_eq!(cancel["id"], command["id"]);
        assert_eq!(cancel["sequence"], command["sequence"]);
        assert_eq!(cancel["reason"], "connection_replaced");
        assert_eq!(
            caller.await.unwrap().unwrap_err().code,
            "COMMAND_OUTCOME_UNKNOWN"
        );
    }

    #[tokio::test]
    async fn disconnecting_helper_after_dequeue_cancels_and_reports_unknown_outcome() {
        let hub = ExtensionHub::computer(Duration::from_secs(1));
        let (connection_id, mut receiver) = hub.attach();
        assert!(hub.mark_ready(connection_id));
        let caller = {
            let hub = hub.clone();
            tokio::spawn(async move { hub.call("computer.click", json!({})).await })
        };
        let command = next_json(&mut receiver).await;

        assert!(hub.detach(connection_id));
        let cancel = next_json(&mut receiver).await;
        assert_eq!(cancel["type"], "cancel");
        assert_eq!(cancel["id"], command["id"]);
        assert_eq!(cancel["sequence"], command["sequence"]);
        assert_eq!(cancel["reason"], "connection_disconnected");
        assert_eq!(
            caller.await.unwrap().unwrap_err().code,
            "COMMAND_OUTCOME_UNKNOWN"
        );
    }
}
