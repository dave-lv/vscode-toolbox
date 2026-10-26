// Renders the app icon as an .icns: a blue tile with a white `</>`, the same mark as the menu bar icon. Drawn
// from plain shapes (no fonts or symbol sets), so it carries no third-party artwork.
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

    func point(_ x: CGFloat, _ y: CGFloat) -> NSPoint { NSPoint(x: tile.minX + x * u, y: tile.minY + y * u) }

    // `</>` in white: the menu bar icon's strokes (scripts/make-tray-icon.swift) scaled to the tile.
    let code = NSBezierPath()
    code.lineWidth = 6 * u
    code.lineCapStyle = .round
    code.lineJoinStyle = .round
    code.move(to: point(36.8, 64.85))
    code.line(to: point(21.95, 50))
    code.line(to: point(36.8, 35.15))
    code.move(to: point(63.2, 64.85))
    code.line(to: point(78.05, 50))
    code.line(to: point(63.2, 35.15))
    code.move(to: point(55.8, 69.8))
    code.line(to: point(44.2, 30.2))
    NSColor.white.setStroke()
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
