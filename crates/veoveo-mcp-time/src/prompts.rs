use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub enum TimePrompt {
    ResolveOperationalTime,
    ExpandOperationalCalendar,
    ValidateMissionTimeline,
}

impl TimePrompt {
    pub const ALL: [Self; 3] = [
        Self::ResolveOperationalTime,
        Self::ExpandOperationalCalendar,
        Self::ValidateMissionTimeline,
    ];
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::ResolveOperationalTime => "resolve_operational_time",
            Self::ExpandOperationalCalendar => "expand_operational_calendar",
            Self::ValidateMissionTimeline => "validate_mission_timeline",
        }
    }
    pub fn definition(self) -> Prompt {
        let (title, description, arguments) = match self {
            Self::ResolveOperationalTime => (
                "Resolve operational time",
                "Resolve a time expression against active authority releases.",
                vec![
                    required(
                        "expression",
                        "RFC 3339/9557, military DTG, GPS, Unix, TAI, Julian TAI, or mission-relative expression.",
                    ),
                    optional("zone_id", "IANA zone required for civil local time."),
                ],
            ),
            Self::ExpandOperationalCalendar => (
                "Expand operational calendar",
                "Expand a versioned operational calendar over a set time horizon.",
                vec![
                    required("calendar_id", "Visible calendar id."),
                    required("version", "Calendar version."),
                    required("horizon", "Half-open TAI horizon."),
                ],
            ),
            Self::ValidateMissionTimeline => (
                "Validate mission timeline",
                "Validate precedence and separation constraints for mission points.",
                vec![required(
                    "timeline",
                    "Named points and temporal constraints.",
                )],
            ),
        };
        Prompt::new(self.name(), Some(description), Some(arguments)).with_title(title)
    }
    pub fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Arguments {
            expression: Option<String>,
            zone_id: Option<String>,
            calendar_id: Option<String>,
            version: Option<String>,
            horizon: Option<String>,
            timeline: Option<String>,
        }
        let arguments: Arguments =
            serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let text = match self {
            Self::ResolveOperationalTime => format!(
                "Resolve the time `{}`{} as the skill skill://time/resolve-a-time/SKILL.md says.",
                required_value(arguments.expression, "expression")?,
                arguments
                    .zone_id
                    .map_or_else(String::new, |zone| format!(" in IANA zone `{zone}`"))
            ),
            Self::ExpandOperationalCalendar => format!(
                "Expand version {1} of the calendar `{0}` over the horizon {2} as the skill skill://time/expand-a-calendar/SKILL.md says.",
                required_value(arguments.calendar_id, "calendar_id")?,
                required_value(arguments.version, "version")?,
                required_value(arguments.horizon, "horizon")?
            ),
            Self::ValidateMissionTimeline => format!(
                "Validate the timeline `{}` as the skill skill://time/validate-a-timeline/SKILL.md says.",
                required_value(arguments.timeline, "timeline")?
            ),
        };
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            text,
        )]))
    }
}

fn required(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(true)
}
fn optional(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(false)
}
fn required_value(value: Option<String>, name: &str) -> Result<String, McpError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| McpError::invalid_params(format!("missing prompt argument `{name}`"), None))
}
