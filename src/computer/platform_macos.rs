use std::ffi::{c_char, c_void};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use core_foundation::base::{CFGetTypeID, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, CGMouseButton, ScrollEventUnit};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use core_graphics::window::{copy_window_info, kCGNullWindowID, kCGWindowListOptionAll};
use foreign_types::ForeignType;
use image::RgbaImage;
use libc::pid_t;
use uuid::Uuid;
use xcap::Window;

use super::{
    COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS, CommandCancellation, ComputerError,
    HelperGlobalPointerPreservation, InputDeliveryProvenance, InputDeliveryRoute,
    InputDeliverySupportLevel, InvariantFailure, InvariantReport, InvariantStage, SemanticSnapshot,
    SemanticTarget, SharedPointerActivityState, SharedPointerBoundaryState, TargetPoint,
    WindowDescriptor, ax_macos, background_contract_violation,
};

const TEXT_EVENT_PACE: Duration = Duration::from_millis(1);
const FOCUS_RESTORE_PROOF_BUDGET: Duration = Duration::from_millis(350);
const FOCUS_RESTORE_OPERATION_BUDGET: Duration = Duration::from_millis(1_200);
const FOCUS_USER_RECOVERY_RESERVE: Duration = Duration::from_millis(400);
const FOCUS_USER_AUTHORIZATION_RESERVE: Duration = Duration::from_millis(270);
const FOCUS_USER_RESTORE_POLL_BUDGET: Duration = Duration::from_millis(120);
const FOCUS_USER_RETRY_RESERVE: Duration = Duration::from_millis(150);
const FOCUS_RECOVERY_CLASSIFY_BUDGET: Duration = Duration::from_millis(170);
const FOCUS_RESTORE_POLL_STEP: Duration = Duration::from_millis(10);
const FOCUS_PREPARATION_MIN_SETTLE: Duration = Duration::from_millis(50);
const KEYBOARD_ELIGIBILITY_PROOF_BUDGET: Duration = Duration::from_millis(650);
const MAX_RAW_WINDOW_INVENTORY: usize = 4_096;
const CURSOR_STAMP_BUDGET: Duration = Duration::from_millis(30);
const CURSOR_STAMP_POLL: Duration = Duration::from_millis(1);
const MAX_HID_POINTER_COUNTER_ADVANCE: u32 = 1_000_000;

type PostToPidFn = unsafe extern "C" fn(pid_t, *mut c_void);
type SetWindowLocationFn = unsafe extern "C" fn(*mut c_void, f64, f64);
type SetIntegerFieldFn = unsafe extern "C" fn(*mut c_void, u32, i64);
type PostEventRecordToFn = unsafe extern "C" fn(*const c_void, *const u8) -> i32;
type GetFrontProcessFn = unsafe extern "C" fn(*mut c_void) -> i32;
type GetProcessPidFn = unsafe extern "C" fn(*const c_void, *mut pid_t) -> i32;
type GetWindowOwnerFn = unsafe extern "C" fn(u32, u32, *mut u32) -> i32;
type GetConnectionPsnFn = unsafe extern "C" fn(u32, *mut c_void) -> i32;
type ConnectionIdFn = unsafe extern "C" fn() -> u32;
type GetActiveSpaceFn = unsafe extern "C" fn(u32) -> u64;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventSetLocation(event: *mut c_void, location: CGPoint);
    fn CGEventSourceCounterForEventType(
        state_id: CGEventSourceStateID,
        event_type: CGEventType,
    ) -> u32;
    fn CGEventSourceSetUserData(source: *mut c_void, user_data: i64);
}

#[derive(Clone, Copy)]
struct Symbols {
    post_to_pid: PostToPidFn,
    set_window_location: SetWindowLocationFn,
    set_integer_field: SetIntegerFieldFn,
    post_event_record: PostEventRecordToFn,
    get_front_process: GetFrontProcessFn,
    get_process_pid: GetProcessPidFn,
    get_window_owner: GetWindowOwnerFn,
    get_connection_psn: GetConnectionPsnFn,
    connection_id: ConnectionIdFn,
    get_active_space: GetActiveSpaceFn,
}

unsafe impl Send for Symbols {}
unsafe impl Sync for Symbols {}

#[derive(Clone, Copy)]
struct MacTargetDispatchRecord {
    target_pid: u32,
    target_window_id: u32,
}

struct PreparedTargetEvent {
    event: CGEvent,
    target_pid: u32,
    target_window_id: u32,
}

impl PreparedTargetEvent {
    fn new(target: &WindowDescriptor, event: CGEvent) -> Result<Self, ComputerError> {
        Ok(Self {
            event,
            target_pid: target.pid,
            target_window_id: target
                .id
                .parse::<u32>()
                .map_err(|_| input_error("invalid macOS target window id"))?,
        })
    }

    fn matches(&self, target: &WindowDescriptor) -> bool {
        self.target_pid == target.pid
            && Some(self.target_window_id) == target.id.parse::<u32>().ok()
    }
}

struct MacTargetDispatchTrace {
    target_pid: u32,
    target_window_id: u32,
    attempt_count: u32,
}

impl MacTargetDispatchTrace {
    fn new(target: &WindowDescriptor) -> Result<Self, ComputerError> {
        Ok(Self {
            target_pid: target.pid,
            target_window_id: target
                .id
                .parse::<u32>()
                .map_err(|_| input_error("invalid macOS target window id"))?,
            attempt_count: 0,
        })
    }

    fn record(&mut self, attempt: MacTargetDispatchRecord) -> Result<(), ComputerError> {
        if attempt.target_pid != self.target_pid
            || attempt.target_window_id != self.target_window_id
        {
            return Err(ComputerError::new(
                "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
                "stage=dispatchRecord;failedInvariants=inputRouteTargetBound",
            ));
        }
        self.attempt_count = self.attempt_count.checked_add(1).ok_or_else(|| {
            ComputerError::new(
                "COMPUTER_DISPATCH_EXHAUSTED",
                "The macOS target-delivery attempt count was exhausted",
            )
        })?;
        Ok(())
    }

    fn provenance(&self) -> Result<InputDeliveryProvenance, ComputerError> {
        if self.attempt_count == 0 {
            return Err(ComputerError::new(
                "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
                "stage=dispatchRecord;failedInvariants=inputRouteTargetBound",
            ));
        }
        Ok(InputDeliveryProvenance {
            route: InputDeliveryRoute::MacosTargetedProcessEvent,
            support_level: InputDeliverySupportLevel::PrivateUnsupported,
            exact_target_bound: true,
            dispatch_attempt_recorded: true,
            // This private API returns void. The record proves only that the
            // exact PID-scoped call returned, not that macOS accepted it.
            os_acceptance_signal_available: false,
            os_acceptance_observed: false,
            shared_input_seat_used: false,
            global_hid_input_used: false,
            hardware_cursor_mutation_requested: false,
        })
    }
}

impl InputDeliveryProvenance {
    fn macos_accessibility(
        attempt: &ax_macos::AxDispatchRecord,
        target: &WindowDescriptor,
        stage: InvariantStage,
    ) -> Result<Self, ComputerError> {
        let operation = match stage {
            InvariantStage::SemanticInvoke => ax_macos::AxDispatchOperation::Invoke,
            InvariantStage::SemanticSetValue => ax_macos::AxDispatchOperation::SetValue,
            _ => {
                return Err(ComputerError::new(
                    "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
                    "stage=dispatchRecord;failedInvariants=inputRouteTargetBound",
                ));
            }
        };
        if !attempt.matches(target, operation) {
            return Err(ComputerError::new(
                "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
                "stage=dispatchRecord;failedInvariants=inputRouteTargetBound",
            ));
        }
        Ok(Self {
            route: InputDeliveryRoute::MacosAccessibility,
            support_level: InputDeliverySupportLevel::PublicDocumented,
            exact_target_bound: true,
            dispatch_attempt_recorded: true,
            os_acceptance_signal_available: true,
            os_acceptance_observed: attempt.os_acceptance_observed(),
            shared_input_seat_used: false,
            global_hid_input_used: false,
            hardware_cursor_mutation_requested: false,
        })
    }
}

pub fn backend_name() -> &'static str {
    "background-window/ax+skylight+screencapturekit-stream"
}

pub fn semantic_backend_name() -> &'static str {
    "macos-accessibility"
}

pub fn semantic_ready(prompt: bool) -> bool {
    ax_macos::accessibility_ready(prompt)
}

pub fn semantic_elements(target: &WindowDescriptor) -> Result<SemanticSnapshot, ComputerError> {
    ax_macos::snapshot(target).map(|elements| SemanticSnapshot {
        elements,
        truncation_reason: None,
    })
}

pub fn invoke(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    action: &str,
    cancellation: &CommandCancellation,
) -> Result<(serde_json::Value, InvariantReport), ComputerError> {
    guarded_semantic(target, InvariantStage::SemanticInvoke, cancellation, || {
        ax_macos::invoke(target, semantic, action, cancellation)
    })
}

pub fn set_value(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    value: &str,
    cancellation: &CommandCancellation,
) -> Result<(serde_json::Value, InvariantReport), ComputerError> {
    guarded_semantic(
        target,
        InvariantStage::SemanticSetValue,
        cancellation,
        || ax_macos::set_value(target, semantic, value, cancellation),
    )
}

pub fn limitations() -> Vec<&'static str> {
    vec![
        "Native ScreenCaptureKit sharing is exact-window; input remains limited to non-minimized windows on the active macOS Space",
        "Accessibility permission is required to prove and restore exact focus/window ownership for focus-capable target-routed input",
        "Secure input, protected content, games, and some GPU surfaces can refuse background events",
        "Private SkyLight event routing may require updates after a macOS release",
        "HID-system pointer counters detect shared pointer activity but do not prove a physical device or identify its source",
    ]
}

pub fn input_ready() -> bool {
    symbols().is_some() && ax_macos::accessibility_ready(false)
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
                focus.front_pid == window.pid
                    && Some(focus.front_window_id) == window.id.parse::<u32>().ok()
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

pub fn capture_window(
    target: &mut WindowDescriptor,
) -> Result<(RgbaImage, Instant), ComputerError> {
    let window = exact_window(target)?;
    let captured_at = Instant::now();
    let image = window.capture_image().map_err(|error| {
            ComputerError::new(
                "COMPUTER_CAPTURE_FAILED",
                format!(
                    "Exact-window capture failed. Grant Screen Recording to Local Computer Helper. {error}"
                ),
            )
        })?;
    Ok((image, captured_at))
}

pub fn move_pointer_path(
    target: &WindowDescriptor,
    points: &[TargetPoint],
    step_delay: Duration,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    guarded(
        target,
        InvariantStage::PointerTrajectory,
        cancellation,
        None,
        |focus, delivery| {
            if points.is_empty() {
                return Err(input_error("synthetic pointer trajectory is empty"));
            }
            for (index, point) in points.iter().copied().enumerate() {
                let event = mouse_event(
                    target,
                    point,
                    CGEventType::MouseMoved,
                    CGMouseButton::Left,
                    0,
                    0,
                    0,
                )?;
                // Focus activation and AX main-window state are asynchronous
                // with the target application's actual event receiver. Re-prove
                // the exact receiver immediately before every trajectory event,
                // matching click, drag, scroll, key, and text dispatch instead
                // of racing the first MouseMoved against activation.
                let move_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
                prove_action_dispatch_owner(focus, target, move_deadline, true)?;
                delivery.record(post_before_deadline(
                    target,
                    &event,
                    cancellation,
                    "pointer dispatch",
                    move_deadline,
                )?)?;
                if index + 1 < points.len() {
                    thread::sleep(step_delay);
                }
            }
            Ok(())
        },
    )
}

pub fn click(
    target: &WindowDescriptor,
    point: TargetPoint,
    button: &str,
    count: usize,
    cancellation: &CommandCancellation,
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
    guarded(
        target,
        InvariantStage::ClickDispatch,
        cancellation,
        None,
        |focus, delivery| {
            let move_event = mouse_event(
                target,
                point,
                CGEventType::MouseMoved,
                CGMouseButton::Left,
                0,
                number,
                0,
            )?;
            let move_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
            prove_action_dispatch_owner(focus, target, move_deadline, true)?;
            delivery.record(post_before_deadline(
                target,
                &move_event,
                cancellation,
                "pointer dispatch",
                move_deadline,
            )?)?;
            for index in 0..count.max(1) {
                let down_event = mouse_event(
                    target,
                    point,
                    down,
                    mouse_button,
                    (index + 1) as i64,
                    number,
                    3,
                )?;
                let up_event = mouse_event(
                    target,
                    point,
                    up,
                    mouse_button,
                    (index + 1) as i64,
                    number,
                    3,
                )?;
                let press_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
                prove_action_dispatch_owner(focus, target, press_deadline, true)?;
                held_event_sequence(
                    target,
                    delivery,
                    cancellation,
                    press_deadline,
                    &down_event,
                    |_| {
                        thread::sleep(Duration::from_millis(24));
                        Ok(())
                    },
                    &up_event,
                )?;
                if index + 1 < count {
                    thread::sleep(Duration::from_millis(70));
                }
            }
            Ok(())
        },
    )
}

pub fn drag(
    target: &WindowDescriptor,
    from: TargetPoint,
    to: TargetPoint,
    duration_ms: u64,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    guarded(
        target,
        InvariantStage::DragDispatch,
        cancellation,
        None,
        |focus, delivery| {
            let move_event = mouse_event(
                target,
                from,
                CGEventType::MouseMoved,
                CGMouseButton::Left,
                0,
                0,
                0,
            )?;
            let move_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
            prove_action_dispatch_owner(focus, target, move_deadline, true)?;
            delivery.record(post_before_deadline(
                target,
                &move_event,
                cancellation,
                "pointer dispatch",
                move_deadline,
            )?)?;
            let down_event = mouse_event(
                target,
                from,
                CGEventType::LeftMouseDown,
                CGMouseButton::Left,
                1,
                0,
                3,
            )?;
            let up_event = mouse_event(
                target,
                from,
                CGEventType::LeftMouseUp,
                CGMouseButton::Left,
                1,
                0,
                3,
            )?;
            let press_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
            prove_action_dispatch_owner(focus, target, press_deadline, true)?;
            held_event_sequence(
                target,
                delivery,
                cancellation,
                press_deadline,
                &down_event,
                |delivery| {
                    let steps = (duration_ms / 16).clamp(4, 120);
                    for step in 1..=steps {
                        let progress = step as f64 / steps as f64;
                        let point = TargetPoint {
                            local_x: interpolate(from.local_x, to.local_x, progress),
                            local_y: interpolate(from.local_y, to.local_y, progress),
                            screen_x: interpolate(from.screen_x, to.screen_x, progress),
                            screen_y: interpolate(from.screen_y, to.screen_y, progress),
                        };
                        // Keep the prebuilt release event at the latest attempted
                        // point, so cancellation or a dispatch failure still sends
                        // a mouse-up at the best-known drag location.
                        retarget_mouse_event(target, &up_event, point, 1, 0, 3)?;
                        let drag_event = mouse_event(
                            target,
                            point,
                            CGEventType::LeftMouseDragged,
                            CGMouseButton::Left,
                            1,
                            0,
                            3,
                        )?;
                        let drag_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
                        prove_action_dispatch_owner(focus, target, drag_deadline, true)?;
                        delivery.record(post_before_deadline(
                            target,
                            &drag_event,
                            cancellation,
                            "pointer dispatch",
                            drag_deadline,
                        )?)?;
                        thread::sleep(Duration::from_millis((duration_ms / steps).max(1)));
                    }
                    Ok(())
                },
                &up_event,
            )
        },
    )
}

pub fn scroll(
    target: &WindowDescriptor,
    point: TargetPoint,
    delta_x: i32,
    delta_y: i32,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    guarded(
        target,
        InvariantStage::ScrollDispatch,
        cancellation,
        None,
        |focus, delivery| {
            let move_event = mouse_event(
                target,
                point,
                CGEventType::MouseMoved,
                CGMouseButton::Left,
                0,
                0,
                0,
            )?;
            let move_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
            prove_action_dispatch_owner(focus, target, move_deadline, true)?;
            delivery.record(post_before_deadline(
                target,
                &move_event,
                cancellation,
                "pointer dispatch",
                move_deadline,
            )?)?;
            let source = private_event_source()?;
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
            let event = PreparedTargetEvent::new(target, event)?;
            let scroll_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
            prove_action_dispatch_owner(focus, target, scroll_deadline, true)?;
            delivery.record(post_before_deadline(
                target,
                &event,
                cancellation,
                "scroll dispatch",
                scroll_deadline,
            )?)?;
            Ok(())
        },
    )
}

pub fn type_text(
    target: &WindowDescriptor,
    text: &str,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    let deadline = Instant::now() + Duration::from_millis(COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS);
    // Preflight proves exact WindowServer/AX top-level identity only. A
    // background app may currently have a different internal key window; the
    // dual-state focus lease below remembers and restores that exact sibling.
    ensure_keyboard_target_eligible_before(target, deadline)?;
    text_dispatch_checkpoint(cancellation, deadline)?;
    guarded(
        target,
        InvariantStage::TextDispatch,
        cancellation,
        Some(deadline),
        |focus, delivery| {
            let mut characters = text.chars().peekable();
            let mut dispatched = 0_usize;
            while let Some(character) = characters.next() {
                text_dispatch_checkpoint(cancellation, deadline)?;
                let value = character.to_string();
                let down = keyboard_event(target, 0, true, CGEventFlags::empty(), Some(&value))?;
                let up = keyboard_event(target, 0, false, CGEventFlags::empty(), Some(&value))?;
                // A text scalar can itself move focus (Tab/newline), and the app or
                // user can react between scalars. Re-prove the actual focused
                // window and focused-element owner after event construction and
                // immediately before every key-down. Releases stay unconditional.
                ensure_text_keyboard_receiver(focus, target, dispatched, deadline)?;
                text_dispatch_checkpoint(cancellation, deadline)?;
                held_event_sequence(
                    target,
                    delivery,
                    cancellation,
                    deadline,
                    &down,
                    |_| Ok(()),
                    &up,
                )?;
                dispatched += 1;
                if characters.peek().is_some() {
                    pace_text_dispatch(cancellation, deadline, TEXT_EVENT_PACE)?;
                }
            }
            ensure_text_keyboard_receiver(focus, target, dispatched, deadline)?;
            text_dispatch_checkpoint(cancellation, deadline)?;
            Ok(())
        },
    )
}

fn text_dispatch_checkpoint(
    cancellation: &CommandCancellation,
    deadline: Instant,
) -> Result<(), ComputerError> {
    cancellation.check("native text dispatch")?;
    if Instant::now() < deadline {
        return Ok(());
    }
    Err(if cancellation.was_dispatched() {
        ComputerError::new(
            "COMPUTER_OUTCOME_UNKNOWN",
            format!(
                "Native text dispatch exceeded its {COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS} ms delivery budget after input began; observe again and do not automatically retry"
            ),
        )
    } else {
        ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            format!(
                "Native text dispatch could not begin within its {COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS} ms delivery budget"
            ),
        )
    })
}

fn pace_text_dispatch(
    cancellation: &CommandCancellation,
    deadline: Instant,
    delay: Duration,
) -> Result<(), ComputerError> {
    text_dispatch_checkpoint(cancellation, deadline)?;
    thread::sleep(delay);
    text_dispatch_checkpoint(cancellation, deadline)
}

pub fn key(
    target: &WindowDescriptor,
    chord: &str,
    cancellation: &CommandCancellation,
) -> Result<InvariantReport, ComputerError> {
    ensure_keyboard_target_eligible(target)?;
    let parts = chord.split('+').map(str::trim).collect::<Vec<_>>();
    let last = parts.last().copied().unwrap_or("");
    let flags = modifier_flags(&parts[..parts.len().saturating_sub(1)]);
    let keycode = keycode(last).ok_or_else(|| {
        ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            format!("The macOS background backend does not map key {last}"),
        )
    })?;
    guarded(
        target,
        InvariantStage::KeyDispatch,
        cancellation,
        None,
        |focus, delivery| {
            let down = keyboard_event(target, keycode, true, flags, None)?;
            let up = keyboard_event(target, keycode, false, flags, None)?;
            let receiver_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
            // Non-text chords are routed through the key window's responder chain;
            // an AXFocusedUIElement is not required (for example, Escape may have
            // no focused control), but the exact AXFocusedWindow is mandatory.
            prove_action_dispatch_owner(focus, target, receiver_deadline, false)?;
            ax_macos::ensure_keyboard_receiver_before(target, false, true, receiver_deadline)?;
            held_event_sequence(
                target,
                delivery,
                cancellation,
                receiver_deadline,
                &down,
                |_| {
                    thread::sleep(Duration::from_millis(18));
                    Ok(())
                },
                &up,
            )
        },
    )
}

fn ensure_keyboard_target_eligible(target: &WindowDescriptor) -> Result<(), ComputerError> {
    ensure_keyboard_target_eligible_before(
        target,
        Instant::now() + KEYBOARD_ELIGIBILITY_PROOF_BUDGET,
    )
}

fn ensure_keyboard_target_eligible_before(
    target: &WindowDescriptor,
    deadline: Instant,
) -> Result<(), ComputerError> {
    let target_id = target
        .id
        .parse::<u32>()
        .map_err(|_| input_error("invalid window id"))?;
    let inventory = raw_keyboard_window_inventory_before(deadline)?;
    if !raw_keyboard_target_matches(target, target_id, &inventory) {
        return Err(ComputerError::new(
            "COMPUTER_STALE_FRAME",
            "The exact macOS keyboard target changed after the captured frame",
        ));
    }
    ax_macos::ensure_keyboard_target_eligible_before(target, deadline)
}

fn ensure_text_keyboard_receiver(
    focus: Option<&FocusLease>,
    target: &WindowDescriptor,
    dispatched: usize,
    deadline: Instant,
) -> Result<(), ComputerError> {
    let proof = prove_action_dispatch_owner(focus, target, deadline, false)
        .and_then(|_| ax_macos::ensure_keyboard_receiver_before(target, true, true, deadline));
    map_text_receiver_failure(proof, dispatched)
}

fn prove_action_dispatch_owner(
    focus: Option<&FocusLease>,
    target: &WindowDescriptor,
    deadline: Instant,
    finish_with_target_window: bool,
) -> Result<(), ComputerError> {
    if let Some(focus) = focus {
        focus.prove_dispatch_owner_before(deadline)?;
        prove_target_focus_window(target, deadline)?;
        focus.prove_dispatch_owner_before(deadline)?;
        if finish_with_target_window {
            prove_target_focus_window(target, deadline)?;
        }
        return Ok(());
    }
    let target_window_id = target
        .id
        .parse::<u32>()
        .map_err(|_| input_error("invalid window id"))?;
    let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
    let front_before = front_process_identity(symbols)?;
    let target_focus_before =
        ax_macos::target_application_focus_state_before(target.pid, deadline)?;
    let front_middle = front_process_identity(symbols)?;
    let target_focus_after = ax_macos::target_application_focus_state_before(target.pid, deadline)?;
    let front_after = front_process_identity(symbols)?;
    if front_before == front_middle
        && front_middle == front_after
        && front_after.1 == target.pid
        && target_focus_before == target_focus_after
        && target_focus_after.frontmost
        && target_focus_after.window_id == target_window_id
        && target_focus_after.main_window_id == target_window_id
        && Instant::now() < deadline
    {
        Ok(())
    } else {
        Err(background_unavailable(
            "The selected macOS window is no longer the exact foreground focus owner",
        ))
    }
}

fn prove_target_focus_window(
    target: &WindowDescriptor,
    deadline: Instant,
) -> Result<(), ComputerError> {
    let target_window_id = target
        .id
        .parse::<u32>()
        .map_err(|_| input_error("invalid window id"))?;
    if ax_macos::target_application_focus_state_before(target.pid, deadline).is_ok_and(|focus| {
        focus.frontmost
            && focus.window_id == target_window_id
            && focus.main_window_id == target_window_id
    }) {
        Ok(())
    } else {
        Err(background_unavailable(
            "The selected macOS target is no longer the exact focused window",
        ))
    }
}

fn map_text_receiver_failure(
    proof: Result<(), ComputerError>,
    dispatched: usize,
) -> Result<(), ComputerError> {
    match proof {
        Ok(()) => Ok(()),
        Err(error) if dispatched == 0 => Err(error),
        Err(_) => Err(ComputerError::new(
            "COMPUTER_OUTCOME_UNKNOWN",
            "The exact macOS text receiver could not be re-proven after input began; observe again and do not automatically retry",
        )),
    }
}

#[derive(Clone, Copy, Debug)]
struct RawKeyboardWindow {
    window_id: u32,
    owner_pid: u32,
    layer: i32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    // OptionAll prevents the target from disappearing from inventory merely
    // because it became ineligible. Mutation still requires a known on-screen
    // target on the active Space; unknown is fail-closed.
    is_on_screen: Option<bool>,
    // Sharing state does not identify the keyboard receiver. It is retained in
    // the independent inventory but never used to distinguish real windows
    // from the ScreenCaptureKit indicator.
    _sharing_state: Option<i32>,
}

fn raw_keyboard_target_matches(
    target: &WindowDescriptor,
    target_id: u32,
    inventory: &[RawKeyboardWindow],
) -> bool {
    let matching = inventory
        .iter()
        .filter(|window| window.window_id == target_id)
        .collect::<Vec<_>>();
    matching.len() == 1
        && matching[0].owner_pid == target.pid
        && matching[0].layer == 0
        && matching[0].is_on_screen == Some(true)
        && matching[0].x as i32 == target.x
        && matching[0].y as i32 == target.y
        && matching[0].width as u32 == target.width
        && matching[0].height as u32 == target.height
}

fn raw_focus_window_restorable(pid: u32, window_id: u32, inventory: &[RawKeyboardWindow]) -> bool {
    let matching = inventory
        .iter()
        .filter(|window| window.window_id == window_id)
        .collect::<Vec<_>>();
    matching.len() == 1
        && matching[0].owner_pid == pid
        && matching[0].layer == 0
        && matching[0].is_on_screen == Some(true)
}

fn raw_keyboard_window_inventory_before(
    deadline: Instant,
) -> Result<Vec<RawKeyboardWindow>, ComputerError> {
    if Instant::now() >= deadline {
        return Err(background_unavailable(
            "The macOS exact-window proof deadline elapsed",
        ));
    }
    let array = copy_window_info(kCGWindowListOptionAll, kCGNullWindowID).ok_or_else(|| {
        ComputerError::new(
            "COMPUTER_BACKGROUND_UNAVAILABLE",
            "Could not inspect the macOS WindowServer keyboard target",
        )
    })?;
    if Instant::now() >= deadline || array.len() as usize > MAX_RAW_WINDOW_INVENTORY {
        return Err(background_unavailable(
            "Could not complete the bounded macOS WindowServer proof",
        ));
    }
    let dictionary_type = CFDictionary::<*const c_void, *const c_void>::type_id();
    let mut output = Vec::with_capacity(array.len() as usize);
    for item in array.iter() {
        if Instant::now() >= deadline {
            return Err(background_unavailable(
                "The macOS WindowServer proof exceeded its deadline",
            ));
        }
        let item = *item as CFTypeRef;
        if unsafe { CFGetTypeID(item) } != dictionary_type {
            continue;
        }
        let dictionary: CFDictionary<*const c_void, *const c_void> =
            unsafe { CFDictionary::wrap_under_get_rule(item as _) };
        let Some(window_id) = dictionary_i64(&dictionary, "kCGWindowNumber")
            .and_then(|value| u32::try_from(value).ok())
        else {
            continue;
        };
        let Some(owner_pid) = dictionary_i64(&dictionary, "kCGWindowOwnerPID")
            .and_then(|value| u32::try_from(value).ok())
        else {
            continue;
        };
        let Some(layer) = dictionary_i64(&dictionary, "kCGWindowLayer")
            .and_then(|value| i32::try_from(value).ok())
        else {
            continue;
        };
        let Some(bounds) = dictionary_value(&dictionary, "kCGWindowBounds") else {
            continue;
        };
        if unsafe { CFGetTypeID(bounds) } != dictionary_type {
            continue;
        }
        let bounds: CFDictionary<*const c_void, *const c_void> =
            unsafe { CFDictionary::wrap_under_get_rule(bounds as _) };
        let (Some(x), Some(y), Some(width), Some(height)) = (
            dictionary_f64(&bounds, "X"),
            dictionary_f64(&bounds, "Y"),
            dictionary_f64(&bounds, "Width"),
            dictionary_f64(&bounds, "Height"),
        ) else {
            continue;
        };
        output.push(RawKeyboardWindow {
            window_id,
            owner_pid,
            layer,
            x,
            y,
            width,
            height,
            is_on_screen: dictionary_bool(&dictionary, "kCGWindowIsOnscreen"),
            _sharing_state: dictionary_i64(&dictionary, "kCGWindowSharingState")
                .and_then(|value| i32::try_from(value).ok()),
        });
    }
    if Instant::now() >= deadline {
        return Err(background_unavailable(
            "The macOS WindowServer proof exceeded its deadline",
        ));
    }
    Ok(output)
}

fn dictionary_value(
    dictionary: &CFDictionary<*const c_void, *const c_void>,
    name: &str,
) -> Option<CFTypeRef> {
    let key = CFString::new(name);
    dictionary
        .find(key.as_concrete_TypeRef() as *const c_void)
        .map(|value| *value as CFTypeRef)
}

fn dictionary_i64(
    dictionary: &CFDictionary<*const c_void, *const c_void>,
    name: &str,
) -> Option<i64> {
    let value = dictionary_value(dictionary, name)?;
    (unsafe { CFGetTypeID(value) } == CFNumber::type_id())
        .then(|| unsafe { CFNumber::wrap_under_get_rule(value as _) }.to_i64())
        .flatten()
}

fn dictionary_f64(
    dictionary: &CFDictionary<*const c_void, *const c_void>,
    name: &str,
) -> Option<f64> {
    let value = dictionary_value(dictionary, name)?;
    (unsafe { CFGetTypeID(value) } == CFNumber::type_id())
        .then(|| unsafe { CFNumber::wrap_under_get_rule(value as _) }.to_f64())
        .flatten()
}

fn dictionary_bool(
    dictionary: &CFDictionary<*const c_void, *const c_void>,
    name: &str,
) -> Option<bool> {
    let value = dictionary_value(dictionary, name)?;
    (unsafe { CFGetTypeID(value) } == CFBoolean::type_id()).then(|| {
        let flag: bool = unsafe { CFBoolean::wrap_under_get_rule(value as _) }.into();
        flag
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
        // `windows` resolves focus separately through the exact AX/PSN oracle;
        // xcap's Chromium focus flag is not an authority for one exact window.
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
    stage: InvariantStage,
    cancellation: &CommandCancellation,
    preparation_deadline: Option<Instant>,
    action: impl FnOnce(Option<&FocusLease>, &mut MacTargetDispatchTrace) -> Result<(), ComputerError>,
) -> Result<InvariantReport, ComputerError> {
    let before = DesktopSnapshot::capture()?;
    let mut delivery = MacTargetDispatchTrace::new(target)?;
    // Mouse-move events addressed to an exact non-main window can be dropped
    // when another same-process window remains the application's receiver.
    // Borrow the same bounded, no-raise focus lease used by the other native
    // pointer paths so hover/move delivery reaches the requested window, then
    // restore both the target application's prior receiver and the user's
    // foreground owner before returning.
    let focus = activate_without_raise(target, &before, cancellation, preparation_deadline)?;
    let action_result = action(focus.as_ref(), &mut delivery);
    cancellation.mark_verification_started();
    let restore_result = focus.map(FocusLease::restore).transpose();
    thread::sleep(Duration::from_millis(35));
    let (input_delivery, delivery_error) = match delivery.provenance() {
        Ok(provenance) => (provenance, None),
        Err(error) => (InputDeliveryProvenance::unverified(), Some(error)),
    };
    let report = before.compare(&DesktopSnapshot::capture()?, input_delivery);
    if action_result.is_ok()
        && let Some(error) = delivery_error
    {
        return Err(error);
    }
    report.clone().assert_held(stage)?;
    restore_result?;
    action_result?;
    Ok(report)
}

fn guarded_semantic(
    target: &WindowDescriptor,
    stage: InvariantStage,
    cancellation: &CommandCancellation,
    action: impl FnOnce() -> Result<ax_macos::AxDispatchAttempt, ComputerError>,
) -> Result<(serde_json::Value, InvariantReport), ComputerError> {
    let before = DesktopSnapshot::capture()?;
    exact_window(target)?;
    cancellation.check("semantic resolution")?;
    let action_result = action();
    cancellation.mark_verification_started();
    thread::sleep(Duration::from_millis(35));
    let delivery = action_result?;
    let (dispatch, backend_effect) = delivery.into_parts();
    let report = before.compare(
        &DesktopSnapshot::capture()?,
        InputDeliveryProvenance::macos_accessibility(&dispatch, target, stage)?,
    );
    finish_semantic_after_snapshot(report, stage, backend_effect)
}

fn finish_semantic_after_snapshot(
    report: InvariantReport,
    stage: InvariantStage,
    backend_effect: Result<serde_json::Value, ComputerError>,
) -> Result<(serde_json::Value, InvariantReport), ComputerError> {
    // Safety verification intentionally precedes the target postcondition
    // result. Once dispatch began, a backend error must never skip or mask a
    // shared-desktop contract violation observed by the after-snapshot.
    let report = report.assert_held(stage)?;
    Ok((backend_effect?, report))
}

fn mouse_event(
    target: &WindowDescriptor,
    point: TargetPoint,
    event_type: CGEventType,
    button: CGMouseButton,
    click_state: i64,
    button_number: i64,
    subtype: i64,
) -> Result<PreparedTargetEvent, ComputerError> {
    let event = CGEvent::new_mouse_event(
        private_event_source()?,
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
    PreparedTargetEvent::new(target, event)
}

fn retarget_mouse_event(
    target: &WindowDescriptor,
    event: &PreparedTargetEvent,
    point: TargetPoint,
    click_state: i64,
    button_number: i64,
    subtype: i64,
) -> Result<(), ComputerError> {
    if !event.matches(target) {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
            "stage=preparedEvent;failedInvariants=inputRouteTargetBound",
        ));
    }
    let raw = event.event.as_ptr() as *mut c_void;
    unsafe {
        CGEventSetLocation(
            raw,
            CGPoint::new(point.screen_x as f64, point.screen_y as f64),
        )
    };
    stamp(target, point, raw, click_state, button_number, subtype)
}

fn keyboard_event(
    target: &WindowDescriptor,
    keycode: u16,
    down: bool,
    flags: CGEventFlags,
    text: Option<&str>,
) -> Result<PreparedTargetEvent, ComputerError> {
    let event = CGEvent::new_keyboard_event(private_event_source()?, keycode, down)
        .map_err(|_| input_error("CGEventCreateKeyboardEvent failed"))?;
    event.set_flags(flags);
    if let Some(text) = text {
        event.set_string(text);
    }
    stamp_keyboard(target, &event)?;
    PreparedTargetEvent::new(target, event)
}

fn held_event_sequence<T>(
    target: &WindowDescriptor,
    delivery: &mut MacTargetDispatchTrace,
    cancellation: &CommandCancellation,
    first_post_deadline: Instant,
    down: &PreparedTargetEvent,
    action: impl FnOnce(&mut MacTargetDispatchTrace) -> Result<T, ComputerError>,
    up: &PreparedTargetEvent,
) -> Result<T, ComputerError> {
    delivery.record(post_before_deadline(
        target,
        down,
        cancellation,
        "held input press",
        first_post_deadline,
    )?)?;
    let action_result = action(delivery);
    match post_release(target, up) {
        Ok(attempt) => {
            delivery.record(attempt)?;
            action_result
        }
        Err(release_error) => Err(release_error),
    }
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

fn post_before_deadline(
    target: &WindowDescriptor,
    event: &PreparedTargetEvent,
    cancellation: &CommandCancellation,
    boundary: &str,
    deadline: Instant,
) -> Result<MacTargetDispatchRecord, ComputerError> {
    ensure_dispatch_deadline(cancellation, boundary, deadline)?;
    cancellation.begin_side_effect(boundary)?;
    // Dispatch accounting can block briefly on the command phase lock. Never
    // let an authorization proof that expired during that boundary post.
    ensure_dispatch_deadline(cancellation, boundary, deadline)?;
    post_release(target, event)
}

fn ensure_dispatch_deadline(
    cancellation: &CommandCancellation,
    boundary: &str,
    deadline: Instant,
) -> Result<(), ComputerError> {
    cancellation.check(boundary)?;
    if Instant::now() < deadline {
        return Ok(());
    }
    Err(if cancellation.was_dispatched() {
        ComputerError::new(
            "COMPUTER_OUTCOME_UNKNOWN",
            format!(
                "The exact macOS input authorization expired after dispatch accounting at {boundary}; observe again and do not automatically retry"
            ),
        )
    } else {
        background_unavailable("The exact macOS input authorization expired before native dispatch")
    })
}

fn post_release(
    target: &WindowDescriptor,
    event: &PreparedTargetEvent,
) -> Result<MacTargetDispatchRecord, ComputerError> {
    if !event.matches(target) {
        return Err(ComputerError::new(
            "COMPUTER_BACKGROUND_CONTRACT_VIOLATION",
            "stage=preparedEvent;failedInvariants=inputRouteTargetBound",
        ));
    }
    let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
    let target_window_id = target
        .id
        .parse::<u32>()
        .map_err(|_| input_error("invalid macOS target window id"))?;
    unsafe { (symbols.post_to_pid)(target.pid as pid_t, event.event.as_ptr() as *mut c_void) };
    Ok(MacTargetDispatchRecord {
        target_pid: target.pid,
        target_window_id,
    })
}

fn activate_without_raise(
    target: &WindowDescriptor,
    before: &DesktopSnapshot,
    cancellation: &CommandCancellation,
    action_deadline: Option<Instant>,
) -> Result<Option<FocusLease>, ComputerError> {
    let window_id = target
        .id
        .parse::<u32>()
        .map_err(|_| input_error("invalid window id"))?;
    if !focus_preparation_needed(before.front_pid, target.pid) {
        ensure_same_process_mutation_target(target, before)?;
        return Ok(None);
    }
    let preparation_deadline = action_deadline.unwrap_or_else(|| {
        Instant::now() + Duration::from_millis(COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS)
    });
    let previous_focus =
        ax_macos::application_focus_state_before(before.front_pid, preparation_deadline)?;
    let target_previous_focus =
        ax_macos::target_application_focus_state_before(target.pid, preparation_deadline)?;
    let target_main_window =
        ax_macos::exact_target_main_window_before(target.pid, window_id, preparation_deadline)?;
    let target_previous_main_window = (target_previous_focus.window_id != window_id)
        .then(|| {
            ax_macos::exact_target_main_window_before(
                target.pid,
                target_previous_focus.window_id,
                preparation_deadline,
            )
        })
        .transpose()?;
    if !previous_focus.frontmost
        || previous_focus.window_id != before.front_window_id
        || previous_focus.main_window_id != before.front_window_id
        || target_previous_focus.frontmost
        || target_previous_focus.main_window_id != target_previous_focus.window_id
    {
        return Err(background_unavailable(
            "Could not prove the macOS focus owners before background preparation",
        ));
    }
    let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
    let destination = exact_window_process(symbols, target.pid, window_id)?;
    if target_previous_focus.window_id != window_id
        && exact_window_process(symbols, target.pid, target_previous_focus.window_id)?
            != destination
    {
        return Err(background_unavailable(
            "macOS could not bind both exact target windows to one process connection",
        ));
    }
    let lease = FocusLease {
        symbols,
        previous_psn: before.front_process,
        previous_pid: before.front_pid,
        previous_window_id: previous_focus.window_id,
        target_psn: destination,
        target_pid: target.pid,
        target_window_id: window_id,
        target_previous_window_id: target_previous_focus.window_id,
        target_main_window,
        target_previous_main_window,
    };
    cancellation.check("background focus preparation")?;
    let selected_phase = if lease.target_previous_window_id == lease.target_window_id {
        FocusLeasePhase::Restored
    } else {
        let selection_deadline = std::cmp::min(
            preparation_deadline,
            Instant::now() + FOCUS_RESTORE_PROOF_BUDGET,
        );
        let selected = match lease.set_target_main_window_before(
            FocusLeasePhase::Restored,
            true,
            true,
            lease.target_window_id,
            cancellation,
            "target exact-window selection",
            selection_deadline,
        ) {
            Ok(selected) => selected,
            Err(error) => {
                lease.restore_previous_after_failed_activation(true, false)?;
                assert_snapshot_held(before)?;
                return Err(error);
            }
        };
        let selected_result = lease.await_phase_before(
            FocusLeasePhase::RestoredWithInactiveTargetRequested,
            true,
            InvariantStage::FocusPreparation,
            None,
            selection_deadline,
            FOCUS_PREPARATION_MIN_SETTLE,
        );
        let cancellation_result = cancellation.check("target exact-window selection");
        if !selected || selected_result.is_err() || cancellation_result.is_err() {
            let error = cancellation_result
                .err()
                .or_else(|| selected_result.err())
                .unwrap_or_else(|| {
                    background_unavailable(
                        "macOS could not select the exact target window without activation",
                    )
                });
            lease.restore_previous_after_failed_activation(true, false)?;
            assert_snapshot_held(before)?;
            return Err(error);
        }
        FocusLeasePhase::RestoredWithInactiveTargetRequested
    };

    let user_release_deadline = std::cmp::min(
        preparation_deadline,
        Instant::now() + FOCUS_RESTORE_PROOF_BUDGET,
    );
    let user_defocused = match lease.post_phase_transition_before(
        selected_phase,
        true,
        true,
        &lease.previous_psn,
        lease.previous_window_id,
        FocusOperation::Defocus,
        cancellation,
        "background focus preparation",
        user_release_deadline,
    ) {
        Ok(posted) => posted,
        Err(error) => {
            lease.restore_previous_after_failed_activation(true, false)?;
            assert_snapshot_held(before)?;
            return Err(error);
        }
    };
    let released_result = lease.await_phase_before(
        FocusLeasePhase::ReleasedWithInactiveTargetRequested,
        true,
        InvariantStage::FocusPreparation,
        None,
        user_release_deadline,
        Duration::ZERO,
    );
    let cancellation_result = cancellation.check("target background focus");
    if !user_defocused || released_result.is_err() || cancellation_result.is_err() {
        let error = cancellation_result
            .err()
            .or_else(|| released_result.err())
            .unwrap_or_else(|| {
                background_unavailable(
                    "macOS could not release the prior focus for exact-window routing",
                )
            });
        lease.restore_previous_after_failed_activation(true, false)?;
        assert_snapshot_held(before)?;
        return Err(error);
    }

    let target_focus_deadline = std::cmp::min(
        preparation_deadline,
        Instant::now() + FOCUS_RESTORE_PROOF_BUDGET,
    );
    let target_focused = match lease.post_phase_transition_before(
        FocusLeasePhase::ReleasedWithInactiveTargetRequested,
        true,
        false,
        &destination,
        window_id,
        FocusOperation::Focus,
        cancellation,
        "target background focus",
        target_focus_deadline,
    ) {
        Ok(posted) => posted,
        Err(error) => {
            lease.restore_previous_after_failed_activation(true, true)?;
            assert_snapshot_held(before)?;
            return Err(error);
        }
    };
    let prepared_result = lease.await_phase_before(
        FocusLeasePhase::ReleasedWithTargetRequested,
        true,
        InvariantStage::FocusPreparation,
        None,
        target_focus_deadline,
        FOCUS_PREPARATION_MIN_SETTLE,
    );
    let cancellation_result = cancellation.check("target background focus");
    if !target_focused || prepared_result.is_err() || cancellation_result.is_err() {
        let error = cancellation_result
            .err()
            .or_else(|| prepared_result.err())
            .unwrap_or_else(|| {
                background_unavailable("macOS could not establish exact-window background focus")
            });
        lease.restore_previous_after_failed_activation(true, true)?;
        assert_snapshot_held(before)?;
        return Err(error);
    }
    if lease.target_previous_window_id != lease.target_window_id {
        let make_key_deadline = std::cmp::min(
            preparation_deadline,
            Instant::now() + FOCUS_RESTORE_PROOF_BUDGET,
        );
        let made_key = match lease.post_target_make_key_window_for_activation_before(
            lease.target_window_id,
            cancellation,
            InvariantStage::FocusPreparation,
            make_key_deadline,
        ) {
            Ok(posted) => posted,
            Err(error) => {
                lease.restore_previous_after_failed_activation(true, true)?;
                assert_snapshot_held(before)?;
                return Err(error);
            }
        };
        let committed_result = lease.await_phase_before(
            FocusLeasePhase::ReleasedWithTargetRequested,
            true,
            InvariantStage::FocusPreparation,
            None,
            make_key_deadline,
            FOCUS_PREPARATION_MIN_SETTLE,
        );
        let cancellation_result = cancellation.check("target exact receiver preparation");
        if !made_key || committed_result.is_err() || cancellation_result.is_err() {
            let error = cancellation_result
                .err()
                .or_else(|| committed_result.err())
                .unwrap_or_else(|| {
                    background_unavailable(
                        "macOS could not commit the exact target application's event receiver",
                    )
                });
            lease.restore_previous_after_failed_activation(true, true)?;
            assert_snapshot_held(before)?;
            return Err(error);
        }
    }
    Ok(Some(lease))
}

fn ensure_same_process_mutation_target(
    target: &WindowDescriptor,
    before: &DesktopSnapshot,
) -> Result<(), ComputerError> {
    if before.front_pid != target.pid {
        return Ok(());
    }
    let target_window_id = target
        .id
        .parse::<u32>()
        .map_err(|_| input_error("invalid window id"))?;
    let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
    let front_before = front_process_identity(symbols).ok();
    let focus = ax_macos::target_application_focus_state(target.pid)?;
    let front_after = front_process_identity(symbols).ok();
    if same_process_mutation_target_matches(
        before.front_process,
        before.front_pid,
        before.front_window_id,
        target.pid,
        target_window_id,
        front_before,
        focus,
        front_after,
    ) {
        Ok(())
    } else {
        Err(background_unavailable(
            "The selected macOS window is not the foreground application's exact receiver",
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn same_process_mutation_target_matches(
    expected_front_process: [u8; 8],
    snapshot_front_pid: u32,
    snapshot_front_window_id: u32,
    target_pid: u32,
    target_window_id: u32,
    front_before: Option<([u8; 8], u32)>,
    focus: ax_macos::ApplicationFocusState,
    front_after: Option<([u8; 8], u32)>,
) -> bool {
    snapshot_front_pid == target_pid
        && snapshot_front_window_id == target_window_id
        && stable_front_focus_owner_matches(
            expected_front_process,
            snapshot_front_pid,
            target_window_id,
            true,
            front_before,
            Some(focus),
            front_after,
        )
}

fn focus_preparation_needed(front_pid: u32, target_pid: u32) -> bool {
    front_pid != target_pid
}

fn assert_snapshot_held(before: &DesktopSnapshot) -> Result<(), ComputerError> {
    thread::sleep(Duration::from_millis(35));
    before
        .compare(
            &DesktopSnapshot::capture()?,
            InputDeliveryProvenance::unverified(),
        )
        .assert_environment_held(InvariantStage::FocusPreparation)
        .map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusOperation {
    Focus,
    Defocus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusLeasePhase {
    Restored,
    RestoredWithInactiveTargetRequested,
    ReleasedWithTargetPrior,
    ReleasedWithTargetRequested,
    ReleasedWithInactiveTargetRequested,
    ReleasedWithActiveTargetPrior,
    Unknown,
}

fn post_focus_record(
    symbols: &Symbols,
    process: &[u8; 8],
    window_id: u32,
    operation: FocusOperation,
) -> bool {
    let mut record = [0u8; 0xF8];
    record[0x04] = 0xF8;
    record[0x08] = 0x0D;
    record[0x3C..0x40].copy_from_slice(&window_id.to_le_bytes());
    record[0x8A] = match operation {
        FocusOperation::Focus => 0x01,
        FocusOperation::Defocus => 0x02,
    };
    unsafe { (symbols.post_event_record)(process.as_ptr().cast(), record.as_ptr()) == 0 }
}

/// Posts the target-only make-key pair used by AppKit to commit an exact key
/// window after a privately active sibling has hosted a field editor. Unlike
/// yabai's foreground-focus operation, this deliberately does not call
/// `_SLPSSetFrontProcessWithOptions` and never raises a window.
fn post_make_key_window_record(symbols: &Symbols, process: &[u8; 8], window_id: u32) -> bool {
    let mut record = [0u8; 0xF8];
    record[0x04] = 0xF8;
    record[0x3A] = 0x10;
    record[0x3C..0x40].copy_from_slice(&window_id.to_le_bytes());
    record[0x20..0x30].fill(0xFF);
    record[0x08] = 0x01;
    let first =
        unsafe { (symbols.post_event_record)(process.as_ptr().cast(), record.as_ptr()) == 0 };
    record[0x08] = 0x02;
    let second =
        unsafe { (symbols.post_event_record)(process.as_ptr().cast(), record.as_ptr()) == 0 };
    first && second
}

struct FocusLease {
    symbols: &'static Symbols,
    previous_psn: [u8; 8],
    previous_pid: u32,
    previous_window_id: u32,
    target_psn: [u8; 8],
    target_pid: u32,
    target_window_id: u32,
    target_previous_window_id: u32,
    target_main_window: ax_macos::ExactTargetMainWindow,
    target_previous_main_window: Option<ax_macos::ExactTargetMainWindow>,
}

#[derive(Clone, Copy, Debug)]
struct FocusLeaseFacts {
    user_frontmost: bool,
    target_frontmost: bool,
    target_window_id: u32,
    target_main_window_id: u32,
    target_process_current: bool,
    user_raw_restorable: bool,
    target_prior_raw_restorable: bool,
    target_requested_raw_restorable: bool,
}

impl FocusLeaseFacts {
    fn matches_phase(
        self,
        phase: FocusLeasePhase,
        target_previous_window_id: u32,
        target_window_id: u32,
    ) -> bool {
        match phase {
            FocusLeasePhase::Restored => {
                self.user_frontmost
                    && !self.target_frontmost
                    && self.target_window_id == target_previous_window_id
                    && self.target_main_window_id == target_previous_window_id
            }
            FocusLeasePhase::RestoredWithInactiveTargetRequested => {
                self.user_frontmost
                    && !self.target_frontmost
                    && self.target_window_id == target_window_id
                    && self.target_main_window_id == target_window_id
            }
            FocusLeasePhase::ReleasedWithTargetPrior => {
                !self.user_frontmost
                    && !self.target_frontmost
                    && self.target_window_id == target_previous_window_id
                    && self.target_main_window_id == target_previous_window_id
            }
            FocusLeasePhase::ReleasedWithTargetRequested => {
                !self.user_frontmost
                    && self.target_frontmost
                    && self.target_window_id == target_window_id
                    && self.target_main_window_id == target_window_id
            }
            FocusLeasePhase::ReleasedWithInactiveTargetRequested => {
                !self.user_frontmost
                    && !self.target_frontmost
                    && self.target_window_id == target_window_id
                    && self.target_main_window_id == target_window_id
            }
            FocusLeasePhase::ReleasedWithActiveTargetPrior => {
                !self.user_frontmost
                    && self.target_frontmost
                    && self.target_window_id == target_previous_window_id
                    && self.target_main_window_id == target_previous_window_id
            }
            FocusLeasePhase::Unknown => false,
        }
    }

    fn classify_recovery(
        self,
        target_previous_window_id: u32,
        target_window_id: u32,
        target_selection_may_be_changed: bool,
        target_may_be_prepared: bool,
    ) -> FocusLeasePhase {
        if self.matches_phase(
            FocusLeasePhase::Restored,
            target_previous_window_id,
            target_window_id,
        ) {
            return FocusLeasePhase::Restored;
        }
        let restored_selected = self.matches_phase(
            FocusLeasePhase::RestoredWithInactiveTargetRequested,
            target_previous_window_id,
            target_window_id,
        );
        let requested_active = self.matches_phase(
            FocusLeasePhase::ReleasedWithTargetRequested,
            target_previous_window_id,
            target_window_id,
        );
        let requested_inactive = self.matches_phase(
            FocusLeasePhase::ReleasedWithInactiveTargetRequested,
            target_previous_window_id,
            target_window_id,
        );
        let prior_inactive = self.matches_phase(
            FocusLeasePhase::ReleasedWithTargetPrior,
            target_previous_window_id,
            target_window_id,
        );
        let prior_active = self.matches_phase(
            FocusLeasePhase::ReleasedWithActiveTargetPrior,
            target_previous_window_id,
            target_window_id,
        );
        if !target_selection_may_be_changed {
            return if prior_inactive {
                FocusLeasePhase::ReleasedWithTargetPrior
            } else {
                FocusLeasePhase::Unknown
            };
        }
        if !target_may_be_prepared {
            return if restored_selected {
                FocusLeasePhase::RestoredWithInactiveTargetRequested
            } else if requested_inactive {
                FocusLeasePhase::ReleasedWithInactiveTargetRequested
            } else if prior_inactive {
                FocusLeasePhase::ReleasedWithTargetPrior
            } else {
                FocusLeasePhase::Unknown
            };
        }
        if requested_active {
            FocusLeasePhase::ReleasedWithTargetRequested
        } else if restored_selected {
            FocusLeasePhase::RestoredWithInactiveTargetRequested
        } else if prior_inactive {
            FocusLeasePhase::ReleasedWithTargetPrior
        } else if requested_inactive {
            FocusLeasePhase::ReleasedWithInactiveTargetRequested
        } else if prior_active {
            FocusLeasePhase::ReleasedWithActiveTargetPrior
        } else {
            FocusLeasePhase::Unknown
        }
    }
}

impl FocusLease {
    #[allow(clippy::too_many_arguments)]
    fn post_phase_transition_before(
        &self,
        phase: FocusLeasePhase,
        require_requested_raw: bool,
        expected_user_frontmost: bool,
        process: &[u8; 8],
        window_id: u32,
        operation: FocusOperation,
        cancellation: &CommandCancellation,
        boundary: &str,
        deadline: Instant,
    ) -> Result<bool, ComputerError> {
        let authorize = || {
            self.prove_phase_before(
                phase,
                require_requested_raw,
                InvariantStage::FocusPreparation,
                deadline,
            )?;
            if self.user_owner_matches_before(expected_user_frontmost, deadline) {
                Ok(())
            } else {
                Err(background_unavailable(
                    "The exact macOS user focus owner changed during preparation",
                ))
            }
        };
        authorize()?;
        ensure_dispatch_deadline(cancellation, boundary, deadline)?;
        cancellation.begin_side_effect(boundary)?;
        ensure_dispatch_deadline(cancellation, boundary, deadline)?;
        authorize()?;
        ensure_dispatch_deadline(cancellation, boundary, deadline)?;
        Ok(post_focus_record(
            self.symbols,
            process,
            window_id,
            operation,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn set_target_main_window_before(
        &self,
        phase: FocusLeasePhase,
        require_requested_raw: bool,
        expected_user_frontmost: bool,
        window_id: u32,
        cancellation: &CommandCancellation,
        boundary: &str,
        deadline: Instant,
    ) -> Result<bool, ComputerError> {
        let exact_window = self.main_window_for_id(window_id).ok_or_else(|| {
            background_unavailable("The exact macOS target selector is unavailable")
        })?;
        let authorize = || {
            self.prove_phase_before(
                phase,
                require_requested_raw,
                InvariantStage::FocusPreparation,
                deadline,
            )?;
            if self.user_owner_matches_before(expected_user_frontmost, deadline) {
                Ok(())
            } else {
                Err(background_unavailable(
                    "The exact macOS user focus owner changed during window selection",
                ))
            }
        };
        authorize()?;
        ensure_dispatch_deadline(cancellation, boundary, deadline)?;
        cancellation.begin_side_effect(boundary)?;
        ensure_dispatch_deadline(cancellation, boundary, deadline)?;
        authorize()?;
        ensure_dispatch_deadline(cancellation, boundary, deadline)?;
        Ok(exact_window.set_main(true))
    }

    fn restore(self) -> Result<(), ComputerError> {
        let stage = InvariantStage::FocusRestore;
        let deadline = Instant::now() + FOCUS_RESTORE_OPERATION_BUDGET;
        let target_deadline = deadline
            .checked_sub(FOCUS_USER_RECOVERY_RESERVE)
            .unwrap_or(deadline);
        let user_authorization_deadline = deadline
            .checked_sub(FOCUS_USER_AUTHORIZATION_RESERVE)
            .unwrap_or(deadline);
        let user_poll_limit = deadline
            .checked_sub(FOCUS_USER_RETRY_RESERVE)
            .unwrap_or(deadline);
        if self
            .prove_phase_before(
                FocusLeasePhase::ReleasedWithTargetRequested,
                true,
                stage,
                target_deadline,
            )
            .is_err()
        {
            let _ = self.restore_released_user_without_target_proof_before(stage, deadline);
            return Err(self.restore_violation(stage));
        }
        if !self.target_restore_destination_is_restorable_before(target_deadline)
            || !self.restore_target_focus_before(stage, target_deadline)
        {
            let _ = self.restore_released_user_without_target_proof_before(stage, deadline);
            return Err(self.restore_violation(stage));
        }
        if !self.restore_user_focus_before(stage, target_deadline, user_authorization_deadline) {
            let _ = self.restore_released_user_without_target_proof_before(stage, deadline);
            return Err(self.restore_violation(stage));
        }
        let user_restored_deadline = std::cmp::min(
            user_poll_limit,
            Instant::now() + FOCUS_USER_RESTORE_POLL_BUDGET,
        );
        if !self.await_user_owner_before(true, user_restored_deadline) {
            let _ = self.restore_released_user_without_target_proof_before(stage, deadline);
            return Err(self.restore_violation(stage));
        }
        self.await_phase_before(
            FocusLeasePhase::Restored,
            false,
            stage,
            None,
            deadline,
            Duration::ZERO,
        )
    }

    fn restore_previous_after_failed_activation(
        &self,
        target_selection_may_be_changed: bool,
        target_may_be_prepared: bool,
    ) -> Result<(), ComputerError> {
        let stage = InvariantStage::FocusRecovery;
        let deadline = Instant::now() + FOCUS_RESTORE_OPERATION_BUDGET;
        let target_deadline = deadline
            .checked_sub(FOCUS_USER_RECOVERY_RESERVE)
            .unwrap_or(deadline);
        let user_authorization_deadline = deadline
            .checked_sub(FOCUS_USER_AUTHORIZATION_RESERVE)
            .unwrap_or(deadline);
        let user_poll_limit = deadline
            .checked_sub(FOCUS_USER_RETRY_RESERVE)
            .unwrap_or(deadline);
        let classify_deadline = std::cmp::min(
            target_deadline,
            Instant::now() + FOCUS_RECOVERY_CLASSIFY_BUDGET,
        );
        let facts = match self.focus_facts_before(classify_deadline, stage) {
            Ok(facts) => facts,
            Err(_) => {
                let _ = self.restore_released_user_without_target_proof_before(stage, deadline);
                return Err(self.restore_violation(stage));
            }
        };
        let phase = facts.classify_recovery(
            self.target_previous_window_id,
            self.target_window_id,
            target_selection_may_be_changed,
            target_may_be_prepared,
        );
        if phase == FocusLeasePhase::Restored
            && facts.target_process_current
            && facts.user_raw_restorable
            && facts.target_prior_raw_restorable
        {
            return Ok(());
        }
        if phase == FocusLeasePhase::RestoredWithInactiveTargetRequested {
            if facts.target_process_current
                && facts.user_raw_restorable
                && facts.target_prior_raw_restorable
                && facts.target_requested_raw_restorable
                && self.restore_selected_target_while_user_front_before(stage, target_deadline)
            {
                return Ok(());
            }
            return Err(self.restore_violation(stage));
        }
        let target_needs_cleanup = matches!(
            phase,
            FocusLeasePhase::ReleasedWithTargetRequested
                | FocusLeasePhase::ReleasedWithInactiveTargetRequested
                | FocusLeasePhase::ReleasedWithActiveTargetPrior
        );
        let requested_destination_needed = matches!(
            phase,
            FocusLeasePhase::ReleasedWithTargetRequested
                | FocusLeasePhase::ReleasedWithInactiveTargetRequested
                | FocusLeasePhase::RestoredWithInactiveTargetRequested
        );
        if phase == FocusLeasePhase::Unknown
            || !facts.target_process_current
            || !facts.user_raw_restorable
            || !facts.target_prior_raw_restorable
            || (requested_destination_needed && !facts.target_requested_raw_restorable)
            || (target_needs_cleanup
                && !self.restore_target_focus_from_phase_before(phase, stage, target_deadline))
        {
            let _ = self.restore_released_user_without_target_proof_before(stage, deadline);
            return Err(self.restore_violation(stage));
        }
        if !self.restore_user_focus_before(stage, target_deadline, user_authorization_deadline) {
            let _ = self.restore_released_user_without_target_proof_before(stage, deadline);
            return Err(self.restore_violation(stage));
        }
        let user_restored_deadline = std::cmp::min(
            user_poll_limit,
            Instant::now() + FOCUS_USER_RESTORE_POLL_BUDGET,
        );
        if !self.await_user_owner_before(true, user_restored_deadline) {
            let _ = self.restore_released_user_without_target_proof_before(stage, deadline);
            return Err(self.restore_violation(stage));
        }
        self.await_phase_before(
            FocusLeasePhase::Restored,
            false,
            stage,
            None,
            deadline,
            Duration::ZERO,
        )
    }

    fn prove_phase_before(
        &self,
        phase: FocusLeasePhase,
        require_requested_raw: bool,
        stage: InvariantStage,
        deadline: Instant,
    ) -> Result<(), ComputerError> {
        let facts = self.focus_facts_before(deadline, stage)?;
        let matches = facts.target_process_current
            && facts.user_raw_restorable
            && facts.target_prior_raw_restorable
            && (!require_requested_raw || facts.target_requested_raw_restorable)
            && facts.matches_phase(phase, self.target_previous_window_id, self.target_window_id)
            && Instant::now() < deadline;
        if matches {
            Ok(())
        } else {
            Err(self.restore_violation(stage))
        }
    }

    fn await_phase_before(
        &self,
        phase: FocusLeasePhase,
        require_requested_raw: bool,
        stage: InvariantStage,
        cancellation: Option<&CommandCancellation>,
        deadline: Instant,
        minimum_settle: Duration,
    ) -> Result<(), ComputerError> {
        let started = Instant::now();
        let not_before = started + minimum_settle;
        loop {
            if let Some(cancellation) = cancellation {
                cancellation.check("exact-window background focus")?;
            }
            if Instant::now() >= not_before
                && self
                    .prove_phase_before(phase, require_requested_raw, stage, deadline)
                    .is_ok()
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(self.restore_violation(stage));
            }
            thread::sleep(FOCUS_RESTORE_POLL_STEP);
        }
    }

    /// Re-proves the user's exact foreground ownership immediately before the
    /// final exact-target receiver proof and native event post.
    fn prove_dispatch_owner_before(&self, deadline: Instant) -> Result<(), ComputerError> {
        self.prove_phase_before(
            FocusLeasePhase::ReleasedWithTargetRequested,
            true,
            InvariantStage::FocusPreparation,
            deadline,
        )
        .map_err(|_| background_unavailable("The exact macOS input owner changed before dispatch"))
    }

    fn focus_facts_before(
        &self,
        deadline: Instant,
        stage: InvariantStage,
    ) -> Result<FocusLeaseFacts, ComputerError> {
        let inventory = raw_keyboard_window_inventory_before(deadline)?;
        let user_raw_restorable =
            raw_focus_window_restorable(self.previous_pid, self.previous_window_id, &inventory);
        let target_prior_raw_restorable = raw_focus_window_restorable(
            self.target_pid,
            self.target_previous_window_id,
            &inventory,
        );
        let target_requested_raw_restorable =
            raw_focus_window_restorable(self.target_pid, self.target_window_id, &inventory);
        let target_process_current = self.target_process_is_current();
        let user_before = self.user_focus_state_before(deadline, stage)?;
        let target_before =
            ax_macos::target_application_focus_state_before(self.target_pid, deadline)?;
        let user_after = self.user_focus_state_before(deadline, stage)?;
        let target_after =
            ax_macos::target_application_focus_state_before(self.target_pid, deadline)?;
        let front_still_matches = front_process_identity(self.symbols)
            .is_ok_and(|identity| identity == (self.previous_psn, self.previous_pid));
        let user_stable = user_before == user_after;
        let target_stable = target_before == target_after;
        if !user_stable || !target_stable || !front_still_matches || Instant::now() >= deadline {
            return Err(self.restore_violation(stage));
        }
        Ok(FocusLeaseFacts {
            user_frontmost: user_after.frontmost,
            target_frontmost: target_after.frontmost,
            target_window_id: target_after.window_id,
            target_main_window_id: target_after.main_window_id,
            target_process_current,
            user_raw_restorable,
            target_prior_raw_restorable,
            target_requested_raw_restorable,
        })
    }

    fn user_focus_state_before(
        &self,
        deadline: Instant,
        stage: InvariantStage,
    ) -> Result<ax_macos::ApplicationFocusState, ComputerError> {
        let front_before = front_process_identity(self.symbols).ok();
        let focus = ax_macos::application_focus_state_before(self.previous_pid, deadline)?;
        let front_after = front_process_identity(self.symbols).ok();
        if front_before == Some((self.previous_psn, self.previous_pid))
            && focus.window_id == self.previous_window_id
            && front_after == Some((self.previous_psn, self.previous_pid))
            && Instant::now() < deadline
        {
            Ok(focus)
        } else {
            Err(self.restore_violation(stage))
        }
    }

    fn user_owner_matches_before(&self, expected_frontmost: bool, deadline: Instant) -> bool {
        stable_front_focus_owner_matches(
            self.previous_psn,
            self.previous_pid,
            self.previous_window_id,
            expected_frontmost,
            front_process_identity(self.symbols).ok(),
            ax_macos::application_focus_state_before(self.previous_pid, deadline).ok(),
            front_process_identity(self.symbols).ok(),
        ) && Instant::now() < deadline
    }

    fn await_user_owner_before(&self, expected_frontmost: bool, deadline: Instant) -> bool {
        while Instant::now() < deadline {
            if self.user_owner_matches_before(expected_frontmost, deadline) {
                return true;
            }
            thread::sleep(FOCUS_RESTORE_POLL_STEP);
        }
        false
    }

    fn target_process_is_current(&self) -> bool {
        exact_window_process(
            self.symbols,
            self.target_pid,
            self.target_previous_window_id,
        )
        .is_ok_and(|current| current == self.target_psn)
    }

    fn restore_target_focus_before(&self, stage: InvariantStage, deadline: Instant) -> bool {
        self.restore_target_focus_from_phase_before(
            FocusLeasePhase::ReleasedWithTargetRequested,
            stage,
            deadline,
        )
    }

    fn restore_selected_target_while_user_front_before(
        &self,
        stage: InvariantStage,
        deadline: Instant,
    ) -> bool {
        if self.target_previous_window_id == self.target_window_id {
            return self
                .prove_phase_before(FocusLeasePhase::Restored, false, stage, deadline)
                .is_ok();
        }
        let _ = self.set_target_main_window_for_restore_before(
            FocusLeasePhase::RestoredWithInactiveTargetRequested,
            self.target_previous_window_id,
            true,
            stage,
            deadline,
        );
        self.await_phase_before(
            FocusLeasePhase::Restored,
            false,
            stage,
            None,
            deadline,
            FOCUS_PREPARATION_MIN_SETTLE,
        )
        .is_ok()
    }

    fn restore_target_focus_from_phase_before(
        &self,
        mut phase: FocusLeasePhase,
        stage: InvariantStage,
        deadline: Instant,
    ) -> bool {
        if phase == FocusLeasePhase::ReleasedWithTargetRequested {
            if !self.post_target_focus_record_before(
                self.target_window_id,
                FocusOperation::Defocus,
                self.target_window_id,
                true,
                deadline,
            ) || self
                .await_phase_before(
                    FocusLeasePhase::ReleasedWithInactiveTargetRequested,
                    true,
                    stage,
                    None,
                    deadline,
                    FOCUS_PREPARATION_MIN_SETTLE,
                )
                .is_err()
            {
                return false;
            }
            phase = FocusLeasePhase::ReleasedWithInactiveTargetRequested;
        }
        if phase == FocusLeasePhase::ReleasedWithInactiveTargetRequested {
            if self.target_previous_window_id == self.target_window_id {
                phase = FocusLeasePhase::ReleasedWithTargetPrior;
            } else {
                // Prime AppKit's exact key-window transfer with a balanced
                // process-focus record, then commit that exact destination
                // with the target-only make-key pair. The focus record can
                // transiently report either the old or new exact receiver, so
                // only those two observed active states are accepted.
                if !self.post_target_focus_record_before(
                    self.target_previous_window_id,
                    FocusOperation::Focus,
                    self.target_window_id,
                    false,
                    deadline,
                ) {
                    return false;
                }
                let Some(active_phase) = self.await_known_active_target_before(stage, deadline)
                else {
                    return false;
                };
                if active_phase == FocusLeasePhase::ReleasedWithTargetRequested {
                    let _ = self.post_target_make_key_window_before(
                        self.target_previous_window_id,
                        stage,
                        deadline,
                    );
                    if self
                        .await_phase_before(
                            FocusLeasePhase::ReleasedWithActiveTargetPrior,
                            false,
                            stage,
                            None,
                            deadline,
                            FOCUS_PREPARATION_MIN_SETTLE,
                        )
                        .is_err()
                    {
                        return false;
                    }
                }
                phase = FocusLeasePhase::ReleasedWithActiveTargetPrior;
            }
        }
        if phase == FocusLeasePhase::ReleasedWithActiveTargetPrior {
            if !self.post_target_focus_record_before(
                self.target_previous_window_id,
                FocusOperation::Defocus,
                self.target_previous_window_id,
                true,
                deadline,
            ) {
                return false;
            }
            phase = FocusLeasePhase::ReleasedWithTargetPrior;
        }
        phase == FocusLeasePhase::ReleasedWithTargetPrior
            && self
                .await_phase_before(
                    FocusLeasePhase::ReleasedWithTargetPrior,
                    false,
                    stage,
                    None,
                    deadline,
                    FOCUS_PREPARATION_MIN_SETTLE,
                )
                .is_ok()
    }

    fn set_target_main_window_for_restore_before(
        &self,
        phase: FocusLeasePhase,
        window_id: u32,
        expected_user_frontmost: bool,
        stage: InvariantStage,
        deadline: Instant,
    ) -> bool {
        let Some(exact_window) = self.main_window_for_id(window_id) else {
            return false;
        };
        if !self.target_process_is_current()
            || !self.target_window_is_restorable_before(window_id, deadline)
            || self
                .prove_phase_before(phase, true, stage, deadline)
                .is_err()
            || !self.user_owner_matches_before(expected_user_frontmost, deadline)
            || Instant::now() >= deadline
        {
            return false;
        }
        exact_window.set_main(true)
    }

    fn main_window_for_id(&self, window_id: u32) -> Option<&ax_macos::ExactTargetMainWindow> {
        if window_id == self.target_window_id {
            Some(&self.target_main_window)
        } else if window_id == self.target_previous_window_id {
            self.target_previous_main_window.as_ref()
        } else {
            None
        }
    }

    fn post_target_make_key_window_before(
        &self,
        window_id: u32,
        stage: InvariantStage,
        deadline: Instant,
    ) -> bool {
        let destination_restorable = self.target_window_is_restorable_before(window_id, deadline);
        if !destination_restorable
            || self
                .prove_phase_before(
                    FocusLeasePhase::ReleasedWithTargetRequested,
                    true,
                    stage,
                    deadline,
                )
                .is_err()
            || !self.user_owner_matches_before(false, deadline)
            || Instant::now() >= deadline
        {
            return false;
        }
        post_make_key_window_record(self.symbols, &self.target_psn, window_id)
    }

    fn post_target_make_key_window_for_activation_before(
        &self,
        window_id: u32,
        cancellation: &CommandCancellation,
        stage: InvariantStage,
        deadline: Instant,
    ) -> Result<bool, ComputerError> {
        let authorize = || {
            if self.target_window_is_restorable_before(window_id, deadline)
                && self
                    .prove_phase_before(
                        FocusLeasePhase::ReleasedWithTargetRequested,
                        true,
                        stage,
                        deadline,
                    )
                    .is_ok()
                && self.user_owner_matches_before(false, deadline)
                && Instant::now() < deadline
            {
                Ok(())
            } else {
                Err(background_unavailable(
                    "The exact macOS target receiver changed during preparation",
                ))
            }
        };
        authorize()?;
        ensure_dispatch_deadline(cancellation, "target exact receiver preparation", deadline)?;
        cancellation.begin_side_effect("target exact receiver preparation")?;
        ensure_dispatch_deadline(cancellation, "target exact receiver preparation", deadline)?;
        authorize()?;
        Ok(post_make_key_window_record(
            self.symbols,
            &self.target_psn,
            window_id,
        ))
    }

    fn await_known_active_target_before(
        &self,
        stage: InvariantStage,
        deadline: Instant,
    ) -> Option<FocusLeasePhase> {
        let not_before = Instant::now() + FOCUS_PREPARATION_MIN_SETTLE;
        while Instant::now() < deadline {
            if Instant::now() >= not_before
                && let Ok(facts) = self.focus_facts_before(deadline, stage)
            {
                let phase = if facts.target_process_current
                    && facts.user_raw_restorable
                    && facts.target_prior_raw_restorable
                    && facts.target_requested_raw_restorable
                    && facts.matches_phase(
                        FocusLeasePhase::ReleasedWithTargetRequested,
                        self.target_previous_window_id,
                        self.target_window_id,
                    ) {
                    Some(FocusLeasePhase::ReleasedWithTargetRequested)
                } else if facts.target_process_current
                    && facts.user_raw_restorable
                    && facts.target_prior_raw_restorable
                    && facts.matches_phase(
                        FocusLeasePhase::ReleasedWithActiveTargetPrior,
                        self.target_previous_window_id,
                        self.target_window_id,
                    )
                {
                    Some(FocusLeasePhase::ReleasedWithActiveTargetPrior)
                } else {
                    None
                };
                if phase.is_some() && Instant::now() < deadline {
                    return phase;
                }
            }
            thread::sleep(FOCUS_RESTORE_POLL_STEP);
        }
        None
    }

    fn post_target_focus_record_before(
        &self,
        window_id: u32,
        operation: FocusOperation,
        expected_target_window_id: u32,
        expected_target_frontmost: bool,
        deadline: Instant,
    ) -> bool {
        let target_process_current = self.target_process_is_current();
        let destination_restorable = self.target_window_is_restorable_before(window_id, deadline);
        let target_matches =
            ax_macos::target_application_focus_state_before(self.target_pid, deadline).is_ok_and(
                |focus| {
                    focus.frontmost == expected_target_frontmost
                        && focus.window_id == expected_target_window_id
                        && focus.main_window_id == expected_target_window_id
                },
            );
        if !target_process_current
            || !destination_restorable
            || !target_matches
            || !self.user_owner_matches_before(false, deadline)
            || Instant::now() >= deadline
        {
            return false;
        }
        post_focus_record(self.symbols, &self.target_psn, window_id, operation)
    }

    fn restore_user_focus_before(
        &self,
        stage: InvariantStage,
        phase_deadline: Instant,
        user_deadline: Instant,
    ) -> bool {
        if self
            .prove_phase_before(
                FocusLeasePhase::ReleasedWithTargetPrior,
                false,
                stage,
                phase_deadline,
            )
            .is_err()
            || !self.front_restore_destination_is_restorable_before(user_deadline)
            || !self.user_owner_matches_before(false, user_deadline)
            || Instant::now() >= user_deadline
        {
            return false;
        }
        post_focus_record(
            self.symbols,
            &self.previous_psn,
            self.previous_window_id,
            FocusOperation::Focus,
        )
    }

    fn restore_released_user_without_target_proof_before(
        &self,
        _stage: InvariantStage,
        deadline: Instant,
    ) -> bool {
        if !self.front_restore_destination_is_restorable_before(deadline)
            || !self.user_owner_matches_before(false, deadline)
            || Instant::now() >= deadline
            || !post_focus_record(
                self.symbols,
                &self.previous_psn,
                self.previous_window_id,
                FocusOperation::Focus,
            )
        {
            return false;
        }
        self.await_user_owner_before(true, deadline)
    }

    fn front_restore_destination_is_restorable_before(&self, deadline: Instant) -> bool {
        raw_keyboard_window_inventory_before(deadline).is_ok_and(|inventory| {
            raw_focus_window_restorable(self.previous_pid, self.previous_window_id, &inventory)
                && Instant::now() < deadline
        })
    }

    fn target_restore_destination_is_restorable_before(&self, deadline: Instant) -> bool {
        self.target_window_is_restorable_before(self.target_previous_window_id, deadline)
    }

    fn target_window_is_restorable_before(&self, window_id: u32, deadline: Instant) -> bool {
        raw_keyboard_window_inventory_before(deadline).is_ok_and(|inventory| {
            raw_focus_window_restorable(self.target_pid, window_id, &inventory)
                && Instant::now() < deadline
        })
    }

    fn restore_violation(&self, stage: InvariantStage) -> ComputerError {
        background_contract_violation(stage, [InvariantFailure::UserFocus])
    }
}

fn stable_front_focus_owner_matches(
    expected_psn: [u8; 8],
    expected_pid: u32,
    expected_window_id: u32,
    expected_frontmost: bool,
    front_before: Option<([u8; 8], u32)>,
    focus: Option<ax_macos::ApplicationFocusState>,
    front_after: Option<([u8; 8], u32)>,
) -> bool {
    front_before == Some((expected_psn, expected_pid))
        && focus.is_some_and(|focus| {
            focus.frontmost == expected_frontmost
                && focus.window_id == expected_window_id
                && focus.main_window_id == expected_window_id
        })
        && front_after == Some((expected_psn, expected_pid))
}

fn front_process_identity(symbols: &Symbols) -> Result<([u8; 8], u32), ComputerError> {
    let mut process = [0u8; 8];
    if unsafe { (symbols.get_front_process)(process.as_mut_ptr() as *mut c_void) } != 0 {
        return Err(input_error("Could not read the front process"));
    }
    let mut pid_raw: pid_t = 0;
    if unsafe { (symbols.get_process_pid)(process.as_ptr().cast(), &mut pid_raw) } != 0
        || pid_raw <= 0
    {
        return Err(input_error("Could not resolve the front process"));
    }
    Ok((process, pid_raw as u32))
}

fn exact_window_process(
    symbols: &Symbols,
    expected_pid: u32,
    window_id: u32,
) -> Result<[u8; 8], ComputerError> {
    let mut owner_connection = 0u32;
    if unsafe {
        (symbols.get_window_owner)((symbols.connection_id)(), window_id, &mut owner_connection)
    } != 0
        || owner_connection == 0
    {
        return Err(background_unavailable(
            "macOS could not resolve the exact target window owner",
        ));
    }
    let mut process = [0u8; 8];
    if unsafe {
        (symbols.get_connection_psn)(owner_connection, process.as_mut_ptr() as *mut c_void)
    } != 0
    {
        return Err(background_unavailable(
            "macOS could not resolve the exact target window process",
        ));
    }
    let mut pid_raw: pid_t = 0;
    if unsafe { (symbols.get_process_pid)(process.as_ptr().cast(), &mut pid_raw) } != 0
        || pid_raw <= 0
        || pid_raw as u32 != expected_pid
    {
        return Err(background_unavailable(
            "The exact macOS target window owner changed",
        ));
    }
    Ok(process)
}

fn private_event_source() -> Result<CGEventSource, ComputerError> {
    let source = CGEventSource::new(CGEventSourceStateID::Private)
        .map_err(|_| input_error("CGEventSourceCreate failed"))?;
    let mut value = [0_u8; 8];
    value.copy_from_slice(&Uuid::new_v4().as_bytes()[..8]);
    let event_source_tag = i64::from_le_bytes(value) | 1;
    unsafe { CGEventSourceSetUserData(source.as_ptr().cast(), event_source_tag) };
    Ok(source)
}

fn hardware_cursor_position() -> Result<CGPoint, ComputerError> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| input_error("Could not create the read-only HID cursor source"))?;
    CGEvent::new(source)
        .map_err(|_| input_error("Could not read the hardware cursor"))
        .map(|event| event.location())
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HidSystemPointerCounters {
    left_down: u32,
    left_up: u32,
    right_down: u32,
    right_up: u32,
    mouse_moved: u32,
    left_dragged: u32,
    right_dragged: u32,
    scroll_wheel: u32,
    other_dragged: u32,
    other_down: u32,
    other_up: u32,
    tablet_pointer: u32,
    tablet_proximity: u32,
}

impl HidSystemPointerCounters {
    fn capture() -> Self {
        let counter = |event_type| unsafe {
            CGEventSourceCounterForEventType(CGEventSourceStateID::HIDSystemState, event_type)
        };
        Self {
            left_down: counter(CGEventType::LeftMouseDown),
            left_up: counter(CGEventType::LeftMouseUp),
            right_down: counter(CGEventType::RightMouseDown),
            right_up: counter(CGEventType::RightMouseUp),
            mouse_moved: counter(CGEventType::MouseMoved),
            left_dragged: counter(CGEventType::LeftMouseDragged),
            right_dragged: counter(CGEventType::RightMouseDragged),
            scroll_wheel: counter(CGEventType::ScrollWheel),
            other_dragged: counter(CGEventType::OtherMouseDragged),
            other_down: counter(CGEventType::OtherMouseDown),
            other_up: counter(CGEventType::OtherMouseUp),
            tablet_pointer: counter(CGEventType::TabletPointer),
            tablet_proximity: counter(CGEventType::TabletProximity),
        }
    }

    fn progress_to(self, after: Self) -> HidCounterProgress {
        let mut progress = HidCounterProgress::Stable;
        for (before, after) in [
            (self.left_down, after.left_down),
            (self.left_up, after.left_up),
            (self.right_down, after.right_down),
            (self.right_up, after.right_up),
            (self.mouse_moved, after.mouse_moved),
            (self.left_dragged, after.left_dragged),
            (self.right_dragged, after.right_dragged),
            (self.scroll_wheel, after.scroll_wheel),
            (self.other_dragged, after.other_dragged),
            (self.other_down, after.other_down),
            (self.other_up, after.other_up),
            (self.tablet_pointer, after.tablet_pointer),
            (self.tablet_proximity, after.tablet_proximity),
        ] {
            let advance = after.wrapping_sub(before);
            if advance == 0 {
                continue;
            }
            if advance > MAX_HID_POINTER_COUNTER_ADVANCE {
                return HidCounterProgress::Unknown;
            }
            progress = HidCounterProgress::Advanced;
        }
        progress
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HidCounterProgress {
    Stable,
    Advanced,
    Unknown,
}

#[derive(Clone, Copy)]
struct CursorStamp {
    position: CGPoint,
    counters: HidSystemPointerCounters,
    boundary_activity_observed: bool,
}

impl CursorStamp {
    fn capture() -> Result<Self, ComputerError> {
        let deadline = Instant::now() + CURSOR_STAMP_BUDGET;
        let mut boundary_activity_observed = false;
        loop {
            let counters_before = HidSystemPointerCounters::capture();
            let position = hardware_cursor_position()?;
            let counters_after = HidSystemPointerCounters::capture();
            match counters_before.progress_to(counters_after) {
                HidCounterProgress::Stable => {
                    return Ok(Self {
                        position,
                        counters: counters_after,
                        boundary_activity_observed,
                    });
                }
                HidCounterProgress::Advanced => {
                    boundary_activity_observed = true;
                    if Instant::now() >= deadline {
                        return Ok(Self {
                            position,
                            counters: counters_after,
                            boundary_activity_observed: true,
                        });
                    }
                }
                HidCounterProgress::Unknown => {
                    return Err(input_error(
                        "The macOS HID-system cursor counter epoch became invalid",
                    ));
                }
            }
            thread::sleep(CURSOR_STAMP_POLL);
        }
    }

    fn classify(self, after: Self, input_delivery: InputDeliveryProvenance) -> CursorAttribution {
        let position_unchanged = (self.position.x - after.position.x).abs() < 0.01
            && (self.position.y - after.position.y).abs() < 0.01;
        let counter_progress = self.counters.progress_to(after.counters);
        let monitor_healthy = counter_progress != HidCounterProgress::Unknown;
        let hid_system_pointer_activity = self.boundary_activity_observed
            || after.boundary_activity_observed
            || counter_progress == HidCounterProgress::Advanced;
        let boundary_corroborated =
            monitor_healthy && (position_unchanged || hid_system_pointer_activity);
        let shared_pointer_boundary_state = if boundary_corroborated {
            SharedPointerBoundaryState::Corroborated
        } else {
            SharedPointerBoundaryState::Unknown
        };
        let helper_global_pointer_preservation = if input_delivery.global_hid_input_used
            || input_delivery.hardware_cursor_mutation_requested
        {
            HelperGlobalPointerPreservation::Violated
        } else if input_delivery.is_target_bound() && boundary_corroborated {
            HelperGlobalPointerPreservation::Confirmed
        } else {
            HelperGlobalPointerPreservation::Unknown
        };
        let shared_pointer_activity_state =
            if !monitor_healthy || (!position_unchanged && !hid_system_pointer_activity) {
                SharedPointerActivityState::Unknown
            } else if hid_system_pointer_activity {
                SharedPointerActivityState::Contaminated
            } else {
                SharedPointerActivityState::Quiet
            };
        CursorAttribution {
            position_unchanged,
            hid_system_pointer_activity,
            monitor_healthy,
            boundary_corroborated,
            shared_pointer_boundary_state,
            preserved_by_helper: helper_global_pointer_preservation
                == HelperGlobalPointerPreservation::Confirmed,
            helper_global_pointer_preservation,
            shared_pointer_activity_state,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CursorAttribution {
    position_unchanged: bool,
    hid_system_pointer_activity: bool,
    monitor_healthy: bool,
    boundary_corroborated: bool,
    shared_pointer_boundary_state: SharedPointerBoundaryState,
    preserved_by_helper: bool,
    helper_global_pointer_preservation: HelperGlobalPointerPreservation,
    shared_pointer_activity_state: SharedPointerActivityState,
}

#[derive(Clone, Copy)]
struct DesktopSnapshot {
    front_process: [u8; 8],
    front_pid: u32,
    front_window_id: u32,
    cursor: CursorStamp,
    active_space: u64,
}

impl DesktopSnapshot {
    fn capture() -> Result<Self, ComputerError> {
        let symbols = symbols().ok_or_else(|| input_error("SkyLight symbols are unavailable"))?;
        let (front_process, front_pid) = front_process_identity(symbols)?;
        let focus_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;
        let front_focus_before =
            ax_macos::application_focus_state_before(front_pid, focus_deadline)?;
        let cursor = CursorStamp::capture()?;
        let active_space = unsafe { (symbols.get_active_space)((symbols.connection_id)()) };
        if active_space == 0 {
            return Err(input_error("Could not prove the active macOS Space"));
        }
        let front_focus_after =
            ax_macos::application_focus_state_before(front_pid, focus_deadline)?;
        let front_after = front_process_identity(symbols)?;
        if front_after != (front_process, front_pid)
            || front_focus_before != front_focus_after
            || !front_focus_after.frontmost
            || Instant::now() >= focus_deadline
        {
            return Err(input_error(
                "Could not stabilize the exact macOS foreground window",
            ));
        }
        let front_window_id = front_focus_after.window_id;
        Ok(Self {
            front_process,
            front_pid,
            front_window_id,
            cursor,
            active_space,
        })
    }

    fn compare(&self, after: &Self, input_delivery: InputDeliveryProvenance) -> InvariantReport {
        let cursor = self.cursor.classify(after.cursor, input_delivery);
        InvariantReport {
            foreground_unchanged: self.front_process == after.front_process,
            user_focus_unchanged: self.front_pid == after.front_pid
                && self.front_window_id == after.front_window_id,
            cursor_position_unchanged: cursor.position_unchanged,
            shared_pointer_activity_observed: cursor.hid_system_pointer_activity,
            hid_system_pointer_activity_observed: cursor.hid_system_pointer_activity,
            raw_input_pointer_activity_observed: false,
            injected_pointer_activity_observed: false,
            pointer_activity_monitor_healthy: cursor.monitor_healthy,
            shared_pointer_boundary_corroborated: cursor.boundary_corroborated,
            shared_pointer_boundary_state: cursor.shared_pointer_boundary_state,
            hardware_cursor_preserved_by_helper: cursor.preserved_by_helper,
            helper_global_pointer_preservation: cursor.helper_global_pointer_preservation,
            shared_pointer_activity_state: cursor.shared_pointer_activity_state,
            space_unchanged: self.active_space == after.active_space,
            input_delivery,
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
                get_process_pid: load(b"GetProcessPID\0")?,
                get_window_owner: load(b"SLSGetWindowOwner\0")?,
                get_connection_psn: load(b"SLSGetConnectionPSN\0")?,
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

fn background_unavailable(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_BACKGROUND_UNAVAILABLE", message)
}

#[cfg(test)]
mod invariant_tests {
    use super::*;

    #[test]
    fn text_deadline_and_cancellation_fail_at_cooperative_boundaries() {
        let cancellation = CommandCancellation::new();
        let expired = Instant::now() - Duration::from_millis(1);
        let before = text_dispatch_checkpoint(&cancellation, expired).unwrap_err();
        assert_eq!(before.code, "COMPUTER_BACKGROUND_UNAVAILABLE");

        cancellation.begin_side_effect("fixture text").unwrap();
        let after = text_dispatch_checkpoint(&cancellation, expired).unwrap_err();
        assert_eq!(after.code, "COMPUTER_OUTCOME_UNKNOWN");

        cancellation.cancel();
        let canceled =
            text_dispatch_checkpoint(&cancellation, Instant::now() + Duration::from_secs(1))
                .unwrap_err();
        assert_eq!(canceled.code, "COMPUTER_OUTCOME_UNKNOWN");
    }

    #[test]
    fn native_post_deadline_is_rechecked_after_dispatch_accounting() {
        let expired = Instant::now() - Duration::from_millis(1);
        let before = CommandCancellation::new();
        let retry_safe = ensure_dispatch_deadline(&before, "fixture post", expired).unwrap_err();
        assert_eq!(retry_safe.code, "COMPUTER_BACKGROUND_UNAVAILABLE");
        assert!(!before.was_dispatched());

        let after = CommandCancellation::new();
        after.begin_side_effect("fixture post").unwrap();
        let unknown = ensure_dispatch_deadline(&after, "fixture post", expired).unwrap_err();
        assert_eq!(unknown.code, "COMPUTER_OUTCOME_UNKNOWN");

        let canceled = CommandCancellation::new();
        canceled.begin_side_effect("fixture post").unwrap();
        canceled.cancel();
        let unknown = ensure_dispatch_deadline(
            &canceled,
            "fixture post",
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(unknown.code, "COMPUTER_OUTCOME_UNKNOWN");
    }

    #[test]
    fn text_receiver_loss_is_retryable_only_before_the_first_scalar() {
        let unavailable = || {
            Err(ComputerError::new(
                "COMPUTER_BACKGROUND_UNAVAILABLE",
                "fixture receiver unavailable",
            ))
        };
        let before = map_text_receiver_failure(unavailable(), 0).unwrap_err();
        assert_eq!(before.code, "COMPUTER_BACKGROUND_UNAVAILABLE");
        let after = map_text_receiver_failure(unavailable(), 1).unwrap_err();
        assert_eq!(after.code, "COMPUTER_OUTCOME_UNKNOWN");
        assert!(after.message.contains("do not automatically retry"));
    }

    fn keyboard_target() -> WindowDescriptor {
        WindowDescriptor {
            id: "62090".to_owned(),
            pid: 4242,
            app_name: "Fixture".to_owned(),
            title: "Target".to_owned(),
            x: 180,
            y: 768,
            width: 820,
            height: 552,
            minimized: false,
            focused: false,
        }
    }

    fn raw_keyboard_target() -> RawKeyboardWindow {
        RawKeyboardWindow {
            window_id: 62090,
            owner_pid: 4242,
            layer: 0,
            x: 180.0,
            y: 768.0,
            width: 820.0,
            height: 552.0,
            is_on_screen: Some(true),
            _sharing_state: Some(0),
        }
    }

    #[test]
    fn raw_keyboard_target_accepts_on_screen_unshareable_exact_window() {
        let target = keyboard_target();
        let window = raw_keyboard_target();
        assert!(raw_keyboard_target_matches(&target, 62090, &[window]));
    }

    #[test]
    fn raw_keyboard_target_rejects_offscreen_or_unknown_screen_state() {
        let target = keyboard_target();
        let mut offscreen = raw_keyboard_target();
        offscreen.is_on_screen = Some(false);
        assert!(!raw_keyboard_target_matches(&target, 62090, &[offscreen]));
        let mut unknown = raw_keyboard_target();
        unknown.is_on_screen = None;
        assert!(!raw_keyboard_target_matches(&target, 62090, &[unknown]));
    }

    #[test]
    fn raw_keyboard_target_rejects_owner_layer_geometry_and_duplicate_mismatches() {
        let target = keyboard_target();
        let mut wrong_owner = raw_keyboard_target();
        wrong_owner.owner_pid += 1;
        assert!(!raw_keyboard_target_matches(&target, 62090, &[wrong_owner]));

        let mut wrong_layer = raw_keyboard_target();
        wrong_layer.layer = 1;
        assert!(!raw_keyboard_target_matches(&target, 62090, &[wrong_layer]));

        let mut wrong_geometry = raw_keyboard_target();
        wrong_geometry.width -= 1.0;
        assert!(!raw_keyboard_target_matches(
            &target,
            62090,
            &[wrong_geometry]
        ));

        let exact = raw_keyboard_target();
        assert!(!raw_keyboard_target_matches(
            &target,
            62090,
            &[exact, exact]
        ));
    }

    #[test]
    fn focus_restore_destination_requires_one_on_screen_layer_zero_owner_match() {
        let exact = raw_keyboard_target();
        assert!(raw_focus_window_restorable(4242, 62090, &[exact]));

        let mut offscreen = exact;
        offscreen.is_on_screen = Some(false);
        assert!(!raw_focus_window_restorable(4242, 62090, &[offscreen]));
        let mut unknown = exact;
        unknown.is_on_screen = None;
        assert!(!raw_focus_window_restorable(4242, 62090, &[unknown]));
        let mut wrong_owner = exact;
        wrong_owner.owner_pid += 1;
        assert!(!raw_focus_window_restorable(4242, 62090, &[wrong_owner]));
        let mut wrong_layer = exact;
        wrong_layer.layer = 1;
        assert!(!raw_focus_window_restorable(4242, 62090, &[wrong_layer]));
        assert!(!raw_focus_window_restorable(4242, 62090, &[exact, exact]));
    }

    #[test]
    fn same_pid_route_never_changes_the_app_internal_key_window() {
        assert!(!focus_preparation_needed(4242, 4242));
        assert!(focus_preparation_needed(7, 4242));
        let psn = [9_u8; 8];
        let front = Some((psn, 4242));
        let exact = ax_macos::ApplicationFocusState {
            window_id: 62090,
            main_window_id: 62090,
            frontmost: true,
        };
        assert!(same_process_mutation_target_matches(
            psn, 4242, 62090, 4242, 62090, front, exact, front
        ));
        assert!(!same_process_mutation_target_matches(
            psn, 4242, 62090, 4242, 62100, front, exact, front
        ));
        assert!(!same_process_mutation_target_matches(
            psn, 4242, 62100, 4242, 62090, front, exact, front
        ));
        assert!(!same_process_mutation_target_matches(
            psn,
            4242,
            62090,
            7,
            62090,
            Some((psn, 7)),
            exact,
            Some((psn, 7)),
        ));
        assert!(!same_process_mutation_target_matches(
            psn,
            4242,
            62090,
            4242,
            62090,
            front,
            ax_macos::ApplicationFocusState {
                frontmost: false,
                ..exact
            },
            front,
        ));
        assert!(!same_process_mutation_target_matches(
            psn,
            4242,
            62090,
            4242,
            62090,
            front,
            exact,
            Some(([8_u8; 8], 4242)),
        ));
    }

    #[test]
    fn front_focus_owner_requires_a_stable_psn_window_and_frontmost_sandwich() {
        let psn = [7_u8; 8];
        let exact = ax_macos::ApplicationFocusState {
            window_id: 62090,
            main_window_id: 62090,
            frontmost: true,
        };
        assert!(stable_front_focus_owner_matches(
            psn,
            42,
            62090,
            true,
            Some((psn, 42)),
            Some(exact),
            Some((psn, 42)),
        ));
        assert!(!stable_front_focus_owner_matches(
            psn,
            42,
            62090,
            true,
            Some((psn, 42)),
            Some(ax_macos::ApplicationFocusState {
                window_id: 62100,
                ..exact
            }),
            Some((psn, 42)),
        ));
        assert!(!stable_front_focus_owner_matches(
            psn,
            42,
            62090,
            true,
            Some((psn, 42)),
            Some(ax_macos::ApplicationFocusState {
                main_window_id: 62100,
                ..exact
            }),
            Some((psn, 42)),
        ));
        assert!(!stable_front_focus_owner_matches(
            psn,
            42,
            62090,
            true,
            Some((psn, 42)),
            Some(ax_macos::ApplicationFocusState {
                frontmost: false,
                ..exact
            }),
            Some((psn, 42)),
        ));
        assert!(!stable_front_focus_owner_matches(
            psn,
            42,
            62090,
            true,
            Some((psn, 42)),
            Some(exact),
            Some(([8_u8; 8], 42)),
        ));
        assert!(stable_front_focus_owner_matches(
            psn,
            42,
            62090,
            false,
            Some((psn, 42)),
            Some(ax_macos::ApplicationFocusState {
                frontmost: false,
                ..exact
            }),
            Some((psn, 42)),
        ));
    }

    #[test]
    fn focus_lease_facts_model_only_observable_private_focus_phases() {
        let facts = |user_frontmost, target_frontmost, target_window_id, target_main_window_id| {
            FocusLeaseFacts {
                user_frontmost,
                target_frontmost,
                target_window_id,
                target_main_window_id,
                target_process_current: true,
                user_raw_restorable: true,
                target_prior_raw_restorable: true,
                target_requested_raw_restorable: true,
            }
        };
        assert!(facts(true, false, 62100, 62100).matches_phase(
            FocusLeasePhase::Restored,
            62100,
            62090
        ));
        assert!(facts(true, false, 62090, 62090).matches_phase(
            FocusLeasePhase::RestoredWithInactiveTargetRequested,
            62100,
            62090
        ));
        assert!(facts(false, false, 62100, 62100).matches_phase(
            FocusLeasePhase::ReleasedWithTargetPrior,
            62100,
            62090
        ));
        assert!(facts(false, true, 62090, 62090).matches_phase(
            FocusLeasePhase::ReleasedWithTargetRequested,
            62100,
            62090
        ));
        assert!(facts(false, false, 62090, 62090).matches_phase(
            FocusLeasePhase::ReleasedWithInactiveTargetRequested,
            62100,
            62090
        ));
        assert!(facts(false, true, 62100, 62100).matches_phase(
            FocusLeasePhase::ReleasedWithActiveTargetPrior,
            62100,
            62090
        ));
        assert_eq!(
            facts(false, true, 62090, 62090).classify_recovery(62100, 62090, true, true),
            FocusLeasePhase::ReleasedWithTargetRequested
        );
        assert_eq!(
            facts(false, false, 62100, 62100).classify_recovery(62100, 62090, false, false),
            FocusLeasePhase::ReleasedWithTargetPrior
        );
        assert_eq!(
            facts(false, false, 62090, 62090).classify_recovery(62100, 62090, true, false),
            FocusLeasePhase::ReleasedWithInactiveTargetRequested
        );
        assert_eq!(
            facts(true, false, 62090, 62090).classify_recovery(62100, 62090, true, false),
            FocusLeasePhase::RestoredWithInactiveTargetRequested
        );
        assert_eq!(
            facts(false, true, 62090, 62090).classify_recovery(62090, 62090, true, true),
            FocusLeasePhase::ReleasedWithTargetRequested
        );
        assert_eq!(
            facts(false, true, 62090, 62090).classify_recovery(62090, 62090, false, false),
            FocusLeasePhase::Unknown
        );
        assert_eq!(
            facts(false, false, 62090, 62090).classify_recovery(62090, 62090, true, true),
            FocusLeasePhase::ReleasedWithTargetPrior
        );
        assert!(!facts(false, true, 62090, 62100).matches_phase(
            FocusLeasePhase::ReleasedWithTargetRequested,
            62100,
            62090
        ));
    }

    fn test_cursor_stamp(x: f64, y: f64, mouse_moved: u32) -> CursorStamp {
        CursorStamp {
            position: CGPoint::new(x, y),
            counters: HidSystemPointerCounters {
                left_down: 1,
                left_up: 2,
                right_down: 3,
                right_up: 4,
                mouse_moved,
                left_dragged: 11,
                right_dragged: 12,
                scroll_wheel: 15,
                other_dragged: 13,
                other_down: 5,
                other_up: 6,
                tablet_pointer: 14,
                tablet_proximity: 16,
            },
            boundary_activity_observed: false,
        }
    }

    fn test_target_delivery() -> InputDeliveryProvenance {
        let trace = MacTargetDispatchTrace {
            target_pid: 42,
            target_window_id: 62090,
            attempt_count: 1,
        };
        trace.provenance().unwrap()
    }

    #[test]
    fn target_dispatch_trace_requires_one_exact_record_and_refuses_overflow() {
        let target = keyboard_target();
        let mut empty = MacTargetDispatchTrace::new(&target).unwrap();
        let missing = empty.provenance().unwrap_err();
        assert_eq!(missing.code, "COMPUTER_BACKGROUND_CONTRACT_VIOLATION");

        let wrong_pid = empty
            .record(MacTargetDispatchRecord {
                target_pid: target.pid + 1,
                target_window_id: 62090,
            })
            .unwrap_err();
        assert_eq!(wrong_pid.code, "COMPUTER_BACKGROUND_CONTRACT_VIOLATION");

        empty
            .record(MacTargetDispatchRecord {
                target_pid: target.pid,
                target_window_id: 62090,
            })
            .unwrap();
        let provenance = empty.provenance().unwrap();
        assert_eq!(
            provenance.support_level,
            InputDeliverySupportLevel::PrivateUnsupported
        );
        assert!(provenance.dispatch_attempt_recorded);
        assert!(!provenance.os_acceptance_signal_available);
        assert!(!provenance.os_acceptance_observed);

        let mut exhausted = MacTargetDispatchTrace {
            target_pid: target.pid,
            target_window_id: 62090,
            attempt_count: u32::MAX,
        };
        let overflow = exhausted
            .record(MacTargetDispatchRecord {
                target_pid: target.pid,
                target_window_id: 62090,
            })
            .unwrap_err();
        assert_eq!(overflow.code, "COMPUTER_DISPATCH_EXHAUSTED");
    }

    #[test]
    fn cursor_attribution_accepts_only_stable_or_hid_explained_motion() {
        let delivery = test_target_delivery();

        let quiet = test_cursor_stamp(100.0, 200.0, 7)
            .classify(test_cursor_stamp(100.0, 200.0, 7), delivery);
        assert_eq!(
            quiet,
            CursorAttribution {
                position_unchanged: true,
                hid_system_pointer_activity: false,
                monitor_healthy: true,
                boundary_corroborated: true,
                shared_pointer_boundary_state: SharedPointerBoundaryState::Corroborated,
                preserved_by_helper: true,
                helper_global_pointer_preservation: HelperGlobalPointerPreservation::Confirmed,
                shared_pointer_activity_state: SharedPointerActivityState::Quiet,
            }
        );

        let explained = test_cursor_stamp(100.0, 200.0, 7)
            .classify(test_cursor_stamp(101.0, 200.0, 8), delivery);
        assert_eq!(
            explained,
            CursorAttribution {
                position_unchanged: false,
                hid_system_pointer_activity: true,
                monitor_healthy: true,
                boundary_corroborated: true,
                shared_pointer_boundary_state: SharedPointerBoundaryState::Corroborated,
                preserved_by_helper: true,
                helper_global_pointer_preservation: HelperGlobalPointerPreservation::Confirmed,
                shared_pointer_activity_state: SharedPointerActivityState::Contaminated,
            }
        );

        let unexplained = test_cursor_stamp(100.0, 200.0, 7)
            .classify(test_cursor_stamp(101.0, 200.0, 7), delivery);
        assert_eq!(
            unexplained,
            CursorAttribution {
                position_unchanged: false,
                hid_system_pointer_activity: false,
                monitor_healthy: true,
                boundary_corroborated: false,
                shared_pointer_boundary_state: SharedPointerBoundaryState::Unknown,
                preserved_by_helper: false,
                helper_global_pointer_preservation: HelperGlobalPointerPreservation::Unknown,
                shared_pointer_activity_state: SharedPointerActivityState::Unknown,
            }
        );
    }

    #[test]
    fn cursor_attribution_records_move_away_and_back_and_counter_wrap() {
        let delivery = test_target_delivery();
        let out_and_back = test_cursor_stamp(100.0, 200.0, 7)
            .classify(test_cursor_stamp(100.0, 200.0, 8), delivery);
        assert!(out_and_back.position_unchanged);
        assert!(out_and_back.hid_system_pointer_activity);
        assert!(out_and_back.monitor_healthy);
        assert!(out_and_back.boundary_corroborated);
        assert!(out_and_back.preserved_by_helper);

        let wrapped = test_cursor_stamp(100.0, 200.0, u32::MAX)
            .classify(test_cursor_stamp(101.0, 200.0, 0), delivery);
        assert!(wrapped.hid_system_pointer_activity);
        assert!(wrapped.monitor_healthy);
        assert!(wrapped.boundary_corroborated);
        assert!(wrapped.preserved_by_helper);

        let mut active_boundary = test_cursor_stamp(101.0, 200.0, 7);
        active_boundary.boundary_activity_observed = true;
        let continuously_active =
            test_cursor_stamp(100.0, 200.0, 7).classify(active_boundary, delivery);
        assert!(continuously_active.hid_system_pointer_activity);
        assert!(continuously_active.monitor_healthy);
        assert!(continuously_active.boundary_corroborated);
        assert!(continuously_active.preserved_by_helper);

        let mut clicked = test_cursor_stamp(100.0, 200.0, 7);
        clicked.counters.left_down += 1;
        let clicked = test_cursor_stamp(100.0, 200.0, 7).classify(clicked, delivery);
        assert!(clicked.position_unchanged);
        assert!(clicked.hid_system_pointer_activity);
        assert_eq!(
            clicked.shared_pointer_activity_state,
            SharedPointerActivityState::Contaminated
        );

        let mut scrolled = test_cursor_stamp(100.0, 200.0, 7);
        scrolled.counters.scroll_wheel += 1;
        let scrolled = test_cursor_stamp(100.0, 200.0, 7).classify(scrolled, delivery);
        assert!(scrolled.hid_system_pointer_activity);
        assert_eq!(
            scrolled.shared_pointer_activity_state,
            SharedPointerActivityState::Contaminated
        );
    }

    #[test]
    fn cursor_attribution_refuses_an_unverified_delivery_route() {
        let attribution = test_cursor_stamp(100.0, 200.0, 7).classify(
            test_cursor_stamp(100.0, 200.0, 7),
            InputDeliveryProvenance::unverified(),
        );
        assert!(attribution.position_unchanged);
        assert!(attribution.monitor_healthy);
        assert!(attribution.boundary_corroborated);
        assert!(!attribution.preserved_by_helper);
    }

    #[test]
    fn cursor_attribution_treats_counter_reset_or_implausible_flood_as_unknown() {
        let delivery = test_target_delivery();
        let reset = test_cursor_stamp(100.0, 200.0, 100)
            .classify(test_cursor_stamp(100.0, 200.0, 50), delivery);
        assert!(!reset.monitor_healthy);
        assert!(!reset.boundary_corroborated);
        assert!(!reset.preserved_by_helper);

        let flood = test_cursor_stamp(100.0, 200.0, 1).classify(
            test_cursor_stamp(
                100.0,
                200.0,
                MAX_HID_POINTER_COUNTER_ADVANCE.saturating_add(2),
            ),
            delivery,
        );
        assert!(!flood.monitor_healthy);
        assert!(!flood.boundary_corroborated);
        assert!(!flood.preserved_by_helper);
    }

    #[test]
    fn post_dispatch_semantic_error_cannot_mask_after_snapshot_invariant_failure() {
        let before = DesktopSnapshot {
            front_process: [1; 8],
            front_pid: 7,
            front_window_id: 70,
            cursor: test_cursor_stamp(100.0, 200.0, 7),
            active_space: 1,
        };
        let after = DesktopSnapshot {
            front_process: [2; 8],
            ..before
        };
        let report = before.compare(&after, test_target_delivery());
        let error = finish_semantic_after_snapshot(
            report,
            InvariantStage::SemanticSetValue,
            Err(ComputerError::new(
                "COMPUTER_POSTCONDITION_FAILED",
                "fixture postcondition failure after dispatch",
            )),
        )
        .unwrap_err();
        assert_eq!(error.code, "COMPUTER_BACKGROUND_CONTRACT_VIOLATION");
        assert!(error.message.contains("foregroundUnchanged"));
    }
}
