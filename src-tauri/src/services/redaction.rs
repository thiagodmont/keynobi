//! Redaction of text Keynobi shares, such as an exported debug session.
//!
//! It is best effort: it replaces what its rules recognise and nothing else,
//! so its output must never be presented as scrubbed. Each match becomes a
//! token that is stable for one [`Redactor`] (`<email-1>`, `<ip-2>`,
//! `<device-1>`, `<secret-3>`), so the same value reads the same everywhere
//! in one bundle. Paths become `~` and `<project>`.
//!
//! Rules run in a fixed order: paths, secrets (URL credentials before
//! emails), device serials (a wireless serial before its IP address),
//! emails, then IP addresses.

use crate::models::redaction::{RedactionCount, RedactionRule, RedactionRules};
use regex::{Captures, Regex};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::sync::LazyLock;

/// Shortest serial replaced, so a short string never stands for a device.
const MIN_SERIAL_CHARS: usize = 4;

/// What to look for besides the patterns: the paths and serials of this
/// session.
#[derive(Debug, Clone, Default)]
pub struct RedactionContext {
    /// The user's home folder.
    pub home: Option<String>,
    /// The project folder, as recorded and canonical.
    pub project_roots: Vec<String>,
    /// Device serials seen; emulator serials among them are kept.
    pub serials: Vec<String>,
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}

static URL_CREDENTIALS: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)\b([a-z][a-z0-9+.\-]*://)([^/\s:@]+:[^/\s@]+)@"));
static AUTHORIZATION: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r#"(?i)\b((?:proxy-)?authorization)(["']?\s*[:=]\s*["']?)((?:bearer|basic|digest|token)\s+)?([^\s"',;<]+)"#,
    )
});
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)\b(bearer|basic)(\s+)([A-Za-z0-9\-._~+/]{8,}=*)"));
static JWT: LazyLock<Regex> =
    LazyLock::new(|| regex(r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]*"));
static KEY_SHAPES: LazyLock<Regex> = LazyLock::new(|| {
    regex(concat!(
        r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
        r"|\bAIza[0-9A-Za-z_\-]{35}",
        r"|\bgh[pousr]_[A-Za-z0-9]{36,}",
        r"|\bgithub_pat_[A-Za-z0-9_]{22,}",
        r"|\bsk-[A-Za-z0-9_\-]{20,}",
        r"|\bxox[abprs]-[A-Za-z0-9\-]{10,}",
        r"|\b[sr]k_(?:live|test)_[A-Za-z0-9]{16,}",
    ))
});
static KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r#"(?i)(["']?)\b([a-z0-9_.\-]*(?:password|passwd|secret|token|api[_\-]?key|access[_\-]?key|private[_\-]?key|credential)s?)(["']?)(\s*[:=]\s*)("[^"]*"|'[^']*'|[^\s,;&"'}\])<]+)"#,
    )
});
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9\-]+(?:\.[a-z0-9\-]+)*\.[a-z]{2,}\b"));
static IPV4: LazyLock<Regex> = LazyLock::new(|| regex(r"\b\d{1,3}(?:\.\d{1,3}){3}\b"));
static IPV6: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)[0-9a-f:]*::[0-9a-f:]*|\b(?:[0-9a-f]{1,4}:){7}[0-9a-f]{1,4}\b"));

/// An emulator's serial (`emulator-5554`), which names no physical device.
pub fn is_emulator_serial(serial: &str) -> bool {
    serial
        .strip_prefix("emulator-")
        .is_some_and(|port| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replaces personal and secret data by its rules, keeping tokens stable.
#[derive(Debug)]
pub struct Redactor {
    rules: RedactionRules,
    home: Option<String>,
    /// Longest first, so a nested root is not cut short.
    projects: Vec<String>,
    serials: Vec<String>,
    tokens: HashMap<(RedactionRule, String), String>,
    next: HashMap<RedactionRule, u32>,
    counts: BTreeMap<RedactionRule, u32>,
}

impl Redactor {
    pub fn new(rules: RedactionRules, context: RedactionContext) -> Self {
        let clean = |p: &String| p.trim_end_matches('/').to_string();
        let mut projects: Vec<String> = context
            .project_roots
            .iter()
            .map(clean)
            .filter(|p| p.len() > 1)
            .collect();
        projects.sort_by_key(|p| std::cmp::Reverse(p.len()));
        projects.dedup();
        let mut serials: Vec<String> = context
            .serials
            .into_iter()
            .filter(|s| s.chars().count() >= MIN_SERIAL_CHARS && !is_emulator_serial(s))
            .collect();
        serials.sort_by_key(|s| std::cmp::Reverse(s.len()));
        serials.dedup();
        Redactor {
            rules,
            home: context.home.as_ref().map(clean).filter(|h| h.len() > 1),
            projects,
            serials,
            tokens: HashMap::new(),
            next: HashMap::new(),
            counts: BTreeMap::new(),
        }
    }

    /// How many matches each rule replaced so far, every rule listed.
    pub fn counts(&self) -> Vec<RedactionCount> {
        RedactionRule::ALL
            .iter()
            .map(|&rule| RedactionCount {
                rule,
                enabled: self.rules.enabled(rule),
                count: self.counts.get(&rule).copied().unwrap_or(0),
            })
            .collect()
    }

    /// Redact every string in `value`, keys excepted.
    pub fn redact_json(&mut self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.redact(text),
            Value::Array(items) => items.iter_mut().for_each(|v| self.redact_json(v)),
            Value::Object(map) => map.values_mut().for_each(|v| self.redact_json(v)),
            _ => {}
        }
    }

    /// `text` with every enabled rule applied.
    pub fn redact(&mut self, text: &str) -> String {
        let mut out = text.to_string();
        if self.rules.paths {
            out = self.redact_paths(&out);
        }
        if self.rules.secrets {
            out = self.redact_secrets(&out);
        }
        if self.rules.device_serials {
            for serial in self.serials.clone() {
                out = self.replace_literal(&out, &serial, RedactionRule::DeviceSerials, |r, s| {
                    r.token(RedactionRule::DeviceSerials, s)
                });
            }
        }
        if self.rules.emails {
            out = self.apply(&out, &EMAIL, RedactionRule::Emails, |r, caps, _| {
                Some(r.token(RedactionRule::Emails, &caps[0].to_ascii_lowercase()))
            });
        }
        if self.rules.ip_addresses {
            out = self.redact_ips(&out);
        }
        out
    }

    fn token(&mut self, rule: RedactionRule, original: &str) -> String {
        if let Some(token) = self.tokens.get(&(rule, original.to_string())) {
            return token.clone();
        }
        let n = self.next.entry(rule).or_insert(0);
        *n += 1;
        let prefix = match rule {
            RedactionRule::Emails => "email",
            RedactionRule::Secrets => "secret",
            RedactionRule::IpAddresses => "ip",
            RedactionRule::Paths => "path",
            RedactionRule::DeviceSerials => "device",
        };
        let token = format!("<{prefix}-{n}>");
        self.tokens
            .insert((rule, original.to_string()), token.clone());
        token
    }

    fn count(&mut self, rule: RedactionRule) {
        *self.counts.entry(rule).or_insert(0) += 1;
    }

    /// Replace each match of `re` for which `replace` returns a value.
    fn apply(
        &mut self,
        text: &str,
        re: &Regex,
        rule: RedactionRule,
        replace: impl Fn(&mut Redactor, &Captures, &str) -> Option<String>,
    ) -> String {
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for caps in re.captures_iter(text) {
            let Some(whole) = caps.get(0) else {
                continue;
            };
            if let Some(replacement) = replace(self, &caps, text) {
                out.push_str(&text[last..whole.start()]);
                out.push_str(&replacement);
                last = whole.end();
                self.count(rule);
            }
        }
        out.push_str(&text[last..]);
        out
    }

    /// Replace `needle` where it is not part of a longer word.
    fn replace_literal(
        &mut self,
        text: &str,
        needle: &str,
        rule: RedactionRule,
        replacement: impl Fn(&mut Redactor, &str) -> String,
    ) -> String {
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        let mut from = 0;
        while let Some(found) = text[from..].find(needle) {
            let start = from + found;
            let end = start + needle.len();
            let before = text[..start].chars().next_back();
            let after = text[end..].chars().next();
            let boundary = |c: Option<char>, extra: &[char]| {
                c.is_none_or(|c| !is_word_char(c) && !extra.contains(&c))
            };
            let bounded = match rule {
                // A path continues with `/`; `/Users/me` is not `/Users/meg`.
                RedactionRule::Paths => {
                    boundary(before, &['.', '-']) && boundary(after, &['.', '-'])
                }
                _ => boundary(before, &['.', '-', ':']) && boundary(after, &['.', '-']),
            };
            if bounded {
                out.push_str(&text[last..start]);
                out.push_str(&replacement(self, needle));
                last = end;
                self.count(rule);
            }
            from = end;
        }
        out.push_str(&text[last..]);
        out
    }

    fn redact_paths(&mut self, text: &str) -> String {
        let mut out = text.to_string();
        for project in self.projects.clone() {
            out = self.replace_literal(&out, &project, RedactionRule::Paths, |_, _| {
                "<project>".into()
            });
        }
        if let Some(home) = self.home.clone() {
            out = self.replace_literal(&out, &home, RedactionRule::Paths, |_, _| "~".into());
        }
        out
    }

    fn redact_secrets(&mut self, text: &str) -> String {
        let secret = RedactionRule::Secrets;
        let mut out = self.apply(text, &URL_CREDENTIALS, secret, |r, caps, _| {
            Some(format!("{}{}@", &caps[1], r.token(secret, &caps[2])))
        });
        out = self.apply(&out, &AUTHORIZATION, secret, |r, caps, _| {
            let scheme = caps.get(3).map_or("", |m| m.as_str());
            Some(format!(
                "{}{}{scheme}{}",
                &caps[1],
                &caps[2],
                r.token(secret, &caps[4])
            ))
        });
        out = self.apply(&out, &BEARER, secret, |r, caps, _| {
            // A credential, not a word: "basic information" stays.
            let value = &caps[3];
            value
                .chars()
                .any(|c| c.is_ascii_digit() || matches!(c, '=' | '+' | '/'))
                .then(|| format!("{}{}{}", &caps[1], &caps[2], r.token(secret, value)))
        });
        out = self.apply(&out, &JWT, secret, |r, caps, _| {
            Some(r.token(secret, &caps[0]))
        });
        out = self.apply(&out, &KEY_SHAPES, secret, |r, caps, _| {
            Some(r.token(secret, &caps[0]))
        });
        self.apply(&out, &KEY_VALUE, secret, |r, caps, _| {
            let value = &caps[5];
            let (open, inner, close) = match value.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    (q.to_string(), &value[1..value.len() - 1], q.to_string())
                }
                _ => (String::new(), value, String::new()),
            };
            if inner.is_empty() || (inner.starts_with('<') && inner.ends_with('>')) {
                return None;
            }
            Some(format!(
                "{}{}{}{}{open}{}{close}",
                &caps[1],
                &caps[2],
                &caps[3],
                &caps[4],
                r.token(secret, inner)
            ))
        })
    }

    fn redact_ips(&mut self, text: &str) -> String {
        let ip = RedactionRule::IpAddresses;
        let out = self.apply(text, &IPV4, ip, |r, caps, text| {
            let m = caps.get(0)?;
            // Part of a longer dotted number, such as a four-part version.
            let before = text[..m.start()].chars().next_back();
            let after = text[m.end()..].chars().next();
            let after_next = text[m.end()..].chars().nth(1);
            if before == Some('.')
                || (after == Some('.') && after_next.is_some_and(|c| c.is_ascii_digit()))
            {
                return None;
            }
            let addr = Ipv4Addr::from_str(m.as_str()).ok()?;
            if addr.is_loopback() || addr == Ipv4Addr::new(10, 0, 2, 2) {
                return None;
            }
            Some(r.token(ip, m.as_str()))
        });
        self.apply(&out, &IPV6, ip, |r, caps, text| {
            let m = caps.get(0)?;
            let before = text[..m.start()].chars().next_back();
            let after = text[m.end()..].chars().next();
            let bounded =
                |c: Option<char>| c.is_none_or(|c| !is_word_char(c) && c != ':' && c != '.');
            if !bounded(before) || !bounded(after) {
                return None;
            }
            let addr = Ipv6Addr::from_str(m.as_str()).ok()?;
            if addr.is_loopback() || addr.is_unspecified() {
                return None;
            }
            Some(r.token(ip, &m.as_str().to_ascii_lowercase()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn redactor() -> Redactor {
        Redactor::new(
            RedactionRules::default(),
            RedactionContext {
                home: Some("/Users/jane".into()),
                project_roots: vec!["/Users/jane/work/MyApp".into()],
                serials: vec![
                    "R5CT1234ABC".into(),
                    "192.168.1.20:5555".into(),
                    "emulator-5554".into(),
                ],
            },
        )
    }

    fn count(r: &Redactor, rule: RedactionRule) -> u32 {
        r.counts()
            .iter()
            .find(|c| c.rule == rule)
            .map_or(0, |c| c.count)
    }

    #[test]
    fn emails_become_stable_tokens() {
        let mut r = redactor();
        assert_eq!(
            r.redact("login jane.doe+test@example.com ok, again Jane.Doe+test@Example.com"),
            "login <email-1> ok, again <email-1>"
        );
        assert_eq!(r.redact("from bob@corp.co.uk"), "from <email-2>");
        // Object ids and annotations are not addresses.
        assert_eq!(
            r.redact("java.lang.Object@5f2a1b and @Composable"),
            "java.lang.Object@5f2a1b and @Composable"
        );
        assert_eq!(count(&r, RedactionRule::Emails), 3);
    }

    #[test]
    fn authorization_values_and_bearer_tokens_are_secrets() {
        let mut r = redactor();
        assert_eq!(
            r.redact("Authorization: Bearer abc123.def-456"),
            "Authorization: Bearer <secret-1>"
        );
        assert_eq!(
            r.redact(r#"{"Authorization":"Basic dXNlcjpwYXNz"}"#),
            r#"{"Authorization":"Basic <secret-2>"}"#
        );
        assert_eq!(
            r.redact("sent with bearer 9f8e7d6c5b4a3210"),
            "sent with bearer <secret-3>"
        );
        // Words after "basic" or "bearer" are not credentials.
        assert_eq!(
            r.redact("basic information about the bearer instrument"),
            "basic information about the bearer instrument"
        );
    }

    #[test]
    fn jwts_and_well_known_key_shapes_are_secrets() {
        let mut r = redactor();
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        assert_eq!(
            r.redact(&format!("token {jwt} end")),
            "token <secret-1> end"
        );
        for key in [
            "AKIAIOSFODNN7EXAMPLE",
            "AIzaSyA1234567890abcdefghijklmnopqrstuv",
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "sk-proj-abcdefghijklmnopqrstuvwx",
            "xoxb-1234567890-abcdefghij",
            "sk_live_abcdefghijklmnop1234",
        ] {
            let out = r.redact(&format!("key {key} used"));
            assert!(out.starts_with("key <secret-"), "{key} -> {out}");
            assert!(!out.contains(key), "{key} -> {out}");
        }
    }

    #[test]
    fn secret_values_in_key_value_and_json_pairs_are_replaced_keeping_the_key() {
        let mut r = redactor();
        assert_eq!(
            r.redact("login password=hunter2&user=jane"),
            "login password=<secret-1>&user=jane"
        );
        assert_eq!(
            r.redact(r#"{"access_token": "abc.def", "api_key":'k-1', "name":"x"}"#),
            r#"{"access_token": "<secret-2>", "api_key":'<secret-3>', "name":"x"}"#
        );
        assert_eq!(
            r.redact("CLIENT_SECRET: s3cr3t"),
            "CLIENT_SECRET: <secret-4>"
        );
        // The same secret keeps its token; an empty value is left alone.
        assert_eq!(r.redact("password=hunter2"), "password=<secret-1>");
        assert_eq!(r.redact(r#"token="""#), r#"token="""#);
        // A key that only contains a keyword is not a secret.
        assert_eq!(r.redact("tokenCount: 5"), "tokenCount: 5");
    }

    #[test]
    fn url_credentials_are_secrets_not_emails() {
        let mut r = redactor();
        assert_eq!(
            r.redact("GET https://admin:pa55@api.example.com/v1"),
            "GET https://<secret-1>@api.example.com/v1"
        );
        assert_eq!(count(&r, RedactionRule::Emails), 0);
    }

    #[test]
    fn ip_addresses_are_replaced_except_loopback_and_the_emulator_host() {
        let mut r = redactor();
        assert_eq!(
            r.redact("connect 192.168.0.12:443 via 8.8.8.8, again 192.168.0.12"),
            "connect <ip-1>:443 via <ip-2>, again <ip-1>"
        );
        assert_eq!(
            r.redact("local 127.0.0.1 and 127.3.4.5, host 10.0.2.2, ::1"),
            "local 127.0.0.1 and 127.3.4.5, host 10.0.2.2, ::1"
        );
        assert_eq!(
            r.redact("v6 2001:db8::8a2e:370:7334 and fe80:0:0:0:0:0:0:1"),
            "v6 <ip-3> and <ip-4>"
        );
        assert_eq!(count(&r, RedactionRule::IpAddresses), 5);
    }

    #[test]
    fn times_versions_macs_and_code_are_not_ip_addresses() {
        let mut r = redactor();
        for text in [
            "09-25 10:32:05.123 1234 1250 E AndroidRuntime: FATAL",
            "version 1.2.3.4.5 and 300.1.2.3",
            "mac aa:bb:cc:dd:ee:ff",
            "std::vector and Foo::bar and ::",
            "sha 6b1c2f0a6b1c2f0a",
        ] {
            assert_eq!(r.redact(text), text);
        }
        assert_eq!(count(&r, RedactionRule::IpAddresses), 0);
    }

    #[test]
    fn home_and_project_paths_are_shortened() {
        let mut r = redactor();
        assert_eq!(
            r.redact("at /Users/jane/work/MyApp/app/src/Main.kt and /Users/jane/.gradle"),
            "at <project>/app/src/Main.kt and ~/.gradle"
        );
        // Another user whose name starts the same is not the home folder.
        assert_eq!(r.redact("/Users/janet/x"), "/Users/janet/x");
        assert_eq!(r.redact("/Users/jane"), "~");
        assert_eq!(count(&r, RedactionRule::Paths), 3);
    }

    #[test]
    fn physical_and_wireless_serials_become_pseudonyms_emulators_stay() {
        let mut r = redactor();
        assert_eq!(
            r.redact("R5CT1234ABC went offline; emulator-5554 stays; 192.168.1.20:5555 too"),
            "<device-2> went offline; emulator-5554 stays; <device-1> too"
        );
        // Part of a longer word is not the serial.
        assert_eq!(r.redact("XR5CT1234ABCY"), "XR5CT1234ABCY");
        assert_eq!(count(&r, RedactionRule::DeviceSerials), 2);
        assert_eq!(count(&r, RedactionRule::IpAddresses), 0);
        assert!(is_emulator_serial("emulator-5554"));
        assert!(!is_emulator_serial("emulator-"));
        assert!(!is_emulator_serial("R5CT1234ABC"));
    }

    #[test]
    fn a_rule_turned_off_leaves_its_matches_and_counts_nothing() {
        let rules = RedactionRules {
            emails: false,
            ip_addresses: false,
            ..RedactionRules::default()
        };
        let mut r = Redactor::new(rules, RedactionContext::default());
        let text = "jane@example.com from 192.168.0.1, password=x1";
        assert_eq!(
            r.redact(text),
            "jane@example.com from 192.168.0.1, password=<secret-1>"
        );
        let counts = r.counts();
        assert_eq!(counts.len(), RedactionRule::ALL.len());
        let emails = counts.iter().find(|c| c.rule == RedactionRule::Emails);
        assert_eq!(emails.map(|c| (c.enabled, c.count)), Some((false, 0)));
        let secrets = counts.iter().find(|c| c.rule == RedactionRule::Secrets);
        assert_eq!(secrets.map(|c| (c.enabled, c.count)), Some((true, 1)));
    }

    #[test]
    fn every_string_in_json_is_redacted_and_keys_are_kept() {
        let mut r = redactor();
        let mut value = json!({
            "projectRoot": "/Users/jane/work/MyApp",
            "device": { "serial": "R5CT1234ABC", "avdName": null },
            "events": [{ "note": "mail jane@example.com" }, 42],
            "jane@example.com": true,
        });
        r.redact_json(&mut value);
        assert_eq!(
            value,
            json!({
                "projectRoot": "<project>",
                "device": { "serial": "<device-1>", "avdName": null },
                "events": [{ "note": "mail <email-1>" }, 42],
                "jane@example.com": true,
            })
        );
    }
}
