import KodosiBridge
import KodosiDesign
import SwiftUI

/// The language of the app. iOS sets it when the app opens, so a new choice applies after that.
struct LanguageSection: View {
    @Environment(\.theme) private var theme
    @State private var language = AppLanguage.chosen(in: .standard)

    var body: some View {
        Section {
            SettingRow(symbol: "globe", tint: TileTint.teal, title: Text("Language")) {
                Picker("Language", selection: $language) {
                    Text("Same as this device").tag(String?.none)
                    ForEach(AppLanguage.choices) { choice in
                        Text(verbatim: choice.name).tag(Optional(choice.tag))
                    }
                }
                .labelsHidden()
                .accessibilityIdentifier("settings.language")
            }
            .raisedRows()
        } header: {
            GroupedListHeader(title: "Language")
        } footer: {
            footer
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkFaint)
        }
        .onChange(of: language) { _, language in AppLanguage.choose(language, in: .standard) }
    }

    private var footer: Text {
        let next = language ?? AppLanguage.systemLanguage
        guard next != AppLanguage.current else {
            return Text("Kodosi shows the language of this device, or the one that you choose here.")
        }
        return Text("Close Kodosi in the app switcher and open it again to show \(AppLanguage.name(of: next)).")
    }
}
