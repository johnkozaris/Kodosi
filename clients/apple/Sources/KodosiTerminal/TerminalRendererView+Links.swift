import KodosiLinks

#if os(macOS)
    import AppKit

    extension TerminalRendererHostedView {
        func review(_ link: ClickedLink) {
            link.review(in: window, from: .terminal)
        }
    }

#elseif os(iOS)
    import UIKit

    extension TerminalRendererHostedView {
        func review(_ link: ClickedLink) {
            link.review(from: window?.rootViewController, origin: .terminal)
        }
    }
#endif
