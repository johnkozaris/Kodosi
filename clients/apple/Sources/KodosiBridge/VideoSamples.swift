import CoreMedia
import Foundation

/// The codec byte in a frame's header.
enum VideoCodec: UInt8, Sendable {
    case h264 = 0
    case hevc = 1

    /// NAL unit types of the parameter sets, in the order a format description takes them.
    fileprivate var parameterSets: [UInt8] {
        switch self {
        case .h264: [7, 8]
        case .hevc: [32, 33, 34]
        }
    }

    fileprivate func type(of header: UInt8) -> UInt8 {
        switch self {
        case .h264: header & 0x1F
        case .hevc: (header >> 1) & 0x3F
        }
    }

    fileprivate func isPicture(_ type: UInt8) -> Bool {
        switch self {
        case .h264: type == 1 || type == 5
        case .hevc: type < 32
        }
    }
}

/// Turns Annex B frames into samples for VideoToolbox, keeping the format of the last parameter
/// sets. A stream changes codec only at a keyframe, which carries the new codec's sets.
struct VideoSamples {
    private var codec: VideoCodec?
    private var format: CMVideoFormatDescription?

    /// Copies the picture once, into the sample's block buffer, so `bytes` may go after the call.
    mutating func sample(of bytes: UnsafeRawBufferPointer, codec: VideoCodec) -> CMSampleBuffer? {
        if codec != self.codec {
            self.codec = codec
            format = nil
        }
        var sets: [UInt8: Data] = [:]
        var slices: [UnsafeRawBufferPointer] = []
        for unit in Self.units(of: bytes) where !unit.isEmpty {
            let type = codec.type(of: bytes[unit.lowerBound])
            if codec.parameterSets.contains(type) {
                sets[type] = Data(bytes[unit])
            } else if codec.isPicture(type) {
                slices.append(UnsafeRawBufferPointer(rebasing: bytes[unit]))
            }
        }
        let units = codec.parameterSets.compactMap { sets[$0] }
        if units.count == codec.parameterSets.count {
            format = Self.format(codec, units) ?? format
        }
        guard let format, !slices.isEmpty else { return nil }
        return Self.sample(slices, format)
    }

    static func units(of bytes: UnsafeRawBufferPointer) -> [Range<Int>] {
        var starts: [Int] = []
        var at = 0
        while at + 2 < bytes.count {
            let third = bytes[at + 2]
            if third > 1 {
                at += 3
            } else if third == 1 {
                if bytes[at] == 0, bytes[at + 1] == 0 {
                    starts.append(at + 3)
                }
                at += 3
            } else {
                at += 1
            }
        }
        return starts.enumerated().map { index, start in
            var end = index + 1 < starts.count ? starts[index + 1] - 3 : bytes.count
            while end > start, index + 1 < starts.count, bytes[end - 1] == 0 {
                end -= 1
            }
            return start ..< end
        }
    }

    private static func format(_ codec: VideoCodec, _ units: [Data]) -> CMVideoFormatDescription? {
        let bytes = [UInt8](units.joined())
        let sizes = units.map(\.count)
        let starts = sizes.indices.map { sizes[..<$0].reduce(0, +) }
        return bytes.withUnsafeBufferPointer { joined in
            let sets = starts.map { joined.baseAddress! + $0 }
            var format: CMVideoFormatDescription?
            let status = switch codec {
            case .h264:
                CMVideoFormatDescriptionCreateFromH264ParameterSets(
                    allocator: kCFAllocatorDefault, parameterSetCount: sets.count, parameterSetPointers: sets,
                    parameterSetSizes: sizes, nalUnitHeaderLength: 4, formatDescriptionOut: &format,
                )
            case .hevc:
                CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                    allocator: kCFAllocatorDefault, parameterSetCount: sets.count, parameterSetPointers: sets,
                    parameterSetSizes: sizes, nalUnitHeaderLength: 4, extensions: nil, formatDescriptionOut: &format,
                )
            }
            return status == noErr ? format : nil
        }
    }

    private static func sample(_ slices: [UnsafeRawBufferPointer], _ format: CMVideoFormatDescription) -> CMSampleBuffer? {
        var size = slices.reduce(0) { $0 + 4 + $1.count }
        var block: CMBlockBuffer?
        var status = CMBlockBufferCreateWithMemoryBlock(
            allocator: kCFAllocatorDefault, memoryBlock: nil, blockLength: size, blockAllocator: kCFAllocatorDefault,
            customBlockSource: nil, offsetToData: 0, dataLength: size, flags: kCMBlockBufferAssureMemoryNowFlag,
            blockBufferOut: &block,
        )
        guard status == noErr, let block else { return nil }
        var offset = 0
        for slice in slices {
            status = withUnsafeBytes(of: UInt32(slice.count).bigEndian) { length in
                CMBlockBufferReplaceDataBytes(with: length.baseAddress!, blockBuffer: block, offsetIntoDestination: offset, dataLength: 4)
            }
            guard status == noErr, let start = slice.baseAddress else { return nil }
            status = CMBlockBufferReplaceDataBytes(
                with: start, blockBuffer: block, offsetIntoDestination: offset + 4, dataLength: slice.count,
            )
            guard status == noErr else { return nil }
            offset += 4 + slice.count
        }
        var sample: CMSampleBuffer?
        status = CMSampleBufferCreateReady(
            allocator: kCFAllocatorDefault, dataBuffer: block, formatDescription: format, sampleCount: 1, sampleTimingEntryCount: 0,
            sampleTimingArray: nil, sampleSizeEntryCount: 1, sampleSizeArray: &size, sampleBufferOut: &sample,
        )
        guard status == noErr, let sample else { return nil }
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: true), CFArrayGetCount(attachments) > 0 {
            let values = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFMutableDictionary.self)
            CFDictionarySetValue(
                values, Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
                Unmanaged.passUnretained(kCFBooleanTrue).toOpaque(),
            )
        }
        return sample
    }
}
