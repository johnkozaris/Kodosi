import Foundation

/// Tells a voice from the steady sound around the speaker, such as a fan or a street, by how far
/// the sound rises above it. Recognized words arrive late, so this is what shows that a person
/// still talks.
struct TalkVoice: Sendable {
    /// The steady sound, in decibels: it drops at once to quieter sound and rises slowly, so the
    /// short gaps between words keep it down while a person talks.
    private var floor: Double?
    private var lastSound: Date?
    private static let rise = 3.0
    private static let margin = 12.0
    private static let quietest = -60.0

    init() {}

    /// Takes the loudness of the latest sound and returns whether it is a voice.
    mutating func hears(_ decibels: Double, at time: Date) -> Bool {
        let elapsed = lastSound.map { time.timeIntervalSince($0) } ?? 0
        lastSound = time
        guard let floor else {
            floor = decibels
            return false
        }
        self.floor = min(decibels, floor + Self.rise * elapsed)
        return decibels > max(floor, Self.quietest) + Self.margin
    }
}
