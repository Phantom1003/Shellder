import Foundation

/// One drag: what to copy, from which side, and into which directory on
/// the other one. Either side can be this Mac or a host, so a copy between
/// two hosts is a job like any other.
struct TransferJob: Identifiable {
    let id = UUID()
    let from: FileSource
    let to: FileSource
    /// The item to copy, on the side it comes from.
    let source: String
    /// The directory it goes into, on the side it lands on.
    let destination: String

    var name: String { (source as NSString).lastPathComponent }
    /// Where the copy ends up: the destination directory plus the name.
    var landing: String { RemoteFS.join(destination, name) }
    /// Into a host is up, onto this Mac is down: which arrow it draws.
    var isUpload: Bool { !to.isLocal }
}

/// One job's scp, with its progress followed by measuring what has arrived
/// on the destination side. scp only draws its own meter on a terminal, and
/// watching the destination works the same way in both directions.
final class TransferRun {
    private let lock = NSLock()
    private var process: Process?
    private var cancelled = false
    private var finished = false

    /// Stop the copy. Whatever has already landed stays there.
    func cancel() {
        lock.lock()
        cancelled = true
        let p = process
        lock.unlock()
        p?.terminate()
    }

    var isCancelled: Bool {
        lock.lock(); defer { lock.unlock() }
        return cancelled
    }

    /// Bytes to move, measured on the source side. nil when the side could
    /// not answer, which leaves the progress bar indeterminate.
    static func total(_ job: TransferJob) -> Int64? { FS.bytes(job.from, job.source) }

    /// Runs scp to completion. `progress` is called on an arbitrary queue
    /// with a fraction of `total` while the copy runs, never after it ends.
    func run(_ job: TransferJob, total: Int64?, progress: @escaping (Double) -> Void) -> SSH.Result {
        if let t = total, t > 0 { watch(job, total: t, progress: progress) }
        defer {
            lock.lock()
            finished = true
            lock.unlock()
        }
        let onStart: (Process) -> Void = { [weak self] p in
            guard let self = self else { return }
            self.lock.lock()
            self.process = p
            let stop = self.cancelled
            self.lock.unlock()
            if stop { p.terminate() }
        }
        return SSH.copy([job.source], on: job.from, into: job.destination, on: job.to, onStart: onStart)
    }

    /// Asks the destination side how much of the copy is there yet, until
    /// the copy ends. A remote answer costs an ssh session, so it is asked
    /// for less often than a local one.
    private func watch(_ job: TransferJob, total: Int64, progress: @escaping (Double) -> Void) {
        let every = job.to.isLocal ? 0.4 : 1.0
        DispatchQueue.global(qos: .utility).async { [weak self] in
            while true {
                Thread.sleep(forTimeInterval: every)
                guard let self = self, !self.isOver else { return }
                let landed = FS.bytes(job.to, job.landing)
                guard !self.isOver, let done = landed else { return }
                // The last percent is the process exiting, not a measurement.
                progress(min(0.99, Double(done) / Double(total)))
            }
        }
    }

    private var isOver: Bool {
        lock.lock(); defer { lock.unlock() }
        return finished || cancelled
    }
}
