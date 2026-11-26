import KodosiBridge
import SwiftUI

struct BrowserInputArea {
    let feed: BrowserFeed
    let typing: Bool
    let ask: CanvasAsk?
}

#if os(macOS)
    import AppKit

    extension BrowserInputArea: NSViewRepresentable {
        func makeNSView(context _: Context) -> BrowserInputView {
            BrowserInputView(feed: feed)
        }

        func updateNSView(_ view: BrowserInputView, context _: Context) {
            view.feed = feed
            view.ask = ask
            view.show(shape: feed.window.shape)
            view.show(menu: feed.window.menu)
        }
    }

    final class BrowserInputView: NSView {
        var feed: BrowserFeed
        var ask: CanvasAsk?
        private var shape = ""
        private var pageMenu: BrowserWindow.Menu?
        private var shownMenu: NSMenu?

        func show(shape: String) {
            if shape != self.shape {
                self.shape = shape
                window?.invalidateCursorRects(for: self)
            }
        }

        func show(menu: BrowserWindow.Menu?) {
            guard menu != pageMenu else { return }
            pageMenu = menu
            if let shownMenu {
                shownMenu.cancelTracking()
            } else if let menu {
                Task { [weak self] in self?.pop(menu) }
            }
        }

        /// `<select>` menus open on mouse-down, so `NSMenu.popUp` swallows the matching mouse-up.
        /// Synthesize releases for any held buttons first so the page does not get stuck pressed.
        private func pop(_ menu: BrowserWindow.Menu) {
            guard menu == pageMenu, shownMenu == nil, window != nil,
                  let place = feed.viewPoint(x: menu.x, y: menu.y, in: bounds.size) else { return }
            let held = NSEvent.pressedMouseButtons
            for (bit, button) in [(0, ViewerInput.MouseButton.left), (1, .right), (2, .middle)] where held & 1 << bit != 0 {
                feed.send(.mouse(.mouseReleased, x: menu.x, y: menu.y, button: button, clicks: 1))
            }
            let native = menu.context.map { context in contextMenu(menu, context) } ?? list(menu)
            shownMenu = native
            let chose = native.popUp(positioning: native.items.first { $0.state == .on }, at: place, in: self)
            shownMenu = nil
            if menu == pageMenu {
                if !chose {
                    feed.send(.choose(menu: menu.id, choice: nil))
                }
            } else if let next = pageMenu {
                Task { [weak self] in self?.pop(next) }
            }
        }

        private func list(_ menu: BrowserWindow.Menu) -> NSMenu {
            let native = NSMenu()
            native.autoenablesItems = false
            for (index, item) in menu.items.enumerated() {
                let entry = MenuAction.item(item.label, enabled: item.enabled) { [feed] in
                    feed.send(.choose(menu: menu.id, choice: index))
                }
                entry.state = item.chosen ? .on : .off
                native.addItem(entry)
            }
            return native
        }

        private func contextMenu(_ menu: BrowserWindow.Menu, _ context: BrowserWindow.MenuContext) -> NSMenu {
            let native = NSMenu()
            native.autoenablesItems = false
            let feed = feed
            native.addItem(MenuAction.item(String(localized: "Copy", bundle: .module), enabled: !context.selection.isEmpty) {
                feed.send(.choose(menu: menu.id, choice: BrowserWindow.MenuContext.copySelection))
            })
            if context.link != nil {
                native.addItem(MenuAction.item(String(localized: "Copy Link", bundle: .module)) {
                    feed.send(.choose(menu: menu.id, choice: BrowserWindow.MenuContext.copyLink))
                })
            }
            let paste = NSMenuItem(title: String(localized: "Paste", bundle: .module), action: #selector(paste(_:)), keyEquivalent: "")
            paste.target = self
            paste.representedObject = menu.id
            paste.isEnabled = NSPasteboard.general.availableType(from: [.string]) != nil
            native.addItem(paste)
            if let ask {
                native.addItem(.separator())
                native.addItem(MenuAction.item(ask.title) {
                    feed.send(.choose(menu: menu.id, choice: nil))
                    ask.send(context.words)
                })
            }
            return native
        }

        init(feed: BrowserFeed) {
            self.feed = feed
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        override var isFlipped: Bool {
            true
        }

        override var acceptsFirstResponder: Bool {
            true
        }

        override func acceptsFirstMouse(for _: NSEvent?) -> Bool {
            true
        }

        private static let cursors: [String: NSCursor] = [
            "pointer": .pointingHand,
            "text": .iBeam,
            "vertical-text": .iBeamCursorForVerticalLayout,
            "crosshair": .crosshair,
            "cell": .crosshair,
            "context-menu": .contextualMenu,
            "alias": .dragLink,
            "copy": .dragCopy,
            "grab": .openHand,
            "grabbing": .closedHand,
            "not-allowed": .operationNotAllowed,
            "col-resize": .columnResize,
            "ew-resize": .columnResize,
            "w-resize": .columnResize(directions: .left),
            "e-resize": .columnResize(directions: .right),
            "row-resize": .rowResize,
            "ns-resize": .rowResize,
            "n-resize": .rowResize(directions: .up),
            "s-resize": .rowResize(directions: .down),
            "ne-resize": .frameResize(position: .topRight, directions: .all),
            "nesw-resize": .frameResize(position: .topRight, directions: .all),
            "nw-resize": .frameResize(position: .topLeft, directions: .all),
            "nwse-resize": .frameResize(position: .topLeft, directions: .all),
            "se-resize": .frameResize(position: .bottomRight, directions: .all),
            "sw-resize": .frameResize(position: .bottomLeft, directions: .all),
            "zoom-in": .zoomIn,
            "zoom-out": .zoomOut,
        ]

        override func resetCursorRects() {
            addCursorRect(bounds, cursor: Self.cursors[feed.window.shape] ?? .arrow)
        }

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            for area in trackingAreas {
                removeTrackingArea(area)
            }
            addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self))
        }

        override func mouseDown(with event: NSEvent) {
            window?.makeFirstResponder(self)
            mouse(.mousePressed, event, button: .left)
        }

        override func mouseUp(with event: NSEvent) {
            mouse(.mouseReleased, event, button: .left)
        }

        override func rightMouseDown(with event: NSEvent) {
            window?.makeFirstResponder(self)
            mouse(.mousePressed, event, button: .right)
        }

        override func rightMouseUp(with event: NSEvent) {
            mouse(.mouseReleased, event, button: .right)
        }

        override func otherMouseDown(with event: NSEvent) {
            mouse(.mousePressed, event, button: .middle)
        }

        override func otherMouseUp(with event: NSEvent) {
            mouse(.mouseReleased, event, button: .middle)
        }

        override func mouseDragged(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .left)
        }

        override func rightMouseDragged(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .right)
        }

        override func otherMouseDragged(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .middle)
        }

        override func mouseMoved(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .none)
        }

        override func scrollWheel(with event: NSEvent) {
            guard let point = feed.browserPoint(convert(event.locationInWindow, from: nil), in: bounds.size) else { return }
            let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 40
            feed.send(.mouse(.mouseWheel, x: point.x, y: point.y, deltaX: Double(-event.scrollingDeltaX * scale),
                             deltaY: Double(-event.scrollingDeltaY * scale), modifiers: ViewerInput.Modifiers(event.modifierFlags)))
        }

        override func keyDown(with event: NSEvent) {
            key(event, down: true)
        }

        override func keyUp(with event: NSEvent) {
            key(event, down: false)
        }

        /// macOS skips the pasteboard permission prompt only when reading inside a `paste:`
        /// action; the context menu therefore checks `availableType` only and targets this method.
        @objc func paste(_ sender: Any?) {
            if let menu = (sender as? NSMenuItem)?.representedObject as? Int {
                feed.send(.choose(menu: menu, choice: nil))
            }
            if let text = NSPasteboard.general.string(forType: .string), !text.isEmpty {
                feed.send(.text(text))
            }
        }

        @objc func copy(_: Any?) {
            command(keyCode: 8, "c")
        }

        @objc func cut(_: Any?) {
            command(keyCode: 7, "x")
        }

        override func selectAll(_: Any?) {
            command(keyCode: 0, "a")
        }

        private func command(keyCode: UInt16, _ letter: String) {
            guard let key = ViewerKeys.key(macKeyCode: keyCode) else { return }
            feed.press(key, typed: letter, down: true, modifiers: .meta)
            feed.press(key, typed: letter, down: false, modifiers: .meta)
        }

        private func mouse(_ type: ViewerInput.MouseEvent, _ event: NSEvent, button: ViewerInput.MouseButton) {
            guard let point = feed.browserPoint(convert(event.locationInWindow, from: nil), in: bounds.size) else { return }
            let clicks = type == .mouseMoved ? 0 : event.clickCount
            feed.send(.mouse(type, x: point.x, y: point.y, button: button, clicks: clicks, modifiers: ViewerInput.Modifiers(event.modifierFlags)))
        }

        private func key(_ event: NSEvent, down: Bool) {
            guard let key = ViewerKeys.key(macKeyCode: event.keyCode) else { return }
            let held = ViewerInput.Modifiers(event.modifierFlags)
            let typed = held.isDisjoint(with: [.control, .meta]) ? event.characters : event.charactersIgnoringModifiers
            feed.press(key, typed: typed?.isEmpty == false ? typed : nil, down: down, modifiers: held)
        }
    }

    @MainActor
    private final class MenuAction: NSObject {
        private let run: () -> Void

        private init(_ run: @escaping () -> Void) {
            self.run = run
        }

        @objc private func choose() {
            run()
        }

        /// Retains the action via `representedObject` because `NSMenuItem.target` is weak.
        static func item(_ title: String, enabled: Bool = true, run: @escaping () -> Void) -> NSMenuItem {
            let action = MenuAction(run)
            let item = NSMenuItem(title: title, action: #selector(choose), keyEquivalent: "")
            item.target = action
            item.representedObject = action
            item.isEnabled = enabled
            return item
        }
    }

#else
    extension BrowserInputArea: UIViewRepresentable {
        func makeUIView(context _: Context) -> TouchInputView {
            TouchInputView(target: feed)
        }

        func updateUIView(_ view: TouchInputView, context _: Context) {
            view.target = feed
            view.wantsKeyboard(typing)
        }
    }
#endif
