import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

/// The shared browser of this Mac: its Kodosi Browser, the sign-ins that a person copies from
/// another browser, and the data of the private terminals. Each step is a person's own.
struct BrowserSettingsView: View {
    private static let clearKey = "clear"
    private static let engineKey = "engine"

    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var status: BrowserStatus?
    @State private var working: String?
    @State private var done: [String: String] = [:]
    @State private var problem: String?
    @State private var clearing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            ListGroup {
                ListRow(String(localized: "Kodosi Browser"), subtitle: engineLine, symbol: "globe", tint: TileTint.blue) {
                    if let status, status.getting == nil {
                        step(status.engine == nil ? String(localized: "Download") : String(localized: "Check for Updates"),
                             key: Self.engineKey, identifier: "settings.browser.get")
                        {
                            try await deps.canvas.getBrowser()
                            let fresh = try await deps.canvas.browserStatus()
                            if status.engine != nil, status.engine == fresh.engine {
                                done[Self.engineKey] = String(localized: "Up to date")
                            }
                        }
                    }
                }
            }
            ListGroup(footer: "Sites then know you in your private terminals on this Mac. Passkeys and sign-ins bound to a device stay in their own browser.") {
                if let status, status.sources.isEmpty {
                    ListRow(String(localized: "No other browser on this Mac"), symbol: "globe", tint: TileTint.graphite)
                }
                ForEach(status?.sources ?? [], id: \.self) { source in
                    ListRow(title: source.name,
                            subtitle: done[source.name] ?? (source.protected ? String(localized: "Needs Full Disk Access") : nil))
                    {
                        BrowserIcon(browser: source.browser)
                    } trailing: {
                        let token = AccessibilityIdentifier.token(source.name)
                        if source.protected {
                            Button("Allow…") { PrivacyGuide.shared.show(.fullDiskAccess, theme: theme) }
                                .buttonStyle(.kodosi(.secondary, size: .small))
                                .accessibilityIdentifier("settings.browser.allow.\(token)")
                        } else {
                            step(String(localized: "Copy"), key: source.name, identifier: "settings.browser.copy.\(token)") {
                                let sites = try await deps.canvas.copySignIns(from: source.name)
                                done[source.name] = String(localized: "Copied for \(sites) sites")
                            }
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
                ErrorNote(message: problem).transition(AnyTransition.rise)
            }
        }
        .animation(theme.motion.spring, value: problem)
        .animation(theme.motion.spring, value: status)
        .confirmationDialog("Remove all browsing data?", isPresented: $clearing, titleVisibility: .visible) {
            Button("Remove All", role: .destructive) { clear() }
        } message: { Text("Sites forget you in your private terminals on this Mac. Their open pages load again.") }
        .task(id: deps.accountEpoch) { await load() }
        .task(id: getting) {
            while getting {
                try? await Task.sleep(for: .milliseconds(400))
                let fresh = try? await deps.canvas.browserStatus()
                guard !Task.isCancelled else { return }
                if let fresh {
                    status = fresh
                }
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            if working == nil {
                Task { await load() }
            }
        }
    }

    /// Whether Kodosi Browser comes now, from this page or from a canvas that opened it first.
    private var getting: Bool {
        working == Self.engineKey || status?.getting != nil
    }

    private var engineLine: String {
        guard let status else { return String(localized: "Looking on this Mac") }
        if let getting = status.getting {
            return String(localized: "Downloading, \(getting)%")
        }
        guard let engine = status.engine else { return String(localized: "Downloads when the shared browser first opens") }
        return done[Self.engineKey].map { "\(engine) · \($0)" } ?? engine
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
        done[key] = nil
        Task { @MainActor in
            do {
                try await action()
            } catch {
                problem = error.localizedDescription
            }
            await load()
            working = nil
        }
    }
}

/// The icon of a browser's app on this Mac, or a globe when the app is not here.
private struct BrowserIcon: View {
    private static let apps = [
        "chrome": "com.google.Chrome", "edge": "com.microsoft.edgemac", "brave": "com.brave.Browser",
        "chromium": "org.chromium.Chromium", "arc": "company.thebrowser.Browser", "vivaldi": "com.vivaldi.Vivaldi",
        "firefox": "org.mozilla.firefox", "safari": "com.apple.Safari",
    ]
    private static var icons: [String: NSImage] = [:]

    let browser: String

    var body: some View {
        if let icon = Self.icon(of: browser) {
            Image(nsImage: icon).resizable().interpolation(.high).frame(width: 28, height: 28)
                .accessibilityHidden(true)
        } else {
            IconTile(symbol: "globe", tint: TileTint.teal, size: 28)
        }
    }

    private static func icon(of browser: String) -> NSImage? {
        if let icon = icons[browser] {
            return icon
        }
        guard let identifier = apps[browser],
              let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: identifier) else { return nil }
        let icon = NSWorkspace.shared.icon(forFile: app.path(percentEncoded: false))
        icons[browser] = icon
        return icon
    }
}
