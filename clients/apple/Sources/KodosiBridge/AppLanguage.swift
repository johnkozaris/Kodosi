import Foundation

/// The languages of Kodosi. The app shows the language of the system, or the one that Settings
/// chose. The system sets the language of an app when it opens, so a new choice applies the next
/// time that Kodosi opens.
public enum AppLanguage {
    public struct Choice: Identifiable, Hashable, Sendable {
        public let tag: String
        /// The name of the language in the language itself.
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

    /// The language of this run of the app.
    public static var current: String {
        Bundle.main.preferredLocalizations.first ?? "en"
    }

    /// The language that Settings chose, or nil when the app follows the system. The system's
    /// own setting for the app writes the same value, so both places agree.
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

    /// The language that the app shows when it follows the system.
    public static var systemLanguage: String {
        Bundle.preferredLocalizations(from: Bundle.main.localizations, forPreferences: systemLanguages).first ?? "en"
    }

    public static func name(of tag: String) -> String {
        choice(for: tag)?.name ?? tag
    }

    private static var systemLanguages: [String] {
        UserDefaults(suiteName: UserDefaults.globalDomain)?.stringArray(forKey: key) ?? []
    }

    private static func choice(for tag: String) -> Choice? {
        choices.first { tag == $0.tag || tag.hasPrefix("\($0.tag)-") }
    }
}
