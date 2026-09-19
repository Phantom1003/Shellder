import AppKit
import Foundation

/// A localized string outside a SwiftUI view: menu items, window titles,
/// tooltips and messages built from pieces. SwiftUI's own Text, Button,
/// Toggle and friends localize their literals by themselves. The key is the
/// English text, the translations live in Resources/<lang>.lproj.
func L(_ key: String.LocalizationValue) -> String { String(localized: key) }

enum Localization {
    /// The language this process started with. Settings compares the choice
    /// against it to know when a relaunch is due.
    static var launched = "en"

    /// Languages with a translation in the bundle, with each one's name in
    /// itself ("English", "简体中文").
    static var available: [(id: String, name: String)] {
        Bundle.main.localizations
            .filter { Bundle.main.path(forResource: "Localizable", ofType: "strings", inDirectory: nil, forLocalization: $0) != nil }
            .sorted()
            .map { id in
                let name = Locale(identifier: id).localizedString(forIdentifier: id) ?? id
                return (id, name.prefix(1).uppercased() + name.dropFirst())
            }
    }
}

extension SecretKind {
    /// As a heading: "Password".
    var localizedTitle: String {
        switch self {
        case .password: return L("Password")
        case .passphrase: return L("Key passphrase")
        case .totp: return L("TOTP secret")
        }
    }

    /// Inside a sentence: "the stored password".
    var noun: String {
        switch self {
        case .password: return L("password")
        case .passphrase: return L("key passphrase")
        case .totp: return L("TOTP secret")
        }
    }
}

extension SSH.IdleMode {
    /// The pop-up entry. `title` stays English for the log.
    var localizedTitle: String {
        switch self {
        case .none: return L("no session (-N)")
        case .shell: return L("idle shell (pty)")
        case .cat: return L("cat on stdin")
        }
    }
}
