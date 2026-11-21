import KodosiBridge
import KodosiDesign
import SwiftUI

/// The model in a chat's facts opens the actions of the terminal's agent, which change its model.
struct ChatModelKey: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let name: String
    let effort: String?
    @State private var open = false
    @State private var hovered = false
    @State private var flyout: AgentActions.Flyout?

    private var launcher: LauncherState? {
        if session.kind == .local {
            return deps.launcher
        }
        guard session.isOwner, let host = session.hostDeviceId else { return nil }
        return deps.launcher(for: host)
    }

    private var face: ChatModelFace {
        ChatModelFace(models: launcher?.models[session.agent.rawValue], model: name, effort: effort)
    }

    var body: some View {
        if session.canControl, let live = session.programLive {
            Button { open.toggle() } label: {
                face.foregroundStyle(hovered || open ? theme.colors.ink : theme.colors.inkFaint)
            }
            .buttonStyle(.plain)
            .onHover { hovered = $0 }
            .task(id: session.program) {
                await launcher?.refresh()
                await launcher?.loadModels(session.agent.rawValue)
            }
            .animation(theme.motion.hover, value: hovered)
            .tip("Model")
            .accessibilityLabel(Text("Model"))
            .accessibilityValue(Text(verbatim: face.title))
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
            face
        }
    }
}
