import AppKit
import ApplicationServices
import CoreGraphics
import Darwin
import KodosiDesign
import SwiftUI

/// The permissions of macOS that Kodosi uses. A person gives them in System Settings, and
/// programs in Kodosi's terminals have them too.
@MainActor
enum Privacy {
    /// A list of System Settings › Privacy & Security that Kodosi can be in.
    enum Pane {
        case fullDiskAccess
        /// What a program needs to change another app, as a browser that a terminal starts does.
        case appManagement
        /// What a program needs to see and control other apps.
        case accessibility
        /// What a program needs to see the screen.
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

    /// Whether Kodosi has Full Disk Access. macOS answers without asking the person, and a
    /// change counts once Kodosi opens again. It asks about the first of the folders that only
    /// apps with Full Disk Access read and that this Mac has.
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

    /// Asks macOS for a permission that an app can ask for. macOS then lists Kodosi there,
    /// switched off, and shows its own question.
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

/// Opens a list of Privacy & Security in System Settings with a small card beside it that holds
/// Kodosi's icon. macOS puts no app in Full Disk Access by itself, and Kodosi in App Management
/// only once it stopped a program of a terminal, so the person drags the icon into the list.
@MainActor
final class PrivacyGuide {
    static let shared = PrivacyGuide()

    private var panel: GuidePanel?
    private var activation: (any NSObjectProtocol)?

    /// Whether the guide shows, so the person is turning on a permission in System Settings.
    var isShowing: Bool {
        panel != nil
    }

    func show(_ pane: Privacy.Pane, theme: AppTheme) {
        Privacy.ask(for: pane)
        Privacy.open(pane)
        let panel = panel ?? GuidePanel()
        let card = PrivacyCard(pane: pane) { [weak self] in self?.close() }
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

/// A card that floats over other apps without taking their place in front.
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
    let close: () -> Void

    private var lines: (title: String, then: String) {
        switch pane {
        case .fullDiskAccess:
            (String(localized: "Drag Kodosi into the list"), String(localized: "Then turn it on, and let macOS open Kodosi again."))
        case .appManagement, .accessibility, .screenRecording:
            (String(localized: "Turn on Kodosi in the list"), String(localized: "If it is not there, drag this icon into the list."))
        }
    }

    var body: some View {
        HStack(spacing: 14) {
            AppIconDrag().frame(width: 48, height: 48)
                .help(String(localized: "Drag Kodosi into the list"))
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

/// Kodosi's icon, which drags the app itself, as Finder does.
private struct AppIconDrag: NSViewRepresentable {
    func makeNSView(context _: Context) -> AppIconSource {
        AppIconSource()
    }

    func updateNSView(_: AppIconSource, context _: Context) {}

    final class AppIconSource: NSView, NSDraggingSource {
        override var mouseDownCanMoveWindow: Bool {
            false
        }

        override func acceptsFirstMouse(for _: NSEvent?) -> Bool {
            true
        }

        override func draw(_: NSRect) {
            NSApp.applicationIconImage.draw(in: bounds)
        }

        override func mouseDown(with _: NSEvent) {}

        override func mouseDragged(with event: NSEvent) {
            let item = NSDraggingItem(pasteboardWriter: Bundle.main.bundleURL as NSURL)
            item.setDraggingFrame(bounds, contents: NSApp.applicationIconImage)
            beginDraggingSession(with: [item], event: event, source: self)
        }

        func draggingSession(_: NSDraggingSession, sourceOperationMaskFor _: NSDraggingContext) -> NSDragOperation {
            .copy
        }
    }
}
