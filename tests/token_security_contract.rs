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
        "ensure_safe_replacement_target(directory, &destination_name)",
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
        "managed_token_path.is_some_and(|managed_path| path == managed_path)",
        "default_token_path().ok()",
        "managed_token_path.as_deref()",
        "validate_private_token_directory(parent)",
        "prepare_managed_token_directory(parent).await",
        "open_owned_unix_token_directory",
        "libc::O_DIRECTORY | libc::O_NOFOLLOW",
        "metadata.mode() & 0o777 == 0o700",
        "create the custom token parent first",
        "refusing to use a multiply linked Unix token file",
        "libc::O_NONBLOCK",
        "rotates_a_fifo_without_blocking_for_a_writer",
        "default_token_path_requires_an_absolute_user_profile",
        "must never fall back to the working directory",
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
    assert!(
        !shared.contains("unwrap_or_else(|| PathBuf::from(\".\"))"),
        "default token storage must never fall back to the working directory"
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
        "windows_relative_read_does_not_follow_an_ancestor_path_swap",
        "windows_relative_create_cleans_its_exact_handle_after_an_ancestor_path_swap",
        "windows_relative_target_check_and_rename_do_not_follow_ancestor_path_swaps",
        "windows_temporary_cleanup_deletes_the_exact_handle_after_an_ancestor_path_swap",
        "windows_junction_swap_fixture",
        "std::fs::rename(&public_ancestor, &moved_ancestor)",
        "std::fs::rename(&decoy_ancestor, &public_ancestor)",
        "assert_windows_path_swap_handoff",
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
        "fn child_name(&self, path: &Path)",
        "the Windows token path is outside the retained parent directory",
        "fn ensure_bound(&self)",
        "directory.ensure_bound()?",
        "query_file_information::<FILE_ID_INFO>",
        "!= self.identity",
        "pub(super) struct PrivateTemporaryFile",
        "directory_identity: FILE_ID_INFO",
        "source.ensure_created_in(directory)?",
        "NtCreateFile(",
        "RootDirectory: handle_for(&directory.file)",
        "Attributes: OBJ_DONT_REPARSE",
        "FILE_TRAVERSE",
        "NtSetInformationFile(",
        "FileRenameInformation",
        "FILE_RENAME_INFORMATION",
        "SetFileInformationByHandle(",
        "FileDispositionInfo",
        "FSCTL_SET_REPARSE_POINT",
        "path_is_mount_point_for_test",
        "resolved_directory_paths_have_same_identity_for_test",
        "let mut bytes_returned = 0u32",
        "Some(&mut bytes_returned)",
        "impl Drop for PrivateTemporaryFile",
        "let result = delete_file_handle(&self.file)",
        "fn ensure_uncommitted(&self)",
        "fn ensure_committed(&self)",
        "source.ensure_uncommitted()?",
        "source.ensure_committed()?",
        "source.mark_committed()",
        "pub(super) struct TestBarrierGuard",
        "impl Drop for TestBarrierGuard",
        "pub(super) struct TestValidationFaultGuard",
        "impl Drop for TestValidationFaultGuard",
    ] {
        assert!(
            windows.contains(required),
            "Windows token-directory capability contract is missing `{required}`"
        );
    }
    assert!(shared.contains("file.discard()"));
    assert!(shared.contains("verify_replaced_token_file(directory, &file, path)"));
    assert!(shared.contains("The rename is the commit boundary"));
    assert!(
        windows
            .contains("#[cfg(test)]\nuse windows::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;"),
        "the junction mutation API must remain test-only"
    );
    assert!(
        !windows.contains("OBJ_CASE_INSENSITIVE"),
        "case-insensitive native lookup can select an unintended case-only sibling"
    );

    let validation_open = windows
        .split("fn open_token_directory_for_validation")
        .nth(1)
        .and_then(|tail| tail.split("fn open_token_directory_for_acl_update").next())
        .expect("Windows validation-open implementation");
    assert!(validation_open.contains("FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE"));
    assert!(validation_open.contains("FILE_TRAVERSE.0"));

    let acl_open = windows
        .split("fn open_token_directory_for_acl_update")
        .nth(1)
        .and_then(|tail| tail.split("fn handle_for").next())
        .expect("Windows ACL-open implementation");
    assert!(acl_open.contains("FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE"));
    assert!(acl_open.contains("FILE_TRAVERSE.0"));

    for forbidden in [
        "MoveFileExW(",
        "open_ancestor_directory_leases",
        "_ancestor_leases",
        "FILE_RENAME_INFO,",
        "FileRenameInfo,",
        "std::fs::remove_file(",
        "CreateFileW(",
    ] {
        assert!(
            !windows.contains(forbidden),
            "Windows child operations must not fall back to pathname authority: {forbidden}"
        );
    }
}

#[test]
fn windows_token_replacement_is_atomic_and_write_through() {
    let windows = source("src/token_windows.rs");
    let shared = source("src/token.rs");

    assert!(windows.contains("FILE_WRITE_THROUGH"));
    assert!(windows.contains("NtSetInformationFile("));
    assert!(windows.contains("FileRenameInformation"));
    assert!(windows.contains("(*information).RootDirectory = handle_for(&directory.file)"));
    assert!(windows.contains("(*information).Anonymous.ReplaceIfExists = true;"));
    assert!(windows.contains("size_of::<FILE_RENAME_INFORMATION>()"));
    assert!(windows.contains(".checked_add(name_bytes)"));
    assert!(windows.contains("let source_identity: FILE_ID_INFO"));
    assert!(
        !shared.contains("fs::remove_file(destination)"),
        "token replacement must never create a delete-before-rename gap"
    );
    assert!(windows.contains(
        "the Windows token rename did not retain the exact private temporary-file identity"
    ));
    assert!(windows.contains("source.file().sync_all()?"));
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
        "windows_relative_read_does_not_follow_an_ancestor_path_swap",
        "windows_relative_create_cleans_its_exact_handle_after_an_ancestor_path_swap",
        "windows_relative_target_check_and_rename_do_not_follow_ancestor_path_swaps",
        "windows_temporary_cleanup_deletes_the_exact_handle_after_an_ancestor_path_swap",
        "windows_private_temporary_validation_failures_always_delete_the_exact_handle",
        "windows_private_temporary_rejects_verify_before_commit",
        "windows_private_temporary_rejects_a_second_replace",
        "windows_token_child_names_reject_win32_namespace_ambiguity",
        "windows_handle_relative_rename_accepts_a_one_character_token_leaf",
        "windows_handle_relative_lookup_preserves_case_distinct_siblings",
        "path_has_private_permissions_for_test",
    ] {
        assert!(
            source.contains(required),
            "Windows native token test contract is missing `{required}`"
        );
    }
}
