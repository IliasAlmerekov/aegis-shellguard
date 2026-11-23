use std::ffi::OsString;
use std::path::PathBuf;

/// Resolve HOME or USERPROFILE, ignoring an empty selected value.
pub fn home_dir() -> Option<PathBuf> {
    home_dir_from(std::env::var_os("HOME"), std::env::var_os("USERPROFILE"))
}

fn home_dir_from(home: Option<OsString>, userprofile: Option<OsString>) -> Option<PathBuf> {
    home.or(userprofile)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_home_and_userprofile() {
        let cases = [
            (None, None, None),
            (Some(""), Some("/profile"), None),
            (None, Some(""), None),
            (Some("/home"), Some("/profile"), Some("/home")),
            (Some("/home"), None, Some("/home")),
            (None, Some("/profile"), Some("/profile")),
        ];
        for (home, profile, expected) in cases {
            assert_eq!(
                home_dir_from(home.map(OsString::from), profile.map(OsString::from)),
                expected.map(PathBuf::from)
            );
        }
    }

    #[test]
    fn reads_current_environment() {
        assert_eq!(
            home_dir(),
            home_dir_from(std::env::var_os("HOME"), std::env::var_os("USERPROFILE"))
        );
    }
}
