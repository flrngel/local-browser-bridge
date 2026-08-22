//! Exact-window computer control for the standalone helper.
//!
//! The helper never injects global HID input. Every captured frame is bound to
//! one `(pid, native window id)` pair, and every mutation revalidates that pair
//! before using a platform background-delivery primitive. Unsupported delivery
//! fails closed instead of stealing the foreground or moving the real cursor.

use std::collections::VecDeque;
use std::io::Cursor;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use image::imageops::FilterType;
use serde::Serialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;

pub use crate::computer_protocol::{
    COMPUTER_HELPER_ORIGIN, COMPUTER_METHODS, COMPUTER_NATIVE_SHARE_CAPABILITY,
    COMPUTER_SHARE_ACK_CAPABILITY, COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS,
    COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS, CommandCancellation, ComputerError, ShareFrameAck,
    ShareMailbox, command_parts, result_envelope, validate_computer_type_text,
};

mod action_record;
mod cursor;
use action_record::{
    ActionEffect, ActionEvidence, ActionRecord, ActionTimer, invariant_evidence, semantic_evidence,
};
use cursor::SyntheticCursor;

#[cfg(target_os = "macos")]
#[path = "computer/ax_macos.rs"]
mod ax_macos;
#[cfg(target_os = "macos")]
#[path = "computer/share_macos.rs"]
mod native_share;
#[cfg(target_os = "windows")]
#[path = "computer/share_windows.rs"]
mod native_share;
#[cfg(target_os = "macos")]
#[path = "computer/platform_macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "computer/platform_windows.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "computer/uia_windows.rs"]
mod uia_windows;

pub const NATIVE_COMPUTER_SUPPORTED: bool = true;

const MAX_CAPTURE_PIXELS: u64 = 1_000_000;
const MAX_WINDOWS: usize = 128;
const MAX_DRAG_DURATION_MS: u64 = 2_000;
const MAX_CURSOR_DURATION_MS: u64 = 2_000;
const MAX_SHARE_FPS: u64 = 10;
const MAX_FRAME_AGE: Duration = Duration::from_secs(3);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvariantStage {
    PointerTrajectory,
    ClickDispatch,
    DragDispatch,
    ScrollDispatch,
    TextDispatch,
    KeyDispatch,
    SemanticInvoke,
    SemanticSetValue,
    #[cfg(target_os = "macos")]
    FocusPreparation,
    #[cfg(target_os = "macos")]
    FocusRestore,
    #[cfg(target_os = "macos")]
    FocusRecovery,
}

impl InvariantStage {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::PointerTrajectory => "pointerTrajectory",
            Self::ClickDispatch => "clickDispatch",
            Self::DragDispatch => "dragDispatch",
            Self::ScrollDispatch => "scrollDispatch",
            Self::TextDispatch => "textDispatch",
            Self::KeyDispatch => "keyDispatch",
            Self::SemanticInvoke => "semanticInvoke",
            Self::SemanticSetValue => "semanticSetValue",
            #[cfg(target_os = "macos")]
            Self::FocusPreparation => "focusPreparation",
            #[cfg(target_os = "macos")]
            Self::FocusRestore => "focusRestore",
            #[cfg(target_os = "macos")]
            Self::FocusRecovery => "focusRecovery",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvariantFailure {
    Foreground,
    UserFocus,
    HardwareCursor,
    DesktopSpace,
}

impl InvariantFailure {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Foreground => "foregroundUnchanged",
            Self::UserFocus => "userFocusUnchanged",
            Self::HardwareCursor => "hardwareCursorUnchanged",
            Self::DesktopSpace => "desktopSpaceUnchanged",
        }
    }
}

impl InvariantReport {
    pub(crate) fn assert_held(self, stage: InvariantStage) -> Result<Self, ComputerError> {
        if self.foreground_unchanged
            && self.user_focus_unchanged
            && self.cursor_unchanged
            && self.space_unchanged
        {
            Ok(self)
        } else {
            Err(background_contract_violation(stage, self.failed_names()))
        }
    }

    fn failed_names(&self) -> Vec<InvariantFailure> {
        [
            (!self.foreground_unchanged).then_some(InvariantFailure::Foreground),
            (!self.user_focus_unchanged).then_some(InvariantFailure::UserFocus),
            (!self.cursor_unchanged).then_some(InvariantFailure::HardwareCursor),
            (!self.space_unchanged).then_some(InvariantFailure::DesktopSpace),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

pub(crate) fn background_contract_violation(
    stage: InvariantStage,
    failed_invariants: impl IntoIterator<Item = InvariantFailure>,
) -> ComputerError {
    ComputerError::new(
        "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
        format!(
            "stage={};failedInvariants={}",
            stage.as_str(),
            failed_invariants
                .into_iter()
                .map(InvariantFailure::as_str)
                .collect::<Vec<_>>()
                .join(",")
        ),
    )
}

#[derive(Debug, Clone)]
pub(crate) struct FrameState {
    id: String,
    target: WindowDescriptor,
    image_width: u32,
    image_height: u32,
    elements: Vec<SemanticTarget>,
    captured_at: Instant,
    share_id: Option<String>,
    source_sequence: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SemanticBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SemanticElement {
    #[serde(rename = "ref")]
    pub reference: String,
    pub role: String,
    pub name: String,
    pub value: Option<String>,
    pub sensitive: bool,
    pub value_redacted: bool,
    pub enabled: Option<bool>,
    pub actions: Vec<String>,
    pub bounds: Option<SemanticBounds>,
    pub coordinate_space: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_bounds: Option<SemanticBounds>,
    /// Provider identity used only to revalidate a native element before an
    /// action. This must never become part of the browser-facing protocol.
    #[serde(skip)]
    pub(crate) native_identity: Option<NativeElementIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeElementIdentity {
    pub(crate) automation_id: String,
    pub(crate) class_name: String,
    pub(crate) framework_id: String,
    pub(crate) control_type: i32,
    pub(crate) provider_process_id: i32,
    pub(crate) capability_mask: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct SemanticTarget {
    pub element: SemanticElement,
    pub path: Vec<usize>,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SemanticTruncationReason {
    NodeBudget,
    DepthBudget,
    ActionableBudget,
    Deadline,
    ProviderError,
}

impl SemanticTruncationReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NodeBudget => "node_budget",
            Self::DepthBudget => "depth_budget",
            Self::ActionableBudget => "actionable_budget",
            Self::Deadline => "deadline",
            Self::ProviderError => "provider_error",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SemanticSnapshot {
    pub(crate) elements: Vec<SemanticTarget>,
    pub(crate) truncation_reason: Option<SemanticTruncationReason>,
}

pub struct ComputerController {
    frame: Option<FrameState>,
    recent_frames: VecDeque<FrameState>,
    cursor: SyntheticCursor,
    share: Option<ShareSession>,
    share_ack_paced: bool,
}

#[derive(Debug)]
struct ShareSession {
    id: String,
    window_id: String,
    target_pid: u32,
    fps: u64,
    sequence: u64,
    source_sequence: u64,
    started_at: String,
    capture: Option<native_share::NativeShareCapture>,
    mailbox: ShareMailbox,
}

impl Default for ComputerController {
    fn default() -> Self {
        Self::new()
    }
}

impl ComputerController {
    pub fn new() -> Self {
        Self {
            frame: None,
            recent_frames: VecDeque::with_capacity(32),
            cursor: SyntheticCursor::new(Uuid::new_v4().to_string()),
            share: None,
            share_ack_paced: false,
        }
    }

    /// Drains a process-fatal native capture shutdown failure for the
    /// disposable helper worker. This is separate from command results because
    /// best-effort lease rollback paths deliberately discard ordinary errors.
    #[doc(hidden)]
    pub fn take_fatal_capture_stop_error() -> Option<ComputerError> {
        #[cfg(target_os = "windows")]
        {
            native_share::take_fatal_stop_error()
        }
        #[cfg(not(target_os = "windows"))]
        {
            None
        }
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
            "isolation": "shared-user-session/target-routed",
            "inputReady": platform::input_ready(),
            "semanticReady": platform::semantic_ready(false),
            "capabilities": advertised_capabilities(),
            "windows": windows,
            "invariants": {
                "globalHidInput": false,
                "movesHardwareCursor": false,
                "activatesTargetApplication": false,
                "exactWindowRequired": true,
                "implicitForegroundFallback": false
            },
            "pointer": self.cursor.snapshot(self.frame.as_ref()),
            "capture": {
                "mode": "native-exact-window-stream",
                "transport": "request-response+bounded-live-frames",
                "cursorComposited": true,
                "nativeStream": true,
                "createsSeparateDesktop": false
            },
            "share": self.share_status_value(),
        })
    }

    pub fn execute(&mut self, method: &str, params: &Value) -> Result<Value, ComputerError> {
        self.execute_cancellable(method, params, &CommandCancellation::new())
    }

    pub fn execute_cancellable(
        &mut self,
        method: &str,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        cancellation.check("command dispatch")?;
        if !COMPUTER_METHODS.contains(&method) {
            return Err(ComputerError::new(
                "COMPUTER_UNSUPPORTED_ACTION",
                format!("Unsupported computer action: {method}"),
            ));
        }
        let result = match method {
            "computer.status" => self.status(),
            "computer.share.start" => self.share_start(params, cancellation),
            "computer.share.status" => Ok(self.share_status_value()),
            "computer.share.stop" => self.share_stop(cancellation),
            "computer.observe" => self.observe(params),
            "computer.move" => self.move_pointer(params, cancellation),
            "computer.click" => self.click(params, cancellation),
            "computer.drag" => self.drag(params, cancellation),
            "computer.scroll" => self.scroll(params, cancellation),
            "computer.typeText" => self.type_text(params, cancellation),
            "computer.key" => self.key(params, cancellation),
            "computer.invoke" => self.invoke(params, cancellation),
            "computer.setValue" => self.set_value(params, cancellation),
            _ => unreachable!(),
        };
        self.finish_command_result(cancellation, result)
    }

    /// Finalizes a controller command and revokes every observation-derived
    /// authority if an operating-system side effect has an unknown outcome.
    ///
    /// This is deliberately inside the controller rather than left to the
    /// caller's retry discipline: an outcome-unknown response requires a fresh
    /// observation, so the frame and pointer that authorized the ambiguous
    /// mutation must stop working synchronously.
    fn finish_command_result<T>(
        &mut self,
        cancellation: &CommandCancellation,
        result: Result<T, ComputerError>,
    ) -> Result<T, ComputerError> {
        let result = cancellation.finish(result);
        if result
            .as_ref()
            .err()
            .is_some_and(|error| error.code == "COMPUTER_OUTCOME_UNKNOWN")
        {
            self.revoke_command_authority();
        }
        result
    }

    /// Revokes all observation and action state owned by a helper transport session.
    ///
    /// A replacement WebSocket session must explicitly start sharing and observe
    /// a fresh frame before it can issue an action.
    pub fn reset_transport_session(&mut self) {
        self.stop_share_best_effort();
        self.share_ack_paced = false;
        self.clear_frame_authority();
    }

    fn clear_frame_authority(&mut self) {
        self.frame = None;
        self.recent_frames.clear();
        self.cursor = SyntheticCursor::new(Uuid::new_v4().to_string());
    }

    /// Enables server-acknowledged share pacing for shares started afterwards.
    ///
    /// The helper sets this exactly once per transport session, after the
    /// bridge confirmed the `computer.share.ack` capability in its hello
    /// acknowledgement; a replacement session always starts unpaced.
    pub fn set_share_ack_pacing(&mut self, enabled: bool) {
        self.share_ack_paced = enabled;
    }

    /// Reports whether this transport session currently owns a live share.
    ///
    /// The helper checks this while holding the controller lock so its
    /// acceptance-only watchdog fixture cannot fire before `share.start` has
    /// committed the share authority epoch.
    pub fn has_active_share(&self) -> bool {
        self.share.is_some()
    }

    /// Revokes share and frame authority after a canceled in-flight command
    /// whose outcome is unknown, without touching the transport session's
    /// negotiated share ack pacing: the session itself is unchanged, so a
    /// later `computer.share.start` must keep the hello-negotiated pacing
    /// the bridge still expects.
    pub fn revoke_command_authority(&mut self) {
        let share_ack_paced = self.share_ack_paced;
        self.reset_transport_session();
        self.share_ack_paced = share_ack_paced;
    }

    pub fn request_permissions(&mut self) -> Value {
        let semantic_ready = platform::semantic_ready(true);
        let windows = available_windows().unwrap_or_default();
        let capture_ready = windows
            .first()
            .cloned()
            .is_some_and(|mut window| platform::capture_window(&mut window).is_ok());
        json!({
            "platform": std::env::consts::OS,
            "screenCaptureReady": capture_ready,
            "inputReady": platform::input_ready(),
            "semanticReady": semantic_ready,
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
            "isolation": "shared-user-session/target-routed",
            "inputReady": platform::input_ready(),
            "semanticReady": platform::semantic_ready(false),
            "windowCount": windows.len(),
            "windows": windows,
            "frameReady": self.frame.is_some(),
            "pointer": self.cursor.snapshot(self.frame.as_ref()),
            "share": self.share_status_value(),
            "limitations": platform::limitations(),
        }))
    }

    fn observe(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let requested_id = params
            .get("windowId")
            .or_else(|| params.get("displayId"))
            .and_then(Value::as_str);
        let windows = available_windows()?;
        let mut target = match requested_id {
            Some(id) => windows
                .iter()
                .find(|window| window.id == id)
                .cloned()
                .ok_or_else(|| {
                    ComputerError::new(
                        "COMPUTER_NO_WINDOW",
                        "The requested exact window is not available; no other window was captured",
                    )
                })?,
            None => windows
                .iter()
                .find(|window| !window.focused)
                .or_else(|| windows.first())
                .cloned()
                .ok_or_else(|| {
                    ComputerError::new(
                        "COMPUTER_NO_WINDOW",
                        "No capturable application window is available on the active desktop",
                    )
                })?,
        };
        let (image, captured_at) = platform::capture_window(&mut target)?;
        self.build_observation(target, windows, image, captured_at, None)
    }

    fn build_observation(
        &mut self,
        target: WindowDescriptor,
        windows: Vec<WindowDescriptor>,
        image: image::RgbaImage,
        captured_at: Instant,
        source_sequence: Option<u64>,
    ) -> Result<Value, ComputerError> {
        if captured_at.elapsed() > MAX_FRAME_AGE {
            return Err(ComputerError::new(
                "COMPUTER_STALE_FRAME",
                "The native exact-window stream did not provide a fresh frame",
            ));
        }
        let mut image = resize_for_transport(image);
        let (elements, semantic_available, semantic_error, semantic_truncation_reason) =
            match platform::semantic_elements(&target) {
                Ok(snapshot) => (
                    snapshot.elements,
                    true,
                    None,
                    snapshot
                        .truncation_reason
                        .map(SemanticTruncationReason::as_str),
                ),
                Err(error) => (Vec::new(), false, Some(error.message), None),
            };
        let image_width = image.width();
        let image_height = image.height();
        let frame_id = Uuid::new_v4().to_string();
        let share_id = source_sequence.and_then(|_| {
            self.share
                .as_ref()
                .filter(|share| share.window_id == target.id && share.target_pid == target.pid)
                .map(|share| share.id.clone())
        });
        let frame = FrameState {
            id: frame_id.clone(),
            target: target.clone(),
            image_width,
            image_height,
            elements: elements.clone(),
            captured_at,
            share_id: share_id.clone(),
            source_sequence,
        };
        let transport_elements = elements
            .iter()
            .cloned()
            .map(|mut semantic| {
                let screen_bounds = semantic.element.bounds.clone();
                semantic.element.bounds = screen_bounds.as_ref().and_then(|bounds| {
                    screen_bounds_to_image(bounds, &target, image_width, image_height)
                });
                semantic.element.screen_bounds = screen_bounds;
                semantic.element.coordinate_space = "image-pixels".to_owned();
                semantic
            })
            .collect::<Vec<_>>();
        self.cursor.composite(&mut image, &target);
        let pointer = self.cursor.snapshot(Some(&frame));
        let mut png = Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string()))?;
        if let Some(source_sequence) = source_sequence {
            let share = self
                .share
                .as_mut()
                .filter(|share| share.window_id == target.id && share.target_pid == target.pid)
                .ok_or_else(|| {
                    ComputerError::new(
                        "COMPUTER_CAPTURE_FAILED",
                        "The native capture frame no longer belongs to the active share lease",
                    )
                })?;
            share.source_sequence = source_sequence;
            share.sequence = share.sequence.saturating_add(1);
        }
        if let Some(previous) = self.frame.replace(frame) {
            self.recent_frames.push_front(previous);
            self.recent_frames.truncate(32);
        }
        let capture_age_ms = captured_at.elapsed().as_secs_f64() * 1_000.0;
        let mut result = json!({
            "screenshot": format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png.into_inner())),
            "frame": {
                "id": frame_id,
                "capturedAt": now_iso(),
                "captureAgeMs": capture_age_ms,
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
                "scaleFactor": image_width as f64 / target.width.max(1) as f64,
                "transportScaleX": image_width as f64 / target.width.max(1) as f64,
                "transportScaleY": image_height as f64 / target.height.max(1) as f64,
                "rotation": 0.0,
                "semanticMode": platform::semantic_backend_name(),
                "semanticAvailable": semantic_available,
                "semanticError": semantic_error,
                "elements": transport_elements.into_iter().map(|target| target.element).collect::<Vec<_>>(),
                "pointer": pointer,
                "sourceSequence": source_sequence,
                "shareId": share_id,
                "share": self.share_frame_value(),
            },
            "windows": windows,
        });
        set_semantic_truncation_metadata(
            result
                .get_mut("frame")
                .expect("the observation frame was constructed above"),
            semantic_truncation_reason,
        );
        Ok(result)
    }

    fn share_start(
        &mut self,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        let window_id = params
            .get("windowId")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("windowId must be a string"))?;
        let fps = params.get("fps").and_then(Value::as_u64).unwrap_or(4);
        if !(1..=MAX_SHARE_FPS).contains(&fps) {
            return Err(invalid(format!(
                "fps must be between 1 and {MAX_SHARE_FPS}"
            )));
        }
        let target = available_windows()?
            .into_iter()
            .find(|window| window.id == window_id && !window.minimized)
            .ok_or_else(|| {
                ComputerError::new(
                    "COMPUTER_NO_WINDOW",
                    "The requested exact window is not available for sharing",
                )
            })?;

        cancellation.check("native share startup")?;
        let mut capture = native_share::NativeShareCapture::start(&target, fps)?;
        if let Err(error) = cancellation.check("native share first-frame readiness") {
            let _ = capture.stop();
            return Err(error);
        }
        cancellation.begin_side_effect("native share lease commit")?;
        let replacement = ShareSession {
            id: Uuid::new_v4().to_string(),
            window_id: window_id.to_owned(),
            target_pid: target.pid,
            fps,
            sequence: 0,
            source_sequence: 0,
            started_at: now_iso(),
            capture: Some(capture),
            mailbox: ShareMailbox::new(self.share_ack_paced),
        };
        self.commit_share_replacement(replacement);
        if let Err(error) = cancellation.check("native share lease committed") {
            self.stop_share_best_effort();
            self.clear_frame_authority();
            return Err(error);
        }
        Ok(self.share_status_value())
    }

    fn commit_share_replacement(&mut self, replacement: ShareSession) {
        let previous = self.share.replace(replacement);
        drop(previous);
        // A share ID is an authority epoch. Frames rendered under the previous
        // share (or before sharing began) must not remain current after the new
        // lease commits, even during the short gap before its first protocol
        // frame is converted.
        self.clear_frame_authority();
    }

    fn share_stop(&mut self, cancellation: &CommandCancellation) -> Result<Value, ComputerError> {
        if self.share.is_none() {
            return Ok(json!({
                "active": false,
                "stopped": false,
                "reason": "not-active",
            }));
        }
        cancellation.begin_side_effect("native share stop")?;
        let mut stopped = self.share.take().expect("share presence checked");
        let stop_result = stopped
            .capture
            .take()
            .map_or(Ok(()), |mut capture| capture.stop());
        self.clear_frame_authority();
        stop_result?;
        Ok(json!({
            "active": false,
            "stopped": true,
            "id": stopped.id,
            "reason": "requested",
        }))
    }

    fn stop_share_best_effort(&mut self) {
        if let Some(mut share) = self.share.take()
            && let Some(mut capture) = share.capture.take()
        {
            let _ = capture.stop();
        }
    }

    fn fail_share_capture(&mut self, error: ComputerError) -> ComputerError {
        self.stop_share_best_effort();
        self.clear_frame_authority();
        error
    }

    fn share_status_value(&self) -> Value {
        self.share
            .as_ref()
            .map(|share| {
                let metadata = share.capture.as_ref().map(|capture| capture.metadata());
                let source_dropped_frames = share
                    .capture
                    .as_ref()
                    .map_or(0, native_share::NativeShareCapture::dropped_frames);
                json!({
                    "active": true,
                    "id": share.id,
                    "windowId": share.window_id,
                    "pid": share.target_pid,
                    "fps": share.fps,
                    "sequence": share.sequence,
                    "sourceSequence": share.source_sequence,
                    "startedAt": share.started_at,
                    "captureScope": "exact-window",
                    "captureMode": "persistent-native-stream",
                    "captureBackend": metadata.map(|metadata| metadata.backend).unwrap_or("test-no-native-stream"),
                    "nativeStream": metadata.is_some_and(|metadata| metadata.native_stream),
                    "systemIndicator": metadata.is_some_and(|metadata| metadata.system_indicator),
                    "selectionMode": metadata.map(|metadata| metadata.selection_mode).unwrap_or("test"),
                    "isolation": "shared-user-session",
                    "createsSeparateDesktop": false,
                    "cursorComposited": true,
                    "droppedFrames": share.mailbox.dropped_frames(),
                    "transportDroppedFrames": share.mailbox.dropped_frames(),
                    "sourceDroppedFrames": source_dropped_frames,
                    "ackPaced": share.mailbox.ack_paced(),
                    "lastAckedSequence": share.mailbox.last_acked_sequence(),
                    "backpressure": if share.mailbox.ack_paced() {
                        "latest-frame-wins"
                    } else {
                        "producer-blocking"
                    },
                })
            })
            .unwrap_or_else(|| json!({ "active": false }))
    }

    fn share_frame_value(&self) -> Value {
        self.share_status_value()
    }

    /// Converts the newest frame from the persistent native capture stream into
    /// a protocol observation and parks it in the transport mailbox.
    ///
    /// A capture failure surfaces immediately for a `computer.share.error`
    /// emission; a successful capture is parked latest-frame-wins until
    /// `take_share_emission` releases it. The native producer and transport
    /// each retain only one latest frame, so neither a slow server nor an input
    /// command can create an unbounded image queue.
    pub fn pump_share_capture(&mut self) -> Option<ComputerError> {
        let (window_id, target_pid, last_source_sequence, native_frame) = {
            let share = self.share.as_ref()?;
            let capture = match share.capture.as_ref() {
                Some(capture) => capture,
                None => {
                    let error = ComputerError::new(
                        "COMPUTER_CAPTURE_FAILED",
                        "The active share has no native capture stream",
                    );
                    return Some(self.fail_share_capture(error));
                }
            };
            (
                share.window_id.clone(),
                share.target_pid,
                share.source_sequence,
                capture.latest_after(share.source_sequence),
            )
        };
        let native_frame = match native_frame {
            Ok(Some(frame)) => frame,
            Ok(None) => return None,
            Err(error) => return Some(self.fail_share_capture(error)),
        };
        let windows = match available_windows() {
            Ok(windows) => windows,
            Err(error) => return Some(self.fail_share_capture(error)),
        };
        let Some(mut target) = windows
            .iter()
            .find(|window| window.id == window_id && window.pid == target_pid && !window.minimized)
            .cloned()
        else {
            let error = ComputerError::new(
                "COMPUTER_NO_WINDOW",
                "The exact shared (process, window) pair is no longer available",
            );
            return Some(self.fail_share_capture(error));
        };
        let geometry_changed = {
            let share = self.share.as_mut()?;
            share.capture.as_mut().map_or_else(
                || {
                    Err(ComputerError::new(
                        "COMPUTER_CAPTURE_FAILED",
                        "The active share has no native capture stream",
                    ))
                },
                |capture| capture.prepare_for_target(&target),
            )
        };
        let geometry_changed = match geometry_changed {
            Ok(changed) => changed,
            Err(error) => return Some(self.fail_share_capture(error)),
        };
        if geometry_changed {
            // The share ID remains stable across an in-place native stream
            // update, but observation/action authority is geometry-scoped.
            // Revoke old frame IDs and any locally queued transport frame
            // before a sample from the new geometry epoch can be published.
            self.clear_frame_authority();
            if let Some(share) = self.share.as_mut() {
                share.mailbox.discard_pending();
            }
            // `native_frame` was taken before current geometry was enumerated.
            // It deliberately does not cross the just-created native epoch.
            return None;
        }
        debug_assert!(native_frame.source_sequence > last_source_sequence);
        // Let a native backend bind pixels to capture-time geometry when it
        // can prove that sample. Windows samples DWM bounds on both sides of
        // the WGC frame copy so later input rejects a moved/resized stale frame
        // instead of silently using newly enumerated coordinates.
        native_share::bind_frame_geometry(&native_frame, &mut target);
        match self.build_observation(
            target,
            windows,
            native_frame.image,
            native_frame.captured_at,
            Some(native_frame.source_sequence),
        ) {
            Ok(frame) => {
                let share = self.share.as_mut()?;
                let sequence = share.sequence;
                share.mailbox.produce(sequence, frame);
                None
            }
            Err(error) => Some(self.fail_share_capture(error)),
        }
    }

    /// Takes the newest parked share frame when ack pacing allows an emission.
    pub fn take_share_emission(&mut self) -> Option<(u64, Value)> {
        self.share.as_mut()?.mailbox.emit()
    }

    /// Applies a bridge share-frame acknowledgement; wrong-share and stale
    /// sequence identities are ignored.
    pub fn acknowledge_share_frame(&mut self, share_id: &str, sequence: u64) -> bool {
        self.share
            .as_mut()
            .is_some_and(|share| share.id == share_id && share.mailbox.acknowledge(sequence))
    }

    fn move_pointer(
        &mut self,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        let mut timer = ActionTimer::start(cancellation);
        let (frame, point) = self.point(params, "x", "y")?;
        let duration_ms = optional_duration(params, "durationMs", 50, MAX_CURSOR_DURATION_MS)?;
        let trajectory = self.cursor.plan(&frame, point, duration_ms, "move");
        timer.resolved();
        let invariants = match platform::move_pointer_path(
            &frame.target,
            &trajectory.points,
            trajectory.step_delay(),
            cancellation,
        )
        .and_then(|report| report.assert_held(InvariantStage::PointerTrajectory))
        {
            Ok(invariants) => invariants,
            Err(error) => {
                self.cursor.mark_unknown("moveFailed");
                return Err(error);
            }
        };
        if let Err(error) = cancellation.check("pointer state commit") {
            self.cursor.mark_unknown("moveCanceled");
            return Err(error);
        }
        self.cursor.commit(&frame, &trajectory, "move");
        self.cursor.settle("move");
        let evidence = invariant_evidence(&invariants);
        recorded_action_result(
            timer,
            ActionEffect::Unverifiable,
            evidence,
            &frame,
            point,
            invariants,
            json!({
                "pointer": self.cursor.snapshot(Some(&frame)),
                "motion": trajectory.metadata(),
            }),
        )
    }

    fn click(
        &mut self,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        let mut timer = ActionTimer::start(cancellation);
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
        let duration_ms = optional_duration(params, "durationMs", 50, MAX_CURSOR_DURATION_MS)?;
        let action = if button == "right" {
            "rightClick"
        } else if count > 1 {
            "doubleClick"
        } else {
            "click"
        };
        let trajectory = self.cursor.plan(&frame, point, duration_ms, action);
        timer.resolved();
        if let Err(error) = platform::move_pointer_path(
            &frame.target,
            &trajectory.points,
            trajectory.step_delay(),
            cancellation,
        )
        .and_then(|report| report.assert_held(InvariantStage::PointerTrajectory))
        {
            self.cursor.mark_unknown("clickMoveFailed");
            return Err(error);
        }
        cancellation.check("click pointer state commit")?;
        self.cursor.commit(&frame, &trajectory, "moveForClick");
        let invariants = platform::click(&frame.target, point, button, count, cancellation)?
            .assert_held(InvariantStage::ClickDispatch)?;
        cancellation.check("click result commit")?;
        self.cursor.settle(action);
        let evidence = invariant_evidence(&invariants);
        recorded_action_result(
            timer,
            ActionEffect::Unverifiable,
            evidence,
            &frame,
            point,
            invariants,
            json!({
                "button": button,
                "clickCount": count,
                "pointer": self.cursor.snapshot(Some(&frame)),
                "motion": trajectory.metadata(),
            }),
        )
    }

    fn drag(
        &mut self,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        let mut timer = ActionTimer::start(cancellation);
        let frame = self.verify_frame(params)?;
        let from = self.frame_point(&frame, params, "fromX", "fromY")?;
        let to = self.frame_point(&frame, params, "toX", "toY")?;
        let duration_ms = params
            .get("durationMs")
            .and_then(Value::as_u64)
            .unwrap_or(500)
            .clamp(50, MAX_DRAG_DURATION_MS);
        let trajectory = self
            .cursor
            .plan(&frame, from, Some(duration_ms.min(400)), "drag");
        timer.resolved();
        if let Err(error) = platform::move_pointer_path(
            &frame.target,
            &trajectory.points,
            trajectory.step_delay(),
            cancellation,
        )
        .and_then(|report| report.assert_held(InvariantStage::PointerTrajectory))
        {
            self.cursor.mark_unknown("dragApproachFailed");
            return Err(error);
        }
        cancellation.check("drag pointer state commit")?;
        self.cursor.commit(&frame, &trajectory, "dragApproach");
        let invariants = match platform::drag(&frame.target, from, to, duration_ms, cancellation)
            .and_then(|report| report.assert_held(InvariantStage::DragDispatch))
        {
            Ok(invariants) => invariants,
            Err(error) => {
                self.cursor.mark_unknown("dragFailed");
                return Err(error);
            }
        };
        cancellation.check("drag result commit")?;
        let arrival = self.cursor.plan(&frame, to, Some(50), "drag");
        self.cursor.commit(&frame, &arrival, "drag");
        self.cursor.settle("drag");
        let evidence = invariant_evidence(&invariants);
        let result = json!({
            "deliveryMode": "exact-window-background",
            "frameId": frame.id,
            "from": { "x": from.local_x, "y": from.local_y },
            "to": { "x": to.local_x, "y": to.local_y },
            "durationMs": duration_ms,
            "invariants": invariants,
            "pointer": self.cursor.snapshot(Some(&frame)),
            "motion": {
                "approach": trajectory.metadata(),
                "gesture": {
                    "curve": "minimum-jerk-drag",
                    "durationMs": duration_ms,
                    "arrivalSequence": arrival.sequence,
                    "arrivalAcknowledged": true
                }
            },
        });
        finish_action_record(timer, ActionEffect::Unverifiable, evidence, result)
    }

    fn scroll(
        &mut self,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        let mut timer = ActionTimer::start(cancellation);
        let (frame, point) = self.point(params, "x", "y")?;
        let delta_x = integer(params, "deltaX", 0)?.clamp(-50, 50) as i32;
        let delta_y = integer(params, "deltaY", 0)?.clamp(-50, 50) as i32;
        let trajectory = self.cursor.plan(&frame, point, None, "scroll");
        timer.resolved();
        if let Err(error) = platform::move_pointer_path(
            &frame.target,
            &trajectory.points,
            trajectory.step_delay(),
            cancellation,
        )
        .and_then(|report| report.assert_held(InvariantStage::PointerTrajectory))
        {
            self.cursor.mark_unknown("scrollMoveFailed");
            return Err(error);
        }
        cancellation.check("scroll pointer state commit")?;
        self.cursor.commit(&frame, &trajectory, "moveForScroll");
        let invariants = platform::scroll(&frame.target, point, delta_x, delta_y, cancellation)?
            .assert_held(InvariantStage::ScrollDispatch)?;
        cancellation.check("scroll result commit")?;
        self.cursor.settle("scroll");
        let evidence = invariant_evidence(&invariants);
        recorded_action_result(
            timer,
            ActionEffect::Unverifiable,
            evidence,
            &frame,
            point,
            invariants,
            json!({
                "deltaX": delta_x,
                "deltaY": delta_y,
                "pointer": self.cursor.snapshot(Some(&frame)),
                "motion": trajectory.metadata(),
            }),
        )
    }

    fn type_text(
        &mut self,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        let mut timer = ActionTimer::start(cancellation);
        let text = params
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("text must be a string"))?;
        // The helper repeats the server's native-only bound before consulting
        // frame authority, so an authenticated but incompatible peer cannot
        // turn one request into an unbounded native event/message burst.
        let utf16_units = validate_computer_type_text(text)?;
        let frame = self.verify_frame(params)?;
        timer.resolved();
        let invariants = platform::type_text(&frame.target, text, cancellation)?
            .assert_held(InvariantStage::TextDispatch)?;
        cancellation.check("text result commit")?;
        let evidence = invariant_evidence(&invariants);
        let result = json!({
            "deliveryMode": "exact-window-background",
            "frameId": frame.id,
            "characters": text.chars().count(),
            "utf16CodeUnits": utf16_units,
            "invariants": invariants,
        });
        finish_action_record(timer, ActionEffect::Unverifiable, evidence, result)
    }

    fn key(
        &mut self,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        let mut timer = ActionTimer::start(cancellation);
        let frame = self.verify_frame(params)?;
        let chord = params
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("key must be a string"))?;
        validate_key_chord(chord)?;
        timer.resolved();
        let invariants = platform::key(&frame.target, chord, cancellation)?
            .assert_held(InvariantStage::KeyDispatch)?;
        cancellation.check("key result commit")?;
        let evidence = invariant_evidence(&invariants);
        let result = json!({
            "deliveryMode": "exact-window-background",
            "frameId": frame.id,
            "key": chord,
            "invariants": invariants,
        });
        finish_action_record(timer, ActionEffect::Unverifiable, evidence, result)
    }

    fn invoke(
        &mut self,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        let mut timer = ActionTimer::start(cancellation);
        let frame = self.verify_frame(params)?;
        let target = semantic_target(&frame, params)?;
        let action = params
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("press");
        if !target
            .element
            .actions
            .iter()
            .any(|candidate| candidate == action)
        {
            return Err(invalid(format!(
                "Action {action} was not advertised for {}",
                target.element.reference
            )));
        }
        timer.resolved();
        let (backend_effect, invariants) =
            platform::invoke(&frame.target, &target, action, cancellation)?;
        cancellation.check("semantic invoke result commit")?;
        let (effect, mut evidence) = semantic_evidence(&backend_effect);
        evidence.extend(invariant_evidence(&invariants));
        let result = json!({
            "deliveryMode": "exact-window-semantic",
            "frameId": frame.id,
            "elementRef": target.element.reference,
            "action": action,
            "backendEffect": backend_effect,
            "invariants": invariants,
        });
        finish_action_record(timer, effect, evidence, result)
    }

    fn set_value(
        &mut self,
        params: &Value,
        cancellation: &CommandCancellation,
    ) -> Result<Value, ComputerError> {
        let mut timer = ActionTimer::start(cancellation);
        let frame = self.verify_frame(params)?;
        let target = semantic_target(&frame, params)?;
        let value = params
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("value must be a string"))?;
        if target.element.sensitive || target.element.value_redacted {
            return Err(ComputerError::new(
                "COMPUTER_SENSITIVE_ELEMENT",
                "Sensitive semantic values are redacted and cannot be set",
            ));
        }
        if !target
            .element
            .actions
            .iter()
            .any(|action| action == "setValue")
        {
            return Err(invalid(format!(
                "setValue was not advertised for {}",
                target.element.reference
            )));
        }
        timer.resolved();
        let (backend_effect, invariants) =
            platform::set_value(&frame.target, &target, value, cancellation)?;
        cancellation.check("semantic value result commit")?;
        let (effect, mut evidence) = semantic_evidence(&backend_effect);
        evidence.extend(invariant_evidence(&invariants));
        let result = json!({
            "deliveryMode": "exact-window-semantic",
            "frameId": frame.id,
            "elementRef": target.element.reference,
            "characters": value.chars().count(),
            "backendEffect": backend_effect,
            "invariants": invariants,
        });
        finish_action_record(timer, effect, evidence, result)
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
        let frame = self.requested_frame(supplied).ok_or_else(stale_frame)?;
        if let Some(expected_revision) = params
            .get("expectedPointerRevision")
            .and_then(Value::as_u64)
            && expected_revision != self.cursor.snapshot(Some(frame)).sequence
        {
            return Err(ComputerError::new(
                "COMPUTER_STALE_POINTER",
                "The synthetic pointer advanced after this request was prepared. Observe the exact window again before acting.",
            ));
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

    fn requested_frame(&self, supplied: &str) -> Option<&FrameState> {
        self.requested_frame_at(supplied, Instant::now())
    }

    fn requested_frame_at(&self, supplied: &str, now: Instant) -> Option<&FrameState> {
        let current = self.frame.as_ref()?;
        let frame = if supplied == current.id {
            if !frame_is_fresh(current, now) {
                return None;
            }
            match (current.share_id.as_deref(), current.source_sequence) {
                (None, None) => current,
                (Some(frame_share_id), Some(_)) => {
                    let share = self.share.as_ref()?;
                    if frame_share_id != share.id
                        || current.target.id != share.window_id
                        || current.target.pid != share.target_pid
                    {
                        return None;
                    }
                    current
                }
                // A partially populated provenance tuple is never valid.
                _ => return None,
            }
        } else {
            let share = self.share.as_ref()?;
            self.recent_frames.iter().find(|candidate| {
                candidate.id == supplied
                    && frame_is_fresh(candidate, now)
                    && candidate.share_id.as_deref() == Some(share.id.as_str())
                    && candidate.target.id == share.window_id
                    && candidate.target.id == current.target.id
                    && candidate.target.pid == current.target.pid
                    && candidate.target.x == current.target.x
                    && candidate.target.y == current.target.y
                    && candidate.target.width == current.target.width
                    && candidate.target.height == current.target.height
            })?
        };
        Some(frame)
    }
}

fn frame_is_fresh(frame: &FrameState, now: Instant) -> bool {
    now.saturating_duration_since(frame.captured_at) <= MAX_FRAME_AGE
}

fn semantic_target(frame: &FrameState, params: &Value) -> Result<SemanticTarget, ComputerError> {
    let reference = params
        .get("elementRef")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("elementRef must be a string"))?;
    frame
        .elements
        .iter()
        .find(|target| target.element.reference == reference)
        .cloned()
        .ok_or_else(|| {
            ComputerError::new(
                "COMPUTER_STALE_ELEMENT",
                "The semantic element ref is missing or stale. Observe the exact window again.",
            )
        })
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
        "shareId": frame.share_id,
        "sourceSequence": frame.source_sequence,
        "x": point.local_x,
        "y": point.local_y,
        "invariants": invariants,
    });
    if let (Some(target), Some(extra)) = (result.as_object_mut(), extra.as_object()) {
        target.extend(extra.clone());
    }
    result
}

fn recorded_action_result(
    timer: ActionTimer,
    effect: ActionEffect,
    evidence: Vec<ActionEvidence>,
    frame: &FrameState,
    point: TargetPoint,
    invariants: InvariantReport,
    extra: Value,
) -> Result<Value, ComputerError> {
    finish_action_record(
        timer,
        effect,
        evidence,
        action_result(frame, point, invariants, extra),
    )
}

fn finish_action_record(
    timer: ActionTimer,
    effect: ActionEffect,
    evidence: Vec<ActionEvidence>,
    mut result: Value,
) -> Result<Value, ComputerError> {
    let record: ActionRecord = timer.finish(effect, evidence)?;
    record.insert_into(&mut result)?;
    Ok(result)
}

fn available_windows() -> Result<Vec<WindowDescriptor>, ComputerError> {
    platform::windows(MAX_WINDOWS)
}

/// Command methods plus negotiated feature flags advertised in the hello.
fn advertised_capabilities() -> Vec<&'static str> {
    let mut capabilities = COMPUTER_METHODS.to_vec();
    capabilities.push(COMPUTER_SHARE_ACK_CAPABILITY);
    capabilities.push(COMPUTER_NATIVE_SHARE_CAPABILITY);
    capabilities
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

fn screen_bounds_to_image(
    bounds: &SemanticBounds,
    target: &WindowDescriptor,
    image_width: u32,
    image_height: u32,
) -> Option<SemanticBounds> {
    let scale_x = f64::from(image_width) / f64::from(target.width.max(1));
    let scale_y = f64::from(image_height) / f64::from(target.height.max(1));
    let x = (bounds.x - f64::from(target.x)) * scale_x;
    let y = (bounds.y - f64::from(target.y)) * scale_y;
    let width = bounds.width * scale_x;
    let height = bounds.height * scale_y;
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return None;
    }
    Some(SemanticBounds {
        x,
        y,
        width,
        height,
    })
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

fn optional_duration(
    params: &Value,
    name: &str,
    minimum: u64,
    maximum: u64,
) -> Result<Option<u64>, ComputerError> {
    let Some(value) = params.get(name) else {
        return Ok(None);
    };
    let value = value
        .as_u64()
        .ok_or_else(|| invalid(format!("{name} must be an integer")))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(invalid(format!(
            "{name} must be between {minimum} and {maximum}"
        )));
    }
    Ok(Some(value))
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

fn set_semantic_truncation_metadata(frame: &mut Value, reason: Option<&str>) {
    let frame = frame
        .as_object_mut()
        .expect("semantic truncation metadata requires a frame object");
    frame.insert(
        "semanticTruncated".to_owned(),
        Value::Bool(reason.is_some()),
    );
    if let Some(reason) = reason {
        frame.insert(
            "semanticTruncationReason".to_owned(),
            Value::String(reason.to_owned()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn helper_rejects_unbounded_native_text_before_frame_resolution() {
        let cancellation = CommandCancellation::new();
        let mut controller = ComputerController::new();
        let error = controller
            .execute_cancellable(
                "computer.typeText",
                &json!({
                    "frameId": "missing",
                    "text": "a".repeat(COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS + 1),
                }),
                &cancellation,
            )
            .unwrap_err();
        assert_eq!(error.code, "COMPUTER_INVALID_REQUEST");
        assert!(!cancellation.was_dispatched());
    }

    #[test]
    fn native_element_identity_is_never_serialized() {
        let element = SemanticElement {
            reference: "u1".to_owned(),
            role: "button".to_owned(),
            name: "Apply".to_owned(),
            value: None,
            sensitive: false,
            value_redacted: false,
            enabled: Some(true),
            actions: vec!["press".to_owned()],
            bounds: None,
            coordinate_space: "screen-pixels".to_owned(),
            screen_bounds: None,
            native_identity: Some(NativeElementIdentity {
                automation_id: "native-automation-secret".to_owned(),
                class_name: "Button".to_owned(),
                framework_id: "Win32".to_owned(),
                control_type: 50_000,
                provider_process_id: 42,
                capability_mask: 1,
            }),
        };
        let encoded = serde_json::to_value(element).unwrap();
        assert!(encoded.get("nativeIdentity").is_none());
        assert!(!encoded.to_string().contains("automation_id"));
        assert!(!encoded.to_string().contains("native-automation-secret"));
    }

    #[test]
    fn complete_semantic_snapshots_omit_a_truncation_reason() {
        let mut complete = json!({});
        set_semantic_truncation_metadata(&mut complete, None);
        assert_eq!(complete["semanticTruncated"], false);
        assert!(complete.get("semanticTruncationReason").is_none());

        let mut partial = json!({});
        set_semantic_truncation_metadata(&mut partial, Some("node_budget"));
        assert_eq!(partial["semanticTruncated"], true);
        assert_eq!(partial["semanticTruncationReason"], "node_budget");
    }

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
            elements: vec![],
            captured_at: Instant::now(),
            share_id: None,
            source_sequence: None,
        }
    }

    fn test_share(ack_paced: bool) -> ShareSession {
        ShareSession {
            id: "share-1".to_owned(),
            window_id: "window-1".to_owned(),
            target_pid: 42,
            fps: 4,
            sequence: 0,
            source_sequence: 0,
            started_at: now_iso(),
            capture: None,
            mailbox: ShareMailbox::new(ack_paced),
        }
    }

    #[test]
    fn cancellation_before_dispatch_never_claims_an_unknown_side_effect() {
        let cancellation = CommandCancellation::new();
        cancellation.cancel();
        let error = cancellation.begin_side_effect("test dispatch").unwrap_err();
        assert_eq!(error.code, "COMPUTER_CANCELED");
        assert!(!cancellation.was_dispatched());
    }

    #[test]
    fn cancellation_during_a_long_path_stops_at_the_next_boundary_as_unknown() {
        let cancellation = CommandCancellation::new();
        let worker_cancellation = cancellation.clone();
        let dispatched = Arc::new(AtomicUsize::new(0));
        let worker_dispatched = Arc::clone(&dispatched);
        let (step_tx, step_rx) = std::sync::mpsc::sync_channel(0);
        let (continue_tx, continue_rx) = std::sync::mpsc::sync_channel(0);
        let worker = std::thread::spawn(move || {
            for step in 0..100 {
                worker_cancellation.begin_side_effect("path step")?;
                worker_dispatched.fetch_add(1, Ordering::SeqCst);
                step_tx.send(step).unwrap();
                continue_rx.recv().unwrap();
            }
            Ok::<(), ComputerError>(())
        });

        for expected in 0..3 {
            assert_eq!(step_rx.recv().unwrap(), expected);
            if expected < 2 {
                continue_tx.send(()).unwrap();
            }
        }
        cancellation.cancel();
        continue_tx.send(()).unwrap();
        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(error.code, "COMPUTER_OUTCOME_UNKNOWN");
        assert_eq!(dispatched.load(Ordering::SeqCst), 3);
        assert!(cancellation.was_dispatched());
    }

    #[test]
    fn resetting_transport_session_revokes_share_and_stale_frames() {
        let mut controller = ComputerController::new();
        controller.set_share_ack_pacing(true);
        controller.frame = Some(frame());
        controller.recent_frames.push_back(frame());
        let mut share = test_share(true);
        share.sequence = 9;
        controller.share = Some(share);

        controller.reset_transport_session();

        assert!(controller.share.is_none());
        assert!(
            !controller.share_ack_paced,
            "a replacement transport session must renegotiate ack pacing"
        );
        assert!(controller.frame.is_none());
        assert!(controller.recent_frames.is_empty());
        assert_eq!(controller.share_status_value(), json!({ "active": false }));
    }

    #[test]
    fn canceled_command_revocation_keeps_the_negotiated_ack_pacing() {
        let mut controller = ComputerController::new();
        controller.set_share_ack_pacing(true);
        controller.frame = Some(frame());
        controller.recent_frames.push_back(frame());
        let mut share = test_share(true);
        share.sequence = 9;
        controller.share = Some(share);

        controller.revoke_command_authority();

        assert!(controller.share.is_none());
        assert!(controller.frame.is_none());
        assert!(controller.recent_frames.is_empty());
        assert!(
            controller.share_ack_paced,
            "a canceled command must not discard the session's hello-negotiated pacing"
        );
        // A later share in the same session stays ack-paced as negotiated.
        let cancellation = CommandCancellation::new();
        let _ = controller.share_start(&json!({ "windowId": "window-1", "fps": 4 }), &cancellation);
        if let Some(share) = controller.share.as_ref() {
            assert!(share.mailbox.ack_paced());
        }
    }

    #[test]
    fn outcome_unknown_synchronously_revokes_share_frame_and_pointer_authority() {
        let mut controller = ComputerController::new();
        controller.set_share_ack_pacing(true);
        controller.frame = Some(frame());
        controller.recent_frames.push_back(frame());
        controller.share = Some(test_share(true));
        let previous_cursor_id = controller.cursor.snapshot(None).id;
        let cancellation = CommandCancellation::new();
        cancellation
            .begin_side_effect("fixture native dispatch")
            .expect("fixture dispatch should begin");
        cancellation.cancel();

        let error = controller
            .finish_command_result(&cancellation, Ok(json!({ "dispatched": true })))
            .expect_err("a canceled dispatched effect has an unknown outcome");

        assert_eq!(error.code, "COMPUTER_OUTCOME_UNKNOWN");
        assert!(controller.share.is_none());
        assert!(controller.frame.is_none());
        assert!(controller.recent_frames.is_empty());
        assert_ne!(controller.cursor.snapshot(None).id, previous_cursor_id);
        assert!(
            controller.share_ack_paced,
            "revocation must retain the transport session's negotiated pacing"
        );
    }

    #[test]
    fn share_status_reports_ack_pacing_and_drop_metrics_from_the_mailbox() {
        let mut controller = ComputerController::new();
        controller.share = Some(test_share(true));
        {
            let mailbox = &mut controller.share.as_mut().unwrap().mailbox;
            assert!(mailbox.produce(1, json!({ "capture": 1 })));
            assert!(mailbox.produce(2, json!({ "capture": 2 })));
        }

        let (sequence, _) = controller.take_share_emission().unwrap();
        assert_eq!(sequence, 2, "only the newest capture is emitted");
        controller
            .share
            .as_mut()
            .unwrap()
            .mailbox
            .produce(3, json!({ "capture": 3 }));
        assert!(
            controller.take_share_emission().is_none(),
            "the next emission waits for the ack"
        );
        assert!(
            !controller.acknowledge_share_frame("share-1", 1),
            "stale ack ignored"
        );
        assert!(controller.acknowledge_share_frame("share-1", 2));
        assert_eq!(controller.take_share_emission().unwrap().0, 3);

        let status = controller.share_status_value();
        assert_eq!(status["active"], true);
        assert_eq!(status["ackPaced"], true);
        assert_eq!(status["droppedFrames"], 1);
        assert_eq!(status["lastAckedSequence"], 2);
        assert_eq!(status["backpressure"], "latest-frame-wins");
    }

    #[test]
    fn acknowledgement_from_a_replaced_share_cannot_release_current_backpressure() {
        let mut controller = ComputerController::new();
        controller.share = Some(test_share(true));
        let mailbox = &mut controller.share.as_mut().unwrap().mailbox;
        assert!(mailbox.produce(1, json!({ "capture": 1 })));
        assert_eq!(controller.take_share_emission().unwrap().0, 1);
        assert!(
            !controller.acknowledge_share_frame("replaced-share", 1),
            "an equal sequence from another share epoch is stale"
        );
        controller
            .share
            .as_mut()
            .unwrap()
            .mailbox
            .produce(2, json!({ "capture": 2 }));
        assert!(
            controller.take_share_emission().is_none(),
            "the current share must remain paced until its own ack arrives"
        );
        assert!(controller.acknowledge_share_frame("share-1", 1));
        assert_eq!(controller.take_share_emission().unwrap().0, 2);
    }

    #[test]
    fn missing_native_stream_revokes_share_and_frame_authority() {
        let mut controller = ComputerController::new();
        controller.frame = Some(frame());
        controller.recent_frames.push_back(frame());
        controller.share = Some(test_share(true));

        let error = controller
            .pump_share_capture()
            .expect("missing native source must fail closed");

        assert_eq!(error.code, "COMPUTER_CAPTURE_FAILED");
        assert!(controller.share.is_none());
        assert!(controller.frame.is_none());
        assert!(controller.recent_frames.is_empty());
    }

    #[test]
    fn hello_advertises_share_ack_pacing_as_a_capability_not_a_method() {
        let capabilities = advertised_capabilities();
        assert!(capabilities.contains(&COMPUTER_SHARE_ACK_CAPABILITY));
        assert!(capabilities.contains(&COMPUTER_NATIVE_SHARE_CAPABILITY));
        assert!(!COMPUTER_METHODS.contains(&COMPUTER_SHARE_ACK_CAPABILITY));
        assert!(!COMPUTER_METHODS.contains(&COMPUTER_NATIVE_SHARE_CAPABILITY));
        let error = ComputerController::new()
            .execute(COMPUTER_SHARE_ACK_CAPABILITY, &json!({}))
            .unwrap_err();
        assert_eq!(error.code, "COMPUTER_UNSUPPORTED_ACTION");
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
        let error = report
            .assert_held(InvariantStage::ClickDispatch)
            .unwrap_err();
        assert_eq!(error.code, "COMPUTER_BACKGROUND_CONTRACT_VIOLATION");
        assert_eq!(
            error.message,
            "stage=clickDispatch;failedInvariants=hardwareCursorUnchanged"
        );
    }

    #[test]
    fn invariant_diagnostics_use_only_ordered_closed_vocabulary() {
        let report = InvariantReport {
            foreground_unchanged: false,
            user_focus_unchanged: false,
            cursor_unchanged: false,
            space_unchanged: false,
        };
        let error = report
            .assert_held(InvariantStage::PointerTrajectory)
            .unwrap_err();
        assert_eq!(
            error.message,
            "stage=pointerTrajectory;failedInvariants=foregroundUnchanged,userFocusUnchanged,hardwareCursorUnchanged,desktopSpaceUnchanged"
        );

        let stages = [
            InvariantStage::PointerTrajectory,
            InvariantStage::ClickDispatch,
            InvariantStage::DragDispatch,
            InvariantStage::ScrollDispatch,
            InvariantStage::TextDispatch,
            InvariantStage::KeyDispatch,
            InvariantStage::SemanticInvoke,
            InvariantStage::SemanticSetValue,
        ];
        assert_eq!(
            stages.map(InvariantStage::as_str),
            [
                "pointerTrajectory",
                "clickDispatch",
                "dragDispatch",
                "scrollDispatch",
                "textDispatch",
                "keyDispatch",
                "semanticInvoke",
                "semanticSetValue",
            ]
        );

        #[cfg(target_os = "macos")]
        assert_eq!(
            [
                InvariantStage::FocusPreparation,
                InvariantStage::FocusRestore,
                InvariantStage::FocusRecovery,
            ]
            .map(InvariantStage::as_str),
            ["focusPreparation", "focusRestore", "focusRecovery"]
        );

        #[cfg(target_os = "macos")]
        let focus_error = background_contract_violation(
            InvariantStage::FocusRestore,
            [InvariantFailure::UserFocus],
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            focus_error.message,
            "stage=focusRestore;failedInvariants=userFocusUnchanged"
        );
    }

    #[test]
    fn unverified_delivery_result_carries_a_non_confirming_action_record() {
        let cancellation = CommandCancellation::new();
        let mut timer = ActionTimer::start(&cancellation);
        timer.resolved();
        cancellation
            .begin_side_effect("test action dispatch")
            .unwrap();
        cancellation.mark_verification_started();
        let invariants = InvariantReport {
            foreground_unchanged: true,
            user_focus_unchanged: true,
            cursor_unchanged: true,
            space_unchanged: true,
        };
        let evidence = invariant_evidence(&invariants);
        let result = recorded_action_result(
            timer,
            ActionEffect::Unverifiable,
            evidence,
            &frame(),
            TargetPoint {
                local_x: 10,
                local_y: 20,
                screen_x: -90,
                screen_y: 40,
            },
            invariants,
            json!({}),
        )
        .unwrap();

        assert!(result.get("actionId").and_then(Value::as_str).is_some());
        assert_eq!(
            result.get("effect").and_then(Value::as_str),
            Some("Unverifiable")
        );
        assert!(
            result
                .pointer("/timings/totalMs")
                .and_then(Value::as_f64)
                .is_some()
        );
        assert!(
            result["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["supportsConfirmation"] == false)
        );
    }

    #[test]
    fn active_share_accepts_only_recent_frames_with_unchanged_exact_window_geometry() {
        let mut controller = ComputerController::new();
        let now = Instant::now();
        let mut current = frame();
        current.id = "frame-current".to_owned();
        current.captured_at = now;
        current.share_id = Some("share-1".to_owned());
        current.source_sequence = Some(2);
        let mut recent = current.clone();
        recent.id = "frame-rendered".to_owned();
        recent.captured_at = now - MAX_FRAME_AGE;
        recent.source_sequence = Some(1);
        controller.frame = Some(current);
        controller.recent_frames.push_front(recent);

        assert!(
            controller
                .requested_frame_at("frame-rendered", now)
                .is_none()
        );
        let mut share = test_share(false);
        share.fps = 10;
        share.sequence = 2;
        controller.share = Some(share);

        assert!(
            controller
                .requested_frame_at("frame-rendered", now)
                .is_some()
        );

        controller.recent_frames.front_mut().unwrap().captured_at =
            now - MAX_FRAME_AGE - Duration::from_nanos(1);
        assert!(
            controller
                .requested_frame_at("frame-rendered", now)
                .is_none()
        );
        controller.recent_frames.front_mut().unwrap().captured_at = now;

        controller.frame.as_mut().unwrap().target.width += 1;
        assert!(
            controller
                .requested_frame_at("frame-rendered", now)
                .is_none()
        );
    }

    #[test]
    fn current_share_frame_requires_the_active_share_identity_but_one_shot_does_not() {
        let mut controller = ComputerController::new();
        let now = Instant::now();
        let mut current = frame();
        current.id = "current-share-frame".to_owned();
        current.share_id = Some("old-share".to_owned());
        current.source_sequence = Some(7);
        controller.frame = Some(current);
        controller.share = Some(test_share(false));

        assert!(
            controller
                .requested_frame_at("current-share-frame", now)
                .is_none(),
            "a current frame from a replaced share must lose authority"
        );

        controller.frame.as_mut().unwrap().share_id = Some("share-1".to_owned());
        assert!(
            controller
                .requested_frame_at("current-share-frame", now)
                .is_some(),
            "a current frame from the active exact-window share remains usable"
        );

        controller.frame.as_mut().unwrap().share_id = None;
        controller.frame.as_mut().unwrap().source_sequence = None;
        assert!(
            controller
                .requested_frame_at("current-share-frame", now)
                .is_some(),
            "one-shot observations remain valid even while a share is active"
        );
    }

    #[test]
    fn share_commit_clears_frames_from_the_previous_authority_epoch() {
        let mut controller = ComputerController::new();
        controller.frame = Some(frame());
        controller.recent_frames.push_back(frame());
        let previous_cursor_id = controller.cursor.snapshot(None).id;

        controller.commit_share_replacement(test_share(false));

        assert!(controller.frame.is_none());
        assert!(controller.recent_frames.is_empty());
        assert_ne!(controller.cursor.snapshot(None).id, previous_cursor_id);
    }

    #[test]
    fn current_frame_lease_has_an_inclusive_three_second_boundary() {
        let mut controller = ComputerController::new();
        let now = Instant::now();
        let mut current = frame();
        current.captured_at = now - MAX_FRAME_AGE;
        let frame_id = current.id.clone();
        controller.frame = Some(current);

        assert!(controller.requested_frame_at(&frame_id, now).is_some());

        controller.frame.as_mut().unwrap().captured_at =
            now - MAX_FRAME_AGE - Duration::from_nanos(1);
        assert!(controller.requested_frame_at(&frame_id, now).is_none());
    }

    #[test]
    fn expired_current_frame_rejects_mutation_before_any_dispatch() {
        let mut controller = ComputerController::new();
        let mut expired = frame();
        expired.captured_at = Instant::now() - MAX_FRAME_AGE - Duration::from_millis(1);
        let frame_id = expired.id.clone();
        controller.frame = Some(expired);
        let cancellation = CommandCancellation::new();

        let error = controller
            .execute_cancellable(
                "computer.move",
                &json!({ "frameId": frame_id, "x": 10, "y": 10 }),
                &cancellation,
            )
            .unwrap_err();

        assert_eq!(error.code, "COMPUTER_STALE_FRAME");
        assert!(!cancellation.was_dispatched());
    }
}
