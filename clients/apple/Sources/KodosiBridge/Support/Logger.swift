import Foundation
import os

extension Logger {
    private static let subsystem = Bundle.main.bundleIdentifier ?? "com.kodosi"

    static let runtime = Logger(subsystem: subsystem, category: "runtime")
    static let terminal = Logger(subsystem: subsystem, category: "terminal")
}
