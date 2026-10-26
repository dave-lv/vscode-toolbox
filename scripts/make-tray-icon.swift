// Renders the tray icons: `</>` drawn from plain strokes (the app icon has the same mark), on transparent.
// macOS: tray.png, black, which macOS uses as a template image and tints. Windows and Linux trays don't tint,
// so they get tray/white-<px>.png for dark taskbars and tray/black-<px>.png for light ones, each drawn at the
// tray's size for the display scaling (16 px at 100%, 20 at 125%, 24 at 150%, 32 at 200%) so it stays sharp.
// Usage: swift scripts/make-tray-icon.swift [app/src-tauri/icons]
import AppKit

let dir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "app/src-tauri/icons"

// The mark on a 44-unit square: two chevrons and the slash between them.
let strokes: [[NSPoint]] = [
    [NSPoint(x: 14, y: 31), NSPoint(x: 5, y: 22), NSPoint(x: 14, y: 13)],
    [NSPoint(x: 30, y: 31), NSPoint(x: 39, y: 22), NSPoint(x: 30, y: 13)],
    [NSPoint(x: 25.5, y: 34), NSPoint(x: 18.5, y: 10)],
]

/// `fill`: scaled to fill the square edge to edge (a tray icon's whole slot) rather than the menu bar's margins.
func write(_ path: String, px: Int, color: NSColor, fill: Bool) {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let size = CGFloat(px)
    // Bolder when small, so the strokes survive at 16 px.
    let width = fill ? max(1.6, size * 0.115) : 3.6 * size / 44
    let scale = fill ? (size - 0.1 * size - width) / 34 : size / 44
    let code = NSBezierPath()
    code.lineWidth = width
    code.lineCapStyle = .round
    code.lineJoinStyle = .round
    for line in strokes {
        for (i, p) in line.enumerated() {
            let q = NSPoint(x: size / 2 + (p.x - 22) * scale, y: size / 2 + (p.y - 22) * scale)
            i == 0 ? code.move(to: q) : code.line(to: q)
        }
    }
    color.setStroke()
    code.stroke()
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
    print("wrote \(path)")
}

write("\(dir)/tray.png", px: 44, color: .black, fill: false) // 22 pt menu bar height at 2x
try! FileManager.default.createDirectory(atPath: "\(dir)/tray", withIntermediateDirectories: true)
for px in [16, 20, 24, 32] {
    write("\(dir)/tray/white-\(px).png", px: px, color: .white, fill: true)
    write("\(dir)/tray/black-\(px).png", px: px, color: .black, fill: true)
}
