@testable import GhosttyTerminal
import Testing

struct TerminalSurfaceOptionsTests {
    @Test
    func `session identity requires a surface rebuild`() {
        let first = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let second = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let base = TerminalSurfaceOptions(session: first)

        #expect(
            TerminalSurfaceOptions(session: second)
                .requiresSurfaceRebuild(comparedTo: base)
        )
    }
}
