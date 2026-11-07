import Foundation
@testable import KodosiDesktop
import Testing

@Test @MainActor func `the language follows macOS until Settings chooses one`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "Language"))
    let settings = DesktopSettings(defaults: defaults)
    #expect(settings.language == nil)
    settings.language = "ja"
    #expect(DesktopSettings(defaults: defaults).language == "ja")
    settings.language = nil
    #expect(DesktopSettings(defaults: defaults).language == nil)
}
