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
    var label: String?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        shape.fill(theme.colors.accentSoft.opacity(shown ? 0.55 : 0))
            .overlay { shape.strokeBorder(theme.colors.accent.opacity(shown ? 1 : 0), style: StrokeStyle(lineWidth: label == nil ? 1.5 : 2, dash: [5, 4])) }
            .overlay {
                if shown, let label {
                    VStack(spacing: 10) {
                        Image(systemName: "arrow.down").font(.system(size: 18, weight: .bold)).foregroundStyle(theme.colors.onAccent)
                            .frame(width: 48, height: 48).background(theme.colors.accent, in: Circle())
                        Text(label).appTextStyle(.subhead).foregroundStyle(theme.colors.ink)
                    }
                    .transition(theme.motion.reduced ? .opacity : .scale(scale: 0.6).combined(with: .opacity))
                }
            }
            .scaleEffect(shown || theme.motion.reduced ? 1 : 0.97)
            .animation(theme.motion.spring, value: shown)
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
