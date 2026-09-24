import CodyncKit
import SwiftUI

struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(Router.self) private var router

    var body: some View {
        @Bindable var router = router
        Group {
            if model.pairing == nil {
                PairingView()
            } else {
                NavigationStack(path: $router.path) {
                    BotListView()
                        .navigationDestination(for: String.self) { botId in
                            ThreadView(botId: botId)
                        }
                }
            }
        }
        .background(Palette.background)
        .alert("Something went wrong", isPresented: Binding(get: { model.lastError != nil }, set: { if !$0 { model.lastError = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(model.lastError ?? "")
        }
    }
}
