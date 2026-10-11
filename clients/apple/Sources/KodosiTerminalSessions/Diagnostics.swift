import Foundation
import os

extension Logger {
    static let terminal = Logger(subsystem: Bundle.main.bundleIdentifier ?? "com.kodosi", category: "terminal")
}

enum PerformanceSignposts {
    static let terminal = OSSignposter(subsystem: Bundle.main.bundleIdentifier ?? "com.kodosi", category: "terminal")
}
