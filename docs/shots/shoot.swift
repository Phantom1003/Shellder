// A page drawn by the system's WebKit, the app's engine, in a window that
// is on no screen, and saved as a picture: for the documentation's
// pictures (shoot.py), which no window of the app's and nothing of the
// desktop is in. The page leaves a JSON text in `window.__SHOT` when it
// is as the picture wants it (stub.js), with the part of it to take
// (`crop`, in the page's pixels); that text is printed, the picture
// written at two pixels a point.
//
//   swiftc -O shoot.swift -o shoot && ./shoot URL WIDTH HEIGHT OUT.png [seconds]

import AppKit
import WebKit

let arguments = CommandLine.arguments
guard arguments.count > 4, let url = URL(string: arguments[1]), let width = Double(arguments[2]),
    let height = Double(arguments[3])
else {
    FileHandle.standardError.write("usage: shoot URL WIDTH HEIGHT OUT.png [seconds]\n".data(using: .utf8)!)
    exit(2)
}
let out = arguments[4]
let deadline = Date().addingTimeInterval(arguments.count > 5 ? Double(arguments[5]) ?? 40 : 40)
// Two pixels a point whatever the screen: the view is twice the size and
// the page zoomed by two, so it lays out at WIDTH by HEIGHT.
let scale = 2.0

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let window = NSWindow(
    contentRect: NSRect(x: -20000, y: -20000, width: width * scale, height: height * scale),
    styleMask: [.borderless], backing: .buffered, defer: false)
window.appearance = NSAppearance(named: .aqua)
let configuration = WKWebViewConfiguration()
// Nothing kept from one picture to the next (a Console's layout is).
configuration.websiteDataStore = .nonPersistent()
let view = WKWebView(frame: window.contentView!.bounds, configuration: configuration)
// A window nobody sees counts as hidden otherwise: no frames, no timers.
view.setValue(false, forKey: "windowOcclusionDetectionEnabled")
view.pageZoom = scale
window.contentView!.addSubview(view)
window.orderFrontRegardless()
view.load(URLRequest(url: url))

func fail(_ why: String) -> Never {
    FileHandle.standardError.write("\(why)\n".data(using: .utf8)!)
    exit(1)
}

var taking = false
Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { _ in
    if taking { return }
    if Date() > deadline { fail("the page did not get ready in time") }
    view.evaluateJavaScript("window.__SHOT || ''") { result, _ in
        guard !taking, let text = result as? String, !text.isEmpty else { return }
        taking = true
        var rect = CGRect(x: 0, y: 0, width: width * scale, height: height * scale)
        if let data = text.data(using: .utf8),
            let shot = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let crop = shot["crop"] as? [Double], crop.count == 4
        {
            rect = CGRect(x: crop[0] * scale, y: crop[1] * scale, width: crop[2] * scale, height: crop[3] * scale)
        }
        let snapshot = WKSnapshotConfiguration()
        snapshot.rect = rect
        snapshot.snapshotWidth = NSNumber(value: Double(rect.width))
        view.takeSnapshot(with: snapshot) { image, error in
            guard let image = image, let tiff = image.tiffRepresentation, let bitmap = NSBitmapImageRep(data: tiff),
                let png = bitmap.representation(using: .png, properties: [:])
            else { fail("no picture: \(error?.localizedDescription ?? "")") }
            do { try png.write(to: URL(fileURLWithPath: out)) } catch { fail("\(error)") }
            print(text)
            exit(0)
        }
    }
}
app.run()
