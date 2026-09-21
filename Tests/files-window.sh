#!/bin/sh
# The Files window's model without a window: a throwaway sshd in Docker, the
# app's own sources built with a test main that drives FilesModel the way the
# panes do (listing, opening a directory, going up, dropping rows both ways,
# queueing, cancelling, a copy that fails). See AGENTS.md.
# Usage: Tests/files-window.sh
set -eu
HOST=shjt-files
DIR=/tmp/shjt-files
export SHELLDER_SSH_CONFIG=$DIR/config SHELLDER_STATE_DIR=$DIR SHELLDER_PREFS_SUITE=local.shellder.test

cleanup() {
    ssh -F $DIR/config -O exit $HOST >/dev/null 2>&1 || true
    docker rm -f shjt-files >/dev/null 2>&1 || true
    rm -rf $DIR
}
trap cleanup EXIT
cleanup
mkdir -p $DIR/here
docker run -d --name shjt-files -p 127.0.0.1:2225:22 alpine sh -c \
    'apk add -q openssh && ssh-keygen -A && adduser -D t && echo t:pw | chpasswd && /usr/sbin/sshd -D -e' >/dev/null
ssh-keygen -q -t ed25519 -N '' -f $DIR/id -C shjt-files
cat > $DIR/config <<CFG
Host $HOST
    HostName 127.0.0.1
    Port 2225
    User t
    IdentityFile $DIR/id
    IdentitiesOnly yes
    ControlMaster auto
    ControlPath $DIR/cm-%C
    StrictHostKeyChecking no
    UserKnownHostsFile /dev/null
    LogLevel ERROR
CFG
for i in $(seq 1 60); do docker exec shjt-files pgrep -x sshd >/dev/null 2>&1 && break; sleep 1; done
sleep 1
docker exec shjt-files sh -c 'mkdir -p /home/t/.ssh && chmod 700 /home/t/.ssh && chown t:t /home/t/.ssh'
docker exec -i shjt-files sh -c 'cat > /home/t/.ssh/authorized_keys && chmod 600 /home/t/.ssh/authorized_keys && chown t:t /home/t/.ssh/authorized_keys' < $DIR/id.pub
# A tree with a hidden entry, a directory with a space in its name, a
# symlinked directory and something big enough to show progress.
docker exec shjt-files su t -c 'cd $HOME && mkdir -p Shellder/sub "my dir" && echo one > Shellder/one.txt \
    && echo two > Shellder/sub/two.txt && echo h > Shellder/.hidden && ln -sf Shellder link \
    && dd if=/dev/zero of=Shellder/big.bin bs=1M count=200 2>/dev/null \
    && dd if=/dev/zero of=huge.bin bs=1M count=600 2>/dev/null'
ssh -F $DIR/config -M -f -N $HOST

cat > $DIR/main.swift <<'SWIFT'
import AppKit

setbuf(stdout, nil)
_ = NSApplication.shared
NSApp.setActivationPolicy(.accessory)
let dir = ProcessInfo.processInfo.environment["SHELLDER_STATE_DIR"]!
let here = dir + "/here"
var failures: [String] = []

func pump(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }
func check(_ what: String, _ ok: Bool, _ detail: String = "") {
    print("\(ok ? "ok  " : "FAIL") \(what)\(detail.isEmpty ? "" : ": \(detail)")")
    if !ok { failures.append(what) }
}
/// Wait for a drop to turn into a copy and for that copy to end.
func settle(_ model: FilesModel) -> [Double] {
    var waited = 0.0
    while model.active == nil && waited < 5 { pump(0.2); waited += 0.2 }
    var seen: [Double] = []
    while model.active != nil {
        pump(0.25)
        if let p = model.progress, seen.last != p { seen.append(p) }
    }
    pump(0.5)
    return seen
}
func names(_ model: FilesModel, _ side: FilesModel.Side) -> [String] {
    model.rows(side).map(\.item.name)
}
/// What a drop hands the model: the pasteboard of the dragging session.
func dragged(_ payload: String) -> NSPasteboard {
    let board = NSPasteboard(name: NSPasteboard.Name("local.shellder.test.drag"))
    board.clearContents()
    board.setString(payload, forType: .string)
    return board
}
func draggedFromFinder(_ path: String) -> NSPasteboard {
    let board = NSPasteboard(name: NSPasteboard.Name("local.shellder.test.drag"))
    board.clearContents()
    board.writeObjects([URL(fileURLWithPath: path) as NSURL])
    return board
}

let model = FilesModel(host: "shjt-files")
model.start()
pump(5)

check("the host's tree opens in ~/Shellder", model.root(.remote).hasSuffix("/Shellder"), model.root(.remote))
check("it lists what is there", names(model, .remote) == ["sub", "big.bin", "one.txt"], "\(names(model, .remote))")
check("dot entries are out of the way", !names(model, .remote).contains(".hidden"))
model.toggleHidden(.remote)
pump(0.3)
check("until the pane is asked for them", names(model, .remote).contains(".hidden"), "\(names(model, .remote))")
model.toggleHidden(.remote)
pump(0.3)

if let sub = model.rows(.remote).first(where: { $0.item.name == "sub" }) {
    model.toggle(.remote, sub.item)
    pump(2)
    check("a directory opens under its own line",
          names(model, .remote) == ["sub", "two.txt", "big.bin", "one.txt"], "\(names(model, .remote))")
    model.toggle(.remote, sub.item)
    pump(0.3)
    check("and folds shut again", names(model, .remote) == ["sub", "big.bin", "one.txt"])
}

model.up(.remote)
pump(2)
check("up one level reaches the home directory", model.root(.remote) == "/home/t", model.root(.remote))
check("a symlinked directory is listed as one", names(model, .remote).contains("link"), "\(names(model, .remote))")

model.setRoot(.local, here)
pump(1)
check("this Mac's pane starts empty", names(model, .local).isEmpty)

// remote row dropped on the local pane: a download, with progress
_ = model.accept(dragged(FilesModel.payload(.remote, "/home/t/Shellder")), into: here, on: .local)
let down = settle(model)
check("dragging a directory over copies it here", !model.messageIsError, model.message)
check("the pane shows what arrived", names(model, .local) == ["Shellder"], "\(names(model, .local))")
check("with progress along the way", !down.isEmpty && down.allSatisfy { $0 > 0 && $0 < 1 }, "\(down)")
check("and every byte of it", LocalFS.bytes(here + "/Shellder") == RemoteFS.bytes("shjt-files", "/home/t/Shellder"))

// local row dropped on the host, into a directory with a space in its name
_ = model.accept(dragged(FilesModel.payload(.local, here + "/Shellder")), into: "/home/t/my dir", on: .remote)
_ = settle(model)
check("dragging it back copies it to the host", !model.messageIsError, model.message)
check("into a directory whose name has a space",
      RemoteFS.bytes("shjt-files", "/home/t/my dir/Shellder") == LocalFS.bytes(here + "/Shellder"))

// a file from the Finder is a file URL, not one of our rows
_ = model.accept(draggedFromFinder(here + "/Shellder/one.txt"), into: "/home/t/my dir", on: .remote)
_ = settle(model)
check("a file dragged in from the Finder uploads too",
      RemoteFS.bytes("shjt-files", "/home/t/my dir/one.txt") == 4, model.message)

// drops that mean nothing
_ = model.accept(dragged(FilesModel.payload(.local, here + "/Shellder")), into: here, on: .local)
_ = model.accept(dragged("text from another app"), into: here, on: .local)
pump(1)
check("a pane does not copy into itself, and stray text is ignored",
      model.active == nil && model.queued == 0)

// two drops queue up behind each other
_ = model.accept(dragged(FilesModel.payload(.remote, "/home/t/Shellder/big.bin")), into: here, on: .local)
_ = model.accept(dragged(FilesModel.payload(.remote, "/home/t/Shellder/one.txt")), into: here, on: .local)
var waited = 0.0
while model.active == nil && waited < 5 { pump(0.05); waited += 0.05 }
check("a second drop waits for the first", model.queued == 1, "queued \(model.queued)")
while model.active != nil { pump(0.2) }
pump(0.5)
check("and both of them land", names(model, .local).contains("big.bin") && names(model, .local).contains("one.txt"),
      "\(names(model, .local))")

// a copy stopped in the middle
_ = model.accept(dragged(FilesModel.payload(.remote, "/home/t/huge.bin")), into: here, on: .local)
waited = 0
while model.active == nil && waited < 5 { pump(0.05); waited += 0.05 }
model.cancel()
while model.active != nil { pump(0.1) }
check("cancelling says so instead of failing",
      !model.messageIsError && model.message.contains("huge.bin"), model.message)
let partial = LocalFS.bytes(here + "/huge.bin")
check("and the copy really stopped", partial < 600 * 1024 * 1024, "\(partial) bytes of 600 MB")

// where a drop on a row lands
check("a drop on a directory goes into it",
      model.destination(for: FileItem(name: "sub", path: "/home/t/Shellder/sub", isDir: true, size: 0), on: .remote)
        == "/home/t/Shellder/sub")
check("a drop on a file goes into the directory around it",
      model.destination(for: FileItem(name: "one.txt", path: "/home/t/Shellder/one.txt", isDir: false, size: 4), on: .remote)
        == "/home/t/Shellder")
check("a pane refuses a drag from its own side",
      !model.canAccept(dragged(FilesModel.payload(.local, here)), on: .local)
        && model.canAccept(dragged(FilesModel.payload(.local, here)), on: .remote))
check("and text that is not one of our rows",
      !model.canAccept(dragged("hello"), on: .remote) && !model.canAccept(dragged("hello"), on: .local))
check("a file from the Finder is for the host's side only",
      model.canAccept(draggedFromFinder(here), on: .remote) && !model.canAccept(draggedFromFinder(here), on: .local))

// a copy the host refuses
_ = model.accept(dragged(FilesModel.payload(.local, here + "/Shellder/one.txt")), into: "/etc", on: .remote)
_ = settle(model)
check("a refused copy shows what scp said", model.messageIsError && model.message.lowercased().contains("permission"),
      model.message)

// a listing the host cannot answer
model.setRoot(.remote, "/home/t/nope")
pump(2)
check("a directory that is not there shows the error", (model.errors[.remote] ?? "").contains("No such file"),
      model.errors[.remote] ?? "none")

print(failures.isEmpty ? "PASS: the Files window's model does what the panes ask of it"
                       : "FAIL: \(failures.count) check(s) failed")
exit(failures.isEmpty ? 0 : 1)
SWIFT
swiftc -o $DIR/filestest Sources/shellder/App/*.swift Sources/shellder/App/Tools/*.swift \
    Sources/shellder/Core/*.swift $DIR/main.swift
$DIR/filestest
