// A display that is no screen: the window server lays it out beside the
// real ones and composites what is on it at its own refresh rate, and
// nothing of it is seen. For `Tests/gui-bench.sh`, whose windows then
// keep off the user's desktop. CGVirtualDisplay is CoreGraphics' own
// class, not in its headers (what screen-sharing tools make their extra
// displays with), reached here by name. Prints the display's frame in
// the desktop's points, "x y width height", and stays until its
// standard input ends or it is killed; the display goes with it.
//
//   swiftc -O display.swift -o display && ./display [width height]

import AppKit
import CoreGraphics

func fail(_ why: String) -> Never {
    FileHandle.standardError.write("no virtual display: \(why)\n".data(using: .utf8)!)
    exit(1)
}

let arguments = CommandLine.arguments
let width = arguments.count > 2 ? UInt(arguments[1]) ?? 1600 : 1600
let height = arguments.count > 2 ? UInt(arguments[2]) ?? 1000 : 1000

guard let Descriptor = NSClassFromString("CGVirtualDisplayDescriptor") as? NSObject.Type,
    let Settings = NSClassFromString("CGVirtualDisplaySettings") as? NSObject.Type,
    let Mode: AnyClass = NSClassFromString("CGVirtualDisplayMode"),
    let Display: AnyClass = NSClassFromString("CGVirtualDisplay")
else { fail("this macOS has no CGVirtualDisplay") }

let descriptor = Descriptor.init()
descriptor.setValue(DispatchQueue.main, forKey: "queue")
descriptor.setValue("Shellder bench", forKey: "name")
// Twice the points: a display of two pixels a point, as a Mac's own.
descriptor.setValue(width * 2, forKey: "maxPixelsWide")
descriptor.setValue(height * 2, forKey: "maxPixelsHigh")
descriptor.setValue(NSValue(size: NSSize(width: 520, height: 325)), forKey: "sizeInMillimeters")
descriptor.setValue(0x5348, forKey: "vendorID")
descriptor.setValue(0x4C44, forKey: "productID")
descriptor.setValue(1, forKey: "serialNum")

typealias InitMode = @convention(c) (AnyObject, Selector, UInt, UInt, Double) -> Unmanaged<AnyObject>?
typealias Apply = @convention(c) (AnyObject, Selector, AnyObject) -> Bool

/// `[cls alloc]`, for an init to take.
func allocated(_ cls: AnyClass) -> AnyObject {
    (cls as AnyObject).perform(NSSelectorFromString("alloc")).takeUnretainedValue()
}

guard
    let display = allocated(Display).perform(NSSelectorFromString("initWithDescriptor:"), with: descriptor)?
        .takeUnretainedValue() as? NSObject
else { fail("the window server refused it") }

let initModeSel = NSSelectorFromString("initWithWidth:height:refreshRate:")
guard let initModeImp = class_getMethodImplementation(Mode, initModeSel) else { fail("no mode") }
let initMode = unsafeBitCast(initModeImp, to: InitMode.self)
guard let mode = initMode(allocated(Mode), initModeSel, width, height, 60)?.takeUnretainedValue() else { fail("no mode") }
let settings = Settings.init()
settings.setValue(1, forKey: "hiDPI")
settings.setValue([mode], forKey: "modes")
let applySel = NSSelectorFromString("applySettings:")
guard let applyImp = class_getMethodImplementation(Display, applySel) else { fail("no applySettings:") }
guard unsafeBitCast(applyImp, to: Apply.self)(display, applySel, settings) else { fail("its settings were refused") }

guard let id = display.value(forKey: "displayID") as? UInt32, id != 0 else { fail("it has no id") }
// The window server places it a moment after it is made.
var bounds = CGRect.zero
for _ in 0..<50 {
    bounds = CGDisplayBounds(id)
    if bounds.width > 0 { break }
    usleep(100_000)
}
guard bounds.width > 0 else { fail("it was never placed") }
// A display of its own only if the desktop is left as it was: one that
// the window server made the main display, mirrored onto a screen or
// laid over one would put the test's windows in front of the user.
if CGDisplayIsMain(id) != 0 { fail("the window server made it the main display") }
if CGDisplayIsInMirrorSet(id) != 0 { fail("the window server mirrors it") }
var online = [CGDirectDisplayID](repeating: 0, count: 16)
var count: UInt32 = 0
CGGetOnlineDisplayList(16, &online, &count)
for other in online.prefix(Int(count)) where other != id && CGDisplayBounds(other).intersects(bounds) {
    fail("it lies over another display")
}
print("\(Int(bounds.minX)) \(Int(bounds.minY)) \(Int(bounds.width)) \(Int(bounds.height))")
fflush(stdout)

// Gone when whoever started it is: its standard input ends.
DispatchQueue.global().async {
    while let _ = readLine() {}
    exit(0)
}
signal(SIGTERM) { _ in exit(0) }
withExtendedLifetime(display) { RunLoop.main.run() }
