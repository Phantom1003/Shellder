import AppKit
import Combine
import Foundation

struct UpdateError: Error, CustomStringConvertible {
    let description: String
    init(_ d: String) { description = d }
}

/// Checks GitHub Releases for a newer Shellder and installs it in place.
///
/// A release is a `vX.Y` tag with one asset, `Shellder-vX.Y.zip`, the CI
/// build (see .github/workflows/build.yml). Installing means: download the
/// zip, unpack it, make sure it holds a Shellder bundle of the promised
/// version with a valid signature, swap it for the running bundle and
/// relaunch. Automatic checks run shortly after launch and once a day.
/// Nothing is installed without a click on "Install and relaunch".
final class Updater: ObservableObject {
    struct Release: Equatable {
        /// "1.3": the tag without its v.
        let version: String
        let asset: URL
        /// The release page, for the notes.
        let page: URL
    }

    enum State: Equatable {
        case idle
        case checking
        case upToDate
        case available(Release)
        /// Fraction downloaded, nil while the size is unknown.
        case downloading(Release, Double?)
        case installing(Release)
        case failed(String)
    }

    static let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?"
    static let interval: TimeInterval = 24 * 3600

    @Published private(set) var state: State = .idle
    /// Check at launch and daily. Off, the "Check now" button is the only check.
    @Published var automatic = Prefs.autoUpdate {
        didSet {
            Prefs.autoUpdate = automatic
            schedule()
        }
    }

    var available: Release? {
        if case .available(let r) = state { return r }
        return nil
    }

    private var busy: Bool {
        switch state {
        case .checking, .downloading, .installing: return true
        default: return false
        }
    }

    private var timer: Timer?
    private var download: Download?
    private let work = DispatchQueue(label: Config.label + ".updater", qos: .utility)

    func start() { schedule() }

    func stop() {
        timer?.invalidate()
        timer = nil
    }

    private func schedule() {
        stop()
        guard automatic else { return }
        timer = Timer.scheduledTimer(withTimeInterval: Self.interval, repeats: true) { [weak self] _ in self?.check() }
        // The launch has enough to do, look a few seconds later.
        DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in
            if let s = self, s.automatic { s.check() }
        }
    }

    // MARK: check

    /// Ask the release endpoint for the latest version. A manual check
    /// reports its failure in Settings, an automatic one only in the log.
    func check(manual: Bool = false) {
        guard !busy else { return }
        let before = state
        state = .checking
        var req = URLRequest(url: Config.updateAPI)
        req.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        req.cachePolicy = .reloadIgnoringLocalCacheData
        req.timeoutInterval = 20
        URLSession.shared.dataTask(with: req) { [weak self] data, resp, err in
            let outcome = Self.latest(data, resp, err)
            DispatchQueue.main.async {
                guard let self = self else { return }
                switch outcome {
                case .success(let r?):
                    Log.info("update: version \(r.version) is available (running \(Self.version))")
                    self.state = .available(r)
                case .success(nil):
                    self.state = .upToDate
                case .failure(let e):
                    Log.warn("update check failed: \(e)")
                    self.state = manual ? .failed(L("Check failed: \(e.description)")) : before
                }
            }
        }.resume()
    }

    /// The newer release in the endpoint's reply, nil when the running
    /// version is the latest.
    static func latest(_ data: Data?, _ resp: URLResponse?, _ err: Error?) -> Result<Release?, UpdateError> {
        if let err = err { return .failure(UpdateError(err.localizedDescription)) }
        if let http = resp as? HTTPURLResponse, http.statusCode != 200 {
            return .failure(UpdateError("HTTP \(http.statusCode)"))
        }
        guard let data = data,
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let tag = json["tag_name"] as? String,
              let page = (json["html_url"] as? String).flatMap(URL.init(string:)) else {
            return .failure(UpdateError("unexpected reply from \(Config.updateAPI.host ?? "the release endpoint")"))
        }
        let latest = tag.hasPrefix("v") ? String(tag.dropFirst()) : tag
        guard isNewer(latest, than: version) else { return .success(nil) }
        let assets = json["assets"] as? [[String: Any]] ?? []
        let zips = assets.compactMap { a -> URL? in
            guard let name = a["name"] as? String, name.hasSuffix(".zip"),
                  let url = a["browser_download_url"] as? String else { return nil }
            return URL(string: url)
        }
        guard let asset = zips.first else { return .failure(UpdateError("release \(tag) has no zip asset")) }
        return .success(Release(version: latest, asset: asset, page: page))
    }

    /// Dotted numbers compared part by part, missing parts count as 0
    /// ("1.3" > "1.2.9", "1.2.0" == "1.2"). Anything else counts as 0 too.
    static func isNewer(_ a: String, than b: String) -> Bool {
        let pa = a.split(separator: ".").map { Int($0) ?? 0 }
        let pb = b.split(separator: ".").map { Int($0) ?? 0 }
        for i in 0..<max(pa.count, pb.count) {
            let x = i < pa.count ? pa[i] : 0
            let y = i < pb.count ? pb[i] : 0
            if x != y { return x > y }
        }
        return false
    }

    // MARK: install

    func install() {
        guard let r = available else { return }
        state = .downloading(r, nil)
        let bundle = Bundle.main.bundleURL
        Log.info("update: downloading \(r.asset.lastPathComponent)")
        download = Download(r.asset, progress: { [weak self] fraction in
            DispatchQueue.main.async {
                if case .downloading = self?.state { self?.state = .downloading(r, fraction) }
            }
        }, done: { [weak self] result in
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.download = nil
                switch result {
                case .failure(let e):
                    Log.error("update: download failed: \(e)")
                    self.state = .failed(L("Update failed: \(e.description)"))
                case .success(let zip):
                    self.state = .installing(r)
                    self.work.async {
                        do {
                            try Self.replace(bundle, with: zip, expecting: r.version)
                            DispatchQueue.main.async {
                                Log.info("update: installed \(r.version) at \(bundle.path), relaunching")
                                Relaunch.now(bundle.path)
                            }
                        } catch {
                            Log.error("update: install failed: \(error)")
                            DispatchQueue.main.async { self.state = .failed(L("Update failed: \(String(describing: error))")) }
                        }
                    }
                }
            }
        })
    }

    /// Unpack the zip next to itself, check what came out, then swap it for
    /// the bundle at `current`. The two moves are renames inside the same
    /// directory, so the app never half exists. The old bundle is deleted,
    /// the process that runs from it keeps its mapped binary until it quits.
    private static func replace(_ current: URL, with zip: URL, expecting version: String) throws {
        let fm = FileManager.default
        if current.path.contains("/AppTranslocation/") {
            throw UpdateError(L("Shellder runs from a quarantined copy, move it to Applications first"))
        }
        let dir = zip.deletingLastPathComponent().appendingPathComponent("unpacked")
        try? fm.removeItem(at: dir)
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        let unzip = run("/usr/bin/ditto", ["-x", "-k", zip.path, dir.path])
        guard unzip.status == 0 else { throw UpdateError("ditto: \(unzip.stderr)") }

        let apps = (try? fm.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? []
        guard let app = apps.first(where: { $0.pathExtension == "app" }),
              let data = try? Data(contentsOf: app.appendingPathComponent("Contents/Info.plist")),
              let info = (try? PropertyListSerialization.propertyList(from: data, format: nil)) as? [String: Any] else {
            throw UpdateError(L("the archive holds no app"))
        }
        guard info["CFBundleIdentifier"] as? String == Config.label else {
            throw UpdateError(L("the download is not Shellder"))
        }
        let got = info["CFBundleShortVersionString"] as? String ?? "?"
        guard got == version else {
            throw UpdateError(L("the download is version \(got), not \(version)"))
        }
        let sig = run("/usr/bin/codesign", ["--verify", "--strict", app.path])
        guard sig.status == 0 else { throw UpdateError(L("code signature check failed: \(sig.stderr)")) }
        // Not quarantined by URLSession, but a belt for the braces.
        _ = run("/usr/bin/xattr", ["-dr", "com.apple.quarantine", app.path])

        let old = current.deletingLastPathComponent().appendingPathComponent("." + current.lastPathComponent + ".old")
        try? fm.removeItem(at: old)
        do { try fm.moveItem(at: current, to: old) } catch {
            throw UpdateError(L("cannot replace \(Config.abbreviateHome(current.path)): \(error.localizedDescription)"))
        }
        do { try fm.moveItem(at: app, to: current) } catch {
            try? fm.moveItem(at: old, to: current)
            throw UpdateError(L("cannot replace \(Config.abbreviateHome(current.path)): \(error.localizedDescription)"))
        }
        try? fm.removeItem(at: old)
        try? fm.removeItem(at: zip.deletingLastPathComponent())
    }

    private static func run(_ binary: String, _ args: [String]) -> (status: Int32, stderr: String) {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: binary)
        p.arguments = args
        p.standardInput = FileHandle.nullDevice
        p.standardOutput = FileHandle.nullDevice
        let err = Pipe()
        p.standardError = err
        do { try p.run() } catch { return (-1, "\(error)") }
        let e = err.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        return (p.terminationStatus, String(decoding: e, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines))
    }
}

/// One download to a directory of our own, with progress. The session keeps
/// this object alive until the task ends.
private final class Download: NSObject, URLSessionDownloadDelegate {
    private let progress: (Double?) -> Void
    private let done: (Result<URL, UpdateError>) -> Void
    private var session: URLSession!
    private var saved: Result<URL, UpdateError>?

    init(_ url: URL, progress: @escaping (Double?) -> Void, done: @escaping (Result<URL, UpdateError>) -> Void) {
        self.progress = progress
        self.done = done
        super.init()
        session = URLSession(configuration: .ephemeral, delegate: self, delegateQueue: nil)
        var req = URLRequest(url: url)
        req.timeoutInterval = 60
        session.downloadTask(with: req).resume()
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64,
                    totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        progress(totalBytesExpectedToWrite > 0 ? Double(totalBytesWritten) / Double(totalBytesExpectedToWrite) : nil)
    }

    /// The file at `location` is gone once this returns, so move it now.
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        let fm = FileManager.default
        let dir = fm.temporaryDirectory.appendingPathComponent("shellder-update-\(getpid())")
        let dest = dir.appendingPathComponent("update.zip")
        do {
            try? fm.removeItem(at: dir)
            try fm.createDirectory(at: dir, withIntermediateDirectories: true)
            try fm.moveItem(at: location, to: dest)
            saved = .success(dest)
        } catch {
            saved = .failure(UpdateError(error.localizedDescription))
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        session.finishTasksAndInvalidate()
        if let e = error { return done(.failure(UpdateError(e.localizedDescription))) }
        if let http = task.response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            return done(.failure(UpdateError("HTTP \(http.statusCode)")))
        }
        done(saved ?? .failure(UpdateError("the download left no file")))
    }
}
