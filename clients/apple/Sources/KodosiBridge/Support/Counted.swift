import Foundation

public enum Counted {
    public static func people(_ count: Int) -> String {
        count == 1 ? String(localized: "1 person", bundle: .module) : String(localized: "\(count) people", bundle: .module)
    }

    public static func terminals(_ count: Int) -> String {
        count == 1 ? String(localized: "1 terminal", bundle: .module) : String(localized: "\(count) terminals", bundle: .module)
    }

    public static func friends(_ count: Int) -> String {
        count == 1 ? String(localized: "1 friend", bundle: .module) : String(localized: "\(count) friends", bundle: .module)
    }

    public static func rooms(_ count: Int) -> String {
        count == 1 ? String(localized: "1 room", bundle: .module) : String(localized: "\(count) rooms", bundle: .module)
    }

    public static func computers(_ count: Int) -> String {
        count == 1 ? String(localized: "1 computer", bundle: .module) : String(localized: "\(count) computers", bundle: .module)
    }
}
