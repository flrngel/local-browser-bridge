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
        "ConfirmPreopenedDemoIsTestOwned",
        "single-preopened-demo-tab",
        "[void]$script:OwnedTabs.Add($targetTabId)",
        "[void]$script:ClosableTabs.Add($blankTabId)",
        "created-tab-about-blank",
        "$demoUrl = \"http://127.0.0.1:$Port/demo\"",
        "dynamic-demo-tab-discovered",
        "Register-PageMutation",
        "Get-PageMutationObservation",
        "$script:PageMutationCount -eq $script:ObservedPageMutationCount",
        "observation-generation-fresh",
        "observation-ref-generation-bound",
        "all-page-mutations-reobserved",
        "Get-ObservedElement $observation \"select\" \"Favorite color\"",
    ] {
        assert!(
            source.contains(required),
            "missing ownership/freshness proof: {required}"
        );
    }
    assert!(
        !source.contains("https://"),
        "the matrix must never navigate off loopback"
    );
    assert!(
        !source.contains("tabId = 1"),
        "the matrix must never hard-code a Chrome tab id"
    );
    assert!(
        !source.contains("Assert-DedicatedDemoTab $targetTabId $historyUrl"),
        "tab inventory strips query strings; route-step proof must use page.waitFor"
    );
    assert!(
        !source
            .contains("Invoke-BridgeCommand \"tabs.close\" ([ordered]@{ tabId = $targetTabId })"),
        "the pre-opened demo must remain for the downstream visual evidence flow"
    );
    assert!(
        !source.contains("Invoke-BridgeCommand \"browser.control.start\" ([ordered]@{\n        tabId = $blankTabId"),
        "top-level about:blank must never be used as a browser-control target"
    );

    let discovered = source.find("single-preopened-demo-tab").unwrap();
    let first_control = source
        .find("$started = Invoke-BridgeCommand \"browser.control.start\"")
        .unwrap();
    let blank_created = source
        .find("$created = Invoke-BridgeCommand \"tabs.new\"")
        .unwrap();
    let blank_activated = source
        .find("$blankActivated = Invoke-BridgeCommand \"tabs.activate\"")
        .unwrap();
    let blank_closed = source
        .find("$blankClosed = Invoke-BridgeCommand \"tabs.close\"")
        .unwrap();
    let reacquired = source.find("demo-control-reacquired").unwrap();
    assert!(discovered < first_control);
    assert!(first_control < blank_created);
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
fn browser_matrix_record_is_reduced_allowlisted_and_append_never() {
    let source = script();
    for required in [
        "schemaVersion = 1",
        "evidenceType  = \"stock-user-chrome-api-matrix\"",
        "target        = \"loopback-demo\"",
        "methodCount   = 25",
        "Assert-ExactPropertyOrder $entry @(\"name\", \"passed\", \"stage\")",
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
        "$AggregateAssertions[\"cleanupComplete\"] = $cleanupSucceeded",
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
