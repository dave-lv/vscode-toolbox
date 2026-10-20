// Renders the app icon as an .icns: a blue tile with a white toolbox, `</>` on its front. Drawn from plain
// shapes (no fonts or symbol sets), so it carries no third-party artwork.
// To regenerate the app's icons: swift scripts/make-icon.swift /tmp/AppIcon.icns, then
// iconutil -c iconset /tmp/AppIcon.icns and `npx tauri icon <the 1024 px PNG> -o src-tauri/icons` (in app/).
import AppKit

let output = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "AppIcon.icns"

func render(_ px: Int) -> Data {
    let size = CGFloat(px)
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

    // macOS icon grid: the tile is ~80% of the canvas.
    let inset = size * 0.1
    let tile = NSRect(x: inset, y: inset, width: size - 2 * inset, height: size - 2 * inset)
    let u = tile.width / 100 // drawing unit: 1% of the tile
    let tilePath = NSBezierPath(roundedRect: tile, xRadius: 22.5 * u, yRadius: 22.5 * u)
    NSGradient(colors: [
        NSColor(red: 0.16, green: 0.56, blue: 0.93, alpha: 1),
        NSColor(red: 0.04, green: 0.33, blue: 0.70, alpha: 1),
    ])!.draw(in: tilePath, angle: -90)

    func rect(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat) -> NSRect {
        NSRect(x: tile.minX + x * u, y: tile.minY + y * u, width: w * u, height: h * u)
    }
    func point(_ x: CGFloat, _ y: CGFloat) -> NSPoint { NSPoint(x: tile.minX + x * u, y: tile.minY + y * u) }

    // Toolbox: the handle, the box, and a band where the lid meets the box.
    let handle = NSBezierPath(roundedRect: rect(35, 62, 30, 16), xRadius: 5 * u, yRadius: 5 * u)
    handle.lineWidth = 6 * u
    NSColor.white.setStroke()
    handle.stroke()
    NSColor.white.setFill()
    NSBezierPath(roundedRect: rect(16, 20, 68, 46), xRadius: 8 * u, yRadius: 8 * u).fill()
    NSColor(red: 0.04, green: 0.33, blue: 0.70, alpha: 0.18).setFill()
    NSBezierPath(rect: rect(16, 50, 68, 4)).fill()

    // `</>` on the front, in the tile's blue.
    let code = NSBezierPath()
    code.lineWidth = 5.5 * u
    code.lineCapStyle = .round
    code.lineJoinStyle = .round
    code.move(to: point(37, 44))
    code.line(to: point(28, 35))
    code.line(to: point(37, 26))
    code.move(to: point(63, 44))
    code.line(to: point(72, 35))
    code.line(to: point(63, 26))
    code.move(to: point(54, 46))
    code.line(to: point(46, 24))
    NSColor(red: 0.07, green: 0.42, blue: 0.82, alpha: 1).setStroke()
    code.stroke()

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let iconset = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("AppIcon.iconset")
try? FileManager.default.removeItem(at: iconset)
try! FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    try! render(base).write(to: iconset.appendingPathComponent("icon_\(base)x\(base).png"))
    try! render(base * 2).write(to: iconset.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
let p = Process()
p.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
p.arguments = ["-c", "icns", iconset.path, "-o", output]
try! p.run()
p.waitUntilExit()
exit(p.terminationStatus)
