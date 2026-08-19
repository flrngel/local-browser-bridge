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
use std::time::Duration;

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
const AX_POINT: i32 = 1;
const AX_SIZE: i32 = 2;
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
) -> Result<Value, ComputerError> {
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
            return Err(ComputerError::new(
                "COMPUTER_SEMANTIC_ACTION_FAILED",
                format!("{native_action} failed with macOS AX error {result}"),
            ));
        }
    }
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
}

pub fn set_value(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    value: &str,
    cancellation: &CommandCancellation,
) -> Result<Value, ComputerError> {
    if semantic.element.sensitive || semantic.element.value_redacted {
        return Err(sensitive_semantic());
    }
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
            return Err(ComputerError::new(
                "COMPUTER_SEMANTIC_ACTION_FAILED",
                format!("AXValue write failed with macOS AX error {result}"),
            ));
        }
    }
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
    let mut candidate_ids = Vec::new();
    let mut candidate_summaries = Vec::new();
    let mut direct_match = None;
    for &window in &windows {
        let mut id = 0;
        let resolved = unsafe { _AXUIElementGetWindow(window, &mut id) } == AX_SUCCESS && id != 0;
        let role = unsafe { string_attr(window, "AXRole") }.unwrap_or_default();
        if resolved && !candidate_ids.contains(&id) {
            candidate_ids.push(id);
        }
        if candidate_summaries.len() < 32 {
            candidate_summaries.push(format!(
                "{}:{}:{:?}",
                role,
                unsafe { string_attr(window, "AXIdentifier") }.unwrap_or_default(),
                unsafe { element_bounds(window) }
            ));
        }
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
    Err(semantic_unavailable(format!(
        "CGWindowID {expected} did not resolve to an exact AX window; AX exposed {candidate_ids:?}; top-level {candidate_summaries:?}"
    )))
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
}
