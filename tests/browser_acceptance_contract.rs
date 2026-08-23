use local_browser_bridge::server::ACTION_METHODS;
use std::fs;

fn script() -> String {
    fs::read_to_string("scripts/test-windows-browser-api.ps1")
        .unwrap_or_else(|error| panic!("read browser API acceptance driver: {error}"))
        .replace("\r\n", "\n")
}

fn powershell_array(source: &str, variable: &str) -> Vec<String> {
    let opening = format!("${variable} = @(");
    let mut lines = source
        .lines()
        .skip_while(|line| line.trim() != opening)
        .skip(1);
    let mut values = Vec::new();
    for line in &mut lines {
        let line = line.trim();
        if line == ")" {
            break;
        }
        let value = line
            .trim_end_matches(',')
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or_else(|| panic!("non-canonical ${variable} entry: {line}"));
        values.push(value.to_owned());
    }
    values
}

#[test]
fn browser_matrix_tracks_the_exact_advertised_method_order() {
    let source = script();
    let methods = powershell_array(&source, "ActionMethods");
    assert_eq!(methods, ACTION_METHODS);
    assert_eq!(methods.len(), 25);
    assert_eq!(
        powershell_array(&source, "AssertionNames"),
        [
            "serverVersionMatched",
            "extensionVersionMatched",
            "browserFloorMet",
            "realExtensionConnected",
            "fullAccessEnabled",
            "capabilitiesComplete",
            "freshCommandIdentity",
            "freshObservationAfterPageMutation",
            "dynamicTargetDiscovery",
            "testOwnedTabsOnly",
            "topLayerControlUiIntegrity",
            "dialogLifecycle",
            "cleanupComplete",
        ]
    );

    for method in ACTION_METHODS {
        assert!(
            source.contains(&format!("Confirm-MethodPassed \"{method}\"")),
            "acceptance driver does not verify {method}"
        );
    }
}

#[test]
fn browser_matrix_uses_process_only_credentials_and_fresh_command_identities() {
    let source = script();
    for required in [
        "GetEnvironmentVariable(\"LBB_TOKEN\", \"Process\")",
        "SetEnvironmentVariable(\"LBB_TOKEN\", $null, \"Process\")",
        "Remove-Item Env:LBB_TOKEN -ErrorAction SilentlyContinue",
        "[Guid]::NewGuid().ToString(\"N\")",
        "$script:SeenCallIds.Add($callId)",
        "$body.callId -ceq $callId",
        "command-not-replayed",
    ] {
        assert!(
            source.contains(required),
            "missing credential/replay boundary: {required}"
        );
    }
    for forbidden in [
        "[string]$Token",
        "[SecureString]",
        "Get-Credential",
        "Read-Host",
        "-Token ",
    ] {
        assert!(
            !source.contains(forbidden),
            "credential can enter through a forbidden surface: {forbidden}"
        );
    }
}

#[test]
fn browser_matrix_is_test_owned_loopback_only_and_reobserves_mutations() {
    let source = script();
    for required in [
        "$script:OwnedTabs.Contains($tabId)",
        "[int]$Port = 17373",
        "$createdDemo = Invoke-BridgeCommand \"tabs.new\" ([ordered]@{ url = $demoUrl })",
        "bridge-demo-created",
        "single-created-demo-tab",
        "created-demo-url-reconciled",
        "$demoDeadline = [DateTime]::UtcNow.AddSeconds(12)",
        "Assert-Acceptance ($newDemoTabs.Count -le 1)",
        "[void]$script:OwnedTabs.Add($targetTabId)",
        "[void]$script:ClosableTabs.Add($targetTabId)",
        "[void]$script:ClosableTabs.Add($blankTabId)",
        "$createdBlank = Invoke-BridgeCommand \"tabs.new\" ([ordered]@{})",
        "created-tab-about-blank",
        "$demoUrl = \"http://127.0.0.1:$Port/demo\"",
        "dynamic-demo-tab-discovered",
        "Register-PageMutation",
        "Get-PageMutationObservation",
        "$script:PageMutationCount -eq $script:ObservedPageMutationCount",
        "observation-generation-fresh",
        "observation-ref-generation-bound",
        "observation-viewport-finite",
        "observation-screenshot-bytes-bounded",
        "observation-screenshot-generation-binding",
        "all-page-mutations-reobserved",
        "$initialControlHostIdentity = Get-PublicControlHostIdentity $targetTabId",
        "navigate-control-host-rotated",
        "reload-control-host-rotated",
        "$snapshotExclusionScheduled = Invoke-BridgeCommand \"page.evaluate\"",
        "__lbbSnapshotExclusionAttack",
        "snapshot-exclusion-duplicate-id",
        "snapshot-exclusion-display-contents",
        "Invoke-BridgeCommandExpectError",
        "-ExpectedHttpStatus 409",
        "-ExpectedErrorCode \"STALE_SNAPSHOT\"",
        "-ExpectedTaxonomyCode \"stale_snapshot\"",
        "-ExpectedRecoveryHint \"reobserve\"",
        "snapshot-exclusion-fresh-action-observed",
        "@(\"attackApplied\", \"fakeRemoved\", \"formRestored\", \"actionRestored\")",
        "Wait-UntilEpochMilliseconds ($snapshotExclusionTiming.AttackAt + 250)",
        "$script:OwnedTargetHandoff = [pscustomobject][ordered]@{",
        "focusedByExactOwnedTabActivation = $true",
        "owned-handoff-active-id",
        "owned-handoff-single-active-target",
        "Write-Output ($script:OwnedTargetHandoff | ConvertTo-Json -Depth 3 -Compress)",
        "Get-VisibleObservedElement $targetTabId $observation \"select\" \"Favorite color\"",
        "bottom-marker-visible",
    ] {
        assert!(
            source.contains(required),
            "missing ownership/freshness proof: {required}"
        );
    }
    assert_eq!(
        source
            .matches("Invoke-BridgeCommand \"page.navigate\"")
            .count(),
        2,
        "the matrix must retain exactly its two owned loopback navigations"
    );
    assert!(source.contains("$historyUrl = \"$demoUrl`?step=2\""));
    assert!(source.contains("url = $demoUrl"));
    assert!(source.contains("url = $historyUrl"));
    for forbidden in ["url = \"https://", "url = 'https://", "url = \"http://"] {
        assert!(
            !source.contains(forbidden),
            "the matrix must never navigate to a literal non-owned URL: {forbidden}"
        );
    }
    assert!(
        !source.contains("tabId = 1"),
        "the matrix must never hard-code a Chrome tab id"
    );
    assert!(
        !source.contains("Snapshot exclusion mutation active"),
        "the exact-object negative must not add an unrelated marker mutation"
    );
    assert!(
        !source.contains("Assert-DedicatedDemoTab $targetTabId $historyUrl"),
        "tab inventory strips query strings; route-step proof must use page.waitFor"
    );
    assert!(
        !source
            .contains("Invoke-BridgeCommand \"tabs.close\" ([ordered]@{ tabId = $targetTabId })"),
        "the created demo must remain for the downstream visual evidence flow"
    );
    assert!(
        !source.contains("Invoke-BridgeCommand \"browser.control.start\" ([ordered]@{\n        tabId = $blankTabId"),
        "top-level about:blank must never be used as a browser-control target"
    );

    assert!(!source.contains("ConfirmPreopenedDemoIsTestOwned"));
    assert!(!source.contains("baselineIds"));

    let extension_checked = source
        .find("$AggregateAssertions[\"capabilitiesComplete\"] = $true")
        .unwrap();
    let demo_created = source
        .find("$createdDemo = Invoke-BridgeCommand \"tabs.new\"")
        .unwrap();
    let demo_listed = source
        .find("$afterDemoCreate = Invoke-BridgeCommand \"tabs.list\"")
        .unwrap();
    let demo_owned = source.find("$script:OwnedTabs.Add($targetTabId)").unwrap();
    let demo_postcondition = source.find("\"bridge-demo-created\"").unwrap();
    let first_control = source
        .find("$started = Invoke-BridgeCommand \"browser.control.start\"")
        .unwrap();
    let blank_created = source
        .find("$createdBlank = Invoke-BridgeCommand \"tabs.new\"")
        .unwrap();
    let blank_owned = source.find("$script:OwnedTabs.Add($blankTabId)").unwrap();
    let blank_postcondition = source.find("\"bridge-blank-created\"").unwrap();
    let blank_activated = source
        .find("$blankActivated = Invoke-BridgeCommand \"tabs.activate\"")
        .unwrap();
    let blank_closed = source
        .find("$blankClosed = Invoke-BridgeCommand \"tabs.close\"")
        .unwrap();
    let reacquired = source.find("demo-control-reacquired").unwrap();
    assert!(extension_checked < demo_created);
    assert!(demo_created < demo_owned && demo_owned < demo_postcondition);
    assert!(demo_created < demo_listed && demo_listed < first_control);
    assert!(first_control < blank_created);
    assert!(blank_created < blank_owned && blank_owned < blank_postcondition);
    assert!(blank_created < blank_activated && blank_activated < blank_closed);
    assert!(blank_closed < reacquired);
}

#[test]
fn browser_matrix_exercises_dialogs_without_serial_api_deadlock() {
    let source = script();
    for required in [
        "setTimeout(function () { confirm(\"bridge acceptance dialog\"); }, 100)",
        "if ($null -ne $dialogStatus.state.pendingDialog)",
        "Invoke-BridgeCommand \"page.handleDialog\"",
        "accept = $false",
        "dialog-state-cleared",
        "$AggregateAssertions[\"dialogLifecycle\"] = $true",
    ] {
        assert!(
            source.contains(required),
            "missing real dialog lifecycle proof: {required}"
        );
    }
}

#[test]
fn browser_matrix_proves_top_layer_retop_and_fail_closed_occlusion() {
    let source = script();
    let demo = fs::read_to_string("public/demo.html").unwrap();
    let css = fs::read_to_string("public/demo.css").unwrap();
    let demo_script = fs::read_to_string("public/demo.js").unwrap();

    for required in [
        "id=\"top-layer-control-fixture\"",
        "id=\"top-layer-late-occluder\"",
        "id=\"top-layer-perpetual-occluder\"",
        "id=\"top-layer-child-frame\"",
        "id=\"stop-guard-fixture\"",
        "popover=\"manual\"",
        "Page top-layer acceptance fixture",
        "Hit-testable page occluder",
        "Perpetual passive page occluder",
        "Benign child-document popover",
    ] {
        assert!(
            demo.contains(required),
            "demo fixture is missing {required}"
        );
    }
    for required in [
        "[popover] {",
        "opacity: 0 !important",
        "filter: opacity(0) !important",
        "mask-image: linear-gradient(transparent, transparent) !important",
        "transform: scale(0) !important",
        "[popover]::before",
        "[popover]::after",
        "content: \"\" !important",
        "background: #000 !important",
        "[popover]::backdrop",
        "#top-layer-control-fixture",
        "#top-layer-late-occluder",
        "#top-layer-perpetual-occluder",
        ".top-layer-sparse-occluder",
        ".top-layer-host-forgery",
        "#top-layer-control-fixture::before",
        "#top-layer-late-occluder::after",
        "#top-layer-control-fixture::backdrop",
        "content: none !important",
        "background: transparent !important",
        "width: 100vw",
        "height: 100vh",
        "pointer-events: auto",
        ".pass-through",
        "pointer-events: none",
        "::view-transition-old(root)",
        "animation-duration: 8s !important",
    ] {
        assert!(
            css.contains(required),
            "demo fixture CSS is missing {required}"
        );
    }
    for required in [
        "hostileStopCaptureListeners = \"armed\"",
        "window.addEventListener(type, blockLateControlActivation, true)",
        "document.addEventListener(type, blockLateControlActivation, true)",
        "event.stopImmediatePropagation()",
        "[\"pointerdown\", \"click\", \"keydown\"]",
    ] {
        assert!(
            demo_script.contains(required),
            "demo hostile Stop fixture is missing {required}"
        );
    }
    for required in [
        "$hostileStyleState = Invoke-BridgeCommand \"page.evaluate\"",
        "getComputedStyle(host)",
        "getComputedStyle(host, '::before')",
        "getComputedStyle(host, '::after')",
        "getComputedStyle(host, '::backdrop')",
        "Number(hostStyle.opacity) === 1",
        "hostStyle.filter === 'none'",
        "hostStyle.maskImage === 'none'",
        "hostStyle.transform === 'none'",
        "style.content === 'none' && style.display === 'none'",
        "backdrop.backgroundImage === 'none'",
        "noneIfExposed(backdrop.backdropFilter)",
        "noneIfExposed(backdrop.maskImage)",
        "noneIfExposed(backdrop.clipPath)",
        "noneIfExposed(backdrop.transform)",
        "backdrop.pointerEvents === 'none'",
        "hostile-host-paint-safe",
        "hostile-before-suppressed",
        "hostile-after-suppressed",
        "hostile-backdrop-safe-if-exposed",
        "hostile-stop-capture-listeners-armed",
        "$cleanRootShow = Invoke-BridgeCommand \"page.evaluate\"",
        "host.parentNode === root && host.parentElement === root",
        "clean-root-event-control-active",
        "clean-root-event-control-clean",
        "$childPopoverOpened = Invoke-BridgeCommand \"page.evaluate\"",
        "top-layer-child-frame",
        "child-popover-did-not-falsely-revoke",
        "$childPopoverSwarmOpened = Invoke-BridgeCommand \"page.evaluate\"",
        "const count = 33",
        "aboveLegacyBound: count > 32",
        "child-popover-swarm-open",
        "$childPopoverSwarmSurvived = Invoke-BridgeCommand \"page.evaluate\"",
        "child-popover-swarm-did-not-falsely-revoke",
        "child-popover-swarm-removed",
        "$fixtureOpened = Invoke-BridgeCommand \"page.evaluate\"",
        "fixture.showPopover()",
        "page-popover-passive",
        "$fixtureRetopped = Invoke-BridgeCommand \"page.evaluate\"",
        "controlRetopped",
        "fixture.classList.add('pass-through')",
        "$lateOccluderScheduled = Invoke-BridgeCommand \"page.evaluate\"",
        "post-evaluate observation/capture bound",
        "occluder.showPopover()",
        "Assert-ControlActiveBeforeFixture $lateTiming.AttackAt",
        "late-occluder-bounded-revocation",
        "$perpetualScheduled = Invoke-BridgeCommand \"page.evaluate\"",
        "__lbbPerpetualTopLayerAttack",
        "perpetual-watchdog-revocation-bound",
        "$sparseOccluderScheduled = Invoke-BridgeCommand \"page.evaluate\"",
        "__lbbSparseTopLayerAttack",
        "stablePointRowsAvoided",
        "sparse-occluder-avoids-stable-point-rows",
        "sparse-occluder-bounded-revocation",
        "$fakeHostScheduled = Invoke-BridgeCommand \"page.evaluate\"",
        "__lbbFakeControlHostAttack",
        "fake-host-bounded-revocation",
        "$duplicateIdScheduled = Invoke-BridgeCommand \"page.evaluate\"",
        "__lbbDuplicateIdTopLayerAttack",
        "duplicate-id-simultaneous",
        "-Label \"duplicate-id\"",
        "duplicate-id-cleanup",
        "$viewTransitionScheduled = Invoke-BridgeCommand \"page.evaluate\"",
        "document.startViewTransition",
        "view-transition-bounded-revocation",
        "$accessibilityScheduled = Invoke-BridgeCommand \"page.evaluate\"",
        "host.setAttribute('aria-hidden', 'true')",
        "host.setAttribute('hidden', '')",
        "accessibility-bounded-revocation",
        "__lbbHostAccessibilityAttack",
        "accessibility-cleanup-restored",
        "$rootAccessibilityScheduled = Invoke-BridgeCommand \"page.evaluate\"",
        "__lbbRootAccessibilityAttack",
        "root.setAttribute('aria-hidden', 'true')",
        "root.setAttribute('hidden', '')",
        "root.setAttribute('inert', '')",
        "-Label \"root-accessibility\"",
        "@(\"attackApplied\", \"restored\", \"freshHostDirect\", \"freshRootAccessible\")",
        "foreach ($wrapperMode in @(\"light\", \"open\", \"closed\"))",
        "wrapper.attachShadow({ mode })",
        "wrapper.id = host.id",
        "host.parentNode !== root",
        "-Label \"direct-parent-$wrapperMode\"",
        "@(\"attackApplied\", \"publicIdentityCopied\", \"wrapperRemoved\", \"hostRestoredOrRetired\", \"freshHostDirect\", \"noWrapperSubstitution\")",
        "$mutationStallArmed = Invoke-BridgeCommand \"page.evaluate\"",
        "__lbbMutationDepthStallAttack",
        "host.addEventListener('beforetoggle', onBeforeToggle)",
        "queueMicrotask(() =>",
        "while (performance.now() - stallStarted < stallMs)",
        "mutation-stall-authoritative-order",
        "mutation-stall-independent-deadline",
        "mutation-stall-renderer-duration",
        "-DeadlineEpochMilliseconds ($perpetualTiming.AttackAt + 3000)",
        "Restart-DemoControlAfterFixture",
        "$captureRace = [Diagnostics.Stopwatch]::StartNew()",
        "capture-watchdog-control-active",
        "$observation = Get-PageMutationObservation $targetTabId",
        "$AggregateAssertions[\"topLayerControlUiIntegrity\"] = $true",
    ] {
        assert!(
            source.contains(required),
            "top-layer proof is missing {required}"
        );
    }

    let hostile = source.find("$hostileStyleState =").unwrap();
    let clean_root = source.find("$cleanRootShow =").unwrap();
    let child = source.find("$childPopoverOpened =").unwrap();
    let child_swarm = source.find("$childPopoverSwarmOpened =").unwrap();
    let opened = source.find("$fixtureOpened =").unwrap();
    let retopped = source.find("$fixtureRetopped =").unwrap();
    let occluder = source.find("$lateOccluderScheduled =").unwrap();
    let perpetual = source.find("$perpetualScheduled =").unwrap();
    let sparse = source.find("$sparseOccluderScheduled =").unwrap();
    let fake = source.find("$fakeHostScheduled =").unwrap();
    let duplicate = source.find("$duplicateIdScheduled =").unwrap();
    let view_transition = source.find("$viewTransitionScheduled =").unwrap();
    let accessibility = source.find("$accessibilityScheduled =").unwrap();
    let root_accessibility = source.find("$rootAccessibilityScheduled =").unwrap();
    let wrappers = source.find("foreach ($wrapperMode in").unwrap();
    let mutation_stall = source.find("$mutationStallArmed =").unwrap();
    let capture = source.find("$captureRace =").unwrap();
    let aggregate = source
        .find("$AggregateAssertions[\"topLayerControlUiIntegrity\"] = $true")
        .unwrap();
    assert!(
        hostile < clean_root
            && clean_root < child
            && child < child_swarm
            && child_swarm < opened
            && opened < retopped
            && retopped < occluder
    );
    assert!(
        occluder < perpetual
            && perpetual < sparse
            && sparse < fake
            && fake < duplicate
            && duplicate < view_transition
            && view_transition < accessibility
            && accessibility < root_accessibility
            && root_accessibility < wrappers
            && wrappers < mutation_stall
            && mutation_stall < capture
            && capture < aggregate
    );
}

#[test]
fn browser_matrix_record_is_reduced_allowlisted_and_append_never() {
    let source = script();
    for required in [
        "schemaVersion = 1",
        "evidenceType  = \"stock-user-chrome-api-matrix\"",
        "target        = \"loopback-demo\"",
        "candidateBinding = $script:CandidateBinding",
        "Get-CandidateBindingFromPreflight",
        "PreflightRecord must name the exact candidate preflight record.",
        "PassThruOwnedTarget is required",
        "methodCount   = 25",
        "\"name\", \"passed\", \"stage\", \"commandInvoked\", \"resultVerified\"",
        "\"postconditionVerified\", \"screenshot\", \"machineProof\"",
        "$MethodCommandInvoked[$Method] = $true",
        "$MethodResultVerified[$Method] = $true",
        "$MethodPostconditionVerified[$Method] = $true",
        "machine-command-result-postcondition",
        "browser-06-action-result.png",
        "\"page.observe\"           = \"N/A\"",
        "record-forbidden-key",
        "record-forbidden-value",
        "[IO.FileMode]::CreateNew",
        "acceptance evidence is append-never",
    ] {
        assert!(
            source.contains(required),
            "missing reduced-record contract: {required}"
        );
    }
    assert!(source.contains("[int]$browserVersion.result.value -ge 140"));

    for negative_self_test in [
        "self-test-token-rejected",
        "self-test-path-rejected",
        "self-test-identifier-rejected",
        "self-test-url-rejected",
        "self-test-other-candidate-rejected",
        "self-test-postcondition-rejected",
        "self-test-screenshot-path-rejected",
    ] {
        assert!(
            source.contains(negative_self_test),
            "missing reduced-record negative test: {negative_self_test}"
        );
    }
}

#[test]
fn browser_matrix_cleanup_never_broadens_beyond_owned_state() {
    let source = script();
    for required in [
        "if ($script:ControlMayBeActive",
        "foreach ($ownedTabId in @($script:ClosableTabs))",
        "Invoke-BridgeCommand \"tabs.close\" ([ordered]@{ tabId = [long]$ownedTabId })",
        "demo-tab-retained-for-visual-evidence",
        "Wait-ForReleasedControl \"released_by_client\"",
        "$control.revocationPending -eq $false",
        "[string]$revocation.reason -ceq $ExpectedReason",
        "[long]$revocation.at",
        "-ReturnRevocationAt",
        "[void]$script:ClosableTabs.Remove($targetTabId)",
        "$AggregateAssertions[\"cleanupComplete\"] = $cleanupSucceeded",
        "$cleanupInventory = Invoke-BridgeCommand \"tabs.list\"",
        "if ($stillPresent)",
    ] {
        assert!(
            source.contains(required),
            "missing bounded cleanup: {required}"
        );
    }
    for forbidden in [
        "Stop-Process",
        "taskkill",
        "Remove-Item -Recurse",
        "chrome.exe",
    ] {
        assert!(
            !source.contains(forbidden),
            "cleanup contains broad operation: {forbidden}"
        );
    }
}
