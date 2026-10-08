import Foundation
@testable import KodosiDesktop
import Testing

@Test func ephemeralDefaultsCleanUpAfterTheirLastConsumer() throws {
    var owner = EphemeralUserDefaults(prefix: "EphemeralDefaultsLifetimeTests")
    let suite = try #require(owner?.suiteName)
    let file = try #require(owner?.persistentFileURL)
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

@Test func ephemeralDefaultsPreserveFoundationPersistenceAndOtherSuites() throws {
    let retained = try #require(EphemeralUserDefaults(prefix: "EphemeralDefaultsIsolationTests"))
    retained.set("retained", forKey: "value")
    #expect(retained.synchronize())
    var discarded = EphemeralUserDefaults(prefix: "EphemeralDefaultsIsolationTests")
    let discardedSuite = try #require(discarded?.suiteName)
    let discardedFile = try #require(discarded?.persistentFileURL)
    #expect(discardedSuite != retained.suiteName)
    discarded?.set(["one", "two"], forKey: "value")
    #expect(discarded?.synchronize() == true)

    let reader = try #require(UserDefaults(suiteName: discardedSuite))
    #expect(reader.stringArray(forKey: "value") == ["one", "two"])
    discarded = nil

    #expect(reader.object(forKey: "value") == nil)
    #expect(!FileManager.default.fileExists(atPath: discardedFile.path))
    #expect(retained.string(forKey: "value") == "retained")
    #expect(FileManager.default.fileExists(atPath: retained.persistentFileURL.path))
    try retained.cleanUp()
    #expect(!FileManager.default.fileExists(atPath: retained.persistentFileURL.path))
}

@Test func ephemeralDefaultsExplicitCleanupIsRepeatable() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "EphemeralDefaultsCleanupTests"))
    defaults.set(Data([1, 2, 3]), forKey: "value")
    #expect(defaults.synchronize())
    try defaults.cleanUp()
    try defaults.cleanUp()

    #expect(defaults.object(forKey: "value") == nil)
    #expect(!FileManager.default.fileExists(atPath: defaults.persistentFileURL.path))
    #expect(!FileManager.default.fileExists(atPath: defaults.persistentFileURL.deletingLastPathComponent().path))
}

@Test func ephemeralDefaultsUsePrivateTemporaryBacking() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "EphemeralDefaultsBackingTests"))
    let directory = defaults.persistentFileURL.deletingLastPathComponent()
    let attributes = try FileManager.default.attributesOfItem(atPath: directory.path)
    let mode = try #require(attributes[.posixPermissions] as? NSNumber)
    #expect(mode.intValue == 0o700)
    #expect(defaults.suiteName == directory.appending(path: "preferences").path)
    #expect(directory.path.hasPrefix(FileManager.default.temporaryDirectory.path))

    defaults.set(42, forKey: "value")
    #expect(defaults.synchronize())
    let data = try Data(contentsOf: defaults.persistentFileURL)
    let plist = try #require(PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Int])
    #expect(plist["value"] == 42)
}
