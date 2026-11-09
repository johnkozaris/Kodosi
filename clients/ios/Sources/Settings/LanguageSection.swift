import KodosiBridge
import KodosiDesign
import SwiftUI

struct LanguageSection: View {
    @State private var language = AppLanguage.chosen(in: .standard)

    var body: some View {
        ListGroup(footer: footer) {
            ListRow(String(localized: "Language"), symbol: "globe", tint: TileTint.teal) {
                MenuPicker(
                    title: "Language",
                    selection: $language,
                    values: [nil] + AppLanguage.choices.map(\.tag),
                    label: { tag in tag.map(AppLanguage.name(of:)) ?? String(localized: "Same as this device") },
                    identifier: "settings.language",
                )
            }
        }
        .onChange(of: language) { _, language in AppLanguage.choose(language, in: .standard) }
    }

    private var footer: LocalizedStringKey {
        let next = language ?? AppLanguage.systemLanguage
        guard next != AppLanguage.current else {
            return "Kodosi shows the language of this device, or the one that you choose here."
        }
        return "Close Kodosi in the app switcher and open it again to show \(AppLanguage.name(of: next))."
    }
}
