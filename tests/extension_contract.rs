use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use local_browser_bridge::VERSION;
use local_browser_bridge::server::ACTION_METHODS;
use serde_json::Value;

const EXTENSION_FILES: &[&str] = &[
    "background.js",
    "content.js",
    "lib.js",
    "manifest.json",
    "popup.css",
    "popup.html",
    "popup.js",
];

fn manifest() -> Value {
    serde_json::from_str(&fs::read_to_string("extension/manifest.json").unwrap()).unwrap()
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn quoted_strings(source: &str) -> BTreeSet<String> {
    let mut strings = BTreeSet::new();
    let mut quote = None;
    let mut current = String::new();
    let mut escaped = false;
    for character in source.chars() {
        if let Some(delimiter) = quote {
            if escaped {
                current.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == delimiter {
                strings.insert(std::mem::take(&mut current));
                quote = None;
            } else {
                current.push(character);
            }
        } else if character == '"' || character == '\'' {
            quote = Some(character);
        }
    }
    strings
}

#[test]
fn manifest_has_only_reviewed_capabilities() {
    let manifest = manifest();
    assert_eq!(manifest["manifest_version"], 3);
    assert_eq!(manifest["version"], VERSION);
    assert_eq!(manifest["minimum_chrome_version"], "116");
    assert_eq!(
        strings(&manifest["permissions"]),
        BTreeSet::from_iter(
            ["alarms", "debugger", "scripting", "storage", "tabs"].map(str::to_owned)
        )
    );
    assert_eq!(
        strings(&manifest["host_permissions"]),
        BTreeSet::from_iter(["file://*/*", "http://*/*", "https://*/*"].map(str::to_owned))
    );
    for forbidden in [
        "cookies",
        "downloads",
        "management",
        "nativeMessaging",
        "proxy",
        "webRequest",
    ] {
        assert!(!strings(&manifest["permissions"]).contains(forbidden));
    }
    assert!(manifest.get("externally_connectable").is_none());
    assert_eq!(
        manifest["content_security_policy"]["extension_pages"],
        "script-src 'self'; object-src 'none'"
    );
}

#[test]
fn package_contains_only_declared_local_assets() {
    let actual = fs::read_dir("extension")
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual,
        EXTENSION_FILES
            .iter()
            .map(|file| (*file).to_owned())
            .collect()
    );

    let manifest = manifest();
    for path in [
        manifest["background"]["service_worker"].as_str().unwrap(),
        manifest["action"]["default_popup"].as_str().unwrap(),
        manifest["content_scripts"][0]["js"][0].as_str().unwrap(),
    ] {
        assert!(
            Path::new("extension").join(path).is_file(),
            "missing {path}"
        );
    }
    let popup = fs::read_to_string("extension/popup.html").unwrap();
    assert!(popup.contains("href=\"popup.css\""));
    assert!(popup.contains("src=\"popup.js\""));
}

#[test]
fn extension_executes_no_remote_code_or_update_client() {
    let source = EXTENSION_FILES
        .iter()
        .filter(|file| file.ends_with(".js") || file.ends_with(".html"))
        .map(|file| fs::read_to_string(Path::new("extension").join(file)).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "fetch(",
        "XMLHttpRequest",
        "EventSource(",
        "import(\"http",
        "from \"http",
        "github.com",
        "api.github.com",
    ] {
        assert!(
            !source.contains(forbidden),
            "found forbidden source: {forbidden}"
        );
    }
    let background = fs::read_to_string("extension/background.js").unwrap();
    assert!(background.contains("new WebSocket(`ws://127.0.0.1:"));
    assert!(!background.contains("wss://"));
}

#[test]
fn server_and_extension_command_allowlists_match() {
    let background = fs::read_to_string("extension/background.js").unwrap();
    let start = background.find("const COMMANDS = new Set([").unwrap();
    let end = background[start..].find("]);").unwrap() + start;
    let extension_commands = quoted_strings(&background[start..end]);
    let server_commands = ACTION_METHODS
        .iter()
        .map(|method| (*method).to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(extension_commands, server_commands);
}

#[test]
fn distributed_text_contains_no_hangul() {
    fn inspect(path: &Path) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                inspect(&path);
            } else if let Ok(source) = fs::read_to_string(&path) {
                assert!(
                    !source.chars().any(|character| {
                        matches!(character as u32, 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7af)
                    }),
                    "Hangul found in {}",
                    path.display()
                );
            }
        }
    }

    for root in ["extension", "public", "src", "tests", "docs"] {
        inspect(Path::new(root));
    }
    for file in ["README.md", "SECURITY.md", "AGENTS.md"] {
        inspect_file(Path::new(file));
    }
}

fn inspect_file(path: &Path) {
    let source = fs::read_to_string(path).unwrap();
    assert!(
        !source.chars().any(|character| {
            matches!(character as u32, 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7af)
        }),
        "Hangul found in {}",
        path.display()
    );
}
