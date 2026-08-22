use std::ffi::c_void;
use std::thread;
use std::time::{Duration, Instant};

use image::RgbaImage;
use xcap::Window;

use super::{
    CommandCancellation, ComputerError, InvariantReport, SemanticSnapshot, SemanticTarget,
    TargetPoint, WindowDescriptor, uia_windows,
};

type Hwnd = isize;
type Hdesk = isize;

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
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
    fn GetKeyboardLayout(thread_id: u32) -> isize;
    fn MapVirtualKeyExW(code: u32, map_type: u32, keyboard_layout: isize) -> u32;
    fn OpenInputDesktop(flags: u32, inherit: i32, desired_access: u32) -> Hdesk;
    fn GetUserObjectInformationW(
        object: isize,
        index: i32,
        info: *mut c_void,
        length: u32,
        needed: *mut u32,
    ) -> i32;
    fn CloseDesktop(desktop: Hdesk) -> i32;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmGetWindowAttribute(
        window: Hwnd,
        attribute: u32,
        value: *mut c_void,
        value_size: u32,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
    fn SetLastError(error: u32);
}

const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_RBUTTONDBLCLK: u32 = 0x0206;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;
const WM_MBUTTONDBLCLK: u32 = 0x0209;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_MOUSEHWHEEL: u32 = 0x020E;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const MK_LBUTTON: usize = 0x0001;
const MK_RBUTTON: usize = 0x0002;
const MK_MBUTTON: usize = 0x0010;
const CWP_SKIPINVISIBLE: u32 = 0x0001;
const CWP_SKIPDISABLED: u32 = 0x0002;
const CWP_SKIPTRANSPARENT: u32 = 0x0004;
const GA_ROOT: u32 = 2;
const MAPVK_VK_TO_VSC_EX: u32 = 4;
const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
const DESKTOP_READOBJECTS: u32 = 0x0001;
const UOI_NAME: i32 = 2;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_NOT_ENOUGH_QUOTA: u32 = 1_816;

const VK_CONTROL: usize = 0x11;
const VK_MENU: usize = 0x12;
const VK_SHIFT: usize = 0x10;
const VK_LWIN: usize = 0x5B;
const VK_F10: usize = 0x79;
const VK_TAB: usize = 0x09;
const VK_ESCAPE: usize = 0x1B;
const VK_DELETE: usize = 0x2E;

pub fn backend_name() -> &'static str {
    "background-window/uia+win32-messages+wgc-stream"
}

pub fn semantic_backend_name() -> &'static str {
    "windows-ui-automation"
}

pub fn semantic_ready(_prompt: bool) -> bool {
    true
}

pub fn semantic_elements(target: &WindowDescriptor) -> Result<SemanticSnapshot, ComputerError> {
    target_hwnd(target)?;
    uia_windows::snapshot(target)
}

pub fn invoke(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    action: &str,
    cancellation: &CommandCancellation,
) -> Result<(serde_json::Value, InvariantReport), ComputerError> {
    guarded_effect(target, cancellation, || {
        uia_windows::invoke(target, semantic, action, cancellation)
    })
}

pub fn set_value(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    value: &str,
    cancellation: &CommandCancellation,
) -> Result<(serde_json::Value, InvariantReport), ComputerError> {
    guarded_effect(target, cancellation, || {
        uia_windows::set_value(target, semantic, value, cancellation)
    })
}

pub fn limitations() -> Vec<&'static str> {
    vec![
        "UI Automation is preferred for Chromium, WPF, WinUI, and native controls",
        "Pixel-only events use Win32 messages; games, elevated windows, and protected content can refuse them",
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

pub fn capture_window(
    target: &mut WindowDescriptor,
) -> Result<(RgbaImage, Instant), ComputerError> {
    let frame = super::native_share::capture_one(target)?;
    super::native_share::bind_frame_geometry(&frame, target);
    Ok((frame.image, frame.captured_at))
}

pub fn move_pointer_path(
    target: &WindowDescriptor,
    points: &[TargetPoint],
    step_delay: Duration,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    guarded(target, cancellation, || {
        if points.is_empty() {
            return Err(input_error("synthetic pointer trajectory is empty"));
        }
        for (index, point) in points.iter().copied().enumerate() {
            let (recipient, local) = mouse_recipient(target, point)?;
            post(
                target,
                recipient,
                WM_MOUSEMOVE,
                0,
                point_lparam(local)?,
                cancellation,
                "pointer path dispatch",
            )?;
            if index + 1 < points.len() {
                thread::sleep(step_delay);
            }
        }
        Ok(())
    })
}

pub fn click(
    target: &WindowDescriptor,
    point: TargetPoint,
    button: &str,
    count: usize,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    let messages = mouse_button_messages(button);
    guarded(target, cancellation, || {
        let (recipient, local) = mouse_recipient(target, point)?;
        post(
            target,
            recipient,
            WM_MOUSEMOVE,
            0,
            point_lparam(local)?,
            cancellation,
            "click pointer dispatch",
        )?;
        let mut exact_recipient = None;
        for index in 0..count.max(1) {
            // Resolve the child hit target again immediately before the
            // irreversible DOWN. A pointer approach can be long enough for a
            // child control to disappear or move without changing the root
            // HWND.
            let (recipient, local) = mouse_recipient(target, point)?;
            if exact_recipient.is_some_and(|expected| expected != recipient) {
                return Err(ComputerError::new(
                    "COMPUTER_STALE_FRAME",
                    "The exact Windows child hit target changed during the click trajectory",
                ));
            }
            exact_recipient = Some(recipient);
            let press = mouse_press_message(messages, index);
            held_message_sequence(
                || {
                    post(
                        target,
                        recipient,
                        press,
                        messages.state,
                        point_lparam(local)?,
                        cancellation,
                        "mouse button press",
                    )
                },
                || {
                    thread::sleep(Duration::from_millis(24));
                    Ok(())
                },
                || post_release(target, recipient, messages.up, 0, point_lparam(local)?),
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
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    guarded(target, cancellation, || {
        let (recipient, from_local) = mouse_recipient(target, from)?;
        post(
            target,
            recipient,
            WM_MOUSEMOVE,
            0,
            point_lparam(from_local)?,
            cancellation,
            "drag approach dispatch",
        )?;
        // The child recipient can change while the approach animation runs.
        // Bind the held drag to the current exact child immediately before
        // posting the button-down message.
        let (recipient, from_local) = mouse_recipient(target, from)?;
        let last_local = std::cell::Cell::new(from_local);
        held_message_sequence(
            || {
                post(
                    target,
                    recipient,
                    WM_LBUTTONDOWN,
                    MK_LBUTTON,
                    point_lparam(from_local)?,
                    cancellation,
                    "drag press",
                )
            },
            || {
                let steps = (duration_ms / 16).clamp(4, 120);
                for step in 1..=steps {
                    let progress = step as f64 / steps as f64;
                    let screen = Point {
                        x: interpolate(from.screen_x, to.screen_x, progress),
                        y: interpolate(from.screen_y, to.screen_y, progress),
                    };
                    let local = screen_to_client(recipient, screen)?;
                    post(
                        target,
                        recipient,
                        WM_MOUSEMOVE,
                        MK_LBUTTON,
                        point_lparam(local)?,
                        cancellation,
                        "drag path dispatch",
                    )?;
                    last_local.set(local);
                    thread::sleep(Duration::from_millis((duration_ms / steps).max(1)));
                }
                Ok(())
            },
            || {
                post_release(
                    target,
                    recipient,
                    WM_LBUTTONUP,
                    0,
                    point_lparam(last_local.get())?,
                )
            },
        )
    })
}

pub fn scroll(
    target: &WindowDescriptor,
    point: TargetPoint,
    delta_x: i32,
    delta_y: i32,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    guarded(target, cancellation, || {
        let (recipient, _) = mouse_recipient(target, point)?;
        let screen = point_lparam(Point {
            x: point.screen_x,
            y: point.screen_y,
        })?;
        if delta_y != 0 {
            post(
                target,
                recipient,
                WM_MOUSEWHEEL,
                wheel_wparam(delta_y.saturating_mul(120)),
                screen,
                cancellation,
                "vertical scroll dispatch",
            )?;
        }
        if delta_x != 0 {
            post(
                target,
                recipient,
                WM_MOUSEHWHEEL,
                wheel_wparam(delta_x.saturating_mul(120)),
                screen,
                cancellation,
                "horizontal scroll dispatch",
            )?;
        }
        Ok(())
    })
}

pub fn type_text(
    target: &WindowDescriptor,
    text: &str,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    guarded(target, cancellation, || {
        let recipient = keyboard_recipient(target)?;
        for unit in text.encode_utf16() {
            post(
                target,
                recipient,
                WM_CHAR,
                unit as usize,
                0,
                cancellation,
                "text dispatch",
            )?;
        }
        Ok(())
    })
}

pub fn key(
    target: &WindowDescriptor,
    chord: &str,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    let parsed = parse_key_chord(chord)?;
    reject_unsupported_system_shortcut(&parsed)?;
    let recipient = keyboard_recipient(target)?;
    guarded(target, cancellation, || {
        let mut attempted_modifiers = Vec::with_capacity(parsed.modifiers.len());
        let mut alt_down = false;
        for modifier in &parsed.modifiers {
            if let Err(press_error) = post_key(
                target,
                recipient,
                *modifier,
                false,
                alt_down,
                cancellation,
                "modifier press",
            ) {
                return finish_with_cleanup(
                    Err(press_error),
                    release_keys(target, recipient, None, &attempted_modifiers),
                );
            }
            attempted_modifiers.push(*modifier);
            alt_down |= *modifier == VK_MENU;
        }
        if let Err(press_error) = post_key(
            target,
            recipient,
            parsed.key,
            false,
            alt_down,
            cancellation,
            "key press",
        ) {
            return finish_with_cleanup(
                Err(press_error),
                release_keys(target, recipient, None, &attempted_modifiers),
            );
        }
        thread::sleep(Duration::from_millis(18));
        release_keys(target, recipient, Some(parsed.key), &attempted_modifiers)
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

fn guarded(
    target: &WindowDescriptor,
    cancellation: &CommandCancellation,
    action: impl FnOnce() -> Result<(), ComputerError>,
) -> Result<InvariantReport, ComputerError> {
    let before = DesktopSnapshot::capture()?;
    target_hwnd(target)?;
    cancellation.check("Windows background dispatch")?;
    let action_result = action();
    cancellation.mark_verification_started();
    thread::sleep(Duration::from_millis(35));
    let report = before.compare(&DesktopSnapshot::capture()?);
    report.clone().assert_held()?;
    action_result?;
    Ok(report)
}

fn guarded_effect<T>(
    target: &WindowDescriptor,
    cancellation: &CommandCancellation,
    action: impl FnOnce() -> Result<T, ComputerError>,
) -> Result<(T, InvariantReport), ComputerError> {
    let before = DesktopSnapshot::capture()?;
    target_hwnd(target)?;
    cancellation.check("UI Automation semantic resolution")?;
    let action_result = action();
    cancellation.mark_verification_started();
    thread::sleep(Duration::from_millis(35));
    let report = before.compare(&DesktopSnapshot::capture()?).assert_held()?;
    let backend_effect = action_result?;
    Ok((backend_effect, report))
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
    revalidate_target_geometry(target, hwnd)?;
    Ok(hwnd)
}

fn revalidate_target_geometry(target: &WindowDescriptor, hwnd: Hwnd) -> Result<(), ComputerError> {
    let mut rect = Rect::default();
    let status = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            std::ptr::from_mut(&mut rect).cast(),
            u32::try_from(std::mem::size_of::<Rect>()).expect("Windows RECT size fits in u32"),
        )
    };
    if status < 0 {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            format!(
                "Windows could not re-read exact target geometry (HRESULT 0x{:08x})",
                status as u32
            ),
        ));
    }
    let Some((x, y, width, height)) = rect_geometry(rect) else {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            "The exact target HWND has invalid current geometry",
        ));
    };
    if x != target.x || y != target.y || width != target.width || height != target.height {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            format!(
                "The exact target HWND geometry changed from ({}, {}) {}x{} to ({x}, {y}) {width}x{}",
                target.x, target.y, target.width, target.height, height
            ),
        ));
    }
    Ok(())
}

fn rect_geometry(rect: Rect) -> Option<(i32, i32, u32, u32)> {
    let width = u32::try_from(i64::from(rect.right) - i64::from(rect.left))
        .ok()
        .filter(|value| *value != 0)?;
    let height = u32::try_from(i64::from(rect.bottom) - i64::from(rect.top))
        .ok()
        .filter(|value| *value != 0)?;
    Some((rect.left, rect.top, width, height))
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

fn post(
    target: &WindowDescriptor,
    window: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    cancellation: &CommandCancellation,
    boundary: &str,
) -> Result<(), ComputerError> {
    cancellation.begin_side_effect(boundary)?;
    post_release(target, window, message, wparam, lparam)
}

fn post_release(
    target: &WindowDescriptor,
    window: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> Result<(), ComputerError> {
    revalidate_recipient(target, window)?;
    unsafe { SetLastError(0) };
    if unsafe { PostMessageW(window, message, wparam, lparam) } == 0 {
        return Err(post_message_error(unsafe { GetLastError() }));
    }
    Ok(())
}

fn post_message_error(error: u32) -> ComputerError {
    let detail = match error {
        ERROR_ACCESS_DENIED => {
            "Windows denied background delivery, usually because UIPI or target elevation blocks this helper"
        }
        ERROR_NOT_ENOUGH_QUOTA => "The target thread's Windows message queue reached its quota",
        0 => "PostMessageW rejected background delivery without an extended error",
        _ => "PostMessageW rejected background delivery",
    };
    ComputerError::new(
        "COMPUTER_BACKGROUND_UNAVAILABLE",
        format!("{detail} (Win32 error {error})"),
    )
}

fn revalidate_recipient(target: &WindowDescriptor, recipient: Hwnd) -> Result<(), ComputerError> {
    let root = target_hwnd(target)?;
    if recipient == 0 || unsafe { IsWindow(recipient) } == 0 {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            "The resolved Windows message recipient no longer exists",
        ));
    }
    let mut recipient_pid = 0;
    let recipient_thread = unsafe { GetWindowThreadProcessId(recipient, &mut recipient_pid) };
    let recipient_root = unsafe { GetAncestor(recipient, GA_ROOT) };
    if recipient_thread == 0
        || !recipient_identity_matches(target.pid, root, recipient_pid, recipient_root)
    {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            "The resolved Windows message recipient changed process or top-level root",
        ));
    }
    Ok(())
}

fn recipient_identity_matches(
    target_pid: u32,
    target_root: Hwnd,
    recipient_pid: u32,
    recipient_root: Hwnd,
) -> bool {
    recipient_pid == target_pid && recipient_root == target_root
}

fn held_message_sequence<T>(
    press: impl FnOnce() -> Result<(), ComputerError>,
    action: impl FnOnce() -> Result<T, ComputerError>,
    release: impl FnOnce() -> Result<(), ComputerError>,
) -> Result<T, ComputerError> {
    // PostMessage returning failure means the DOWN message was not queued.
    // Sending a compensating UP in that case would create a new, unpaired
    // input event and could target a window that changed during the failure.
    press()?;
    finish_with_cleanup(action(), release())
}

fn release_keys(
    target: &WindowDescriptor,
    recipient: Hwnd,
    key: Option<usize>,
    modifiers: &[usize],
) -> Result<(), ComputerError> {
    let mut first_error = None;
    let mut alt_down = key == Some(VK_MENU) || modifiers.contains(&VK_MENU);
    if let Some(key) = key {
        if let Err(error) = post_key_release(target, recipient, key, alt_down) {
            first_error = Some(error);
        }
        if key == VK_MENU {
            alt_down = false;
        }
    }
    for modifier in modifiers.iter().rev() {
        if let Err(error) = post_key_release(target, recipient, *modifier, alt_down)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if *modifier == VK_MENU {
            alt_down = false;
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn post_key(
    target: &WindowDescriptor,
    recipient: Hwnd,
    virtual_key: usize,
    released: bool,
    alt_down: bool,
    cancellation: &CommandCancellation,
    boundary: &str,
) -> Result<(), ComputerError> {
    revalidate_recipient(target, recipient)?;
    let (scan_code, extended) = mapped_key_for_recipient(recipient, virtual_key)?;
    let (message, lparam) =
        key_message_with_scan(virtual_key, released, alt_down, scan_code, extended);
    post(
        target,
        recipient,
        message,
        virtual_key,
        lparam,
        cancellation,
        boundary,
    )
}

fn post_key_release(
    target: &WindowDescriptor,
    recipient: Hwnd,
    virtual_key: usize,
    alt_down: bool,
) -> Result<(), ComputerError> {
    revalidate_recipient(target, recipient)?;
    let (scan_code, extended) = mapped_key_for_recipient(recipient, virtual_key)?;
    let (message, lparam) = key_message_with_scan(virtual_key, true, alt_down, scan_code, extended);
    post_release(target, recipient, message, virtual_key, lparam)
}

fn mapped_key_for_recipient(
    recipient: Hwnd,
    virtual_key: usize,
) -> Result<(u8, bool), ComputerError> {
    let thread_id = unsafe { GetWindowThreadProcessId(recipient, std::ptr::null_mut()) };
    if thread_id == 0 {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            "The exact keyboard recipient thread no longer exists",
        ));
    }
    let keyboard_layout = unsafe { GetKeyboardLayout(thread_id) };
    if keyboard_layout == 0 {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "Windows could not read the exact keyboard recipient's input layout",
        ));
    }
    let mapped = unsafe {
        MapVirtualKeyExW(
            u32::try_from(virtual_key).expect("supported virtual keys fit in u32"),
            MAPVK_VK_TO_VSC_EX,
            keyboard_layout,
        )
    };
    if mapped == 0 {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "Windows could not map the virtual key with the exact recipient's keyboard layout",
        ));
    }
    Ok(((mapped & 0xff) as u8, mapped & 0xff00 != 0))
}

fn key_message_with_scan(
    virtual_key: usize,
    released: bool,
    alt_down: bool,
    scan_code: u8,
    extended: bool,
) -> (u32, isize) {
    // Message kind and bit 29 are related but not identical. Pressing Alt
    // itself is WM_SYSKEYDOWN while bit 29 is still zero; releasing it has bit
    // 29 set because Alt was down before that transition.
    let alt_context = alt_down;
    let message = key_message_kind(virtual_key, released, alt_context);
    (
        message,
        key_lparam(scan_code, extended, alt_context, released),
    )
}

fn key_message_kind(virtual_key: usize, released: bool, alt_context: bool) -> u32 {
    let system_key = alt_context || virtual_key == VK_MENU || virtual_key == VK_F10;
    match (system_key, released) {
        (true, true) => WM_SYSKEYUP,
        (true, false) => WM_SYSKEYDOWN,
        (false, true) => WM_KEYUP,
        (false, false) => WM_KEYDOWN,
    }
}

fn key_lparam(scan_code: u8, extended: bool, alt_context: bool, released: bool) -> isize {
    let mut value = 1_u32 | (u32::from(scan_code) << 16);
    if extended {
        value |= 1 << 24;
    }
    if alt_context {
        value |= 1 << 29;
    }
    if released {
        value |= (1 << 30) | (1 << 31);
    }
    value as isize
}

fn finish_with_cleanup<T>(
    result: Result<T, ComputerError>,
    cleanup: Result<(), ComputerError>,
) -> Result<T, ComputerError> {
    match cleanup {
        Ok(()) => result,
        Err(cleanup_error) => Err(cleanup_error),
    }
}

fn point_lparam(point: Point) -> Result<isize, ComputerError> {
    let x = i16::try_from(point.x).map_err(|_| {
        ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "The Windows message coordinate exceeds the signed 16-bit LPARAM range",
        )
    })?;
    let y = i16::try_from(point.y).map_err(|_| {
        ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "The Windows message coordinate exceeds the signed 16-bit LPARAM range",
        )
    })?;
    Ok((u32::from(x as u16) | (u32::from(y as u16) << 16)) as isize)
}

fn wheel_wparam(delta: i32) -> usize {
    ((delta as i16 as u16 as u32) << 16) as usize
}

fn interpolate(from: i32, to: i32, progress: f64) -> i32 {
    (from as f64 + (to - from) as f64 * progress).round() as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MouseButtonMessages {
    down: u32,
    up: u32,
    double_click: u32,
    state: usize,
}

fn mouse_button_messages(button: &str) -> MouseButtonMessages {
    match button {
        "right" => MouseButtonMessages {
            down: WM_RBUTTONDOWN,
            up: WM_RBUTTONUP,
            double_click: WM_RBUTTONDBLCLK,
            state: MK_RBUTTON,
        },
        "middle" => MouseButtonMessages {
            down: WM_MBUTTONDOWN,
            up: WM_MBUTTONUP,
            double_click: WM_MBUTTONDBLCLK,
            state: MK_MBUTTON,
        },
        _ => MouseButtonMessages {
            down: WM_LBUTTONDOWN,
            up: WM_LBUTTONUP,
            double_click: WM_LBUTTONDBLCLK,
            state: MK_LBUTTON,
        },
    }
}

fn mouse_press_message(messages: MouseButtonMessages, click_index: usize) -> u32 {
    if click_index % 2 == 1 {
        messages.double_click
    } else {
        messages.down
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedKeyChord {
    modifiers: Vec<usize>,
    key: usize,
}

fn parse_key_chord(chord: &str) -> Result<ParsedKeyChord, ComputerError> {
    let parts = chord.split('+').map(str::trim).collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "The Windows background backend requires a complete key chord",
        ));
    }
    let (key_name, modifier_names) = parts.split_last().expect("non-empty chord");
    let mut modifiers = Vec::with_capacity(modifier_names.len());
    for modifier_name in modifier_names {
        let modifier = virtual_key(modifier_name)
            .filter(|candidate| matches!(*candidate, VK_CONTROL | VK_MENU | VK_SHIFT | VK_LWIN));
        let Some(modifier) = modifier else {
            return Err(ComputerError::new(
                "COMPUTER_BACKGROUND_UNAVAILABLE",
                "The Windows background backend does not map this modifier",
            ));
        };
        if modifiers.contains(&modifier) {
            return Err(ComputerError::new(
                "COMPUTER_BACKGROUND_UNAVAILABLE",
                "The Windows background backend rejects duplicate modifiers",
            ));
        }
        modifiers.push(modifier);
    }
    let key = virtual_key(key_name).ok_or_else(|| {
        ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "The Windows background backend does not map this key",
        )
    })?;
    if modifiers.contains(&key) && matches!(key, VK_CONTROL | VK_MENU | VK_SHIFT | VK_LWIN) {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "The Windows background backend rejects a modifier repeated as the key",
        ));
    }
    Ok(ParsedKeyChord { modifiers, key })
}

fn reject_unsupported_system_shortcut(chord: &ParsedKeyChord) -> Result<(), ComputerError> {
    let control = chord.modifiers.contains(&VK_CONTROL);
    let alt = chord.modifiers.contains(&VK_MENU);
    let windows = chord.modifiers.contains(&VK_LWIN) || chord.key == VK_LWIN;
    let global_switch = alt && matches!(chord.key, VK_TAB | VK_ESCAPE);
    let start_menu = control && chord.key == VK_ESCAPE;
    let secure_attention = control && alt && chord.key == VK_DELETE;
    if windows || global_switch || start_menu || secure_attention {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "This global or secure Windows shortcut cannot be delivered to one background window",
        ));
    }
    Ok(())
}

fn virtual_key(value: &str) -> Option<usize> {
    let normalized = value.to_ascii_lowercase();
    Some(match normalized.as_str() {
        "control" | "ctrl" => VK_CONTROL,
        "alt" | "option" => VK_MENU,
        "shift" => VK_SHIFT,
        "meta" | "command" | "cmd" | "super" | "win" => VK_LWIN,
        "enter" | "return" => 0x0D,
        "tab" => VK_TAB,
        "escape" | "esc" => VK_ESCAPE,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => VK_DELETE,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "left" | "arrowleft" => 0x25,
        "up" | "arrowup" => 0x26,
        "right" | "arrowright" => 0x27,
        "down" | "arrowdown" => 0x28,
        ";" => 0xBA,
        "=" => 0xBB,
        "," => 0xBC,
        "-" => 0xBD,
        "." => 0xBE,
        "/" => 0xBF,
        "`" => 0xC0,
        "[" => 0xDB,
        "\\" => 0xDC,
        "]" => 0xDD,
        "'" => 0xDE,
        value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphanumeric() => {
            value.as_bytes()[0].to_ascii_uppercase() as usize
        }
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct InputDesktopIdentity(Vec<u16>);

#[derive(Clone)]
struct DesktopSnapshot {
    foreground: Hwnd,
    user_focus: Hwnd,
    cursor: Point,
    input_desktop: InputDesktopIdentity,
}

impl DesktopSnapshot {
    fn capture() -> Result<Self, ComputerError> {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground == 0 {
            return Err(input_error(
                "GetForegroundWindow returned no readable window",
            ));
        }
        let thread_id = unsafe { GetWindowThreadProcessId(foreground, std::ptr::null_mut()) };
        if thread_id == 0 {
            return Err(input_error(
                "Could not resolve the foreground window thread",
            ));
        }
        let mut info = empty_gui_thread_info();
        if unsafe { GetGUIThreadInfo(thread_id, &mut info) } == 0 {
            return Err(input_error(
                "GetGUIThreadInfo failed for the foreground thread",
            ));
        }
        let mut cursor = Point::default();
        if unsafe { GetCursorPos(&mut cursor) } == 0 {
            return Err(input_error("GetCursorPos failed"));
        }
        Self::from_observations(
            foreground,
            info.hwnd_focus,
            Some(cursor),
            Some(input_desktop_identity()?),
        )
    }

    fn from_observations(
        foreground: Hwnd,
        user_focus: Hwnd,
        cursor: Option<Point>,
        input_desktop: Option<InputDesktopIdentity>,
    ) -> Result<Self, ComputerError> {
        if foreground == 0 {
            return Err(input_error("The foreground window identity is unknown"));
        }
        if user_focus == 0 {
            return Err(input_error("The user focus window identity is unknown"));
        }
        Ok(Self {
            foreground,
            user_focus,
            cursor: cursor.ok_or_else(|| input_error("The hardware cursor is unreadable"))?,
            input_desktop: input_desktop
                .ok_or_else(|| input_error("The input desktop identity is unknown"))?,
        })
    }

    fn compare(&self, after: &Self) -> InvariantReport {
        InvariantReport {
            foreground_unchanged: self.foreground == after.foreground,
            user_focus_unchanged: self.user_focus == after.user_focus,
            cursor_unchanged: self.cursor == after.cursor,
            space_unchanged: self.input_desktop == after.input_desktop,
        }
    }
}

fn input_desktop_identity() -> Result<InputDesktopIdentity, ComputerError> {
    let desktop = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    if desktop == 0 {
        return Err(input_error("OpenInputDesktop failed"));
    }
    let _desktop = DesktopHandle(desktop);
    let mut needed = 0_u32;
    unsafe { GetUserObjectInformationW(desktop, UOI_NAME, std::ptr::null_mut(), 0, &mut needed) };
    if needed < std::mem::size_of::<u16>() as u32 || !needed.is_multiple_of(2) {
        return Err(input_error(
            "GetUserObjectInformationW returned an invalid desktop-name size",
        ));
    }
    let mut name = vec![0_u16; needed as usize / std::mem::size_of::<u16>()];
    if unsafe {
        GetUserObjectInformationW(
            desktop,
            UOI_NAME,
            name.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(input_error(
            "GetUserObjectInformationW failed for the input desktop",
        ));
    }
    while name.last() == Some(&0) {
        name.pop();
    }
    if name.is_empty() {
        return Err(input_error("The input desktop name is empty"));
    }
    Ok(InputDesktopIdentity(name))
}

struct DesktopHandle(Hdesk);

impl Drop for DesktopHandle {
    fn drop(&mut self) {
        unsafe { CloseDesktop(self.0) };
    }
}

fn capture_error(error: impl std::fmt::Display) -> ComputerError {
    ComputerError::new("COMPUTER_CAPTURE_FAILED", error.to_string())
}

fn input_error(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_INPUT_FAILED", message)
}

#[cfg(test)]
mod message_tests {
    use super::*;

    #[test]
    fn keyboard_lparam_contains_documented_win32_fields() {
        assert_eq!(
            key_lparam(0x1e, false, false, false) as u32,
            1 | (0x1e << 16)
        );
        assert_eq!(
            key_lparam(0x53, true, true, true) as u32,
            1 | (0x53 << 16) | (1 << 24) | (1 << 29) | (1 << 30) | (1 << 31)
        );
    }

    #[test]
    fn keyboard_messages_use_system_variants_for_alt_and_f10() {
        assert_eq!(key_message_kind(b'A' as usize, false, false), WM_KEYDOWN);
        assert_eq!(key_message_kind(b'A' as usize, true, false), WM_KEYUP);
        assert_eq!(key_message_kind(b'A' as usize, false, true), WM_SYSKEYDOWN);
        assert_eq!(key_message_kind(b'A' as usize, true, true), WM_SYSKEYUP);
        assert_eq!(key_message_kind(VK_F10, false, false), WM_SYSKEYDOWN);

        let (message, lparam) = key_message_with_scan(VK_MENU, false, false, 0x38, false);
        assert_eq!(message, WM_SYSKEYDOWN);
        assert_eq!(lparam as u32 & (1 << 29), 0);

        let (message, lparam) = key_message_with_scan(VK_MENU, true, true, 0x38, false);
        assert_eq!(message, WM_SYSKEYUP);
        assert_ne!(lparam as u32 & (1 << 29), 0);

        let (_, delete_lparam) = key_message_with_scan(VK_DELETE, true, false, 0x53, true);
        assert_ne!(delete_lparam as u32 & (1 << 24), 0);
        assert_eq!(
            delete_lparam as u32 & ((1 << 30) | (1 << 31)),
            (1 << 30) | (1 << 31)
        );
    }

    #[test]
    fn double_clicks_replace_only_the_second_down_message() {
        for button in ["left", "right", "middle"] {
            let messages = mouse_button_messages(button);
            assert_eq!(mouse_press_message(messages, 0), messages.down);
            assert_eq!(mouse_press_message(messages, 1), messages.double_click);
            assert_eq!(mouse_press_message(messages, 2), messages.down);
        }
    }

    #[test]
    fn failed_mouse_down_does_not_emit_an_unpaired_release() {
        let released = std::cell::Cell::new(false);
        let error = held_message_sequence::<()>(
            || Err(input_error("fixture press failed")),
            || Ok(()),
            || {
                released.set(true);
                Ok(())
            },
        )
        .expect_err("a failed press must fail the sequence");
        assert_eq!(error.code, "COMPUTER_INPUT_FAILED");
        assert!(!released.get());
    }

    #[test]
    fn recipient_identity_requires_both_process_and_root_to_match() {
        assert!(recipient_identity_matches(42, 100, 42, 100));
        assert!(!recipient_identity_matches(42, 100, 7, 100));
        assert!(!recipient_identity_matches(42, 100, 42, 200));
    }

    #[test]
    fn message_coordinates_fail_instead_of_wrapping_signed_words() {
        assert_eq!(
            point_lparam(Point { x: -1, y: 32_767 }).unwrap() as u32,
            0x7fff_ffff
        );
        assert!(point_lparam(Point { x: 32_768, y: 0 }).is_err());
        assert!(point_lparam(Point { x: 0, y: -32_769 }).is_err());
    }

    #[test]
    fn rectangle_geometry_rejects_empty_and_overflown_bounds() {
        assert_eq!(
            rect_geometry(Rect {
                left: -20,
                top: 10,
                right: 80,
                bottom: 60,
            }),
            Some((-20, 10, 100, 50))
        );
        assert!(
            rect_geometry(Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 1,
            })
            .is_none()
        );
        assert!(
            rect_geometry(Rect {
                left: i32::MAX,
                top: 0,
                right: i32::MIN,
                bottom: 1,
            })
            .is_none()
        );
    }

    #[test]
    fn post_message_errors_distinguish_uipi_and_queue_exhaustion() {
        let denied = post_message_error(ERROR_ACCESS_DENIED);
        assert!(denied.message.contains("UIPI"));
        assert!(denied.message.contains("Win32 error 5"));
        let quota = post_message_error(ERROR_NOT_ENOUGH_QUOTA);
        assert!(quota.message.contains("quota"));
        assert!(quota.message.contains("Win32 error 1816"));
    }

    #[test]
    fn key_parser_rejects_incomplete_unknown_and_duplicate_modifiers() {
        assert_eq!(
            parse_key_chord("Control+Shift+A").unwrap(),
            ParsedKeyChord {
                modifiers: vec![VK_CONTROL, VK_SHIFT],
                key: b'A' as usize,
            }
        );
        for invalid in ["", "Control+", "Control+Control+A", "Alt+Alt", "A+B", "!"] {
            assert!(parse_key_chord(invalid).is_err(), "accepted {invalid}");
        }
        assert_eq!(parse_key_chord("Control+.").unwrap().key, 0xBE);
    }

    #[test]
    fn global_and_secure_shortcuts_fail_closed() {
        for shortcut in [
            "Meta+L",
            "Alt+Tab",
            "Alt+Shift+Escape",
            "Control+Escape",
            "Control+Shift+Escape",
            "Control+Alt+Delete",
        ] {
            let error = reject_unsupported_system_shortcut(&parse_key_chord(shortcut).unwrap())
                .expect_err(shortcut);
            assert_eq!(error.code, "COMPUTER_BACKGROUND_UNAVAILABLE");
        }
        for shortcut in ["Control+L", "Alt+F4", "Shift+ArrowLeft"] {
            reject_unsupported_system_shortcut(&parse_key_chord(shortcut).unwrap())
                .expect(shortcut);
        }
    }
}

#[cfg(test)]
mod invariant_tests {
    use super::*;

    fn identity(name: &str) -> InputDesktopIdentity {
        InputDesktopIdentity(name.encode_utf16().collect())
    }

    #[test]
    fn unreadable_invariant_components_fail_closed() {
        let point = Some(Point { x: 10, y: 20 });
        let desktop = Some(identity("Default"));
        assert!(DesktopSnapshot::from_observations(0, 2, point, desktop.clone()).is_err());
        assert!(DesktopSnapshot::from_observations(1, 0, point, desktop.clone()).is_err());
        assert!(DesktopSnapshot::from_observations(1, 2, None, desktop.clone()).is_err());
        assert!(DesktopSnapshot::from_observations(1, 2, point, None).is_err());
    }

    #[test]
    fn input_desktop_identity_must_match() {
        let before = DesktopSnapshot::from_observations(
            1,
            2,
            Some(Point { x: 10, y: 20 }),
            Some(identity("Default")),
        )
        .unwrap();
        let same = before.clone();
        let changed = DesktopSnapshot::from_observations(
            1,
            2,
            Some(Point { x: 10, y: 20 }),
            Some(identity("Secure")),
        )
        .unwrap();
        assert!(before.compare(&same).space_unchanged);
        assert!(!before.compare(&changed).space_unchanged);
    }
}
