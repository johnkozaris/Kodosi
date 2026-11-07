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

    public static var current: String {
        Bundle.main.preferredLocalizations.first ?? "en"
    }

    public static func chosen(in defaults: UserDefaults) -> String? {
        guard let own = defaults.stringArray(forKey: key)?.first,
              own != systemLanguages.first
        else { return nil }
        return choice(for: own)?.tag
    }

    public static func choose(_ tag: String?, in defaults: UserDefaults) {
        if let tag {
            defaults.set([tag], forKey: key)
        } else {
            defaults.removeObject(forKey: key)
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
