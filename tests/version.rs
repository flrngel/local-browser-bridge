use std::fs;

use local_browser_bridge::VERSION;
use serde_json::Value;

#[test]
fn keeps_server_and_extension_versions_aligned() {
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string("extension/manifest.json").unwrap()).unwrap();
    let extension_lib = fs::read_to_string("extension/lib.js").unwrap();
    assert_eq!(manifest["version"], VERSION);
    assert!(extension_lib.contains(&format!("VERSION = \"{VERSION}\"")));
}
