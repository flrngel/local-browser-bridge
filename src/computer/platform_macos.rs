use std::ffi::{c_char, c_void};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, CGMouseButton, ScrollEventUnit};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use foreign_types::ForeignType;
use image::RgbaImage;
use libc::pid_t;
use xcap::Window;

use super::{
    ComputerError, InvariantReport, SemanticTarget, TargetPoint, WindowDescriptor, ax_macos,
};

type PostToPidFn = unsafe extern "C" fn(pid_t, *mut c_void);
type SetWindowLocationFn = unsafe extern "C" fn(*mut c_void, f64, f64);
type SetIntegerFieldFn = unsafe extern "C" fn(*mut c_void, u32, i64);
type PostEventRecordToFn = unsafe extern "C" fn(*const c_void, *const u8) -> i32;
type GetFrontProcessFn = unsafe extern "C" fn(*mut c_void) -> i32;
type GetProcessForPidFn = unsafe extern "C" fn(pid_t, *mut c_void) -> i32;
type GetProcessPidFn = unsafe extern "C" fn(*const c_void, *mut pid_t) -> i32;
type ConnectionIdFn = unsafe extern "C" fn() -> u32;
type GetActiveSpaceFn = unsafe extern "C" fn(u32) -> u64;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventSetLocation(event: *mut c_void, location: CGPoint);
}

#[derive(Clone, Copy)]
struct Symbols {
    post_to_pid: PostToPidFn,
    set_window_location: SetWindowLocationFn,
    set_integer_field: SetIntegerFieldFn,
    post_event_record: PostEventRecordToFn,
    get_front_process: GetFrontProcessFn,
    get_process_for_pid: GetProcessForPidFn,
    get_process_pid: GetProcessPidFn,
    connection_id: ConnectionIdFn,
    get_active_space: GetActiveSpaceFn,
}

unsafe impl Send for Symbols {}
unsafe impl Sync for Symbols {}

pub fn backend_name() -> &'static str {
    "background-window/skylight+cgwindow"
}

pub fn semantic_backend_name() -> &'static str {
    "macos-accessibility"
}

pub fn semantic_ready(prompt: bool) -> bool {
    ax_macos::accessibility_ready(prompt)
}

pub fn semantic_elements(target: &WindowDescriptor) -> Result<Vec<SemanticTarget>, ComputerError> {
    ax_macos::snapshot(target)
}

pub fn invoke(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    action: &str,
) -> Result<serde_json::Value, ComputerError> {
    ax_macos::invoke(target, semantic, action)
}

pub fn set_value(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    value: &str,
) -> Result<serde_json::Value, ComputerError> {
    ax_macos::set_value(target, semantic, value)
}

pub fn limitations() -> Vec<&'static str> {
    vec![
        "Only non-minimized windows on the active macOS Space are mutable",
        "Secure input, protected content, games, and some GPU surfaces can refuse background events",
        "Private SkyLight event routing may require updates after a macOS release",
    ]
}

pub fn input_ready() -> bool {
    symbols().is_some()
}

pub fn windows(limit: usize) -> Result<Vec<WindowDescriptor>, ComputerError> {
    let current_pid = std::process::id();
    let windows = Window::all().map_err(capture_error)?;
    let focus = DesktopSnapshot::capture().ok();
    Ok(windows
        .into_iter()
        .filter_map(|window| descriptor(&window).ok())
        .map(|mut window| {
            window.focused = focus.is_some_and(|focus| {
                focus.front_pid == Some(window.pid)
                    && focus.front_window_id == window.id.parse::<u32>().ok()
            });
            window
        })
        .filter(|window| {
            window.pid != current_pid
                && !window.minimized
                && window.width >= 80
                && window.height >= 60
                && (!window.title.is_empty() || !window.app_name.is_empty())
        })
        .take(limit)
        .collect())
}

pub fn capture_window(target: &WindowDescriptor) -> Result<RgbaImage, ComputerError> {
    exact_window(target)?
        .capture_image()
        .map_err(|error| {
            ComputerError::new(
                "COMPUTER_CAPTURE_FAILED",
                format!(
                    "Exact-window capture failed. Grant Screen Recording to Local Computer Helper. {error}"
                ),
            )
        })
}

pub fn move_pointer(
    target: &WindowDescriptor,
    point: TargetPoint,
) -> Result<InvariantReport, ComputerError> {
    guarded(target, false, || {
        post_mouse(target, point, CGEventType::MouseMoved, 0, 0, 0)
    })
}

pub fn click(
    target: &WindowDescriptor,
    point: TargetPoint,
    button: &str,
    count: usize,
) -> Result<InvariantReport, ComputerError> {
    let (down, up, mouse_button, number) = match button {
        "right" => (
            CGEventType::RightMouseDown,
            CGEventType::RightMouseUp,
            CGMouseButton::Right,
            1,
        ),
        "middle" => (
            CGEventType::OtherMouseDown,
            CGEventType::OtherMouseUp,
            CGMouseButton::Center,
            2,
        ),
        _ => (
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
            CGMouseButton::Left,
            0,
        ),
    };
    guarded(target, true, || {
        post_mouse(target, point, CGEventType::MouseMoved, 0, number, 0)?;
        for index in 0..count.max(1) {
            post_mouse_with_button(
                target,
                point,
                down,
                mouse_button,
                (index + 1) as i64,
                number,
                3,
            )?;
            thread::sleep(Duration::from_millis(24));
            post_mouse_with_button(
                target,
                point,
                up,
                mouse_button,
                (index + 1) as i64,
                number,
                3,
            )?;
            if index + 1 < count {
                thread::sleep(Duration::from_millis(70));
            }
        }
        Ok(())
    })
}

pub fn drag(
    target: &WindowDescriptor,
    from: TargetPoint,
    to: TargetPoint,
    duration_ms: u64,
) -> Result<InvariantReport, ComputerError> {
    guarded(target, true, || {
        post_mouse(target, from, CGEventType::MouseMoved, 0, 0, 0)?;
        post_mouse_with_button(
            target,
            from,
            CGEventType::LeftMouseDown,
            CGMouseButton::Left,
            1,
            0,
            3,
        )?;
        let steps = (duration_ms / 16).clamp(4, 120);
        for step in 1..=steps {
            let progress = step as f64 / steps as f64;
            let point = TargetPoint {
                local_x: interpolate(from.local_x, to.local_x, progress),
                local_y: interpolate(from.local_y, to.local_y, progress),
                screen_x: interpolate(from.screen_x, to.screen_x, progress),
                screen_y: interpolate(from.screen_y, to.screen_y, progress),
            };
            post_mouse_with_button(
                target,
                point,
                CGEventType::LeftMouseDragged,
                CGMouseButton::Left,
                1,
                0,
                3,
            )?;
            thread::sleep(Duration::from_millis((duration_ms / steps).max(1)));
        }
        post_mouse_with_button(
            target,
            to,
            CGEventType::LeftMouseUp,
            CGMouseButton::Left,
            1,
            0,
            3,
        )
    })
}

pub fn scroll(
    target: &WindowDescriptor,
    point: TargetPoint,
    delta_x: i32,
    delta_y: i32,
) -> Result<InvariantReport, ComputerError> {
    guarded(target, true, || {
        post_mouse(target, point, CGEventType::MouseMoved, 0, 0, 0)?;
        let source = source()?;
        let event = CGEvent::new_scroll_event(
            source,
            ScrollEventUnit::LINE,
            2,
            delta_y.clamp(-10, 10),
            delta_x.clamp(-10, 10),
            0,
        )
        .map_err(|_| input_error("CGEventCreateScrollWheelEvent2 failed"))?;
        let raw = event.as_ptr() as *mut c_void;
        unsafe {
            CGEventSetLocation(
                raw,
                CGPoint::new(point.screen_x as f64, point.screen_y as f64),
            )
        };
        stamp(target, point, raw, 0, 0, 0)?;
        post(target, &event)
    })
}

pub fn type_text(target: &WindowDescriptor, text: &str) -> Result<InvariantReport, ComputerError> {
    ensure_unique_keyboard_destination(target)?;
    guarded(target, true, || {
        for character in text.chars() {
            let value = character.to_string();
            let down = CGEvent::new_keyboard_event(source()?, 0, true)
                .map_err(|_| input_error("CGEventCreateKeyboardEvent failed"))?;
            down.set_string(&value);
            stamp_keyboard(target, &down)?;
            post(target, &down)?;
            let up = CGEvent::new_keyboard_event(source()?, 0, false)
                .map_err(|_| input_error("CGEventCreateKeyboardEvent failed"))?;
            up.set_string(&value);
            stamp_keyboard(target, &up)?;
            post(target, &up)?;
        }
        Ok(())
    })
}

pub fn key(target: &WindowDescriptor, chord: &str) -> Result<InvariantReport, ComputerError> {
    ensure_unique_keyboard_destination(target)?;
    let parts = chord.split('+').map(str::trim).collect::<Vec<_>>();
    let last = parts.last().copied().unwrap_or("");
    let flags = modifier_flags(&parts[..parts.len().saturating_sub(1)]);
    let keycode = keycode(last).ok_or_else(|| {
        ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            format!("The macOS background backend does not map key {last}"),
        )
    })?;
    guarded(target, true, || {
        let down = CGEvent::new_keyboard_event(source()?, keycode, true)
            .map_err(|_| input_error("CGEventCreateKeyboardEvent failed"))?;
        down.set_flags(flags);
        stamp_keyboard(target, &down)?;
        post(target, &down)?;
        thread::sleep(Duration::from_millis(18));
        let up = CGEvent::new_keyboard_event(source()?, keycode, false)
            .map_err(|_| input_error("CGEventCreateKeyboardEvent failed"))?;
        up.set_flags(flags);
        stamp_keyboard(target, &up)?;
        post(target, &up)
    })
}

fn ensure_unique_keyboard_destination(target: &WindowDescriptor) -> Result<(), ComputerError> {
    let destinations = Window::all()
        .map_err(capture_error)?
        .into_iter()
        .filter(|window| {
            window.pid().ok() == Some(target.pid) && !window.is_minimized().unwrap_or(false)
        })
        .take(2)
        .count();
    if destinations == 1 {
        Ok(())
    } else {
        Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "Process-scoped macOS keyboard delivery is ambiguous because the target process has multiple eligible windows",
        ))
    }
}

fn descriptor(window: &Window) -> Result<WindowDescriptor, ComputerError> {
    let map = |error: xcap::XCapError| capture_error(error);
    Ok(WindowDescriptor {
        id: window.id().map_err(map)?.to_string(),
        pid: window.pid().map_err(map)?,
        app_name: window.app_name().map_err(map)?,
        title: window.title().map_err(map)?,
        x: window.x().map_err(map)?,
        y: window.y().map_err(map)?,
        width: window.width().map_err(map)?,
        height: window.height().map_err(map)?,
        minimized: window.is_minimized().unwrap_or(false),
        // Corrected against the front ProcessSerialNumber and WindowServer
        // z-order in `windows`; xcap marks every Chromium window focused.
        focused: false,
    })
}

fn exact_window(target: &WindowDescriptor) -> Result<Window, ComputerError> {
    Window::all()
        .map_err(capture_error)?
        .into_iter()
        .find(|window| {
            window
                .id()
                .ok()
                .is_some_and(|id| id.to_string() == target.id)
                && window.pid().ok() == Some(target.pid)
        })
        .ok_or_else(|| {
            ComputerError::new(
                "COMPUTER_STALE_FRAME",
                "The exact target window no longer exists or changed owner",
            )
        })
}

fn guarded(
    target: &WindowDescriptor,
    prepare_focus: bool,
    action: impl FnOnce() -> Result<(), ComputerError>,
) -> Result<InvariantReport, ComputerError> {
    let before = DesktopSnapshot::capture()?;
    let focus = prepare_focus
        .then(|| activate_without_raise(target, &before))
        .transpose()?
        .flatten();
    let action_result = action();
    if let Some(focus) = focus {
        focus.restore()?;
    }
    action_result?;
    thread::sleep(Duration::from_millis(35));
    Ok(before.compare(&DesktopSnapshot::capture()?))
}

fn post_mouse(
    target: &WindowDescriptor,
    point: TargetPoint,
    event_type: CGEventType,
    click_state: i64,
    button_number: i64,
    subtype: i64,
) -> Result<(), ComputerError> {
    post_mouse_with_button(
        target,
        point,
        event_type,
        CGMouseButton::Left,
        click_state,
        button_number,
        subtype,
    )
}

fn post_mouse_with_button(
    target: &WindowDescriptor,
    point: TargetPoint,
    event_type: CGEventType,
    button: CGMouseButton,
    click_state: i64,
    button_number: i64,
    subtype: i64,
) -> Result<(), ComputerError> {
    let event = CGEvent::new_mouse_event(
        source()?,
        event_type,
        CGPoint::new(point.screen_x as f64, point.screen_y as f64),
        button,
    )
    .map_err(|_| input_error("CGEventCreateMouseEvent failed"))?;
    stamp(
        target,
        point,
        event.as_ptr() as *mut c_void,
        click_state,
        button_number,
        subtype,
    )?;
    post(target, &event)
}

fn stamp(
    target: &WindowDescriptor,
    point: TargetPoint,
    raw: *mut c_void,
    click_state: i64,
    button_number: i64,
    subtype: i64,
) -> Result<(), ComputerError> {
    let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
    let window_id = target
        .id
        .parse::<i64>()
        .map_err(|_| input_error("invalid window id"))?;
    unsafe {
        (symbols.set_window_location)(raw, point.local_x as f64, point.local_y as f64);
        (symbols.set_integer_field)(raw, 1, click_state);
        (symbols.set_integer_field)(raw, 3, button_number);
        (symbols.set_integer_field)(raw, 7, subtype);
        (symbols.set_integer_field)(raw, 40, target.pid as i64);
        (symbols.set_integer_field)(raw, 51, window_id);
        (symbols.set_integer_field)(raw, 91, window_id);
        (symbols.set_integer_field)(raw, 92, window_id);
    }
    Ok(())
}

fn stamp_keyboard(target: &WindowDescriptor, event: &CGEvent) -> Result<(), ComputerError> {
    let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
    unsafe { (symbols.set_integer_field)(event.as_ptr() as *mut c_void, 40, target.pid as i64) };
    Ok(())
}

fn post(target: &WindowDescriptor, event: &CGEvent) -> Result<(), ComputerError> {
    let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
    unsafe { (symbols.post_to_pid)(target.pid as pid_t, event.as_ptr() as *mut c_void) };
    Ok(())
}

fn activate_without_raise(
    target: &WindowDescriptor,
    before: &DesktopSnapshot,
) -> Result<Option<FocusLease>, ComputerError> {
    if before.front_pid == Some(target.pid) {
        return Ok(None);
    }
    let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
    let window_id = target
        .id
        .parse::<u32>()
        .map_err(|_| input_error("invalid window id"))?;
    let mut destination = [0u8; 8];
    if unsafe {
        (symbols.get_process_for_pid)(target.pid as pid_t, destination.as_mut_ptr() as *mut c_void)
    } != 0
    {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "macOS could not resolve the exact target process for background focus",
        ));
    }
    let previous_window_id = before.front_window_id.ok_or_else(|| {
        ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "macOS could not prove the user's prior front window for focus restoration",
        )
    })?;
    let mut record = [0u8; 0xF8];
    record[0x04] = 0xF8;
    record[0x08] = 0x0D;
    record[0x3C..0x40].copy_from_slice(&window_id.to_le_bytes());
    record[0x8A] = 0x02;
    let defocused = unsafe {
        (symbols.post_event_record)(before.front_process.as_ptr().cast(), record.as_ptr())
    } == 0;
    record[0x8A] = 0x01;
    let focused =
        unsafe { (symbols.post_event_record)(destination.as_ptr().cast(), record.as_ptr()) } == 0;
    if !defocused || !focused {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "macOS could not establish exact-window background focus without activation",
        ));
    }
    Ok(Some(FocusLease {
        symbols,
        previous_psn: before.front_process,
        previous_window_id,
        target_psn: destination,
        target_window_id: window_id,
    }))
}

struct FocusLease {
    symbols: &'static Symbols,
    previous_psn: [u8; 8],
    previous_window_id: u32,
    target_psn: [u8; 8],
    target_window_id: u32,
}

impl FocusLease {
    fn restore(self) -> Result<(), ComputerError> {
        let mut record = [0u8; 0xF8];
        record[0x04] = 0xF8;
        record[0x08] = 0x0D;
        record[0x3C..0x40].copy_from_slice(&self.target_window_id.to_le_bytes());
        record[0x8A] = 0x02;
        let defocused = unsafe {
            (self.symbols.post_event_record)(self.target_psn.as_ptr().cast(), record.as_ptr())
        } == 0;
        record[0x3C..0x40].copy_from_slice(&self.previous_window_id.to_le_bytes());
        record[0x8A] = 0x01;
        let focused = unsafe {
            (self.symbols.post_event_record)(self.previous_psn.as_ptr().cast(), record.as_ptr())
        } == 0;
        if defocused && focused {
            Ok(())
        } else {
            Err(ComputerError::new(
                "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
                "macOS could not restore the user's prior background-focus state",
            ))
        }
    }
}

fn source() -> Result<CGEventSource, ComputerError> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| input_error("CGEventSourceCreate failed"))
}

fn interpolate(from: i32, to: i32, progress: f64) -> i32 {
    (from as f64 + (to - from) as f64 * progress).round() as i32
}

fn modifier_flags(parts: &[&str]) -> CGEventFlags {
    let mut flags = CGEventFlags::empty();
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "control" | "ctrl" => flags |= CGEventFlags::CGEventFlagControl,
            "alt" | "option" => flags |= CGEventFlags::CGEventFlagAlternate,
            "shift" => flags |= CGEventFlags::CGEventFlagShift,
            "meta" | "command" | "cmd" | "super" | "win" => {
                flags |= CGEventFlags::CGEventFlagCommand
            }
            _ => {}
        }
    }
    flags
}

fn keycode(value: &str) -> Option<u16> {
    let normalized = value.to_ascii_lowercase();
    Some(match normalized.as_str() {
        "enter" | "return" => 36,
        "tab" => 48,
        "space" => 49,
        "backspace" => 51,
        "escape" | "esc" => 53,
        "delete" | "del" => 117,
        "home" => 115,
        "end" => 119,
        "pageup" => 116,
        "pagedown" => 121,
        "left" | "arrowleft" => 123,
        "right" | "arrowright" => 124,
        "down" | "arrowdown" => 125,
        "up" | "arrowup" => 126,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        "a" => 0,
        "s" => 1,
        "d" => 2,
        "f" => 3,
        "h" => 4,
        "g" => 5,
        "z" => 6,
        "x" => 7,
        "c" => 8,
        "v" => 9,
        "b" => 11,
        "q" => 12,
        "w" => 13,
        "e" => 14,
        "r" => 15,
        "y" => 16,
        "t" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "=" => 24,
        "9" => 25,
        "7" => 26,
        "-" => 27,
        "8" => 28,
        "0" => 29,
        "]" => 30,
        "o" => 31,
        "u" => 32,
        "[" => 33,
        "i" => 34,
        "p" => 35,
        "l" => 37,
        "j" => 38,
        "'" => 39,
        "k" => 40,
        ";" => 41,
        "\\" => 42,
        "," => 43,
        "/" => 44,
        "n" => 45,
        "m" => 46,
        "." => 47,
        _ => return None,
    })
}

#[derive(Clone, Copy)]
struct DesktopSnapshot {
    front_process: [u8; 8],
    front_pid: Option<u32>,
    front_window_id: Option<u32>,
    cursor: CGPoint,
    active_space: u64,
}

impl DesktopSnapshot {
    fn capture() -> Result<Self, ComputerError> {
        let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
        let mut front_process = [0u8; 8];
        if unsafe { (symbols.get_front_process)(front_process.as_mut_ptr() as *mut c_void) } != 0 {
            return Err(input_error("Could not read the front process"));
        }
        let cursor = CGEvent::new(source()?)
            .map_err(|_| input_error("Could not read the hardware cursor"))?
            .location();
        let mut front_pid_raw: pid_t = 0;
        let front_pid = (unsafe {
            (symbols.get_process_pid)(front_process.as_ptr().cast(), &mut front_pid_raw)
        } == 0
            && front_pid_raw > 0)
            .then_some(front_pid_raw as u32);
        // xcap reports every window owned by a frontmost Chromium process as
        // focused. Resolve the true front process from the ProcessSerialNumber,
        // then take its first WindowServer entry (front-to-back z-order).
        let front_window = front_pid.and_then(|pid| {
            Window::all()
                .ok()?
                .into_iter()
                .find(|window| window.pid().ok() == Some(pid))
        });
        let front_window_id = front_window.as_ref().and_then(|window| window.id().ok());
        let active_space = unsafe { (symbols.get_active_space)((symbols.connection_id)()) };
        if active_space == 0 {
            return Err(input_error("Could not prove the active macOS Space"));
        }
        Ok(Self {
            front_process,
            front_pid,
            front_window_id,
            cursor,
            active_space,
        })
    }

    fn compare(&self, after: &Self) -> InvariantReport {
        InvariantReport {
            foreground_unchanged: self.front_process == after.front_process,
            user_focus_unchanged: self.front_pid == after.front_pid
                && self.front_window_id == after.front_window_id,
            cursor_unchanged: (self.cursor.x - after.cursor.x).abs() < 0.01
                && (self.cursor.y - after.cursor.y).abs() < 0.01,
            space_unchanged: self.active_space == after.active_space,
        }
    }
}

fn symbols() -> Option<&'static Symbols> {
    static SYMBOLS: OnceLock<Option<Symbols>> = OnceLock::new();
    SYMBOLS
        .get_or_init(|| unsafe {
            let path = b"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight\0";
            libc::dlopen(
                path.as_ptr() as *const c_char,
                libc::RTLD_LAZY | libc::RTLD_GLOBAL,
            );
            Some(Symbols {
                post_to_pid: load(b"SLEventPostToPid\0")?,
                set_window_location: load(b"CGEventSetWindowLocation\0")?,
                set_integer_field: load(b"SLEventSetIntegerValueField\0")?,
                post_event_record: load(b"SLPSPostEventRecordTo\0")?,
                get_front_process: load(b"_SLPSGetFrontProcess\0")?,
                get_process_for_pid: load(b"GetProcessForPID\0")?,
                get_process_pid: load(b"GetProcessPID\0")?,
                connection_id: load(b"CGSMainConnectionID\0")?,
                get_active_space: load(b"SLSGetActiveSpace\0")
                    .or_else(|| load(b"CGSGetActiveSpace\0"))?,
            })
        })
        .as_ref()
}

unsafe fn load<T: Copy>(name: &[u8]) -> Option<T> {
    let raw = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr() as *const c_char) };
    (!raw.is_null()).then(|| unsafe { std::mem::transmute_copy::<*mut c_void, T>(&raw) })
}

fn capture_error(error: impl std::fmt::Display) -> ComputerError {
    ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string())
}

fn input_error(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_INPUT_FAILED", message)
}
