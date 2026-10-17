import AppKit

let output = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
let variants = [(16,1),(16,2),(32,1),(32,2),(128,1),(128,2),(256,1),(256,2),(512,1),(512,2)]
for (points, scale) in variants {
    let size = points * scale
    let image = NSImage(size: NSSize(width: size, height: size))
    image.lockFocus()
    let context = NSGraphicsContext.current!.cgContext
    context.scaleBy(x: CGFloat(size) / 1024, y: CGFloat(size) / 1024)
    let rect = NSRect(x: 90, y: 90, width: 844, height: 844)
    let path = NSBezierPath(roundedRect: rect, xRadius: 190, yRadius: 190)
    NSGradient(starting: NSColor(srgbRed: 0.15, green: 0.43, blue: 0.72, alpha: 1), ending: NSColor(srgbRed: 0.08, green: 0.23, blue: 0.43, alpha: 1))!.draw(in: path, angle: -70)
    // The bundle mark uses original vector strokes; SF Symbols remain native UI assets only.
    let arrows = NSBezierPath()
    arrows.lineWidth = 62; arrows.lineCapStyle = .round; arrows.lineJoinStyle = .round
    arrows.move(to: NSPoint(x: 310, y: 630)); arrows.line(to: NSPoint(x: 720, y: 630))
    arrows.move(to: NSPoint(x: 634, y: 718)); arrows.line(to: NSPoint(x: 722, y: 630)); arrows.line(to: NSPoint(x: 634, y: 542))
    arrows.move(to: NSPoint(x: 714, y: 394)); arrows.line(to: NSPoint(x: 304, y: 394))
    arrows.move(to: NSPoint(x: 392, y: 482)); arrows.line(to: NSPoint(x: 304, y: 394)); arrows.line(to: NSPoint(x: 392, y: 306))
    NSColor.white.setStroke(); arrows.stroke()
    image.unlockFocus()
    guard let tiff = image.tiffRepresentation, let bitmap = NSBitmapImageRep(data: tiff), let data = bitmap.representation(using: .png, properties: [:]) else { fatalError("Cannot render app icon") }
    let name = "icon_\(points)x\(points)" + (scale == 2 ? "@2x" : "") + ".png"
    try data.write(to: output.appendingPathComponent(name))
}
