use std::time::Instant;

use serde::Serialize;
use serde_json::{Map, Value, json};
use thiserror::Error;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use {
    base64::Engine as _, base64::engine::general_purpose::STANDARD as BASE64_STANDARD,
    image::imageops::FilterType, std::io::Cursor, std::thread, std::time::Duration,
    time::OffsetDateTime, uuid::Uuid, xcap::Monitor,
};

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

#[cfg(any(target_os = "macos", target_os = "windows"))]
const MAX_CAPTURE_PIXELS: u64 = 1_000_000;
#[cfg(any(target_os = "macos", target_os = "windows"))]
const MAX_DISPLAYS: usize = 16;
#[cfg(any(target_os = "macos", target_os = "windows"))]
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
pub struct DisplayDescriptor {
    pub id: String,
    pub index: usize,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub rotation: f32,
    pub primary: bool,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Debug, Clone)]
struct FrameState {
    id: String,
    display: DisplayDescriptor,
    image_width: u32,
    image_height: u32,
}

pub struct ComputerController {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    frame: Option<FrameState>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    input: Option<Enigo>,
}

impl Default for ComputerController {
    fn default() -> Self {
        Self::new()
    }
}

impl ComputerController {
    pub fn new() -> Self {
        Self {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            frame: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            input: input_without_prompt(),
        }
    }

    pub fn hello(&mut self) -> Value {
        json!({
            "type": "hello",
            "version": crate::VERSION,
            "platform": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "backend": backend_name(),
            "inputReady": self.input_ready(),
            "capabilities": COMPUTER_METHODS,
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
            "computer.move" => self.move_mouse(params),
            "computer.click" => self.click(params),
            "computer.drag" => self.drag(params),
            "computer.scroll" => self.scroll(params),
            "computer.typeText" => self.type_text(params),
            "computer.key" => self.key(params),
            _ => unreachable!(),
        }
    }

    pub fn request_permissions(&mut self) -> Value {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let capture_ready = primary_monitor()
            .and_then(|(_, monitor)| {
                monitor.capture_image().map_err(|error| {
                    ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string())
                })
            })
            .is_ok();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let capture_ready = false;
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.input = Enigo::new(&Settings::default()).ok();
        }
        json!({
            "platform": std::env::consts::OS,
            "screenCaptureReady": capture_ready,
            "inputReady": self.input_ready(),
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
            "note": "Capture, resize, PNG encode, and base64; no model inference or network time",
        }))
    }

    fn status(&mut self) -> Result<Value, ComputerError> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let displays = displays()?;
            Ok(json!({
                "platform": std::env::consts::OS,
                "architecture": std::env::consts::ARCH,
                "backend": backend_name(),
                "inputReady": self.input_ready(),
                "displayCount": displays.len(),
                "displays": displays,
                "frameReady": self.frame.is_some(),
            }))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Err(unsupported_platform())
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn observe(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let requested_id = params.get("displayId").and_then(Value::as_str);
        let monitors = Monitor::all()
            .map_err(|error| ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string()))?;
        let mut candidates = monitors
            .into_iter()
            .take(MAX_DISPLAYS)
            .enumerate()
            .map(|(index, monitor)| descriptor(index, &monitor).map(|item| (item, monitor)))
            .collect::<Result<Vec<_>, _>>()?;
        if candidates.is_empty() {
            return Err(ComputerError::new(
                "COMPUTER_NO_DISPLAY",
                "No active display is available",
            ));
        }
        let selected_index = requested_id
            .and_then(|id| candidates.iter().position(|(display, _)| display.id == id))
            .or_else(|| candidates.iter().position(|(display, _)| display.primary))
            .unwrap_or(0);
        let (display, monitor) = candidates.swap_remove(selected_index);
        let image = monitor.capture_image().map_err(|error| {
            ComputerError::new(
                "COMPUTER_CAPTURE_FAILED",
                format!(
                    "Screen capture failed. On macOS, grant Screen Recording to Local Computer Helper. {error}"
                ),
            )
        })?;
        let image = resize_for_transport(image);
        let image_width = image.width();
        let image_height = image.height();
        let mut png = Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string()))?;
        let frame_id = Uuid::new_v4().to_string();
        self.frame = Some(FrameState {
            id: frame_id.clone(),
            display: display.clone(),
            image_width,
            image_height,
        });
        let all_displays = candidates
            .into_iter()
            .map(|(descriptor, _)| descriptor)
            .chain(std::iter::once(display.clone()))
            .collect::<Vec<_>>();
        Ok(json!({
            "screenshot": format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png.into_inner())),
            "frame": {
                "id": frame_id,
                "capturedAt": now_iso(),
                "displayId": display.id,
                "displayIndex": display.index,
                "displayName": display.name,
                "imageWidth": image_width,
                "imageHeight": image_height,
                "screenX": display.x,
                "screenY": display.y,
                "screenWidth": display.width,
                "screenHeight": display.height,
                "scaleFactor": display.scale_factor,
                "rotation": display.rotation,
            },
            "displays": all_displays,
        }))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn observe(&mut self, _params: &Value) -> Result<Value, ComputerError> {
        Err(unsupported_platform())
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn move_mouse(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let (x, y) = self.point(params, "x", "y")?;
        self.input()?
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(input_error)?;
        Ok(json!({ "x": x, "y": y }))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn move_mouse(&mut self, _params: &Value) -> Result<Value, ComputerError> {
        Err(unsupported_platform())
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn click(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let (x, y) = self.point(params, "x", "y")?;
        let button_name = params
            .get("button")
            .and_then(Value::as_str)
            .unwrap_or("left");
        let button = parse_button(button_name)?;
        let count = params
            .get("clickCount")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .clamp(1, 3);
        let input = self.input()?;
        input
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(input_error)?;
        for index in 0..count {
            input
                .button(button, Direction::Click)
                .map_err(input_error)?;
            if index + 1 < count {
                thread::sleep(Duration::from_millis(60));
            }
        }
        Ok(json!({ "x": x, "y": y, "button": button_name, "clickCount": count }))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn click(&mut self, _params: &Value) -> Result<Value, ComputerError> {
        Err(unsupported_platform())
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn drag(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let (from_x, from_y) = self.point(params, "fromX", "fromY")?;
        let (to_x, to_y) = self.point(params, "toX", "toY")?;
        let duration_ms = params
            .get("durationMs")
            .and_then(Value::as_u64)
            .unwrap_or(500)
            .clamp(50, MAX_DRAG_DURATION_MS);
        let steps = (duration_ms / 16).clamp(4, 120);
        let input = self.input()?;
        input
            .move_mouse(from_x, from_y, Coordinate::Abs)
            .map_err(input_error)?;
        input
            .button(Button::Left, Direction::Press)
            .map_err(input_error)?;
        let result = (|| {
            for step in 1..=steps {
                let progress = step as f64 / steps as f64;
                let x = from_x as f64 + (to_x - from_x) as f64 * progress;
                let y = from_y as f64 + (to_y - from_y) as f64 * progress;
                input
                    .move_mouse(x.round() as i32, y.round() as i32, Coordinate::Abs)
                    .map_err(input_error)?;
                thread::sleep(Duration::from_millis(duration_ms / steps));
            }
            Ok::<_, ComputerError>(())
        })();
        let release = input
            .button(Button::Left, Direction::Release)
            .map_err(input_error);
        result?;
        release?;
        Ok(json!({
            "from": { "x": from_x, "y": from_y },
            "to": { "x": to_x, "y": to_y },
            "durationMs": duration_ms,
        }))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn drag(&mut self, _params: &Value) -> Result<Value, ComputerError> {
        Err(unsupported_platform())
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn scroll(&mut self, params: &Value) -> Result<Value, ComputerError> {
        let (x, y) = self.point(params, "x", "y")?;
        let delta_x = integer(params, "deltaX", 0)?.clamp(-50, 50);
        let delta_y = integer(params, "deltaY", 0)?.clamp(-50, 50);
        let input = self.input()?;
        input
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(input_error)?;
        if delta_y != 0 {
            input
                .scroll(delta_y as i32, Axis::Vertical)
                .map_err(input_error)?;
        }
        if delta_x != 0 {
            input
                .scroll(delta_x as i32, Axis::Horizontal)
                .map_err(input_error)?;
        }
        Ok(json!({ "x": x, "y": y, "deltaX": delta_x, "deltaY": delta_y }))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn scroll(&mut self, _params: &Value) -> Result<Value, ComputerError> {
        Err(unsupported_platform())
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn type_text(&mut self, params: &Value) -> Result<Value, ComputerError> {
        self.verify_frame(params)?;
        let text = params
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("text must be a string"))?
            .to_owned();
        self.input()?.text(&text).map_err(input_error)?;
        Ok(json!({ "characters": text.chars().count() }))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn type_text(&mut self, _params: &Value) -> Result<Value, ComputerError> {
        Err(unsupported_platform())
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn key(&mut self, params: &Value) -> Result<Value, ComputerError> {
        self.verify_frame(params)?;
        let chord = params
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("key must be a string"))?
            .to_owned();
        let keys = parse_key_chord(&chord)?;
        let (last, modifiers) = keys
            .split_last()
            .ok_or_else(|| invalid("key must not be empty"))?;
        let input = self.input()?;
        let mut pressed = Vec::new();
        for modifier in modifiers {
            if let Err(error) = input.key(*modifier, Direction::Press) {
                for key in pressed.iter().rev() {
                    let _ = input.key(*key, Direction::Release);
                }
                return Err(input_error(error));
            }
            pressed.push(*modifier);
        }
        let clicked = input.key(*last, Direction::Click).map_err(input_error);
        let mut release_error = None;
        for key in pressed.iter().rev() {
            if let Err(error) = input.key(*key, Direction::Release) {
                release_error.get_or_insert_with(|| input_error(error));
            }
        }
        clicked?;
        if let Some(error) = release_error {
            return Err(error);
        }
        Ok(json!({ "key": chord }))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn key(&mut self, _params: &Value) -> Result<Value, ComputerError> {
        Err(unsupported_platform())
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn point(
        &self,
        params: &Value,
        x_name: &str,
        y_name: &str,
    ) -> Result<(i32, i32), ComputerError> {
        let frame = self.verify_frame(params)?;
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

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn verify_frame(&self, params: &Value) -> Result<&FrameState, ComputerError> {
        let supplied = params.get("frameId").and_then(Value::as_str).unwrap_or("");
        let frame = self.frame.as_ref().ok_or_else(stale_frame)?;
        if supplied != frame.id {
            return Err(stale_frame());
        }
        let live = displays()?
            .into_iter()
            .find(|display| display.id == frame.display.id)
            .ok_or_else(stale_frame)?;
        if live.x != frame.display.x
            || live.y != frame.display.y
            || live.width != frame.display.width
            || live.height != frame.display.height
            || (live.scale_factor - frame.display.scale_factor).abs() > f32::EPSILON
            || (live.rotation - frame.display.rotation).abs() > f32::EPSILON
        {
            return Err(stale_frame());
        }
        Ok(frame)
    }

    fn input_ready(&mut self) -> bool {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            if self.input.is_none() {
                self.input = input_without_prompt();
            }
            self.input.is_some()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            false
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn input(&mut self) -> Result<&mut Enigo, ComputerError> {
        if self.input.is_none() {
            self.input = Enigo::new(&Settings::default()).ok();
        }
        self.input.as_mut().ok_or_else(|| {
            ComputerError::new(
                "COMPUTER_PERMISSION_REQUIRED",
                "Input control is unavailable. On macOS, grant Accessibility to Local Computer Helper, then retry.",
            )
        })
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn input_without_prompt() -> Option<Enigo> {
    let settings = Settings {
        open_prompt_to_get_permissions: false,
        ..Settings::default()
    };
    Enigo::new(&settings).ok()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn displays() -> Result<Vec<DisplayDescriptor>, ComputerError> {
    Monitor::all()
        .map_err(|error| ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string()))?
        .into_iter()
        .take(MAX_DISPLAYS)
        .enumerate()
        .map(|(index, monitor)| descriptor(index, &monitor))
        .collect()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn primary_monitor() -> Result<(DisplayDescriptor, Monitor), ComputerError> {
    let monitors = Monitor::all()
        .map_err(|error| ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string()))?;
    let mut fallback = None;
    for (index, monitor) in monitors.into_iter().take(MAX_DISPLAYS).enumerate() {
        let display = descriptor(index, &monitor)?;
        if display.primary {
            return Ok((display, monitor));
        }
        fallback.get_or_insert((display, monitor));
    }
    fallback
        .ok_or_else(|| ComputerError::new("COMPUTER_NO_DISPLAY", "No active display is available"))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn descriptor(index: usize, monitor: &Monitor) -> Result<DisplayDescriptor, ComputerError> {
    let capture_error =
        |error: xcap::XCapError| ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string());
    Ok(DisplayDescriptor {
        id: monitor.id().map_err(capture_error)?.to_string(),
        index,
        name: monitor.friendly_name().map_err(capture_error)?,
        x: monitor.x().map_err(capture_error)?,
        y: monitor.y().map_err(capture_error)?,
        width: monitor.width().map_err(capture_error)?,
        height: monitor.height().map_err(capture_error)?,
        scale_factor: monitor.scale_factor().map_err(capture_error)?,
        rotation: monitor.rotation().map_err(capture_error)?,
        primary: monitor.is_primary().map_err(capture_error)?,
    })
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn map_image_point(frame: &FrameState, x: f64, y: f64) -> (i32, i32) {
    let relative_x = (x / frame.image_width as f64 * frame.display.width as f64)
        .round()
        .clamp(0.0, frame.display.width.saturating_sub(1) as f64);
    let relative_y = (y / frame.image_height as f64 * frame.display.height as f64)
        .round()
        .clamp(0.0, frame.display.height.saturating_sub(1) as f64);
    (
        frame.display.x.saturating_add(relative_x as i32),
        frame.display.y.saturating_add(relative_y as i32),
    )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn parse_button(value: &str) -> Result<Button, ComputerError> {
    match value {
        "left" => Ok(Button::Left),
        "middle" => Ok(Button::Middle),
        "right" => Ok(Button::Right),
        _ => Err(invalid("button must be left, middle, or right")),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn parse_key_chord(value: &str) -> Result<Vec<Key>, ComputerError> {
    let parts = value
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 8 {
        return Err(invalid("key must contain between 1 and 8 chord parts"));
    }
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| parse_key_part(part, index + 1 < parts.len()))
        .collect()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn parse_key_part(value: &str, modifier_only: bool) -> Result<Key, ComputerError> {
    let normalized = value.to_ascii_lowercase();
    let key = match normalized.as_str() {
        "control" | "ctrl" => Key::Control,
        "alt" | "option" => Key::Alt,
        "shift" => Key::Shift,
        "meta" | "command" | "cmd" | "super" | "win" => Key::Meta,
        _ if modifier_only => {
            return Err(invalid(format!("Unsupported modifier: {value}")));
        }
        "enter" | "return" => Key::Return,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "up" | "arrowup" => Key::UpArrow,
        "down" | "arrowdown" => Key::DownArrow,
        "left" | "arrowleft" => Key::LeftArrow,
        "right" | "arrowright" => Key::RightArrow,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        _ => {
            let mut characters = value.chars();
            let character = characters
                .next()
                .filter(|_| characters.next().is_none())
                .ok_or_else(|| invalid(format!("Unsupported key: {value}")))?;
            Key::Unicode(character)
        }
    };
    Ok(key)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn backend_name() -> &'static str {
    "xcap+enigo"
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn backend_name() -> &'static str {
    "unsupported"
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn stale_frame() -> ComputerError {
    ComputerError::new(
        "COMPUTER_STALE_FRAME",
        "The desktop frame is missing or stale. Observe the computer again before acting.",
    )
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn unsupported_platform() -> ComputerError {
    ComputerError::new(
        "COMPUTER_PLATFORM_UNSUPPORTED",
        "Computer input is currently supported on macOS and Windows",
    )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn input_error(error: impl std::fmt::Display) -> ComputerError {
    ComputerError::new("COMPUTER_INPUT_FAILED", error.to_string())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn frame() -> FrameState {
        FrameState {
            id: "frame-1".to_owned(),
            display: DisplayDescriptor {
                id: "display-1".to_owned(),
                index: 0,
                name: "Primary".to_owned(),
                x: -100,
                y: 20,
                width: 2_000,
                height: 1_000,
                scale_factor: 2.0,
                rotation: 0.0,
                primary: true,
            },
            image_width: 1_000,
            image_height: 500,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn maps_delivered_image_coordinates_to_live_screen_space() {
        assert_eq!(map_image_point(&frame(), 0.0, 0.0), (-100, 20));
        assert_eq!(map_image_point(&frame(), 500.0, 250.0), (900, 520));
        assert_eq!(map_image_point(&frame(), 999.0, 499.0), (1_898, 1_018));
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
    fn emits_structured_errors() {
        let envelope = result_envelope("1", Err(invalid("bad point")));
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["error"]["code"], "COMPUTER_INVALID_REQUEST");
        assert_eq!(envelope["error"]["message"], "bad point");
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn accepts_named_keys_and_rejects_non_modifier_prefixes() {
        assert_eq!(parse_key_chord("Control+Shift+A").unwrap().len(), 3);
        assert_eq!(parse_key_chord("Enter").unwrap(), vec![Key::Return]);
        assert!(parse_key_chord("A+B").is_err());
        assert!(parse_key_chord("").is_err());
    }
}
