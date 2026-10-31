use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::AuthorityBinding;

const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// The largest integer a JavaScript number holds exactly, `2^53 - 1`. Every integer Time
/// admits or emits in JSON lies within `-JSON_INTEGER_MAX..=JSON_INTEGER_MAX`, the
/// I-JSON (RFC 7493) exact range.
pub const JSON_INTEGER_MAX: i64 = 9_007_199_254_740_991;

/// Whether `value` lies within the JSON integer range.
pub fn json_integer(value: i64) -> bool {
    (-JSON_INTEGER_MAX..=JSON_INTEGER_MAX).contains(&value)
}

/// Serde adapters for an `i64` field held within the JSON integer range: decoding refuses
/// a value outside it and encoding never emits one.
pub(crate) mod json_i64 {
    use serde::{Deserialize, Deserializer, Serializer, de, ser};

    pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        if !super::json_integer(*value) {
            return Err(ser::Error::custom("integer outside -(2^53 - 1)..=2^53 - 1"));
        }
        serializer.serialize_i64(*value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        let value = i64::deserialize(deserializer)?;
        if !super::json_integer(value) {
            return Err(de::Error::custom("integer outside -(2^53 - 1)..=2^53 - 1"));
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeCoordinateError {
    InvalidNanosecond,
    SecondsOverflow,
}

impl fmt::Display for TimeCoordinateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidNanosecond => "subsecond nanoseconds must be in 0..=999999999",
            Self::SecondsOverflow => "time coordinate seconds lie outside -(2^53 - 1)..=2^53 - 1",
        })
    }
}

impl Error for TimeCoordinateError {}

/// Splits signed total nanoseconds into JSON-range seconds and a subsecond, by
/// Euclidean division.
fn split_total(total: i128) -> Result<(i64, SubsecondNanoseconds), TimeCoordinateError> {
    let seconds = i64::try_from(total.div_euclid(NANOS_PER_SECOND))
        .ok()
        .filter(|seconds| json_integer(*seconds))
        .ok_or(TimeCoordinateError::SecondsOverflow)?;
    let nanosecond = SubsecondNanoseconds::new(total.rem_euclid(NANOS_PER_SECOND) as u32)?;
    Ok((seconds, nanosecond))
}

/// A bound on a time coordinate's error: bounded nanoseconds within the JSON integer
/// range, or unbounded when its source states no finite bound, as an unmeasured clock
/// does. Every bounded value orders below unbounded.
///
/// The bound is checked at construction:
/// ```compile_fail
/// use veoveo_mcp_time::NanosecondBound;
/// let bound = NanosecondBound(Some(u64::MAX));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "NanosecondBoundWire", into = "NanosecondBoundWire")]
pub struct NanosecondBound(Option<u64>);

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[schemars(rename = "NanosecondBound")]
enum NanosecondBoundWire {
    Bounded {
        #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
        nanoseconds: u64,
    },
    Unbounded {},
}

impl NanosecondBound {
    pub const ZERO: Self = Self(Some(0));
    pub const UNBOUNDED: Self = Self(None);
    /// The largest bounded value, `2^53 - 1` nanoseconds.
    pub const MAX_NANOSECONDS: u64 = JSON_INTEGER_MAX as u64;

    pub fn bounded(nanoseconds: u64) -> Result<Self, super::admission::TimeValueError> {
        if nanoseconds > Self::MAX_NANOSECONDS {
            return Err(super::admission::TimeValueError(
                "a bounded uncertainty lies in 0..=2^53 - 1 nanoseconds",
            ));
        }
        Ok(Self(Some(nanoseconds)))
    }

    /// The bound in nanoseconds; `None` when unbounded.
    pub fn nanoseconds(self) -> Option<u64> {
        self.0
    }

    pub fn is_unbounded(self) -> bool {
        self.0.is_none()
    }

    /// The sum of two bounds: unbounded when either is, `None` when two bounded values
    /// sum beyond the JSON range.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        match (self.0, other.0) {
            (Some(left), Some(right)) => left
                .checked_add(right)
                .filter(|sum| *sum <= Self::MAX_NANOSECONDS)
                .map(|sum| Self(Some(sum))),
            _ => Some(Self::UNBOUNDED),
        }
    }
}

impl Default for NanosecondBound {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Ord for NanosecondBound {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.0, other.0) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    }
}

impl PartialOrd for NanosecondBound {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl TryFrom<NanosecondBoundWire> for NanosecondBound {
    type Error = super::admission::TimeValueError;
    fn try_from(value: NanosecondBoundWire) -> Result<Self, Self::Error> {
        match value {
            NanosecondBoundWire::Bounded { nanoseconds } => Self::bounded(nanoseconds),
            NanosecondBoundWire::Unbounded {} => Ok(Self::UNBOUNDED),
        }
    }
}

impl From<NanosecondBound> for NanosecondBoundWire {
    fn from(value: NanosecondBound) -> Self {
        match value.0 {
            Some(nanoseconds) => Self::Bounded { nanoseconds },
            None => Self::Unbounded {},
        }
    }
}

impl JsonSchema for NanosecondBound {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "NanosecondBound".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        NanosecondBoundWire::json_schema(generator)
    }
}

/// A signed span of time: JSON-range seconds and a nanosecond within that second, split
/// by Euclidean division as instants are, so `-1.5 s` is `{seconds: -2, nanosecond:
/// 500000000}`. Spans order by their total length.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeSpan {
    #[serde(
        serialize_with = "json_i64::serialize",
        deserialize_with = "json_i64::deserialize"
    )]
    #[schemars(range(min = -9_007_199_254_740_991_i64, max = 9_007_199_254_740_991_i64))]
    seconds: i64,
    #[serde(default)]
    nanosecond: SubsecondNanoseconds,
}

impl TimeSpan {
    pub const ZERO: Self = Self {
        seconds: 0,
        nanosecond: SubsecondNanoseconds::ZERO,
    };

    pub fn new(
        seconds: i64,
        nanosecond: SubsecondNanoseconds,
    ) -> Result<Self, TimeCoordinateError> {
        if !json_integer(seconds) {
            return Err(TimeCoordinateError::SecondsOverflow);
        }
        Ok(Self {
            seconds,
            nanosecond,
        })
    }

    pub fn from_total_nanoseconds(total: i128) -> Result<Self, TimeCoordinateError> {
        let (seconds, nanosecond) = split_total(total)?;
        Ok(Self {
            seconds,
            nanosecond,
        })
    }

    pub fn seconds(&self) -> i64 {
        self.seconds
    }

    pub fn nanosecond(&self) -> SubsecondNanoseconds {
        self.nanosecond
    }

    pub fn total_nanoseconds(&self) -> i128 {
        i128::from(self.seconds) * NANOS_PER_SECOND + i128::from(self.nanosecond.get())
    }

    pub fn is_negative(&self) -> bool {
        self.seconds < 0
    }
}

/// Nanoseconds within one integral second, distinct from a duration.
/// ```compile_fail
/// use veoveo_mcp_time::TimeInstant;
/// fn cannot_overflow_second(instant: &mut TimeInstant) {
///     instant.nanosecond = 1_000_000_000;
/// }
/// ```
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "u32", into = "u32")]
pub struct SubsecondNanoseconds(u32);

impl SubsecondNanoseconds {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(999_999_999);

    pub fn new(value: u32) -> Result<Self, TimeCoordinateError> {
        if value > Self::MAX.0 {
            return Err(TimeCoordinateError::InvalidNanosecond);
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for SubsecondNanoseconds {
    type Error = TimeCoordinateError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SubsecondNanoseconds> for u32 {
    fn from(value: SubsecondNanoseconds) -> Self {
        value.0
    }
}

impl JsonSchema for SubsecondNanoseconds {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SubsecondNanoseconds".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"integer", "minimum":0, "maximum":999_999_999})
    }
}

/// An instant as TAI seconds and nanoseconds, bound to the timezone and leap-second
/// releases that interpreted it. `resolve_time` returns one as its `instant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TimeInstant {
    /// Integral TAI seconds elapsed since 1970-01-01 00:00:00 TAI.
    #[serde(
        serialize_with = "json_i64::serialize",
        deserialize_with = "json_i64::deserialize"
    )]
    #[schemars(range(min = -9_007_199_254_740_991_i64, max = 9_007_199_254_740_991_i64))]
    pub tai_seconds_since_1970: i64,
    pub nanosecond: SubsecondNanoseconds,
    /// The instant's uncertainty: bounded nanoseconds, or unbounded.
    pub uncertainty: NanosecondBound,
    pub authority: AuthorityBinding,
}

impl TimeInstant {
    pub fn total_nanoseconds(&self) -> i128 {
        i128::from(self.tai_seconds_since_1970) * NANOS_PER_SECOND
            + i128::from(self.nanosecond.get())
    }

    /// Split a signed coordinate without wrapping seconds or losing its fraction; the
    /// seconds must lie within the JSON integer range.
    pub fn from_total_nanoseconds(
        total: i128,
        uncertainty: NanosecondBound,
        authority: AuthorityBinding,
    ) -> Result<Self, TimeCoordinateError> {
        let (seconds, nanosecond) = split_total(total)?;
        Ok(Self {
            tai_seconds_since_1970: seconds,
            nanosecond,
            uncertainty,
            authority,
        })
    }
}
