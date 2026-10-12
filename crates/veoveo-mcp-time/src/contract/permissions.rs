//! Time's permissions. Each names one action on one resource type, with one access
//! kind and its risks; installation roles grant them and the Gateway forwards the
//! granted ones with every request.
use veoveo_mcp_ext_registration::{Access, PermissionDefinition, Risk};
use veoveo_types::{PermissionName, Vocabulary};

/// ```compile_fail
/// use veoveo_mcp_time::contract::TimePermission;
/// fn requires(_: TimePermission) {}
/// requires("time.events.write");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
pub enum TimePermission {
    /// Resolve and convert times, assess the clock and read Time's resources.
    #[vocabulary(rename = "time.authorities.read")]
    AuthoritiesRead,
    /// Evaluate windows and expand operational calendars into windows.
    #[vocabulary(rename = "time.schedules.expand")]
    SchedulesExpand,
    /// Validate mission timelines.
    #[vocabulary(rename = "time.timelines.validate")]
    TimelinesValidate,
    /// Create and cancel the caller's own events.
    #[vocabulary(rename = "time.events.write")]
    EventsWrite,
    /// Register authority sources, acquire releases and activate them.
    #[vocabulary(rename = "time.authorities.administer")]
    AuthoritiesAdminister,
}

impl PermissionDefinition for TimePermission {
    fn resource(self) -> &'static str {
        match self {
            Self::AuthoritiesRead | Self::AuthoritiesAdminister => "time.authority",
            Self::SchedulesExpand => "time.schedule",
            Self::TimelinesValidate => "time.timeline",
            Self::EventsWrite => "time.event",
        }
    }

    fn access(self) -> Access {
        match self {
            Self::AuthoritiesRead | Self::SchedulesExpand | Self::TimelinesValidate => Access::Read,
            Self::EventsWrite | Self::AuthoritiesAdminister => Access::Write,
        }
    }

    fn risks(self) -> &'static [Risk] {
        match self {
            Self::AuthoritiesAdminister => &[Risk::Administer],
            _ => &[],
        }
    }
}

impl TimePermission {
    /// The permission's wire name, as declared and as granted.
    pub fn name(self) -> PermissionName {
        PermissionName::parse(self.as_str()).expect("declared Time permission spelling")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_permission_is_named_in_time_and_acts_on_a_time_resource() {
        for permission in TimePermission::ALL {
            let name = permission.name();
            assert!(name.as_str().starts_with("time."), "{name}");
            assert!(permission.resource().starts_with("time."));
            assert_eq!(name.as_str().split('.').count(), 3, "{name}");
        }
    }

    #[test]
    fn only_administration_carries_a_risk_and_reads_are_read_access() {
        assert_eq!(TimePermission::AuthoritiesRead.access(), Access::Read);
        assert_eq!(TimePermission::EventsWrite.access(), Access::Write);
        assert_eq!(
            TimePermission::AuthoritiesAdminister.risks(),
            &[Risk::Administer]
        );
        for permission in TimePermission::ALL {
            if *permission != TimePermission::AuthoritiesAdminister {
                assert!(permission.risks().is_empty());
            }
        }
    }
}
