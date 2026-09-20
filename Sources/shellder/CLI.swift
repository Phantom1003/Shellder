import Foundation

/// Terminal subcommands (the same binary, run with arguments).
enum CLI {
    static let usage = """
    usage: shellder <command> [args]

      hosts                       Host entries from ~/.ssh/config and their ControlPath
      status                      master state for every locked host
      lock HOST | unlock HOST     lock a host (reconnect after drops, connect at launch) or unlock it
      add-secret HOST KIND        store password | passphrase | totp in the keychain
      del-secret HOST KIND        remove one stored secret
      install                     register as a login item (LaunchAgent) and start
      uninstall                   stop and remove the LaunchAgent
      help

    Run with no arguments to start the app (--background: no window for this
    launch; --login: a login-item launch, which honours "start silently").
    """

    static func run(_ args: [String]) -> Int32 {
        Log.alsoStderr = isatty(STDERR_FILENO) != 0
        guard let cmd = args.first else { print(usage); return 2 }
        let rest = Array(args.dropFirst())
        Keychain.reownIfNeeded()
        switch cmd {
        case "status": return status()
        case "hosts": return hosts()
        case "lock": return setLocked(rest, true)
        case "unlock": return setLocked(rest, false)
        case "add-secret": return addSecret(rest)
        case "del-secret": return delSecret(rest)
        case "install": return install()
        case "uninstall": return uninstall()
        case "help", "-h", "--help": print(usage); return 0
        default:
            fputs("unknown command: \(cmd)\n\(usage)\n", stderr)
            return 2
        }
    }

    private static func pad(_ s: String, _ n: Int) -> String {
        s.count >= n ? s : s + String(repeating: " ", count: n - s.count)
    }

    private static func status() -> Int32 {
        let locked = Prefs.lockedHosts
        if locked.isEmpty { print("no hosts are locked (lock one in the app or with `shellder lock HOST`)"); return 0 }
        let known = Set(HostCatalog.load().entries.map { $0.alias })
        // Jump hosts from the config are kept up for the hosts going through
        // them, lock or no lock: list those too.
        var rows: [(host: String, note: String)] = locked.map { ($0, "") }
        var i = 0
        while i < rows.count {
            let h = rows[i].host
            i += 1
            guard let j = (try? SSH.resolve(h))?.firstJumpHost, known.contains(j), j != h,
                  !rows.contains(where: { $0.host == j }) else { continue }
            rows.append((j, "  (jump host for \(h))"))
        }
        for (h, note) in rows {
            do {
                guard let cp = try SSH.controlPath(h) else {
                    print("\(pad(h, 24)) no ControlPath configured")
                    continue
                }
                print("\(pad(h, 24)) \(pad(SSH.masterAlive(h) ? "UP" : "down", 8)) \(cp)\(note)")
            } catch {
                print("\(pad(h, 24)) ERROR \(error)")
            }
        }
        return 0
    }

    private static func hosts() -> Int32 {
        let cat = HostCatalog.load()
        let locked = Set(Prefs.lockedHosts)
        for e in cat.entries {
            let mark = locked.contains(e.alias) ? "*" : " "
            do {
                let cp = try SSH.controlPath(e.alias) ?? "(no ControlPath)"
                print("\(mark) \(pad(e.alias, 24)) \(cp)")
            } catch {
                print("\(mark) \(pad(e.alias, 24)) ERROR \(error)")
            }
        }
        print("(* = locked, from \(cat.files.map(Config.abbreviateHome).joined(separator: ", ")))")
        return 0
    }

    private static func setLocked(_ a: [String], _ on: Bool) -> Int32 {
        guard let host = a.first else { fputs("usage: \(on ? "lock" : "unlock") HOST\n", stderr); return 2 }
        let known = HostCatalog.load().entries.map { $0.alias }
        if !known.contains(host) { fputs("warning: \(host) is not a Host entry in ~/.ssh/config\n", stderr) }
        Prefs.setLocked(host, on)
        print("\(host): \(on ? "locked" : "unlocked") (the running app picks this up within a few seconds)")
        // Same rules as the lock buttons in the app: locking takes the jump
        // hosts along, unlocking the hosts that jump through this one.
        func chain(_ host: String) -> [String] {
            var out: [String] = []
            var seen: Set<String> = [host]
            var h = host
            while let j = (try? SSH.resolve(h))?.firstJumpHost, known.contains(j), seen.insert(j).inserted {
                out.append(j)
                h = j
            }
            return out
        }
        if on {
            var via = host
            for j in chain(host) {
                Prefs.setLocked(j, true)
                print("\(j): locked (jump host for \(via))")
                via = j
            }
        } else {
            for d in Prefs.lockedHosts where d != host && chain(d).contains(host) {
                Prefs.setLocked(d, false)
                print("\(d): unlocked (jumps through \(host))")
            }
        }
        return 0
    }

    private static func parseHostKind(_ a: [String]) -> (String, SecretKind)? {
        guard a.count == 2, let k = SecretKind(rawValue: a[1]) else {
            fputs("usage: HOST password|passphrase|totp\n", stderr)
            return nil
        }
        return (a[0], k)
    }

    /// Interactive: hidden prompt on the tty. Piped stdin: first line, so
    /// scripts can do `printf '%s' "$PW" | shellder add-secret HOST password`.
    private static func readSecret(_ prompt: String) -> String? {
        if isatty(STDIN_FILENO) == 0 {
            guard let line = readLine(strippingNewline: true) else { return nil }
            return line
        }
        guard let cs = getpass(prompt) else { return nil }
        return String(cString: cs)
    }

    private static func addSecret(_ a: [String]) -> Int32 {
        guard let hk = parseHostKind(a) else { return 2 }
        let (host, kind) = hk
        let prompt = kind == .totp ? "TOTP secret (base32 or otpauth:// URI) for \(host): "
                                   : "\(kind.rawValue) for \(host): "
        guard let value = readSecret(prompt), !value.isEmpty else { fputs("aborted\n", stderr); return 1 }
        do {
            if kind == .totp {
                let t = try TOTP(parsing: value)
                try Keychain.set(host, kind, value)
                print("stored. current code: \(t.code())  (should match your authenticator app)")
            } else {
                try Keychain.set(host, kind, value)
                print("stored \(kind.rawValue) for \(host)")
            }
            return 0
        } catch {
            fputs("error: \(error)\n", stderr)
            return 1
        }
    }

    private static func delSecret(_ a: [String]) -> Int32 {
        guard let hk = parseHostKind(a) else { return 2 }
        let (host, kind) = hk
        Keychain.delete(host, kind)
        print("deleted \(kind.rawValue) for \(host)")
        return 0
    }

    private static func install() -> Int32 {
        do {
            try Launchd.install(bootstrap: true)
            print("installed and started \(Config.label)")
            print("log: \(Config.logFile)")
            for h in Prefs.lockedHosts {
                if (try? SSH.controlPath(h)) == nil {
                    print("WARNING: \(h) has no ControlPath in ~/.ssh/config")
                }
            }
            return 0
        } catch {
            fputs("error: \(error)\n", stderr)
            return 1
        }
    }

    private static func uninstall() -> Int32 {
        Launchd.uninstall(bootout: true)
        print("removed \(Config.label)")
        return 0
    }
}
