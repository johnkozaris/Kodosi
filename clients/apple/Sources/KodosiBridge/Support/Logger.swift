import Foundation
import os

public extension Logger {
    private static let subsystem = Bundle.main.bundleIdentifier ?? "com.kodosi"

    static let app = Logger(subsystem: subsystem, category: "app")
    static let runtime = Logger(subsystem: subsystem, category: "runtime")
    static let terminal = Logger(subsystem: subsystem, category: "terminal")
}
