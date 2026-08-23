//! Exact-window macOS Accessibility snapshots and semantic actions.
//!
//! Element refs are valid only for the screenshot that produced them. An
//! action re-resolves the element from its window-relative path and verifies
//! its semantic signature before dispatch, so a ref cannot drift to a new UI.

#![allow(non_camel_case_types, non_snake_case)]

use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{
    CFEqual, CFGetTypeID, CFRelease, CFRetain, CFTypeID, CFTypeRef, TCFType,
};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use serde_json::{Value, json};

use super::{
    CommandCancellation, ComputerError, SemanticBounds, SemanticElement, SemanticTarget,
    WindowDescriptor,
};

const AX_SUCCESS: i32 = 0;
const AX_ATTRIBUTE_UNSUPPORTED: i32 = -25205;
const AX_NO_VALUE: i32 = -25212;
const AX_POINT: i32 = 1;
const AX_SIZE: i32 = 2;
const AX_ELIGIBILITY_TIMEOUT_SECONDS: f32 = 0.05;
const AX_RECEIVER_TIMEOUT_SECONDS: f32 = 0.05;
const MAX_KEYBOARD_PARENT_DEPTH: usize = 4;
const MAX_KEYBOARD_TOP_LEVEL_ELEMENTS: usize = 256;
const KEYBOARD_RECEIVER_PROOF_BUDGET: Duration = Duration::from_millis(650);
const MAX_NODES: usize = 1_500;
const MAX_DEPTH: usize = 25;
const MAX_ACTIONABLE: usize = 500;

#[repr(C)]
struct __AXUIElement(c_void);
type AXUIElementRef = *mut __AXUIElement;
#[repr(C)]
struct __AXValue(c_void);
type AXValueRef = *mut __AXValue;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementCopyActionNames(element: AXUIElementRef, names: *mut CFArrayRef) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut u8,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout: f32) -> i32;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXValueGetValue(value: AXValueRef, kind: i32, output: *mut c_void) -> bool;
    fn _AXUIElementGetWindow(element: AXUIElementRef, window_id: *mut u32) -> i32;
}

pub fn accessibility_ready(prompt: bool) -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let value = if prompt {
            CFBoolean::true_value()
        } else {
            CFBoolean::false_value()
        };
        let options = CFDictionary::from_CFType_pairs(&[(key, value)]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    }
}

struct OwnedAxElement(AXUIElementRef);

impl OwnedAxElement {
    unsafe fn from_create_rule(element: AXUIElementRef) -> Self {
        Self(element)
    }

    unsafe fn retain(element: AXUIElementRef) -> Self {
        unsafe { CFRetain(element as CFTypeRef) };
        Self(element)
    }

    fn as_ptr(&self) -> AXUIElementRef {
        self.0
    }
}

impl Drop for OwnedAxElement {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0 as CFTypeRef) };
    }
}

/// A retained, exact, non-minimized target window whose `AXMain` attribute is
/// writable. Resolution is read-only; the caller separately authorizes the
/// write against its focus lease immediately before calling `set_main`.
pub struct ExactTargetMainWindow(OwnedAxElement);

impl ExactTargetMainWindow {
    pub fn set_main(&self, main: bool) -> bool {
        let attribute = CFString::new("AXMain");
        let value = if main {
            CFBoolean::true_value()
        } else {
            CFBoolean::false_value()
        };
        unsafe {
            AXUIElementSetAttributeValue(
                self.0.as_ptr(),
                attribute.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            ) == AX_SUCCESS
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KeyboardWindowFact {
    window_id: Option<u32>,
    top_level_role: bool,
    minimized: Option<bool>,
}

fn keyboard_target_eligible(
    target_id: u32,
    collections_complete: bool,
    app_hidden: Option<bool>,
    candidates: &[KeyboardWindowFact],
) -> bool {
    if !collections_complete || app_hidden != Some(false) {
        return false;
    }
    let target_candidates = candidates
        .iter()
        .filter(|candidate| candidate.top_level_role && candidate.window_id == Some(target_id))
        .collect::<Vec<_>>();
    !target_candidates.is_empty()
        && target_candidates
            .iter()
            .all(|candidate| candidate.minimized == Some(false))
}

fn keyboard_receiver_matches(
    target_id: u32,
    focused_window_id: Option<u32>,
    focused_element_window_id: Option<u32>,
    require_focused_element: bool,
) -> bool {
    focused_window_id == Some(target_id)
        && (!require_focused_element || focused_element_window_id == Some(target_id))
}

/// Proves the exact target is a non-minimized Accessibility top-level window.
/// Sibling count is deliberately not an authorization signal: ScreenCaptureKit
/// can add a same-PID AXDialog for its sharing indicator, while the receiver
/// proof below identifies the actual keyboard destination.
pub fn ensure_keyboard_target_eligible_before(
    target: &WindowDescriptor,
    action_deadline: Instant,
) -> Result<(), ComputerError> {
    let deadline = std::cmp::min(
        action_deadline,
        Instant::now() + KEYBOARD_RECEIVER_PROOF_BUDGET,
    );
    let target_id = target
        .id
        .parse::<u32>()
        .map_err(|_| keyboard_destination_unavailable())?;
    let app = keyboard_application(target.pid, deadline)?;
    ensure_receiver_deadline(deadline)?;
    let app_hidden = strict_bool_attr(app.as_ptr(), "AXHidden", deadline)?;
    ensure_receiver_deadline(deadline)?;
    let mut windows = required_element_array_attr(app.as_ptr(), "AXChildren", deadline)?;
    ensure_receiver_deadline(deadline)?;
    windows.extend(required_element_array_attr(
        app.as_ptr(),
        "AXWindows",
        deadline,
    )?);

    let mut facts = Vec::with_capacity(windows.len());
    for window in &windows {
        ensure_receiver_deadline(deadline)?;
        let window_id = candidate_window_id(window.as_ptr());
        let (top_level_role, minimized) = if window_id == Some(target_id) {
            let role = required_string_attr(window.as_ptr(), "AXRole", deadline)?;
            let top_level_role = matches!(role.as_str(), "AXWindow" | "AXSheet" | "AXDialog");
            let minimized = top_level_role
                .then(|| strict_bool_attr(window.as_ptr(), "AXMinimized", deadline))
                .transpose()?
                .flatten();
            (top_level_role, minimized)
        } else {
            (false, None)
        };
        facts.push(KeyboardWindowFact {
            window_id,
            top_level_role,
            minimized,
        });
    }
    ensure_receiver_deadline(deadline)?;
    if keyboard_target_eligible(target_id, true, app_hidden, &facts) {
        Ok(())
    } else {
        Err(keyboard_destination_unavailable())
    }
}

/// Proves which exact window will receive a subsequent PID-scoped key event.
/// This must run after private exact-window focus preparation and immediately
/// before dispatch. Text additionally requires the focused element to belong
/// to the target window; non-text chords use the focused window's responder
/// chain and therefore require the exact focused window itself.
/// Receiver proof capped by both its own AX budget and the caller's action
/// deadline. This keeps a late AX IPC from crossing the native text boundary.
pub fn ensure_keyboard_receiver_before(
    target: &WindowDescriptor,
    require_focused_element: bool,
    expected_frontmost: bool,
    action_deadline: Instant,
) -> Result<(), ComputerError> {
    let deadline = std::cmp::min(
        action_deadline,
        Instant::now() + KEYBOARD_RECEIVER_PROOF_BUDGET,
    );
    let target_id = target
        .id
        .parse::<u32>()
        .map_err(|_| keyboard_destination_unavailable())?;
    let app = receiver_application(target.pid, true, deadline)?;
    ensure_receiver_deadline(deadline)?;
    let app_hidden = strict_bool_attr(app.as_ptr(), "AXHidden", deadline)?;
    let app_frontmost = strict_bool_attr(app.as_ptr(), "AXFrontmost", deadline)?;
    ensure_receiver_deadline(deadline)?;
    let focused_window = required_element_attr(app.as_ptr(), "AXFocusedWindow", deadline)?;
    let focused_window_id = element_window_id(focused_window.as_ptr())?;
    ensure_receiver_deadline(deadline)?;
    let focused_window_minimized =
        strict_bool_attr(focused_window.as_ptr(), "AXMinimized", deadline)?;
    let focused_element_window_id = if require_focused_element {
        let focused_element = required_element_attr(app.as_ptr(), "AXFocusedUIElement", deadline)?;
        resolve_containing_window_id(&focused_element, deadline)?
    } else {
        None
    };
    ensure_receiver_deadline(deadline)?;
    if app_hidden == Some(false)
        && app_frontmost == Some(expected_frontmost)
        && focused_window_minimized == Some(false)
        && keyboard_receiver_matches(
            target_id,
            focused_window_id,
            focused_element_window_id,
            require_focused_element,
        )
    {
        Ok(())
    } else {
        Err(keyboard_destination_unavailable())
    }
}

fn ensure_receiver_deadline(deadline: Instant) -> Result<(), ComputerError> {
    if Instant::now() < deadline {
        Ok(())
    } else {
        Err(keyboard_destination_unavailable())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplicationFocusState {
    pub window_id: u32,
    pub main_window_id: u32,
    pub frontmost: bool,
}

/// Returns read-only exact focus state without enabling Chromium's manual or
/// enhanced Accessibility modes. It is safe to use for an unrelated front app.
pub fn application_focus_state_before(
    pid: u32,
    action_deadline: Instant,
) -> Result<ApplicationFocusState, ComputerError> {
    application_focus_state_impl(pid, false, action_deadline)
}

/// Reads the selected target application's focus state. Unlike the unrelated
/// foreground oracle above, this may use the existing one-time Chromium AX
/// opt-in needed to expose the selected target's exact window.
pub fn target_application_focus_state(pid: u32) -> Result<ApplicationFocusState, ComputerError> {
    application_focus_state_impl(pid, true, Instant::now() + KEYBOARD_RECEIVER_PROOF_BUDGET)
}

pub fn target_application_focus_state_before(
    pid: u32,
    action_deadline: Instant,
) -> Result<ApplicationFocusState, ComputerError> {
    application_focus_state_impl(pid, true, action_deadline)
}

/// Resolves one exact target AX window for a bounded, reversible `AXMain`
/// selection. The application-level `AXFocusedWindow` attribute is not used:
/// it can report a successful write while remaining unchanged. The exact
/// window's settable `AXMain` attribute is the receiver-selection primitive.
pub fn exact_target_main_window_before(
    pid: u32,
    target_id: u32,
    action_deadline: Instant,
) -> Result<ExactTargetMainWindow, ComputerError> {
    let deadline = std::cmp::min(
        action_deadline,
        Instant::now() + KEYBOARD_RECEIVER_PROOF_BUDGET,
    );
    let app = receiver_application(pid, true, deadline)?;
    ensure_receiver_deadline(deadline)?;
    if strict_bool_attr(app.as_ptr(), "AXHidden", deadline)? != Some(false) {
        return Err(keyboard_destination_unavailable());
    }
    let windows = required_element_array_attr(app.as_ptr(), "AXWindows", deadline)?;
    let mut matched = None;
    for window in windows {
        ensure_receiver_deadline(deadline)?;
        if candidate_window_id(window.as_ptr()) != Some(target_id) {
            continue;
        }
        if matched.is_some()
            || !matches!(
                required_string_attr(window.as_ptr(), "AXRole", deadline)?.as_str(),
                "AXWindow" | "AXSheet" | "AXDialog"
            )
            || strict_bool_attr(window.as_ptr(), "AXMinimized", deadline)? != Some(false)
            || !unsafe { attribute_settable(window.as_ptr(), "AXMain") }
        {
            return Err(keyboard_destination_unavailable());
        }
        matched = Some(ExactTargetMainWindow(window));
    }
    ensure_receiver_deadline(deadline)?;
    matched.ok_or_else(keyboard_destination_unavailable)
}

fn application_focus_state_impl(
    pid: u32,
    enable_target_accessibility: bool,
    action_deadline: Instant,
) -> Result<ApplicationFocusState, ComputerError> {
    let deadline = std::cmp::min(
        action_deadline,
        Instant::now() + KEYBOARD_RECEIVER_PROOF_BUDGET,
    );
    let app = receiver_application(pid, enable_target_accessibility, deadline)?;
    ensure_receiver_deadline(deadline)?;
    if strict_bool_attr(app.as_ptr(), "AXHidden", deadline)? != Some(false) {
        return Err(keyboard_destination_unavailable());
    }
    let focused_window = required_element_attr(app.as_ptr(), "AXFocusedWindow", deadline)?;
    let window_id =
        element_window_id(focused_window.as_ptr())?.ok_or_else(keyboard_destination_unavailable)?;
    ensure_receiver_deadline(deadline)?;
    let role = required_string_attr(focused_window.as_ptr(), "AXRole", deadline)?;
    if !matches!(role.as_str(), "AXWindow" | "AXSheet" | "AXDialog")
        || strict_bool_attr(focused_window.as_ptr(), "AXMinimized", deadline)? != Some(false)
    {
        return Err(keyboard_destination_unavailable());
    }
    ensure_receiver_deadline(deadline)?;
    let main_window = required_element_attr(app.as_ptr(), "AXMainWindow", deadline)?;
    let main_window_id =
        element_window_id(main_window.as_ptr())?.ok_or_else(keyboard_destination_unavailable)?;
    let main_role = required_string_attr(main_window.as_ptr(), "AXRole", deadline)?;
    if !matches!(main_role.as_str(), "AXWindow" | "AXSheet" | "AXDialog")
        || strict_bool_attr(main_window.as_ptr(), "AXMinimized", deadline)? != Some(false)
    {
        return Err(keyboard_destination_unavailable());
    }
    ensure_receiver_deadline(deadline)?;
    let frontmost = strict_bool_attr(app.as_ptr(), "AXFrontmost", deadline)?
        .ok_or_else(keyboard_destination_unavailable)?;
    ensure_receiver_deadline(deadline)?;
    Ok(ApplicationFocusState {
        window_id,
        main_window_id,
        frontmost,
    })
}

fn keyboard_application(pid: u32, deadline: Instant) -> Result<OwnedAxElement, ComputerError> {
    application_element(pid, true, AX_ELIGIBILITY_TIMEOUT_SECONDS, deadline)
}

fn receiver_application(
    pid: u32,
    enable_target_accessibility: bool,
    deadline: Instant,
) -> Result<OwnedAxElement, ComputerError> {
    application_element(
        pid,
        enable_target_accessibility,
        AX_RECEIVER_TIMEOUT_SECONDS,
        deadline,
    )
}

fn application_element(
    pid: u32,
    enable_target_accessibility: bool,
    timeout_seconds: f32,
    deadline: Instant,
) -> Result<OwnedAxElement, ComputerError> {
    ensure_receiver_deadline(deadline)?;
    if !accessibility_ready(false) {
        return Err(keyboard_destination_unavailable());
    }
    let raw = unsafe { AXUIElementCreateApplication(pid as i32) };
    if raw.is_null() {
        return Err(keyboard_destination_unavailable());
    }
    let app = unsafe { OwnedAxElement::from_create_rule(raw) };
    set_keyboard_timeout(app.as_ptr(), timeout_seconds, deadline)?;
    if enable_target_accessibility && enable_chromium_accessibility_once(pid, app.as_ptr()) {
        thread::sleep(Duration::from_millis(180));
        ensure_receiver_deadline(deadline)?;
    }
    Ok(app)
}

fn set_keyboard_timeout(
    element: AXUIElementRef,
    maximum_timeout_seconds: f32,
    deadline: Instant,
) -> Result<(), ComputerError> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(keyboard_destination_unavailable)?;
    let timeout_seconds = remaining
        .as_secs_f32()
        .min(maximum_timeout_seconds)
        .max(0.001);
    if unsafe { AXUIElementSetMessagingTimeout(element, timeout_seconds) } == AX_SUCCESS {
        Ok(())
    } else {
        Err(keyboard_destination_unavailable())
    }
}

fn required_element_array_attr(
    element: AXUIElementRef,
    name: &str,
    deadline: Instant,
) -> Result<Vec<OwnedAxElement>, ComputerError> {
    let value = copy_required_keyboard_attr(element, name, deadline)?;
    if unsafe { CFGetTypeID(value) } != CFArray::<CFTypeRef>::type_id() {
        unsafe { CFRelease(value) };
        return Err(keyboard_destination_unavailable());
    }
    let array = unsafe { CFArray::<CFTypeRef>::wrap_under_create_rule(value as _) };
    if array.len() as usize > MAX_KEYBOARD_TOP_LEVEL_ELEMENTS {
        return Err(keyboard_destination_unavailable());
    }
    let expected_type = unsafe { AXUIElementGetTypeID() };
    let mut output = Vec::with_capacity(array.len() as usize);
    for index in 0..array.len() {
        ensure_receiver_deadline(deadline)?;
        let Some(item) = array.get(index) else {
            return Err(keyboard_destination_unavailable());
        };
        let item = *item;
        if unsafe { CFGetTypeID(item) } != expected_type {
            return Err(keyboard_destination_unavailable());
        }
        let child = unsafe { OwnedAxElement::retain(item as AXUIElementRef) };
        set_keyboard_timeout(child.as_ptr(), AX_ELIGIBILITY_TIMEOUT_SECONDS, deadline)?;
        output.push(child);
    }
    Ok(output)
}

fn required_element_attr(
    element: AXUIElementRef,
    name: &str,
    deadline: Instant,
) -> Result<OwnedAxElement, ComputerError> {
    let value = copy_required_keyboard_attr(element, name, deadline)?;
    if unsafe { CFGetTypeID(value) } != unsafe { AXUIElementGetTypeID() } {
        unsafe { CFRelease(value) };
        return Err(keyboard_destination_unavailable());
    }
    let child = unsafe { OwnedAxElement::from_create_rule(value as AXUIElementRef) };
    set_keyboard_timeout(child.as_ptr(), AX_RECEIVER_TIMEOUT_SECONDS, deadline)?;
    Ok(child)
}

fn optional_element_attr(
    element: AXUIElementRef,
    name: &str,
    deadline: Instant,
) -> Result<Option<OwnedAxElement>, ComputerError> {
    let Some(value) = copy_optional_keyboard_attr(element, name, deadline)? else {
        return Ok(None);
    };
    if unsafe { CFGetTypeID(value) } != unsafe { AXUIElementGetTypeID() } {
        unsafe { CFRelease(value) };
        return Err(keyboard_destination_unavailable());
    }
    let child = unsafe { OwnedAxElement::from_create_rule(value as AXUIElementRef) };
    set_keyboard_timeout(child.as_ptr(), AX_RECEIVER_TIMEOUT_SECONDS, deadline)?;
    Ok(Some(child))
}

fn required_string_attr(
    element: AXUIElementRef,
    name: &str,
    deadline: Instant,
) -> Result<String, ComputerError> {
    let value = copy_required_keyboard_attr(element, name, deadline)?;
    if unsafe { CFGetTypeID(value) } != CFString::type_id() {
        unsafe { CFRelease(value) };
        return Err(keyboard_destination_unavailable());
    }
    Ok(unsafe { CFString::wrap_under_create_rule(value as _) }.to_string())
}

fn strict_bool_attr(
    element: AXUIElementRef,
    name: &str,
    deadline: Instant,
) -> Result<Option<bool>, ComputerError> {
    let Some(value) = copy_optional_keyboard_attr(element, name, deadline)? else {
        return Ok(None);
    };
    if unsafe { CFGetTypeID(value) } != CFBoolean::type_id() {
        unsafe { CFRelease(value) };
        return Err(keyboard_destination_unavailable());
    }
    let flag: bool = unsafe { CFBoolean::wrap_under_create_rule(value as _) }.into();
    Ok(Some(flag))
}

fn copy_required_keyboard_attr(
    element: AXUIElementRef,
    name: &str,
    deadline: Instant,
) -> Result<CFTypeRef, ComputerError> {
    copy_optional_keyboard_attr(element, name, deadline)?
        .ok_or_else(keyboard_destination_unavailable)
}

fn copy_optional_keyboard_attr(
    element: AXUIElementRef,
    name: &str,
    deadline: Instant,
) -> Result<Option<CFTypeRef>, ComputerError> {
    set_keyboard_timeout(element, AX_RECEIVER_TIMEOUT_SECONDS, deadline)?;
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = ptr::null();
    let status = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
    };
    if status == AX_SUCCESS && !value.is_null() {
        return Ok(Some(value));
    }
    if !value.is_null() {
        unsafe { CFRelease(value) };
    }
    if matches!(status, AX_ATTRIBUTE_UNSUPPORTED | AX_NO_VALUE) {
        Ok(None)
    } else {
        Err(keyboard_destination_unavailable())
    }
}

fn element_window_id(element: AXUIElementRef) -> Result<Option<u32>, ComputerError> {
    let mut window_id = 0_u32;
    let status = unsafe { _AXUIElementGetWindow(element, &mut window_id) };
    match (status, window_id) {
        (AX_SUCCESS, 1..) => Ok(Some(window_id)),
        (AX_SUCCESS, 0) | (AX_ATTRIBUTE_UNSUPPORTED | AX_NO_VALUE, _) => Ok(None),
        _ => Err(keyboard_destination_unavailable()),
    }
}

fn candidate_window_id(element: AXUIElementRef) -> Option<u32> {
    let mut window_id = 0_u32;
    (unsafe { _AXUIElementGetWindow(element, &mut window_id) } == AX_SUCCESS && window_id != 0)
        .then_some(window_id)
}

fn resolve_containing_window_id(
    focused_element: &OwnedAxElement,
    deadline: Instant,
) -> Result<Option<u32>, ComputerError> {
    let mut current = unsafe { OwnedAxElement::retain(focused_element.as_ptr()) };
    let mut visited = Vec::<OwnedAxElement>::new();
    for _ in 0..MAX_KEYBOARD_PARENT_DEPTH {
        ensure_receiver_deadline(deadline)?;
        if visited.iter().any(|candidate| unsafe {
            CFEqual(
                candidate.as_ptr() as CFTypeRef,
                current.as_ptr() as CFTypeRef,
            ) != 0
        }) {
            return Ok(None);
        }
        visited.push(unsafe { OwnedAxElement::retain(current.as_ptr()) });
        if let Some(window_id) = element_window_id(current.as_ptr())? {
            return Ok(Some(window_id));
        }
        for relationship in ["AXWindow", "AXTopLevelUIElement"] {
            ensure_receiver_deadline(deadline)?;
            if let Some(related) = optional_element_attr(current.as_ptr(), relationship, deadline)?
                && let Some(window_id) = element_window_id(related.as_ptr())?
            {
                return Ok(Some(window_id));
            }
        }
        ensure_receiver_deadline(deadline)?;
        let Some(parent) = optional_element_attr(current.as_ptr(), "AXParent", deadline)? else {
            return Ok(None);
        };
        current = parent;
    }
    Ok(None)
}

pub fn snapshot(target: &WindowDescriptor) -> Result<Vec<SemanticTarget>, ComputerError> {
    if !accessibility_ready(false) {
        return Err(semantic_unavailable(
            "Accessibility permission is required for semantic elements. Run the packaged helper with --request-permissions.",
        ));
    }
    unsafe {
        let root = exact_window(target)?;
        let mut output = Vec::new();
        let mut visited = 0;
        walk(root, &mut Vec::new(), 0, &mut visited, &mut output);
        CFRelease(root as CFTypeRef);
        Ok(output)
    }
}

pub fn invoke(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    action: &str,
    cancellation: &CommandCancellation,
) -> Result<AxDispatchAttempt, ComputerError> {
    let before_snapshot = snapshot(target).ok();
    let native_action = match action {
        "press" => "AXPress",
        "showMenu" => "AXShowMenu",
        "pick" => "AXPick",
        "confirm" => "AXConfirm",
        "cancel" => "AXCancel",
        "open" => "AXOpen",
        _ => return Err(invalid_semantic("Unsupported native accessibility action")),
    };
    let dispatch = AxDispatchRecord::new(target, AxDispatchOperation::Invoke)?;
    unsafe {
        let element = resolve_verified(target, semantic)?;
        let action_name = CFString::new(native_action);
        if let Err(error) = cancellation.begin_side_effect("macOS AX action dispatch") {
            CFRelease(element as CFTypeRef);
            return Err(error);
        }
        let result = AXUIElementPerformAction(element, action_name.as_concrete_TypeRef());
        CFRelease(element as CFTypeRef);
        if result != AX_SUCCESS {
            return Ok(AxDispatchAttempt::new(
                dispatch,
                Err(ComputerError::new(
                    "COMPUTER_SEMANTIC_ACTION_FAILED",
                    format!("{native_action} failed with macOS AX error {result}"),
                )),
            ));
        }
        let dispatch = dispatch.with_os_acceptance();
        cancellation.mark_verification_started();
        let outcome = (|| {
            cancellation.check("macOS AX action observation")?;
            thread::sleep(Duration::from_millis(90));
            cancellation.check("macOS AX action observation")?;
            let mut effect = observe_effect(target, semantic, None);
            if !effect.0
                && let (Some(before), Ok(after)) = (before_snapshot, snapshot(target))
            {
                let before = before
                    .into_iter()
                    .map(|target| target.element)
                    .collect::<Vec<_>>();
                let after = after
                    .into_iter()
                    .map(|target| target.element)
                    .collect::<Vec<_>>();
                if before != after {
                    effect = (true, "window-state-changed");
                }
            }
            Ok(json!({
                "delivered": true,
                "effectObserved": effect.0,
                "postcondition": effect.1,
            }))
        })();
        Ok(AxDispatchAttempt::new(dispatch, outcome))
    }
}

pub fn set_value(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    value: &str,
    cancellation: &CommandCancellation,
) -> Result<AxDispatchAttempt, ComputerError> {
    if semantic.element.sensitive || semantic.element.value_redacted {
        return Err(sensitive_semantic());
    }
    let dispatch = AxDispatchRecord::new(target, AxDispatchOperation::SetValue)?;
    unsafe {
        let element = resolve_verified(target, semantic)?;
        let attr = CFString::new("AXValue");
        let cf_value = CFString::new(value);
        if let Err(error) = cancellation.begin_side_effect("macOS AX value dispatch") {
            CFRelease(element as CFTypeRef);
            return Err(error);
        }
        let result = AXUIElementSetAttributeValue(
            element,
            attr.as_concrete_TypeRef(),
            cf_value.as_CFTypeRef(),
        );
        CFRelease(element as CFTypeRef);
        if result != AX_SUCCESS {
            return Ok(AxDispatchAttempt::new(
                dispatch,
                Err(ComputerError::new(
                    "COMPUTER_SEMANTIC_ACTION_FAILED",
                    format!("AXValue write failed with macOS AX error {result}"),
                )),
            ));
        }
        let dispatch = dispatch.with_os_acceptance();
        cancellation.mark_verification_started();
        let outcome = (|| {
            cancellation.check("macOS AX value observation")?;
            thread::sleep(Duration::from_millis(60));
            cancellation.check("macOS AX value observation")?;
            let effect = observe_effect(target, semantic, Some(value));
            if !effect.0 {
                return Err(ComputerError::new(
                    "COMPUTER_POSTCONDITION_FAILED",
                    "macOS accepted AXValue but the requested value was not observed",
                ));
            }
            Ok(json!({
                "delivered": true,
                "effectObserved": true,
                "postcondition": effect.1,
            }))
        })();
        Ok(AxDispatchAttempt::new(dispatch, outcome))
    }
}

fn observe_effect(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    expected_value: Option<&str>,
) -> (bool, &'static str) {
    unsafe {
        let Ok(root) = exact_window(target) else {
            return (true, "target-window-closed");
        };
        let resolved = resolve_path(root, &semantic.path);
        CFRelease(root as CFTypeRef);
        let Some(element) = resolved else {
            return (true, "element-disappeared");
        };
        let value = if expected_value.is_some() && !semantic.element.sensitive {
            stringish_attr(element, "AXValue")
        } else {
            None
        };
        let current = element_signature(element, &semantic.element.reference);
        CFRelease(element as CFTypeRef);
        if let Some(expected) = expected_value {
            let masked_length_matches = value.as_deref().is_some_and(|actual| {
                actual.chars().count() == expected.chars().count()
                    && actual
                        .chars()
                        .all(|character| matches!(character, '•' | '*' | '●'))
            });
            return (
                value.as_deref() == Some(expected) || masked_length_matches,
                if value.as_deref() == Some(expected) {
                    "value-confirmed"
                } else if masked_length_matches {
                    "masked-length-confirmed"
                } else {
                    "value-not-confirmed"
                },
            );
        }
        if current.as_ref() != Some(&semantic.element) {
            (true, "element-state-changed")
        } else {
            (false, "no-observable-change")
        }
    }
}

unsafe fn exact_window(target: &WindowDescriptor) -> Result<AXUIElementRef, ComputerError> {
    let expected = target.id.parse::<u32>().map_err(|_| {
        ComputerError::new("COMPUTER_STALE_FRAME", "Invalid native macOS window id")
    })?;
    let app = unsafe { AXUIElementCreateApplication(target.pid as i32) };
    if app.is_null() {
        return Err(semantic_unavailable(
            "Could not create the application AX element",
        ));
    }
    unsafe { AXUIElementSetMessagingTimeout(app, 2.0) };
    if enable_chromium_accessibility_once(target.pid, app) {
        thread::sleep(Duration::from_millis(180));
    }
    // AXChildren and AXWindows are both required: Chromium can omit a
    // background/native dialog from either collection depending on focus.
    let mut windows = unsafe { element_array_attr(app, "AXChildren") };
    windows.extend(unsafe { element_array_attr(app, "AXWindows") });
    unsafe { CFRelease(app as CFTypeRef) };
    let mut direct_match = None;
    for &window in &windows {
        let mut id = 0;
        let resolved = unsafe { _AXUIElementGetWindow(window, &mut id) } == AX_SUCCESS && id != 0;
        let role = unsafe { string_attr(window, "AXRole") }.unwrap_or_default();
        let matched = resolved
            && id == expected
            && matches!(role.as_str(), "AXWindow" | "AXSheet" | "AXDialog");
        if matched {
            unsafe { AXUIElementSetMessagingTimeout(window, 2.0) };
            direct_match = Some(window);
            break;
        }
    }
    if let Some(matched) = direct_match {
        unsafe { CFRetain(matched as CFTypeRef) };
        for window in windows {
            unsafe { CFRelease(window as CFTypeRef) };
        }
        return Ok(matched);
    }

    // Chrome hosts its macOS Open/Save panel inside the owning browser AX
    // window even though WindowServer gives the panel its own CGWindowID. Bind
    // that surface only when one unique AX container matches the screenshot's
    // exact geometry; never substitute the whole parent window.
    let mut surface_matches = Vec::new();
    for &window in &windows {
        unsafe { find_surface_by_bounds(window, target, 0, &mut surface_matches) };
    }
    for window in windows {
        unsafe { CFRelease(window as CFTypeRef) };
    }
    if surface_matches.len() == 1 {
        unsafe { AXUIElementSetMessagingTimeout(surface_matches[0], 2.0) };
        return Ok(surface_matches.remove(0));
    }
    for surface in surface_matches.drain(..) {
        unsafe { CFRelease(surface as CFTypeRef) };
    }
    Err(semantic_unavailable(
        "The exact macOS Accessibility window could not be resolved",
    ))
}

fn enable_chromium_accessibility_once(pid: u32, app: AXUIElementRef) -> bool {
    static ENABLED: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();
    let enabled = ENABLED.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut enabled) = enabled.lock() else {
        return false;
    };
    if enabled.contains(&pid) {
        return false;
    }
    let attr = CFString::new("AXManualAccessibility");
    let value = CFBoolean::true_value();
    let manual = unsafe {
        AXUIElementSetAttributeValue(app, attr.as_concrete_TypeRef(), value.as_CFTypeRef())
    };
    let accepted = if manual == AX_SUCCESS {
        true
    } else if manual == -25205 {
        let fallback = CFString::new("AXEnhancedUserInterface");
        (unsafe {
            AXUIElementSetAttributeValue(app, fallback.as_concrete_TypeRef(), value.as_CFTypeRef())
        }) == AX_SUCCESS
    } else {
        false
    };
    if accepted {
        enabled.insert(pid);
    }
    accepted
}

unsafe fn find_surface_by_bounds(
    element: AXUIElementRef,
    target: &WindowDescriptor,
    depth: usize,
    output: &mut Vec<AXUIElementRef>,
) {
    if depth > MAX_DEPTH || output.len() > 1 {
        return;
    }
    let role = unsafe { string_attr(element, "AXRole") }.unwrap_or_default();
    let matches_role = matches!(
        role.as_str(),
        "AXWindow" | "AXSheet" | "AXGroup" | "AXDialog"
    );
    let matches_bounds = unsafe { element_bounds(element) }.is_some_and(|bounds| {
        (bounds.x - target.x as f64).abs() <= 2.0
            && (bounds.y - target.y as f64).abs() <= 2.0
            && (bounds.width - target.width as f64).abs() <= 2.0
            && (bounds.height - target.height as f64).abs() <= 2.0
    });
    if matches_role && matches_bounds {
        let duplicate = output.iter().any(
            |candidate| unsafe { CFEqual(*candidate as CFTypeRef, element as CFTypeRef) } != 0,
        );
        if !duplicate {
            unsafe { CFRetain(element as CFTypeRef) };
            output.push(element);
        }
        return;
    }
    for child in unsafe { traversal_children(element) } {
        unsafe { find_surface_by_bounds(child, target, depth + 1, output) };
        unsafe { CFRelease(child as CFTypeRef) };
    }
}

unsafe fn walk(
    element: AXUIElementRef,
    path: &mut Vec<usize>,
    depth: usize,
    visited: &mut usize,
    output: &mut Vec<SemanticTarget>,
) {
    if depth > MAX_DEPTH || *visited >= MAX_NODES || output.len() >= MAX_ACTIONABLE {
        return;
    }
    *visited += 1;
    let reference = format!("a{}", output.len() + 1);
    if let Some(element_info) = unsafe { element_signature(element, &reference) }
        && !element_info.actions.is_empty()
        && element_info.enabled != Some(false)
    {
        output.push(SemanticTarget {
            element: element_info,
            path: path.clone(),
        });
    }
    let children = unsafe { traversal_children(element) };
    for (index, child) in children.into_iter().enumerate() {
        path.push(index);
        unsafe { walk(child, path, depth + 1, visited, output) };
        path.pop();
        unsafe { CFRelease(child as CFTypeRef) };
        if *visited >= MAX_NODES || output.len() >= MAX_ACTIONABLE {
            break;
        }
    }
}

unsafe fn resolve_verified(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
) -> Result<AXUIElementRef, ComputerError> {
    let root = unsafe { exact_window(target)? };
    let resolved = unsafe { resolve_path(root, &semantic.path) };
    unsafe { CFRelease(root as CFTypeRef) };
    let element = resolved.ok_or_else(|| {
        ComputerError::new(
            "COMPUTER_STALE_ELEMENT",
            "The accessibility path no longer resolves. Observe again.",
        )
    })?;
    let current = unsafe { element_signature(element, &semantic.element.reference) };
    if current.as_ref() != Some(&semantic.element) {
        unsafe { CFRelease(element as CFTypeRef) };
        return Err(ComputerError::new(
            "COMPUTER_STALE_ELEMENT",
            "The accessibility element changed since the frame was captured. Observe again.",
        ));
    }
    Ok(element)
}

unsafe fn resolve_path(root: AXUIElementRef, path: &[usize]) -> Option<AXUIElementRef> {
    unsafe { CFRetain(root as CFTypeRef) };
    let mut current = root;
    for index in path {
        let mut children = unsafe { traversal_children(current) };
        unsafe { CFRelease(current as CFTypeRef) };
        if *index >= children.len() {
            for child in children {
                unsafe { CFRelease(child as CFTypeRef) };
            }
            return None;
        }
        let next = children.swap_remove(*index);
        for child in children {
            unsafe { CFRelease(child as CFTypeRef) };
        }
        current = next;
    }
    Some(current)
}

unsafe fn element_signature(element: AXUIElementRef, reference: &str) -> Option<SemanticElement> {
    let role = unsafe { string_attr(element, "AXRole") }?;
    let subrole = unsafe { string_attr(element, "AXSubrole") };
    let sensitive = sensitive_ax_semantics(&role, subrole.as_deref());
    let title = unsafe { string_attr(element, "AXTitle") }.unwrap_or_default();
    let description = unsafe { string_attr(element, "AXDescription") }.unwrap_or_default();
    let identifier = unsafe { string_attr(element, "AXIdentifier") }.unwrap_or_default();
    let value = if sensitive {
        None
    } else {
        unsafe { stringish_attr(element, "AXValue") }
    };
    let name = [&title, &description, &identifier]
        .into_iter()
        .find(|candidate| !candidate.trim().is_empty())
        .cloned()
        .or_else(|| value.clone())
        .unwrap_or_else(|| role.clone());
    let mut actions = unsafe { action_names(element) }
        .into_iter()
        .filter_map(|action| match action.as_str() {
            "AXPress" => Some("press"),
            "AXShowMenu" => Some("showMenu"),
            "AXPick" => Some("pick"),
            "AXConfirm" => Some("confirm"),
            "AXCancel" => Some("cancel"),
            "AXOpen" => Some("open"),
            _ => None,
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if !sensitive
        && matches!(
            role.as_str(),
            "AXTextField"
                | "AXTextArea"
                | "AXComboBox"
                | "AXSlider"
                | "AXStepper"
                | "AXIncrementor"
        )
        && unsafe { attribute_settable(element, "AXValue") }
    {
        actions.push("setValue".to_owned());
    }
    actions.sort();
    actions.dedup();
    Some(SemanticElement {
        reference: reference.to_owned(),
        role,
        name,
        value,
        sensitive,
        value_redacted: sensitive,
        enabled: unsafe { bool_attr(element, "AXEnabled") },
        actions,
        bounds: unsafe { element_bounds(element) },
        coordinate_space: "screen-points".to_owned(),
        screen_bounds: None,
        native_identity: None,
    })
}

fn sensitive_ax_semantics(role: &str, subrole: Option<&str>) -> bool {
    role == "AXSecureTextField"
        || subrole.is_some_and(|subrole| {
            subrole == "AXSecureTextField" || subrole.starts_with("AXSecure")
        })
}

fn sensitive_semantic() -> ComputerError {
    ComputerError::new(
        "COMPUTER_SENSITIVE_ELEMENT",
        "Sensitive accessibility values are redacted and cannot be read or set",
    )
}

unsafe fn element_array_attr(element: AXUIElementRef, name: &str) -> Vec<AXUIElementRef> {
    let attr = CFString::new(name);
    let mut value: CFTypeRef = ptr::null();
    if unsafe { AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value) }
        != AX_SUCCESS
        || value.is_null()
    {
        return Vec::new();
    }
    if unsafe { CFGetTypeID(value) } != CFArray::<CFTypeRef>::type_id() {
        unsafe { CFRelease(value) };
        return Vec::new();
    }
    let array = unsafe { CFArray::<CFTypeRef>::wrap_under_create_rule(value as _) };
    let ax_type = unsafe { AXUIElementGetTypeID() };
    (0..array.len())
        .filter_map(|index| {
            let item = *array.get(index)?;
            if unsafe { CFGetTypeID(item) } == ax_type {
                unsafe { CFRetain(item) };
                Some(item as AXUIElementRef)
            } else {
                None
            }
        })
        .collect()
}

unsafe fn traversal_children(element: AXUIElementRef) -> Vec<AXUIElementRef> {
    let mut output = Vec::new();
    for name in ["AXChildren", "AXContents", "AXChildrenInNavigationOrder"] {
        for child in unsafe { element_array_attr(element, name) } {
            let duplicate = output.iter().any(|candidate| {
                (unsafe { CFEqual(*candidate as CFTypeRef, child as CFTypeRef) }) != 0
            });
            if duplicate {
                unsafe { CFRelease(child as CFTypeRef) };
            } else {
                output.push(child);
            }
        }
    }
    output
}

unsafe fn string_attr(element: AXUIElementRef, name: &str) -> Option<String> {
    let value = unsafe { attribute_value(element, name)? };
    if unsafe { CFGetTypeID(value) } != CFString::type_id() {
        unsafe { CFRelease(value) };
        return None;
    }
    let string = unsafe { CFString::wrap_under_create_rule(value as _) };
    Some(string.to_string())
}

unsafe fn stringish_attr(element: AXUIElementRef, name: &str) -> Option<String> {
    let value = unsafe { attribute_value(element, name)? };
    let type_id = unsafe { CFGetTypeID(value) };
    let output = if type_id == CFString::type_id() {
        Some(unsafe { CFString::wrap_under_get_rule(value as _) }.to_string())
    } else if type_id == CFNumber::type_id() {
        unsafe { CFNumber::wrap_under_get_rule(value as _) }
            .to_f64()
            .map(|number| number.to_string())
    } else if type_id == CFBoolean::type_id() {
        let flag: bool = unsafe { CFBoolean::wrap_under_get_rule(value as _) }.into();
        Some(flag.to_string())
    } else {
        None
    };
    unsafe { CFRelease(value) };
    output
}

unsafe fn bool_attr(element: AXUIElementRef, name: &str) -> Option<bool> {
    let value = unsafe { attribute_value(element, name)? };
    let type_id = unsafe { CFGetTypeID(value) };
    let output = if type_id == CFBoolean::type_id() {
        Some(unsafe { CFBoolean::wrap_under_get_rule(value as _) }.into())
    } else if type_id == CFNumber::type_id() {
        unsafe { CFNumber::wrap_under_get_rule(value as _) }
            .to_f64()
            .map(|number| number != 0.0)
    } else {
        None
    };
    unsafe { CFRelease(value) };
    output
}

unsafe fn attribute_value(element: AXUIElementRef, name: &str) -> Option<CFTypeRef> {
    let attr = CFString::new(name);
    let mut value: CFTypeRef = ptr::null();
    (unsafe { AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value) }
        == AX_SUCCESS
        && !value.is_null())
    .then_some(value)
}

unsafe fn action_names(element: AXUIElementRef) -> Vec<String> {
    let mut names: CFArrayRef = ptr::null();
    if unsafe { AXUIElementCopyActionNames(element, &mut names) } != AX_SUCCESS || names.is_null() {
        return Vec::new();
    }
    let array = unsafe { CFArray::<CFTypeRef>::wrap_under_create_rule(names as _) };
    (0..array.len())
        .filter_map(|index| {
            let item = *array.get(index)?;
            (unsafe { CFGetTypeID(item) } == CFString::type_id())
                .then(|| unsafe { CFString::wrap_under_get_rule(item as _) }.to_string())
        })
        .collect()
}

unsafe fn attribute_settable(element: AXUIElementRef, name: &str) -> bool {
    let attr = CFString::new(name);
    let mut settable = 0_u8;
    (unsafe { AXUIElementIsAttributeSettable(element, attr.as_concrete_TypeRef(), &mut settable) })
        == AX_SUCCESS
        && settable != 0
}

unsafe fn element_bounds(element: AXUIElementRef) -> Option<SemanticBounds> {
    #[repr(C)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    struct Size {
        width: f64,
        height: f64,
    }
    let position = unsafe { attribute_value(element, "AXPosition")? };
    let size = unsafe { attribute_value(element, "AXSize")? };
    let mut point = Point { x: 0.0, y: 0.0 };
    let mut dimensions = Size {
        width: 0.0,
        height: 0.0,
    };
    let point_ok = unsafe {
        AXValueGetValue(
            position as AXValueRef,
            AX_POINT,
            &mut point as *mut _ as *mut c_void,
        )
    };
    let size_ok = unsafe {
        AXValueGetValue(
            size as AXValueRef,
            AX_SIZE,
            &mut dimensions as *mut _ as *mut c_void,
        )
    };
    unsafe {
        CFRelease(position);
        CFRelease(size);
    }
    (point_ok && size_ok && dimensions.width > 0.0 && dimensions.height > 0.0).then_some(
        SemanticBounds {
            x: point.x,
            y: point.y,
            width: dimensions.width,
            height: dimensions.height,
        },
    )
}

fn semantic_unavailable(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_SEMANTIC_UNAVAILABLE", message)
}

fn keyboard_destination_unavailable() -> ComputerError {
    ComputerError::new(
        "COMPUTER_BACKGROUND_UNAVAILABLE",
        "Could not prove the exact macOS keyboard receiver",
    )
}

fn invalid_semantic(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_INVALID_REQUEST", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_role_or_subrole_is_always_sensitive() {
        assert!(sensitive_ax_semantics("AXSecureTextField", None));
        assert!(sensitive_ax_semantics(
            "AXTextField",
            Some("AXSecureTextField")
        ));
        assert!(sensitive_ax_semantics(
            "AXTextField",
            Some("AXSecureCustomField")
        ));
        assert!(!sensitive_ax_semantics("AXTextField", None));
    }

    fn target_window(minimized: Option<bool>) -> KeyboardWindowFact {
        KeyboardWindowFact {
            window_id: Some(62090),
            top_level_role: true,
            minimized,
        }
    }

    #[test]
    fn keyboard_target_requires_known_non_minimized_exact_top_level_window() {
        assert!(keyboard_target_eligible(
            62090,
            true,
            Some(false),
            &[target_window(Some(false))]
        ));
        assert!(!keyboard_target_eligible(
            62090,
            true,
            Some(false),
            &[target_window(Some(true))]
        ));
        assert!(!keyboard_target_eligible(
            62090,
            true,
            Some(false),
            &[target_window(None)]
        ));
        assert!(!keyboard_target_eligible(
            62090,
            true,
            Some(true),
            &[target_window(Some(false))]
        ));
        assert!(!keyboard_target_eligible(
            62090,
            true,
            None,
            &[target_window(Some(false))]
        ));
    }

    #[test]
    fn keyboard_target_dedupes_collections_and_ignores_arbitrary_ax_children() {
        let arbitrary_child = KeyboardWindowFact {
            window_id: None,
            top_level_role: false,
            minimized: None,
        };
        assert!(keyboard_target_eligible(
            62090,
            true,
            Some(false),
            &[
                arbitrary_child,
                target_window(Some(false)),
                target_window(Some(false)),
            ]
        ));
    }

    #[test]
    fn keyboard_target_collection_error_and_missing_target_fail_closed() {
        assert!(!keyboard_target_eligible(
            62090,
            false,
            Some(false),
            &[target_window(Some(false))]
        ));
        assert!(!keyboard_target_eligible(
            62090,
            true,
            Some(false),
            &[KeyboardWindowFact {
                window_id: Some(62100),
                top_level_role: true,
                minimized: Some(false),
            }]
        ));
    }

    #[test]
    fn keyboard_receiver_requires_exact_focused_window_and_text_element_owner() {
        assert!(keyboard_receiver_matches(62090, Some(62090), None, false));
        assert!(keyboard_receiver_matches(
            62090,
            Some(62090),
            Some(62090),
            true
        ));
        assert!(!keyboard_receiver_matches(
            62090,
            Some(62100),
            Some(62090),
            true
        ));
        assert!(!keyboard_receiver_matches(
            62090,
            Some(62090),
            Some(62100),
            true
        ));
        assert!(!keyboard_receiver_matches(62090, Some(62090), None, true));
    }

    fn dispatch_target() -> WindowDescriptor {
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

    #[test]
    fn accessibility_dispatch_record_binds_pid_window_operation_and_acceptance() {
        let target = dispatch_target();
        let invoke = AxDispatchRecord::new(&target, AxDispatchOperation::Invoke).unwrap();
        assert!(invoke.matches(&target, AxDispatchOperation::Invoke));
        assert!(!invoke.matches(&target, AxDispatchOperation::SetValue));
        assert!(!invoke.os_acceptance_observed());

        let mut wrong_pid = target.clone();
        wrong_pid.pid += 1;
        assert!(!invoke.matches(&wrong_pid, AxDispatchOperation::Invoke));
        let mut wrong_window = target.clone();
        wrong_window.id = "62100".to_owned();
        assert!(!invoke.matches(&wrong_window, AxDispatchOperation::Invoke));

        let accepted = invoke.with_os_acceptance();
        assert!(accepted.os_acceptance_observed());
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AxDispatchOperation {
    Invoke,
    SetValue,
}

pub(crate) struct AxDispatchRecord {
    target_pid: u32,
    target_window_id: u32,
    operation: AxDispatchOperation,
    os_acceptance_observed: bool,
}

impl AxDispatchRecord {
    fn new(
        target: &WindowDescriptor,
        operation: AxDispatchOperation,
    ) -> Result<Self, ComputerError> {
        Ok(Self {
            target_pid: target.pid,
            target_window_id: target.id.parse::<u32>().map_err(|_| {
                ComputerError::new(
                    "COMPUTER_STALE_FRAME",
                    "The exact macOS Accessibility window id is invalid",
                )
            })?,
            operation,
            os_acceptance_observed: false,
        })
    }

    fn with_os_acceptance(mut self) -> Self {
        self.os_acceptance_observed = true;
        self
    }

    pub(crate) fn matches(
        &self,
        target: &WindowDescriptor,
        operation: AxDispatchOperation,
    ) -> bool {
        self.target_pid == target.pid
            && Some(self.target_window_id) == target.id.parse::<u32>().ok()
            && self.operation == operation
    }

    pub(crate) fn os_acceptance_observed(&self) -> bool {
        self.os_acceptance_observed
    }
}

pub(crate) struct AxDispatchAttempt {
    dispatch: AxDispatchRecord,
    outcome: Result<Value, ComputerError>,
}

impl AxDispatchAttempt {
    fn new(dispatch: AxDispatchRecord, outcome: Result<Value, ComputerError>) -> Self {
        Self { dispatch, outcome }
    }

    pub(crate) fn into_parts(self) -> (AxDispatchRecord, Result<Value, ComputerError>) {
        (self.dispatch, self.outcome)
    }
}
