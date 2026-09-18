import AppKit
import SwiftUI

/// Makes every scroll view in the window use overlay scrollers whatever the
/// system setting says. With "Show scroll bars: Always" AppKit draws legacy
/// scrollers, whose track is an opaque white strip that no background
/// colour reaches. Overlay scrollers have no track. main.swift sets the
/// app-wide preference, this probe catches scroll views that were styled
/// before that took hold. Applied as a background of a ScrollView or List:
/// `.background(OverlayScrollers())`. A background view sits next to the
/// scroll view rather than inside it, so the probe does not rely on
/// `enclosingScrollView` and sweeps the window instead, once more a moment
/// later for scroll views SwiftUI adds after it.
struct OverlayScrollers: NSViewRepresentable {
    func makeNSView(context: Context) -> Probe { Probe() }
    func updateNSView(_ view: Probe, context: Context) { view.apply() }

    final class Probe: NSView {
        private var observer: NSObjectProtocol?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            apply()
            // AppKit resets the style when the system preference changes.
            observer = observer ?? NotificationCenter.default.addObserver(
                forName: NSScroller.preferredScrollerStyleDidChangeNotification, object: nil, queue: .main
            ) { [weak self] _ in self?.apply() }
        }

        deinit {
            if let o = observer { NotificationCenter.default.removeObserver(o) }
        }

        func apply() {
            guard let root = window?.contentView else { return }
            Self.sweep(root)
            DispatchQueue.main.async { [weak self] in
                if let root = self?.window?.contentView { Self.sweep(root) }
            }
        }

        private static func sweep(_ view: NSView) {
            if let scroll = view as? NSScrollView, scroll.scrollerStyle != .overlay {
                scroll.scrollerStyle = .overlay
            }
            for sub in view.subviews { sweep(sub) }
        }
    }
}
