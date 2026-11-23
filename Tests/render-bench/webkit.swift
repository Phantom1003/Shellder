// Runs a page in the system's WebKit, the app's engine, in a window that
// is on no screen, and prints what the page wrote into #results once it
// says it is done. For `Tests/render-bench.sh`: the page asks (`want`
// in `__state()`) to have its terminal fed from here, and gets the
// pieces through `evaluateJavaScript`, which is how an event of the
// app's reaches a page, at the pace it names.
//
//   swiftc -O webkit.swift -o webkit && ./webkit URL [seconds]

import AppKit
import WebKit

let arguments = CommandLine.arguments
guard arguments.count > 1, let url = URL(string: arguments[1]) else {
    FileHandle.standardError.write("usage: webkit URL [seconds]\n".data(using: .utf8)!)
    exit(2)
}
let deadline = Date().addingTimeInterval(arguments.count > 2 ? Double(arguments[2]) ?? 600 : 600)

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let window = NSWindow(
    contentRect: NSRect(x: -4000, y: -4000, width: 1280, height: 900),
    styleMask: [.borderless], backing: .buffered, defer: false)
let view = WKWebView(frame: window.contentView!.bounds, configuration: WKWebViewConfiguration())
// A window nobody sees counts as hidden otherwise: no frames, no timers
// at their pace.
view.setValue(false, forKey: "windowOcclusionDetectionEnabled")
view.customUserAgent = "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 (KHTML, like Gecko) ShellderBench"
window.contentView!.addSubview(view)
window.orderFrontRegardless()
view.load(URLRequest(url: url))

var pushing = false

func finish(_ code: Int32) {
    view.evaluateJavaScript("document.getElementById('results').textContent") { text, _ in
        print(text as? String ?? "(the page has no #results)")
        exit(code)
    }
}

/// The page's piece, `count` times, each an event's worth of script, at
/// `rate` pieces a second: what was due by now goes out at every tick.
func push(_ renderer: String, _ count: Int, _ rate: Double) {
    pushing = true
    view.evaluateJavaScript("__piece()") { piece, _ in
        guard let piece = piece as? String else { finish(1); return }
        view.callAsyncJavaScript(
            "return await __pushBegin(renderer)", arguments: ["renderer": renderer], in: nil,
            in: .page
        ) { _ in
            let script = "__push({\"id\":1,\"data\":\"\(piece)\"})"
            let start = Date()
            var sent = 0
            Timer.scheduledTimer(withTimeInterval: 0.001, repeats: true) { timer in
                let due = min(count, Int(Date().timeIntervalSince(start) * rate) + 1)
                while sent < due {
                    view.evaluateJavaScript(script, completionHandler: nil)
                    sent += 1
                }
                if sent >= count {
                    timer.invalidate()
                    view.evaluateJavaScript("__pushEnd()") { _, _ in pushing = false }
                }
            }
        }
    }
}

Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { _ in
    if Date() > deadline {
        FileHandle.standardError.write("the page did not finish in time\n".data(using: .utf8)!)
        finish(1)
        return
    }
    if pushing { return }
    view.evaluateJavaScript("typeof __state === 'function' ? __state() : ''") { result, _ in
        guard let text = result as? String, let data = text.data(using: .utf8),
            let state = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return }
        if state["done"] as? Bool == true { finish(0); return }
        if !pushing, let want = state["want"] as? String, let count = state["count"] as? Int,
            let rate = state["rate"] as? Double
        {
            push(want, count, rate)
        }
    }
}

app.run()
