import KodosiBridge
import SwiftUI

public struct ReportForm: View {
    @Environment(\.theme) private var theme
    @State private var note = ""
    @State private var sending = false
    @State private var failure: String?
    @FocusState private var writing: Bool
    let name: String
    let key: String
    let send: (String) async throws -> Void
    let cancel: () -> Void
    let reported: () -> Void

    public init(
        name: String, key: String, send: @escaping (String) async throws -> Void, cancel: @escaping () -> Void,
        reported: @escaping () -> Void,
    ) {
        self.name = name
        self.key = key
        self.send = send
        self.cancel = cancel
        self.reported = reported
    }

    public var body: some View {
        VStack(spacing: 22) {
            PersonAvatar(name: name, key: key, size: 52)
                .accessibilityHidden(true)
            VStack(spacing: 6) {
                Text("Report \(name)", bundle: .module)
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                Text("Kodosi gets their user name and what you write here.", bundle: .module)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
            }
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            TextField(String(localized: "What happened (optional)", bundle: .module), text: $note, axis: .vertical)
                .textFieldStyle(.plain)
                .appTextStyle(.body)
                .foregroundStyle(theme.colors.ink)
                .tint(theme.colors.accent)
                .lineLimit(4 ... 8)
                .focused($writing)
                .padding(12)
                .well(Radius.lg)
                .overlay {
                    RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                        .strokeBorder(theme.colors.accent.opacity(writing ? 0.55 : 0), lineWidth: 1.5)
                }
                .disabled(sending)
                .onChange(of: note) { _, text in
                    let fitting = ProductInput.clip(text, bytes: ProductInput.reportNoteBytes)
                    if fitting != text {
                        note = fitting
                    }
                }
                .accessibilityIdentifier("people.report.note")
            if let failure {
                ErrorNote(message: failure).transition(AnyTransition.rise)
            }
            HStack(spacing: 8) {
                Button(action: cancel) { Text("Cancel", bundle: .module) }
                    .keyboardShortcut(.cancelAction)
                    .buttonStyle(.kodosi(.ghost, size: .large))
                    .accessibilityIdentifier("people.report.cancel")
                Button(action: report) {
                    Text("Report", bundle: .module).frame(maxWidth: .infinity).shimmer(sending)
                }
                .buttonStyle(.kodosi(.danger, size: .large))
                .disabled(sending)
                .accessibilityIdentifier("people.report.send")
            }
        }
        .animation(theme.motion.spring, value: failure)
        .animation(theme.motion.hover, value: writing)
        .interactiveDismissDisabled(sending)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("people.report")
    }

    private func report() {
        guard !sending else { return }
        sending = true
        failure = nil
        Task {
            defer { sending = false }
            do {
                try await send(note)
                reported()
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}
