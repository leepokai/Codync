import CodyncKit
import SwiftUI

/// A small live picture of the computer's screen, for the bot details' Computer page. View only: it never
/// takes over or sends input. It streams only while it shows, the app is in front and no full viewer is
/// open (one stream from this phone at a time). Tapping it opens the viewer over the details, which come
/// back when it closes.
struct ScreenPreview: View {
    /// The bot whose details show it: the viewer watches it when it's the one using the computer.
    let botId: String
    @Environment(BotStore.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.displayScale) private var displayScale
    @State private var session: ScreenSession?
    @State private var size = CGSize.zero
    @State private var viewer: ScreenRequest?

    private var streams: Bool {
        scenePhase == .active && viewer == nil && model.screenRequest == nil && model.connection == .online && model.screen?.available == true
    }

    var body: some View {
        Color.black
            .aspectRatio(16 / 10, contentMode: .fit)
            .overlay { picture }
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .onGeometryChange(for: CGSize.self) { $0.size } action: { size = $0; requestView() }
            .environment(\.colorScheme, .dark)
            .environment(\.screenNoticeCompact, true)
            .onAppear(perform: update)
            .onChange(of: streams) { update() }
            .onDisappear(perform: stop)
            .codyncOverlay(isPresented: Binding(get: { viewer != nil }, set: { if !$0 { viewer = nil } })) { close in
                ScreenView(watching: viewer?.watching, close: close)
            }
    }

    @ViewBuilder private var picture: some View {
        if let notice = ScreenNotice(model) {
            notice
        } else if let session {
            if let notice = ScreenNotice(session) { notice } else { live(session) }
        }
    }

    private func live(_ session: ScreenSession) -> some View {
        Button {
            viewer = ScreenRequest(watching: model.screen?.agentBot == botId ? botId : nil)
        } label: {
            ScreenVideo(track: session.track)
                .overlay(alignment: .topLeading) {
                    if let agent = model.screen?.agentBot.flatMap({ model.bots[$0] }) {
                        AgentAtScreen(name: agent.name).padding(8)
                    }
                }
                .overlay(alignment: .bottomTrailing) {
                    Image(systemName: "arrow.up.left.and.arrow.down.right")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.white)
                        .frame(width: 28, height: 28)
                        .background(.ultraThinMaterial, in: Circle())
                        .padding(8)
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        .accessibilityLabel(L10n.tr("Open the screen"))
    }

    // MARK: session

    private func update() {
        if streams { start() } else { stop() }
    }

    private func start() {
        guard session == nil, let client = model.client, let display = model.screen?.mainDisplay else { return }
        let s = ScreenSession(client: client, display: display)
        session = s
        requestView()
        Task { await s.start() }
    }

    private func stop() {
        session?.close()
        session = nil
    }

    /// The picture's size in pixels: the session sends it once its channel opens, and the computer
    /// streams no larger.
    private func requestView() {
        guard size.width > 0, size.height > 0 else { return }
        session?.view(crop: nil, pixels: CGSize(width: size.width * displayScale, height: size.height * displayScale))
    }
}
