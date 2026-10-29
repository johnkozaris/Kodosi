import KodosiBridge
import KodosiDesign
import SwiftUI

struct TilingStage: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Bindable var workbench: WorkbenchState
    @State private var dropTarget: String?

    private enum Tile: Hashable {
        case terminal(String)
        case canvas(String)

        var sessionId: String {
            switch self {
            case let .terminal(id), let .canvas(id): id
            }
        }
    }

    private var presentedSessionIds: [String] {
        workbench.stagedSessionIds.filter { deps.session($0).map { $0.status != .closing } == true }
    }

    /// Each terminal, with its canvas next to it when the canvas is open.
    private var tiles: [Tile] {
        presentedSessionIds.flatMap { id in
            workbench.canvasSessionIds.contains(id) && deps.session(id)?.canvas != nil ? [Tile.terminal(id), .canvas(id)] : [.terminal(id)]
        }
    }

    var body: some View {
        if presentedSessionIds.isEmpty {
            StageOverview().transition(.opacity)
        } else {
            TerminalTilesLayout {
                ForEach(tiles, id: \.self) { tile in
                    if let session = deps.session(tile.sessionId) {
                        let id = session.id
                        let visible = workbench.focusedSessionId == nil || workbench.focusedSessionId == id
                        if case .canvas = tile {
                            CanvasTileView(session: session) {
                                withAnimation(theme.motion.spring) { workbench.closeCanvas(id) }
                            }
                            .disabled(!visible)
                            .layoutValue(key: TerminalTileVisibility.self, value: visible)
                            .layoutValue(key: TerminalTileCompanion.self, value: true)
                            .opacity(visible ? 1 : 0).allowsHitTesting(visible).accessibilityHidden(!visible)
                        } else {
                            terminal(session, visible: visible)
                        }
                    }
                }
            }
            .padding(6)
            .animation(theme.motion.snappy, value: dropTarget)
        }
    }

    private func terminal(_ session: RuntimeSession, visible: Bool) -> some View {
        let id = session.id
        return SessionTileView(session: session, isFocused: workbench.focusedSessionId == id)
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
