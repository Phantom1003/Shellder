#!/bin/sh
# A keychain that refuses is not an empty vault. Builds the app's own sources
# with a test main and checks the decisions that used to get this wrong:
# how a read is interpreted, when a refusal is asked again, and what a write
# may start from. Nothing here touches the real vault.
# Usage: Tests/keychain-refusal.sh
set -eu
DIR=$(mktemp -d)
trap 'rm -rf "$DIR"' EXIT

cat > "$DIR/main.swift" <<'SWIFT'
import Foundation
import Security

var failures: [String] = []
func check(_ what: String, _ ok: Bool, _ detail: String = "") {
    print("\(ok ? "ok  " : "FAIL") \(what)\(detail.isEmpty ? "" : ": \(detail)")")
    if !ok { failures.append(what) }
}

let vault: Keychain.Vault = ["h": ["password": "s"]]
let data = try! JSONEncoder().encode(vault)

check("a vault that reads is a vault", Keychain.interpret(errSecSuccess, data as CFData) == .ok(vault))
check("no item at all is an empty vault", Keychain.interpret(errSecItemNotFound, nil) == .empty)
for status in [errSecAuthFailed, errSecUserCanceled, errSecInteractionNotAllowed, errSecNotAvailable] {
    check("a refusal (\(status)) is not an empty vault", Keychain.interpret(status, nil) == .refused(status))
}

// A refusal is repeated for a while instead of asking again: the dialog
// dismissed at launch must not come back for every host that logs in.
let t0 = Date()
check("a refusal a minute old is repeated",
      !Keychain.shouldAsk(refusedAt: t0, now: t0.addingTimeInterval(60), force: false))
check("a refusal older than refusalTTL is asked again",
      Keychain.shouldAsk(refusedAt: t0, now: t0.addingTimeInterval(Keychain.refusalTTL), force: false))
check("Settings may ask right away",
      Keychain.shouldAsk(refusedAt: t0, now: t0, force: true))
check("no refusal on record means ask",
      Keychain.shouldAsk(refusedAt: nil, now: t0, force: false))

// A write never starts from a refused read: that would put the one secret
// being saved where every other one was.
check("a refused read is not written over",
      (try? Keychain.writable(.refused(errSecAuthFailed))) == nil)
check("an empty vault may be written", (try? Keychain.writable(.empty)) == [:])
check("a vault that reads may be written", (try? Keychain.writable(.ok(vault))) == vault)

print(failures.isEmpty ? "PASS: a keychain that refuses is told apart from an empty vault"
                       : "FAIL: \(failures.count) check(s) failed")
exit(failures.isEmpty ? 0 : 1)
SWIFT

swiftc -o "$DIR/keytest" Sources/shellder/Core/Keychain.swift Sources/shellder/Core/Config.swift \
    Sources/shellder/Core/Log.swift Sources/shellder/Core/Prefs.swift "$DIR/main.swift"
"$DIR/keytest"
