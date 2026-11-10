import KodosiBridge
import KodosiDesign
import SwiftUI

@main
struct KodosiMobileApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        BarAppearance.apply()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .environment(model.notices)
                .buttonStyle(.kodosi(.secondary))
                .textFieldStyle(WellTextFieldStyle())
                .toggleStyle(KodosiSwitchStyle())
                .kodosiTheme()
                .task {
                    AppDelegate.push = model.push
                    model.start()
                }
                .onChange(of: model.account.isReady, initial: true) { _, ready in model.push.accountChanged(ready: ready) }
                .onOpenURL { url in
                    if url.scheme == "com.kodosi" {
                        model.account.finishSignIn(with: url)
                    }
                }
        }
        .onChange(of: scenePhase) { _, phase in
            model.sceneChanged(to: phase)
        }
    }
}
