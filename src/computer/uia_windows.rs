//! Exact-HWND Windows UI Automation snapshots and semantic actions.
//!
//! The HWND owner is validated by the platform layer. UIA refs are bound to a
//! single snapshot, re-enumerated, and signature-checked before each action.

use std::ffi::c_void;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, ExpandCollapseState_Collapsed, IUIAutomation, IUIAutomationElement,
    IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
    IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern, IUIAutomationValuePattern,
    TreeScope_Subtree, UIA_ExpandCollapsePatternId, UIA_InvokePatternId,
    UIA_SelectionItemPatternId, UIA_TogglePatternId, UIA_ValuePatternId,
};
use windows::core::{BSTR, Interface};

use super::{ComputerError, SemanticBounds, SemanticElement, SemanticTarget, WindowDescriptor};

const MAX_ELEMENTS: i32 = 500;

pub fn snapshot(target: &WindowDescriptor) -> Result<Vec<SemanticTarget>, ComputerError> {
    unsafe {
        let (automation, root) = root(target)?;
        let condition = automation
            .CreateTrueCondition()
            .map_err(|error| uia_error("CreateTrueCondition", error))?;
        let array = root
            .FindAll(TreeScope_Subtree, &condition)
            .map_err(|error| uia_error("FindAll", error))?;
        let count = array
            .Length()
            .map_err(|error| uia_error("ElementArray.Length", error))?
            .min(MAX_ELEMENTS);
        let mut output = Vec::new();
        for index in 0..count {
            let Ok(element) = array.GetElement(index) else {
                continue;
            };
            let reference = format!("u{}", output.len() + 1);
            let Some(signature) = signature(&element, &reference) else {
                continue;
            };
            if !signature.actions.is_empty() && signature.enabled != Some(false) {
                output.push(SemanticTarget {
                    element: signature,
                    path: vec![index as usize],
                });
            }
        }
        Ok(output)
    }
}

pub fn invoke(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    action: &str,
) -> Result<Value, ComputerError> {
    if action != "press" {
        return Err(ComputerError::new(
            "COMPUTER_INVALID_REQUEST",
            "Windows UI Automation currently advertises press and setValue",
        ));
    }
    let before = snapshot(target)?;
    unsafe {
        let element = resolve_verified(target, semantic)?;
        if let Ok(pattern) = element.GetCurrentPattern(UIA_InvokePatternId) {
            pattern
                .cast::<IUIAutomationInvokePattern>()
                .and_then(|pattern| pattern.Invoke())
                .map_err(|error| uia_error("InvokePattern.Invoke", error))?;
        } else if let Ok(pattern) = element.GetCurrentPattern(UIA_TogglePatternId) {
            pattern
                .cast::<IUIAutomationTogglePattern>()
                .and_then(|pattern| pattern.Toggle())
                .map_err(|error| uia_error("TogglePattern.Toggle", error))?;
        } else if let Ok(pattern) = element.GetCurrentPattern(UIA_SelectionItemPatternId) {
            pattern
                .cast::<IUIAutomationSelectionItemPattern>()
                .and_then(|pattern| pattern.Select())
                .map_err(|error| uia_error("SelectionItemPattern.Select", error))?;
        } else if let Ok(pattern) = element.GetCurrentPattern(UIA_ExpandCollapsePatternId) {
            let pattern = pattern
                .cast::<IUIAutomationExpandCollapsePattern>()
                .map_err(|error| uia_error("ExpandCollapsePattern cast", error))?;
            if pattern.CurrentExpandCollapseState().ok() == Some(ExpandCollapseState_Collapsed) {
                pattern
                    .Expand()
                    .map_err(|error| uia_error("ExpandCollapsePattern.Expand", error))?;
            } else {
                pattern
                    .Collapse()
                    .map_err(|error| uia_error("ExpandCollapsePattern.Collapse", error))?;
            }
        } else {
            return Err(ComputerError::new(
                "COMPUTER_STALE_ELEMENT",
                "The UIA action pattern is no longer available",
            ));
        }
    }
    thread::sleep(Duration::from_millis(90));
    let after = snapshot(target).ok();
    let changed = after.as_ref().is_some_and(|after| {
        before
            .iter()
            .map(|item| &item.element)
            .ne(after.iter().map(|item| &item.element))
    });
    Ok(json!({
        "delivered": true,
        "effectObserved": changed,
        "postcondition": if changed { "window-state-changed" } else { "no-observable-change" },
    }))
}

pub fn set_value(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    value: &str,
) -> Result<Value, ComputerError> {
    unsafe {
        let element = resolve_verified(target, semantic)?;
        let pattern = element
            .GetCurrentPattern(UIA_ValuePatternId)
            .map_err(|error| uia_error("ValuePattern lookup", error))?
            .cast::<IUIAutomationValuePattern>()
            .map_err(|error| uia_error("ValuePattern cast", error))?;
        pattern
            .SetValue(&BSTR::from(value))
            .map_err(|error| uia_error("ValuePattern.SetValue", error))?;
    }
    thread::sleep(Duration::from_millis(60));
    let current = snapshot(target)?
        .into_iter()
        .find(|candidate| candidate.path == semantic.path)
        .and_then(|candidate| candidate.element.value);
    if current.as_deref() != Some(value) {
        return Err(ComputerError::new(
            "COMPUTER_POSTCONDITION_FAILED",
            "UIA accepted ValuePattern.SetValue but read-back did not match",
        ));
    }
    Ok(json!({
        "delivered": true,
        "effectObserved": true,
        "postcondition": "value-confirmed",
    }))
}

unsafe fn root(
    target: &WindowDescriptor,
) -> Result<(IUIAutomation, IUIAutomationElement), ComputerError> {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| uia_error("CoCreateInstance(CUIAutomation)", error))?;
    let hwnd = target
        .id
        .parse::<usize>()
        .map_err(|_| ComputerError::new("COMPUTER_STALE_FRAME", "Invalid target HWND"))?;
    let element = unsafe { automation.ElementFromHandle(HWND(hwnd as *mut c_void)) }
        .map_err(|error| uia_error("ElementFromHandle", error))?;
    Ok((automation, element))
}

unsafe fn resolve_verified(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
) -> Result<IUIAutomationElement, ComputerError> {
    let index =
        *semantic.path.first().ok_or_else(|| {
            ComputerError::new("COMPUTER_STALE_ELEMENT", "The UIA locator is invalid")
        })? as i32;
    let (automation, root) = unsafe { root(target)? };
    let condition = unsafe { automation.CreateTrueCondition() }
        .map_err(|error| uia_error("CreateTrueCondition", error))?;
    let array = unsafe { root.FindAll(TreeScope_Subtree, &condition) }
        .map_err(|error| uia_error("FindAll", error))?;
    let element = unsafe { array.GetElement(index) }.map_err(|_| {
        ComputerError::new(
            "COMPUTER_STALE_ELEMENT",
            "The UIA element index no longer resolves",
        )
    })?;
    let current = unsafe { signature(&element, &semantic.element.reference) };
    if current.as_ref() != Some(&semantic.element) {
        return Err(ComputerError::new(
            "COMPUTER_STALE_ELEMENT",
            "The UIA element changed since the frame was captured. Observe again.",
        ));
    }
    Ok(element)
}

unsafe fn signature(element: &IUIAutomationElement, reference: &str) -> Option<SemanticElement> {
    let control_type = unsafe { element.CurrentControlType().ok()? };
    let name = unsafe { element.CurrentName().ok()?.to_string() };
    let enabled = unsafe { element.CurrentIsEnabled().ok() }.map(|enabled| enabled.as_bool());
    let rectangle = unsafe { element.CurrentBoundingRectangle().ok() };
    let bounds = rectangle.and_then(|rectangle| {
        (rectangle.right > rectangle.left && rectangle.bottom > rectangle.top).then_some(
            SemanticBounds {
                x: rectangle.left as f64,
                y: rectangle.top as f64,
                width: (rectangle.right - rectangle.left) as f64,
                height: (rectangle.bottom - rectangle.top) as f64,
            },
        )
    });
    let mut actions = Vec::new();
    if unsafe { element.GetCurrentPattern(UIA_InvokePatternId) }.is_ok()
        || unsafe { element.GetCurrentPattern(UIA_TogglePatternId) }.is_ok()
        || unsafe { element.GetCurrentPattern(UIA_SelectionItemPatternId) }.is_ok()
        || unsafe { element.GetCurrentPattern(UIA_ExpandCollapsePatternId) }.is_ok()
    {
        actions.push("press".to_owned());
    }
    let value_pattern = unsafe { element.GetCurrentPattern(UIA_ValuePatternId) }
        .ok()
        .and_then(|pattern| pattern.cast::<IUIAutomationValuePattern>().ok());
    let value = value_pattern
        .as_ref()
        .and_then(|pattern| unsafe { pattern.CurrentValue().ok() })
        .map(|value| value.to_string());
    if value_pattern.is_some() {
        actions.push("setValue".to_owned());
    }
    Some(SemanticElement {
        reference: reference.to_owned(),
        role: format!("UIAControlType({})", control_type.0),
        name,
        value,
        enabled,
        actions,
        bounds,
    })
}

fn uia_error(operation: &str, error: impl std::fmt::Display) -> ComputerError {
    ComputerError::new(
        "COMPUTER_SEMANTIC_UNAVAILABLE",
        format!("Windows UI Automation {operation} failed: {error}"),
    )
}
