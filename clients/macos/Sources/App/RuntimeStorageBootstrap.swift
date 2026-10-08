import Foundation

struct RuntimeStorageBootstrap {
    static let dataRootEnvironmentKey = "KODOSI_DATA_ROOT"
    static let productionDataRootEnvironmentKey = "KODOSI_PRODUCTION_DATA_ROOT"
    static let xctestEnvironmentKey = "XCTestConfigurationFilePath"
    static let xctestTemporaryRoot = "__KODOSI_XCTEST_TMPDIR__"
    static func resolveForLaunch(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        processIdentifier: Int32 = ProcessInfo.processInfo.processIdentifier,
        fileManager: FileManager = .default,
        installEnvironment: Bool = true
    ) throws -> RuntimeStorageBootstrap {
        try resolve(
            environment: environment,
            processIdentifier: processIdentifier,
            fileManager: fileManager,
            installEnvironment: installEnvironment
        )
    }

    let dataRoot: URL?
    let harnessHome: URL?
    let defaultWorkingDirectory: URL?
    let defaults: UserDefaults

    init(
        dataRoot: URL?,
        harnessHome: URL?,
        defaultWorkingDirectory: URL? = nil,
        defaults: UserDefaults
    ) {
        self.dataRoot = dataRoot
        self.harnessHome = harnessHome
        self.defaultWorkingDirectory = defaultWorkingDirectory
        self.defaults = defaults
    }

    var isIsolated: Bool {
        dataRoot != nil
    }

    static func resolve(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        processIdentifier: Int32 = ProcessInfo.processInfo.processIdentifier,
        fileManager: FileManager = .default,
        installEnvironment: Bool = true
    ) throws -> RuntimeStorageBootstrap {
        let isHostedTest = environment[xctestEnvironmentKey] != nil
        let productionHome = fileManager.homeDirectoryForCurrentUser.standardizedFileURL
        let productionRoot = productionHome
            .appending(path: "Library/Application Support", directoryHint: .isDirectory)
            .appending(path: "kodosi", directoryHint: .isDirectory)
            .standardizedFileURL
        guard let configured = environment[dataRootEnvironmentKey] else {
            guard !isHostedTest else {
                throw BootstrapError.missingTestDataRoot
            }
            return RuntimeStorageBootstrap(
                dataRoot: nil,
                harnessHome: nil,
                defaultWorkingDirectory: nil,
                defaults: .standard
            )
        }

        let base = try baseRoot(
            configured: configured, isHostedTest: isHostedTest, fileManager: fileManager,
            productionHome: productionHome, productionRoot: productionRoot
        )
        let root = isHostedTest
            ? base.appending(path: "run-\(processIdentifier)-\(UUID().uuidString)", directoryHint: .isDirectory)
            : base
        _ = try validateRoot(
            root.path,
            fileManager: fileManager,
            productionHome: productionHome,
            productionRoot: productionRoot
        )
        let harnessHome = root.appending(path: "harness-home", directoryHint: .isDirectory)
        let runtimeDirectory = root.appending(path: "runtime", directoryHint: .isDirectory)
        let defaultWorkingDirectory = root.appending(
            path: "workspace",
            directoryHint: .isDirectory
        )
        for directory in [root, harnessHome, runtimeDirectory, defaultWorkingDirectory] {
            _ = try validateRoot(directory.path, fileManager: fileManager,
                                 productionHome: productionHome, productionRoot: productionRoot)
        }
        for directory in [root, harnessHome, runtimeDirectory, defaultWorkingDirectory] {
            try createPrivateDirectory(directory, fileManager: fileManager)
        }

        if installEnvironment {
            for (key, directory) in [
                (dataRootEnvironmentKey, root), (productionDataRootEnvironmentKey, productionRoot),
                ("HOME", harnessHome), ("XDG_RUNTIME_DIR", runtimeDirectory),
            ] {
                guard setenv(key, directory.path, 1) == 0 else {
                    throw BootstrapError.environmentUpdateFailed(key)
                }
            }
        }

        guard let defaults = EphemeralUserDefaults(
            prefix: "com.kodosi.desktop.isolated.\(processIdentifier)"
        ) else {
            throw BootstrapError.preferencesUnavailable
        }
        return RuntimeStorageBootstrap(
            dataRoot: root,
            harnessHome: harnessHome,
            defaultWorkingDirectory: defaultWorkingDirectory,
            defaults: defaults
        )
    }

    private static func baseRoot(
        configured: String, isHostedTest: Bool, fileManager: FileManager,
        productionHome: URL, productionRoot: URL
    ) throws -> URL {
        if configured == xctestTemporaryRoot {
            guard isHostedTest else {
                throw BootstrapError.invalidDataRoot(
                    "the XCTest temporary-root sentinel is valid only for hosted tests"
                )
            }
            let temporaryDirectory = try canonicalExistingDirectory(
                fileManager.temporaryDirectory
            )
            return try validateRoot(
                temporaryDirectory
                    .appending(path: "kodosi-xctest", directoryHint: .isDirectory)
                    .path,
                fileManager: fileManager,
                productionHome: productionHome,
                productionRoot: productionRoot
            )
        } else {
            return try validateRoot(
                configured,
                fileManager: fileManager,
                productionHome: productionHome,
                productionRoot: productionRoot
            )
        }
    }

    static func validateRoot(
        _ raw: String,
        fileManager: FileManager = .default,
        productionHome: URL? = nil,
        productionRoot: URL? = nil
    ) throws -> URL {
        guard !raw.isEmpty else { throw BootstrapError.invalidDataRoot("must not be empty") }
        guard !raw.contains("$"),
              !raw.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
        else {
            throw BootstrapError.invalidDataRoot(
                "must not contain unresolved variables or control characters"
            )
        }
        guard (raw as NSString).isAbsolutePath else {
            throw BootstrapError.invalidDataRoot("must be absolute")
        }
        let candidate = URL(fileURLWithPath: raw, isDirectory: true)
        let components = raw.split(separator: "/", omittingEmptySubsequences: false)
        guard !components.contains("."), !components.contains("..") else {
            throw BootstrapError.invalidDataRoot("must not contain . or .. components")
        }
        let normalized = candidate
        guard normalized.path != "/" else {
            throw BootstrapError.invalidDataRoot("must not be the filesystem root")
        }

        let home = (productionHome ?? fileManager.homeDirectoryForCurrentUser)
            .standardizedFileURL
        guard normalized != home, !home.path.hasPrefix(normalized.path + "/") else {
            throw BootstrapError.invalidDataRoot("must not be the user home directory or an ancestor")
        }
        let productionRoot = productionRoot?.standardizedFileURL
            ?? home
            .appending(path: "Library/Application Support", directoryHint: .isDirectory)
            .appending(path: "kodosi", directoryHint: .isDirectory)
            .standardizedFileURL
        if normalized == productionRoot
            || normalized.path.hasPrefix(productionRoot.path + "/")
            || productionRoot.path.hasPrefix(normalized.path + "/")
        {
            throw BootstrapError.invalidDataRoot(
                "must not overlap the production Kodosi data directory"
            )
        }

        try rejectExistingSymlinks(in: normalized)
        if try sharesExistingIdentity(normalized, home) {
            throw BootstrapError.invalidDataRoot("must not alias the user home directory")
        }
        if try sharesExistingPrefixIdentity(normalized, productionRoot) {
            throw BootstrapError.invalidDataRoot(
                "must not alias the production Kodosi data directory or a descendant"
            )
        }
        return normalized
    }

    private static func canonicalExistingDirectory(_ directory: URL) throws -> URL {
        guard let resolved = realpath(directory.path, nil) else {
            throw BootstrapError.invalidDataRoot(
                "could not resolve the XCTest temporary directory"
            )
        }
        defer { free(resolved) }
        return URL(fileURLWithPath: String(cString: resolved), isDirectory: true)
    }

    private static func sharesExistingIdentity(_ candidate: URL, _ protected: URL) throws -> Bool {
        guard let protectedIdentity = try fileIdentity(protected) else { return false }
        return try fileIdentity(candidate) == protectedIdentity
    }

    private static func sharesExistingPrefixIdentity(
        _ candidate: URL,
        _ protectedRoot: URL
    ) throws -> Bool {
        guard let protectedIdentity = try fileIdentity(protectedRoot) else { return false }
        var current = URL(fileURLWithPath: "/", isDirectory: true)
        for component in candidate.pathComponents.dropFirst() {
            current.append(path: component)
            guard let identity = try fileIdentity(current) else { return false }
            if identity == protectedIdentity {
                return true
            }
        }
        return false
    }

    private static func fileIdentity(_ url: URL) throws -> FileIdentity? {
        var status = stat()
        let result = url.path.withCString { stat($0, &status) }
        if result == 0 {
            return FileIdentity(device: status.st_dev, inode: status.st_ino)
        }
        if errno == ENOENT {
            return nil
        }
        throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
    }

    private struct FileIdentity: Equatable {
        let device: dev_t
        let inode: ino_t
    }

    private static func rejectExistingSymlinks(in root: URL) throws {
        var current = URL(fileURLWithPath: "/", isDirectory: true)
        for component in root.pathComponents.dropFirst() {
            current.append(path: component)
            var status = stat()
            let result = current.path.withCString { lstat($0, &status) }
            if result != 0 {
                if errno == ENOENT {
                    return
                }
                throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
            }
            let type = status.st_mode & S_IFMT
            if type == S_IFLNK {
                throw BootstrapError.invalidDataRoot(
                    "must not traverse symbolic link \(current.path)"
                )
            }
            if type != S_IFDIR {
                throw BootstrapError.invalidDataRoot(
                    "existing component \(current.path) is not a directory"
                )
            }
        }
    }

    private static func createPrivateDirectory(
        _ directory: URL,
        fileManager: FileManager
    ) throws {
        try rejectExistingSymlinks(in: directory)
        if let attributes = try? fileManager.attributesOfItem(atPath: directory.path),
           let owner = attributes[.ownerAccountID] as? NSNumber,
           owner.uint32Value != geteuid()
        {
            throw BootstrapError.invalidDataRoot("must belong to the current user")
        }
        try fileManager.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try fileManager.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: directory.path
        )
    }

    enum BootstrapError: LocalizedError, Equatable {
        case missingTestDataRoot
        case invalidDataRoot(String)
        case environmentUpdateFailed(String)
        case preferencesUnavailable

        var errorDescription: String? {
            switch self {
            case .missingTestDataRoot:
                "Hosted tests require an explicit KODOSI_DATA_ROOT before app startup."
            case let .invalidDataRoot(reason):
                "Invalid KODOSI_DATA_ROOT: \(reason)."
            case let .environmentUpdateFailed(key):
                "Could not install isolated runtime environment variable \(key)."
            case .preferencesUnavailable:
                "Could not create isolated application preferences."
            }
        }
    }
}
