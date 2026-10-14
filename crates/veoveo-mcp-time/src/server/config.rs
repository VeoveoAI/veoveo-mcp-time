use std::path::PathBuf;

use clap::Parser;
use secrecy::SecretString;
use veoveo_sdk_rust::{PublicDeployment, parse_allowed_host_authority};

/// Every flag has the matching `VEOVEO_` environment variable. The Gateway issues the
/// database user and the access-token trust when Time registers, so neither is
/// configured here.
#[derive(Parser)]
#[command(name = "veoveo-mcp-time", about = "Time MCP server")]
pub(super) struct Args {
    #[arg(long, env = "VEOVEO_PORT", default_value_t = 8800)]
    pub port: u16,
    /// The origin where the Gateway reaches Time; Time serves MCP at
    /// `{public_base_url}/time/mcp` and registers that URL.
    #[arg(long, env = "VEOVEO_PUBLIC_BASE_URL")]
    pub public_base_url: String,
    /// The Gateway's RFC 8414 issuer; registration discovers its endpoint from there.
    #[arg(long, env = "VEOVEO_GATEWAY_ISSUER")]
    pub gateway_issuer: url::Url,
    /// The installation's registration secret, or the placeholder the installation's edge replaces.
    #[arg(
        long,
        env = "VEOVEO_REGISTRATION_SECRET",
        hide_env_values = true,
        value_parser = parse_secret
    )]
    pub registration_secret: SecretString,
    /// Holds Time's registration key, which keeps its namespace across restarts.
    #[arg(
        long,
        env = "VEOVEO_KEY_DIRECTORY",
        default_value = "/var/lib/veoveo/time/keys"
    )]
    pub key_directory: PathBuf,
    #[arg(
        long,
        env = "VEOVEO_BOOTSTRAP_TZDB_DIRECTORY",
        default_value = "/usr/share/zoneinfo"
    )]
    pub bootstrap_tzdb_directory: PathBuf,
    #[arg(
        long,
        env = "VEOVEO_BOOTSTRAP_TZDB_SOURCE_FILE",
        default_value = "/usr/share/zoneinfo/tzdata.zi"
    )]
    pub bootstrap_tzdb_source_file: PathBuf,
    #[arg(
        long,
        env = "VEOVEO_BOOTSTRAP_LEAP_SECONDS_FILE",
        default_value = "/usr/share/zoneinfo/leap-seconds.list"
    )]
    pub bootstrap_leap_seconds_file: PathBuf,
    #[arg(
        long,
        env = "VEOVEO_ACQUISITION_SCRATCH_DIRECTORY",
        default_value = "/var/lib/veoveo/time/acquisitions"
    )]
    pub acquisition_scratch_directory: PathBuf,
    #[arg(
        long,
        env = "VEOVEO_RELEASE_DIRECTORY",
        default_value = "/var/lib/veoveo/time/releases"
    )]
    pub release_directory: PathBuf,
    #[arg(long, env = "VEOVEO_ZIC_EXECUTABLE", default_value = "/usr/sbin/zic")]
    pub zic_executable: PathBuf,
    #[arg(
        long,
        env = "VEOVEO_MAXIMUM_SOURCE_BYTES",
        default_value_t = 67_108_864
    )]
    pub maximum_source_bytes: u64,
    #[arg(
        long,
        env = "VEOVEO_MAXIMUM_EXPANDED_BYTES",
        default_value_t = 536_870_912
    )]
    pub maximum_expanded_bytes: u64,
    #[arg(
        long,
        env = "VEOVEO_ACQUISITION_TIMEOUT_SECONDS",
        default_value_t = 300
    )]
    pub acquisition_timeout_seconds: u64,
    #[arg(long, env = "VEOVEO_NTPD_OBSERVATION_SOCKET")]
    pub ntpd_observation_socket: Option<PathBuf>,
    #[arg(
        long,
        env = "VEOVEO_CLOCK_OBSERVATION_TIMEOUT_SECONDS",
        default_value_t = 2
    )]
    pub clock_observation_timeout_seconds: u64,
    #[arg(long, env = "VEOVEO_ALLOW_LOOPBACK_HOSTS", default_value_t = false)]
    pub allow_loopback_hosts: bool,
    #[arg(
        long = "allowed-host",
        env = "VEOVEO_ALLOWED_HOSTS",
        value_name = "HOST",
        value_delimiter = ',',
        value_parser = parse_allowed_host
    )]
    pub allowed_hosts: Vec<String>,
}

impl Args {
    pub fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }
}

fn parse_secret(value: &str) -> Result<SecretString, String> {
    (!value.is_empty())
        .then(|| SecretString::from(value))
        .ok_or_else(|| "secret must not be empty".to_owned())
}
fn parse_allowed_host(value: &str) -> Result<String, String> {
    let value = value.trim();
    parse_allowed_host_authority(value)
        .map(|_| value.to_owned())
        .ok_or_else(|| "expected a host authority such as vv-mcp-time.internal:8800".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(extra: &[&str]) -> Result<Args, clap::Error> {
        let mut argv = vec![
            "veoveo-mcp-time",
            "--public-base-url",
            "https://time.example",
            "--gateway-issuer",
            "https://gateway.example",
            "--registration-secret",
            "fixture-only",
        ];
        argv.extend(extra);
        Args::try_parse_from(argv)
    }

    #[test]
    fn registration_inputs_are_required_and_database_inputs_are_gone() {
        let parsed = args(&[]).unwrap();
        assert_eq!(parsed.gateway_issuer.as_str(), "https://gateway.example/");
        assert_eq!(
            parsed.key_directory,
            PathBuf::from("/var/lib/veoveo/time/keys")
        );
        assert!(args(&["--registration-secret", ""]).is_err());
        assert!(
            Args::try_parse_from(["veoveo-mcp-time", "--public-base-url", "https://t"]).is_err()
        );
        assert!(args(&["--database-url", "postgres://127.0.0.1/veoveo"]).is_err());
    }

    #[test]
    fn allowed_hosts_accept_host_authorities_only() {
        let parsed = args(&["--allowed-host", "vv-mcp-time.internal:8800"]).unwrap();
        assert_eq!(parsed.allowed_hosts, ["vv-mcp-time.internal:8800"]);
        assert!(args(&["--allowed-host", "two hosts"]).is_err());
    }
}
