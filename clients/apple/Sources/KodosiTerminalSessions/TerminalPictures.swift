import Foundation
import ImageIO
import UniformTypeIdentifiers

public struct TerminalPicture: Equatable, Sendable {
    public let sent: UInt64
    public let total: UInt64
    public let arrived: Bool

    public var fraction: Double {
        total == 0 ? 0 : min(1, Double(sent) / Double(total))
    }
}

enum TerminalPictureFile {
    private static let readable: Set<String> = Set([UTType.png, .jpeg, .gif, .webP].map(\.identifier))

    /// Agents read PNG, JPEG, GIF and WebP, so other formats become PNG.
    static func readable(_ picture: Data) -> Data? {
        guard let source = CGImageSourceCreateWithData(picture as CFData, nil),
              let type = CGImageSourceGetType(source) as String?
        else { return nil }
        if readable.contains(type) {
            return picture
        }
        guard let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
        let png = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(png, UTType.png.identifier as CFString, 1, nil)
        else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? png as Data : nil
    }

    static func write(_ picture: Data) async throws -> URL {
        let file = FileManager.default.temporaryDirectory.appending(path: "kodosi-picture-\(UUID().uuidString)")
        try await Task.detached {
            guard let readable = readable(picture) else {
                throw CocoaError(.fileReadCorruptFile)
            }
            try readable.write(to: file, options: [.atomic, .completeFileProtection])
        }.value
        return file
    }
}
