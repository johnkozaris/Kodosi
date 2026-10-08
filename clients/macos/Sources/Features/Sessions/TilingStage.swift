import SwiftUI

struct TilingStage: View {
    @Environment(\.theme) private var theme
    @Environment(AppDependencies.self) private var deps
    @Bindable var workbench: WorkbenchState

    private var presentedSessionIds: [String] {
        workbench.stagedSessionIds.filter { deps.session($0) != nil }
    }

    var body: some View {
        Group {
            if presentedSessionIds.isEmpty {
                VStack(spacing: 16) {
                    Image(systemName: "terminal").font(.system(size: 32)).foregroundStyle(theme.colors.primary).accessibilityHidden(true)
                    Text(deps.sessions.isEmpty ? String(localized: "Start a session") : String(localized: "Choose a session"))
                        .appTextStyle(.headingSection)
                    Button("New terminal") { deps.newSession() }
                        .buttonStyle(SolidPrimaryButtonStyle()).accessibilityIdentifier("stage.newSession")
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
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
                                .clipped()
                        }
                    }
                }.padding(4)
            }
        }.background(theme.colors.surfaceStage)
    }
}
