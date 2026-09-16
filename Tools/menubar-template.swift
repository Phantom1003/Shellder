// Turn a coloured picture into a monochrome template image for the menu bar:
// dark pixels (outlines, shadows) become opaque, light ones transparent, so
// macOS can tint the result white or black like its own status icons.
//   swift Tools/menubar-template.swift IN.png OUT.png SIZE [dark|light]
// "dark" (default) keeps the dark pixels, "light" keeps the light ones.
import AppKit

let args = CommandLine.arguments
guard args.count >= 4, let size = Int(args[3]),
      let src = NSImage(contentsOfFile: args[1]),
      let cg = src.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    FileHandle.standardError.write("usage: menubar-template IN OUT SIZE\n".data(using: .utf8)!)
    exit(2)
}

// Work at 4x the target size, then downsample for smooth edges.
let work = size * 4
let cs = CGColorSpaceCreateDeviceRGB()
guard let ctx = CGContext(data: nil, width: work, height: work, bitsPerComponent: 8, bytesPerRow: work * 4,
                          space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { exit(1) }
ctx.interpolationQuality = .high
ctx.draw(cg, in: CGRect(x: 0, y: 0, width: work, height: work))
guard let px = ctx.data?.assumingMemoryBound(to: UInt8.self) else { exit(1) }

// alpha = source alpha * "darkness" (or "lightness" in light mode); the ramp
// runs between luminance lo and hi. Tune lo/hi to taste.
let keepLight = args.count > 4 && args[4] == "light"
// The same ramp in both modes: mid-tones stay semi-transparent, which keeps
// the shading of the picture instead of flattening it into one shape.
let lo = 0.28, hi = 0.55
for i in 0..<(work * work) {
    let o = i * 4
    let a = Double(px[o + 3]) / 255
    guard a > 0 else { continue }
    // un-premultiply
    let r = Double(px[o]) / 255 / a, g = Double(px[o + 1]) / 255 / a, b = Double(px[o + 2]) / 255 / a
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
    let dark = max(0, min(1, (hi - lum) / (hi - lo)))
    let out = a * (keepLight ? 1 - dark : dark)
    px[o] = 0; px[o + 1] = 0; px[o + 2] = 0           // black, premultiplied
    px[o + 3] = UInt8(max(0, min(255, out * 255)))
}
guard let big = ctx.makeImage() else { exit(1) }
guard let small = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: size * 4,
                            space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { exit(1) }
small.interpolationQuality = .high
small.draw(big, in: CGRect(x: 0, y: 0, width: size, height: size))
guard let result = small.makeImage() else { exit(1) }
let rep = NSBitmapImageRep(cgImage: result)
guard let png = rep.representation(using: .png, properties: [:]) else { exit(1) }
try! png.write(to: URL(fileURLWithPath: args[2]))
