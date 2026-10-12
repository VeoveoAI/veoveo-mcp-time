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
    /// Read the tenant's authority sources and acquisitions.
    #[vocabulary(rename = "time.authorities.inspect")]
    AuthoritiesInspect,
    /// Publish operational calendar versions for the tenant.
    #[vocabulary(rename = "time.calendars.write")]
    CalendarsWrite,
    /// Publish mission epoch versions for the tenant.
    #[vocabulary(rename = "time.epochs.write")]
    EpochsWrite,
    /// Replace the tenant's clock quality policy.
    #[vocabulary(rename = "time.clock-policies.write")]
    ClockPoliciesWrite,
}

impl PermissionDefinition for TimePermission {
    fn resource(self) -> &'static str {
        match self {
            Self::AuthoritiesRead | Self::AuthoritiesAdminister | Self::AuthoritiesInspect => {
                "time.authority"
            }
            Self::SchedulesExpand => "time.schedule",
            Self::TimelinesValidate => "time.timeline",
            Self::EventsWrite => "time.event",
            Self::CalendarsWrite => "time.calendar",
            Self::EpochsWrite => "time.epoch",
            Self::ClockPoliciesWrite => "time.clock-policy",
        }
    }

    fn access(self) -> Access {
        match self {
            Self::AuthoritiesRead
            | Self::SchedulesExpand
            | Self::TimelinesValidate
            | Self::AuthoritiesInspect => Access::Read,
            Self::EventsWrite
            | Self::AuthoritiesAdminister
            | Self::CalendarsWrite
            | Self::EpochsWrite
            | Self::ClockPoliciesWrite => Access::Write,
        }
    }

    fn risks(self) -> &'static [Risk] {
        match self {
            Self::AuthoritiesAdminister
            | Self::AuthoritiesInspect
            | Self::CalendarsWrite
            | Self::EpochsWrite
            | Self::ClockPoliciesWrite => &[Risk::Administer],
            Self::AuthoritiesRead
            | Self::SchedulesExpand
            | Self::TimelinesValidate
            | Self::EventsWrite => &[],
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
    fn tenant_administration_carries_the_administer_risk_and_reads_are_read_access() {
        use TimePermission::*;
        assert_eq!(AuthoritiesRead.access(), Access::Read);
        assert_eq!(AuthoritiesInspect.access(), Access::Read);
        assert_eq!(EventsWrite.access(), Access::Write);
        for permission in TimePermission::ALL {
            let administers = matches!(
                permission,
                AuthoritiesAdminister
                    | AuthoritiesInspect
                    | CalendarsWrite
                    | EpochsWrite
                    | ClockPoliciesWrite
            );
            assert_eq!(
                permission.risks(),
                if administers {
                    &[Risk::Administer][..]
                } else {
                    &[][..]
                },
                "{}",
                permission.name()
            );
        }
    }
}
