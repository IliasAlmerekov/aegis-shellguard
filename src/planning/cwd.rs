use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CwdState {
    Resolved(PathBuf),
    Unavailable,
}

impl CwdState {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Resolved(path) => Some(path),
            Self::Unavailable => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_returns_resolved_path() {
        let path = PathBuf::from("/srv/project");
        let cwd_state = CwdState::Resolved(path.clone());

        assert_eq!(cwd_state.path(), Some(path.as_path()));
    }

    #[test]
    fn path_is_none_when_unavailable() {
        assert_eq!(CwdState::Unavailable.path(), None);
    }
}
