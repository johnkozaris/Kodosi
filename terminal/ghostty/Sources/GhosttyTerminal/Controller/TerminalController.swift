import Foundation
import GhosttyKit

@MainActor
public final class TerminalController {
    struct PreparedConfig {
        let rawValue: ghostty_config_t
        let managedConfigURL: URL?
        let renderedContents: String
    }

    struct ConfigurationIssue: Error, CustomStringConvertible {
        let description: String

        init(_ description: String) {
            self.description = description
        }
    }

    enum ConfigSource: Sendable, Hashable {
        case none
        case generated(String)
    }

    public static let shared = TerminalController()

    static let defaultRenderedConfig = TerminalConfiguration.default.rendered
    private static var runtimeInitialized = false

    nonisolated(unsafe) var app: ghostty_app_t?
    nonisolated(unsafe) var config: ghostty_config_t?
    var retainedBridges: [TerminalCallbackBridge] = []
    var configSource: ConfigSource
    var managedConfigURL: URL?
    var renderedConfigContents: String = TerminalController.defaultRenderedConfig

    var lastConfigurationIssue: String?

    private let baseConfigSource: ConfigSource
    private var baseConfigTemplate: String = ""

    let terminalConfiguration: TerminalConfiguration

    let theme: TerminalTheme

    private(set) var effectiveColorScheme: TerminalColorScheme = .light

    var renderedConfig: String {
        renderedConfigContents
    }

    public convenience init() {
        self.init(configuration: .default)
    }

    public convenience init(
        configuration: TerminalConfiguration,
        theme: TerminalTheme = .default
    ) {
        self.init(
            configSource: .generated(configuration.rendered),
            theme: theme
        )
    }

    public convenience init(
        theme: TerminalTheme = .default,
        configure: (inout TerminalConfiguration.Builder) -> Void
    ) {
        self.init(
            configuration: TerminalConfiguration(
                startingFrom: .default,
                configure: configure
            ),
            theme: theme
        )
    }

    private init(
        configSource: ConfigSource = .none,
        theme: TerminalTheme = .default,
        terminalConfiguration: TerminalConfiguration = .init()
    ) {
        Self.initializeRuntimeIfNeeded()

        baseConfigSource = configSource
        self.theme = theme
        self.terminalConfiguration = terminalConfiguration
        self.configSource = configSource

        applyInitialConfig(source: configSource)
        baseConfigTemplate = renderedConfigContents

        reconfigure()
        createApp()
    }

    @discardableResult
    func setColorScheme(_ scheme: TerminalColorScheme) -> Bool {
        let previous = effectiveColorScheme
        guard scheme != previous else {
            if let app {
                ghostty_app_set_color_scheme(app, scheme.ghosttyValue)
            }
            return false
        }

        let resolved = resolveEffectiveConfig(colorScheme: scheme)
        guard applyResolvedConfig(
            resolved,
            applyState: { effectiveColorScheme = scheme }
        ) else {
            return false
        }

        if let app {
            ghostty_app_set_color_scheme(app, scheme.ghosttyValue)
        }

        return true
    }

    @discardableResult
    private func reconfigure() -> Bool {
        applyResolvedConfig(resolveEffectiveConfig())
    }

    private func resolveEffectiveConfig() -> (
        source: ConfigSource, contents: String
    ) {
        resolveEffectiveConfig(
            theme: theme,
            terminalConfiguration: terminalConfiguration,
            colorScheme: effectiveColorScheme
        )
    }

    private func resolveEffectiveConfig(
        theme: TerminalTheme? = nil,
        terminalConfiguration: TerminalConfiguration? = nil,
        colorScheme: TerminalColorScheme? = nil
    ) -> (source: ConfigSource, contents: String) {
        let nextTheme = theme ?? self.theme
        let nextTerminalConfiguration = terminalConfiguration ?? self.terminalConfiguration
        let nextColorScheme = colorScheme ?? effectiveColorScheme
        let themeConfig = nextTheme.configuration(for: nextColorScheme)
        if nextTerminalConfiguration.isEmpty, themeConfig.isEmpty {
            return (baseConfigSource, baseConfigTemplate)
        }

        let contents = GhosttyConfigRenderer.render(
            baseContents: baseConfigTemplate,
            configuration: nextTerminalConfiguration,
            theme: themeConfig
        )
        return (.generated(contents), contents)
    }

    public func tick() {
        guard let app else { return }
        ghostty_app_tick(app)
    }

    func handleWakeup() {
        handleWakeup(tickApp: tick)
    }

    func handleWakeup(tickApp: () -> Void) {
        if retainedBridges.isEmpty {
            tickApp()
            return
        }

        let activeBridges = retainedBridges.filter {
            $0.canProcessAppWakeup?() ?? true
        }
        tickApp()
        guard !activeBridges.isEmpty else {
            TerminalDebugLog.log(.lifecycle, "surface wakeup suspended")
            return
        }

        for bridge in activeBridges {
            bridge.onAppWakeup?()
        }
    }

    private static func initializeRuntimeIfNeeded() {
        guard !runtimeInitialized else { return }
        runtimeInitialized = true
        ghostty_init(0, nil)
    }

    deinit {
        if let app {
            ghostty_app_free(app)
        }
        if let config {
            ghostty_config_free(config)
        }
        if let managedConfigURL {
            try? FileManager.default.removeItem(at: managedConfigURL)
        }
    }
}
