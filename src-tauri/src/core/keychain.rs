//! All credentials live in ONE keychain item, the "shellder vault": a JSON
//! object {host: {password|passphrase|totp: secret}}. On macOS it is a
//! generic password in the login keychain, so the keychain asks for
//! permission at most once per signature: "Always Allow" adds the app to
//! the item's access list and its Team ID to the partition list, and that
//! answer survives rebuilds signed with the same identity. The item is
//! never deleted or re-created: a delete of an item another signature made
//! is a second password dialog, and a fresh item trusts only its creator.
//! On Linux it is one Secret Service item with the same label.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::config;
use super::log;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretKind {
    Password,
    Passphrase,
    Totp,
}

impl SecretKind {
    pub const ALL: [SecretKind; 3] = [
        SecretKind::Password,
        SecretKind::Passphrase,
        SecretKind::Totp,
    ];

    pub fn raw(self) -> &'static str {
        match self {
            SecretKind::Password => "password",
            SecretKind::Passphrase => "passphrase",
            SecretKind::Totp => "totp",
        }
    }

    pub fn parse(s: &str) -> Option<SecretKind> {
        SecretKind::ALL.into_iter().find(|k| k.raw() == s)
    }

    pub fn title(self) -> &'static str {
        match self {
            SecretKind::Password => "Password",
            SecretKind::Passphrase => "Key passphrase",
            SecretKind::Totp => "TOTP secret",
        }
    }
}

pub type Vault = BTreeMap<String, BTreeMap<String, String>>;

pub const VAULT_SERVICE: &str = "shellder:vault";
pub const VAULT_ACCOUNT: &str = "shellder";

/// What came back when the vault was last read. A keychain that refuses
/// (the dialog dismissed or denied at launch, a locked keychain) is not an
/// empty vault, and must never be taken for one: the secrets are still
/// there, they just cannot be read this minute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultRead {
    Ok(Vault),
    Empty,
    Refused(String),
}

/// A refusal is taken at its word for this long: the dialog was dismissed
/// once, and every host that logs in meanwhile must not bring it back.
/// Settings asks sooner (read(force)), and so does every write.
pub const REFUSAL_TTL: Duration = Duration::from_secs(300);

struct State {
    /// The vault as last read or written. It does not expire: every read
    /// that reaches the keychain under a signature the item does not trust
    /// yet is a dialog. Writes from another process drop it (see `stamp`).
    cached: Option<Vault>,
    refusal: Option<String>,
    refused_at: Option<Instant>,
}

static STATE: Mutex<State> = Mutex::new(State {
    cached: None,
    refusal: None,
    refused_at: None,
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The three outcomes of asking the keychain for the vault.
pub fn interpret(outcome: Result<Option<Vec<u8>>, String>) -> VaultRead {
    match outcome {
        Ok(Some(data)) => match serde_json::from_slice::<Vault>(&data) {
            Ok(v) => VaultRead::Ok(v),
            Err(_) => VaultRead::Empty,
        },
        Ok(None) => VaultRead::Empty,
        Err(e) => VaultRead::Refused(e),
    }
}

/// Whether a read should go to the keychain, or repeat the refusal it
/// gave a moment ago.
pub fn should_ask(refused_at: Option<Instant>, now: Instant, force: bool) -> bool {
    if force {
        return true;
    }
    match refused_at {
        None => true,
        Some(t) => now.duration_since(t) >= REFUSAL_TTL,
    }
}

/// The vault a write starts from. Never a refused read: saving one secret
/// over a vault that could not be read would replace every other secret
/// in it with that one.
pub fn writable(read: VaultRead) -> Result<Vault, String> {
    match read {
        VaultRead::Ok(v) => Ok(v),
        VaultRead::Empty => Ok(Vault::new()),
        VaultRead::Refused(e) => Err(e),
    }
}

/// Why the last read failed, if it did.
pub fn refusal() -> Option<String> {
    state().refusal.clone()
}

/// Read the vault, saying which of the three things happened. A refusal
/// is repeated for REFUSAL_TTL without asking the keychain again, unless
/// forced.
pub fn read(force: bool) -> VaultRead {
    let mut st = state();
    if force {
        st.cached = None;
        st.refused_at = None;
    }
    if let Some(v) = &st.cached {
        return VaultRead::Ok(v.clone());
    }
    if let Some(r) = &st.refusal {
        if !should_ask(st.refused_at, Instant::now(), force) {
            return VaultRead::Refused(r.clone());
        }
    }
    let outcome = interpret(backend::read());
    match &outcome {
        VaultRead::Ok(v) => {
            st.refusal = None;
            st.refused_at = None;
            st.cached = Some(v.clone());
        }
        VaultRead::Empty => {
            st.refusal = None;
            st.refused_at = None;
            st.cached = Some(Vault::new());
        }
        VaultRead::Refused(e) => {
            st.refusal = Some(e.clone());
            st.refused_at = Some(Instant::now());
            st.cached = None;
            log::error(format!("keychain: cannot read the shellder vault: {}. The secrets are still in it; nothing here treats this as an empty vault.", e.trim_end_matches('.')));
        }
    }
    outcome
}

fn vault() -> Vault {
    match read(false) {
        VaultRead::Ok(v) => v,
        _ => Vault::new(),
    }
}

pub fn get(host: &str, kind: SecretKind) -> Option<String> {
    vault().get(host)?.get(kind.raw()).cloned()
}

pub fn has(host: &str, kind: SecretKind) -> bool {
    get(host, kind).is_some()
}

/// Hosts that have at least one secret stored.
pub fn hosts() -> Vec<String> {
    vault().keys().cloned().collect()
}

/// Writes ask the keychain afresh: a save is something the user just did,
/// so one more dialog is fine there, and a vault that still cannot be read
/// is not written over.
pub fn set(host: &str, kind: SecretKind, secret: &str) -> Result<(), String> {
    let mut v = writable(read(true))?;
    v.entry(host.to_string())
        .or_default()
        .insert(kind.raw().to_string(), secret.to_string());
    save(v)
}

pub fn delete(host: &str, kind: SecretKind) -> Result<(), String> {
    let mut v = writable(read(true))?;
    if let Some(h) = v.get_mut(host) {
        h.remove(kind.raw());
        if h.is_empty() {
            v.remove(host);
        }
    }
    save(v)
}

fn save(v: Vault) -> Result<(), String> {
    let data = serde_json::to_vec(&v).map_err(|e| e.to_string())?;
    backend::write(&data)?;
    state().cached = Some(v);
    stamp();
    Ok(())
}

/// Every write touches a file in the state directory with the writer's pid,
/// so a running app notices a save made by another process (the CLI, a
/// second copy of the app) and reads the vault again.
fn stamp() {
    config::ensure_dirs();
    let _ = std::fs::write(&config::get().vault_stamp, std::process::id().to_string());
}

/// (when the stamp changed, who wrote it)
pub fn stamp_state() -> Option<(std::time::SystemTime, u32)> {
    let p = &config::get().vault_stamp;
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    let pid = std::fs::read_to_string(p).ok()?.trim().parse().ok()?;
    Some((m, pid))
}

/// Another process wrote the vault: the next read asks the keychain.
pub fn invalidate() {
    state().cached = None;
}

#[cfg(target_os = "macos")]
mod backend {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::data::CFData;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use security_framework_sys::base::{
        errSecItemNotFound, errSecSuccess, SecCopyErrorMessageString,
    };
    use security_framework_sys::item::*;
    use security_framework_sys::keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemUpdate};

    use super::{VAULT_ACCOUNT, VAULT_SERVICE};

    fn key(k: core_foundation::string::CFStringRef) -> CFString {
        unsafe { CFString::wrap_under_get_rule(k) }
    }

    fn describe(status: i32) -> String {
        unsafe {
            let s = SecCopyErrorMessageString(status, std::ptr::null_mut());
            if s.is_null() {
                format!("OSStatus {status}")
            } else {
                CFString::wrap_under_create_rule(s).to_string()
            }
        }
    }

    fn query() -> Vec<(CFString, CFType)> {
        unsafe {
            vec![
                (key(kSecClass), key(kSecClassGenericPassword).as_CFType()),
                (
                    key(kSecAttrService),
                    CFString::new(VAULT_SERVICE).as_CFType(),
                ),
                (
                    key(kSecAttrAccount),
                    CFString::new(VAULT_ACCOUNT).as_CFType(),
                ),
            ]
        }
    }

    fn dict(pairs: Vec<(CFString, CFType)>) -> CFDictionary<CFString, CFType> {
        CFDictionary::from_CFType_pairs(&pairs)
    }

    pub fn read() -> Result<Option<Vec<u8>>, String> {
        let mut q = query();
        unsafe {
            q.push((key(kSecReturnData), CFBoolean::true_value().as_CFType()));
            q.push((key(kSecMatchLimit), CFNumber::from(1i32).as_CFType()));
        }
        let q = dict(q);
        let mut item: core_foundation::base::CFTypeRef = std::ptr::null();
        let status = unsafe { SecItemCopyMatching(q.as_concrete_TypeRef(), &mut item) };
        if status == errSecSuccess {
            if item.is_null() {
                return Ok(None);
            }
            let data = unsafe { CFData::wrap_under_create_rule(item as _) };
            return Ok(Some(data.bytes().to_vec()));
        }
        if status == errSecItemNotFound {
            return Ok(None);
        }
        Err(describe(status))
    }

    /// Write the vault in place: SecItemUpdate keeps the item's access list
    /// and partition, so a signature that can read the vault can also write
    /// it. (Delete + add used to be the only path; on an existing vault the
    /// delete can fail with "Invalid attempt to change the owner of this
    /// item", and when it works it is a second password dialog.)
    pub fn write(data: &[u8]) -> Result<(), String> {
        let q = dict(query());
        let update = unsafe {
            dict(vec![(
                key(kSecValueData),
                CFData::from_buffer(data).as_CFType(),
            )])
        };
        let up = unsafe { SecItemUpdate(q.as_concrete_TypeRef(), update.as_concrete_TypeRef()) };
        if up == errSecItemNotFound {
            let mut add = query();
            unsafe {
                add.push((key(kSecValueData), CFData::from_buffer(data).as_CFType()));
                add.push((
                    key(kSecAttrLabel),
                    CFString::new("shellder vault").as_CFType(),
                ));
            }
            let add = dict(add);
            let st = unsafe { SecItemAdd(add.as_concrete_TypeRef(), std::ptr::null_mut()) };
            if st != errSecSuccess {
                return Err(describe(st));
            }
            return Ok(());
        }
        if up != errSecSuccess {
            return Err(describe(up));
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod backend {
    use super::{VAULT_ACCOUNT, VAULT_SERVICE};

    fn entry() -> Result<keyring::Entry, String> {
        keyring::Entry::new(VAULT_SERVICE, VAULT_ACCOUNT).map_err(|e| e.to_string())
    }

    pub fn read() -> Result<Option<Vec<u8>>, String> {
        match entry()?.get_secret() {
            Ok(data) => Ok(Some(data)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn write(data: &[u8]) -> Result<(), String> {
        entry()?.set_secret(data).map_err(|e| e.to_string())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod backend {
    pub fn read() -> Result<Option<Vec<u8>>, String> {
        Err("no keychain on this platform".into())
    }
    pub fn write(_data: &[u8]) -> Result<(), String> {
        Err("no keychain on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    //! A keychain that refuses is not an empty vault: the decisions that
    //! used to get this wrong. Nothing here touches the real vault.
    use super::*;

    fn vault() -> Vault {
        let mut v = Vault::new();
        v.entry("h".into())
            .or_default()
            .insert("password".into(), "s".into());
        v
    }

    #[test]
    fn a_read_is_one_of_three_things() {
        let data = serde_json::to_vec(&vault()).unwrap();
        assert_eq!(interpret(Ok(Some(data))), VaultRead::Ok(vault()));
        assert_eq!(interpret(Ok(None)), VaultRead::Empty);
        for why in [
            "User interaction is not allowed.",
            "The user name or passphrase you entered is not correct.",
        ] {
            assert_eq!(interpret(Err(why.into())), VaultRead::Refused(why.into()));
        }
    }

    #[test]
    fn a_refusal_is_repeated_for_a_while() {
        let t0 = Instant::now();
        assert!(!should_ask(Some(t0), t0 + Duration::from_secs(60), false));
        assert!(should_ask(Some(t0), t0 + REFUSAL_TTL, false));
        assert!(should_ask(Some(t0), t0, true));
        assert!(should_ask(None, t0, false));
    }

    #[test]
    fn a_refused_read_is_not_written_over() {
        assert!(writable(VaultRead::Refused("no".into())).is_err());
        assert_eq!(writable(VaultRead::Empty).unwrap(), Vault::new());
        assert_eq!(writable(VaultRead::Ok(vault())).unwrap(), vault());
    }
}
