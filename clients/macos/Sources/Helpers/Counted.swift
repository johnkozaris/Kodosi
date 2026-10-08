import Foundation

enum Counted {
    static func people(_ count: Int) -> String {
        count == 1 ? String(localized: "1 person") : String(localized: "\(count) people")
    }

    static func terminals(_ count: Int) -> String {
        count == 1 ? String(localized: "1 terminal") : String(localized: "\(count) terminals")
    }

    static func friends(_ count: Int) -> String {
        count == 1 ? String(localized: "1 friend") : String(localized: "\(count) friends")
    }

    static func rooms(_ count: Int) -> String {
        count == 1 ? String(localized: "1 room") : String(localized: "\(count) rooms")
    }

    static func computers(_ count: Int) -> String {
        count == 1 ? String(localized: "1 computer") : String(localized: "\(count) computers")
    }
}
