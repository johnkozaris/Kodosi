#if os(iOS)
    import SwiftUI
    import UIKit

    /// A message's text where a person selects any part of it. An iPhone selects no part of the
    /// text in the message itself, where a long press opens the message's menu.
    public struct SelectableTextSheet: View {
        @Environment(\.theme) private var theme
        @Environment(\.dismiss) private var dismiss
        let text: String

        public init(text: String) {
            self.text = text
        }

        public var body: some View {
            NavigationStack {
                SelectableText(
                    text: text, color: UIColor(theme.colors.ink), code: UIColor(theme.colors.ink.opacity(0.08)),
                    tint: UIColor(theme.colors.accent),
                )
                .padding(.horizontal, 16)
                .background(theme.colors.surface)
                .navigationTitle(Text("Select text", bundle: .module))
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        IconButton(title: "Done", bundle: .module, symbol: "xmark", identifier: "text.select.done", size: 32) {
                            dismiss()
                        }
                    }
                    .sharedBackgroundVisibility(.hidden)
                }
            }
            .presentationDetents([.medium, .large])
            .kodosiSheet()
        }
    }

    private struct SelectableText: UIViewRepresentable {
        let text: String
        let color: UIColor
        let code: UIColor
        let tint: UIColor

        final class Shown {
            var text: NSAttributedString?
        }

        func makeCoordinator() -> Shown {
            Shown()
        }

        func makeUIView(context _: Context) -> UITextView {
            let view = UITextView()
            view.isEditable = false
            view.isSelectable = true
            view.backgroundColor = .clear
            view.adjustsFontForContentSizeCategory = true
            view.dataDetectorTypes = [.link]
            view.textContainerInset = UIEdgeInsets(top: 16, left: 0, bottom: 24, right: 0)
            view.accessibilityIdentifier = "text.select.text"
            return view
        }

        func updateUIView(_ view: UITextView, context: Context) {
            let shown = SelectableProse(color: color, code: code).text(text)
            if context.coordinator.text != shown {
                context.coordinator.text = shown
                view.attributedText = shown
            }
            view.tintColor = tint
        }
    }

    /// The text as the chat shows it, with its Markdown formatted rather than written out.
    private struct SelectableProse {
        let color: UIColor
        let code: UIColor
        private var body: UIFont {
            .preferredFont(forTextStyle: .body)
        }

        func text(_ text: String) -> NSAttributedString {
            let shown = NSMutableAttributedString()
            for block in ProseBlock.blocks(of: text) {
                if shown.length > 0 {
                    shown.append(NSAttributedString(string: "\n\n", attributes: [.font: body]))
                }
                switch block {
                case let .lines(lines):
                    shown.append(inline(lines, font: body))
                case let .heading(heading):
                    shown.append(inline(heading, font: .preferredFont(forTextStyle: .headline)))
                case let .code(lines):
                    shown.append(NSAttributedString(string: lines, attributes: [.font: mono(.footnote)]))
                case let .table(table):
                    shown.append(self.table(table))
                }
            }
            shown.addAttribute(.foregroundColor, value: color, range: NSRange(location: 0, length: shown.length))
            return shown
        }

        private func inline(_ text: String, font: UIFont) -> NSAttributedString {
            let formatted = ProjectMessageText.formatted(text)
            let shown = NSMutableAttributedString()
            for run in formatted.runs {
                let intent = run.inlinePresentationIntent ?? []
                var attributes: [NSAttributedString.Key: Any] = [.font: styled(font, intent)]
                if intent.contains(.code) {
                    attributes[.backgroundColor] = code
                }
                if intent.contains(.strikethrough) {
                    attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
                }
                if let link = run.link {
                    attributes[.link] = link
                }
                shown.append(NSAttributedString(string: String(formatted[run.range].characters), attributes: attributes))
            }
            return shown
        }

        private func styled(_ font: UIFont, _ intent: InlinePresentationIntent) -> UIFont {
            if intent.contains(.code) {
                return UIFontMetrics(forTextStyle: .body)
                    .scaledFont(for: .monospacedSystemFont(ofSize: font.pointSize * 0.94, weight: .regular))
            }
            var traits = font.fontDescriptor.symbolicTraits
            if intent.contains(.stronglyEmphasized) {
                traits.insert(.traitBold)
            }
            if intent.contains(.emphasized) {
                traits.insert(.traitItalic)
            }
            guard let descriptor = font.fontDescriptor.withSymbolicTraits(traits) else { return font }
            return UIFont(descriptor: descriptor, size: 0)
        }

        private func mono(_ style: UIFont.TextStyle) -> UIFont {
            let size = UIFont.preferredFont(forTextStyle: style).pointSize
            return UIFontMetrics(forTextStyle: style).scaledFont(for: .monospacedSystemFont(ofSize: size, weight: .regular))
        }

        /// Rows of cells between tabs, so that a copied table pastes into a spreadsheet, with a tab
        /// stop after the widest cell of each column.
        private func table(_ table: ProseTable) -> NSAttributedString {
            let header = styled(body, .stronglyEmphasized)
            let rows = (table.header.isEmpty ? [] : [(table.header, header)]) + table.rows.map { ($0, body) }
            let cells = rows.map { row, font in
                (0 ..< table.columns).map { column in inline(column < row.count ? row[column] : "", font: font) }
            }
            var stops: [NSTextTab] = []
            var end: CGFloat = 0
            for column in 0 ..< max(table.columns - 1, 0) {
                end += (cells.map { ceil($0[column].size().width) }.max() ?? 0) + 18
                stops.append(NSTextTab(textAlignment: .natural, location: end))
            }
            let paragraph = NSMutableParagraphStyle()
            paragraph.tabStops = stops
            paragraph.paragraphSpacing = 7
            let shown = NSMutableAttributedString()
            for (index, row) in cells.enumerated() {
                if index > 0 {
                    shown.append(NSAttributedString(string: "\n", attributes: [.font: body]))
                }
                for (column, cell) in row.enumerated() {
                    if column > 0 {
                        shown.append(NSAttributedString(string: "\t", attributes: [.font: body]))
                    }
                    shown.append(cell)
                }
            }
            shown.addAttribute(.paragraphStyle, value: paragraph, range: NSRange(location: 0, length: shown.length))
            return shown
        }
    }
#endif
