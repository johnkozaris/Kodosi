import AppKit
import KodosiKit
import SwiftUI

let cliStatus = UnsafeRawPointer(CommandLine.unsafeArgv).withMemoryRebound(
    to: UnsafePointer<CChar>?.self, capacity: Int(CommandLine.argc)
) { arguments in
    kodosi_cli_main(CommandLine.argc, arguments)
}

if cliStatus != KODOSI_CLI_NOT_INVOKED {
    exit(cliStatus)
}

KodosiDesktopApp.main()
