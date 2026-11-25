import KodosiBridge
import SwiftUI

/// Renders headless-browser dialogs (and menus on iOS) inside SwiftUI; macOS presents menus
/// natively via `NSMenu` in `BrowserInputArea`.
struct BrowserPartsLayer: View {
    @Environment(\.theme) private var theme
    let feed: BrowserFeed
    let ask: CanvasAsk?
    @State private var typed = ""
    @State private var size = CGSize.zero
    @State private var listSize = CGSize.zero

    var body: some View {
        ZStack(alignment: .topLeading) {
            #if os(iOS)
                if let menu = feed.window.menu {
                    Color.clear.contentShape(Rectangle())
                        .onTapGesture { feed.send(.choose(menu: menu.id, choice: nil)) }
                    list(menu)
                        .onGeometryChange(for: CGSize.self) { $0.size } action: { listSize = $0 }
                        .offset(place(menu))
                }
            #endif
            if let dialog = feed.window.dialog {
                card(dialog).frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .onGeometryChange(for: CGSize.self) { $0.size } action: { size = $0 }
        .animation(theme.motion.snappy, value: feed.window.menu)
        .animation(theme.motion.snappy, value: feed.window.dialog)
        .onChange(of: feed.window.dialog) { _, dialog in typed = dialog?.text ?? "" }
    }

    #if os(iOS)
        private func place(_ menu: BrowserWindow.Menu) -> CGSize {
            let margin: CGFloat = 8
            let point = feed.viewPoint(x: menu.x, y: menu.y, in: size) ?? CGPoint(x: size.width / 2, y: size.height / 2)
            let x = min(max(point.x, margin), max(margin, size.width - listSize.width - margin))
            let y = point.y + listSize.height + margin <= size.height ? point.y : max(margin, point.y - listSize.height)
            return CGSize(width: x, height: y)
        }

        private func list(_ menu: BrowserWindow.Menu) -> some View {
            ScrollView {
                VStack(alignment: .leading, spacing: 1) {
                    if let context = menu.context {
                        contextRows(menu, context)
                    } else {
                        ForEach(menu.items.enumerated(), id: \.offset) { index, item in
                            row(MenuRow(title: item.label, selected: item.chosen), enabled: item.enabled) {
                                feed.send(.choose(menu: menu.id, choice: index))
                            }
                        }
                    }
                }
                .padding(6)
            }
            .frame(width: 240).frame(maxHeight: 320).fixedSize(horizontal: false, vertical: true)
            .background(RaisedBackground(shape: RoundedRectangle(cornerRadius: Radius.md, style: .continuous), fill: theme.colors.lifted, elevation: .lifted))
            .transition(.scale(scale: 0.96, anchor: .topLeading).combined(with: .opacity))
            .accessibilityIdentifier("canvas.browser.menu")
        }

        @ContentBuilder
        private func contextRows(_ menu: BrowserWindow.Menu, _ context: BrowserWindow.MenuContext) -> some View {
            row(MenuRow(title: String(localized: "Copy", bundle: .module), symbol: "doc.on.doc"), enabled: !context.selection.isEmpty) {
                feed.send(.choose(menu: menu.id, choice: BrowserWindow.MenuContext.copySelection))
            }
            if context.link != nil {
                row(MenuRow(title: String(localized: "Copy Link", bundle: .module), symbol: "link")) {
                    feed.send(.choose(menu: menu.id, choice: BrowserWindow.MenuContext.copyLink))
                }
            }
            PasteButton(payloadType: String.self) { [feed, id = menu.id] words in
                Task {
                    feed.send(.choose(menu: id, choice: nil))
                    if let text = words.first, !text.isEmpty {
                        feed.send(.text(text))
                    }
                }
            }
            .labelStyle(.titleAndIcon)
            .buttonBorderShape(.roundedRectangle(radius: Radius.sm))
            .tint(theme.colors.accent)
            .padding(.horizontal, 4)
            .frame(minHeight: 32)
            .accessibilityIdentifier("canvas.browser.paste")
            if let ask {
                row(MenuRow(title: ask.title, symbol: "text.bubble")) {
                    feed.send(.choose(menu: menu.id, choice: nil))
                    ask.send(context.words)
                }
            }
        }

        private func row(_ label: MenuRow, enabled: Bool = true, action: @escaping () -> Void) -> some View {
            Button(action: action) { label }
                .buttonStyle(.plain)
                .disabled(!enabled)
        }
    #endif

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
