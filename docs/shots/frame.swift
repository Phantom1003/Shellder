// A page's picture as the window it is on macOS, on a soft, blurred
// backdrop with a shadow under it: the README's and the front page's
// pictures (shoot.py, the scenes with `window`). The picture is what
// shoot.swift writes, the page alone, drawn under the mac class so that
// its top is clear of the traffic lights; here it gets the window's round
// corners, its edge and the three lights, which no window of the app's
// was on screen to show. The result is a PNG with a border of `pad`
// points around the window.
//
//   swiftc -O frame.swift -o frame && ./frame IN.png OUT.png [PIXELS A POINT]
import AppKit
import CoreImage

let args = CommandLine.arguments
guard args.count >= 3, let data = FileManager.default.contents(atPath: args[1]),
    let source = NSBitmapImageRep(data: data), let page = source.cgImage
else {
    FileHandle.standardError.write("usage: frame IN.png OUT.png [PIXELS A POINT]\n".data(using: .utf8)!)
    exit(2)
}

// Everything in pixels of the picture, so a 2x picture gets a 2x frame.
let scale = CGFloat(args.count > 3 ? Double(args[3]) ?? 2 : 2)
let w = CGFloat(page.width), h = CGFloat(page.height)
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

// The window: macOS 26's corner for one without a toolbar, the shadow
// following that shape.
let frame = CGRect(x: pad, y: pad, width: w, height: h)
let corner = 16 * scale
let shape = CGPath(roundedRect: frame, cornerWidth: corner, cornerHeight: corner, transform: nil)
cg.saveGState()
cg.setShadow(offset: CGSize(width: 0, height: -8 * scale), blur: 24 * scale,
    color: CGColor(gray: 0, alpha: 0.35))
cg.addPath(shape)
cg.setFillColor(CGColor(gray: 1, alpha: 1))
cg.fillPath()
cg.restoreGState()
cg.saveGState()
cg.addPath(shape)
cg.clip()
cg.interpolationQuality = .none
cg.draw(page, in: frame)
// The traffic lights where an overlaid title bar has them: 14 points
// across, the first 16 points in from the corner, 23 points apart.
let lights: [(CGFloat, CGFloat, CGFloat)] = [(1.0, 0.373, 0.341), (0.996, 0.737, 0.180), (0.157, 0.784, 0.251)]
for (i, (r, g, b)) in lights.enumerated() {
    let centre = CGPoint(x: pad + (16 + 23 * CGFloat(i)) * scale, y: pad + h - 16 * scale)
    let light = CGRect(x: centre.x - 7 * scale, y: centre.y - 7 * scale, width: 14 * scale, height: 14 * scale)
    cg.setFillColor(CGColor(srgbRed: r, green: g, blue: b, alpha: 1))
    cg.fillEllipse(in: light)
    cg.setStrokeColor(CGColor(gray: 0, alpha: 0.12))
    cg.setLineWidth(0.5 * scale)
    cg.strokeEllipse(in: light.insetBy(dx: 0.25 * scale, dy: 0.25 * scale))
}
cg.restoreGState()
// The window's edge, a hairline inside its shape.
cg.addPath(CGPath(roundedRect: frame.insetBy(dx: 0.5, dy: 0.5), cornerWidth: corner - 0.5, cornerHeight: corner - 0.5, transform: nil))
cg.setStrokeColor(CGColor(gray: 0, alpha: 0.18))
cg.setLineWidth(1)
cg.strokePath()
NSGraphicsContext.restoreGraphicsState()

guard let png = out.representation(using: .png, properties: [:]) else { exit(1) }
try! png.write(to: URL(fileURLWithPath: args[2]))
