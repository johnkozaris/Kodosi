import KodosiDesign
import SwiftUI
import UIKit

struct RoomTextSheet: View {
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let text: String

    var body: some View {
        NavigationStack {
            RoomSelectableText(text: text, color: UIColor(theme.colors.ink), tint: UIColor(theme.colors.accent))
                .padding(.horizontal, 16)
                .background(theme.colors.surface)
                .navigationTitle("Select text")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { dismiss() }
                            .accessibilityIdentifier("rooms.text.done")
                    }
                }
        }
        .presentationDetents([.medium, .large])
        .presentationBackground(theme.colors.surface)
    }
}

private struct RoomSelectableText: UIViewRepresentable {
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
        view.accessibilityIdentifier = "rooms.text.selectable"
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
