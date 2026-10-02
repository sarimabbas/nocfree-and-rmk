// Quick Look adds a white canvas. Remove only its edge-connected background;
// the white artwork enclosed by the blue tile remains untouched.
import AppKit

let input = URL(fileURLWithPath: CommandLine.arguments[1])
let output = URL(fileURLWithPath: CommandLine.arguments[2])
let image = NSBitmapImageRep(data: try Data(contentsOf: input))!
precondition(image.bitsPerSample == 8 && image.samplesPerPixel == 4 && !image.isPlanar)
let pixels = image.bitmapData!
let width = image.pixelsWide
let height = image.pixelsHigh
let alphaFirst = image.bitmapFormat.contains(.alphaFirst)
let alpha = alphaFirst ? 0 : 3
let red = alphaFirst ? 1 : 0
var queue = [0, width - 1, (height - 1) * width, height * width - 1]
var cursor = 0
while cursor < queue.count {
    let position = queue[cursor]
    cursor += 1
    let x = position % width
    let y = position / width
    let offset = y * image.bytesPerRow + x * 4
    guard pixels[offset + alpha] != 0,
          pixels[offset + red] >= 250,
          pixels[offset + red + 1] >= 250,
          pixels[offset + red + 2] >= 250 else { continue }
    pixels[offset + alpha] = 0
    if x > 0 { queue.append(position - 1) }
    if x + 1 < width { queue.append(position + 1) }
    if y > 0 { queue.append(position - width) }
    if y + 1 < height { queue.append(position + width) }
}
try image.representation(using: .png, properties: [:])!.write(to: output)
