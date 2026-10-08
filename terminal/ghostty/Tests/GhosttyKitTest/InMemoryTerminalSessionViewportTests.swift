import Foundation
@testable import GhosttyTerminal
import Testing

@MainActor
struct InMemoryTerminalSessionViewportTests {
    @Test
    func `read viewport text returns nil before surface attached`() {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        #expect(session.readViewportText() == nil)
    }

    @Test
    func `backend resize callback does not dispatch host resize`() {
        let collector = ViewportCollector()
        let session = InMemoryTerminalSession(
            write: { _ in },
            resize: { collector.append($0) }
        )

        InMemoryTerminalSession.receiveResizeCallback(
            Unmanaged.passUnretained(session).toOpaque(),
            80,
            24,
            800,
            480
        )

        #expect(collector.values.isEmpty)
    }

    @Test
    func `platform viewport dispatches authoritative geometry`() {
        let collector = ViewportCollector()
        let session = InMemoryTerminalSession(
            write: { _ in },
            resize: { collector.append($0) }
        )
        let platformViewport = TerminalGridMetrics(
            columns: 120,
            rows: 40,
            widthPixels: 1920,
            heightPixels: 1200,
            cellWidthPixels: 16,
            cellHeightPixels: 30
        )

        session.updateViewport(platformViewport)

        #expect(collector.values == [
            InMemoryTerminalViewport(
                columns: 120,
                rows: 40,
                widthPixels: 1920,
                heightPixels: 1200,
                cellWidthPixels: 16,
                cellHeightPixels: 30
            ),
        ])
    }
}

private final class ViewportCollector: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: [InMemoryTerminalViewport] = []

    var values: [InMemoryTerminalViewport] {
        lock.withLock { storage }
    }

    func append(_ viewport: InMemoryTerminalViewport) {
        lock.withLock {
            storage.append(viewport)
        }
    }
}
