// Render the maintainer-supplied artwork unchanged into platform-required icon sizes.
// Run from the repository root: swift scripts/make_brand_icons.swift
import AppKit
import Foundation
let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let source = root.appendingPathComponent("assets/branding/logo.png")
guard let image = NSImage(contentsOf: source) else { fatalError("Missing branding source") }
for size in [16, 32, 48, 64, 128, 256, 512, 1024] {
    let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    bitmap.size = NSSize(width: size, height: size)
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: bitmap)!
    NSGraphicsContext.current = context
    context.imageInterpolation = .high
    let factor = Double(size) * 0.94 / max(image.size.width, image.size.height)
    let scaled = NSSize(width: image.size.width * factor, height: image.size.height * factor)
    image.draw(in: NSRect(x: (Double(size)-scaled.width)/2, y: (Double(size)-scaled.height)/2, width: scaled.width, height: scaled.height), from: .zero, operation: .copy, fraction: 1)
    NSGraphicsContext.restoreGraphicsState()
    let data = bitmap.representation(using: .png, properties: [:])!
    try data.write(to: root.appendingPathComponent("assets/branding/icons/icon-\(size).png"))
}
