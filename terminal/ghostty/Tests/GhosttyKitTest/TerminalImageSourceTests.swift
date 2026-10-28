import AppKit
import Foundation
import GhosttyKit
@testable import GhosttyTerminal
import IOSurface
import Testing

@MainActor
@Suite(.serialized)
struct TerminalImageSourceTests {
    @Test
    func `a view draws Kitty images from its output and never from a file of this computer`() async throws {
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 320, height: 160))
        view.wantsLayer = true
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController()
        let bridge = TerminalCallbackBridge()
        let surface = try #require(controller.createSurface(
            bridge: bridge,
            configuration: options(background: 0x00, session: session),
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

        let pixels = Data((0 ..< 64).flatMap { _ in [UInt8(0xFF), 0x00, 0x00] })
        let file = FileManager.default.temporaryDirectory
            .appending(path: "kodosi-image-source-\(UUID().uuidString).rgb")
        try pixels.write(to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        let path = Data(file.path(percentEncoded: false).utf8)

        #expect(session.receive(kittyImage(id: 1, medium: "f", payload: path)))
        #expect(try await !draws(surface, session: session, in: view, frames: 25) { $0.showsRed })

        #expect(controller.updateSurface(surface, bridge: bridge, configuration: options(background: 0x30, session: session)))
        #expect(try await draws(surface, session: session, in: view, frames: 150) { $0.background == 0x30 })
        #expect(session.receive(kittyImage(id: 2, medium: "f", payload: path)))
        #expect(try await !draws(surface, session: session, in: view, frames: 25) { $0.showsRed })

        #expect(session.receive(kittyImage(id: 3, medium: "d", payload: pixels)))
        #expect(try await draws(surface, session: session, in: view, frames: 150) { $0.showsRed })
    }

    private func options(background: UInt8, session: InMemoryTerminalSession) -> TerminalSurfaceOptions {
        TerminalSurfaceOptions(
            session: session,
            terminalConfiguration: .init {
                $0.withBackground(String(format: "%02x%02x%02x", background, background, background))
                $0.withWindowPaddingX(0)
                $0.withWindowPaddingY(0)
            }
        )
    }

    private func kittyImage(id: Int, medium: String, payload: Data) -> Data {
        Data("\u{1B}_Ga=T,f=24,s=8,v=8,i=\(id),t=\(medium);\(payload.base64EncodedString())\u{1B}\\".utf8)
    }

    private func draws(
        _ surface: ghostty_surface_t,
        session: InMemoryTerminalSession,
        in view: NSView,
        frames: Int,
        until condition: (Frame) -> Bool
    ) async throws -> Bool {
        session.waitForPendingOutput()
        for _ in 0 ..< frames {
            ghostty_surface_refresh(surface)
            ghostty_surface_draw(surface)
            try await Task.sleep(for: .milliseconds(20))
            if let frame = Frame(view), condition(frame) {
                return true
            }
        }
        return false
    }
}

private struct Frame {
    let showsRed: Bool
    let background: UInt8

    @MainActor
    init?(_ view: NSView) {
        guard let contents = view.layer?.contents,
              CFGetTypeID(contents as CFTypeRef) == IOSurfaceGetTypeID()
        else { return nil }
        let surface = unsafeDowncast(contents as AnyObject, to: IOSurface.self)
        guard surface.pixelFormat == kCVPixelFormatType_32BGRA else { return nil }
        surface.lock(options: .readOnly, seed: nil)
        defer { surface.unlock(options: .readOnly, seed: nil) }
        func channel(_ row: Int, _ column: Int, _ index: Int) -> UInt8 {
            surface.baseAddress.load(fromByteOffset: row * surface.bytesPerRow + column * 4 + index, as: UInt8.self)
        }
        showsRed = (0 ..< surface.height).contains { row in
            (0 ..< surface.width).contains { column in
                channel(row, column, 2) > 200 && channel(row, column, 1) < 60 && channel(row, column, 0) < 60
            }
        }
        background = channel(surface.height - 1, surface.width - 1, 1)
    }
}
