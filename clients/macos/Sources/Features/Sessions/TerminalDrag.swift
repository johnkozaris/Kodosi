import CoreTransferable
import KodosiBridge
import KodosiDesign
import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    static let kodosiTerminal = UTType(exportedAs: "com.kodosi.terminal")
}

struct TerminalDrag: Codable, Transferable {
    let sessionId: String

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .kodosiTerminal)
    }
}

struct RoomShare: Equatable {
    let sessionId: String
    let roomId: String
}

struct TerminalDragPreview: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession

    var body: some View {
        HStack(spacing: 8) {
            AgentMark(session: session, size: 20, activity: session.mark(rested: .awake))
            Text(session.name).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
        }
        .padding(.leading, 8).padding(.trailing, 13).frame(height: 34)
        .raised(Radius.lg, fill: theme.colors.lifted, elevation: .floating)
    }
}

struct DropSlot: View {
    @Environment(\.theme) private var theme
    let radius: CGFloat
    let shown: Bool

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        shape.fill(theme.colors.accentSoft.opacity(shown ? 0.55 : 0))
            .overlay { shape.strokeBorder(theme.colors.accent.opacity(shown ? 1 : 0), style: StrokeStyle(lineWidth: 1.5, dash: [5, 4])) }
            .scaleEffect(shown || theme.motion.reduced ? 1 : 0.97)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

extension View {
    @ViewBuilder
    func terminalDrag(_ session: RuntimeSession, when enabled: Bool = true) -> some View {
        if enabled {
            draggable(TerminalDrag(sessionId: session.id)) { TerminalDragPreview(session: session) }
        } else {
            self
        }
    }

    func roomDrop(_ roomId: String, deps: AppDependencies, targeted: Binding<Bool>) -> some View {
        dropDestination(for: TerminalDrag.self) { items, _ in
            guard let id = items.first?.sessionId else { return false }
            return deps.askToShare(id, with: roomId)
        } isTargeted: { targeted.wrappedValue = $0 }
    }
}
