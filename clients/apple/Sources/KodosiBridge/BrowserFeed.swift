import CoreGraphics
import CoreMedia
import Foundation
import Observation

/// The live picture of a terminal's shared browser and a person's input to it. A frame shows when
/// it arrives, and each frame is acknowledged, so the host keeps the picture as sharp as the
/// link allows and no queue forms.
@MainActor
@Observable
public final class BrowserFeed {
    private static let frameHeader = 14
    private static let messageTag: UInt8 = 0
    private static let frameTag: UInt8 = 1
    private static let longestWait = Duration.seconds(30)

    /// What the window says around the picture.
    public private(set) var window = BrowserWindow()
    public private(set) var hasPicture = false
    public private(set) var failure: String?
    /// Counts each press, key and scroll of a person on this device.
    public private(set) var touches = 0
    /// Takes each frame to show.
    @ObservationIgnored public var show: (CMSampleBuffer) -> Void = { _ in }
    /// The view of this device, in points, and the pixels of a point.
    @ObservationIgnored public var viewSize = CGSize.zero
    @ObservationIgnored public var viewScale: CGFloat = 1
    @ObservationIgnored private let store: CanvasStore
    @ObservationIgnored private let session: String
    @ObservationIgnored private var link: DocumentLink?
    @ObservationIgnored private var samples = H264Samples()
    @ObservationIgnored private var lastFrame: UInt32 = 0
    @ObservationIgnored private var behind = true
    @ObservationIgnored private var sentSize: BrowserSize?

    init(store: CanvasStore, session: String) {
        self.store = store
        self.session = session
    }

    /// Shows the live picture until the task ends. Only one view at a time watches.
    public func watch() async {
        guard link == nil else { return }
        let link = store.browser(of: session)
        self.link = link
        let (losses, loser) = AsyncStream.makeStream(of: Void.self)
        link.onFrame = { [weak self] in self?.receive($0) }
        link.onLost = { loser.yield() }
        defer {
            link.close()
            self.link = nil
        }
        await join(link)
        for await _ in losses {
            hasPicture = false
            await join(link)
        }
    }

    public func wake() async {
        await run(.wake)
    }

    public func run(_ act: BrowserAct) async {
        do {
            try await store.act(act, in: session)
            failure = nil
        } catch {
            failure = error.localizedDescription
        }
    }

    public func send(_ event: BrowserInput) {
        if !event.isPlainMove {
            touches += 1
            sendSize()
        }
        post(.input(event))
    }

    /// Asks for a new key frame, after the display lost the stream.
    public func restart() {
        behind = true
        post(.keyframe)
    }

    /// The point in the browser's CSS pixels for a point in a view of `size` that shows the
    /// picture scaled to fit; none outside the picture.
    public func browserPoint(_ point: CGPoint, in size: CGSize) -> BrowserInput.Point? {
        Self.point(point, in: size, browser: window.size)
    }

    /// Where a point of the browser is in a view of `size` that shows the picture scaled to fit.
    public func viewPoint(x: Double, y: Double, in size: CGSize) -> CGPoint? {
        Self.viewPoint(CGPoint(x: x, y: y), in: size, browser: window.size)
    }

    public nonisolated static func viewPoint(_ point: CGPoint, in size: CGSize, browser: CGSize) -> CGPoint? {
        guard browser.width > 0, browser.height > 0, size.width > 0, size.height > 0 else { return nil }
        let fit = min(size.width / browser.width, size.height / browser.height)
        let origin = CGPoint(x: (size.width - browser.width * fit) / 2, y: (size.height - browser.height * fit) / 2)
        return CGPoint(x: origin.x + point.x * fit, y: origin.y + point.y * fit)
    }

    public nonisolated static func point(_ point: CGPoint, in size: CGSize, browser: CGSize) -> BrowserInput.Point? {
        guard browser.width > 0, browser.height > 0, size.width > 0, size.height > 0 else { return nil }
        let fit = min(size.width / browser.width, size.height / browser.height)
        let origin = CGPoint(x: (size.width - browser.width * fit) / 2, y: (size.height - browser.height * fit) / 2)
        let x = (point.x - origin.x) / fit
        let y = (point.y - origin.y) / fit
        guard x >= 0, y >= 0, x < browser.width, y < browser.height else { return nil }
        return BrowserInput.Point(x: Double(x), y: Double(y))
    }

    /// The computer of the terminal can be away for a time, so this goes on until the view ends.
    private func join(_ link: DocumentLink) async {
        var wait = Duration.seconds(1)
        while !Task.isCancelled {
            do {
                try await link.open()
                failure = nil
                behind = true
                let size = currentSize
                sentSize = size
                post(.watch(size))
                return
            } catch {
                failure = error.localizedDescription
                try? await Task.sleep(for: wait)
                wait = min(wait * 2, Self.longestWait)
            }
        }
    }

    private var currentSize: BrowserSize {
        BrowserSize(width: Int(viewSize.width), height: Int(viewSize.height), scale: Double(viewScale))
    }

    /// The browser takes the size of the view where a person used it last.
    private func sendSize() {
        let size = currentSize
        if size != sentSize {
            sentSize = size
            post(.size(size))
        }
    }

    private func post(_ message: ToBrowserWindow) {
        guard let body = try? JSONEncoder().encode(message) else { return }
        link?.send(Data([Self.messageTag]) + body)
    }

    private func receive(_ message: Data) {
        switch message.first {
        case Self.frameTag:
            frame(message)
        case Self.messageTag:
            if let state = try? JSONDecoder().decode(BrowserWindow.self, from: message.dropFirst()) {
                window = state
            }
        default:
            break
        }
    }

    private func frame(_ message: Data) {
        guard message.count > Self.frameHeader else { return }
        let header = [UInt8](message.prefix(Self.frameHeader))
        let id = header[1 ..< 5].reduce(UInt32(0)) { $0 << 8 | UInt32($1) }
        let key = header[5] != 0
        post(.ack(id))
        if !key, !behind, id != lastFrame &+ 1 {
            restart()
        }
        lastFrame = id
        guard key || !behind, let sample = samples.sample(of: message.dropFirst(Self.frameHeader)) else { return }
        behind = false
        hasPicture = true
        show(sample)
    }
}

/// What a viewer sends to its window, in the browser's own names.
private enum ToBrowserWindow: Encodable {
    case watch(BrowserSize)
    case size(BrowserSize)
    case ack(UInt32)
    case keyframe
    case input(BrowserInput)

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case let .watch(size):
            try container.encode("watch", forKey: .type)
            try container.encode(size, forKey: .size)
        case let .size(size):
            try container.encode("size", forKey: .type)
            try container.encode(size, forKey: .size)
        case let .ack(frame):
            try container.encode("ack", forKey: .type)
            try container.encode(frame, forKey: .frame)
        case .keyframe:
            try container.encode("keyframe", forKey: .type)
        case let .input(input):
            try container.encode("input", forKey: .type)
            try container.encode(input, forKey: .input)
        }
    }

    private enum Field: String, CodingKey { case type, size, frame, input }
}

private struct BrowserSize: Encodable, Equatable {
    let width: Int
    let height: Int
    let scale: Double
}

/// Makes sample buffers from H.264 in Annex B form.
struct H264Samples {
    private static let startCode: [UInt8] = [0, 0, 1]
    private static let sequenceParameters: UInt8 = 7
    private static let pictureParameters: UInt8 = 8
    private static let pictures: Set<UInt8> = [1, 5]

    private var format: CMVideoFormatDescription?

    mutating func sample(of data: Data) -> CMSampleBuffer? {
        var sequence: Data?
        var picture: Data?
        var body = Data()
        for unit in Self.units(of: data) {
            guard let type = unit.first.map({ $0 & 0x1F }) else { continue }
            if type == Self.sequenceParameters {
                sequence = unit
            } else if type == Self.pictureParameters {
                picture = unit
            } else if Self.pictures.contains(type) {
                withUnsafeBytes(of: UInt32(unit.count).bigEndian) { body.append(contentsOf: $0) }
                body.append(unit)
            }
        }
        if let sequence, let picture {
            format = Self.format(sequence, picture) ?? format
        }
        guard let format, !body.isEmpty else { return nil }
        return Self.sample(body, format)
    }

    /// The units between the start codes.
    static func units(of data: Data) -> [Data] {
        let bytes = [UInt8](data)
        var starts: [Int] = []
        var at = 0
        while at + 2 < bytes.count {
            if bytes[at] == 0, bytes[at + 1] == 0, bytes[at + 2] == 1 {
                starts.append(at + 3)
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
            return Data(bytes[start ..< end])
        }
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

    private static func sample(_ body: Data, _ format: CMVideoFormatDescription) -> CMSampleBuffer? {
        var block: CMBlockBuffer?
        var status = CMBlockBufferCreateWithMemoryBlock(
            allocator: kCFAllocatorDefault, memoryBlock: nil, blockLength: body.count, blockAllocator: kCFAllocatorDefault,
            customBlockSource: nil, offsetToData: 0, dataLength: body.count, flags: kCMBlockBufferAssureMemoryNowFlag,
            blockBufferOut: &block,
        )
        guard status == noErr, let block else { return nil }
        status = body.withUnsafeBytes { bytes in
            CMBlockBufferReplaceDataBytes(with: bytes.baseAddress!, blockBuffer: block, offsetIntoDestination: 0, dataLength: body.count)
        }
        guard status == noErr else { return nil }
        var sample: CMSampleBuffer?
        var size = body.count
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
