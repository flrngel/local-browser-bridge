use std::fs;
#[cfg(not(target_os = "windows"))]
use std::process::Command;

const SKILL_ROOT: &str = "skills/local-browser-bridge";
const REFERENCES: [&str; 4] = ["transport.md", "browser.md", "computer.md", "http.md"];
const SORTED_REFERENCES: [&str; 4] = ["browser.md", "computer.md", "http.md", "transport.md"];

#[test]
fn skill_is_compact_and_routes_to_existing_references() {
    let skill = fs::read_to_string(format!("{SKILL_ROOT}/SKILL.md")).unwrap();
    assert!(skill.starts_with("---\nname: local-browser-bridge\n"));
    assert!(skill.contains("description: Use Local Browser Bridge's authenticated loopback API"));
    assert!(skill.lines().count() < 500);
    assert!(!skill.contains("TODO"));
    for reference in REFERENCES {
        assert!(
            skill.contains(&format!("references/{reference}")),
            "SKILL.md must route to {reference}"
        );
        assert!(
            fs::metadata(format!("{SKILL_ROOT}/references/{reference}"))
                .unwrap()
                .is_file()
        );
    }
}

#[test]
fn skill_inventory_is_small_plain_and_installable() {
    let mut inventory: Vec<_> = fs::read_dir(SKILL_ROOT)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    inventory.sort();
    assert_eq!(inventory, ["SKILL.md", "agents", "references"]);

    let mut references: Vec<_> = fs::read_dir(format!("{SKILL_ROOT}/references"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    references.sort();
    assert_eq!(references, SORTED_REFERENCES);

    for path in [
        format!("{SKILL_ROOT}/SKILL.md"),
        format!("{SKILL_ROOT}/agents/openai.yaml"),
    ]
    .into_iter()
    .chain(
        REFERENCES
            .iter()
            .map(|name| format!("{SKILL_ROOT}/references/{name}")),
    ) {
        assert!(!fs::symlink_metadata(path).unwrap().file_type().is_symlink());
    }
}

#[test]
fn generated_references_reconstruct_the_canonical_protocol_byte_for_byte() {
    let canonical = fs::read("docs/PROTOCOL.md").unwrap();
    let combined: Vec<u8> = REFERENCES
        .iter()
        .flat_map(|reference| fs::read(format!("{SKILL_ROOT}/references/{reference}")).unwrap())
        .collect();
    assert_eq!(combined, canonical);
}

#[test]
fn documentation_exposes_standard_and_fallback_install_paths() {
    let readme = fs::read_to_string("README.md").unwrap();
    assert!(
        readme.contains(
            "npx skills add flrngel/local-browser-bridge --skill local-browser-bridge -g"
        )
    );
    assert!(readme.contains("bash scripts/install-agent-skill.sh --target agents"));
    assert!(readme.contains("agents that do not support skills"));
    assert!(readme.contains("docs/PROTOCOL.md"));
}

#[cfg(not(target_os = "windows"))]
#[test]
fn local_skill_installer_passes_its_non_destructive_self_test() {
    let status = Command::new("bash")
        .args(["scripts/install-agent-skill.sh", "--self-test"])
        .status()
        .unwrap();
    assert!(status.success());
}
