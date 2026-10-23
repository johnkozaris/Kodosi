import KodosiBridge
import SwiftUI

enum TerminalPrompt: Identifiable, Equatable {
    case rename(RuntimeSession)
    case close(RuntimeSession)
    case offline(RuntimeSession)
    case leave(RuntimeSession)

    static func closing(_ session: RuntimeSession) -> Self {
        session.hostOffline ? .offline(session) : .close(session)
    }

    var id: String {
        switch self {
        case let .rename(session): "rename.\(session.id)"
        case let .close(session): "close.\(session.id)"
        case let .offline(session): "offline.\(session.id)"
        case let .leave(session): "leave.\(session.id)"
        }
    }

    var session: RuntimeSession {
        switch self {
        case let .rename(session), let .close(session), let .offline(session), let .leave(session): session
        }
    }
}

private struct TerminalPrompts: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var prompt: TerminalPrompt?
    let onEnd: () -> Void
    @State private var name = ""

    private func presenting(_ matches: @escaping (TerminalPrompt) -> Bool) -> Binding<Bool> {
        Binding(
            get: { prompt.map(matches) ?? false },
            set: { shown in
                if !shown {
                    prompt = nil
                }
            },
        )
    }

    private var renaming: Binding<Bool> {
        presenting {
            if case .rename = $0 {
                true
            } else {
                false
            }
        }
    }

    private var closing: Binding<Bool> {
        presenting {
            if case .close = $0 {
                true
            } else {
                false
            }
        }
    }

    private var leaving: Binding<Bool> {
        presenting {
            if case .leave = $0 {
                true
            } else {
                false
            }
        }
    }

    private var away: Binding<Bool> {
        presenting {
            if case .offline = $0 {
                true
            } else {
                false
            }
        }
    }

    func body(content: Content) -> some View {
        content
            .onChange(of: prompt) { _, next in
                if case let .rename(session) = next {
                    name = session.name
                }
            }
            .alert("Rename terminal", isPresented: renaming, presenting: prompt) { current in
                TextField("Name", text: $name)
                    .accessibilityIdentifier("terminal.rename.field")
                Button("Rename") {
                    model.terminals.rename(current.session, to: name)
                }
                .accessibilityIdentifier("terminal.rename.save")
                Button("Cancel", role: .cancel) {}
            }
            .confirmationDialog(
                prompt.map { String(localized: "Close \($0.session.name)?") } ?? "",
                isPresented: closing,
                titleVisibility: .visible,
                presenting: prompt,
            ) { current in
                Button("Close terminal", role: .destructive) {
                    model.terminals.close(current.session)
                    onEnd()
                }
                .accessibilityIdentifier("terminal.close.confirm")
            } message: { _ in
                Text("Its programs stop.")
            }
            .confirmationDialog(
                prompt.map { String(localized: "\($0.session.hostLabel) is offline") } ?? "",
                isPresented: away,
                titleVisibility: .visible,
                presenting: prompt,
            ) { current in
                Button("Hide until it's back") {
                    model.terminals.hide(current.session)
                    onEnd()
                }
                .accessibilityIdentifier("terminal.offline.hide")
                Button("Close forever", role: .destructive) {
                    model.terminals.close(current.session)
                    onEnd()
                }
                .accessibilityIdentifier("terminal.offline.close")
            }
            .confirmationDialog("Leave this terminal?", isPresented: leaving, titleVisibility: .visible, presenting: prompt) { current in
                Button("Leave", role: .destructive) {
                    model.terminals.leave(current.session)
                    onEnd()
                }
                .accessibilityIdentifier("terminal.leave.confirm")
            } message: { _ in
                Text("You lose access until the owner shares it again. The terminal keeps running.")
            }
    }
}

extension View {
    func terminalPrompts(_ prompt: Binding<TerminalPrompt?>, onEnd: @escaping () -> Void = {}) -> some View {
        modifier(TerminalPrompts(prompt: prompt, onEnd: onEnd))
    }
}

extension RuntimeSession {
    var canLeave: Bool {
        !isOwner && missionId == nil
    }
}
