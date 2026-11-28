import Foundation

/// A picture that a person added to the message that they write.
public struct ChatDraftPicture: Identifiable, Equatable, Sendable {
    public let id = UUID()
    public let data: Data

    /// The pictures that the runtime takes in one message, and their bytes together.
    public static let most = 8
    static let mostBytes = 32 * 1024 * 1024

    public enum Refusal: Sendable {
        /// The bytes are no PNG, JPEG, GIF or WebP picture.
        case unreadable
        /// The message would hold more pictures, or more of their bytes, than it takes.
        case full
    }

    /// PNG, JPEG, GIF and WebP, the pictures that the runtime takes, by their first bytes.
    public static func isPicture(_ data: Data) -> Bool {
        let head = [UInt8](data.prefix(12))
        return head.starts(with: [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
            || head.starts(with: [0xFF, 0xD8, 0xFF])
            || head.starts(with: Array("GIF87a".utf8)) || head.starts(with: Array("GIF89a".utf8))
            || (head.count == 12 && head.starts(with: Array("RIFF".utf8)) && head[8 ..< 12].elementsEqual("WEBP".utf8))
    }
}
