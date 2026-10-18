import AppKit
import CoreText

// 1024x1024 Graphite DarkBot app icon; no external font asset.
let size: CGFloat = 1024
let output = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "DarkBot.png"
func color(_ v: UInt32, _ alpha: CGFloat = 1) -> NSColor {
    NSColor(srgbRed: CGFloat((v >> 16) & 255) / 255, green: CGFloat((v >> 8) & 255) / 255,
            blue: CGFloat(v & 255) / 255, alpha: alpha)
}
guard let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 1024, pixelsHigh: 1024,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
    let graphics = NSGraphicsContext(bitmapImageRep: rep) else { fatalError("bitmap") }
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = graphics
let ctx = graphics.cgContext
ctx.clear(CGRect(x: 0, y: 0, width: size, height: size))
let body = CGRect(x: 100, y: 100, width: 824, height: 824)
let shape = NSBezierPath(roundedRect: body, xRadius: 185, yRadius: 185)
NSGraphicsContext.saveGraphicsState()
let shadow = NSShadow()
shadow.shadowColor = color(0x000000, 0.45)
shadow.shadowOffset = NSSize(width: 0, height: -12)
shadow.shadowBlurRadius = 28
shadow.set()
color(0x1b1b1b).setFill()
shape.fill()
NSGraphicsContext.restoreGraphicsState()
NSGraphicsContext.saveGraphicsState()
shape.addClip()
guard let gradient = NSGradient(starting: color(0x3a3a3a), ending: color(0x1b1b1b)) else { fatalError("gradient") }
gradient.draw(in: body, angle: -90)
NSGraphicsContext.restoreGraphicsState()
let outline = NSBezierPath(roundedRect: body.insetBy(dx: 2, dy: 2), xRadius: 183, yRadius: 183)
outline.lineWidth = 4
color(0x464646).setStroke()
outline.stroke()
let font = NSFont.systemFont(ofSize: 560, weight: .heavy) as CTFont
var character: UniChar = 0x44
var glyph = CGGlyph()
guard CTFontGetGlyphsForCharacters(font, &character, &glyph, 1),
      let path = CTFontCreatePathForGlyph(font, glyph, nil) else { fatalError("D glyph") }
let bounds = path.boundingBox
var translation = CGAffineTransform(translationX: body.midX - bounds.midX, y: body.midY - bounds.midY)
guard let centered = path.copy(using: &translation) else { fatalError("transform") }
ctx.addPath(centered)
ctx.setFillColor(color(0xeaeaea).cgColor)
ctx.fillPath()
NSGraphicsContext.restoreGraphicsState()
guard let png = rep.representation(using: .png, properties: [:]) else { fatalError("png") }
try png.write(to: URL(fileURLWithPath: output))
print("Icon written: \(output)")
