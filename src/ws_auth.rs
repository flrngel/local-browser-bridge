use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac as _};
use rand::RngCore as _;
use serde_json::{Map, Value, json};
use sha2::Sha256;
use thiserror::Error;
use uuid::Uuid;

use crate::token::token_is_valid;

pub const AUTH_VERSION: u64 = 1;
pub const AUTH_TIMEOUT: Duration = Duration::from_secs(3);
pub const MAX_AUTH_MESSAGE_BYTES: usize = 8 * 1024;
pub const MAX_AUTH_MESSAGES: usize = 4;
pub const MAX_PROVISIONAL_CONNECTIONS: usize = 4;
pub const BROWSER_CONNECTOR: &str = "browser-extension";
pub const COMPUTER_CONNECTOR: &str = "computer-helper";

const DOMAIN: &str = "LBB-WS-AUTH-V1";
const SERVER_ROLE: &str = "server";
const CLIENT_ROLE: &str = "client";
const KEY_BYTES: usize = 32;
const ENCODED_BYTES: usize = 43;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct AuthError(&'static str);

pub struct ServerChallenge {
    connector: &'static str,
    session_id: Uuid,
    client_nonce: String,
    server_nonce: String,
    envelope: Value,
}

impl ServerChallenge {
    pub fn from_client_hello(
        token: &str,
        connector: &'static str,
        hello: &Value,
    ) -> Result<Self, AuthError> {
        validate_connector(connector)?;
        let object = exact_object(hello, &["type", "authVersion", "connector", "clientNonce"])?;
        require_string(object, "type", "authHello")?;
        require_version(object)?;
        require_string(object, "connector", connector)?;
        let client_nonce = canonical_encoded_32(require_text(object, "clientNonce")?)?.to_owned();
        let session_id = Uuid::new_v4();
        let server_nonce = loop {
            let candidate = random_nonce();
            if candidate != client_nonce {
                break candidate;
            }
        };
        let proof = create_proof(
            token,
            &server_transcript(connector, session_id, &client_nonce, &server_nonce),
        )?;
        let envelope = json!({
            "type": "authChallenge",
            "authVersion": AUTH_VERSION,
            "connector": connector,
            "sessionId": session_id.to_string(),
            "clientNonce": client_nonce,
            "serverNonce": server_nonce,
            "serverProof": proof,
        });
        Ok(Self {
            connector,
            session_id,
            client_nonce,
            server_nonce,
            envelope,
        })
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn envelope(&self) -> &Value {
        &self.envelope
    }

    pub fn verify_response(&self, token: &str, response: &Value) -> Result<(), AuthError> {
        let object = exact_object(
            response,
            &[
                "type",
                "authVersion",
                "connector",
                "sessionId",
                "clientNonce",
                "serverNonce",
                "clientProof",
            ],
        )?;
        require_string(object, "type", "authResponse")?;
        require_version(object)?;
        require_string(object, "connector", self.connector)?;
        require_string(object, "sessionId", &self.session_id.to_string())?;
        require_string(object, "clientNonce", &self.client_nonce)?;
        require_string(object, "serverNonce", &self.server_nonce)?;
        let client_proof = require_text(object, "clientProof")?;
        verify_proof(
            token,
            &client_transcript(
                self.connector,
                self.session_id,
                &self.client_nonce,
                &self.server_nonce,
            ),
            client_proof,
        )
    }
}

pub struct ClientHello {
    connector: &'static str,
    client_nonce: String,
    envelope: Value,
}

impl ClientHello {
    pub fn new(connector: &'static str) -> Result<Self, AuthError> {
        validate_connector(connector)?;
        let client_nonce = random_nonce();
        let envelope = json!({
            "type": "authHello",
            "authVersion": AUTH_VERSION,
            "connector": connector,
            "clientNonce": client_nonce,
        });
        Ok(Self {
            connector,
            client_nonce,
            envelope,
        })
    }

    pub fn envelope(&self) -> &Value {
        &self.envelope
    }

    pub fn answer_challenge(
        &self,
        token: &str,
        challenge: &Value,
    ) -> Result<(Uuid, Value), AuthError> {
        let object = exact_object(
            challenge,
            &[
                "type",
                "authVersion",
                "connector",
                "sessionId",
                "clientNonce",
                "serverNonce",
                "serverProof",
            ],
        )?;
        require_string(object, "type", "authChallenge")?;
        require_version(object)?;
        require_string(object, "connector", self.connector)?;
        require_string(object, "clientNonce", &self.client_nonce)?;
        let session_text = require_text(object, "sessionId")?;
        let session_id = canonical_session_id(session_text)?;
        let server_nonce = canonical_encoded_32(require_text(object, "serverNonce")?)?;
        if server_nonce == self.client_nonce {
            return Err(AuthError("server nonce must differ from client nonce"));
        }
        verify_proof(
            token,
            &server_transcript(self.connector, session_id, &self.client_nonce, server_nonce),
            require_text(object, "serverProof")?,
        )?;
        let client_proof = create_proof(
            token,
            &client_transcript(self.connector, session_id, &self.client_nonce, server_nonce),
        )?;
        Ok((
            session_id,
            json!({
                "type": "authResponse",
                "authVersion": AUTH_VERSION,
                "connector": self.connector,
                "sessionId": session_id.to_string(),
                "clientNonce": self.client_nonce,
                "serverNonce": server_nonce,
                "clientProof": client_proof,
            }),
        ))
    }
}

fn server_transcript(
    connector: &str,
    session_id: Uuid,
    client_nonce: &str,
    server_nonce: &str,
) -> String {
    format!("{DOMAIN}\n{SERVER_ROLE}\n{connector}\n{session_id}\n{client_nonce}\n{server_nonce}")
}

fn client_transcript(
    connector: &str,
    session_id: Uuid,
    client_nonce: &str,
    server_nonce: &str,
) -> String {
    format!("{DOMAIN}\n{CLIENT_ROLE}\n{connector}\n{session_id}\n{client_nonce}\n{server_nonce}")
}

fn create_proof(token: &str, transcript: &str) -> Result<String, AuthError> {
    let key = token_key(token)?;
    let mut mac = HmacSha256::new_from_slice(&key).map_err(|_| AuthError("invalid HMAC key"))?;
    mac.update(transcript.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

fn verify_proof(token: &str, transcript: &str, proof: &str) -> Result<(), AuthError> {
    let key = token_key(token)?;
    let proof = decode_canonical_32(proof).map_err(|_| AuthError("invalid HMAC proof"))?;
    let mut mac = HmacSha256::new_from_slice(&key).map_err(|_| AuthError("invalid HMAC key"))?;
    mac.update(transcript.as_bytes());
    mac.verify_slice(&proof)
        .map_err(|_| AuthError("HMAC proof did not verify"))
}

fn token_key(token: &str) -> Result<[u8; KEY_BYTES], AuthError> {
    if !token_is_valid(token) {
        return Err(AuthError("invalid bridge token"));
    }
    decode_canonical_32(token).map_err(|_| AuthError("invalid bridge token"))
}

fn decode_canonical_32(value: &str) -> Result<[u8; KEY_BYTES], AuthError> {
    if value.len() != ENCODED_BYTES || value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(AuthError("invalid base64url value"));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AuthError("invalid base64url value"))?;
    let decoded: [u8; KEY_BYTES] = decoded
        .try_into()
        .map_err(|_| AuthError("invalid decoded length"))?;
    if URL_SAFE_NO_PAD.encode(decoded) != value {
        return Err(AuthError("non-canonical base64url value"));
    }
    Ok(decoded)
}

fn canonical_encoded_32(value: &str) -> Result<&str, AuthError> {
    decode_canonical_32(value)?;
    Ok(value)
}

fn canonical_session_id(value: &str) -> Result<Uuid, AuthError> {
    let session_id = Uuid::parse_str(value).map_err(|_| AuthError("invalid session id"))?;
    if session_id.to_string() != value {
        return Err(AuthError("non-canonical session id"));
    }
    Ok(session_id)
}

fn random_nonce() -> String {
    let mut bytes = [0_u8; KEY_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn validate_connector(connector: &str) -> Result<(), AuthError> {
    if matches!(connector, BROWSER_CONNECTOR | COMPUTER_CONNECTOR) {
        Ok(())
    } else {
        Err(AuthError("unknown connector"))
    }
}

fn exact_object<'a>(
    value: &'a Value,
    expected_keys: &[&str],
) -> Result<&'a Map<String, Value>, AuthError> {
    let object = value
        .as_object()
        .ok_or(AuthError("authentication envelope must be an object"))?;
    if object.len() != expected_keys.len()
        || expected_keys.iter().any(|key| !object.contains_key(*key))
    {
        return Err(AuthError("authentication envelope fields did not match"));
    }
    Ok(object)
}

fn require_version(object: &Map<String, Value>) -> Result<(), AuthError> {
    if object.get("authVersion").and_then(Value::as_u64) == Some(AUTH_VERSION) {
        Ok(())
    } else {
        Err(AuthError("authentication version did not match"))
    }
}

fn require_text<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, AuthError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(AuthError("authentication text field was missing"))
}

fn require_string(object: &Map<String, Value>, key: &str, expected: &str) -> Result<(), AuthError> {
    if require_text(object, key)? == expected {
        Ok(())
    } else {
        Err(AuthError("authentication text field did not match"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_token;

    #[test]
    fn mutual_proofs_are_bound_to_role_connector_session_and_nonces() {
        let token = create_token();
        let client = ClientHello::new(BROWSER_CONNECTOR).unwrap();
        let challenge =
            ServerChallenge::from_client_hello(&token, BROWSER_CONNECTOR, client.envelope())
                .unwrap();
        let (session_id, response) = client
            .answer_challenge(&token, challenge.envelope())
            .unwrap();
        assert_eq!(session_id, challenge.session_id());
        challenge.verify_response(&token, &response).unwrap();

        for field in ["sessionId", "serverNonce", "clientNonce", "clientProof"] {
            let mut altered = response.clone();
            altered[field] = match field {
                "sessionId" => Value::String(Uuid::new_v4().to_string()),
                _ => Value::String(URL_SAFE_NO_PAD.encode([7_u8; KEY_BYTES])),
            };
            assert!(challenge.verify_response(&token, &altered).is_err());
        }
        let wrong_connector = ClientHello::new(COMPUTER_CONNECTOR).unwrap();
        assert!(
            wrong_connector
                .answer_challenge(&token, challenge.envelope())
                .is_err()
        );
        assert!(
            client
                .answer_challenge(&create_token(), challenge.envelope())
                .is_err()
        );
    }

    #[test]
    fn authentication_envelopes_reject_unknown_fields_and_noncanonical_values() {
        let token = create_token();
        let client = ClientHello::new(COMPUTER_CONNECTOR).unwrap();
        let challenge =
            ServerChallenge::from_client_hello(&token, COMPUTER_CONNECTOR, client.envelope())
                .unwrap();
        let mut envelope = challenge.envelope().clone();
        envelope["extra"] = Value::Bool(true);
        assert!(client.answer_challenge(&token, &envelope).is_err());

        let mut envelope = challenge.envelope().clone();
        envelope["serverNonce"] = Value::String("A".repeat(42));
        assert!(client.answer_challenge(&token, &envelope).is_err());
    }

    #[test]
    fn captured_server_challenge_cannot_authenticate_a_fresh_client() {
        let token = create_token();
        let first_client = ClientHello::new(BROWSER_CONNECTOR).unwrap();
        let captured =
            ServerChallenge::from_client_hello(&token, BROWSER_CONNECTOR, first_client.envelope())
                .unwrap();
        let fresh_client = ClientHello::new(BROWSER_CONNECTOR).unwrap();
        assert!(
            fresh_client
                .answer_challenge(&token, captured.envelope())
                .is_err()
        );
    }

    #[test]
    fn canonical_transcript_matches_the_cross_language_test_vector() {
        let token = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
        let client_nonce = "ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8";
        let server_nonce = "QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl8";
        let session_id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let server = server_transcript(BROWSER_CONNECTOR, session_id, client_nonce, server_nonce);
        let client = client_transcript(BROWSER_CONNECTOR, session_id, client_nonce, server_nonce);
        assert_eq!(
            server,
            "LBB-WS-AUTH-V1\nserver\nbrowser-extension\n123e4567-e89b-12d3-a456-426614174000\nICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8\nQEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl8"
        );
        assert_eq!(
            create_proof(token, &server).unwrap(),
            "kCFSEQMkA3UFR8ONc1oTIET5-XtGDT6K-qHpqxcBoC0"
        );
        assert_eq!(
            create_proof(token, &client).unwrap(),
            "aLkKO_2gRdXF217mxjepybU7h-Cdu3rVqK-U_wpauvY"
        );
    }

    #[test]
    fn provisional_authentication_limits_remain_strict() {
        assert_eq!(AUTH_TIMEOUT, Duration::from_secs(3));
        assert_eq!(MAX_AUTH_MESSAGE_BYTES, 8 * 1024);
        assert_eq!(MAX_AUTH_MESSAGES, 4);
        assert_eq!(MAX_PROVISIONAL_CONNECTIONS, 4);
    }
}
