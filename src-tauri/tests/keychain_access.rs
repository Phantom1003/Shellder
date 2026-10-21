//! The "Try again" for a keychain that refused: the model asks the keychain
//! once more, says what it found, and comes back. It once locked its own
//! state twice on one thread when the vault opened (a guard taken in a
//! match's scrutinee lives through the arms), and every window's call then
//! waited on that lock for good. No keychain is touched: a copy with
//! SHELLDER_NO_KEYCHAIN reads an empty vault, which is the "opened" path.
use std::sync::mpsc;
use std::time::Duration;

use shellder::app::model::Model;

#[test]
fn asking_the_keychain_again_comes_back() {
    let dir = std::env::temp_dir().join(format!("shellder-keychain-access-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("SHELLDER_STATE_DIR", &dir);
    std::env::set_var("SHELLDER_NO_KEYCHAIN", "1");

    let model = Model::new();
    let (tx, rx) = mpsc::channel();
    let m = model.clone();
    std::thread::spawn(move || {
        m.request_keychain_access();
        let _ = tx.send(());
    });
    assert!(
        rx.recv_timeout(Duration::from_secs(10)).is_ok(),
        "request_keychain_access did not come back: the model waits on its own state lock"
    );
    assert_eq!(model.snapshot().keychain_refusal, None);
    let _ = std::fs::remove_dir_all(&dir);
}
