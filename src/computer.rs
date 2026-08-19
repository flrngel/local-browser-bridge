//! Exact-window computer control for the standalone helper.
//!
//! The helper never injects global HID input. Every captured frame is bound to
//! one `(pid, native window id)` pair, and every mutation revalidates that pair
//! before using a platform background-delivery primitive. Unsupported delivery
//! fails closed instead of stealing the foreground or moving the real cursor.

use std::io::Cursor;
use std::time::Instant;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use image::imageops::FilterType;
use serde::Serialize;
use serde_json::{Map, Value, json};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

#[cfg(target_os = "macos")]
#[path = "computer/platform_macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "computer/platform_windows.rs"]
mod platform;

pub const COMPUTER_HELPER_ORIGIN: &str = "lbb-computer-helper://local";
pub const COMPUTER_METHODS: &[&str] = &[
    "computer.status",
    "computer.observe",
    "computer.move",
    "computer.click",
    "computer.drag",
    "computer.scroll",
    "computer.typeText",
    "computer.key",
];

const MAX_CAPTURE_PIXELS: u64 = 1_000_000;
const MAX_WINDOWS: usize = 128;
const MAX_DRAG_DURATION_MS: u64 = 2_000;

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

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowDescriptor {
    pub id: String,
    pub pid: u32,
    pub app_name: String,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub minimized: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InvariantReport {
    pub foreground_unchanged: bool,
    pub user_focus_unchanged: bool,
    pub cursor_unchanged: bool,
    pub space_unchanged: bool,
}

impl InvariantReport {
    pub(crate) fn assert_held(self) -> Result<Self, ComputerError> {
        if self.foreground_unchanged
            && self.user_focus_unchanged
            && self.cursor_unchanged
            && self.space_unchanged
        {
            Ok(self)
        } else {
            Err(ComputerError::new(
                "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
                "The foreground, hardware cursor, or active desktop changed during background delivery",
            ))
        }
    }
}

#[derive(Debug, Clone)]
struct FrameState {
    id: String,
    target: WindowDescriptor,
    image_width: u32,
    image_height: u32,
}

pub struct ComputerController {
    frame: Option<FrameState>,
}

impl Default for ComputerController {
    fn default() -> Self {
        Self::new()
    }
}

impl ComputerController {
    pub fn new() -> Self {
        Self { frame: None }
    }

    pub fn hello(&mut self) -> Value {
        let windows = available_windows().unwrap_or_default();
        json!({
            "type": "hello",
            "version": crate::VERSION,
            "platform": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "backend": platform::backend_name(),
            "sessionMode": "background-window",
            "inputReady": platform::input_ready(),
            "capabilities": COMPUTER_METHODS,
            "windows": windows,
            "invariants": {
                "globalHidInput": false,
                "movesHardwareCursor": false,
                "activatesTargetApplication": false,
                "exactWindowRequired": true,
                "implicitForegroundFallback": false
            }
        })
    }

    pub fn execute(&mut self, method: &str, params: &Value) -> Result<Value, ComputerError> {
        if !COMPUTER_METHODS.contains(&method) {
            return Err(ComputerError::new(
                "COMPUTER_UNSUPPORTED_ACTION",
                format!("Unsupported computer action: {method}"),
            ));
        }
        match method {
            "computer.status" => self.status(),
            "computer.observe" => self.observe(params),
            "computer.move" => self.move_pointer(params),
            "computer.click" => self.click(params),
            "computer.drag" => self.drag(params),
            "computer.scroll" => self.scroll(params),
            "computer.typeText" => self.type_text(params),
            "computer.key" => self.key(params),
            _ => unreachable!(),
        }
    }

    pub fn request_permissions(&mut self) -> Value {
        let windows = available_windows().unwrap_or_default();
        let capture_ready = windows
            .first()
            .is_some_and(|window| platform::capture_window(window).is_ok());
        json!({
            "platform": std::env::consts::OS,
            "screenCaptureReady": capture_ready,
            "inputReady": platform::input_ready(),
            "windowCount": windows.len(),
            "sessionMode": "background-window"
        })
    }

    pub fn benchmark(&mut self, iterations: usize) -> Result<Value, ComputerError> {
        let iterations = iterations.clamp(1, 20);
        let mut capture_ms = Vec::with_capacity(iterations);
        let mut encoded_bytes = Vec::with_capacity(iterations);
        let mut last_frame = Value::Null;
        for _ in 0..iterations {
            let started = Instant::now();
            let observation = self.observe(&json!({}))?;
            capture_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
            encoded_bytes.push(
                observation
                    .get("screenshot")
                    .and_then(Value::as_str)
                    .map(str::len)
                    .unwrap_or(0),
            );
            last_frame = observation.get("frame").cloned().unwrap_or(Value::Null);
        }
        capture_ms.sort_by(f64::total_cmp);
        encoded_bytes.sort_unstable();
        let mean = capture_ms.iter().sum::<f64>() / capture_ms.len() as f64;
        Ok(json!({
            "iterations": iterations,
            "captureMs": {
                "min": capture_ms[0],
                "median": capture_ms[capture_ms.len() / 2],
                "mean": mean,
                "max": capture_ms[capture_ms.len() - 1],
            },
            "encodedDataUrlBytes": {
                "median": encoded_bytes[encoded_bytes.len() / 2],
                "max": encoded_bytes[encoded_bytes.len() - 1],
            },
            "lastFrame": last_frame,
            "note": "Exact-window capture, resize, PNG encode, and base64; no model inference or network time",
        }))
    }

    fn status(&mut self) -> Result<Value, ComputerError> {
        let windows = available_windows()?;
        Ok(json!({
            "platform": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "backend": platform::backend_name(),
            "sessionMode": "background-window",
            "isolation": "foreground-and-hardware-cursor-preserved",
            "inputReady": platform::input_ready(),
            "windowCount": windows.len(),
            "windows": windows,
            "frameReady": self.frame.is_some(),
            "limitations": platform::limitations(),
        }))
    }

    fn observe(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let requested_id = params
            .get("windowId")
            .or_else(|| params.get("displayId"))
            .and_then(Value::as_str);
        let windows = available_windows()?;
        let target = requested_id
            .and_then(|id| windows.iter().find(|window| window.id == id))
            .or_else(|| windows.iter().find(|window| !window.focused))
            .or_else(|| windows.first())
            .cloned()
            .ok_or_else(|| {
                ComputerError::new(
                    "COMPUTER_NO_WINDOW",
                    "No capturable application window is available on the active desktop",
                )
            })?;
        let image = resize_for_transport(platform::capture_window(&target)?);
        let image_width = image.width();
        let image_height = image.height();
        let mut png = Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string()))?;
        let frame_id = Uuid::new_v4().to_string();
        self.frame = Some(FrameState {
            id: frame_id.clone(),
            target: target.clone(),
            image_width,
            image_height,
        });
        Ok(json!({
            "screenshot": format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png.into_inner())),
            "frame": {
                "id": frame_id,
                "capturedAt": now_iso(),
                "windowId": target.id,
                "pid": target.pid,
                "appName": target.app_name,
                "windowTitle": target.title,
                "imageWidth": image_width,
                "imageHeight": image_height,
                "windowX": target.x,
                "windowY": target.y,
                "windowWidth": target.width,
                "windowHeight": target.height,
                "sessionMode": "background-window",
                "deliveryMode": "exact-window-background",
                "displayId": target.id,
                "displayIndex": 0,
                "displayName": format!("{} — {}", target.app_name, target.title),
                "screenX": target.x,
                "screenY": target.y,
                "screenWidth": target.width,
                "screenHeight": target.height,
                "scaleFactor": 1.0,
                "rotation": 0.0,
            },
            "windows": windows,
        }))
    }

    fn move_pointer(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let (frame, point) = self.point(params, "x", "y")?;
        let invariants = platform::move_pointer(&frame.target, point)?.assert_held()?;
        Ok(action_result(&frame, point, invariants, json!({})))
    }

    fn click(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let (frame, point) = self.point(params, "x", "y")?;
        let button = params
            .get("button")
            .and_then(Value::as_str)
            .unwrap_or("left");
        if !["left", "middle", "right"].contains(&button) {
            return Err(invalid("button must be left, middle, or right"));
        }
        let count = params
            .get("clickCount")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .clamp(1, 3) as usize;
        let invariants = platform::click(&frame.target, point, button, count)?.assert_held()?;
        Ok(action_result(
            &frame,
            point,
            invariants,
            json!({ "button": button, "clickCount": count }),
        ))
    }

    fn drag(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let frame = self.verify_frame(params)?;
        let from = self.frame_point(&frame, params, "fromX", "fromY")?;
        let to = self.frame_point(&frame, params, "toX", "toY")?;
        let duration_ms = params
            .get("durationMs")
            .and_then(Value::as_u64)
            .unwrap_or(500)
            .clamp(50, MAX_DRAG_DURATION_MS);
        let invariants = platform::drag(&frame.target, from, to, duration_ms)?.assert_held()?;
        Ok(json!({
            "deliveryMode": "exact-window-background",
            "frameId": frame.id,
            "from": { "x": from.local_x, "y": from.local_y },
            "to": { "x": to.local_x, "y": to.local_y },
            "durationMs": duration_ms,
            "invariants": invariants,
        }))
    }

    fn scroll(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let (frame, point) = self.point(params, "x", "y")?;
        let delta_x = integer(params, "deltaX", 0)?.clamp(-50, 50) as i32;
        let delta_y = integer(params, "deltaY", 0)?.clamp(-50, 50) as i32;
        let invariants = platform::scroll(&frame.target, point, delta_x, delta_y)?.assert_held()?;
        Ok(action_result(
            &frame,
            point,
            invariants,
            json!({ "deltaX": delta_x, "deltaY": delta_y }),
        ))
    }

    fn type_text(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let frame = self.verify_frame(params)?;
        let text = params
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("text must be a string"))?;
        let invariants = platform::type_text(&frame.target, text)?.assert_held()?;
        Ok(json!({
            "deliveryMode": "exact-window-background",
            "frameId": frame.id,
            "characters": text.chars().count(),
            "invariants": invariants,
        }))
    }

    fn key(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let frame = self.verify_frame(params)?;
        let chord = params
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("key must be a string"))?;
        validate_key_chord(chord)?;
        let invariants = platform::key(&frame.target, chord)?.assert_held()?;
        Ok(json!({
            "deliveryMode": "exact-window-background",
            "frameId": frame.id,
            "key": chord,
            "invariants": invariants,
        }))
    }

    fn point(
        &self,
        params: &Value,
        x_name: &str,
        y_name: &str,
    ) -> Result<(FrameState, TargetPoint), ComputerError> {
        let frame = self.verify_frame(params)?;
        let point = self.frame_point(&frame, params, x_name, y_name)?;
        Ok((frame, point))
    }

    fn frame_point(
        &self,
        frame: &FrameState,
        params: &Value,
        x_name: &str,
        y_name: &str,
    ) -> Result<TargetPoint, ComputerError> {
        let x = number(params, x_name)?;
        let y = number(params, y_name)?;
        if x < 0.0 || y < 0.0 || x >= frame.image_width as f64 || y >= frame.image_height as f64 {
            return Err(invalid(format!(
                "Point ({x}, {y}) is outside the {}x{} captured frame",
                frame.image_width, frame.image_height
            )));
        }
        Ok(map_image_point(frame, x, y))
    }

    fn verify_frame(&self, params: &Value) -> Result<FrameState, ComputerError> {
        let supplied = params.get("frameId").and_then(Value::as_str).unwrap_or("");
        let frame = self.frame.as_ref().ok_or_else(stale_frame)?;
        if supplied != frame.id {
            return Err(stale_frame());
        }
        let live = available_windows()?
            .into_iter()
            .find(|window| window.id == frame.target.id && window.pid == frame.target.pid)
            .ok_or_else(stale_frame)?;
        if live.x != frame.target.x
            || live.y != frame.target.y
            || live.width != frame.target.width
            || live.height != frame.target.height
            || live.minimized
        {
            return Err(stale_frame());
        }
        Ok(frame.clone())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TargetPoint {
    pub local_x: i32,
    pub local_y: i32,
    pub screen_x: i32,
    pub screen_y: i32,
}

fn action_result(
    frame: &FrameState,
    point: TargetPoint,
    invariants: InvariantReport,
    extra: Value,
) -> Value {
    let mut result = json!({
        "deliveryMode": "exact-window-background",
        "frameId": frame.id,
        "x": point.local_x,
        "y": point.local_y,
        "invariants": invariants,
    });
    if let (Some(target), Some(extra)) = (result.as_object_mut(), extra.as_object()) {
        target.extend(extra.clone());
    }
    result
}

fn available_windows() -> Result<Vec<WindowDescriptor>, ComputerError> {
    platform::windows(MAX_WINDOWS)
}

fn resize_for_transport(image: image::RgbaImage) -> image::RgbaImage {
    let pixels = u64::from(image.width()) * u64::from(image.height());
    if pixels <= MAX_CAPTURE_PIXELS {
        return image;
    }
    let scale = (MAX_CAPTURE_PIXELS as f64 / pixels as f64).sqrt();
    let width = (image.width() as f64 * scale).floor().max(1.0) as u32;
    let height = (image.height() as f64 * scale).floor().max(1.0) as u32;
    image::imageops::resize(&image, width, height, FilterType::Triangle)
}

fn map_image_point(frame: &FrameState, x: f64, y: f64) -> TargetPoint {
    let local_x = (x / frame.image_width as f64 * frame.target.width as f64)
        .round()
        .clamp(0.0, frame.target.width.saturating_sub(1) as f64) as i32;
    let local_y = (y / frame.image_height as f64 * frame.target.height as f64)
        .round()
        .clamp(0.0, frame.target.height.saturating_sub(1) as f64) as i32;
    TargetPoint {
        local_x,
        local_y,
        screen_x: frame.target.x.saturating_add(local_x),
        screen_y: frame.target.y.saturating_add(local_y),
    }
}

fn validate_key_chord(value: &str) -> Result<(), ComputerError> {
    let parts = value
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 8 {
        return Err(invalid("key must contain between 1 and 8 chord parts"));
    }
    for modifier in &parts[..parts.len().saturating_sub(1)] {
        if ![
            "control", "ctrl", "alt", "option", "shift", "meta", "command", "cmd", "super", "win",
        ]
        .contains(&modifier.to_ascii_lowercase().as_str())
        {
            return Err(invalid(format!("Unsupported modifier: {modifier}")));
        }
    }
    Ok(())
}

fn number(params: &Value, name: &str) -> Result<f64, ComputerError> {
    let value = params
        .get(name)
        .and_then(Value::as_f64)
        .ok_or_else(|| invalid(format!("{name} must be a number")))?;
    if !value.is_finite() {
        return Err(invalid(format!("{name} must be finite")));
    }
    Ok(value)
}

fn integer(params: &Value, name: &str, default: i64) -> Result<i64, ComputerError> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_i64()
            .ok_or_else(|| invalid(format!("{name} must be an integer"))),
    }
}

fn invalid(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_INVALID_REQUEST", message)
}

fn stale_frame() -> ComputerError {
    ComputerError::new(
        "COMPUTER_STALE_FRAME",
        "The exact-window frame is missing or stale. Observe the target window again before acting.",
    )
}

fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_owned())
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

    fn frame() -> FrameState {
        FrameState {
            id: "frame-1".to_owned(),
            target: WindowDescriptor {
                id: "window-1".to_owned(),
                pid: 42,
                app_name: "Fixture".to_owned(),
                title: "Background target".to_owned(),
                x: -100,
                y: 20,
                width: 2_000,
                height: 1_000,
                minimized: false,
                focused: false,
            },
            image_width: 1_000,
            image_height: 500,
        }
    }

    #[test]
    fn maps_image_coordinates_to_exact_window_space() {
        let top_left = map_image_point(&frame(), 0.0, 0.0);
        assert_eq!((top_left.local_x, top_left.local_y), (0, 0));
        assert_eq!((top_left.screen_x, top_left.screen_y), (-100, 20));
        let center = map_image_point(&frame(), 500.0, 250.0);
        assert_eq!((center.local_x, center.local_y), (1_000, 500));
        assert_eq!((center.screen_x, center.screen_y), (900, 520));
    }

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
    fn validates_key_chord_modifiers() {
        assert!(validate_key_chord("Control+Shift+A").is_ok());
        assert!(validate_key_chord("Enter").is_ok());
        assert!(validate_key_chord("A+B").is_err());
        assert!(validate_key_chord("").is_err());
    }

    #[test]
    fn invariant_report_fails_closed() {
        let report = InvariantReport {
            foreground_unchanged: true,
            user_focus_unchanged: true,
            cursor_unchanged: false,
            space_unchanged: true,
        };
        assert_eq!(
            report.assert_held().unwrap_err().code,
            "COMPUTER_BACKGROUND_CONTRACT_VIOLATION"
        );
    }
}
