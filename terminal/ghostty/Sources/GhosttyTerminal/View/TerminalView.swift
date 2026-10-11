#if os(macOS)
import AppKit

public typealias TerminalView = AppTerminalView
#elseif os(iOS)
import UIKit

public typealias TerminalView = UITerminalView
#endif
