#if os(macOS)
    import AppKit

    extension TerminalRendererHostedView {
        func review(_ link: TerminalLink) {
            guard !linkAlertOpen, let window else { return }
            switch link.decision {
            case .open:
                return
            case let .ask(url):
                guard let app = NSWorkspace.shared.urlForApplication(toOpen: url) else {
                    return refuse(link, because: String(localized: "No app on this Mac opens this link."), in: window)
                }
                let name = app.deletingPathExtension().lastPathComponent
                let alert = linkAlert(
                    String(localized: "Open this link in \(name)?"),
                    String(localized: "The link comes from terminal output. Open it only if you trust it."),
                    link,
                )
                alert.addButton(withTitle: String(localized: "Cancel"))
                alert.addButton(withTitle: String(localized: "Open"))
                linkAlertOpen = true
                alert.beginSheetModal(for: window) { [weak self] response in
                    self?.linkAlertOpen = false
                    if response == .alertSecondButtonReturn {
                        NSWorkspace.shared.open(url)
                    }
                }
            case let .refuse(refusal):
                refuse(link, because: refusal.message, in: window)
            }
        }

        private func refuse(_ link: TerminalLink, because reason: String, in window: NSWindow) {
            let alert = linkAlert(String(localized: "Kodosi does not open this link"), reason, link)
            alert.addButton(withTitle: String(localized: "OK"))
            alert.addButton(withTitle: String(localized: "Copy Link"))
            linkAlertOpen = true
            alert.beginSheetModal(for: window) { [weak self] response in
                self?.linkAlertOpen = false
                if response == .alertSecondButtonReturn {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(link.shown, forType: .string)
                }
            }
        }

        /// Alert text breaks long words with hyphens, so the link shows exactly in a
        /// box of its own.
        private func linkAlert(_ title: String, _ text: String, _ link: TerminalLink) -> NSAlert {
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
            field.textStorage?.setAttributedString(NSAttributedString(string: link.shown, attributes: [
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

    extension TerminalRendererHostedView {
        func review(_ link: TerminalLink) {
            guard !linkAlertOpen, var presenter = window?.rootViewController else { return }
            while let presented = presenter.presentedViewController {
                presenter = presented
            }
            let alert: UIAlertController
            switch link.decision {
            case .open:
                return
            case let .ask(url):
                alert = UIAlertController(
                    title: String(localized: "Open this link in another app?"),
                    message: String(localized: "\(link.shown)\n\nThe link comes from terminal output. Open it only if you trust it."),
                    preferredStyle: .alert,
                )
                alert.addAction(UIAlertAction(title: String(localized: "Cancel"), style: .cancel) { [weak self] _ in
                    self?.linkAlertOpen = false
                })
                alert.addAction(UIAlertAction(title: String(localized: "Open"), style: .default) { [weak self] _ in
                    self?.linkAlertOpen = false
                    UIApplication.shared.open(url)
                })
            case let .refuse(refusal):
                alert = UIAlertController(
                    title: String(localized: "Kodosi does not open this link"),
                    message: "\(refusal.message)\n\n\(link.shown)",
                    preferredStyle: .alert,
                )
                alert.addAction(UIAlertAction(title: String(localized: "OK"), style: .cancel) { [weak self] _ in
                    self?.linkAlertOpen = false
                })
                alert.addAction(UIAlertAction(title: String(localized: "Copy Link"), style: .default) { [weak self] _ in
                    self?.linkAlertOpen = false
                    UIPasteboard.general.string = link.shown
                })
            }
            linkAlertOpen = true
            presenter.present(alert, animated: true)
        }
    }
#endif
