import SwiftUI
#if os(macOS)
    import AppKit
#endif

private struct KodosiThemeModifier: ViewModifier {
    @Environment(\.colorScheme) private var systemColorScheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage(ThemePreference.storageKey) private var preference: String = ThemePreference.system.rawValue
    @AppStorage(MotionPreference.storageKey) private var reducesMotion = false
    @State private var windowVisible = true

    private var resolved: AppTheme {
        let pref = ThemePreference(rawValue: preference) ?? .system
        return pref.resolve(systemColorScheme: systemColorScheme).reducingMotion(reduceMotion || reducesMotion || !windowVisible)
    }

    private var colorSchemeOverride: ColorScheme? {
        let pref = ThemePreference(rawValue: preference) ?? .system
        return pref.preferredColorScheme
    }

    func body(content: Content) -> some View {
        let theme = resolved
        content
            .transaction { transaction in
                if theme.motion.reduced {
                    transaction.animation = nil
                    transaction.disablesAnimations = true
                }
            }
            .environment(\.theme, theme)
            .tint(theme.colors.accent)
            .background(theme.colors.ground.ignoresSafeArea())
        #if os(macOS)
            .background(WindowVisibility(visible: $windowVisible))
        #endif
            .preferredColorScheme(colorSchemeOverride)
    }
}

#if os(macOS)
    /// Reports whether any part of the window shows on a screen, so that no animation runs in a hidden window.
    private struct WindowVisibility: NSViewRepresentable {
        @Binding var visible: Bool

        func makeNSView(context _: Context) -> Probe {
            Probe { visible = $0 }
        }

        func updateNSView(_: Probe, context _: Context) {}

        final class Probe: NSView {
            private let changed: (Bool) -> Void
            private var observer: (any NSObjectProtocol)?

            init(changed: @escaping (Bool) -> Void) {
                self.changed = changed
                super.init(frame: .zero)
            }

            @available(*, unavailable)
            required init?(coder _: NSCoder) {
                fatalError("init(coder:) is not available")
            }

            override func hitTest(_: NSPoint) -> NSView? {
                nil
            }

            override func viewDidMoveToWindow() {
                super.viewDidMoveToWindow()
                if let observer {
                    NotificationCenter.default.removeObserver(observer)
                }
                observer = window.map { window in
                    NotificationCenter.default.addObserver(
                        forName: NSWindow.didChangeOcclusionStateNotification, object: window, queue: .main
                    ) { [weak self] _ in
                        MainActor.assumeIsolated { self?.report() }
                    }
                }
                DispatchQueue.main.async { [weak self] in self?.report() }
            }

            private func report() {
                guard let window else { return }
                changed(window.occlusionState.contains(.visible))
            }
        }
    }
#endif

public extension View {
    func kodosiTheme() -> some View {
        modifier(KodosiThemeModifier())
    }
}
