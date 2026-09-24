import CodyncKit
import CodyncUI
import SwiftUI

struct RootView: View {
    @Environment(BotStore.self) private var model

    /// The open bot, as a NavigationStack path.
    private var path: Binding<[String]> {
        Binding(get: { model.selection.map { [$0] } ?? [] }, set: { model.selection = $0.last })
    }

    var body: some View {
        Group {
            if model.pairing == nil {
                PairingView()
            } else {
                NavigationStack(path: path) {
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
