use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    println!("cargo:rerun-if-env-changed=PATH");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    for candidate in swift_runtime_candidates() {
        if candidate.is_dir() {
            println!("cargo:rustc-link-search=native={}", candidate.display());
            return;
        }
    }

    println!(
        "cargo:warning=Could not locate the macOS Swift runtime. Install Xcode or Command Line Tools before linking the ScreenCaptureKit helper."
    );
}

fn swift_runtime_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(swiftc) = command_stdout("xcrun", &["--find", "swiftc"])
        && let Some(toolchain_usr) = Path::new(&swiftc).parent().and_then(Path::parent)
    {
        candidates.push(toolchain_usr.join("lib/swift/macosx"));
    }
    if let Some(developer_dir) = command_stdout("xcode-select", &["-p"]) {
        let developer_dir = PathBuf::from(developer_dir);
        candidates
            .push(developer_dir.join("Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx"));
        candidates.push(developer_dir.join("usr/lib/swift/macosx"));
    }
    candidates
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
}
