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
    @Binding var open: Bool
    @State private var flyout: AgentActions.Flyout?

    private var launcher: LauncherState? {
        if session.kind == .local {
            return deps.launcher
        }
        guard session.isOwner, let host = session.hostDeviceId else { return nil }
        return deps.launcher(for: host)
    }

    private var face: ChatModelFace {
        ChatModelFace(models: launcher?.models[session.agent.rawValue], model: name, effort: effort, opens: session.canControl && session.programLive != nil)
    }

    var body: some View {
        if session.canControl, let live = session.programLive {
            Button { open.toggle() } label: {
                face.padding(.horizontal, -3)
            }
            .buttonStyle(.kodosi(open ? .tinted : .secondary, size: .small))
            .task(id: session.program) {
                await launcher?.refresh()
                await launcher?.loadModels(session.agent.rawValue)
            }
            .animation(theme.motion.snappy, value: open)
            .help("Model")
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
            face.foregroundStyle(theme.colors.inkMuted).padding(.horizontal, 4)
        }
    }
}
