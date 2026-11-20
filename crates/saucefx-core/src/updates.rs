//! User-visible state for an explicitly requested application update.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    #[default]
    Ready,
    Checking,
    Installing(u32),
    UpToDate,
    RestartRequired,
    Failed(String),
    Unsupported,
}

impl UpdateStatus {
    pub fn message(&self) -> String {
        match self {
            Self::Ready => "Check for a newer version of SauceFX.".into(),
            Self::Checking => "Checking for updates…".into(),
            Self::Installing(percent) => format!("Updating SauceFX… {}%", percent.min(&100)),
            Self::UpToDate => "SauceFX is up to date.".into(),
            Self::RestartRequired => "Update installed. Restart SauceFX to use it.".into(),
            Self::Failed(message) => message.clone(),
            Self::Unsupported => "Updates for this package are managed outside SauceFX.".into(),
        }
    }

    pub fn action_label(&self) -> &'static str {
        match self {
            Self::Checking | Self::Installing(_) => "Working…",
            Self::RestartRequired => "Restart now",
            Self::Failed(_) => "Try again",
            _ => "Check & update",
        }
    }

    pub fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing(_))
    }

    pub fn action_enabled(&self) -> bool {
        !self.busy() && !matches!(self, Self::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_user_action_starts_an_update() {
        let initial = UpdateStatus::default();
        assert_eq!(initial, UpdateStatus::Ready);
        assert!(!initial.busy());
        assert!(initial.action_enabled());
        assert_eq!(initial.action_label(), "Check & update");
    }

    #[test]
    fn working_states_prevent_duplicate_requests() {
        for status in [UpdateStatus::Checking, UpdateStatus::Installing(42)] {
            assert!(status.busy());
            assert!(!status.action_enabled());
            assert_eq!(status.action_label(), "Working…");
        }
        assert!(UpdateStatus::Installing(142).message().ends_with("100%"));
    }

    #[test]
    fn completed_and_empty_transactions_are_distinct() {
        assert_eq!(UpdateStatus::RestartRequired.action_label(), "Restart now");
        assert_eq!(UpdateStatus::UpToDate.action_label(), "Check & update");
        assert_ne!(
            UpdateStatus::RestartRequired.message(),
            UpdateStatus::UpToDate.message()
        );
        assert!(UpdateStatus::RestartRequired.action_enabled());
    }

    #[test]
    fn failure_can_be_retried_without_reporting_success() {
        let status = UpdateStatus::Failed("Could not reach the update service.".into());
        assert_eq!(status.action_label(), "Try again");
        assert!(status.action_enabled());
        assert!(!status.message().contains("up to date"));
        assert!(!status.message().contains("installed"));
        assert!(!UpdateStatus::Unsupported.action_enabled());
    }
}
