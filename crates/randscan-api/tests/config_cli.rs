//! `ApiConfig::from_env` and the `randscan-api hash-password` subcommand. The environment is
//! process-wide, so the config checks are one test, run in order. No database.

use randscan_api::auth::verify_password;
use randscan_api::ApiConfig;
use std::io::Write;
use std::process::{Command, Stdio};

const KEYS: &[&str] = &[
    "API_HOST",
    "API_PORT",
    "COOKIE_SECURE",
    "TRUST_PROXY",
    "ANON_RATE_LIMIT_RPM",
    "KEY_RATE_LIMIT_RPM",
    "AUTH_RATE_LIMIT_RPM",
    "PUBLIC_URL",
    "MAIL_FROM",
];

/// Puts the named environment variables back as they were when it is dropped, on a panic too,
/// so a failing assertion cannot leave the process environment altered for the other tests.
struct EnvGuard(Vec<(&'static str, Option<String>)>);

impl EnvGuard {
    fn new(keys: &[&'static str]) -> Self {
        EnvGuard(keys.iter().map(|k| (*k, std::env::var(k).ok())).collect())
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in self.0.drain(..) {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

#[test]
fn the_environment_overrides_the_defaults_and_bad_values_fall_back() {
    let _env = EnvGuard::new(KEYS);
    for k in KEYS {
        std::env::remove_var(k);
    }
    let d = ApiConfig::default();
    let c = ApiConfig::from_env();
    assert_eq!(c.listen_addr, d.listen_addr);
    assert_eq!(c.listen_addr.port(), 3000);
    assert!(c.cookie_secure && !c.trust_proxy);
    assert_eq!((c.anon_rpm, c.key_rpm, c.auth_rpm), (60, 600, 10));
    assert_eq!(c.public_url, "https://randscan.org");
    assert_eq!(c.mail_from, "RandScan <no-reply@randscan.org>");

    std::env::set_var("API_HOST", "127.0.0.1");
    std::env::set_var("API_PORT", "8081");
    std::env::set_var("COOKIE_SECURE", "Off");
    std::env::set_var("TRUST_PROXY", "yes");
    std::env::set_var("ANON_RATE_LIMIT_RPM", "5");
    std::env::set_var("KEY_RATE_LIMIT_RPM", "0");
    std::env::set_var("AUTH_RATE_LIMIT_RPM", "3");
    std::env::set_var("PUBLIC_URL", "  https://explorer.example/// ");
    std::env::set_var("MAIL_FROM", " Ops <ops@example.org> ");
    let c = ApiConfig::from_env();
    assert_eq!(c.listen_addr, "127.0.0.1:8081".parse().unwrap());
    assert!(
        !c.cookie_secure,
        "\"Off\" turns the flag off, whatever the case"
    );
    assert!(c.trust_proxy);
    assert_eq!((c.anon_rpm, c.key_rpm, c.auth_rpm), (5, 0, 3));
    assert_eq!(
        c.public_url, "https://explorer.example",
        "trimmed, no trailing slash"
    );
    assert_eq!(c.mail_from, "Ops <ops@example.org>");

    // Unparsable numbers and addresses, and blank strings, keep the defaults.
    for f in ["0", "false", "no", "off"] {
        std::env::set_var("COOKIE_SECURE", f);
        assert!(!ApiConfig::from_env().cookie_secure, "{f}");
    }
    std::env::set_var("COOKIE_SECURE", "1");
    std::env::set_var("API_HOST", "not an address");
    std::env::set_var("ANON_RATE_LIMIT_RPM", "many");
    std::env::set_var("PUBLIC_URL", "   ");
    std::env::set_var("MAIL_FROM", "");
    let c = ApiConfig::from_env();
    assert!(c.cookie_secure);
    assert_eq!(c.listen_addr, d.listen_addr);
    assert_eq!(c.anon_rpm, 60);
    assert_eq!(c.public_url, d.public_url);
    assert_eq!(c.mail_from, d.mail_from);
}

fn hash_password_cli(stdin: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_randscan-api"))
        .arg("hash-password")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[tokio::test]
async fn hash_password_prints_an_argon2id_hash_the_api_accepts() {
    let out = hash_password_cli("a long enough passphrase\r\n");
    assert!(out.status.success());
    let hash = String::from_utf8(out.stdout).unwrap();
    let hash = hash.trim_end();
    assert!(hash.starts_with("$argon2id$"), "{hash}");
    assert!(
        verify_password(hash.to_string(), "a long enough passphrase".into())
            .await
            .unwrap(),
        "the line ending is not part of the password"
    );
    assert!(
        !verify_password(hash.to_string(), "another passphrase".into())
            .await
            .unwrap()
    );
}

#[test]
fn hash_password_refuses_a_password_the_policy_refuses() {
    let out = hash_password_cli("short\n");
    assert!(!out.status.success());
    assert!(
        out.stdout.is_empty(),
        "nothing is printed for a refused password"
    );
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("at least 10 characters"), "{err}");
}
