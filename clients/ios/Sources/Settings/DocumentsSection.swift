import KodosiDesign
import SwiftUI

/// The live viewers of this device: which ones show their files on the canvas, and how.
struct DocumentsSection: View {
    @Environment(\.theme) private var theme
    @AppStorage(DocumentViewers.storageKey) private var viewers = DocumentViewers()

    var body: some View {
        Section {
            choice("Document viewer", ".docx", symbol: "doc.text", tint: TileTint.blue, isOn: $viewers.documents)
                .accessibilityIdentifier("settings.documents.document")
            if viewers.documents {
                choice("Pages", "When off, the text fits the screen", symbol: "doc.on.doc", tint: TileTint.teal,
                       isOn: $viewers.documentPages)
                    .accessibilityIdentifier("settings.documents.document.pages")
                choice("Dark pages", "When Kodosi is dark", symbol: "moon", tint: TileTint.graphite,
                       isOn: $viewers.darkDocuments)
                    .accessibilityIdentifier("settings.documents.document.dark")
            }
            choice("Sheet viewer", ".xlsx", symbol: "tablecells", tint: TileTint.green, isOn: $viewers.sheets)
                .accessibilityIdentifier("settings.documents.sheet")
            if viewers.sheets {
                choice("Dark sheets", "When Kodosi is dark", symbol: "moon", tint: TileTint.graphite,
                       isOn: $viewers.darkSheets)
                    .accessibilityIdentifier("settings.documents.sheet.dark")
            }
            choice("Presentation viewer", ".pptx", symbol: "play.rectangle", tint: TileTint.amber,
                   isOn: $viewers.presentations)
                .accessibilityIdentifier("settings.documents.presentation")
            choice("PDF viewer", ".pdf", symbol: "doc.richtext", tint: TileTint.red, isOn: $viewers.pdfs)
                .accessibilityIdentifier("settings.documents.pdf")
            if viewers.pdfs {
                choice("Dark pages", "When Kodosi is dark", symbol: "moon", tint: TileTint.graphite,
                       isOn: $viewers.darkPdfs)
                    .accessibilityIdentifier("settings.documents.pdf.dark")
            }
        } header: {
            GroupedListHeader(title: "Documents")
        } footer: {
            Text("When a viewer is off, the canvas shows a preview of its files.")
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkFaint)
        }
        .animation(theme.motion.spring, value: viewers)
    }

    private func choice(_ title: LocalizedStringKey, _ subtitle: LocalizedStringKey, symbol: String, tint: Color,
                        isOn: Binding<Bool>) -> some View
    {
        SettingRow(symbol: symbol, tint: tint, title: Text(title), subtitle: Text(subtitle)) {
            Toggle(isOn: isOn) { Text(title) }
                .labelsHidden()
        }
        .raisedRows()
    }
}
