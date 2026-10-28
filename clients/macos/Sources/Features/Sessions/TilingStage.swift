import KodosiBridge
import KodosiDesign
import SwiftUI

struct TilingStage: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Bindable var workbench: WorkbenchState
    @State private var dropTarget: String?

    private var presentedSessionIds: [String] {
        workbench.stagedSessionIds.filter { deps.session($0).map { $0.status != .closing } == true }
    }

    var body: some View {
        if presentedSessionIds.isEmpty {
            StageOverview().transition(.opacity)
        } else {
            TerminalTilesLayout {
                ForEach(presentedSessionIds, id: \.self) { id in
                    if let session = deps.session(id) {
                        let visible = workbench.focusedSessionId == nil || workbench.focusedSessionId == id
                        SessionTileView(session: session, isFocused: workbench.focusedSessionId == id)
                            .disabled(!visible)
                            .layoutValue(key: TerminalTileVisibility.self, value: visible)
                            .opacity(visible ? 1 : 0).allowsHitTesting(visible).accessibilityHidden(!visible)
                            .environment(\.terminalTileVisible, visible)
                            .terminalScope()
                            .overlay {
                                RoundedRectangle(cornerRadius: SessionTileView.radius, style: .continuous)
                                    .strokeBorder(theme.colors.accent, lineWidth: 2)
                                    .opacity(dropTarget == id ? 1 : 0)
                                    .allowsHitTesting(false)
                            }
                            .dropDestination(for: TerminalDrag.self) { items, _ in
                                guard let dragged = items.first?.sessionId else { return false }
                                return withAnimation(theme.motion.spring) { deps.place(dragged, at: id) }
                            } isTargeted: { targeted in
                                if targeted {
                                    dropTarget = id
                                } else if dropTarget == id {
                                    dropTarget = nil
                                }
                            }
                    }
                }
            }
            .padding(6)
            .animation(theme.motion.snappy, value: dropTarget)
        }
    }
}

struct StageOverview: View {
    @Environment(AppDependencies.self) private var deps

    var body: some View {
        GeometryReader { proxy in
            AgentLauncher(centered: true, focus: deps.workbench.launcherFocus)
                .frame(maxWidth: .infinity)
                .padding(.top, max(40, proxy.size.height / 2 - 150))
        }
    }
}
