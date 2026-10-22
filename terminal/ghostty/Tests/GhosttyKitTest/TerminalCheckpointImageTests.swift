import AppKit
import Foundation
import GhosttyKit
@testable import GhosttyTerminal
import IOSurface
import Testing

@MainActor
@Suite(.serialized)
struct TerminalCheckpointImageTests {
    @Test
    func `a visible checkpoint shows the image pixels that follow its screens`() async throws {
        let fixtureURL = try #require(
            Bundle.module.url(forResource: "visible-image-checkpoint", withExtension: "snapshot")
        )
        let checkpoint = try Data(contentsOf: fixtureURL)
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 320, height: 160))
        view.wantsLayer = true
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController()
        let bridge = TerminalCallbackBridge()
        let surface = try #require(controller.createSurface(
            bridge: bridge,
            configuration: TerminalSurfaceOptions(
                session: session,
                terminalConfiguration: .init {
                    $0.withBackground("000000")
                    $0.withWindowPaddingX(0)
                    $0.withWindowPaddingY(0)
                }
            ),
            platformSetup: { config in
                config.platform_tag = GHOSTTY_PLATFORM_MACOS
                config.platform = ghostty_platform_u(macos: ghostty_platform_macos_s(
                    nsview: Unmanaged.passUnretained(view).toOpaque()
                ))
                config.scale_factor = 1
            }
        ))
        session.setSurface(surface)
        defer {
            session.clearSurface(ifMatches: surface)
            ghostty_surface_free(surface)
            controller.remove(bridge)
        }
        ghostty_surface_set_size(surface, 320, 160)

        #expect(session.restoreCheckpointSynchronously(checkpoint, console: false))
        session.waitForPendingOutput()
        var red = false
        for _ in 0 ..< 150 where !red {
            ghostty_surface_refresh(surface)
            ghostty_surface_draw(surface)
            try await Task.sleep(for: .milliseconds(20))
            red = showsRed(view)
        }
        #expect(red, "the restored image was not drawn")
    }

    private func showsRed(_ view: NSView) -> Bool {
        guard let contents = view.layer?.contents,
              CFGetTypeID(contents as CFTypeRef) == IOSurfaceGetTypeID()
        else { return false }
        let frame = unsafeDowncast(contents as AnyObject, to: IOSurface.self)
        guard frame.pixelFormat == kCVPixelFormatType_32BGRA else { return false }
        frame.lock(options: .readOnly, seed: nil)
        defer { frame.unlock(options: .readOnly, seed: nil) }
        for row in 0 ..< frame.height {
            for column in 0 ..< frame.width {
                let offset = row * frame.bytesPerRow + column * 4
                let blue = frame.baseAddress.load(fromByteOffset: offset, as: UInt8.self)
                let green = frame.baseAddress.load(fromByteOffset: offset + 1, as: UInt8.self)
                let redValue = frame.baseAddress.load(fromByteOffset: offset + 2, as: UInt8.self)
                if redValue > 200, green < 60, blue < 60 {
                    return true
                }
            }
        }
        return false
    }
}
