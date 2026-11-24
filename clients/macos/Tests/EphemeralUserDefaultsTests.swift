import Foundation
@testable import KodosiDesktop
import Testing

@Test func `ephemeral defaults clean up after their last consumer`() throws {
    var owner = EphemeralUserDefaults(prefix: "EphemeralDefaultsLifetimeTests")
    let suite = try #require(owner?.suiteName)
    let file = try #require(owner?.preferencesFile)
    weak let reference = owner
    var consumer: UserDefaults? = owner

    owner?.set("saved", forKey: "value")
    #expect(owner?.synchronize() == true)
    #expect(FileManager.default.fileExists(atPath: file.path))
    owner = nil

    #expect(reference != nil)
    #expect(consumer?.string(forKey: "value") == "saved")
    #expect(FileManager.default.fileExists(atPath: file.path))
    consumer = nil

    #expect(reference == nil)
    #expect(!FileManager.default.fileExists(atPath: file.path))
    let reader = try #require(UserDefaults(suiteName: suite))
    #expect(reader.object(forKey: "value") == nil)
    #expect(reader.synchronize())
    #expect(!FileManager.default.fileExists(atPath: file.path))
}

@Test func `ephemeral defaults explicit cleanup is repeatable`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "EphemeralDefaultsCleanupTests"))
    defaults.set(Data([1, 2, 3]), forKey: "value")
    #expect(defaults.synchronize())
    try defaults.cleanUp()
    try defaults.cleanUp()

    #expect(defaults.object(forKey: "value") == nil)
    #expect(!FileManager.default.fileExists(atPath: defaults.preferencesFile.path))
    #expect(!FileManager.default.fileExists(atPath: defaults.preferencesFile.deletingLastPathComponent().path))
}

private extension EphemeralUserDefaults {
    var preferencesFile: URL {
        URL(filePath: suiteName + ".plist")
    }
}
