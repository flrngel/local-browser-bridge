use std::ffi::{OsStr, c_void};
use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::mem::{offset_of, size_of};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _};
use std::path::{Path, PathBuf};
use std::ptr;

#[cfg(test)]
use std::cell::RefCell;

use windows::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
    FILE_RENAME_INFORMATION, FILE_SYNCHRONOUS_IO_NONALERT, FILE_WRITE_THROUGH,
    FileRenameInformation, NTCREATEFILE_CREATE_DISPOSITION, NTCREATEFILE_CREATE_OPTIONS,
    NtCreateFile, NtSetInformationFile,
};
use windows::Win32::Foundation::{
    BOOL, BOOLEAN, CloseHandle, ERROR_SUCCESS, HANDLE, NTSTATUS, RtlNtStatusToDosError,
    STATUS_REPARSE_POINT_ENCOUNTERED, UNICODE_STRING,
};
use windows::Win32::Security::Authorization::{SE_FILE_OBJECT, SetSecurityInfo};
use windows::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_FLAGS, ACL, ACL_REVISION, ACL_SIZE_INFORMATION, AclSizeInformation,
    AddAccessAllowedAceEx, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, EqualSid, GetAce,
    GetAclInformation, GetKernelObjectSecurity, GetLengthSid, GetSecurityDescriptorControl,
    GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, GetTokenInformation, INHERITED_ACE,
    InitializeAcl, InitializeSecurityDescriptor, IsValidSid, OBJECT_INHERIT_ACE,
    OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SetSecurityDescriptorControl,
    SetSecurityDescriptorDacl, SetSecurityDescriptorOwner, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    CreateDirectoryW, DELETE, FILE_ACCESS_RIGHTS, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_DISPOSITION_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE, FILE_ID_INFO,
    FILE_INFO_BY_HANDLE_CLASS, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE,
    FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FILE_TRAVERSE,
    FileAttributeTagInfo, FileDispositionInfo, FileIdInfo, FileStandardInfo,
    GetFileInformationByHandleEx, READ_CONTROL, SYNCHRONIZE, SetFileInformationByHandle, WRITE_DAC,
};
#[cfg(test)]
use windows::Win32::Storage::FileSystem::{
    FILE_FLAGS_AND_ATTRIBUTES, FILE_WRITE_ATTRIBUTES, FileCaseSensitiveInfo,
};
#[cfg(test)]
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::IO::IO_STATUS_BLOCK;
#[cfg(test)]
use windows::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;
#[cfg(test)]
use windows::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{Error as WindowsError, PCWSTR, PWSTR};

const SECURITY_DESCRIPTOR_REVISION: u32 = 1;
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const MAX_TOKEN_FILE_BYTES: i64 = 128;
const OBJ_DONT_REPARSE: u32 = 0x0000_1000;

pub(super) struct TokenDirectory {
    file: File,
    identity: FILE_ID_INFO,
    path: PathBuf,
}

#[derive(Debug)]
pub(super) struct PrivateTemporaryFile {
    file: File,
    directory_identity: FILE_ID_INFO,
    committed: bool,
}

impl PrivateTemporaryFile {
    pub(super) fn write_and_sync(&mut self, contents: &[u8]) -> io::Result<()> {
        self.ensure_uncommitted()?;
        self.file.write_all(contents)?;
        self.file.flush()?;
        self.file.sync_all()
    }

    pub(super) fn discard(mut self) -> io::Result<()> {
        self.ensure_uncommitted()?;
        run_test_barrier("cleanup");
        let result = delete_file_handle(&self.file);
        if result.is_ok() {
            self.committed = true;
        }
        result
    }

    fn file(&self) -> &File {
        &self.file
    }

    fn ensure_uncommitted(&self) -> io::Result<()> {
        if self.committed {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the Windows token temporary-file capability was already committed",
            ))
        } else {
            Ok(())
        }
    }

    fn ensure_committed(&self) -> io::Result<()> {
        if self.committed {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the Windows token temporary-file capability has not reached its rename commit boundary",
            ))
        }
    }

    fn mark_committed(&mut self) -> io::Result<()> {
        self.ensure_uncommitted()?;
        self.committed = true;
        Ok(())
    }

    fn ensure_created_in(&self, directory: &TokenDirectory) -> io::Result<()> {
        if self.directory_identity == directory.identity {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the Windows token temporary-file capability belongs to another directory",
            ))
        }
    }
}

impl Drop for PrivateTemporaryFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = delete_file_handle(&self.file);
        }
    }
}

impl TokenDirectory {
    fn new(file: File, path: &Path) -> io::Result<Self> {
        let identity = query_file_information(handle_for(&file), FileIdInfo)?;
        let path = std::path::absolute(path)?;
        let directory = Self {
            file,
            identity,
            path,
        };
        directory.ensure_bound()?;
        Ok(directory)
    }

    fn child_name(&self, path: &Path) -> io::Result<Vec<u16>> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if std::path::absolute(parent)? != self.path {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the Windows token path is outside the retained parent directory",
            ));
        }
        let name = path.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "the Windows token path must end with an ordinary file name",
            )
        })?;
        validate_child_name(name)
    }

    fn ensure_bound(&self) -> io::Result<()> {
        let current_user = CurrentUser::load()?;
        if !handle_is_exact_private_path(handle_for(&self.file), true, &current_user)? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the held Windows token-directory capability is no longer private",
            ));
        }
        let reopened = open_token_directory_for_validation(&self.path)?;
        if !handle_is_exact_private_path(handle_for(&reopened), true, &current_user)?
            || query_file_information::<FILE_ID_INFO>(handle_for(&reopened), FileIdInfo)?
                != self.identity
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the Windows token-directory path no longer names the validated directory",
            ));
        }
        Ok(())
    }
}

pub(super) fn open_private_token_file(
    directory: &TokenDirectory,
    path: &Path,
) -> io::Result<Option<File>> {
    directory.ensure_bound()?;
    run_test_barrier("read");
    let name = directory.child_name(path)?;
    let file = open_relative_file(
        directory,
        &name,
        FILE_READ_DATA | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE,
        FILE_SHARE_READ,
        FILE_OPEN,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        None,
    )?;
    let identity = CurrentUser::load()?;
    if !handle_is_exact_private_path(handle_for(&file), false, &identity)? {
        return Ok(None);
    }
    directory.ensure_bound()?;
    Ok(Some(file))
}

pub(super) fn create_private_token_file(
    directory: &TokenDirectory,
    path: &Path,
) -> io::Result<PrivateTemporaryFile> {
    directory.ensure_bound()?;
    run_test_barrier("create");
    let name = directory.child_name(path)?;
    let mut security = PrivateSecurity::new(ACE_FLAGS(0))?;
    let file = open_relative_file(
        directory,
        &name,
        FILE_GENERIC_WRITE | DELETE | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_DELETE,
        FILE_CREATE,
        FILE_NON_DIRECTORY_FILE
            | FILE_OPEN_REPARSE_POINT
            | FILE_SYNCHRONOUS_IO_NONALERT
            | FILE_WRITE_THROUGH,
        Some(&mut security),
    )?;
    let temporary = PrivateTemporaryFile {
        file,
        directory_identity: directory.identity,
        committed: false,
    };
    let validation =
        validate_private_temporary_file("post_create", temporary.file(), &security.identity);
    match validation {
        Ok(true) => {}
        Ok(false) => {
            let error = io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Windows did not create the token file with its protected owner-only DACL",
            );
            let cleanup = temporary.discard();
            return Err(cleanup_after_new_file_error(error, cleanup));
        }
        Err(error) => {
            let cleanup = temporary.discard();
            return Err(cleanup_after_new_file_error(error, cleanup));
        }
    }
    if let Err(error) = directory.ensure_bound() {
        let cleanup = temporary.discard();
        return Err(cleanup_after_new_file_error(error, cleanup));
    }
    Ok(temporary)
}

pub(super) fn replace_token_file(
    directory: &TokenDirectory,
    source: &mut PrivateTemporaryFile,
    destination: &Path,
) -> io::Result<()> {
    source.ensure_uncommitted()?;
    directory.ensure_bound()?;
    source.ensure_created_in(directory)?;
    let destination_name = directory.child_name(destination)?;
    run_test_barrier("target_check");
    ensure_safe_replacement_target(directory, &destination_name)?;
    let identity = CurrentUser::load()?;
    if !validate_private_temporary_file("pre_rename", source.file(), &identity)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to rename a non-private Windows token temporary file",
        ));
    }
    run_test_barrier("rename");
    rename_relative_file(directory, source.file(), &destination_name)?;
    source.mark_committed()
}

pub(super) fn verify_replaced_token_file(
    directory: &TokenDirectory,
    source: &PrivateTemporaryFile,
    destination: &Path,
) -> io::Result<()> {
    source.ensure_committed()?;
    source.ensure_created_in(directory)?;
    source.file().sync_all()?;
    let destination_name = directory.child_name(destination)?;
    let source_identity: FILE_ID_INFO =
        query_file_information(handle_for(source.file()), FileIdInfo)?;
    let identity = CurrentUser::load()?;
    let renamed = open_relative_file(
        directory,
        &destination_name,
        FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        FILE_OPEN,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        None,
    )?;
    if query_file_information::<FILE_ID_INFO>(handle_for(&renamed), FileIdInfo)? != source_identity
        || !handle_is_exact_private_path(handle_for(&renamed), false, &identity)?
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the Windows token rename did not retain the exact private temporary-file identity",
        ));
    }
    directory.ensure_bound()
}

pub(super) fn ensure_token_directory_bound(directory: &TokenDirectory) -> io::Result<()> {
    directory.ensure_bound()
}

pub(super) fn create_private_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    let mut security = PrivateSecurity::new(CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE)?;
    let attributes = security.attributes();
    let wide_path = wide_path(path)?;
    unsafe { CreateDirectoryW(PCWSTR(wide_path.as_ptr()), Some(&attributes)) }
        .map_err(windows_error)?;
    validate_private_token_directory(path)
}

pub(super) fn validate_private_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    let file = open_token_directory_for_validation(path)?;
    let identity = CurrentUser::load()?;
    if handle_is_exact_private_path(handle_for(&file), true, &identity)? {
        TokenDirectory::new(file, path)
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the token parent must already have a protected current-user-only DACL",
        ))
    }
}

fn ensure_safe_replacement_target(directory: &TokenDirectory, name: &[u16]) -> io::Result<()> {
    let file = match open_relative_file(
        directory,
        name,
        FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        FILE_OPEN,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        None,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let handle = handle_for(&file);
    let attributes: FILE_ATTRIBUTE_TAG_INFO = query_file_information(handle, FileAttributeTagInfo)?;
    let standard: FILE_STANDARD_INFO = query_file_information(handle, FileStandardInfo)?;
    if attributes.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || standard.Directory.0 != 0
        || standard.DeletePending.0 != 0
        || standard.NumberOfLinks != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to replace a reparse-point, directory, or multiply linked Windows token path",
        ));
    }
    Ok(())
}

pub(super) fn harden_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    let file = open_token_directory_for_acl_update(path)?;
    let security = PrivateSecurity::new(CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE)?;
    let handle = handle_for(&file);
    if handle_is_exact_private_path(handle, true, &security.identity)? {
        return TokenDirectory::new(file, path);
    }
    if !handle_has_expected_kind(handle, true)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the Windows token directory is not an ordinary non-reparse directory",
        ));
    }
    if !handle_owner_is_current_user(handle, &security.identity)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the Windows token directory is not owned by the current user",
        ));
    }
    install_private_dacl(handle, &security)?;
    if !handle_is_exact_private_path(handle, true, &security.identity)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Windows did not retain the token directory's protected owner-only DACL",
        ));
    }
    TokenDirectory::new(file, path)
}

#[cfg(test)]
fn open_path_no_follow(path: &Path, flags: FILE_FLAGS_AND_ATTRIBUTES) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(flags.0);
    options.open(path)
}

#[cfg(test)]
fn open_path_for_acl_update(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(READ_CONTROL.0 | WRITE_DAC.0 | FILE_READ_ATTRIBUTES.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
        .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
    options.open(path)
}

fn open_token_directory_for_validation(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(READ_CONTROL.0 | FILE_READ_ATTRIBUTES.0 | FILE_TRAVERSE.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
        .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
    options.open(path)
}

fn open_token_directory_for_acl_update(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(READ_CONTROL.0 | WRITE_DAC.0 | FILE_READ_ATTRIBUTES.0 | FILE_TRAVERSE.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
        .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
    options.open(path)
}

fn open_relative_file(
    directory: &TokenDirectory,
    name: &[u16],
    desired_access: FILE_ACCESS_RIGHTS,
    share_access: FILE_SHARE_MODE,
    disposition: NTCREATEFILE_CREATE_DISPOSITION,
    options: NTCREATEFILE_CREATE_OPTIONS,
    security: Option<&mut PrivateSecurity>,
) -> io::Result<File> {
    let name_bytes = name.len().checked_mul(size_of::<u16>()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows child name is too long",
        )
    })?;
    let name_length = u16::try_from(name_bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows child name exceeds the native Unicode-string limit",
        )
    })?;
    let mut name = name.to_vec();
    let unicode_name = UNICODE_STRING {
        Length: name_length,
        MaximumLength: name_length,
        Buffer: PWSTR(name.as_mut_ptr()),
    };
    let security_descriptor = security
        .as_ref()
        .map_or(ptr::null(), |security| security.descriptor_ptr());
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>())
            .expect("Windows object attributes fit u32"),
        RootDirectory: handle_for(&directory.file),
        ObjectName: ptr::addr_of!(unicode_name),
        Attributes: OBJ_DONT_REPARSE,
        SecurityDescriptor: security_descriptor,
        SecurityQualityOfService: ptr::null(),
    };
    let mut status_block = IO_STATUS_BLOCK::default();
    let mut handle = HANDLE::default();
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            desired_access,
            ptr::addr_of!(attributes),
            &mut status_block,
            None,
            FILE_ATTRIBUTE_NORMAL,
            share_access,
            disposition,
            options,
            None,
            0,
        )
    };
    if status.0 < 0 {
        if !handle.is_invalid() {
            let _ = unsafe { CloseHandle(handle) };
        }
        Err(ntstatus_error(status))
    } else if handle.is_invalid() {
        Err(io::Error::other(
            "NtCreateFile returned success without a valid child handle",
        ))
    } else {
        Ok(unsafe { File::from_raw_handle(handle.0) })
    }
}

fn rename_relative_file(
    directory: &TokenDirectory,
    source: &File,
    destination_name: &[u16],
) -> io::Result<()> {
    let name_bytes = destination_name
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows child name is too long",
            )
        })?;
    // FILE_RENAME_INFORMATION is a variable-length structure whose documented input size is the
    // full fixed structure plus the filename bytes, even though FileName already declares one
    // WCHAR.
    let buffer_bytes = size_of::<FILE_RENAME_INFORMATION>()
        .checked_add(name_bytes)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows rename buffer is too large",
            )
        })?;
    let mut buffer = AlignedBuffer::new(buffer_bytes)?;
    let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    unsafe {
        (*information).Anonymous.ReplaceIfExists = BOOLEAN(1);
        (*information).RootDirectory = handle_for(&directory.file);
        (*information).FileNameLength = u32::try_from(name_bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows child name is too long",
            )
        })?;
        ptr::copy_nonoverlapping(
            destination_name.as_ptr(),
            ptr::addr_of_mut!((*information).FileName).cast::<u16>(),
            destination_name.len(),
        );
    }
    let mut status_block = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtSetInformationFile(
            handle_for(source),
            &mut status_block,
            information.cast(),
            u32::try_from(buffer_bytes).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Windows rename buffer is too large",
                )
            })?,
            FileRenameInformation,
        )
    };
    if status.0 < 0 {
        Err(ntstatus_error(status))
    } else {
        Ok(())
    }
}

fn delete_file_handle(file: &File) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO {
        DeleteFile: BOOLEAN(1),
    };
    unsafe {
        SetFileInformationByHandle(
            handle_for(file),
            FileDispositionInfo,
            ptr::addr_of!(disposition).cast(),
            u32::try_from(size_of::<FILE_DISPOSITION_INFO>())
                .expect("Windows disposition information fits u32"),
        )
    }
    .map_err(windows_error)
}

fn cleanup_after_new_file_error(operation: io::Error, cleanup: io::Result<()>) -> io::Error {
    match cleanup {
        Ok(()) => operation,
        Err(cleanup) => io::Error::new(
            cleanup.kind(),
            format!(
                "validation of a newly created Windows token file failed and exact-handle cleanup also failed: operation={operation}; cleanup={cleanup}"
            ),
        ),
    }
}

#[cfg(test)]
type TestBarrier = Option<(&'static str, Box<dyn FnOnce()>)>;

#[cfg(test)]
type TestValidationFault = Option<(&'static str, bool)>;

#[cfg(test)]
pub(super) struct TestBarrierGuard {
    armed: bool,
}

#[cfg(test)]
impl TestBarrierGuard {
    pub(super) fn assert_consumed(mut self) {
        TEST_BARRIER.with(|barrier| {
            assert!(
                barrier.borrow().is_none(),
                "the expected Windows token test barrier was not reached"
            );
        });
        self.armed = false;
    }
}

#[cfg(test)]
impl Drop for TestBarrierGuard {
    fn drop(&mut self) {
        if self.armed {
            TEST_BARRIER.with(|barrier| {
                barrier.borrow_mut().take();
            });
        }
    }
}

#[cfg(test)]
pub(super) struct TestValidationFaultGuard {
    armed: bool,
}

#[cfg(test)]
impl TestValidationFaultGuard {
    pub(super) fn assert_consumed(mut self) {
        TEST_VALIDATION_FAULT.with(|fault| {
            assert!(
                fault.borrow().is_none(),
                "the expected Windows token validation fault was not reached"
            );
        });
        self.armed = false;
    }
}

#[cfg(test)]
impl Drop for TestValidationFaultGuard {
    fn drop(&mut self) {
        if self.armed {
            TEST_VALIDATION_FAULT.with(|fault| {
                fault.borrow_mut().take();
            });
        }
    }
}

#[cfg(test)]
thread_local! {
    static TEST_BARRIER: RefCell<TestBarrier> = RefCell::new(None);
    static TEST_VALIDATION_FAULT: RefCell<TestValidationFault> = const { RefCell::new(None) };
}

#[cfg(test)]
fn run_test_barrier(stage: &'static str) {
    TEST_BARRIER.with(|barrier| {
        let callback = {
            let mut barrier = barrier.borrow_mut();
            if barrier
                .as_ref()
                .is_some_and(|(expected, _)| *expected == stage)
            {
                barrier.take().map(|(_, callback)| callback)
            } else {
                None
            }
        };
        if let Some(callback) = callback {
            callback();
        }
    });
}

#[cfg(not(test))]
fn run_test_barrier(_stage: &'static str) {}

#[cfg(test)]
pub(super) fn install_test_barrier(
    stage: &'static str,
    callback: impl FnOnce() + 'static,
) -> TestBarrierGuard {
    TEST_BARRIER.with(|barrier| {
        assert!(
            barrier.replace(Some((stage, Box::new(callback)))).is_none(),
            "a Windows token test barrier was already installed"
        );
    });
    TestBarrierGuard { armed: true }
}

#[cfg(test)]
pub(super) fn install_validation_fault_for_test(
    stage: &'static str,
    return_error: bool,
) -> TestValidationFaultGuard {
    TEST_VALIDATION_FAULT.with(|fault| {
        assert!(
            fault.replace(Some((stage, return_error))).is_none(),
            "a Windows token validation fault was already installed"
        );
    });
    TestValidationFaultGuard { armed: true }
}

fn validate_child_name(name: &OsStr) -> io::Result<Vec<u16>> {
    let name: Vec<u16> = name.encode_wide().collect();
    let invalid_shape = name.is_empty()
        || name == [u16::from(b'.')]
        || name == [u16::from(b'.'), u16::from(b'.')]
        || name
            .iter()
            .any(|unit| matches!(*unit, 0..=31 | 34 | 42 | 47 | 58 | 60 | 62 | 63 | 92 | 124))
        || name.last().is_some_and(|unit| matches!(*unit, 32 | 46))
        || name
            .windows(2)
            .any(|units| units[0] == u16::from(b' ') && units[1] == u16::from(b'.'));
    let base_end = name
        .iter()
        .position(|unit| *unit == u16::from(b'.'))
        .unwrap_or(name.len());
    let base: Vec<u16> = name[..base_end]
        .iter()
        .map(|unit| {
            if (u16::from(b'a')..=u16::from(b'z')).contains(unit) {
                unit - u16::from(b'a') + u16::from(b'A')
            } else {
                *unit
            }
        })
        .collect();
    let reserved_device = matches!(
        base.as_slice(),
        [67, 79, 78]
            | [80, 82, 78]
            | [65, 85, 88]
            | [78, 85, 76]
            | [67, 76, 79, 67, 75, 36]
            | [67, 79, 78, 73, 78, 36]
            | [67, 79, 78, 79, 85, 84, 36]
    ) || (base.len() == 4
        && matches!(&base[..3], [67, 79, 77] | [76, 80, 84])
        && matches!(base[3], 49..=57 | 0x00b2 | 0x00b3 | 0x00b9));
    if invalid_shape || reserved_device {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the Windows token file name must be an unambiguous ordinary child name",
        ));
    }
    Ok(name)
}

fn handle_for(file: &File) -> HANDLE {
    HANDLE(file.as_raw_handle())
}

fn validate_private_temporary_file(
    stage: &'static str,
    file: &File,
    identity: &CurrentUser,
) -> io::Result<bool> {
    #[cfg(not(test))]
    let _ = stage;
    #[cfg(test)]
    if let Some(return_error) = TEST_VALIDATION_FAULT.with(|fault| {
        let mut fault = fault.borrow_mut();
        if fault
            .as_ref()
            .is_some_and(|(expected, _)| *expected == stage)
        {
            fault.take().map(|(_, return_error)| return_error)
        } else {
            None
        }
    }) {
        return if return_error {
            Err(io::Error::other(
                "injected Windows token validation query failure",
            ))
        } else {
            Ok(false)
        };
    }
    handle_is_exact_private_path(handle_for(file), false, identity)
}

fn handle_is_exact_private_path(
    handle: HANDLE,
    expect_directory: bool,
    identity: &CurrentUser,
) -> io::Result<bool> {
    if !handle_has_expected_kind(handle, expect_directory)? {
        return Ok(false);
    }
    handle_acl_is_private(handle, expect_directory, identity)
}

fn handle_has_expected_kind(handle: HANDLE, expect_directory: bool) -> io::Result<bool> {
    let attributes: FILE_ATTRIBUTE_TAG_INFO = query_file_information(handle, FileAttributeTagInfo)?;
    if attributes.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Ok(false);
    }

    let standard: FILE_STANDARD_INFO = query_file_information(handle, FileStandardInfo)?;
    if standard.Directory.0 != u8::from(expect_directory) {
        return Ok(false);
    }
    if expect_directory {
        return Ok(true);
    }
    Ok(standard.DeletePending.0 == 0
        && standard.NumberOfLinks == 1
        && (0..=MAX_TOKEN_FILE_BYTES).contains(&standard.EndOfFile))
}

fn query_file_information<T: Default>(
    handle: HANDLE,
    class: FILE_INFO_BY_HANDLE_CLASS,
) -> io::Result<T> {
    let mut value = T::default();
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            class,
            ptr::addr_of_mut!(value).cast(),
            u32::try_from(size_of::<T>()).expect("Windows file information structure fits u32"),
        )
    }
    .map_err(windows_error)?;
    Ok(value)
}

fn handle_owner_is_current_user(handle: HANDLE, identity: &CurrentUser) -> io::Result<bool> {
    let descriptor = read_security_descriptor(handle)?;
    security_descriptor_owner_matches(descriptor.as_security_descriptor(), identity)
}

fn handle_acl_is_private(
    handle: HANDLE,
    directory: bool,
    identity: &CurrentUser,
) -> io::Result<bool> {
    let descriptor = read_security_descriptor(handle)?;
    let descriptor = descriptor.as_security_descriptor();
    if !security_descriptor_owner_matches(descriptor, identity)? {
        return Ok(false);
    }

    let mut control = 0_u16;
    let mut revision = 0_u32;
    unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) }
        .map_err(windows_error)?;
    if control & SE_DACL_PROTECTED.0 == 0 {
        return Ok(false);
    }

    let mut dacl_present = BOOL(0);
    let mut dacl_defaulted = BOOL(0);
    let mut dacl = ptr::null_mut();
    unsafe {
        GetSecurityDescriptorDacl(
            descriptor,
            &mut dacl_present,
            &mut dacl,
            &mut dacl_defaulted,
        )
    }
    .map_err(windows_error)?;
    if !dacl_present.as_bool() || dacl_defaulted.as_bool() || dacl.is_null() {
        return Ok(false);
    }

    let mut information = ACL_SIZE_INFORMATION::default();
    unsafe {
        GetAclInformation(
            dacl,
            ptr::addr_of_mut!(information).cast(),
            u32::try_from(size_of::<ACL_SIZE_INFORMATION>())
                .expect("Windows ACL information structure fits u32"),
            AclSizeInformation,
        )
    }
    .map_err(windows_error)?;
    if information.AceCount != 1 {
        return Ok(false);
    }

    let mut raw_ace = ptr::null_mut();
    unsafe { GetAce(dacl, 0, &mut raw_ace) }.map_err(windows_error)?;
    if raw_ace.is_null() {
        return Ok(false);
    }
    let ace = unsafe { &*raw_ace.cast::<ACCESS_ALLOWED_ACE>() };
    let expected_flags = if directory {
        (CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE).0
    } else {
        0
    };
    if ace.Header.AceType != ACCESS_ALLOWED_ACE_TYPE
        || u32::from(ace.Header.AceFlags) != expected_flags
        || ace.Header.AceFlags & INHERITED_ACE.0 as u8 != 0
        || ace.Mask != FILE_ALL_ACCESS.0
    {
        return Ok(false);
    }

    let sid_offset = offset_of!(ACCESS_ALLOWED_ACE, SidStart);
    if usize::from(ace.Header.AceSize) <= sid_offset {
        return Ok(false);
    }
    let ace_sid = PSID(ptr::addr_of!(ace.SidStart).cast_mut().cast());
    if !unsafe { IsValidSid(ace_sid) }.as_bool() {
        return Ok(false);
    }
    let sid_length = unsafe { GetLengthSid(ace_sid) } as usize;
    if sid_length > usize::from(ace.Header.AceSize) - sid_offset {
        return Ok(false);
    }
    Ok(unsafe { EqualSid(ace_sid, identity.sid()) }.is_ok())
}

fn security_descriptor_owner_matches(
    descriptor: PSECURITY_DESCRIPTOR,
    identity: &CurrentUser,
) -> io::Result<bool> {
    let mut owner = PSID::default();
    let mut owner_defaulted = BOOL(0);
    unsafe { GetSecurityDescriptorOwner(descriptor, &mut owner, &mut owner_defaulted) }
        .map_err(windows_error)?;
    if owner.is_invalid() || !unsafe { IsValidSid(owner) }.as_bool() {
        return Ok(false);
    }
    Ok(unsafe { EqualSid(owner, identity.sid()) }.is_ok())
}

fn read_security_descriptor(handle: HANDLE) -> io::Result<AlignedBuffer> {
    let requested = OWNER_SECURITY_INFORMATION.0 | DACL_SECURITY_INFORMATION.0;
    let mut required = 0_u32;
    let probe = unsafe {
        GetKernelObjectSecurity(
            handle,
            requested,
            PSECURITY_DESCRIPTOR::default(),
            0,
            &mut required,
        )
    };
    if required == 0 {
        return Err(probe.err().map(windows_error).unwrap_or_else(|| {
            io::Error::other("Windows returned an empty security descriptor size")
        }));
    }

    let descriptor = AlignedBuffer::new(required as usize)?;
    unsafe {
        GetKernelObjectSecurity(
            handle,
            requested,
            descriptor.as_security_descriptor(),
            required,
            &mut required,
        )
    }
    .map_err(windows_error)?;
    Ok(descriptor)
}

fn install_private_dacl(handle: HANDLE, security: &PrivateSecurity) -> io::Result<()> {
    let result = unsafe {
        SetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            PSID::default(),
            PSID::default(),
            Some(security.acl_ptr()),
            None,
        )
    };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result.0 as i32))
    }
}

struct CurrentUser {
    token_information: AlignedBuffer,
}

impl CurrentUser {
    fn load() -> io::Result<Self> {
        let mut token = HANDLE::default();
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
            .map_err(windows_error)?;
        let token = OwnedHandle(token);

        let mut required = 0_u32;
        let probe = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut required) };
        if required < u32::try_from(size_of::<TOKEN_USER>()).expect("TOKEN_USER fits u32") {
            return Err(probe.err().map(windows_error).unwrap_or_else(|| {
                io::Error::other("Windows returned an invalid current-user token size")
            }));
        }

        let mut token_information = AlignedBuffer::new(required as usize)?;
        unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                Some(token_information.as_mut_ptr()),
                required,
                &mut required,
            )
        }
        .map_err(windows_error)?;
        let identity = Self { token_information };
        if !unsafe { IsValidSid(identity.sid()) }.as_bool() {
            return Err(io::Error::other(
                "Windows returned an invalid current-user SID",
            ));
        }
        Ok(identity)
    }

    fn sid(&self) -> PSID {
        let user = unsafe { &*self.token_information.as_ptr().cast::<TOKEN_USER>() };
        user.User.Sid
    }
}

struct PrivateSecurity {
    identity: CurrentUser,
    acl: AlignedBuffer,
    descriptor: Box<SECURITY_DESCRIPTOR>,
}

impl PrivateSecurity {
    fn new(ace_flags: ACE_FLAGS) -> io::Result<Self> {
        let identity = CurrentUser::load()?;
        let sid_length = unsafe { GetLengthSid(identity.sid()) } as usize;
        let acl_length = size_of::<ACL>()
            .checked_add(size_of::<ACCESS_ALLOWED_ACE>() - size_of::<u32>())
            .and_then(|length| length.checked_add(sid_length))
            .ok_or_else(|| io::Error::other("Windows owner-only ACL length overflowed"))?;
        let acl_length = u32::try_from(acl_length)
            .map_err(|_| io::Error::other("Windows owner-only ACL was too large"))?;
        let mut acl = AlignedBuffer::new(acl_length as usize)?;
        unsafe { InitializeAcl(acl.as_mut_ptr().cast(), acl_length, ACL_REVISION) }
            .map_err(windows_error)?;
        unsafe {
            AddAccessAllowedAceEx(
                acl.as_mut_ptr().cast(),
                ACL_REVISION,
                ace_flags,
                FILE_ALL_ACCESS.0,
                identity.sid(),
            )
        }
        .map_err(windows_error)?;

        let mut descriptor = Box::new(SECURITY_DESCRIPTOR::default());
        let descriptor_ptr = PSECURITY_DESCRIPTOR(
            ptr::from_mut::<SECURITY_DESCRIPTOR>(descriptor.as_mut()).cast::<c_void>(),
        );
        unsafe { InitializeSecurityDescriptor(descriptor_ptr, SECURITY_DESCRIPTOR_REVISION) }
            .map_err(windows_error)?;
        unsafe { SetSecurityDescriptorOwner(descriptor_ptr, identity.sid(), BOOL(0)) }
            .map_err(windows_error)?;
        unsafe {
            SetSecurityDescriptorDacl(descriptor_ptr, BOOL(1), Some(acl.as_ptr().cast()), BOOL(0))
        }
        .map_err(windows_error)?;
        unsafe {
            SetSecurityDescriptorControl(descriptor_ptr, SE_DACL_PROTECTED, SE_DACL_PROTECTED)
        }
        .map_err(windows_error)?;

        Ok(Self {
            identity,
            acl,
            descriptor,
        })
    }

    fn acl_ptr(&self) -> *const ACL {
        self.acl.as_ptr().cast()
    }

    fn descriptor_ptr(&self) -> *const c_void {
        ptr::from_ref::<SECURITY_DESCRIPTOR>(self.descriptor.as_ref()).cast()
    }

    fn attributes(&mut self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
                .expect("SECURITY_ATTRIBUTES fits u32"),
            lpSecurityDescriptor: ptr::from_mut::<SECURITY_DESCRIPTOR>(self.descriptor.as_mut())
                .cast(),
            bInheritHandle: BOOL(0),
        }
    }
}

struct AlignedBuffer {
    words: Vec<usize>,
}

impl AlignedBuffer {
    fn new(bytes: usize) -> io::Result<Self> {
        let words = bytes
            .checked_add(size_of::<usize>() - 1)
            .ok_or_else(|| io::Error::other("Windows security buffer length overflowed"))?
            / size_of::<usize>();
        Ok(Self {
            words: vec![0; words.max(1)],
        })
    }

    fn as_ptr(&self) -> *const c_void {
        self.words.as_ptr().cast()
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.words.as_mut_ptr().cast()
    }

    fn as_security_descriptor(&self) -> PSECURITY_DESCRIPTOR {
        PSECURITY_DESCRIPTOR(self.as_ptr().cast_mut())
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn wide_path(path: &Path) -> io::Result<Vec<u16>> {
    let mut path: Vec<u16> = path.as_os_str().encode_wide().collect();
    if path.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a Windows token path cannot contain NUL",
        ));
    }
    path.push(0);
    Ok(path)
}

fn windows_error(error: WindowsError) -> io::Error {
    let hresult = error.code().0 as u32;
    if hresult & 0xffff_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((hresult & 0xffff) as i32)
    } else {
        io::Error::other(error.to_string())
    }
}

fn ntstatus_error(status: NTSTATUS) -> io::Error {
    if status == STATUS_REPARSE_POINT_ENCOUNTERED {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to traverse a reparse point in a Windows token child operation",
        )
    } else {
        io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(status) } as i32)
    }
}

#[cfg(test)]
pub(super) fn harden_file_for_test(path: &Path) -> io::Result<()> {
    let file = open_path_for_acl_update(path)?;
    let security = PrivateSecurity::new(ACE_FLAGS(0))?;
    if !handle_has_expected_kind(handle_for(&file), false)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the test token path is not an ordinary file",
        ));
    }
    install_private_dacl(handle_for(&file), &security)
}

#[cfg(test)]
pub(super) fn path_has_private_permissions_for_test(
    path: &Path,
    directory: bool,
) -> io::Result<bool> {
    let file = if directory {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
        options.open(path)?
    } else {
        open_path_no_follow(
            path,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
        )?
    };
    let identity = CurrentUser::load()?;
    handle_is_exact_private_path(handle_for(&file), directory, &identity)
}

#[cfg(test)]
pub(super) fn install_permissive_null_dacl_for_test(path: &Path) -> io::Result<()> {
    let file = open_path_for_acl_update(path)?;
    let result = unsafe {
        SetSecurityInfo(
            handle_for(&file),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            PSID::default(),
            PSID::default(),
            None,
            None,
        )
    };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result.0 as i32))
    }
}

#[cfg(test)]
pub(super) fn enable_case_sensitive_directory_for_test(path: &Path) -> io::Result<()> {
    #[repr(C)]
    struct FileCaseSensitiveInformation {
        flags: u32,
    }

    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(FILE_WRITE_ATTRIBUTES.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
        .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
    let directory = options.open(path)?;
    let information = FileCaseSensitiveInformation { flags: 1 };
    unsafe {
        SetFileInformationByHandle(
            handle_for(&directory),
            FileCaseSensitiveInfo,
            ptr::addr_of!(information).cast(),
            u32::try_from(size_of::<FileCaseSensitiveInformation>())
                .expect("Windows case-sensitive information fits u32"),
        )
    }
    .map_err(windows_error)
}

#[cfg(test)]
pub(super) fn number_of_links_for_test(path: &Path) -> io::Result<u32> {
    let file = open_path_no_follow(
        path,
        FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
    )?;
    let standard: FILE_STANDARD_INFO = query_file_information(handle_for(&file), FileStandardInfo)?;
    Ok(standard.NumberOfLinks)
}

#[cfg(test)]
pub(super) fn path_is_mount_point_for_test(path: &Path) -> io::Result<bool> {
    let file = open_path_no_follow(
        path,
        FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
    )?;
    let information: FILE_ATTRIBUTE_TAG_INFO =
        query_file_information(handle_for(&file), FileAttributeTagInfo)?;
    Ok(
        information.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
            && information.ReparseTag == IO_REPARSE_TAG_MOUNT_POINT,
    )
}

#[cfg(test)]
pub(super) fn resolved_directory_paths_have_same_identity_for_test(
    left: &Path,
    right: &Path,
) -> io::Result<bool> {
    let left = open_token_directory_for_validation(left)?;
    let right = open_token_directory_for_validation(right)?;
    Ok(
        query_file_information::<FILE_ID_INFO>(handle_for(&left), FileIdInfo)?
            == query_file_information::<FILE_ID_INFO>(handle_for(&right), FileIdInfo)?,
    )
}

#[cfg(test)]
pub(super) fn create_directory_junction_for_test(link: &Path, target: &Path) -> io::Result<()> {
    #[repr(C)]
    struct MountPointReparseData {
        tag: u32,
        data_length: u16,
        reserved: u16,
        substitute_name_offset: u16,
        substitute_name_length: u16,
        print_name_offset: u16,
        print_name_length: u16,
        path_buffer: [u16; 1],
    }

    const REPARSE_DATA_HEADER_BYTES: usize = 8;

    let target = std::path::absolute(target)?;
    if !target.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a Windows test junction target must be absolute",
        ));
    }
    let print_name: Vec<u16> = target.as_os_str().encode_wide().collect();
    let mut substitute_name: Vec<u16> = OsStr::new(r"\??\").encode_wide().collect();
    substitute_name.extend_from_slice(&print_name);
    let print_name_offset = substitute_name
        .len()
        .checked_add(1)
        .and_then(|units| units.checked_mul(size_of::<u16>()))
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "junction target is too long")
        })?;
    let path_units = substitute_name
        .len()
        .checked_add(1)
        .and_then(|units| units.checked_add(print_name.len()))
        .and_then(|units| units.checked_add(1))
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "junction target is too long")
        })?;
    let buffer_bytes = offset_of!(MountPointReparseData, path_buffer)
        .checked_add(path_units.checked_mul(size_of::<u16>()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "junction target is too long")
        })?)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "junction target is too long")
        })?;
    let data_length = buffer_bytes
        .checked_sub(REPARSE_DATA_HEADER_BYTES)
        .and_then(|length| u16::try_from(length).ok())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "junction target is too long")
        })?;
    let substitute_name_length = substitute_name
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u16::try_from(length).ok())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "junction target is too long")
        })?;
    let print_name_offset = u16::try_from(print_name_offset)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "junction target is too long"))?;
    let print_name_length = print_name
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u16::try_from(length).ok())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "junction target is too long")
        })?;

    std::fs::create_dir(link)?;
    let result = (|| {
        let mut options = OpenOptions::new();
        options
            .access_mode(FILE_GENERIC_WRITE.0)
            .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
            .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
        let junction = options.open(link)?;
        let mut buffer = AlignedBuffer::new(buffer_bytes)?;
        let information = buffer.as_mut_ptr().cast::<MountPointReparseData>();
        let mut bytes_returned = 0u32;
        unsafe {
            (*information).tag = IO_REPARSE_TAG_MOUNT_POINT;
            (*information).data_length = data_length;
            (*information).substitute_name_offset = 0;
            (*information).substitute_name_length = substitute_name_length;
            (*information).print_name_offset = print_name_offset;
            (*information).print_name_length = print_name_length;
            let path_buffer = ptr::addr_of_mut!((*information).path_buffer).cast::<u16>();
            ptr::copy_nonoverlapping(substitute_name.as_ptr(), path_buffer, substitute_name.len());
            ptr::copy_nonoverlapping(
                print_name.as_ptr(),
                path_buffer.add(usize::from(print_name_offset) / size_of::<u16>()),
                print_name.len(),
            );
            DeviceIoControl(
                handle_for(&junction),
                FSCTL_SET_REPARSE_POINT,
                Some(information.cast()),
                u32::try_from(buffer_bytes).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "junction target is too long")
                })?,
                None,
                0,
                Some(&mut bytes_returned),
                None,
            )
        }
        .map_err(windows_error)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir(link);
    }
    result
}
