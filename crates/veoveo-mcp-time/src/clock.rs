use std::{collections::BTreeSet, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use tokio::io::AsyncReadExt;

use crate::contract::{ClockAssessment, ClockQuality, ClockQualityPolicy};

const OBSERVATION_LIMIT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub enum ClockSource {
    System,
    NtpdRs { observation_socket: PathBuf },
}

#[derive(Clone, Debug)]
pub struct ClockMonitor {
    source: ClockSource,
    timeout: Duration,
}

#[derive(Debug, Deserialize)]
struct NtpdObservation {
    system: NtpdSystem,
    #[serde(default)]
    sources: Vec<NtpdSource>,
}

#[derive(Debug, Deserialize)]
struct NtpdSystem {
    stratum: u8,
    #[serde(default)]
    root_delay: f64,
    #[serde(default)]
    root_variance_base: f64,
    leap_indicator: NtpdLeapIndicator,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
enum NtpdLeapIndicator {
    NoWarning,
    Leap61,
    Leap59,
    Unknown,
}

#[derive(Debug, Deserialize)]
struct NtpdSource {
    #[serde(default)]
    offset: f64,
    #[serde(default)]
    uncertainty: f64,
    #[serde(default)]
    delay: f64,
    #[serde(default)]
    unanswered_polls: u32,
    #[serde(default)]
    nts_cookies: Option<usize>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    address: String,
}

impl ClockMonitor {
    pub fn new(source: ClockSource, timeout: Duration) -> Self {
        Self { source, timeout }
    }

    pub async fn quality(&self) -> Result<ClockQuality> {
        match &self.source {
            ClockSource::System => Ok(crate::ClockQualityValue {
                synchronized: false,
                estimated_offset_nanoseconds: 0,
                error_bound: crate::NanosecondBound::UNBOUNDED,
                stratum: 16,
                holdover_age_seconds: None,
                source_diversity: 0,
                traceability: vec!["system_clock_unmeasured".to_owned()],
                observed_at: Utc::now().to_rfc3339(),
            }
            .build()?),
            ClockSource::NtpdRs { observation_socket } => {
                let stream = tokio::time::timeout(
                    self.timeout,
                    tokio::net::UnixStream::connect(observation_socket),
                )
                .await
                .context("ntpd-rs observation socket timed out")??;
                let mut bytes = Vec::new();
                tokio::time::timeout(
                    self.timeout,
                    stream
                        .take(OBSERVATION_LIMIT_BYTES + 1)
                        .read_to_end(&mut bytes),
                )
                .await
                .context("ntpd-rs observation read timed out")??;
                if bytes.len() as u64 > OBSERVATION_LIMIT_BYTES {
                    anyhow::bail!("ntpd-rs observation exceeds the size limit");
                }
                let observation: NtpdObservation =
                    serde_json::from_slice(&bytes).context("decoding ntpd-rs observation")?;
                project_ntpd(observation)
            }
        }
    }
}

fn project_ntpd(observation: NtpdObservation) -> Result<ClockQuality> {
    let usable: Vec<_> = observation
        .sources
        .iter()
        .filter(|source| source.unanswered_polls < 8 && source.uncertainty.is_finite())
        .collect();
    let best = usable.iter().copied().min_by(|left, right| {
        left.uncertainty
            .total_cmp(&right.uncertainty)
            .then_with(|| left.delay.total_cmp(&right.delay))
    });
    let offset_seconds = best.map_or(0.0, |source| source.offset);
    let source_bound = best.map_or(f64::INFINITY, |source| {
        source.offset.abs() + source.uncertainty.abs() + source.delay.abs() / 2.0
    });
    let system_bound = observation.system.root_delay.abs() / 2.0
        + observation.system.root_variance_base.max(0.0).sqrt();
    let error_bound = error_bound_from_seconds(source_bound + system_bound);
    let diversity: BTreeSet<_> = usable
        .iter()
        .map(|source| {
            if source.address.is_empty() {
                source.name.as_str()
            } else {
                source.address.as_str()
            }
        })
        .collect();
    let mut traceability = vec!["ntp".to_owned()];
    if usable.iter().any(|source| source.nts_cookies.is_some()) {
        traceability.push("nts".to_owned());
    }
    crate::ClockQualityValue {
        synchronized: observation.system.stratum < 16
            && !usable.is_empty()
            && !matches!(
                observation.system.leap_indicator,
                NtpdLeapIndicator::Unknown
            ),
        estimated_offset_nanoseconds: offset_nanoseconds(offset_seconds)?,
        error_bound,
        stratum: observation.system.stratum,
        holdover_age_seconds: None,
        source_diversity: diversity.len().try_into().unwrap_or(u32::MAX),
        traceability,
        observed_at: Utc::now().to_rfc3339(),
    }
    .build()
    .map_err(Into::into)
}

/// The observed offset in nanoseconds. An offset that is not finite or lies outside the
/// JSON integer range cannot be stated, so the observation is refused.
fn offset_nanoseconds(seconds: f64) -> Result<i64> {
    let nanoseconds = (seconds * 1_000_000_000.0).round();
    let limit = crate::JSON_INTEGER_MAX as f64;
    if !nanoseconds.is_finite() || nanoseconds.abs() > limit {
        anyhow::bail!("ntpd-rs reports a clock offset Time cannot state: {seconds} s");
    }
    Ok(nanoseconds as i64)
}

/// The conservative error bound, rounded up to the nanosecond. A bound that is not finite,
/// is negative or exceeds `2^53 - 1` nanoseconds states no usable limit, so it is unbounded.
fn error_bound_from_seconds(seconds: f64) -> crate::NanosecondBound {
    let nanoseconds = (seconds * 1_000_000_000.0).ceil();
    if !nanoseconds.is_finite()
        || nanoseconds < 0.0
        || nanoseconds > crate::NanosecondBound::MAX_NANOSECONDS as f64
    {
        return crate::NanosecondBound::UNBOUNDED;
    }
    crate::NanosecondBound::bounded(nanoseconds as u64).unwrap_or(crate::NanosecondBound::UNBOUNDED)
}

pub fn assess_clock(quality: ClockQuality, policy: ClockQualityPolicy) -> Result<ClockAssessment> {
    let mut violations = Vec::new();
    if !quality.synchronized {
        violations.push("clock is not synchronized".to_owned());
    }
    if quality
        .error_bound
        .nanoseconds()
        .is_none_or(|bound| bound > policy.maximum_error_nanoseconds())
    {
        violations.push("clock error bound exceeds policy".to_owned());
    }
    if quality.stratum > policy.maximum_stratum() {
        violations.push("clock stratum exceeds policy".to_owned());
    }
    if quality.source_diversity < policy.minimum_source_diversity() {
        violations.push("clock source diversity is below policy".to_owned());
    }
    if quality
        .holdover_age_seconds
        .is_some_and(|age| age > policy.maximum_holdover_seconds())
    {
        violations.push("clock holdover age exceeds policy".to_owned());
    }
    crate::ClockAssessmentValue {
        acceptable: violations.is_empty(),
        quality,
        policy,
        violations,
    }
    .build()
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_ntpd_quality_and_enforces_policy() {
        let observation: NtpdObservation = serde_json::from_value(serde_json::json!({
            "system": {
                "stratum": 2,
                "root_delay": 0.002,
                "root_variance_base": 0.000001,
                "leap_indicator": "NoWarning"
            },
            "sources": [{
                "offset": 0.0002,
                "uncertainty": 0.0003,
                "delay": 0.004,
                "unanswered_polls": 0,
                "nts_cookies": 4,
                "name": "alpha",
                "address": "192.0.2.1:123"
            }]
        }))
        .unwrap();
        let quality = project_ntpd(observation).unwrap();
        assert!(quality.synchronized);
        assert_eq!(quality.estimated_offset_nanoseconds, 200_000);
        assert_eq!(quality.source_diversity, 1);
        assert!(quality.traceability.contains(&"nts".to_owned()));
        let assessment = assess_clock(
            quality,
            ClockQualityPolicy::builder()
                .maximum_error_nanoseconds(10_000_000)
                .maximum_stratum(4)
                .minimum_source_diversity(1)
                .maximum_holdover_seconds(60)
                .build()
                .expect("valid built-in Time clock policy"),
        )
        .unwrap();
        assert!(assessment.acceptable);
        let bytes = serde_json::to_value(&assessment).unwrap();
        assert!(serde_json::from_value::<ClockAssessment>(bytes.clone()).is_ok());
        let mut contradictory = bytes;
        contradictory["acceptable"] = serde_json::json!(false);
        assert!(serde_json::from_value::<ClockAssessment>(contradictory).is_err());
        let mut draft = crate::contract::ClockAssessmentValue::from(assessment);
        draft.acceptable = false;
        assert!(draft.build().is_err());
    }

    #[tokio::test]
    async fn an_unmeasured_clock_has_an_unbounded_error_that_no_policy_accepts() {
        let quality = ClockMonitor::new(ClockSource::System, std::time::Duration::from_secs(1))
            .quality()
            .await
            .unwrap();
        assert!(quality.error_bound.is_unbounded());
        assert_eq!(
            serde_json::to_value(&quality).unwrap()["errorBound"],
            serde_json::json!({"kind": "unbounded"})
        );
        let loosest = ClockQualityPolicy::builder()
            .maximum_error_nanoseconds(crate::JSON_INTEGER_MAX as u64)
            .maximum_stratum(15)
            .minimum_source_diversity(1)
            .maximum_holdover_seconds(crate::JSON_INTEGER_MAX as u64)
            .build()
            .unwrap();
        let assessment = assess_clock(quality, loosest).unwrap();
        assert!(
            assessment
                .violations
                .contains(&"clock error bound exceeds policy".to_owned())
        );
    }

    #[test]
    fn observed_values_keep_within_the_json_range() {
        assert_eq!(
            error_bound_from_seconds(0.000_000_001_5),
            crate::NanosecondBound::bounded(2).unwrap()
        );
        let largest = crate::NanosecondBound::MAX_NANOSECONDS as f64 / 1e9;
        assert!(!error_bound_from_seconds(largest).is_unbounded());
        for unstated in [largest * 2.0, f64::INFINITY, f64::NAN, -1.0] {
            assert!(
                error_bound_from_seconds(unstated).is_unbounded(),
                "{unstated}"
            );
        }
        assert_eq!(offset_nanoseconds(-0.0002).unwrap(), -200_000);
        for refused in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 1e8, -1e8] {
            assert!(offset_nanoseconds(refused).is_err(), "{refused}");
        }
        let mut quality = crate::ClockQualityValue::from(
            project_ntpd(
                serde_json::from_value(serde_json::json!({
                    "system": {"stratum": 2, "root_delay": 0.0, "root_variance_base": 0.0,
                        "leap_indicator": "NoWarning"},
                    "sources": []
                }))
                .unwrap(),
            )
            .unwrap(),
        );
        quality.estimated_offset_nanoseconds = crate::JSON_INTEGER_MAX + 1;
        assert!(quality.clone().build().is_err());
        quality.estimated_offset_nanoseconds = 0;
        quality.holdover_age_seconds = Some(crate::NanosecondBound::MAX_NANOSECONDS + 1);
        assert!(quality.build().is_err());
    }
}
