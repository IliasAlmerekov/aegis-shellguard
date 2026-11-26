use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    Active,
    Unavailable,
    NotAttempted,
    #[default]
    NotConfigured,
}

impl SandboxStatus {
    pub fn as_legacy_active(self) -> Option<bool> {
        match self {
            SandboxStatus::Active => Some(true),
            SandboxStatus::Unavailable => Some(false),
            SandboxStatus::NotAttempted | SandboxStatus::NotConfigured => None,
        }
    }
}

impl From<Option<bool>> for SandboxStatus {
    fn from(available: Option<bool>) -> Self {
        match available {
            None => SandboxStatus::NotConfigured,
            Some(true) => SandboxStatus::Active,
            Some(false) => SandboxStatus::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SandboxStatus;

    #[test]
    fn default_is_not_configured() {
        assert_eq!(SandboxStatus::default(), SandboxStatus::NotConfigured);
    }

    #[test]
    fn maps_from_legacy_availability_tristate() {
        assert_eq!(SandboxStatus::from(None), SandboxStatus::NotConfigured);
        assert_eq!(SandboxStatus::from(Some(true)), SandboxStatus::Active);
        assert_eq!(SandboxStatus::from(Some(false)), SandboxStatus::Unavailable);
    }

    #[test]
    fn legacy_active_round_trips_representable_statuses_through_from() {
        for status in [
            SandboxStatus::Active,
            SandboxStatus::Unavailable,
            SandboxStatus::NotConfigured,
        ] {
            assert_eq!(SandboxStatus::from(status.as_legacy_active()), status);
        }
    }

    #[test]
    fn serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&SandboxStatus::NotAttempted).unwrap(),
            "\"not_attempted\""
        );
        assert_eq!(
            serde_json::to_string(&SandboxStatus::NotConfigured).unwrap(),
            "\"not_configured\""
        );
        assert_eq!(
            serde_json::to_string(&SandboxStatus::Unavailable).unwrap(),
            "\"unavailable\""
        );
    }

    #[test]
    fn deserializes_only_snake_case_not_configured() {
        assert_eq!(
            serde_json::from_str::<SandboxStatus>("\"not_configured\"").unwrap(),
            SandboxStatus::NotConfigured
        );
        assert!(serde_json::from_str::<SandboxStatus>("\"NotConfigured\"").is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CeilingLayer {
    Global,
    Project,
    Computed,
}

impl CeilingLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
            Self::Computed => "computed",
        }
    }
}

#[cfg(test)]
mod ceiling_layer_tests {
    use super::CeilingLayer;

    #[test]
    fn names_are_stable_and_match_the_serialised_form() {
        for (layer, name) in [
            (CeilingLayer::Global, "global"),
            (CeilingLayer::Project, "project"),
            (CeilingLayer::Computed, "computed"),
        ] {
            assert_eq!(layer.as_str(), name);
            assert_eq!(
                serde_json::to_string(&layer).unwrap(),
                format!("\"{name}\"")
            );
        }
    }
}
