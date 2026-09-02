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

#[test]
fn every_cli_flag_is_documented_in_configuration() {
    // Flags deliberately hidden from --help and reserved for internal use
    // (the Windows helper's own disposable-worker relaunch) are not part of
    // the documented user-facing surface.
    const INTERNAL_ONLY: &[&str] = &["--worker"];

    let configuration = read("docs/CONFIGURATION.md");
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
        for flag in &flags {
            if INTERNAL_ONLY.contains(&flag.as_str()) {
                continue;
            }
            assert!(
                configuration.contains(flag.as_str()),
                "docs/CONFIGURATION.md is missing `{flag}` (present in {path}, {label})"
            );
        }
    }
}

#[test]
fn documented_env_vars_are_named_in_source() {
    // The inverse direction: every LBB_* variable CONFIGURATION.md claims to
    // read must actually be read by some shipped binary, so the reference
    // never documents an aspirational variable as if it exists today.
    let configuration = read("docs/CONFIGURATION.md");
    let combined = read("src/main.rs")
        + &read("src/bin/local-browser-bridge-desktop.rs")
        + &read("src/bin/local-computer-helper.rs");
    for line in configuration.lines() {
        let trimmed = line.trim_start_matches('|').trim();
        if let Some(rest) = trimmed.strip_prefix("`LBB_") {
            let name = format!("LBB_{}", rest.split('`').next().unwrap_or(""));
            assert!(
                combined.contains(&name),
                "docs/CONFIGURATION.md documents {name}, but no shipped binary reads it"
            );
        }
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

/// Extracts every `](target)` link target from a markdown source string, in
/// order. Does not attempt to parse reference-style links or image syntax
/// beyond the leading `!`, which callers can ignore.
fn extract_link_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while let Some(offset) = text[index..].find("](") {
        let start = index + offset + 2;
        if let Some(end_offset) = text[start..].find(')') {
            let end = start + end_offset;
            targets.push(text[start..end].to_owned());
            index = end + 1;
        } else {
            break;
        }
        if index >= bytes.len() {
            break;
        }
    }
    targets
}

#[test]
fn doc_tree_has_no_dead_relative_links_or_anchors() {
    let mut files = Vec::new();
    markdown_files(Path::new("docs"), &mut files);
    files.push(PathBuf::from("README.md"));
    files.push(PathBuf::from("SECURITY.md"));
    files.push(PathBuf::from("AGENTS.md"));

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
