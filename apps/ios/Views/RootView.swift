import CodyncKit
import CodyncUI
import SwiftUI

enum AppTab: Hashable { case bots, usage }

struct RootView: View {
    @Environment(BotStore.self) private var model
    @Binding var tab: AppTab

    /// The open bot, as a NavigationStack path.
    private var path: Binding<[String]> {
        Binding(get: { model.selection.map { [$0] } ?? [] }, set: { model.selection = $0.last })
    }

    var body: some View {
        Group {
            if model.pairing == nil {
                PairingView()
            } else {
                TabView(selection: $tab) {
                    Tab("Bots", systemImage: "bubble.left.and.bubble.right.fill", value: .bots) {
                        NavigationStack(path: path) {
                            BotListView()
                                .navigationDestination(for: String.self) { botId in
                                    ThreadView(botId: botId)
                                        .toolbar(.hidden, for: .tabBar)
                                }
                        }
                    }
                    Tab("Usage", systemImage: "chart.bar.fill", value: .usage) {
                        NavigationStack { UsageView() }
                    }
                }
                // Opening a bot (notification, widget, link) always lands on the Bots tab.
                .onChange(of: model.selection) { _, id in if id != nil { tab = .bots } }
            }
        }
        .background(Palette.background)
        .storeErrorAlert(model)
        .fullScreenCover(item: Bindable(model).screenRequest) { request in
            ScreenView(watching: request.watching)
                .environment(model)
        }
    }
}
