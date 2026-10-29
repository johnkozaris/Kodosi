import CoreGraphics
import Foundation
import ImageIO
import Observation

/// The pictures of a terminal's shared browser and a person's input to it. Input goes in order,
/// a batch at a time, with the size of the view where the person uses the browser.
@MainActor
@Observable
public final class BrowserFeed {
    public private(set) var image: CGImage?
    /// The size of the browser that the picture shows, in CSS pixels.
    public private(set) var pictureSize = CGSize.zero
    public private(set) var failure: String?
    @ObservationIgnored public var viewSize = CGSize.zero
    @ObservationIgnored private let store: CanvasStore
    @ObservationIgnored private let session: String
    @ObservationIgnored private var pending: [BrowserInput] = []
    @ObservationIgnored private var sending = false
    @ObservationIgnored private var watching = false

    init(store: CanvasStore, session: String) {
        self.store = store
        self.session = session
    }

    /// Shows each new picture until the task ends. Only one view at a time asks for pictures.
    public func watch() async {
        guard !watching else { return }
        watching = true
        defer { watching = false }
        var sequence: UInt64 = 0
        while !Task.isCancelled {
            let asked = ContinuousClock.now
            do {
                if let frame = try await store.picture(in: session, after: sequence) {
                    sequence = frame.sequence
                    show(frame)
                    continue
                }
            } catch is CancellationError {
                return
            } catch {
                failure = error.localizedDescription
            }
            if ContinuousClock.now - asked < .seconds(1) {
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }

    public func wake() async {
        await run(.wake(width: Int(viewSize.width), height: Int(viewSize.height)))
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
        if case .mouse(.mouseMoved, _, _, _, _, _, _, _) = event, case .mouse(.mouseMoved, _, _, _, _, _, _, _)? = pending.last {
            pending[pending.count - 1] = event
        } else {
            pending.append(event)
        }
        guard !sending else { return }
        sending = true
        Task { await flush() }
    }

    /// The point in the browser's CSS pixels for a point in a view of `size` that shows the
    /// picture scaled to fit; none outside the picture.
    public func browserPoint(_ point: CGPoint, in size: CGSize) -> BrowserInput.Point? {
        image.flatMap { Self.point(point, in: size, picture: CGSize(width: $0.width, height: $0.height), browser: pictureSize) }
    }

    /// The picture can have more pixels than the browser has CSS pixels, so its height gives the
    /// pixels of one CSS pixel.
    public nonisolated static func point(_ point: CGPoint, in size: CGSize, picture: CGSize, browser: CGSize) -> BrowserInput.Point? {
        guard picture.width > 0, picture.height > 0, browser.height > 0, size.width > 0, size.height > 0 else { return nil }
        let fit = min(size.width / picture.width, size.height / picture.height)
        let origin = CGPoint(x: (size.width - picture.width * fit) / 2, y: (size.height - picture.height * fit) / 2)
        let perPixel = picture.height / browser.height
        let x = (point.x - origin.x) / fit / perPixel
        let y = (point.y - origin.y) / fit / perPixel
        guard x >= 0, y >= 0, x < browser.width, y < browser.height else { return nil }
        return BrowserInput.Point(x: Int(x), y: Int(y))
    }

    private func flush() async {
        while !pending.isEmpty {
            let batch = Array(pending.prefix(64))
            pending.removeFirst(batch.count)
            do {
                try await store.send(batch, width: Int(viewSize.width), height: Int(viewSize.height), in: session)
                failure = nil
            } catch {
                failure = error.localizedDescription
            }
        }
        sending = false
    }

    private func show(_ frame: BrowserFrame) {
        guard let source = CGImageSourceCreateWithData(frame.jpeg as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else { return }
        self.image = image
        pictureSize = CGSize(width: frame.width, height: frame.height)
        failure = nil
    }
}
