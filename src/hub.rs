use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::Message;
use serde_json::{Value, json};
use thiserror::Error;
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
}

#[derive(Clone)]
struct Connection {
    id: Uuid,
    sender: mpsc::UnboundedSender<Message>,
}

struct PendingCall {
    connection_id: Uuid,
    sender: oneshot::Sender<Result<Value, HubError>>,
}

impl ExtensionHub {
    pub fn new(call_timeout: Duration) -> Self {
        Self {
            inner: Arc::new(HubInner {
                connection: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                call_timeout,
            }),
        }
    }

    pub fn connected(&self) -> bool {
        self.inner.connection.lock().unwrap().is_some()
    }

    pub fn attach(&self) -> (Uuid, mpsc::UnboundedReceiver<Message>) {
        let id = Uuid::new_v4();
        let (sender, receiver) = mpsc::unbounded_channel();
        let previous = self
            .inner
            .connection
            .lock()
            .unwrap()
            .replace(Connection { id, sender });
        if let Some(previous) = previous {
            let _ = previous.sender.send(Message::Close(None));
        }
        (id, receiver)
    }

    pub fn detach(&self, connection_id: Uuid) -> bool {
        let was_current = {
            let mut connection = self.inner.connection.lock().unwrap();
            if connection
                .as_ref()
                .is_some_and(|active| active.id == connection_id)
            {
                *connection = None;
                true
            } else {
                false
            }
        };

        let disconnected =
            HubError::new("EXTENSION_DISCONNECTED", "Browser extension disconnected");
        let pending = {
            let mut calls = self.inner.pending.lock().unwrap();
            let ids = calls
                .iter()
                .filter_map(|(id, call)| {
                    (call.connection_id == connection_id).then_some(id.clone())
                })
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| calls.remove(&id))
                .collect::<Vec<_>>()
        };
        for call in pending {
            let _ = call.sender.send(Err(disconnected.clone()));
        }
        was_current
    }

    pub fn resolve(&self, message: &Value) {
        let Some(id) = message.get("id").and_then(Value::as_str) else {
            return;
        };
        let Some(call) = self.inner.pending.lock().unwrap().remove(id) else {
            return;
        };
        let result = if message.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(message.get("result").cloned().unwrap_or(Value::Null))
        } else {
            let error = message.get("error").unwrap_or(&Value::Null);
            Err(HubError::new(
                error
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("EXTENSION_ERROR"),
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown extension error"),
            ))
        };
        let _ = call.sender.send(result);
    }

    pub fn send(&self, message: Value) -> Result<(), HubError> {
        self.send_message(Message::Text(message.to_string().into()))
    }

    pub fn send_message(&self, message: Message) -> Result<(), HubError> {
        let connection = self
            .inner
            .connection
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| {
                HubError::new("EXTENSION_OFFLINE", "Browser extension is not connected")
            })?;
        connection
            .sender
            .send(message)
            .map_err(|_| HubError::new("EXTENSION_DISCONNECTED", "Browser extension disconnected"))
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, HubError> {
        let connection = self
            .inner
            .connection
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| {
                HubError::new("EXTENSION_OFFLINE", "Browser extension is not connected")
            })?;
        let id = Uuid::new_v4().to_string();
        let (sender, receiver) = oneshot::channel();
        self.inner.pending.lock().unwrap().insert(
            id.clone(),
            PendingCall {
                connection_id: connection.id,
                sender,
            },
        );

        let command = json!({ "id": id, "type": "command", "method": method, "params": params });
        if connection
            .sender
            .send(Message::Text(command.to_string().into()))
            .is_err()
        {
            self.inner.pending.lock().unwrap().remove(&id);
            return Err(HubError::new(
                "EXTENSION_DISCONNECTED",
                "Browser extension disconnected",
            ));
        }

        match tokio::time::timeout(self.inner.call_timeout, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(HubError::new(
                "EXTENSION_DISCONNECTED",
                "Browser extension disconnected",
            )),
            Err(_) => {
                self.inner.pending.lock().unwrap().remove(&id);
                Err(HubError::new(
                    "COMMAND_TIMEOUT",
                    format!("Extension command timed out: {method}"),
                ))
            }
        }
    }

    pub fn close(&self) {
        if let Some(connection) = self.inner.connection.lock().unwrap().take() {
            let _ = connection.sender.send(Message::Close(None));
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
                "SERVER_STOPPED",
                "Bridge server stopped",
            )));
        }
    }
}
