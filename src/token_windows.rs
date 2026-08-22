use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io;
use std::mem::{offset_of, size_of};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _};
use std::path::Path;
use std::ptr;

use windows::Win32::Foundation::{BOOL, CloseHandle, ERROR_SUCCESS, HANDLE};
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
    CREATE_NEW, CreateFileW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_WRITE, FILE_INFO_BY_HANDLE_CLASS, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO,
    FileAttributeTagInfo, FileStandardInfo, GetFileInformationByHandleEx,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, READ_CONTROL, WRITE_DAC,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{Error as WindowsError, PCWSTR};

const SECURITY_DESCRIPTOR_REVISION: u32 = 1;
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const MAX_TOKEN_FILE_BYTES: i64 = 128;

pub(super) fn open_private_token_file(path: &Path) -> io::Result<Option<File>> {
    let file = open_path_no_follow(
        path,
        FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
    )?;
    let identity = CurrentUser::load()?;
    if !handle_is_exact_private_path(handle_for(&file), false, &identity)? {
        return Ok(None);
    }
    Ok(Some(file))
}

pub(super) fn token_path_has_private_permissions(path: &Path) -> io::Result<bool> {
    let file = open_path_no_follow(
        path,
        FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
    )?;
    let identity = CurrentUser::load()?;
    handle_is_exact_private_path(handle_for(&file), false, &identity)
}

pub(super) fn create_private_token_file(path: &Path) -> io::Result<File> {
    let mut security = PrivateSecurity::new(ACE_FLAGS(0))?;
    let attributes = security.attributes();
    let path = wide_path(path)?;
    let desired_access = FILE_GENERIC_WRITE.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0;
    let flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT;
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            desired_access,
            FILE_SHARE_MODE(0),
            Some(&attributes),
            CREATE_NEW,
            flags,
            HANDLE::default(),
        )
    }
    .map_err(windows_error)?;
    let file = unsafe { File::from_raw_handle(handle.0) };
    if !handle_is_exact_private_path(handle_for(&file), false, &security.identity)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Windows did not create the token file with its protected owner-only DACL",
        ));
    }
    Ok(file)
}

pub(super) fn replace_token_file(source: &Path, destination: &Path) -> io::Result<()> {
    ensure_safe_replacement_target(destination)?;
    let source = wide_path(source)?;
    let destination = wide_path(destination)?;
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(windows_error)
}

fn ensure_safe_replacement_target(path: &Path) -> io::Result<()> {
    let file = match open_path_for_replacement_check(path) {
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

pub(super) fn harden_token_directory(path: &Path) -> io::Result<()> {
    let file = open_path_for_acl_update(path)?;
    let security = PrivateSecurity::new(CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE)?;
    let handle = handle_for(&file);
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
    Ok(())
}

fn open_path_no_follow(path: &Path, flags: FILE_FLAGS_AND_ATTRIBUTES) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).custom_flags(flags.0);
    options.open(path)
}

fn open_path_for_replacement_check(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(FILE_READ_ATTRIBUTES.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
        .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
    options.open(path)
}

fn open_path_for_acl_update(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(READ_CONTROL.0 | WRITE_DAC.0 | FILE_READ_ATTRIBUTES.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
        .custom_flags((FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).0);
    options.open(path)
}

fn handle_for(file: &File) -> HANDLE {
    HANDLE(file.as_raw_handle())
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
