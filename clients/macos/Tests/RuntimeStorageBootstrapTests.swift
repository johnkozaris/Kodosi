import Foundation
@testable import KodosiDesktop
import Testing

private func canonicalTemporaryTestDirectory(_ name: String) throws -> URL {
    guard let resolved = realpath(FileManager.default.temporaryDirectory.path, nil) else {
        throw CocoaError(.fileNoSuchFile)
    }
    defer { free(resolved) }
    return URL(fileURLWithPath: String(cString: resolved), isDirectory: true)
        .appending(path: name, directoryHint: .isDirectory)
}

@Test func debugBackendConfigurationAcceptsOnlyLoopbackEndpoints() {
    #expect(DebugBackendConfiguration.resolve(info: [
        DebugBackendConfiguration.hostInfoKey: "127.0.0.1",
        DebugBackendConfiguration.portInfoKey: "5180",
    ]) == DebugBackendConfiguration(
        api: "http://127.0.0.1:5180"
    ))
    #expect(DebugBackendConfiguration.resolve(info: [
        DebugBackendConfiguration.hostInfoKey: "localhost",
        DebugBackendConfiguration.portInfoKey: "5180",
    ]) != nil)
    for info in [
        [:],
        [DebugBackendConfiguration.hostInfoKey: "api.kodosi.com"],
        [
            DebugBackendConfiguration.hostInfoKey: "api.kodosi.com",
            DebugBackendConfiguration.portInfoKey: "5180",
        ],
        [
            DebugBackendConfiguration.hostInfoKey: "127.0.0.1",
            DebugBackendConfiguration.portInfoKey: "0",
        ],
        [
            DebugBackendConfiguration.hostInfoKey: "127.0.0.1",
            DebugBackendConfiguration.portInfoKey: "not-a-port",
        ],
    ] {
        #expect(DebugBackendConfiguration.resolve(info: info) == nil)
    }
}

@MainActor
@Test func hostedBootstrapRequiresExplicitDataRoot() {
    #expect(throws: RuntimeStorageBootstrap.BootstrapError.missingTestDataRoot) {
        try RuntimeStorageBootstrap.resolve(
            environment: [RuntimeStorageBootstrap.xctestEnvironmentKey: "/tmp/test.xctestconfiguration"],
            processIdentifier: 42,
            installEnvironment: false
        )
    }
}

@MainActor
@Test func storageBootstrapRejectsUnsafeRoots() {
    for candidate in [
        "",
        "relative",
        "/tmp/../tmp/kodosi",
        "/",
        "/tmp/$(BUILD_ROOT)",
        FileManager.default.homeDirectoryForCurrentUser.path,
    ] {
        #expect(throws: (any Error).self) {
            try RuntimeStorageBootstrap.validateRoot(candidate)
        }
    }
}

@MainActor
@Test func storageBootstrapRejectsFilesystemAliasesOfProductionRoot() throws {
    let parent = try canonicalTemporaryTestDirectory(
        "KodosiStorageIdentityTests-\(UUID().uuidString)"
    )
    defer { try? FileManager.default.removeItem(at: parent) }
    let production = parent.appending(path: "kodosi", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(
        at: production,
        withIntermediateDirectories: true
    )
    let alias = parent.appending(path: "KODOSI", directoryHint: .isDirectory)
    let candidate = alias.appending(path: "isolated", directoryHint: .isDirectory)

    #expect(throws: (any Error).self) {
        try RuntimeStorageBootstrap.validateRoot(
            candidate.path,
            productionHome: parent,
            productionRoot: production
        )
    }
    #expect(!FileManager.default.fileExists(atPath: candidate.path))
}

@MainActor
@Test func storageBootstrapRejectsProtectedAncestorsAndChildSymlinksBeforeMutation() throws {
    let parent = try canonicalTemporaryTestDirectory("KodosiBootstrapPreflight-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: parent) }
    let production = parent.appending(path: "production/kodosi")
    let home = parent.appending(path: "user")
    #expect(throws: (any Error).self) {
        try RuntimeStorageBootstrap.validateRoot(parent.appending(path: "production").path,
                                                 productionHome: home, productionRoot: production)
    }
    #expect(!FileManager.default.fileExists(atPath: parent.path))
    let root = parent.appending(path: "isolated")
    let outside = parent.appending(path: "outside")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true,
                                            attributes: [.posixPermissions: 0o755])
    try FileManager.default.createSymbolicLink(at: root.appending(path: "harness-home"), withDestinationURL: outside)
    #expect(throws: (any Error).self) {
        try RuntimeStorageBootstrap.resolve(environment: [RuntimeStorageBootstrap.dataRootEnvironmentKey: root.path],
                                            installEnvironment: false)
    }
    #expect(!FileManager.default.fileExists(atPath: root.appending(path: "runtime").path))
    let permissions = try FileManager.default.attributesOfItem(atPath: outside.path)[.posixPermissions] as? NSNumber
    #expect(permissions?.intValue == 0o755)
}

@MainActor
@Test func hostedBootstrapCreatesUniqueContainedPaths() throws {
    let parent = URL(
        fileURLWithPath: "/private/var/tmp/KodosiBootstrapTests-\(UUID().uuidString)",
        isDirectory: true
    )
    defer { try? FileManager.default.removeItem(at: parent) }
    let environment = [
        RuntimeStorageBootstrap.xctestEnvironmentKey: "/tmp/test.xctestconfiguration",
        RuntimeStorageBootstrap.dataRootEnvironmentKey: parent.path,
    ]

    let first = try RuntimeStorageBootstrap.resolve(
        environment: environment,
        processIdentifier: 101,
        installEnvironment: false
    )
    let second = try RuntimeStorageBootstrap.resolve(
        environment: environment,
        processIdentifier: 102,
        installEnvironment: false
    )
    let firstRoot = try #require(first.dataRoot)
    let secondRoot = try #require(second.dataRoot)

    #expect(firstRoot != secondRoot)
    #expect(firstRoot.path.hasPrefix(parent.path + "/"))
    #expect(secondRoot.path.hasPrefix(parent.path + "/"))
    #expect(first.harnessHome?.path.hasPrefix(firstRoot.path + "/") == true)
    #expect(first.defaultWorkingDirectory?.path.hasPrefix(firstRoot.path + "/") == true)
    #expect(first.defaults !== UserDefaults.standard)
    #expect(first.defaults is EphemeralUserDefaults)
}

@MainActor
@Test func isolatedBootstrapReleasesPreferencesAfterItsLastConsumer() throws {
    let root = try canonicalTemporaryTestDirectory("KodosiBootstrapLifetimeTests-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: root) }
    var bootstrap: RuntimeStorageBootstrap? = try RuntimeStorageBootstrap.resolve(
        environment: [RuntimeStorageBootstrap.dataRootEnvironmentKey: root.path],
        processIdentifier: 103,
        installEnvironment: false
    )
    let file = try #require((bootstrap?.defaults as? EphemeralUserDefaults)?.persistentFileURL)
    var consumer = bootstrap?.defaults
    consumer?.set("temporary", forKey: "value")
    #expect(consumer?.synchronize() == true)
    bootstrap = nil

    #expect(consumer?.string(forKey: "value") == "temporary")
    #expect(FileManager.default.fileExists(atPath: file.path))
    consumer = nil
    #expect(!FileManager.default.fileExists(atPath: file.path))
}

@MainActor
@Test func productionBootstrapKeepsStandardPreferences() throws {
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    #expect(!bootstrap.isIsolated)
    #expect(bootstrap.defaults === UserDefaults.standard)
    #expect(!(bootstrap.defaults is EphemeralUserDefaults))
}
