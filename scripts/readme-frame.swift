// Puts a window capture on a soft, blurred backdrop with a shadow under it,
// the way the README shows its pictures: a window alone on the page's white
// has no edge. The capture is what `screencapture -o -l` writes (the window
// without its shadow, transparent rounded corners); the result is a PNG
// with a border of `pad` points around the window. Used by
// readme-screenshot.sh: swift scripts/readme-frame.swift IN.png OUT.png
import AppKit
import CoreImage

let args = CommandLine.arguments
guard args.count == 3, let data = FileManager.default.contents(atPath: args[1]),
    let source = NSBitmapImageRep(data: data), let window = source.cgImage
else {
    FileHandle.standardError.write("usage: readme-frame.swift IN.png OUT.png\n".data(using: .utf8)!)
    exit(2)
}

// Everything in pixels of the capture, so a 2x capture gets a 2x frame.
let scale = CGFloat(source.pixelsWide) / source.size.width
let w = CGFloat(window.width), h = CGFloat(window.height)
let pad = (40 * scale).rounded()
let W = w + 2 * pad, H = h + 2 * pad

// The backdrop: a diagonal wash of two tints with three soft blobs over
// it, then blurred until nothing has an edge.
func color(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> CIColor { CIColor(red: r, green: g, blue: b) }
var backdrop = CIFilter(name: "CILinearGradient", parameters: [
    "inputPoint0": CIVector(x: 0, y: H), "inputColor0": color(0.78, 0.84, 0.98),
    "inputPoint1": CIVector(x: W, y: 0), "inputColor1": color(0.96, 0.82, 0.90),
])!.outputImage!.cropped(to: CGRect(x: 0, y: 0, width: W, height: H))
let blobs: [(CGFloat, CGFloat, CGFloat, CIColor)] = [
    (0.18, 0.72, 0.55, color(0.62, 0.72, 0.99)),
    (0.82, 0.30, 0.60, color(0.98, 0.75, 0.86)),
    (0.55, 0.95, 0.45, color(0.70, 0.93, 0.96)),
]
for (x, y, r, c) in blobs {
    let blob = CIFilter(name: "CIRadialGradient", parameters: [
        "inputCenter": CIVector(x: x * W, y: y * H),
        "inputRadius0": 0, "inputRadius1": r * min(W, H),
        "inputColor0": c, "inputColor1": CIColor(red: c.red, green: c.green, blue: c.blue, alpha: 0),
    ])!.outputImage!
    backdrop = blob.composited(over: backdrop)
}
backdrop = backdrop.clampedToExtent()
    .applyingFilter("CIGaussianBlur", parameters: ["inputRadius": pad * 0.6])
    .cropped(to: CGRect(x: 0, y: 0, width: W, height: H))

// Draw it, then the window with its shadow, through AppKit.
let out = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(W), pixelsHigh: Int(H),
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
let ctx = NSGraphicsContext(bitmapImageRep: out)!
NSGraphicsContext.current = ctx
let cg = ctx.cgContext
let ci = CIContext(cgContext: cg, options: nil)
ci.draw(backdrop, in: CGRect(x: 0, y: 0, width: W, height: H), from: backdrop.extent)
let frame = CGRect(x: pad, y: pad, width: w, height: h)
// The shadow follows the window's own shape (its transparent corners).
cg.setShadow(offset: CGSize(width: 0, height: -8 * scale), blur: 24 * scale,
    color: CGColor(gray: 0, alpha: 0.35))
cg.draw(window, in: frame)
NSGraphicsContext.restoreGraphicsState()

guard let png = out.representation(using: .png, properties: [:]) else { exit(1) }
try! png.write(to: URL(fileURLWithPath: args[2]))
