import AVFoundation

/// Turns the microphone's audio into the format that the recognizer reads.
public final class TalkConverter {
    private let converter: AVAudioConverter
    private let format: AVAudioFormat

    public init?(from microphone: AVAudioFormat, to format: AVAudioFormat) {
        guard let converter = AVAudioConverter(from: microphone, to: format) else { return nil }
        // Without priming, the converted audio keeps the timing of the microphone, which the
        // recognizer's times for the words depend on.
        converter.primeMethod = .none
        // The inputs of an audio interface are discrete channels with no mix, which the converter
        // turns into silence: the recognizer takes the first ones, where the level comes from too.
        if microphone.channelCount > format.channelCount {
            converter.channelMap = (0 ..< Int(format.channelCount)).map { NSNumber(value: $0) }
        }
        self.converter = converter
        self.format = format
    }

    /// Converts one buffer from the microphone, or gives nil when it can't.
    public func convert(_ buffer: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        let frames = (Double(buffer.frameLength) * format.sampleRate / buffer.format.sampleRate).rounded(.up)
        guard let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(frames) + 1) else { return nil }
        var supplied = false
        var error: NSError?
        let status = converter.convert(to: output, error: &error) { _, state in
            // The converter asks for input until it has enough; this buffer is all there is now.
            if supplied {
                state.pointee = .noDataNow
                return nil
            }
            supplied = true
            state.pointee = .haveData
            return buffer
        }
        guard status != .error, output.frameLength > 0 else { return nil }
        return output
    }
}
