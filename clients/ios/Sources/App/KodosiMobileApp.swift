import KodosiDesign
import SwiftUI

@main
struct KodosiMobileApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .environment(model.notices)
                .kodosiTheme()
                .task { model.start() }
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
