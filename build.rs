use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    println!("cargo:rerun-if-env-changed=PATH");
    println!("cargo:rerun-if-changed=packaging/windows/app.manifest");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_resources();
        return;
    }

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

fn embed_windows_resources() {
    embed_manifest::embed_manifest_file("packaging/windows/app.manifest")
        .expect("failed to embed the required Windows application manifest");

    let version = env::var("CARGO_PKG_VERSION")
        .expect("Cargo must provide CARGO_PKG_VERSION to the build script");
    let fixed_version = windows_fixed_version(&version);
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR"));

    for resource in [
        WindowsVersionResource {
            bin: "local-browser-bridge",
            description: "Local Browser Bridge Server",
            original_filename: "local-browser-bridge.exe",
        },
        WindowsVersionResource {
            bin: "local-computer-helper",
            description: "Local Browser Bridge Computer Helper",
            original_filename: "local-computer-helper.exe",
        },
        WindowsVersionResource {
            bin: "local-browser-bridge-desktop",
            description: "Local Browser Bridge Desktop Host",
            original_filename: "local-browser-bridge-desktop.exe",
        },
    ] {
        let resource_path = out_dir.join(format!("{}-version.rc", resource.bin));
        fs::write(
            &resource_path,
            render_windows_version_resource(resource, &version, &fixed_version),
        )
        .unwrap_or_else(|error| {
            panic!(
                "failed to write Windows version resource {}: {error}",
                resource_path.display()
            )
        });
        embed_resource::compile_for(&resource_path, [resource.bin], embed_resource::NONE)
            .manifest_required()
            .unwrap_or_else(|error| {
                panic!(
                    "failed to compile Windows version resource for {}: {error}",
                    resource.bin
                )
            });
    }
}

#[derive(Clone, Copy)]
struct WindowsVersionResource {
    bin: &'static str,
    description: &'static str,
    original_filename: &'static str,
}

fn windows_fixed_version(version: &str) -> String {
    let core = version
        .split(['-', '+'])
        .next()
        .expect("Cargo package version must contain a numeric core");
    let components = core
        .split('.')
        .map(|component| {
            component.parse::<u16>().unwrap_or_else(|_| {
                panic!(
                    "Windows version component {component:?} in {version:?} must be an unsigned 16-bit integer"
                )
            })
        })
        .collect::<Vec<_>>();
    let [major, minor, patch] = components.as_slice() else {
        panic!("Cargo package version {version:?} must have major.minor.patch components");
    };
    format!("{major},{minor},{patch},0")
}

fn render_windows_version_resource(
    resource: WindowsVersionResource,
    version: &str,
    fixed_version: &str,
) -> String {
    format!(
        r#"1 VERSIONINFO
FILEVERSION {fixed_version}
PRODUCTVERSION {fixed_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "CompanyName", "Local Browser Bridge contributors\0"
            VALUE "FileDescription", "{description}\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "InternalName", "{bin}\0"
            VALUE "LegalCopyright", "Copyright (c) 2026 Local Browser Bridge contributors\0"
            VALUE "OriginalFilename", "{original_filename}\0"
            VALUE "ProductName", "Local Browser Bridge\0"
            VALUE "ProductVersion", "{version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#,
        description = resource.description,
        bin = resource.bin,
        original_filename = resource.original_filename,
    )
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
