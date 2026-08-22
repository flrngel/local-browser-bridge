use std::env;
use std::path::PathBuf;

/// Returns the interactive user's absolute profile directory.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let value = env::var_os("USERPROFILE");
    #[cfg(not(target_os = "windows"))]
    let value = env::var_os("HOME");

    absolute_path(value)
}

fn absolute_path(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let path = PathBuf::from(value?);
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return None;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_nonempty_absolute_profile_paths() {
        assert!(absolute_path(None).is_none());
        assert!(absolute_path(Some("".into())).is_none());
        assert!(absolute_path(Some("relative".into())).is_none());

        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            absolute_path(Some("/Users/example".into())),
            Some(PathBuf::from("/Users/example"))
        );
        #[cfg(target_os = "windows")]
        assert_eq!(
            absolute_path(Some(r"C:\Users\example".into())),
            Some(PathBuf::from(r"C:\Users\example"))
        );
    }
}
