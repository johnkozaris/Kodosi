import Foundation

public extension ClickedLink {
    /// Where a person clicked the link.
    enum Origin: Sendable {
        case terminal
        case canvas

        var trust: String {
            switch self {
            case .terminal:
                String(localized: "The link comes from terminal output. Open it only if you trust it.", bundle: .module)
            case .canvas:
                String(localized: "The link comes from the canvas of a terminal. Open it only if you trust it.", bundle: .module)
            }
        }
    }
}

#if os(macOS)
    import AppKit

    @MainActor
    public extension ClickedLink {
        /// Asks before a link to another app opens, or says why a link does not open, on a sheet
        /// of the window. A window shows one sheet at a time.
        func review(in window: NSWindow?, from origin: Origin) {
            switch decision {
            case .open:
                return
            case let .ask(url):
                guard let window, window.attachedSheet == nil else { return }
                guard let app = NSWorkspace.shared.urlForApplication(toOpen: url) else {
                    return refuse(because: String(localized: "No app on this Mac opens this link.", bundle: .module), in: window)
                }
                let name = app.deletingPathExtension().lastPathComponent
                let alert = alert(String(localized: "Open this link in \(name)?", bundle: .module), origin.trust)
                alert.addButton(withTitle: String(localized: "Cancel", bundle: .module))
                alert.addButton(withTitle: String(localized: "Open", bundle: .module))
                alert.beginSheetModal(for: window) { response in
                    if response == .alertSecondButtonReturn {
                        NSWorkspace.shared.open(url)
                    }
                }
            case let .refuse(refusal):
                guard let window, window.attachedSheet == nil else { return }
                refuse(because: refusal.message, in: window)
            }
        }

        private func refuse(because reason: String, in window: NSWindow) {
            let alert = alert(String(localized: "Kodosi does not open this link", bundle: .module), reason)
            alert.addButton(withTitle: String(localized: "OK", bundle: .module))
            alert.addButton(withTitle: String(localized: "Copy Link", bundle: .module))
            let shown = shown
            alert.beginSheetModal(for: window) { response in
                if response == .alertSecondButtonReturn {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(shown, forType: .string)
                }
            }
        }

        /// Alert text breaks long words with hyphens, so the link shows exactly in a box of its
        /// own.
        private func alert(_ title: String, _ text: String) -> NSAlert {
            let alert = NSAlert()
            alert.alertStyle = .warning
            alert.messageText = title
            alert.informativeText = text
            let box = NSScrollView(frame: NSRect(x: 0, y: 0, width: 320, height: 64))
            box.borderType = .bezelBorder
            box.hasVerticalScroller = true
            box.autohidesScrollers = true
            let field = NSTextView(frame: box.contentView.bounds)
            field.isEditable = false
            field.autoresizingMask = .width
            field.textContainerInset = NSSize(width: 4, height: 4)
            field.textContainer?.widthTracksTextView = true
            field.textContainer?.containerSize = NSSize(width: box.contentSize.width, height: .greatestFiniteMagnitude)
            let style = NSMutableParagraphStyle()
            style.lineBreakMode = .byCharWrapping
            field.textStorage?.setAttributedString(NSAttributedString(string: shown, attributes: [
                .font: NSFont.monospacedSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular),
                .foregroundColor: NSColor.textColor,
                .paragraphStyle: style,
            ]))
            box.documentView = field
            alert.accessoryView = box
            return alert
        }
    }

#elseif os(iOS)
    import UIKit

    @MainActor
    public extension ClickedLink {
        /// Asks before a link to another app opens, or says why a link does not open, in an
        /// alert, one at a time.
        func review(from root: UIViewController?, origin: Origin) {
            guard var presenter = root else { return }
            while let presented = presenter.presentedViewController {
                presenter = presented
            }
            guard !(presenter is UIAlertController) else { return }
            let alert: UIAlertController
            switch decision {
            case .open:
                return
            case let .ask(url):
                alert = UIAlertController(
                    title: String(localized: "Open this link in another app?", bundle: .module),
                    message: "\(shown)\n\n\(origin.trust)",
                    preferredStyle: .alert,
                )
                alert.addAction(UIAlertAction(title: String(localized: "Cancel", bundle: .module), style: .cancel))
                alert.addAction(UIAlertAction(title: String(localized: "Open", bundle: .module), style: .default) { _ in
                    UIApplication.shared.open(url)
                })
            case let .refuse(refusal):
                alert = UIAlertController(
                    title: String(localized: "Kodosi does not open this link", bundle: .module),
                    message: "\(refusal.message)\n\n\(shown)",
                    preferredStyle: .alert,
                )
                alert.addAction(UIAlertAction(title: String(localized: "OK", bundle: .module), style: .cancel))
                let shown = shown
                alert.addAction(UIAlertAction(title: String(localized: "Copy Link", bundle: .module), style: .default) { _ in
                    UIPasteboard.general.string = shown
                })
            }
            presenter.present(alert, animated: true)
        }
    }
#endif
