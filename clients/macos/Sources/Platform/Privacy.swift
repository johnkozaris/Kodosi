import AppKit
import ApplicationServices
import CoreGraphics
import Darwin
import KodosiDesign
import SwiftUI

enum Privacy {
    enum Pane {
        case fullDiskAccess
        /// Required to modify other apps, as a browser launched from a terminal does.
        case appManagement
        case accessibility
        case screenRecording

        fileprivate var anchor: String {
            switch self {
            case .fullDiskAccess: "Privacy_AllFiles"
            case .appManagement: "Privacy_AppBundles"
            case .accessibility: "Privacy_Accessibility"
            case .screenRecording: "Privacy_ScreenCapture"
            }
        }
    }

    /// macOS has no API to check Full Disk Access, so probe readability of the first existing
    /// TCC-protected user directory with `access()` (which does not trigger a prompt). Changes
    /// take effect after the app restarts.
    static var fullDiskAccess: Bool {
        let home = URL(filePath: NSHomeDirectory())
        let protected = ["Library/Safari", "Library/Containers/com.apple.Safari", "Library/Mail", "Library/Messages"]
            .map { home.appending(path: $0).path(percentEncoded: false) }
            .first { FileManager.default.fileExists(atPath: $0) }
        return protected.map { access($0, R_OK) == 0 } ?? false
    }

    static var accessibility: Bool {
        AXIsProcessTrusted()
    }

    static var screenRecording: Bool {
        CGPreflightScreenCaptureAccess()
    }

    /// Triggers the system permission prompt so macOS registers Kodosi in the Settings list.
    static func ask(for pane: Pane) {
        switch pane {
        case .accessibility:
            _ = AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
        case .screenRecording:
            _ = CGRequestScreenCaptureAccess()
        case .fullDiskAccess, .appManagement:
            break
        }
    }

    static func open(_ pane: Pane) {
        go("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?\(pane.anchor)")
    }

    static func openNotifications() {
        go("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=\(Bundle.main.bundleIdentifier ?? "")")
    }

    private static func go(_ address: String) {
        if let url = URL(string: address) {
            NSWorkspace.shared.open(url)
        }
    }
}

/// An app that macOS gives its own permissions: Kodosi, or Kodosi Screen inside it, which records
/// and controls this Mac for its owner's other devices.
enum PrivacyApp {
    case kodosi
    case screen

    var url: URL {
        switch self {
        case .kodosi: Bundle.main.bundleURL
        case .screen: Bundle.main.bundleURL.appending(path: "Contents/Helpers/Kodosi.app")
        }
    }

    var icon: NSImage {
        switch self {
        case .kodosi: NSApp.applicationIconImage
        case .screen: NSWorkspace.shared.icon(forFile: url.path(percentEncoded: false))
        }
    }
}

/// Floating helper offering a draggable app icon because macOS does not pre-populate Full Disk
/// Access (and only adds App Management after blocking an attempt). Kodosi Screen asks for its
/// own permissions when the sharing of the screen starts.
final class PrivacyGuide {
    static let shared = PrivacyGuide()

    private var panel: GuidePanel?
    private var activation: any NSObjectProtocol?

    var isShowing: Bool {
        panel != nil
    }

    func show(_ pane: Privacy.Pane, for app: PrivacyApp = .kodosi, theme: AppTheme) {
        if app == .kodosi {
            Privacy.ask(for: pane)
        }
        Privacy.open(pane)
        let panel = panel ?? GuidePanel()
        let card = PrivacyCard(pane: pane, app: app) { [weak self] in self?.close() }
            .environment(\.theme, theme)
            .environment(\.colorScheme, theme.isDark ? .dark : .light)
        panel.contentView = NSHostingView(rootView: card)
        let size = panel.contentView?.fittingSize ?? NSSize(width: 380, height: 86)
        if let screen = NSApp.keyWindow?.screen ?? NSScreen.main {
            let frame = screen.visibleFrame
            panel.setFrame(NSRect(x: frame.midX - size.width / 2, y: frame.minY + 28, width: size.width, height: size.height),
                           display: true)
        }
        panel.orderFrontRegardless()
        self.panel = panel
        activation = activation ?? NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main,
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.close() }
        }
    }

    func close() {
        panel?.orderOut(nil)
        panel = nil
        if let activation {
            NotificationCenter.default.removeObserver(activation)
        }
        activation = nil
    }
}

private final class GuidePanel: NSPanel {
    init() {
        super.init(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .floating
        hidesOnDeactivate = false
        isMovableByWindowBackground = true
        backgroundColor = .clear
        isOpaque = false
        hasShadow = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
    }

    override var canBecomeKey: Bool {
        true
    }
}

private struct PrivacyCard: View {
    @Environment(\.theme) private var theme
    let pane: Privacy.Pane
    let app: PrivacyApp
    let close: () -> Void

    private var lines: (title: String, then: String) {
        switch (pane, app) {
        case (.fullDiskAccess, _):
            (String(localized: "Drag Kodosi into the list"), String(localized: "Then turn it on, and let macOS open Kodosi again."))
        case (_, .kodosi):
            (String(localized: "Turn on Kodosi in the list"), String(localized: "If it is not there, drag this icon into the list."))
        case (_, .screen):
            (String(localized: "Turn on Kodosi Screen in the list"), String(localized: "If it is not there, drag this icon into the list."))
        }
    }

    var body: some View {
        HStack(spacing: 14) {
            AppIconDrag(app: app).frame(width: 48, height: 48)
                .help(app == .screen ? String(localized: "Drag Kodosi Screen into the list") : String(localized: "Drag Kodosi into the list"))
            VStack(alignment: .leading, spacing: 3) {
                Text(lines.title).appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                Text(lines.then).appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(width: 250, alignment: .leading)
            Button(action: close) {
                Image(systemName: "xmark").font(.system(size: 10, weight: .bold)).foregroundStyle(theme.colors.inkMuted)
                    .frame(width: 22, height: 22).contentShape(Circle())
            }
            .buttonStyle(PressScaleStyle(scale: 0.88))
            .help(String(localized: "Close"))
            .accessibilityLabel(Text("Close"))
            .accessibilityIdentifier("privacy.guide.close")
        }
        .padding(.leading, 14).padding(.trailing, 10).padding(.vertical, 12)
        .raised(Radius.lg)
        .padding(14)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("privacy.guide")
    }
}

private struct AppIconDrag: NSViewRepresentable {
    let app: PrivacyApp

    func makeNSView(context _: Context) -> AppIconSource {
        AppIconSource(app: app)
    }

    func updateNSView(_: AppIconSource, context _: Context) {}

    final class AppIconSource: NSView, NSDraggingSource {
        private let app: PrivacyApp

        init(app: PrivacyApp) {
            self.app = app
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        override var mouseDownCanMoveWindow: Bool {
            false
        }

        override func acceptsFirstMouse(for _: NSEvent?) -> Bool {
            true
        }

        override func draw(_: NSRect) {
            app.icon.draw(in: bounds)
        }

        override func mouseDown(with _: NSEvent) {}

        override func mouseDragged(with event: NSEvent) {
            let item = NSDraggingItem(pasteboardWriter: app.url as NSURL)
            item.setDraggingFrame(bounds, contents: app.icon)
            beginDraggingSession(with: [item], event: event, source: self)
        }

        func draggingSession(_: NSDraggingSession, sourceOperationMaskFor _: NSDraggingContext) -> NSDragOperation {
            .copy
        }
    }
}
