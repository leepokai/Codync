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
        .storeErrorAlert(model)
    }
}
