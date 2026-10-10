//! RFC 6238 time-based one-time passwords. Accepts a bare base32 secret or
//! an otpauth://totp/... URI (digits, period and algorithm honoured).

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use std::time::{Duration, SystemTime};

use super::config;
use super::log;

#[derive(Debug, Clone)]
pub struct Totp {
    pub key: Vec<u8>,
    pub digits: u32,
    pub period: u64,
    pub algorithm: String,
}

impl Totp {
    pub fn parse(raw: &str) -> Result<Totp, String> {
        let mut s = raw.trim().to_string();
        let (mut digits, mut period, mut algo) = (6u32, 30u64, "sha1".to_string());
        if s.to_lowercase().starts_with("otpauth://") {
            let u = url::Url::parse(&s).map_err(|_| "invalid otpauth:// URI".to_string())?;
            let q = |n: &str| {
                u.query_pairs()
                    .find(|(k, _)| k.to_lowercase() == n)
                    .map(|(_, v)| v.into_owned())
            };
            s = q("secret").ok_or_else(|| "invalid otpauth:// URI".to_string())?;
            if let Some(d) = q("digits").and_then(|d| d.parse().ok()) {
                digits = d;
            }
            if let Some(p) = q("period").and_then(|p| p.parse().ok()) {
                period = p;
            }
            if let Some(a) = q("algorithm") {
                algo = a.to_lowercase();
            }
        }
        let cleaned: String = s
            .to_uppercase()
            .chars()
            .filter(|c| *c != ' ' && *c != '-')
            .collect();
        let key = base32_decode(&cleaned)
            .filter(|k| !k.is_empty())
            .ok_or_else(|| "invalid base32 secret".to_string())?;
        Ok(Totp {
            key,
            digits,
            period: period.max(1),
            algorithm: algo,
        })
    }

    pub fn code_at(&self, unix_seconds: u64) -> String {
        let counter = (unix_seconds / self.period).to_be_bytes();
        let mac: Vec<u8> = match self.algorithm.as_str() {
            "sha256" => {
                let mut m = Hmac::<Sha256>::new_from_slice(&self.key).expect("hmac");
                m.update(&counter);
                m.finalize().into_bytes().to_vec()
            }
            "sha512" => {
                let mut m = Hmac::<Sha512>::new_from_slice(&self.key).expect("hmac");
                m.update(&counter);
                m.finalize().into_bytes().to_vec()
            }
            _ => {
                let mut m = Hmac::<Sha1>::new_from_slice(&self.key).expect("hmac");
                m.update(&counter);
                m.finalize().into_bytes().to_vec()
            }
        };
        let off = (mac[mac.len() - 1] & 0x0f) as usize;
        let bin = ((mac[off] as u32) << 24
            | (mac[off + 1] as u32) << 16
            | (mac[off + 2] as u32) << 8
            | mac[off + 3] as u32)
            & 0x7fff_ffff;
        let modulus = 10u32.pow(self.digits.min(9));
        format!("{:0width$}", bin % modulus, width = self.digits as usize)
    }

    pub fn code(&self) -> String {
        self.code_at(now())
    }

    pub fn seconds_remaining(&self) -> u64 {
        self.period - now() % self.period
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits = 0u32;
    let mut value = 0u32;
    let mut out = Vec::new();
    for c in s.bytes().filter(|&c| c != b'=') {
        let v = ALPHABET.iter().position(|&a| a == c)? as u32;
        value = (value << 5) | v;
        bits += 5;
        if bits >= 8 {
            out.push(((value >> (bits - 8)) & 0xff) as u8);
            bits -= 8;
            value &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// A code not already used for this host. Most PAM/TOTP modules reject
/// re-use inside a window, so a quick reconnect waits for the next one.
pub fn fresh(host: &str, raw: &str) -> Result<String, String> {
    let t = Totp::parse(raw)?;
    let safe: String = host
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || ".-_@".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    config::ensure_dirs();
    let marker = config::get().state_dir.join(format!("{safe}.totp-last"));
    let last = std::fs::read_to_string(&marker)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let mut code = t.code();
    if code == last {
        let wait = t.seconds_remaining() + 1;
        log::info(format!(
            "{host}: TOTP code already used, waiting {wait}s for next window"
        ));
        std::thread::sleep(Duration::from_secs(wait));
        code = t.code();
    }
    let _ = std::fs::write(&marker, &code);
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vectors() {
        // RFC 6238 appendix B, the SHA1 key "12345678901234567890" (base32).
        let t = Totp::parse("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ").unwrap();
        let t8 = Totp {
            digits: 8,
            ..t.clone()
        };
        assert_eq!(t8.code_at(59), "94287082");
        assert_eq!(t8.code_at(1111111109), "07081804");
        assert_eq!(t8.code_at(20000000000), "65353130");
        assert_eq!(t.code_at(59), "287082");
    }

    #[test]
    fn otpauth_uri() {
        let t = Totp::parse("otpauth://totp/Example:alice@x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8&period=60&algorithm=SHA256").unwrap();
        assert_eq!(
            (t.digits, t.period, t.algorithm.as_str()),
            (8, 60, "sha256")
        );
        assert!(Totp::parse("otpauth://totp/x?digits=6").is_err());
        assert!(Totp::parse("not base32 !!").is_err());
    }
}
