import AppKit
import Foundation
import GhosttyKit
@testable import GhosttyTerminal
import Testing

@MainActor
@Suite(.serialized)
struct TerminalSurfaceCoordinatorTests {
    private static let padding = TerminalConfiguration {
        $0.withWindowPaddingX(4)
        $0.withWindowPaddingY(4)
    }

    @MainActor
    private final class Rig {
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 480))
        let controller = TerminalController()
        let coordinator = TerminalSurfaceCoordinator()
        let collector = ViewportCollector()
        let session: InMemoryTerminalSession
        var size = (width: 640.0, height: 480.0)

        init(configuration: TerminalConfiguration?) {
            view.wantsLayer = true
            let collector = collector
            session = InMemoryTerminalSession(write: { _ in }, resize: { collector.append($0) })
            coordinator.isAttached = { true }
            coordinator.scaleFactor = { 2 }
            coordinator.viewSize = { [unowned self] in size }
            coordinator.platformSetup = { [view] config in
                config.platform_tag = GHOSTTY_PLATFORM_MACOS
                config.platform = ghostty_platform_u(
                    macos: ghostty_platform_macos_s(
                        nsview: Unmanaged.passUnretained(view).toOpaque()
                    )
                )
            }
            coordinator.configuration = TerminalSurfaceOptions(
                session: session,
                terminalConfiguration: configuration
            )
            coordinator.controller = controller
        }

        func hostResize(columns: UInt16, rows: UInt16, cell: InMemoryTerminalViewport) {
            #expect(session.applyHostResize(InMemoryTerminalViewport(
                columns: columns,
                rows: rows,
                cellWidthPixels: cell.cellWidthPixels,
                cellHeightPixels: cell.cellHeightPixels
            )))
            session.waitForPendingOutput()
        }

        func stop() {
            coordinator.freeSurface()
        }
    }

    @Test
    func `the grid of a view fits the view after the host resized the terminal`() throws {
        let rig = Rig(configuration: Self.padding)
        defer { rig.stop() }
        let viewport = try #require(rig.collector.values.last)
        rig.hostResize(columns: 120, rows: 32, cell: viewport)

        rig.coordinator.synchronizeMetrics()

        #expect(rig.collector.values == [viewport])
    }

    @Test
    func `a view that changes after a host resize reports the grid that fits it`() throws {
        let rig = Rig(configuration: Self.padding)
        defer { rig.stop() }
        let first = try #require(rig.collector.values.last)
        rig.hostResize(columns: 120, rows: 32, cell: first)

        rig.size = (width: 320, height: 240)
        rig.coordinator.synchronizeMetrics()
        rig.hostResize(columns: 40, rows: 10, cell: first)
        rig.size = (width: 640, height: 480)
        rig.coordinator.synchronizeMetrics()

        let last = try #require(rig.collector.values.last)
        #expect((last.columns, last.rows) == (first.columns, first.rows))
    }

    @Test(arguments: [(640.0, 480.0), (333.0, 217.0), (1229.5, 371.5), (81.0, 41.0)])
    func `the grid of a view is the grid that Ghostty gives a surface of that size`(
        width: Double,
        height: Double
    ) throws {
        let rig = Rig(configuration: Self.padding)
        defer { rig.stop() }
        rig.size = (width, height)

        rig.coordinator.synchronizeMetrics()

        let reported = try #require(rig.coordinator.surface?.size())
        let viewport = try #require(rig.collector.values.last)
        #expect((viewport.columns, viewport.rows) == (reported.columns, reported.rows))
    }

    @Test
    func `a surface without its own padding uses the padding of Ghostty`() throws {
        let rig = Rig(configuration: nil)
        defer { rig.stop() }
        rig.size = (width: 401, height: 303)

        rig.coordinator.synchronizeMetrics()

        let reported = try #require(rig.coordinator.surface?.size())
        let viewport = try #require(rig.collector.values.last)
        #expect((viewport.columns, viewport.rows) == (reported.columns, reported.rows))
    }
}
