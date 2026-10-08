import Foundation
import GhosttyKit

extension TerminalController {
    func createSurface(
        bridge: TerminalCallbackBridge,
        configuration: TerminalSurfaceOptions,
        platformSetup: (inout ghostty_surface_config_s) -> Void
    ) -> ghostty_surface_t? {
        guard let app else { return nil }

        var surfaceConfig = ghostty_surface_config_new()
        surfaceConfig.userdata = Unmanaged.passUnretained(bridge).toOpaque()
        surfaceConfig.context = GHOSTTY_SURFACE_CONTEXT_WINDOW
        surfaceConfig.backend = GHOSTTY_SURFACE_IO_BACKEND_HOST_MANAGED
        surfaceConfig.receive_userdata =
            Unmanaged.passUnretained(configuration.session).toOpaque()
        surfaceConfig.receive_buffer = InMemoryTerminalSession.receiveBufferCallback
        surfaceConfig.receive_resize = InMemoryTerminalSession.receiveResizeCallback

        if let fontSize = configuration.fontSize {
            surfaceConfig.font_size = fontSize
        }

        return buildSurface(
            app: app,
            bridge: bridge,
            configuration: configuration,
            config: &surfaceConfig,
            platformSetup: platformSetup
        )
    }

    func retain(_ bridge: TerminalCallbackBridge) {
        guard !retainedBridges.contains(where: { $0 === bridge }) else { return }
        retainedBridges.append(bridge)
    }

    func remove(_ bridge: TerminalCallbackBridge) {
        retainedBridges.removeAll { $0 === bridge }
        bridge.clearSurfaceConfig()
    }

    @discardableResult
    func updateSurface(
        _ surface: ghostty_surface_t,
        bridge: TerminalCallbackBridge,
        configuration: TerminalSurfaceOptions
    ) -> Bool {
        guard configuration.hasSurfaceConfiguration else {
            bridge.clearSurfaceConfig()
            if let config {
                ghostty_surface_update_config(surface, config)
            }
            return true
        }

        let rendered = resolvedSurfaceConfigContents(configuration)
        switch Self.prepareConfig(source: .generated(rendered)) {
        case let .success(prepared):
            ghostty_surface_update_config(surface, prepared.rawValue)
            let previousConfig = bridge.surfaceConfig
            let previousURL = bridge.managedConfigURL
            bridge.surfaceConfig = prepared.rawValue
            bridge.managedConfigURL = prepared.managedConfigURL
            if let previousConfig {
                ghostty_config_free(previousConfig)
            }
            if let previousURL, previousURL != prepared.managedConfigURL {
                try? FileManager.default.removeItem(at: previousURL)
            }
            return true
        case let .failure(issue):
            lastConfigurationIssue = issue.description
            return false
        }
    }

    var retainedBridgeCount: Int {
        retainedBridges.count
    }

    func resolvedSurfaceConfigContents(
        _ options: TerminalSurfaceOptions
    ) -> String {
        guard options.hasSurfaceConfiguration else {
            return renderedConfigContents
        }

        var overrides = options.terminalConfiguration ?? .init()
        if let fontSize = options.fontSize {
            overrides = TerminalConfiguration(startingFrom: overrides) {
                $0.withFontSize(fontSize)
            }
        }
        if let scrollbackLimitBytes = options.scrollbackLimitBytes {
            overrides = TerminalConfiguration(startingFrom: overrides) {
                $0.withCustom("scrollback-limit", String(scrollbackLimitBytes))
            }
        }
        return GhosttyConfigRenderer.render(
            baseContents: renderedConfigContents,
            configuration: overrides,
            theme: .init()
        )
    }

    private func buildSurface(
        app: ghostty_app_t,
        bridge: TerminalCallbackBridge,
        configuration: TerminalSurfaceOptions,
        config: inout ghostty_surface_config_s,
        platformSetup: (inout ghostty_surface_config_s) -> Void
    ) -> ghostty_surface_t? {
        platformSetup(&config)
        guard configuration.hasSurfaceConfiguration else {
            guard let surface = ghostty_surface_new(app, &config) else {
                return nil
            }
            retain(bridge)
            return surface
        }

        let rendered = resolvedSurfaceConfigContents(configuration)
        guard case let .success(prepared) = Self.prepareConfig(
            source: .generated(rendered)
        ) else {
            return nil
        }

        config.config = prepared.rawValue
        guard let surface = ghostty_surface_new(app, &config) else {
            ghostty_config_free(prepared.rawValue)
            if let managedConfigURL = prepared.managedConfigURL {
                try? FileManager.default.removeItem(at: managedConfigURL)
            }
            return nil
        }

        bridge.surfaceConfig = prepared.rawValue
        bridge.managedConfigURL = prepared.managedConfigURL
        retain(bridge)

        return surface
    }
}
