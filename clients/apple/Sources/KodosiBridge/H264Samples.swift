import CoreMedia
import Foundation

struct H264Samples {
    private static let sequenceParameters: UInt8 = 7
    private static let pictureParameters: UInt8 = 8
    private static let pictures: Set<UInt8> = [1, 5]

    private var format: CMVideoFormatDescription?

    mutating func sample(of data: Data) -> CMSampleBuffer? {
        var sequence: Data?
        var picture: Data?
        var slices: [Data] = []
        for unit in Self.units(of: data) {
            guard let type = unit.first.map({ $0 & 0x1F }) else { continue }
            if type == Self.sequenceParameters {
                sequence = unit
            } else if type == Self.pictureParameters {
                picture = unit
            } else if Self.pictures.contains(type) {
                slices.append(unit)
            }
        }
        if let sequence, let picture {
            format = Self.format(sequence, picture) ?? format
        }
        guard let format, !slices.isEmpty else { return nil }
        return Self.sample(slices, format)
    }

    static func units(of data: Data) -> [Data] {
        let ranges = data.withUnsafeBytes { bytes in
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
        return ranges.map { data[data.startIndex + $0.lowerBound ..< data.startIndex + $0.upperBound] }
    }

    private static func format(_ sequence: Data, _ picture: Data) -> CMVideoFormatDescription? {
        let first = [UInt8](sequence)
        let second = [UInt8](picture)
        return first.withUnsafeBufferPointer { firstBytes in
            second.withUnsafeBufferPointer { secondBytes in
                let sets = [firstBytes.baseAddress!, secondBytes.baseAddress!]
                let sizes = [first.count, second.count]
                var format: CMVideoFormatDescription?
                let status = CMVideoFormatDescriptionCreateFromH264ParameterSets(
                    allocator: kCFAllocatorDefault, parameterSetCount: 2, parameterSetPointers: sets,
                    parameterSetSizes: sizes, nalUnitHeaderLength: 4, formatDescriptionOut: &format,
                )
                return status == noErr ? format : nil
            }
        }
    }

    private static func sample(_ slices: [Data], _ format: CMVideoFormatDescription) -> CMSampleBuffer? {
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
            guard status == noErr else { return nil }
            status = slice.withUnsafeBytes { bytes in
                CMBlockBufferReplaceDataBytes(with: bytes.baseAddress!, blockBuffer: block, offsetIntoDestination: offset + 4, dataLength: slice.count)
            }
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
