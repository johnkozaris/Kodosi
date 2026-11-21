import KodosiBridge
import KodosiDesign
import SwiftUI

/// The model in a chat's facts opens the actions of the terminal's agent, which change its model.
struct ChatModelKey: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let label: Text
    @State private var open = false
    @State private var hovered = false
    @State private var flyout: AgentActions.Flyout?

    var body: some View {
        if session.canControl, let live = session.programLive {
            Button { open.toggle() } label: {
                label.foregroundStyle(hovered || open ? theme.colors.ink : theme.colors.inkFaint)
            }
            .buttonStyle(.plain)
            .onHover { hovered = $0 }
            .animation(theme.motion.hover, value: hovered)
            .tip("Model")
            .accessibilityLabel(Text("Model"))
            .accessibilityValue(label)
            .accessibilityIdentifier("\(AccessibilityIdentifier.stageSession(session.id)).chat.model")
            .kodosiPopover(isPresented: $open, arrowEdge: .top, alignment: .leading) {
                VStack(alignment: .leading, spacing: 2) {
                    AgentActions(session: session, agent: session.agent, live: live, stageId: AccessibilityIdentifier.stageSession(session.id),
                                 flyout: $flyout, divided: false)
                }
                .padding(6).panelMenuWidth(220).chromeScope()
            }
            .onChange(of: open) { _, open in
                if !open {
                    flyout = nil
                }
            }
        } else {
            label
        }
    }
}
