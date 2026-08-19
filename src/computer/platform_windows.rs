use std::thread;
use std::time::Duration;

use image::RgbaImage;
use xcap::Window;

use super::{ComputerError, InvariantReport, TargetPoint, WindowDescriptor};

type Hwnd = isize;

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GuiThreadInfo {
    cb_size: u32,
    flags: u32,
    hwnd_active: Hwnd,
    hwnd_focus: Hwnd,
    hwnd_capture: Hwnd,
    hwnd_menu_owner: Hwnd,
    hwnd_move_size: Hwnd,
    hwnd_caret: Hwnd,
    caret: [i32; 4],
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetForegroundWindow() -> Hwnd;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn ScreenToClient(window: Hwnd, point: *mut Point) -> i32;
    fn ChildWindowFromPointEx(parent: Hwnd, point: Point, flags: u32) -> Hwnd;
    fn PostMessageW(window: Hwnd, message: u32, wparam: usize, lparam: isize) -> i32;
    fn GetWindowThreadProcessId(window: Hwnd, pid: *mut u32) -> u32;
    fn GetAncestor(window: Hwnd, flags: u32) -> Hwnd;
    fn GetGUIThreadInfo(thread_id: u32, info: *mut GuiThreadInfo) -> i32;
    fn IsWindow(window: Hwnd) -> i32;
    fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
    fn SetWindowLongPtrW(window: Hwnd, index: i32, value: isize) -> isize;
}

const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_MOUSEHWHEEL: u32 = 0x020E;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
const MK_LBUTTON: usize = 0x0001;
const MK_RBUTTON: usize = 0x0002;
const MK_MBUTTON: usize = 0x0010;
const GWL_EXSTYLE: i32 = -20;
const WS_EX_NOACTIVATE: isize = 0x0800_0000;
const CWP_SKIPINVISIBLE: u32 = 0x0001;
const CWP_SKIPDISABLED: u32 = 0x0002;
const CWP_SKIPTRANSPARENT: u32 = 0x0004;
const GA_ROOT: u32 = 2;

pub fn backend_name() -> &'static str {
    "background-window/win32-messages+wgc"
}

pub fn limitations() -> Vec<&'static str> {
    vec![
        "Background Win32 messages work best with native Win32 controls",
        "Chromium, WPF, WinUI, games, elevated windows, and protected content can refuse background events",
        "Unsupported controls fail rather than falling back to SendInput or SetForegroundWindow",
    ]
}

pub fn input_ready() -> bool {
    true
}

pub fn windows(limit: usize) -> Result<Vec<WindowDescriptor>, ComputerError> {
    let current_pid = std::process::id();
    Ok(Window::all()
        .map_err(capture_error)?
        .into_iter()
        .filter_map(|window| descriptor(&window).ok())
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
    exact_window(target)?.capture_image().map_err(capture_error)
}

pub fn move_pointer(
    target: &WindowDescriptor,
    point: TargetPoint,
) -> Result<InvariantReport, ComputerError> {
    guarded(target, || {
        let (recipient, local) = mouse_recipient(target, point)?;
        post(recipient, WM_MOUSEMOVE, 0, point_lparam(local))
    })
}

pub fn click(
    target: &WindowDescriptor,
    point: TargetPoint,
    button: &str,
    count: usize,
) -> Result<InvariantReport, ComputerError> {
    let (down, up, state) = match button {
        "right" => (WM_RBUTTONDOWN, WM_RBUTTONUP, MK_RBUTTON),
        "middle" => (WM_MBUTTONDOWN, WM_MBUTTONUP, MK_MBUTTON),
        _ => (WM_LBUTTONDOWN, WM_LBUTTONUP, MK_LBUTTON),
    };
    guarded(target, || {
        let (recipient, local) = mouse_recipient(target, point)?;
        post(recipient, WM_MOUSEMOVE, 0, point_lparam(local))?;
        for index in 0..count.max(1) {
            post(recipient, down, state, point_lparam(local))?;
            thread::sleep(Duration::from_millis(24));
            post(recipient, up, 0, point_lparam(local))?;
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
    guarded(target, || {
        let (recipient, from_local) = mouse_recipient(target, from)?;
        post(recipient, WM_MOUSEMOVE, 0, point_lparam(from_local))?;
        post(
            recipient,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            point_lparam(from_local),
        )?;
        let steps = (duration_ms / 16).clamp(4, 120);
        for step in 1..=steps {
            let progress = step as f64 / steps as f64;
            let screen = Point {
                x: interpolate(from.screen_x, to.screen_x, progress),
                y: interpolate(from.screen_y, to.screen_y, progress),
            };
            let local = screen_to_client(recipient, screen)?;
            post(recipient, WM_MOUSEMOVE, MK_LBUTTON, point_lparam(local))?;
            thread::sleep(Duration::from_millis((duration_ms / steps).max(1)));
        }
        let to_local = screen_to_client(
            recipient,
            Point {
                x: to.screen_x,
                y: to.screen_y,
            },
        )?;
        post(recipient, WM_LBUTTONUP, 0, point_lparam(to_local))
    })
}

pub fn scroll(
    target: &WindowDescriptor,
    point: TargetPoint,
    delta_x: i32,
    delta_y: i32,
) -> Result<InvariantReport, ComputerError> {
    guarded(target, || {
        let (recipient, _) = mouse_recipient(target, point)?;
        let screen = point_lparam(Point {
            x: point.screen_x,
            y: point.screen_y,
        });
        if delta_y != 0 {
            post(
                recipient,
                WM_MOUSEWHEEL,
                wheel_wparam(delta_y.saturating_mul(120)),
                screen,
            )?;
        }
        if delta_x != 0 {
            post(
                recipient,
                WM_MOUSEHWHEEL,
                wheel_wparam(delta_x.saturating_mul(120)),
                screen,
            )?;
        }
        Ok(())
    })
}

pub fn type_text(target: &WindowDescriptor, text: &str) -> Result<InvariantReport, ComputerError> {
    guarded(target, || {
        let recipient = keyboard_recipient(target)?;
        for unit in text.encode_utf16() {
            post(recipient, WM_CHAR, unit as usize, 0)?;
        }
        Ok(())
    })
}

pub fn key(target: &WindowDescriptor, chord: &str) -> Result<InvariantReport, ComputerError> {
    let parts = chord.split('+').map(str::trim).collect::<Vec<_>>();
    let recipient = keyboard_recipient(target)?;
    let modifiers = parts[..parts.len().saturating_sub(1)]
        .iter()
        .filter_map(|part| virtual_key(part))
        .collect::<Vec<_>>();
    let key = virtual_key(parts.last().copied().unwrap_or("")).ok_or_else(|| {
        ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "The Windows background backend does not map this key",
        )
    })?;
    guarded(target, || {
        for modifier in &modifiers {
            post(recipient, WM_KEYDOWN, *modifier, 0)?;
        }
        post(recipient, WM_KEYDOWN, key, 0)?;
        thread::sleep(Duration::from_millis(18));
        post(recipient, WM_KEYUP, key, 1 << 31)?;
        for modifier in modifiers.iter().rev() {
            post(recipient, WM_KEYUP, *modifier, 1 << 31)?;
        }
        Ok(())
    })
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
        focused: window.is_focused().unwrap_or(false),
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
    action: impl FnOnce() -> Result<(), ComputerError>,
) -> Result<InvariantReport, ComputerError> {
    let before = DesktopSnapshot::capture()?;
    let hwnd = target_hwnd(target)?;
    let _no_activate = NoActivateGuard::arm(hwnd);
    action()?;
    thread::sleep(Duration::from_millis(35));
    Ok(before.compare(&DesktopSnapshot::capture()?))
}

fn target_hwnd(target: &WindowDescriptor) -> Result<Hwnd, ComputerError> {
    let hwnd = target
        .id
        .parse::<u32>()
        .map(|value| value as Hwnd)
        .map_err(|_| input_error("invalid HWND"))?;
    if hwnd == 0 || unsafe { IsWindow(hwnd) } == 0 {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            "The exact target HWND is invalid",
        ));
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid != target.pid {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            "The exact target HWND changed owner",
        ));
    }
    Ok(hwnd)
}

fn mouse_recipient(
    target: &WindowDescriptor,
    point: TargetPoint,
) -> Result<(Hwnd, Point), ComputerError> {
    let mut recipient = target_hwnd(target)?;
    let screen = Point {
        x: point.screen_x,
        y: point.screen_y,
    };
    for _ in 0..20 {
        let local = screen_to_client(recipient, screen)?;
        let child = unsafe {
            ChildWindowFromPointEx(
                recipient,
                local,
                CWP_SKIPINVISIBLE | CWP_SKIPDISABLED | CWP_SKIPTRANSPARENT,
            )
        };
        if child == 0 || child == recipient {
            return Ok((recipient, local));
        }
        recipient = child;
    }
    Ok((recipient, screen_to_client(recipient, screen)?))
}

fn keyboard_recipient(target: &WindowDescriptor) -> Result<Hwnd, ComputerError> {
    let hwnd = target_hwnd(target)?;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) };
    let mut info = empty_gui_thread_info();
    if unsafe { GetGUIThreadInfo(thread_id, &mut info) } != 0 && info.hwnd_focus != 0 {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(info.hwnd_focus, &mut pid) };
        let focus_root = unsafe { GetAncestor(info.hwnd_focus, GA_ROOT) };
        if pid == target.pid && focus_root == hwnd {
            return Ok(info.hwnd_focus);
        }
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "The target process focus belongs to a different top-level window",
        ));
    }
    Ok(hwnd)
}

fn empty_gui_thread_info() -> GuiThreadInfo {
    GuiThreadInfo {
        cb_size: std::mem::size_of::<GuiThreadInfo>() as u32,
        flags: 0,
        hwnd_active: 0,
        hwnd_focus: 0,
        hwnd_capture: 0,
        hwnd_menu_owner: 0,
        hwnd_move_size: 0,
        hwnd_caret: 0,
        caret: [0; 4],
    }
}

fn screen_to_client(window: Hwnd, mut point: Point) -> Result<Point, ComputerError> {
    if unsafe { ScreenToClient(window, &mut point) } == 0 {
        return Err(input_error("ScreenToClient failed"));
    }
    Ok(point)
}

fn post(window: Hwnd, message: u32, wparam: usize, lparam: isize) -> Result<(), ComputerError> {
    if unsafe { PostMessageW(window, message, wparam, lparam) } == 0 {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "Windows rejected exact-window background message delivery",
        ));
    }
    Ok(())
}

fn point_lparam(point: Point) -> isize {
    let x = point.x as u16 as u32;
    let y = point.y as u16 as u32;
    (x | (y << 16)) as isize
}

fn wheel_wparam(delta: i32) -> usize {
    ((delta as i16 as u16 as u32) << 16) as usize
}

fn interpolate(from: i32, to: i32, progress: f64) -> i32 {
    (from as f64 + (to - from) as f64 * progress).round() as i32
}

fn virtual_key(value: &str) -> Option<usize> {
    let normalized = value.to_ascii_lowercase();
    Some(match normalized.as_str() {
        "control" | "ctrl" => 0x11,
        "alt" | "option" => 0x12,
        "shift" => 0x10,
        "meta" | "command" | "cmd" | "super" | "win" => 0x5B,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "escape" | "esc" => 0x1B,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "left" | "arrowleft" => 0x25,
        "up" | "arrowup" => 0x26,
        "right" | "arrowright" => 0x27,
        "down" | "arrowdown" => 0x28,
        value if value.len() == 1 => value.as_bytes()[0].to_ascii_uppercase() as usize,
        value if value.starts_with('f') => {
            let number = value[1..].parse::<usize>().ok()?;
            if !(1..=24).contains(&number) {
                return None;
            }
            0x70 + number - 1
        }
        _ => return None,
    })
}

struct NoActivateGuard {
    hwnd: Hwnd,
    applied: bool,
}

impl NoActivateGuard {
    fn arm(hwnd: Hwnd) -> Self {
        let previous = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        let applied = previous & WS_EX_NOACTIVATE == 0;
        if applied {
            unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, previous | WS_EX_NOACTIVATE) };
        }
        Self { hwnd, applied }
    }
}

impl Drop for NoActivateGuard {
    fn drop(&mut self) {
        if self.applied {
            let current = unsafe { GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) };
            unsafe { SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, current & !WS_EX_NOACTIVATE) };
        }
    }
}

#[derive(Clone, Copy)]
struct DesktopSnapshot {
    foreground: Hwnd,
    user_focus: Hwnd,
    cursor: Point,
}

impl DesktopSnapshot {
    fn capture() -> Result<Self, ComputerError> {
        let foreground = unsafe { GetForegroundWindow() };
        let user_focus = if foreground == 0 {
            0
        } else {
            let thread_id = unsafe { GetWindowThreadProcessId(foreground, std::ptr::null_mut()) };
            let mut info = empty_gui_thread_info();
            if unsafe { GetGUIThreadInfo(thread_id, &mut info) } != 0 {
                info.hwnd_focus
            } else {
                0
            }
        };
        let mut cursor = Point::default();
        if unsafe { GetCursorPos(&mut cursor) } == 0 {
            return Err(input_error("GetCursorPos failed"));
        }
        Ok(Self {
            foreground,
            user_focus,
            cursor,
        })
    }

    fn compare(&self, after: &Self) -> InvariantReport {
        InvariantReport {
            foreground_unchanged: self.foreground == after.foreground,
            user_focus_unchanged: self.user_focus == after.user_focus,
            cursor_unchanged: self.cursor == after.cursor,
            space_unchanged: true,
        }
    }
}

fn capture_error(error: impl std::fmt::Display) -> ComputerError {
    ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string())
}

fn input_error(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_INPUT_FAILED", message)
}
