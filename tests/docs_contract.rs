//! Fact-locked documentation tests. These replace phrase-locking assertions
//! with checks on stable facts: does every route/method/flag that exists in
//! the code appear in the page that documents it, and does every relative
//! link and anchor in the doc tree resolve. A doc rewrite that keeps these
//! facts true can freely change wording, structure, and examples.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("could not read {path}: {error}"))
}

/// Extracts the quoted string literals between `pub const NAME: &[&str] = &[`
/// and the next `];`, in source order.
fn extract_str_array(source: &str, const_name: &str) -> Vec<String> {
    let marker = format!("pub const {const_name}: &[&str] = &[");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("could not find `{marker}` in source"))
        + marker.len();
    let end = source[start..]
        .find("];")
        .unwrap_or_else(|| panic!("could not find closing `];` for {const_name}"))
        + start;
    let body = &source[start..end];
    let mut items = Vec::new();
    let mut cursor = 0;
    while let Some(open_offset) = body[cursor..].find('"') {
        let open = cursor + open_offset + 1;
        let close_offset = body[open..].find('"').expect("unterminated string literal");
        let close = open + close_offset;
        items.push(body[open..close].to_owned());
        cursor = close + 1;
    }
    items
}

/// Extracts every `.route("path", ...)` string literal from the router
/// construction in `src/server.rs`, in source order.
fn extract_routes(source: &str) -> Vec<String> {
    let mut routes = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(after) = trimmed.strip_prefix(".route(\"") {
            let end = after.find('"').expect("unterminated route literal");
            routes.push(after[..end].to_owned());
        }
    }
    routes
}

/// Extracts every `"--flag"` that begins a `match` arm (a line, once
/// trimmed, that starts with `"` and contains `=>` — i.e. an actual parse
/// target, not a flag mentioned only in prose or in a test's own call site).
fn extract_cli_match_flags(source: &str) -> Vec<String> {
    let mut flags = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('"') || !trimmed.contains("=>") {
            continue;
        }
        let head = trimmed.split("=>").next().unwrap_or("");
        for part in head.split('|') {
            let part = part.trim().trim_matches('"');
            if part.starts_with("--") {
                flags.push(part.to_owned());
            }
        }
    }
    flags
}

#[test]
fn every_server_route_is_documented_in_api_reference() {
    let server = read("src/server.rs");
    let routes = extract_routes(&server);
    assert!(routes.len() >= 14, "router construction looks incomplete");
    let api_reference = read("docs/API_REFERENCE.md");
    for route in &routes {
        assert!(
            api_reference.contains(route.as_str()),
            "docs/API_REFERENCE.md is missing route `{route}` (present in src/server.rs)"
        );
    }
}

#[test]
fn every_action_method_is_documented() {
    let server = read("src/server.rs");
    let methods = extract_str_array(&server, "ACTION_METHODS");
    assert_eq!(
        methods.len(),
        25,
        "ACTION_METHODS count changed; update the docs and this test"
    );
    let api_reference = read("docs/API_REFERENCE.md");
    for method in &methods {
        assert!(
            api_reference.contains(&format!("`{method}`")),
            "docs/API_REFERENCE.md is missing browser method `{method}` (present in ACTION_METHODS)"
        );
    }
}

#[test]
fn every_computer_method_is_documented() {
    let computer_protocol = read("src/computer_protocol.rs");
    let methods = extract_str_array(&computer_protocol, "COMPUTER_METHODS");
    assert_eq!(
        methods.len(),
        13,
        "COMPUTER_METHODS count changed; update the docs and this test"
    );
    let api_reference = read("docs/API_REFERENCE.md");
    for method in &methods {
        assert!(
            api_reference.contains(&format!("`{method}`")),
            "docs/API_REFERENCE.md is missing computer method `{method}` (present in COMPUTER_METHODS)"
        );
    }
}

#[test]
fn every_shell_method_is_documented() {
    let shell = read("src/shell.rs");
    let methods = extract_str_array(&shell, "SHELL_METHODS");
    assert_eq!(methods, vec!["shell.status", "shell.run"]);
    let api_reference = read("docs/API_REFERENCE.md");
    for method in &methods {
        assert!(
            api_reference.contains(&format!("`{method}`")),
            "docs/API_REFERENCE.md is missing shell method `{method}` (present in SHELL_METHODS)"
        );
    }
}

/// Splits `docs/CONFIGURATION.md` into the body text following each
/// binary's own `## \`name\` ...` heading, up to (not including) the next
/// `## ` heading. Text under any other heading (File locations, Ports, ...)
/// belongs to no binary and is excluded, so a flag or variable mentioned
/// only in shared prose there cannot count as documented for one binary.
fn configuration_sections(text: &str) -> BTreeMap<&'static str, String> {
    const BINARIES: [&str; 3] = [
        "local-browser-bridge",
        "local-browser-bridge-desktop",
        "local-computer-helper",
    ];
    let mut sections: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut current: Option<&'static str> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            current = BINARIES
                .iter()
                .find(|name| rest.trim_start().starts_with(&format!("`{name}`")))
                .copied();
            continue;
        }
        if let Some(name) = current {
            let section = sections.entry(name).or_default();
            section.push_str(line);
            section.push('\n');
        }
    }
    sections
}

#[test]
fn every_cli_flag_is_documented_in_configuration() {
    // Flags deliberately hidden from --help and reserved for internal use
    // (the Windows helper's own disposable-worker relaunch) are not part of
    // the documented user-facing surface.
    const INTERNAL_ONLY: &[&str] = &["--worker"];

    let configuration = read("docs/CONFIGURATION.md");
    let sections = configuration_sections(&configuration);
    let console_server_scope = sections
        .get("local-browser-bridge")
        .cloned()
        .unwrap_or_default();

    for (path, label) in [
        ("src/main.rs", "local-browser-bridge"),
        (
            "src/bin/local-browser-bridge-desktop.rs",
            "local-browser-bridge-desktop",
        ),
        ("src/bin/local-computer-helper.rs", "local-computer-helper"),
    ] {
        let source = read(path);
        let flags = extract_cli_match_flags(&source);
        assert!(!flags.is_empty(), "found no CLI flags in {path}");
        let mut scope = sections
            .get(label)
            .cloned()
            .unwrap_or_else(|| panic!("docs/CONFIGURATION.md has no `## `{label}`` section"));
        if label == "local-browser-bridge-desktop" {
            // The desktop host's own section documents its flags by
            // explicit inheritance from the console server's table ("all
            // its flags except ..."), so a shared flag is documented there,
            // not repeated in the desktop section.
            scope.push_str(&console_server_scope);
        }
        for flag in &flags {
            if INTERNAL_ONLY.contains(&flag.as_str()) {
                continue;
            }
            assert!(
                scope.contains(flag.as_str()),
                "docs/CONFIGURATION.md's `{label}` section (plus its documented \
                 inheritance) is missing `{flag}` (present in {path}); a flag \
                 documented only under a different binary's heading does not count"
            );
        }
    }
}

#[test]
fn documented_env_vars_are_named_in_source() {
    // The inverse direction: every LBB_* variable CONFIGURATION.md claims a
    // given binary reads must actually be read by that binary's own source
    // file, so a variable documented under the wrong binary's heading (or
    // one no shipped binary reads at all) fails here.
    let configuration = read("docs/CONFIGURATION.md");
    let sections = configuration_sections(&configuration);
    for (path, label) in [
        ("src/main.rs", "local-browser-bridge"),
        (
            "src/bin/local-browser-bridge-desktop.rs",
            "local-browser-bridge-desktop",
        ),
        ("src/bin/local-computer-helper.rs", "local-computer-helper"),
    ] {
        let source = read(path);
        let scope = sections
            .get(label)
            .unwrap_or_else(|| panic!("docs/CONFIGURATION.md has no `## `{label}`` section"));
        for line in scope.lines() {
            let trimmed = line.trim_start_matches('|').trim();
            if let Some(rest) = trimmed.strip_prefix("`LBB_") {
                let name = format!("LBB_{}", rest.split('`').next().unwrap_or(""));
                assert!(
                    source.contains(&name),
                    "docs/CONFIGURATION.md's `{label}` section documents {name}, \
                     but {path} does not read it"
                );
            }
        }
    }
}

#[test]
fn health_response_keys_are_documented() {
    let server = read("src/server.rs");
    let start = server
        .find("async fn health(")
        .expect("could not find `async fn health(` in src/server.rs");
    let object_start = server[start..]
        .find("json!({")
        .map(|offset| start + offset + "json!({".len())
        .expect("could not find the health handler's json!({...}) body");
    let object_end = server[object_start..]
        .find("})")
        .map(|offset| object_start + offset)
        .expect("could not find the closing `})` of the health handler's body");
    let mut keys = Vec::new();
    for line in server[object_start..object_end].lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('"') {
            if let Some(end) = rest.find('"') {
                keys.push(rest[..end].to_owned());
            }
        }
    }
    assert_eq!(
        keys.len(),
        5,
        "GET /health's response shape changed; update docs/API_REFERENCE.md and this test"
    );
    let api_reference = read("docs/API_REFERENCE.md");
    for key in &keys {
        assert!(
            api_reference.contains(&format!("\"{key}\"")),
            "docs/API_REFERENCE.md's /health example is missing key `{key}` (present in the handler)"
        );
    }
}

#[test]
fn taxonomy_codes_are_documented() {
    let error_taxonomy = read("src/error_taxonomy.rs");
    let start = error_taxonomy
        .find("pub fn as_str(self) -> &'static str {")
        .expect("could not find TaxonomyCode::as_str in src/error_taxonomy.rs");
    let mut codes = Vec::new();
    for line in error_taxonomy[start..].lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.starts_with("pub fn") {
            break;
        }
        let Some(arrow) = trimmed.find("=>") else {
            continue;
        };
        let Some(rest) = trimmed[arrow + 2..].trim().strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else {
            continue;
        };
        codes.push(rest[..end].to_owned());
    }
    assert_eq!(
        codes.len(),
        20,
        "TaxonomyCode's variant count changed; update docs/API_REFERENCE.md and this test"
    );
    let api_reference = read("docs/API_REFERENCE.md");
    for code in &codes {
        assert!(
            api_reference.contains(&format!("`{code}`")),
            "docs/API_REFERENCE.md's error taxonomy is missing code `{code}` (present in TaxonomyCode)"
        );
    }
}

#[test]
fn health_example_version_matches_cargo_toml() {
    // The three flagship `GET /health` transcripts show a literal version
    // number. Nothing else ties that number to the package's actual
    // version, so it silently goes stale at the next bump unless this test
    // fails first.
    let cargo_toml = read("Cargo.toml");
    let marker = "version = \"";
    let start = cargo_toml
        .find(marker)
        .map(|offset| offset + marker.len())
        .expect("could not find `version = \"...\"` in Cargo.toml");
    let end = cargo_toml[start..]
        .find('"')
        .map(|offset| start + offset)
        .expect("unterminated version string in Cargo.toml");
    let version = &cargo_toml[start..end];
    let needle = format!("\"version\":\"{version}\"");
    for path in [
        "README.md",
        "docs/API_REFERENCE.md",
        "docs/AGENT_INTEGRATION.md",
    ] {
        let text = read(path);
        assert!(
            text.contains(&needle),
            "{path}'s /health example does not show the current crate version \
             {version} ({needle}); update the pinned example after every version bump"
        );
    }
}

#[test]
fn installer_one_liners_point_at_scripts_that_exist() {
    for doc_path in [
        "README.md",
        "docs/INSTALL_MACOS.md",
        "docs/INSTALL_WINDOWS.md",
    ] {
        let text = read(doc_path);
        for line in text.lines() {
            let mut remainder = line;
            while let Some(index) = remainder.find("scripts/") {
                remainder = &remainder[index..];
                let end = remainder
                    .find(|c: char| {
                        c.is_whitespace() || c == '\'' || c == ')' || c == '"' || c == '`'
                    })
                    .unwrap_or(remainder.len());
                let candidate = &remainder[..end];
                if (candidate.ends_with(".sh") || candidate.ends_with(".ps1"))
                    && !candidate.contains("${")
                {
                    assert!(
                        Path::new(candidate).is_file(),
                        "{doc_path} references `{candidate}`, which does not exist"
                    );
                }
                remainder = &remainder[end..];
            }
        }
    }
}

/// GitHub's heading-anchor algorithm, close enough for this repository's
/// headings: lowercase, drop backticks, drop everything but word characters/
/// spaces/hyphens, collapse whitespace to a single hyphen.
fn slugify(heading: &str) -> String {
    let lowered = heading.trim().to_lowercase().replace('`', "");
    let mut slug = String::new();
    let mut last_was_space = false;
    for character in lowered.chars() {
        if character.is_alphanumeric() || character == '_' || character == '-' {
            slug.push(character);
            last_was_space = false;
        } else if character.is_whitespace() {
            if !last_was_space {
                slug.push('-');
            }
            last_was_space = true;
        }
        // every other character (punctuation) is dropped, not replaced
    }
    slug.trim_matches('-').to_owned()
}

fn heading_slugs(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    let mut slugs = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let rest = rest.trim_start_matches('#');
            if rest.starts_with(' ') {
                slugs.push(slugify(rest.trim()));
            }
        }
    }
    slugs
}

fn markdown_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            out.push(path);
        }
    }
}

/// Blanks out the content of fenced (```) code blocks, keeping every other
/// line as-is, so a `](` inside a code example is never mistaken for a
/// markdown link.
fn strip_fenced_code_blocks(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            result.push_str(line);
        }
        result.push('\n');
    }
    result
}

/// Extracts every `](target)` link target from a markdown source string, in
/// order. Does not attempt to parse reference-style links or image syntax
/// beyond the leading `!`, which callers can ignore. The target's closing
/// `)` is found by tracking paren depth so a target with its own balanced
/// parentheses (a parenthetical aside around a bare URL, for example) is not
/// truncated at the first `)`.
fn extract_link_targets(text: &str) -> Vec<String> {
    let text = strip_fenced_code_blocks(text);
    let mut targets = Vec::new();
    let mut index = 0;
    while let Some(offset) = text[index..].find("](") {
        let start = index + offset + 2;
        let mut depth: i32 = 1;
        let mut end = None;
        for (byte_offset, character) in text[start..].char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + byte_offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            break;
        };
        targets.push(text[start..end].to_owned());
        index = end + 1;
        if index >= text.len() {
            break;
        }
    }
    targets
}

#[test]
fn doc_tree_has_no_dead_relative_links_or_anchors() {
    let mut files = Vec::new();
    markdown_files(Path::new("docs"), &mut files);
    markdown_files(Path::new("skills"), &mut files);
    files.push(PathBuf::from("README.md"));
    files.push(PathBuf::from("SECURITY.md"));
    files.push(PathBuf::from("AGENTS.md"));
    files.push(PathBuf::from("evidence/README.md"));

    let mut heading_cache: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let mut failures = Vec::new();

    for file in &files {
        let text = fs::read_to_string(file).unwrap();
        let base = file.parent().unwrap_or_else(|| Path::new("."));
        for target in extract_link_targets(&text) {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                continue;
            }
            let (path_part, anchor) = match target.split_once('#') {
                Some((path, anchor)) => (path, Some(anchor)),
                None => (target.as_str(), None),
            };

            let resolved = if path_part.is_empty() {
                file.clone()
            } else {
                let joined = base.join(path_part);
                match joined.canonicalize() {
                    Ok(canonical) => canonical,
                    Err(_) => {
                        failures.push(format!(
                            "{}: `{target}` -> {} does not exist",
                            file.display(),
                            joined.display()
                        ));
                        continue;
                    }
                }
            };

            if let Some(anchor) = anchor {
                if resolved
                    .extension()
                    .is_none_or(|extension| extension != "md")
                {
                    continue;
                }
                let slugs = heading_cache
                    .entry(resolved.clone())
                    .or_insert_with(|| heading_slugs(&resolved));
                if !slugs.iter().any(|slug| slug == anchor) {
                    failures.push(format!(
                        "{}: `{target}` -> no heading in {} slugifies to `#{anchor}`",
                        file.display(),
                        resolved.display()
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "dead documentation links:\n{}",
        failures.join("\n")
    );
}
