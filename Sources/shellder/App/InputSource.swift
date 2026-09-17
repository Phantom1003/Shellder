import Carbon

/// Keeps a password field on an ASCII keyboard layout.
///
/// A secure text field refuses input-method composition, but the keys still pass
/// through the active input method first. Apple's Pinyin treats lowercase "u" as a
/// mode prefix, so in a password field the key is swallowed with a beep. Switching
/// to the ASCII-capable layout while the field has focus, and back afterwards,
/// avoids that without touching the user's input method preference.
enum ASCIIInputSource {
    private static var saved: TISInputSource?

    /// Switch to the ASCII layout, remembering the source that was active.
    static func enter() {
        guard saved == nil,
              let current = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue(),
              let ascii = TISCopyCurrentASCIICapableKeyboardInputSource()?.takeRetainedValue(),
              id(current) != id(ascii),
              TISSelectInputSource(ascii) == noErr else { return }
        saved = current
    }

    /// Restore the source that was active before `enter()`.
    static func leave() {
        guard let previous = saved else { return }
        saved = nil
        TISSelectInputSource(previous)
    }

    private static func id(_ source: TISInputSource) -> String {
        guard let p = TISGetInputSourceProperty(source, kTISPropertyInputSourceID) else { return "" }
        return Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String
    }
}
