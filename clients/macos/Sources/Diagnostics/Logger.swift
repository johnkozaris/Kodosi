import os

extension Logger {
    static let app = Logger(subsystem: "com.kodosi.desktop", category: "app")
    static let runtime = Logger(subsystem: "com.kodosi.desktop", category: "runtime")
    static let bridge = Logger(subsystem: "com.kodosi.desktop", category: "bridge")
    static let performance = Logger(subsystem: "com.kodosi.desktop", category: "performance")
}
