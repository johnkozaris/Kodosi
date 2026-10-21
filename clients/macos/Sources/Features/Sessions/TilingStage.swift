import KodosiBridge
import KodosiDesign
import SwiftUI

struct TilingStage: View {
    @Environment(AppDependencies.self) private var deps
    @Bindable var workbench: WorkbenchState

    private var presentedSessionIds: [String] {
        workbench.stagedSessionIds.filter { deps.session($0) != nil }
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
                    }
                }
            }
            .padding(6)
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
