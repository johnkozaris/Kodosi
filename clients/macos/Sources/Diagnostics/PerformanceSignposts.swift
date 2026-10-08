import os

enum PerformanceSignposts {
    static let terminal = OSSignposter(subsystem: subsystem, category: "terminal")

    private static let subsystem = "com.kodosi.desktop"
}
