import AppKit
import Foundation
import GhosttyKit
@testable import GhosttyTerminal
import IOSurface
import Testing

@MainActor
@Suite(.serialized)
struct TerminalCheckpointColorTests {
    @Test
    func `checkpoint restore keeps the default colors of the surface configuration`() async throws {
        let fixtureURL = try #require(Bundle.module.url(forResource: "visual-checkpoint", withExtension: "json"))
        let checkpoint = try Data(contentsOf: fixtureURL)
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 320, height: 160))
        view.wantsLayer = true
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let controller = TerminalController()
        let bridge = TerminalCallbackBridge()
        let light = SurfaceColors(background: 0xD0, foreground: 0x30, paletteEntry: 0x80)
        let dark = SurfaceColors(background: 0x20, foreground: 0xB0, paletteEntry: 0x60)
        let surface = try #require(controller.createSurface(
            bridge: bridge,
            configuration: light.options(for: session),
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

        for colors in [light, dark] {
            #expect(controller.updateSurface(surface, bridge: bridge, configuration: colors.options(for: session)))
            for restore in [false, true] {
                if restore {
                    #expect(session.restoreCheckpointSynchronously(checkpoint))
                }
                let rendered = try await renderedColors(of: surface, session: session, in: view, expecting: colors)
                #expect(rendered == colors, "restore=\(restore) rendered=\(rendered) configured=\(colors)")
            }
        }
    }

    private func renderedColors(
        of surface: ghostty_surface_t,
        session: InMemoryTerminalSession,
        in view: NSView,
        expecting expected: SurfaceColors
    ) async throws -> SurfaceColors {
        let swatches = "\u{1B}[?1049l\u{1B}[2J\u{1B}[H\u{1B}[41m    \u{1B}[0m\u{1B}[7m    \u{1B}[0m"
        #expect(session.receive(Data(swatches.utf8)))
        session.waitForPendingOutput()
        let size = ghostty_surface_size(surface)
        let row = Int(size.cell_height_px) / 2
        let columns = [2, 6, 12].map { $0 * Int(size.cell_width_px) }
        var latest: SurfaceColors?
        for _ in 0 ..< 150 {
            ghostty_surface_refresh(surface)
            ghostty_surface_draw(surface)
            try await Task.sleep(for: .milliseconds(20))
            latest = sample(view, row: row, columns: columns) ?? latest
            if latest == expected {
                break
            }
        }
        return try #require(latest, "the surface did not present a frame")
    }

    private func sample(_ view: NSView, row: Int, columns: [Int]) -> SurfaceColors? {
        guard let contents = view.layer?.contents,
              CFGetTypeID(contents as CFTypeRef) == IOSurfaceGetTypeID()
        else { return nil }
        let frame = unsafeDowncast(contents as AnyObject, to: IOSurface.self)
        guard frame.pixelFormat == kCVPixelFormatType_32BGRA,
              row < frame.height,
              columns.allSatisfy({ $0 < frame.width })
        else { return nil }
        frame.lock(options: .readOnly, seed: nil)
        defer { frame.unlock(options: .readOnly, seed: nil) }
        let grays = columns.map { column in
            frame.baseAddress.load(fromByteOffset: row * frame.bytesPerRow + column * 4 + 1, as: UInt8.self)
        }
        return SurfaceColors(background: grays[2], foreground: grays[1], paletteEntry: grays[0])
    }
}

private struct SurfaceColors: Equatable, CustomStringConvertible {
    let background: UInt8
    let foreground: UInt8
    let paletteEntry: UInt8

    var description: String {
        "background=\(Self.hex(background)) foreground=\(Self.hex(foreground)) palette=\(Self.hex(paletteEntry))"
    }

    func options(for session: InMemoryTerminalSession) -> TerminalSurfaceOptions {
        TerminalSurfaceOptions(
            session: session,
            terminalConfiguration: .init {
                $0.withBackground(Self.hex(background))
                $0.withForeground(Self.hex(foreground))
                $0.withPalette(1, color: Self.hex(paletteEntry))
                $0.withWindowPaddingX(0)
                $0.withWindowPaddingY(0)
            }
        )
    }

    private static func hex(_ gray: UInt8) -> String {
        String(format: "%02x%02x%02x", gray, gray, gray)
    }
}
