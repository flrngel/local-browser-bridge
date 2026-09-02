//! Exact-HWND Windows UI Automation snapshots and semantic actions.
//!
//! The HWND owner is validated by the platform layer. UIA refs are bound to a
//! single snapshot, resolved through bounded Control View paths, and checked
//! against provider identity plus their semantic signature before each action.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Accessibility::{
    AutomationElementMode_Full, CUIAutomation, ExpandCollapseState_Collapsed,
    ExpandCollapseState_Expanded, ExpandCollapseState_LeafNode, IUIAutomation,
    IUIAutomationCacheRequest, IUIAutomationElement, IUIAutomationExpandCollapsePattern,
    IUIAutomationInvokePattern, IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern,
    IUIAutomationTreeWalker, IUIAutomationValuePattern, TreeScope_Element,
    UIA_AutomationIdPropertyId, UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_ExpandCollapsePatternId, UIA_FrameworkIdPropertyId,
    UIA_InvokePatternId, UIA_IsEnabledPropertyId, UIA_IsPasswordPropertyId, UIA_NamePropertyId,
    UIA_ProcessIdPropertyId, UIA_SelectionItemPatternId, UIA_TogglePatternId,
    UIA_ValueIsReadOnlyPropertyId, UIA_ValuePatternId,
};
use windows::core::{BSTR, Interface};

use super::{
    CommandCancellation, ComputerError, NativeElementIdentity, SemanticBounds, SemanticElement,
    SemanticSnapshot, SemanticTarget, SemanticTruncationReason, WindowDescriptor,
};

const MAX_VISITED_NODES: usize = 1_500;
const MAX_DEPTH: usize = 25;
const MAX_ACTIONABLE: usize = 500;
const WALK_DEADLINE: Duration = Duration::from_millis(750);

/// The deadline bounds work between provider calls. UIA is an in-process COM
/// client of arbitrary providers, so one provider call can still block past the
/// deadline. The helper-wide disposable-worker supervisor can recover the
/// process, but UIA offers no per-call cancellation; finer containment still
/// requires a dedicated UIA broker process. Unsafe thread termination is
/// deliberately not attempted here.
struct WalkBudget {
    started: Instant,
    visited: usize,
    truncation_reason: Option<SemanticTruncationReason>,
}

impl WalkBudget {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            visited: 0,
            truncation_reason: None,
        }
    }

    fn limit_reason(&self, depth: usize, actionable: usize) -> Option<SemanticTruncationReason> {
        if self.started.elapsed() >= WALK_DEADLINE {
            Some(SemanticTruncationReason::Deadline)
        } else if self.visited >= MAX_VISITED_NODES {
            Some(SemanticTruncationReason::NodeBudget)
        } else if actionable >= MAX_ACTIONABLE {
            Some(SemanticTruncationReason::ActionableBudget)
        } else if depth > MAX_DEPTH {
            Some(SemanticTruncationReason::DepthBudget)
        } else {
            None
        }
    }

    fn mark(&mut self, reason: SemanticTruncationReason) {
        self.truncation_reason.get_or_insert(reason);
    }

    fn exhausted(&mut self, depth: usize, actionable: usize) -> bool {
        if let Some(reason) = self.limit_reason(depth, actionable) {
            self.mark(reason);
            true
        } else {
            false
        }
    }

    fn claim(&mut self, depth: usize, actionable: usize) -> bool {
        if self.exhausted(depth, actionable) {
            return false;
        }
        self.visited += 1;
        true
    }
}

/// Balances every successful `CoInitializeEx`, including `S_FALSE`, on the
/// same thread. The `Rc` marker prevents an apartment guard from being moved to
/// another thread before `CoUninitialize` runs.
struct ComApartment {
    _same_thread: PhantomData<Rc<()>>,
}

impl ComApartment {
    fn enter_mta() -> Result<Self, ComputerError> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Err(ComputerError::new(
                "COMPUTER_SEMANTIC_UNAVAILABLE",
                "Windows UI Automation requires an MTA thread, but this thread already uses an incompatible COM apartment",
            ));
        }
        result
            .ok()
            .map_err(|error| uia_error("CoInitializeEx(COINIT_MULTITHREADED)", error))?;
        Ok(Self {
            _same_thread: PhantomData,
        })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// Owns one MTA and all COM interfaces used by one complete public operation.
/// Interface fields are declared before the apartment guard so they are
/// released before `CoUninitialize` during field destruction.
struct UiaOperation {
    _automation: IUIAutomation,
    root: IUIAutomationElement,
    walker: IUIAutomationTreeWalker,
    cache: IUIAutomationCacheRequest,
    _apartment: ComApartment,
}

impl UiaOperation {
    fn open(target: &WindowDescriptor) -> Result<Self, ComputerError> {
        let apartment = ComApartment::enter_mta()?;
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| uia_error("CoCreateInstance(CUIAutomation)", error))?;
        let hwnd = target
            .id
            .parse::<usize>()
            .map_err(|_| ComputerError::new("COMPUTER_STALE_FRAME", "Invalid target HWND"))?;
        let cache = unsafe { element_cache(&automation)? };
        let root =
            unsafe { automation.ElementFromHandleBuildCache(HWND(hwnd as *mut c_void), &cache) }
                .map_err(|error| uia_error("ElementFromHandleBuildCache", error))?;
        let provider_pid = unsafe { root.CachedProcessId() }
            .map_err(|error| uia_error("root CachedProcessId", error))?;
        if !provider_pid_matches(provider_pid, target.pid) {
            return Err(ComputerError::new(
                "COMPUTER_STALE_FRAME",
                "The UI Automation root no longer belongs to the captured window process",
            ));
        }
        let walker = unsafe { automation.ControlViewWalker() }
            .map_err(|error| uia_error("ControlViewWalker", error))?;
        Ok(Self {
            _automation: automation,
            root,
            walker,
            cache,
            _apartment: apartment,
        })
    }
}

unsafe fn element_cache(
    automation: &IUIAutomation,
) -> Result<IUIAutomationCacheRequest, ComputerError> {
    let cache = unsafe { automation.CreateCacheRequest() }
        .map_err(|error| uia_error("CreateCacheRequest", error))?;
    unsafe { cache.SetTreeScope(TreeScope_Element) }
        .map_err(|error| uia_error("CacheRequest.SetTreeScope(Element)", error))?;
    unsafe { cache.SetAutomationElementMode(AutomationElementMode_Full) }
        .map_err(|error| uia_error("CacheRequest.SetAutomationElementMode(Full)", error))?;
    for property in [
        UIA_AutomationIdPropertyId,
        UIA_BoundingRectanglePropertyId,
        UIA_ClassNamePropertyId,
        UIA_ControlTypePropertyId,
        UIA_FrameworkIdPropertyId,
        UIA_IsEnabledPropertyId,
        UIA_IsPasswordPropertyId,
        UIA_NamePropertyId,
        UIA_ProcessIdPropertyId,
        // Required for `CachedIsReadOnly`; without it every ValuePattern
        // element is filtered out and `setValue` targets are never observed.
        UIA_ValueIsReadOnlyPropertyId,
    ] {
        unsafe { cache.AddProperty(property) }
            .map_err(|error| uia_error("CacheRequest.AddProperty", error))?;
    }
    for pattern in [
        UIA_ExpandCollapsePatternId,
        UIA_InvokePatternId,
        UIA_SelectionItemPatternId,
        UIA_TogglePatternId,
        UIA_ValuePatternId,
    ] {
        unsafe { cache.AddPattern(pattern) }
            .map_err(|error| uia_error("CacheRequest.AddPattern", error))?;
    }
    Ok(cache)
}

pub fn snapshot(target: &WindowDescriptor) -> Result<SemanticSnapshot, ComputerError> {
    let operation = UiaOperation::open(target)?;
    snapshot_with(&operation)
}

fn snapshot_with(operation: &UiaOperation) -> Result<SemanticSnapshot, ComputerError> {
    let mut output = Vec::new();
    let mut path = Vec::new();
    let mut budget = WalkBudget::new();
    unsafe {
        walk_control_view(
            operation,
            &operation.root,
            &mut path,
            0,
            &mut budget,
            &mut output,
        );
    }
    Ok(SemanticSnapshot {
        elements: output,
        truncation_reason: budget.truncation_reason,
    })
}

pub fn invoke(
    target: &WindowDescriptor,
    semantic: &SemanticTarget,
    action: &str,
    cancellation: &CommandCancellation,
) -> Result<Value, ComputerError> {
    if action != "press" {
        return Err(ComputerError::new(
            "COMPUTER_INVALID_REQUEST",
            "Windows UI Automation currently advertises press and setValue",
        ));
    }

    let operation = UiaOperation::open(target)?;
    let element = unsafe { resolve_verified(&operation, semantic)? };

    // Prefer patterns with exact target-local state. A generic InvokePattern
    // acknowledges dispatch only and must never be promoted to confirmation by
    // unrelated changes elsewhere in the target window.
    if let Ok(pattern) = unsafe { element.GetCurrentPattern(UIA_TogglePatternId) } {
        let pattern = pattern
            .cast::<IUIAutomationTogglePattern>()
            .map_err(|error| uia_error("TogglePattern cast", error))?;
        let before = unsafe { pattern.CurrentToggleState() }
            .map_err(|error| uia_error("TogglePattern.CurrentToggleState", error))?;
        cancellation.begin_side_effect("UI Automation Toggle dispatch")?;
        unsafe { pattern.Toggle() }.map_err(|error| uia_error("TogglePattern.Toggle", error))?;
        cancellation.mark_verification_started();
        observe_delay(cancellation)?;
        let after = unsafe { pattern.CurrentToggleState() }
            .map_err(|error| uia_error("TogglePattern.CurrentToggleState read-back", error))?;
        return exact_state_result(after != before, "toggle-state-changed");
    }

    if let Ok(pattern) = unsafe { element.GetCurrentPattern(UIA_SelectionItemPatternId) } {
        let pattern = pattern
            .cast::<IUIAutomationSelectionItemPattern>()
            .map_err(|error| uia_error("SelectionItemPattern cast", error))?;
        let before = unsafe { pattern.CurrentIsSelected() }
            .map_err(|error| uia_error("SelectionItemPattern.CurrentIsSelected", error))?
            .as_bool();
        cancellation.begin_side_effect("UI Automation Select dispatch")?;
        unsafe { pattern.Select() }
            .map_err(|error| uia_error("SelectionItemPattern.Select", error))?;
        cancellation.mark_verification_started();
        observe_delay(cancellation)?;
        let after = unsafe { pattern.CurrentIsSelected() }
            .map_err(|error| uia_error("SelectionItemPattern.CurrentIsSelected read-back", error))?
            .as_bool();
        return exact_state_result(!before && after, "selection-state-changed");
    }

    if let Ok(pattern) = unsafe { element.GetCurrentPattern(UIA_ExpandCollapsePatternId) } {
        let pattern = pattern
            .cast::<IUIAutomationExpandCollapsePattern>()
            .map_err(|error| uia_error("ExpandCollapsePattern cast", error))?;
        let before = unsafe { pattern.CurrentExpandCollapseState() }.map_err(|error| {
            uia_error("ExpandCollapsePattern.CurrentExpandCollapseState", error)
        })?;
        let expected = if before == ExpandCollapseState_Collapsed {
            cancellation.begin_side_effect("UI Automation Expand dispatch")?;
            unsafe { pattern.Expand() }
                .map_err(|error| uia_error("ExpandCollapsePattern.Expand", error))?;
            cancellation.mark_verification_started();
            ExpandCollapseState_Expanded
        } else if before != ExpandCollapseState_LeafNode {
            cancellation.begin_side_effect("UI Automation Collapse dispatch")?;
            unsafe { pattern.Collapse() }
                .map_err(|error| uia_error("ExpandCollapsePattern.Collapse", error))?;
            cancellation.mark_verification_started();
            ExpandCollapseState_Collapsed
        } else {
            return Err(ComputerError::new(
                "COMPUTER_STALE_ELEMENT",
                "The UIA expand/collapse target is a leaf node and is no longer actionable",
            ));
        };
        observe_delay(cancellation)?;
        let after = unsafe { pattern.CurrentExpandCollapseState() }.map_err(|error| {
            uia_error(
                "ExpandCollapsePattern.CurrentExpandCollapseState read-back",
                error,
            )
        })?;
        return exact_state_result(
            after == expected && after != before,
            "expand-collapse-state-changed",
        );
    }

    if let Ok(pattern) = unsafe { element.GetCurrentPattern(UIA_InvokePatternId) } {
        let pattern = pattern
            .cast::<IUIAutomationInvokePattern>()
            .map_err(|error| uia_error("InvokePattern cast", error))?;
        cancellation.begin_side_effect("UI Automation Invoke dispatch")?;
        unsafe { pattern.Invoke() }.map_err(|error| uia_error("InvokePattern.Invoke", error))?;
        cancellation.mark_verification_started();
        cancellation.check("UI Automation Invoke dispatch observation")?;
        return Ok(json!({
            "delivered": true,
            "effectObserved": false,
        }));
    }

    Err(ComputerError::new(
        "COMPUTER_STALE_ELEMENT",
        "The UIA action pattern is no longer available",
    ))
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
    let operation = UiaOperation::open(target)?;
    let element = unsafe { resolve_verified(&operation, semantic)? };
    if unsafe { sensitive_uia_element(&element) } {
        return Err(sensitive_semantic());
    }
    let pattern = unsafe { element.GetCurrentPattern(UIA_ValuePatternId) }
        .map_err(|error| uia_error("ValuePattern lookup", error))?
        .cast::<IUIAutomationValuePattern>()
        .map_err(|error| uia_error("ValuePattern cast", error))?;
    let read_only = unsafe { pattern.CurrentIsReadOnly() }
        .map_err(|error| uia_error("ValuePattern.CurrentIsReadOnly", error))?
        .as_bool();
    if read_only {
        return Err(ComputerError::new(
            "COMPUTER_SEMANTIC_ACTION_FAILED",
            "The exact UI Automation target currently reports a read-only value",
        ));
    }
    let requested = BSTR::from(value);
    cancellation.begin_side_effect("UI Automation SetValue dispatch")?;
    unsafe { pattern.SetValue(&requested) }
        .map_err(|error| uia_error("ValuePattern.SetValue", error))?;
    cancellation.mark_verification_started();
    cancellation.check("UI Automation value observation")?;
    thread::sleep(Duration::from_millis(60));
    cancellation.check("UI Automation value observation")?;
    let current = unsafe { pattern.CurrentValue() }
        .map_err(|error| uia_error("ValuePattern.CurrentValue read-back", error))?
        .to_string();
    if current != value {
        return Err(ComputerError::new(
            "COMPUTER_POSTCONDITION_FAILED",
            "UIA accepted ValuePattern.SetValue but exact-target read-back did not match",
        ));
    }
    Ok(json!({
        "delivered": true,
        "effectObserved": true,
        "postcondition": "value-confirmed",
    }))
}

fn observe_delay(cancellation: &CommandCancellation) -> Result<(), ComputerError> {
    cancellation.check("UI Automation action observation")?;
    thread::sleep(Duration::from_millis(90));
    cancellation.check("UI Automation action observation")
}

fn exact_state_result(changed: bool, postcondition: &'static str) -> Result<Value, ComputerError> {
    Ok(if changed {
        json!({
            "delivered": true,
            "effectObserved": true,
            "postcondition": postcondition,
        })
    } else {
        json!({
            "delivered": true,
            "effectObserved": false,
            "postcondition": "no-observable-change",
        })
    })
}

unsafe fn walk_control_view(
    operation: &UiaOperation,
    element: &IUIAutomationElement,
    path: &mut Vec<usize>,
    depth: usize,
    budget: &mut WalkBudget,
    output: &mut Vec<SemanticTarget>,
) {
    if !budget.claim(depth, output.len()) {
        return;
    }

    let reference = format!("u{}", output.len() + 1);
    if let Some(element_info) = unsafe { signature(element, &reference) }
        && !element_info.actions.is_empty()
        && element_info.enabled != Some(false)
    {
        output.push(SemanticTarget {
            element: element_info,
            path: path.clone(),
        });
    }
    // Enforce the depth limit before asking an arbitrary provider for another
    // element. Reaching the boundary is reported conservatively because the
    // unqueried node may have children.
    if budget.exhausted(depth + 1, output.len()) {
        return;
    }

    let mut child = walker_element(
        unsafe {
            operation
                .walker
                .GetFirstChildElementBuildCache(element, &operation.cache)
        },
        budget,
    );
    let mut index = 0usize;
    while let Some(current) = child {
        if budget.truncation_reason.is_some() {
            break;
        }
        path.push(index);
        unsafe {
            walk_control_view(operation, &current, path, depth + 1, budget, output);
        }
        path.pop();
        if budget.truncation_reason.is_some() {
            break;
        }
        child = walker_element(
            unsafe {
                operation
                    .walker
                    .GetNextSiblingElementBuildCache(&current, &operation.cache)
            },
            budget,
        );
        index = index.saturating_add(1);
    }
}

/// The generated windows-rs wrapper represents UIA's successful null
/// end-of-tree result as an empty error with a success HRESULT. Real provider
/// failures have a failing HRESULT and mean the partial tree is not complete.
fn walker_element(
    result: windows::core::Result<IUIAutomationElement>,
    budget: &mut WalkBudget,
) -> Option<IUIAutomationElement> {
    match result {
        Ok(element) => Some(element),
        Err(error) if error.code().is_ok() => None,
        Err(_) => {
            budget.mark(SemanticTruncationReason::ProviderError);
            None
        }
    }
}

unsafe fn resolve_verified(
    operation: &UiaOperation,
    semantic: &SemanticTarget,
) -> Result<IUIAutomationElement, ComputerError> {
    let element = unsafe { resolve_path(operation, &semantic.path) }.ok_or_else(|| {
        ComputerError::new(
            "COMPUTER_STALE_ELEMENT",
            "The bounded UIA Control View path no longer resolves. Observe again.",
        )
    })?;
    let current = unsafe { signature(&element, &semantic.element.reference) };
    if current.as_ref() != Some(&semantic.element) {
        return Err(ComputerError::new(
            "COMPUTER_STALE_ELEMENT",
            "The UIA provider identity or semantic state changed since capture. Observe again.",
        ));
    }
    Ok(element)
}

unsafe fn resolve_path(operation: &UiaOperation, path: &[usize]) -> Option<IUIAutomationElement> {
    if path.len() > MAX_DEPTH {
        return None;
    }
    let started = Instant::now();
    let mut visited = 1usize;
    let mut current = operation.root.clone();
    for wanted_index in path {
        if visited >= MAX_VISITED_NODES || started.elapsed() >= WALK_DEADLINE {
            return None;
        }
        let mut candidate = unsafe {
            operation
                .walker
                .GetFirstChildElementBuildCache(&current, &operation.cache)
        }
        .ok()?;
        visited += 1;
        for _ in 0..*wanted_index {
            if visited >= MAX_VISITED_NODES || started.elapsed() >= WALK_DEADLINE {
                return None;
            }
            candidate = unsafe {
                operation
                    .walker
                    .GetNextSiblingElementBuildCache(&candidate, &operation.cache)
            }
            .ok()?;
            visited += 1;
        }
        current = candidate;
    }
    Some(current)
}

unsafe fn signature(element: &IUIAutomationElement, reference: &str) -> Option<SemanticElement> {
    let has_invoke = unsafe { element.GetCachedPattern(UIA_InvokePatternId) }.is_ok();
    let has_toggle = unsafe { element.GetCachedPattern(UIA_TogglePatternId) }.is_ok();
    let has_selection = unsafe { element.GetCachedPattern(UIA_SelectionItemPatternId) }.is_ok();
    let has_expand_collapse =
        unsafe { element.GetCachedPattern(UIA_ExpandCollapsePatternId) }.is_ok();
    let mut actions = Vec::new();
    if has_invoke || has_toggle || has_selection || has_expand_collapse {
        actions.push("press".to_owned());
    }

    let sensitive = unsafe { cached_sensitive_uia_element(element) };
    let value_pattern = (!sensitive)
        .then(|| unsafe { element.GetCachedPattern(UIA_ValuePatternId) })
        .and_then(Result::ok)
        .and_then(|pattern| pattern.cast::<IUIAutomationValuePattern>().ok())
        .filter(|pattern| {
            unsafe { pattern.CachedIsReadOnly() }
                .ok()
                .is_some_and(|read_only| !read_only.as_bool())
        });
    if value_pattern.is_some() {
        actions.push("setValue".to_owned());
    }
    if actions.is_empty() {
        return None;
    }

    let control_type = unsafe { element.CachedControlType().ok()? };
    let native_identity = NativeElementIdentity {
        automation_id: unsafe { element.CachedAutomationId().ok()? }.to_string(),
        class_name: unsafe { element.CachedClassName().ok()? }.to_string(),
        framework_id: unsafe { element.CachedFrameworkId().ok()? }.to_string(),
        control_type: control_type.0,
        provider_process_id: unsafe { element.CachedProcessId().ok()? },
        capability_mask: u8::from(has_invoke)
            | (u8::from(has_toggle) << 1)
            | (u8::from(has_selection) << 2)
            | (u8::from(has_expand_collapse) << 3)
            | (u8::from(value_pattern.is_some()) << 4),
    };
    let name = unsafe { element.CachedName().ok()?.to_string() };
    let enabled = unsafe { element.CachedIsEnabled().ok() }.map(|enabled| enabled.as_bool());
    let rectangle = unsafe { element.CachedBoundingRectangle().ok() };
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
    let value = value_pattern
        .as_ref()
        .and_then(|pattern| unsafe { pattern.CachedValue().ok() })
        .map(|value| value.to_string());
    actions.sort();
    actions.dedup();
    Some(SemanticElement {
        reference: reference.to_owned(),
        role: format!("UIAControlType({})", control_type.0),
        name,
        value,
        sensitive,
        value_redacted: sensitive,
        enabled,
        actions,
        bounds,
        coordinate_space: "screen-pixels".to_owned(),
        screen_bounds: None,
        native_identity: Some(native_identity),
    })
}

unsafe fn sensitive_uia_element(element: &IUIAutomationElement) -> bool {
    fail_closed_password_state(
        unsafe { element.CurrentIsPassword() }
            .ok()
            .map(|password| password.as_bool()),
    )
}

unsafe fn cached_sensitive_uia_element(element: &IUIAutomationElement) -> bool {
    fail_closed_password_state(
        unsafe { element.CachedIsPassword() }
            .ok()
            .map(|password| password.as_bool()),
    )
}

fn fail_closed_password_state(password: Option<bool>) -> bool {
    password.unwrap_or(true)
}

fn provider_pid_matches(provider_pid: i32, target_pid: u32) -> bool {
    provider_pid >= 0 && provider_pid as u32 == target_pid
}

fn sensitive_semantic() -> ComputerError {
    ComputerError::new(
        "COMPUTER_SENSITIVE_ELEMENT",
        "Sensitive UI Automation values are redacted and cannot be read or set",
    )
}

fn uia_error(operation: &str, error: impl std::fmt::Display) -> ComputerError {
    ComputerError::new(
        "COMPUTER_SEMANTIC_UNAVAILABLE",
        format!("Windows UI Automation {operation} failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_state_errors_fail_closed() {
        assert!(!fail_closed_password_state(Some(false)));
        assert!(fail_closed_password_state(Some(true)));
        assert!(fail_closed_password_state(None));
    }

    #[test]
    fn provider_process_identity_rejects_negative_and_reused_roots() {
        assert!(provider_pid_matches(42, 42));
        assert!(!provider_pid_matches(-1, u32::MAX));
        assert!(!provider_pid_matches(7, 42));
    }

    #[test]
    fn walk_budget_has_explicit_node_depth_and_actionable_limits() {
        let mut budget = WalkBudget::new();
        assert!(budget.claim(0, 0));
        budget.visited = MAX_VISITED_NODES;
        assert!(!budget.claim(0, 0));
        assert_eq!(
            budget.truncation_reason,
            Some(SemanticTruncationReason::NodeBudget)
        );

        let mut budget = WalkBudget::new();
        assert!(!budget.claim(MAX_DEPTH + 1, 0));
        assert_eq!(
            budget.truncation_reason,
            Some(SemanticTruncationReason::DepthBudget)
        );

        let mut budget = WalkBudget::new();
        assert!(budget.claim(MAX_DEPTH, 0));
        let visited_at_boundary = budget.visited;
        assert!(budget.exhausted(MAX_DEPTH + 1, 0));
        assert_eq!(budget.visited, visited_at_boundary);
        assert_eq!(
            budget.truncation_reason,
            Some(SemanticTruncationReason::DepthBudget)
        );

        let mut budget = WalkBudget::new();
        assert!(!budget.claim(0, MAX_ACTIONABLE));
        assert_eq!(
            budget.truncation_reason,
            Some(SemanticTruncationReason::ActionableBudget)
        );

        let mut budget = WalkBudget::new();
        budget.started = Instant::now()
            .checked_sub(WALK_DEADLINE)
            .expect("the test deadline fits in Instant");
        assert!(!budget.claim(0, 0));
        assert_eq!(
            budget.truncation_reason,
            Some(SemanticTruncationReason::Deadline)
        );
    }

    #[test]
    fn semantic_truncation_reasons_have_a_closed_protocol_vocabulary() {
        assert_eq!(SemanticTruncationReason::NodeBudget.as_str(), "node_budget");
        assert_eq!(
            SemanticTruncationReason::DepthBudget.as_str(),
            "depth_budget"
        );
        assert_eq!(
            SemanticTruncationReason::ActionableBudget.as_str(),
            "actionable_budget"
        );
        assert_eq!(SemanticTruncationReason::Deadline.as_str(), "deadline");
        assert_eq!(
            SemanticTruncationReason::ProviderError.as_str(),
            "provider_error"
        );
    }
}
