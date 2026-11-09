import KodosiDesign
import SwiftUI

/// The live viewers of this device: which ones show their files on the canvas, and how.
struct DocumentsSection: View {
    @Environment(\.theme) private var theme
    @AppStorage(DocumentViewers.storageKey) private var viewers = DocumentViewers()

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            ListGroup(title: "Documents") {
                choice("Document viewer", ".docx", symbol: "doc.text", tint: TileTint.blue, isOn: $viewers.documents)
                    .accessibilityIdentifier("settings.documents.document")
                if viewers.documents {
                    choice("Pages", "When off, the text fits the screen", symbol: "doc.on.doc", tint: TileTint.teal,
                           isOn: $viewers.documentPages)
                        .accessibilityIdentifier("settings.documents.document.pages")
                    choice("Dark pages", "When Kodosi is dark", symbol: "moon", tint: TileTint.graphite, isOn: $viewers.darkDocuments)
                        .accessibilityIdentifier("settings.documents.document.dark")
                }
            }
            ListGroup {
                choice("Sheet viewer", ".xlsx", symbol: "tablecells", tint: TileTint.green, isOn: $viewers.sheets)
                    .accessibilityIdentifier("settings.documents.sheet")
                if viewers.sheets {
                    choice("Dark sheets", "When Kodosi is dark", symbol: "moon", tint: TileTint.graphite, isOn: $viewers.darkSheets)
                        .accessibilityIdentifier("settings.documents.sheet.dark")
                }
            }
            ListGroup {
                choice("Presentation viewer", ".pptx", symbol: "play.rectangle", tint: TileTint.amber, isOn: $viewers.presentations)
                    .accessibilityIdentifier("settings.documents.presentation")
            }
            ListGroup {
                choice("Text viewer", ".txt, .json, .yaml, .toml, .xml", symbol: "text.alignleft", tint: TileTint.graphite,
                       isOn: $viewers.texts)
                    .accessibilityIdentifier("settings.documents.text")
            }
            ListGroup(footer: "When a viewer is off, the canvas shows a preview of its files.") {
                choice("PDF viewer", ".pdf", symbol: "doc.richtext", tint: TileTint.red, isOn: $viewers.pdfs)
                    .accessibilityIdentifier("settings.documents.pdf")
                if viewers.pdfs {
                    choice("Dark pages", "When Kodosi is dark", symbol: "moon", tint: TileTint.graphite, isOn: $viewers.darkPdfs)
                        .accessibilityIdentifier("settings.documents.pdf.dark")
                }
            }
        }
        .animation(theme.motion.spring, value: viewers)
    }

    private func choice(_ title: String.LocalizationValue, _ subtitle: String.LocalizationValue, symbol: String, tint: Color,
                        isOn: Binding<Bool>) -> some View
    {
        SwitchRow(String(localized: title), subtitle: String(localized: subtitle), symbol: symbol, tint: tint, isOn: isOn)
            .transition(AnyTransition.rise)
    }
}
