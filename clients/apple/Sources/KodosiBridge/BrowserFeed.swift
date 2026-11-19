import CoreGraphics
import CoreMedia
import Foundation
import ImageIO
import Observation

/// Each frame is acknowledged, so the host keeps the picture as sharp as the link allows and no
/// queue forms.
@MainActor
@Observable
public final class BrowserFeed {
    public enum Picture: Sendable {
        case waiting
        case live
        /// The surface keeps the last picture while the feed joins again, until a new frame
        /// replaces it.
        case kept
    }

    private static let frameHeader = 6
    private static let messageTag: UInt8 = 0
    private static let frameTag: UInt8 = 1
    private static let keyFlag: UInt8 = 1
    private static let anchorFlag: UInt8 = 2
    private static let settle = Duration.milliseconds(250)

    public private(set) var window = BrowserWindow()
    public private(set) var icons: [String: CGImage] = [:]
    public private(set) var picture = Picture.waiting
    public private(set) var failure: String?
    /// Counts each press, key and scroll of a person on this device.
    public private(set) var touches = 0
    @ObservationIgnored public var copied: (String) -> Void = { _ in }
    /// The page takes the shape of the view of the person who acted last, so a new size goes to
    /// the host once the view settles, never while a button is held, and one that waits goes
    /// right after the person's act: the act then reaches the page that the person saw.
    @ObservationIgnored public var viewSize = CGSize.zero {
        didSet {
            if viewSize != oldValue {
                settleSize()
            }
        }
    }

    @ObservationIgnored public var viewScale: CGFloat = 1 {
        didSet {
            if viewScale != oldValue {
                settleSize()
            }
        }
    }

    @ObservationIgnored private let store: CanvasStore
    @ObservationIgnored private let session: String
    @ObservationIgnored private var link: DocumentLink?
    @ObservationIgnored private var display: (CMSampleBuffer) -> Void = { _ in }
    @ObservationIgnored private weak var surface: AnyObject?
    @ObservationIgnored private var samples = H264Samples()
    @ObservationIgnored private var lastFrame: UInt32 = 0
    @ObservationIgnored private var holds = Holds.nothing
    @ObservationIgnored private var sentSize: BrowserSize?
    @ObservationIgnored private var settling: Task<Void, Never>?
    @ObservationIgnored private var holding = false

    init(store: CanvasStore, session: String) {
        self.store = store
        self.session = session
    }

    /// Only one view at a time watches, until it ends.
    public func watch() async {
        guard link == nil else { return }
        let link = store.browser(of: session)
        self.link = link
        let (news, teller) = AsyncStream.makeStream(of: DocumentLink.News.self)
        link.onFrame = { [weak self] in self?.receive($0) }
        link.onNews = { teller.yield($0) }
        defer {
            link.close()
            self.link = nil
        }
        do {
            try await link.open()
        } catch {
            // The view ends on each trip off the screen, which is no problem of the browser.
            if !Task.isCancelled {
                failure = error.localizedDescription
            }
            return
        }
        var opened = true
        for await item in news {
            switch item {
            case .open:
                join(opened: opened)
                opened = false
            case let .lost(reason):
                if picture == .live {
                    picture = .kept
                }
                failure = reason
            case .closed:
                return
            }
        }
    }

    /// A surface that is new to the feed has no picture, so the host sends a key frame at once.
    public func show(on surface: AnyObject, _ display: @escaping (CMSampleBuffer) -> Void) {
        self.display = display
        guard surface !== self.surface else { return }
        self.surface = surface
        picture = .waiting
        restart()
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
        post(.input(event))
        guard !event.isPlainMove else { return }
        touches += 1
        if case let .mouse(action, _, _, _, _, _, _, _) = event {
            if action == .mousePressed {
                holding = true
            } else if action == .mouseReleased {
                holding = false
            }
        }
        sendSize()
    }

    /// The files go into the folder of the terminal first, because the browser runs on the
    /// computer of the terminal.
    public func give(_ files: [URL]) async {
        var paths: [String] = []
        do {
            for file in files {
                if let path = try await store.upload(file, to: session, page: true) {
                    paths.append(path)
                }
            }
        } catch {
            failure = error.localizedDescription
        }
        send(.files(paths))
    }

    public func restart() {
        holds = .nothing
        post(.keyframe)
    }

    /// While a button is held, a point beyond the page goes to its nearest edge, so that the page
    /// also gets the rest of the drag and its release.
    public func browserPoint(_ point: CGPoint, in size: CGSize) -> BrowserInput.Point? {
        Self.point(point, in: size, browser: window.size, clamped: holding)
    }

    public func viewPoint(x: Double, y: Double, in size: CGSize) -> CGPoint? {
        Self.viewPoint(CGPoint(x: x, y: y), in: size, browser: window.size)
    }

    public nonisolated static func viewPoint(_ point: CGPoint, in size: CGSize, browser: CGSize) -> CGPoint? {
        guard browser.width > 0, browser.height > 0, size.width > 0, size.height > 0 else { return nil }
        let fit = min(size.width / browser.width, size.height / browser.height)
        let origin = CGPoint(x: (size.width - browser.width * fit) / 2, y: (size.height - browser.height * fit) / 2)
        return CGPoint(x: origin.x + point.x * fit, y: origin.y + point.y * fit)
    }

    public nonisolated static func point(_ point: CGPoint, in size: CGSize, browser: CGSize,
                                         clamped: Bool = false) -> BrowserInput.Point?
    {
        guard browser.width > 0, browser.height > 0, size.width > 0, size.height > 0 else { return nil }
        let fit = min(size.width / browser.width, size.height / browser.height)
        let origin = CGPoint(x: (size.width - browser.width * fit) / 2, y: (size.height - browser.height * fit) / 2)
        var x = (point.x - origin.x) / fit
        var y = (point.y - origin.y) / fit
        if clamped {
            x = min(max(x, 0), browser.width.nextDown)
            y = min(max(y, 0), browser.height.nextDown)
        }
        guard x >= 0, y >= 0, x < browser.width, y < browser.height else { return nil }
        return BrowserInput.Point(x: Double(x), y: Double(y))
    }

    private func join(opened: Bool) {
        failure = nil
        holds = .nothing
        let size = currentSize
        sentSize = size
        post(.watch(size, opened: opened))
    }

    private var currentSize: BrowserSize {
        BrowserSize(width: Int(viewSize.width), height: Int(viewSize.height), scale: Double(viewScale))
    }

    private func settleSize() {
        settling?.cancel()
        settling = Task { [weak self] in
            try? await Task.sleep(for: Self.settle)
            guard !Task.isCancelled else { return }
            self?.sendSize()
        }
    }

    private func sendSize() {
        let size = currentSize
        guard !holding, size != sentSize else { return }
        sentSize = size
        post(.size(size))
    }

    private func post(_ message: ToBrowserWindow) {
        guard let body = try? JSONEncoder().encode(message) else { return }
        link?.send(Data([Self.messageTag]) + body)
    }

    func receive(_ message: Data) {
        switch message.first {
        case Self.frameTag:
            frame(message)
        case Self.messageTag:
            if let state = try? JSONDecoder().decode(BrowserWindow.self, from: message.dropFirst()) {
                window = state
                keep(state.icons, of: state.tabs)
                if let words = state.copied {
                    copied(words)
                }
            }
        default:
            break
        }
    }

    private func keep(_ changed: [String: String], of tabs: [BrowserWindow.Tab]) {
        var kept = icons
        for (tab, png) in changed {
            kept[tab] = Data(base64Encoded: png).flatMap { data in
                CGImageSourceCreateWithData(data as CFData, nil).flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) }
            }
        }
        let open = Set(tabs.map(\.id))
        kept = kept.filter { open.contains($0.key) }
        if kept.keys != icons.keys || !changed.isEmpty {
            icons = kept
        }
    }

    /// The host skips frames for a view that falls behind and goes on at an anchor, so only a
    /// delta frame after a gap shows a frame lost on the way, and the decoder needs a key frame.
    private func frame(_ message: Data) {
        guard message.count > Self.frameHeader else { return }
        let header = [UInt8](message.prefix(Self.frameHeader))
        let id = header[1 ..< 5].reduce(UInt32(0)) { $0 << 8 | UInt32($1) }
        let key = header[5] & Self.keyFlag != 0
        let anchor = header[5] & Self.anchorFlag != 0
        post(.ack(id))
        if holds == .stream, id != lastFrame &+ 1 {
            holds = .gap
        }
        lastFrame = id
        switch holds {
        case .nothing where !key:
            return
        case .gap where !key && !anchor:
            restart()
            return
        default:
            break
        }
        guard let sample = samples.sample(of: message.dropFirst(Self.frameHeader)) else { return }
        holds = .stream
        picture = .live
        display(sample)
    }
}

/// What the decoder of a feed holds.
private enum Holds {
    /// Nothing that a frame after a key frame needs, as a new decoder.
    case nothing
    /// Each frame since its key frame.
    case stream
    /// Every frame up to one that the host skipped.
    case gap
}

private enum ToBrowserWindow: Encodable {
    case watch(BrowserSize, opened: Bool)
    case size(BrowserSize)
    case ack(UInt32)
    case keyframe
    case input(BrowserInput)

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case let .watch(size, opened):
            try container.encode("watch", forKey: .type)
            try container.encode(size, forKey: .size)
            try container.encode(true, forKey: .anchors)
            try container.encode(opened, forKey: .opened)
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

    private enum Field: String, CodingKey { case type, size, frame, input, anchors, opened }
}

private struct BrowserSize: Encodable, Equatable {
    let width: Int
    let height: Int
    let scale: Double
}

struct H264Samples {
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
