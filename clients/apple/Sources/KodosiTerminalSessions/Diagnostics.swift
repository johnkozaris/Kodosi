import Foundation
import os

enum PerformanceSignposts {
    static let terminal = OSSignposter(subsystem: Bundle.main.bundleIdentifier ?? "com.kodosi", category: "terminal")
}
