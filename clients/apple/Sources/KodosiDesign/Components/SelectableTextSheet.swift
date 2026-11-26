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
                SelectableText(text: text, color: UIColor(theme.colors.ink), tint: UIColor(theme.colors.accent))
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
        let tint: UIColor

        func makeUIView(context _: Context) -> UITextView {
            let view = UITextView()
            view.isEditable = false
            view.isSelectable = true
            view.backgroundColor = .clear
            view.font = .preferredFont(forTextStyle: .body)
            view.adjustsFontForContentSizeCategory = true
            view.dataDetectorTypes = [.link]
            view.textContainerInset = UIEdgeInsets(top: 16, left: 0, bottom: 24, right: 0)
            view.accessibilityIdentifier = "text.select.text"
            return view
        }

        func updateUIView(_ view: UITextView, context _: Context) {
            if view.text != text {
                view.text = text
            }
            view.textColor = color
            view.tintColor = tint
        }
    }
#endif
