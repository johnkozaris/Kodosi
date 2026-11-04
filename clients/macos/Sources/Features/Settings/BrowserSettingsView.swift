import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

/// The shared browser of this Mac: its Kodosi Browser, the sign-ins that a person copies from
/// another browser, and the data of the private terminals. Each step is a person's own.
struct BrowserSettingsView: View {
    private static let fullDiskAccess = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
    private static let clearKey = "clear"

    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var status: BrowserStatus?
    @State private var working: String?
    @State private var done: [String: String] = [:]
    @State private var problem: String?
    @State private var protectedSource = false
    @State private var clearing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            ListGroup {
                ListRow(String(localized: "Kodosi Browser"), subtitle: engineLine, symbol: "globe", tint: TileTint.blue)
            }
            ListGroup(footer: "Sites then know you in your private terminals on this Mac. Passkeys and sign-ins bound to a device stay in their own browser.") {
                if let status, status.sources.isEmpty {
                    ListRow(String(localized: "No other browser on this Mac"), symbol: "globe", tint: TileTint.graphite)
                }
                ForEach(status?.sources ?? [], id: \.self) { source in
                    ListRow(source, subtitle: done[source], symbol: "globe", tint: TileTint.teal) {
                        step(String(localized: "Copy"), key: source, identifier: "settings.browser.copy.\(AccessibilityIdentifier.token(source))") {
                            let sites = try await deps.canvas.copySignIns(from: source)
                            done[source] = String(localized: "Copied for \(sites) sites")
                        }
                    }
                }
            }
            ListGroup {
                ListRow(String(localized: "Copied sign-ins"), subtitle: copiedLine, symbol: "key.fill", tint: TileTint.amber) {
                    step(String(localized: "Remove"), key: "forget", identifier: "settings.browser.forget", disabled: (status?.sites ?? 0) == 0) {
                        _ = try await deps.canvas.removeCopiedSignIns()
                        done = [:]
                    }
                }
                ListRow(String(localized: "Browsing data"),
                        subtitle: done[Self.clearKey] ?? String(localized: "Sign-ins and site data of your private terminals"),
                        symbol: "trash.fill", tint: TileTint.red)
                {
                    Button("Remove All…") { clearing = true }
                        .buttonStyle(.kodosi(.secondary, size: .small))
                        .disabled(working != nil)
                        .accessibilityIdentifier("settings.browser.clear")
                }
            }
            if let problem {
                VStack(alignment: .leading, spacing: 10) {
                    ErrorNote(message: problem)
                    if protectedSource, let url = Self.fullDiskAccess {
                        Button("Open Privacy Settings") { NSWorkspace.shared.open(url) }
                            .buttonStyle(.kodosi(.secondary, size: .small))
                            .accessibilityIdentifier("settings.browser.privacy")
                    }
                }
                .transition(AnyTransition.rise)
            }
        }
        .animation(theme.motion.spring, value: problem)
        .animation(theme.motion.spring, value: status)
        .confirmationDialog("Remove all browsing data?", isPresented: $clearing, titleVisibility: .visible) {
            Button("Remove All", role: .destructive) { clear() }
        } message: { Text("Sites forget you in your private terminals on this Mac. Their open pages load again.") }
        .task(id: deps.accountEpoch) { await load() }
    }

    private var engineLine: String {
        guard let status else { return String(localized: "Looking on this Mac") }
        return status.engine ?? String(localized: "Downloads when the shared browser first opens")
    }

    private var copiedLine: String {
        guard let sites = status?.sites, sites > 0 else { return String(localized: "None") }
        return String(localized: "For \(sites) sites")
    }

    private func step(_ title: String, key: String, identifier: String, disabled: Bool = false,
                      _ action: @escaping @MainActor () async throws -> Void) -> some View
    {
        Button {
            run(key, action)
        } label: {
            Text(title).opacity(working == key ? 0 : 1)
                .overlay {
                    if working == key {
                        Spinner(size: 12)
                    }
                }
        }
        .buttonStyle(.kodosi(.secondary, size: .small))
        .disabled(working != nil || disabled)
        .accessibilityIdentifier(identifier)
    }

    private func clear() {
        run(Self.clearKey) {
            try await deps.canvas.removeBrowsingData()
            done = [Self.clearKey: String(localized: "Removed")]
        }
    }

    private func load() async {
        do {
            status = try await deps.canvas.browserStatus()
        } catch {
            problem = error.localizedDescription
        }
    }

    private func run(_ key: String, _ action: @escaping @MainActor () async throws -> Void) {
        working = key
        problem = nil
        protectedSource = false
        Task { @MainActor in
            do {
                try await action()
            } catch {
                problem = error.localizedDescription
                protectedSource = key == "Safari"
            }
            await load()
            working = nil
        }
    }
}
