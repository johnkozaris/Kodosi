import KodosiBridge
import SwiftUI

/// What a page asks of a person outside its picture: the list of a select or a right-click, and
/// its alert, confirm and prompt. The app draws them, because the browser never reaches the
/// screen.
struct BrowserPartsLayer: View {
    @Environment(\.theme) private var theme
    let feed: BrowserFeed
    @State private var typed = ""

    var body: some View {
        ZStack {
            if let menu = feed.window.menu {
                Color.clear.contentShape(Rectangle())
                    .onTapGesture { feed.send(.choose(menu: menu.id, choice: nil)) }
                list(menu)
            }
            if let dialog = feed.window.dialog {
                card(dialog)
            }
        }
        .animation(theme.motion.snappy, value: feed.window.menu)
        .animation(theme.motion.snappy, value: feed.window.dialog)
        .onChange(of: feed.window.dialog) { _, dialog in typed = dialog?.text ?? "" }
    }

    private func list(_ menu: BrowserWindow.Menu) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 1) {
                ForEach(Array(menu.items.enumerated()), id: \.offset) { index, item in
                    Button {
                        feed.send(.choose(menu: menu.id, choice: index))
                    } label: {
                        MenuRow(title: item.label, selected: item.chosen)
                    }
                    .buttonStyle(.plain)
                    .disabled(!item.enabled)
                }
            }
            .padding(6)
        }
        .frame(width: 240).frame(maxHeight: 320).fixedSize(horizontal: false, vertical: true)
        .background(RaisedBackground(shape: RoundedRectangle(cornerRadius: Radius.md, style: .continuous), fill: theme.colors.lifted, elevation: .lifted))
        .transition(.scale(scale: 0.96).combined(with: .opacity))
        .accessibilityIdentifier("canvas.browser.menu")
    }

    private func card(_ dialog: BrowserWindow.Dialog) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(verbatim: dialog.message).appTextStyle(.body).foregroundStyle(theme.colors.ink)
                .lineLimit(8).fixedSize(horizontal: false, vertical: true)
            if dialog.kind == .prompt {
                TextField(String(localized: "Answer", bundle: .module), text: $typed)
                    .textFieldStyle(.plain).appTextStyle(.body).padding(.horizontal, 10).frame(height: 30).wellCapsule()
            }
            HStack(spacing: 8) {
                Spacer()
                if dialog.kind != .alert {
                    Button { feed.send(.dialog(accept: false, text: nil)) } label: { Text("Cancel", bundle: .module) }
                        .buttonStyle(.kodosi(.secondary))
                }
                Button {
                    feed.send(.dialog(accept: true, text: dialog.kind == .prompt ? typed : nil))
                } label: {
                    Text("OK", bundle: .module)
                }
                .buttonStyle(.kodosi(.primary))
            }
        }
        .padding(16).frame(width: 340)
        .background(RaisedBackground(shape: RoundedRectangle(cornerRadius: Radius.md, style: .continuous), fill: theme.colors.lifted, elevation: .lifted))
        .transition(.scale(scale: 0.96).combined(with: .opacity))
        .accessibilityIdentifier("canvas.browser.dialog")
    }
}

enum Pasteboard {
    static func copy(_ words: String) {
        #if os(macOS)
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(words, forType: .string)
        #else
            UIPasteboard.general.string = words
        #endif
    }
}
