import KodosiBridge
import KodosiDesign
import SwiftUI

struct TilingStage: View {
    private static let edge: CGFloat = 6

    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Bindable var workbench: WorkbenchState
    @State private var dropTarget: String?
    @Namespace private var keys

    private var presentedSessionIds: [String] {
        workbench.stagedSessionIds.filter { deps.session($0).map { $0.status != .closing } == true }
    }

    private var canvasSession: RuntimeSession? {
        workbench.canvasSessionId.flatMap { id in
            presentedSessionIds.contains(id) ? deps.session(id).flatMap { deps.canvas.showsSomething($0) ? $0 : nil } : nil
        }
    }

    var body: some View {
        if presentedSessionIds.isEmpty {
            StageOverview().transition(.opacity)
                .onGeometryChange(for: CGSize.self, of: \.size) { size in
                    workbench.tileRoom = CGSize(width: size.width - 2 * Self.edge, height: size.height - 2 * Self.edge)
                }
        } else {
            let alone = workbench.focusedSessionId != nil || presentedSessionIds.count == 1
            CanvasStage(canvas: canvasSession, front: workbench.focusedSessionId, wide: $workbench.canvasWide,
                        width: $workbench.canvasWidth)
            {
                workbench.canvasSessionId.map(deps.canvas.closeCanvas)
                workbench.hideCanvas()
            } terminal: {
                tiles(alone: alone).onGeometryChange(for: CGSize.self, of: \.size) { workbench.tileRoom = $0 }
            }
            .padding(Self.edge)
            .environment(\.canvasPlace, CanvasPlace(flight: keys, open: canvasSession?.id) { session, section in
                workbench.openCanvas(session.id, at: section)
            })
            .animation(theme.motion.snappy, value: dropTarget)
        }
    }

    private func tiles(alone: Bool) -> some View {
        TerminalTilesLayout {
            ForEach(presentedSessionIds, id: \.self) { id in
                if let session = deps.session(id) {
                    let visible = workbench.focusedSessionId == nil || workbench.focusedSessionId == id
                    SessionTileView(session: session, isFocused: workbench.focusedSessionId == id)
                        .disabled(!visible)
                        .layoutValue(key: TerminalTileVisibility.self, value: visible)
                        .opacity(visible ? 1 : 0).allowsHitTesting(visible).accessibilityHidden(!visible)
                        .environment(\.terminalTileVisible, visible)
                        .environment(\.terminalAlone, alone)
                        .terminalScope()
                        .overlay {
                            RoundedRectangle(cornerRadius: Radius.tile, style: .continuous)
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
