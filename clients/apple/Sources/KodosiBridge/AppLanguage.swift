import Foundation

public enum AppLanguage {
    public struct Choice: Identifiable, Hashable, Sendable {
        public let tag: String
        public let name: String
        public var id: String {
            tag
        }
    }

    public static let choices: [Choice] = [
        Choice(tag: "en", name: "English"),
        Choice(tag: "de", name: "Deutsch"),
        Choice(tag: "el", name: "Ελληνικά"),
        Choice(tag: "es", name: "Español"),
        Choice(tag: "fr", name: "Français"),
        Choice(tag: "it", name: "Italiano"),
        Choice(tag: "ja", name: "日本語"),
        Choice(tag: "ko", name: "한국어"),
        Choice(tag: "zh-Hans", name: "简体中文"),
    ]

    private static let key = "AppleLanguages"
    private static let choiceKey = "app.language"

    public static var current: String {
        Bundle.main.preferredLocalizations.first ?? "en"
    }

    public static func chosen(in defaults: UserDefaults) -> String? {
        defaults.string(forKey: choiceKey).flatMap { choice(for: $0)?.tag }
    }

    public static func choose(_ tag: String?, in defaults: UserDefaults) {
        if let tag {
            defaults.set([tag], forKey: key)
            defaults.set(tag, forKey: choiceKey)
        } else {
            defaults.removeObject(forKey: key)
            defaults.removeObject(forKey: choiceKey)
        }
    }

    public static var systemLanguage: String {
        Bundle.preferredLocalizations(from: Bundle.main.localizations, forPreferences: systemLanguages).first ?? "en"
    }

    public static func name(of tag: String) -> String {
        choice(for: tag)?.name ?? tag
    }

    private static var systemLanguages: [String] {
        CFPreferencesCopyValue(key as CFString, kCFPreferencesAnyApplication, kCFPreferencesCurrentUser, kCFPreferencesAnyHost)
            as? [String] ?? []
    }

    private static func choice(for tag: String) -> Choice? {
        choices.first { tag == $0.tag || tag.hasPrefix("\($0.tag)-") }
    }
}
