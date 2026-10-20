// Renders the macOS menu bar icon: `</>` drawn from plain strokes (like the app icon's), black on transparent
// (macOS uses it as a template image and tints it). Windows and Linux show the app icon instead.
// Usage: swift scripts/make-tray-icon.swift app/src-tauri/icons/tray.png
import AppKit

let output = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "app/src-tauri/icons/tray.png"
let px = 44 // 22 pt menu bar height at 2x

let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
                           samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                           colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
let code = NSBezierPath()
code.lineWidth = 3.6
code.lineCapStyle = .round
code.lineJoinStyle = .round
code.move(to: NSPoint(x: 14, y: 31))
code.line(to: NSPoint(x: 5, y: 22))
code.line(to: NSPoint(x: 14, y: 13))
code.move(to: NSPoint(x: 30, y: 31))
code.line(to: NSPoint(x: 39, y: 22))
code.line(to: NSPoint(x: 30, y: 13))
code.move(to: NSPoint(x: 25.5, y: 34))
code.line(to: NSPoint(x: 18.5, y: 10))
NSColor.black.setStroke()
code.stroke()
NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output))
print("wrote \(output) (\(px)x\(px))")
