@testable import GhosttyTerminal
import Testing

@MainActor
struct TerminalThemeConfigurationTests {
    @Test
    func `command builder preserves insertion order`() {
        let configuration = TerminalConfiguration {
            $0.withFontSize(13)
            $0.withCursorStyle(.bar)
            $0.withCursorStyleBlink(false)
            $0.withBackground("#101010")
        }

        #expect(
            configuration.rendered
                == """
                font-size = 13
                cursor-style = bar
                cursor-style-blink = false
                background = #101010
                """
        )
    }

    @Test
    func `machine numeric configuration is ungrouped decimal text`() {
        let configuration = TerminalConfiguration {
            $0.withFontSize(1234.5)
            $0.withMinimumContrast(1.1)
            $0.withCursorOpacity(0.875)
            $0.withBackgroundOpacity(0.5)
        }
        #expect(configuration.rendered == """
        font-size = 1234.5
        minimum-contrast = 1.1
        cursor-opacity = 0.875
        background-opacity = 0.5
        """)
    }

    @Test
    func `controller resolves configured theme`() {
        let controller = TerminalController(
            configuration: TerminalConfiguration {
                $0.withFontSize(14)
                $0.withCursorStyle(.block)
            },
            theme: .init(
                light: TerminalConfiguration { $0.withBackground("#111111") },
                dark: TerminalConfiguration { $0.withBackground("#000000") }
            )
        )

        #expect(controller.renderedConfig.contains("font-size = 14"))
        #expect(controller.renderedConfig.contains("cursor-style = block"))
        #expect(controller.renderedConfig.contains("background = #111111"))
    }
}
