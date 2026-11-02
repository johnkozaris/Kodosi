import KodosiBridge
import KodosiDesign
import SwiftUI

/// Copies the sign-ins of another browser of this Mac into the shared browser. Each copy is a
/// person's own step, and a key takes back what the copies brought.
struct BrowserSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var sources: [String] = []
    @State private var working = false
    @State private var told: String?
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Sign-ins").appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                Text("Copy the sign-ins of a browser of this Mac, so that sites know you in your private terminals. Shared terminals never get them.")
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).fixedSize(horizontal: false, vertical: true)
            }
            VStack(spacing: 1) {
                ForEach(sources, id: \.self) { source in
                    HStack {
                        Text(verbatim: source).appTextStyle(.body).foregroundStyle(theme.colors.ink).lineLimit(1)
                        Spacer()
                        Button("Copy") { copy(source) }
                            .disabled(working)
                            .accessibilityIdentifier("settings.browser.copy.\(AccessibilityIdentifier.token(source))")
                    }
                    .padding(.horizontal, 14).frame(height: 44)
                }
                if sources.isEmpty {
                    Text("This Mac has no browser to copy from.").appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(14)
                }
            }
            .raised(Radius.xl)
            HStack(spacing: 10) {
                Button("Remove Copied Sign-ins") { forget() }
                    .disabled(working)
                    .accessibilityIdentifier("settings.browser.forget")
                if working {
                    Spinner(size: 14)
                }
                if let told {
                    Text(verbatim: told).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
            }
            if let problem {
                Text(verbatim: problem).appTextStyle(.footnote).foregroundStyle(theme.colors.caution)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Text("Passkeys and sign-ins bound to a device stay in their own browser. macOS can ask you to allow Kodosi to read the key of a browser.")
                .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                .fixedSize(horizontal: false, vertical: true)
        }
        .task { await load() }
    }

    private func load() async {
        do {
            sources = try await deps.canvas.signInSources()
        } catch { problem = error.localizedDescription }
    }

    private func copy(_ source: String) {
        run {
            let done = try await deps.canvas.copySignIns(from: source)
            told = String(localized: "Copied \(done.copied) sign-ins from \(source).")
        }
    }

    private func forget() {
        run {
            let removed = try await deps.canvas.removeCopiedSignIns()
            told = String(localized: "Removed \(removed) sign-ins.")
        }
    }

    private func run(_ step: @escaping @MainActor () async throws -> Void) {
        working = true
        told = nil
        problem = nil
        Task { @MainActor in
            defer { working = false }
            do {
                try await step()
            } catch { problem = error.localizedDescription }
        }
    }
}
