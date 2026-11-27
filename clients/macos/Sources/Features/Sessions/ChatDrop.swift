import KodosiBridge
import KodosiDesign
import SwiftUI

extension View {
    /// A terminal dropped on the chat names its agent in the message, or hands that agent's work
    /// to the chat's agent. Only the agents that the chat reaches can drop, while the chat takes
    /// messages.
    func chatDrop(_ chat: TerminalChat) -> some View {
        modifier(ChatDrop(chat: chat))
    }
}

private struct ChatDrop: ViewModifier {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    @State private var targeted = false
    @State private var dropped: Dropped?

    private struct Dropped {
        let peer: ChatPeer
        let point: CGPoint
    }

    func body(content: Content) -> some View {
        content
            .overlay { DropSlot(radius: Radius.lg, shown: targeted).padding(6) }
            .overlay(alignment: .topLeading) {
                if let dropped {
                    Color.clear.frame(width: 1, height: 1)
                        .kodosiPopover(isPresented: shown, arrowEdge: .bottom) { menu(dropped.peer).chromeScope() }
                        .position(dropped.point)
                }
            }
            .dropDestination(for: TerminalDrag.self) { items, point in
                guard let peer = reachable(items.first?.sessionId) else { return false }
                dropped = Dropped(peer: peer, point: point)
                return true
            } isTargeted: { targeted = $0 && reachable(TerminalDrag.current) != nil }
            .animation(theme.motion.snappy, value: targeted)
    }

    private func reachable(_ terminal: String?) -> ChatPeer? {
        guard chat.canWrite else { return nil }
        return chat.peers.first { $0.terminal == terminal }
    }

    private var shown: Binding<Bool> {
        Binding(get: { dropped != nil }, set: { shown in
            if !shown {
                dropped = nil
            }
        })
    }

    private func menu(_ peer: ChatPeer) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            PanelMenuHeader(verbatim: peer.name)
            PanelMenuRow(Text("Mention"), icon: .symbol(ChatSymbol.mention), identifier: "chat.drop.mention") {
                chat.mention(peer)
            }
            PanelMenuRow(Text("Hand off"), icon: .symbol(ChatSymbol.handOff), identifier: "chat.drop.handOff") {
                chat.handOff(peer)
            }
        }
        .padding(6).panelMenuWidth(180)
    }
}
