import Foundation

public enum Counted {
    public static func people(_ count: Int) -> String {
        String(localized: "\(count) people", bundle: .module)
    }

    public static func terminals(_ count: Int) -> String {
        String(localized: "\(count) terminals", bundle: .module)
    }

    public static func friends(_ count: Int) -> String {
        String(localized: "\(count) friends", bundle: .module)
    }

    public static func rooms(_ count: Int) -> String {
        String(localized: "\(count) rooms", bundle: .module)
    }
}
