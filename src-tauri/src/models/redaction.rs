//! What redaction removes from text Keynobi shares (an exported debug
//! session). See `services/redaction.rs`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One kind of personal or secret data redaction replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum RedactionRule {
    /// Email addresses.
    Emails,
    /// Authorization values, bearer and basic credentials, JWTs, well-known
    /// key shapes, URL credentials, and password, token, secret, and key
    /// values in `key=value` and JSON pairs.
    Secrets,
    /// IPv4 and IPv6 addresses, except loopback and the emulator's host
    /// alias `10.0.2.2`.
    IpAddresses,
    /// The home folder (`~`) and the project folder (`<project>`).
    Paths,
    /// Physical and wireless device serials; emulator serials and AVD names
    /// stay.
    DeviceSerials,
}

impl RedactionRule {
    pub const ALL: [RedactionRule; 5] = [
        RedactionRule::Emails,
        RedactionRule::Secrets,
        RedactionRule::IpAddresses,
        RedactionRule::Paths,
        RedactionRule::DeviceSerials,
    ];
}

/// Which rules to apply. Every rule is on unless turned off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct RedactionRules {
    pub emails: bool,
    pub secrets: bool,
    pub ip_addresses: bool,
    pub paths: bool,
    pub device_serials: bool,
}

impl Default for RedactionRules {
    fn default() -> Self {
        RedactionRules {
            emails: true,
            secrets: true,
            ip_addresses: true,
            paths: true,
            device_serials: true,
        }
    }
}

impl RedactionRules {
    pub fn enabled(&self, rule: RedactionRule) -> bool {
        match rule {
            RedactionRule::Emails => self.emails,
            RedactionRule::Secrets => self.secrets,
            RedactionRule::IpAddresses => self.ip_addresses,
            RedactionRule::Paths => self.paths,
            RedactionRule::DeviceSerials => self.device_serials,
        }
    }
}

/// How many matches of one rule were replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct RedactionCount {
    pub rule: RedactionRule,
    pub enabled: bool,
    pub count: u32,
}
