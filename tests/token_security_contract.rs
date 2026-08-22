use std::fs;

fn source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

#[test]
fn windows_token_storage_uses_a_protected_current_user_dacl() {
    let source = source("src/token_windows.rs");

    for required in [
        "OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY",
        "GetTokenInformation(token.0, TokenUser",
        "AddAccessAllowedAceEx(",
        "FILE_ALL_ACCESS.0",
        "SetSecurityDescriptorOwner",
        "SE_DACL_PROTECTED",
        "DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION",
        "information.AceCount != 1",
        "EqualSid(ace_sid, identity.sid())",
    ] {
        assert!(
            source.contains(required),
            "Windows token ACL contract is missing `{required}`"
        );
    }
}

#[test]
fn windows_token_paths_are_opened_without_following_reparse_points() {
    let source = source("src/token_windows.rs");

    for required in [
        "FILE_FLAG_OPEN_REPARSE_POINT",
        "FileAttributeTagInfo",
        "FILE_ATTRIBUTE_REPARSE_POINT",
        "standard.NumberOfLinks == 1",
        "ensure_safe_replacement_target(&destination)",
        "handle_has_expected_kind(handle, true)",
        "validate_private_token_directory(path)",
    ] {
        assert!(
            source.contains(required),
            "Windows no-follow token-path contract is missing `{required}`"
        );
    }
}

#[test]
fn custom_token_parents_are_validated_without_permission_rewrites() {
    let shared = source("src/token.rs");
    let windows = source("src/token_windows.rs");

    for required in [
        "MANAGED_TOKEN_DIRECTORY",
        "path == managed_token_path",
        "&default_token_path()",
        "validate_private_token_directory(parent)",
        "prepare_managed_token_directory(parent).await",
        "open_owned_unix_token_directory",
        "libc::O_DIRECTORY | libc::O_NOFOLLOW",
        "metadata.mode() & 0o777 == 0o700",
        "create the custom token parent first",
        "refusing to use a multiply linked Unix token file",
    ] {
        assert!(
            shared.contains(required),
            "shared parent-safety contract is missing `{required}`"
        );
    }
    assert!(
        !shared.contains("fs::create_dir_all(parent)"),
        "custom token parents must never be created recursively"
    );
    assert!(windows.contains("pub(super) fn validate_private_token_directory"));
    assert!(windows.contains("CreateDirectoryW("));
    assert!(windows.contains("handle_is_exact_private_path(handle_for(&file), true"));
    assert!(windows.contains("if handle_is_exact_private_path(handle, true, &security.identity)?"));
    assert!(shared.contains("an_arbitrary_same_named_unix_parent_is_still_custom_and_unchanged"));
    assert!(shared.contains("rejects_a_multiply_linked_unix_token_without_modifying_either_name"));
}

#[test]
fn token_transactions_remain_bound_to_the_validated_parent_capability() {
    let shared = source("src/token.rs");
    let windows = source("src/token_windows.rs");

    for required in [
        "let directory = prepare_token_parent(path, managed_token_path).await?",
        "unix_openat(",
        "libc::openat(",
        "libc::renameat(",
        "libc::unlinkat(",
        "unix_token_transaction_stays_bound_to_the_validated_parent_after_a_path_swap",
        "windows_token_transaction_holds_no_delete_parent_and_ancestor_leases",
    ] {
        assert!(
            shared.contains(required),
            "capability-bound token transaction is missing `{required}`"
        );
    }

    for required in [
        "pub(super) struct TokenDirectory",
        "identity: FILE_ID_INFO",
        "let path = std::path::absolute(path)?",
        "_ancestor_leases: Vec<File>",
        "open_ancestor_directory_leases(&path)?",
        "fn child_path(&self, path: &Path)",
        "the Windows token path is outside the retained parent directory",
        "fn ensure_bound(&self)",
        "directory.ensure_bound()?",
        "query_file_information::<FILE_ID_INFO>",
        "!= self.identity",
    ] {
        assert!(
            windows.contains(required),
            "Windows token-directory lease contract is missing `{required}`"
        );
    }

    let validation_open = windows
        .split("fn open_token_directory_for_validation")
        .nth(1)
        .and_then(|tail| tail.split("fn open_token_directory_for_acl_update").next())
        .expect("Windows validation-open implementation");
    assert!(validation_open.contains("FILE_SHARE_READ | FILE_SHARE_WRITE"));
    assert!(
        !validation_open.contains("FILE_SHARE_DELETE"),
        "the retained Windows parent handle must deny delete sharing"
    );

    let acl_open = windows
        .split("fn open_token_directory_for_acl_update")
        .nth(1)
        .and_then(|tail| tail.split("fn handle_for").next())
        .expect("Windows ACL-open implementation");
    assert!(acl_open.contains("FILE_SHARE_READ | FILE_SHARE_WRITE"));
    assert!(
        !acl_open.contains("FILE_SHARE_DELETE"),
        "the retained Windows parent handle must deny delete sharing"
    );

    let ancestor_open = windows
        .split("fn open_ancestor_directory_leases")
        .nth(1)
        .and_then(|tail| tail.split("fn handle_for").next())
        .expect("Windows ancestor-lease implementation");
    assert!(ancestor_open.contains("FILE_SHARE_READ | FILE_SHARE_WRITE"));
    assert!(
        !ancestor_open.contains("FILE_SHARE_DELETE"),
        "every retained Windows ancestor handle must deny delete sharing"
    );
}

#[test]
fn windows_token_replacement_is_atomic_and_write_through() {
    let windows = source("src/token_windows.rs");
    let shared = source("src/token.rs");

    assert!(windows.contains("MoveFileExW("));
    assert!(windows.contains("MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH"));
    assert!(
        !shared.contains("fs::remove_file(destination)"),
        "token replacement must never create a delete-before-rename gap"
    );
    assert!(shared.contains("token_path_has_private_permissions(directory, path)"));
}

#[test]
fn windows_acl_behavior_is_covered_by_native_unit_tests() {
    let source = source("src/token.rs");

    for required in [
        "rotates_a_windows_token_with_a_null_dacl",
        "rejects_a_windows_file_symlink_without_touching_its_target",
        "rejects_a_multiply_linked_windows_token_without_modifying_either_name",
        "rejects_a_permissive_windows_custom_parent_without_rewriting_its_dacl",
        "an_arbitrary_same_named_windows_parent_is_still_custom_and_unchanged",
        "managed_windows_policy_replaces_a_permissive_dacl_with_the_exact_private_dacl",
        "rejects_a_windows_parent_reparse_point_without_touching_its_target",
        "windows_token_transaction_holds_no_delete_parent_and_ancestor_leases",
        "path_has_private_permissions_for_test",
    ] {
        assert!(
            source.contains(required),
            "Windows native token test contract is missing `{required}`"
        );
    }
}
