import CodyncKit
import SwiftUI

/// An artifact card in the chat: the top of the artifact itself, small and still, fading into a footer with the
/// artifact symbol, its title and "Artifact". It shows the newest version, whichever card it is; tapping opens it.
struct ArtifactCard: View {
    let botId: String
    let ref: ArtifactRef
    let open: () -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    static let symbol = "chart.bar.doc.horizontal"
    private static let previewHeight: CGFloat = 150
    /// The artifact lays out at twice the card's size and shows at half.
    private static let scale: CGFloat = 0.5
    private static let shape = RoundedRectangle(cornerRadius: 16, style: .continuous)

    var body: some View {
        Button(action: open) {
            VStack(spacing: 0) {
                preview
                footer
            }
            .frame(maxWidth: .infinity)
            .background(Palette.bubbleAgent, in: Self.shape)
            .clipShape(Self.shape)
            .contentShape(Self.shape)
        }
        .buttonStyle(PressScale())
        .accessibilityLabel("\(L10n.tr("Artifact")): \(model.artifacts.title(ref))")
        // Read only once the row is on screen, and again when a newer version is published.
        .onAppear { model.showArtifact(ref, botId: botId) }
        .onChange(of: model.artifacts.newest(ref.id)) { model.loadArtifact(ref.id, botId: botId) }
    }

    private var preview: some View {
        let html = model.artifacts.html[ref.id]
        return GeometryReader { proxy in
            let width = proxy.size.width
            ZStack {
                if let html {
                    // ponytail: one live web view per card on screen; a cached snapshot per version if scrolling stutters.
                    ArtifactWebView(artifactId: ref.id, html: html, interactive: false)
                        .frame(width: width / Self.scale, height: Self.previewHeight / Self.scale)
                        .scaleEffect(Self.scale, anchor: .topLeading)
                        .frame(width: width, height: Self.previewHeight, alignment: .topLeading)
                        .allowsHitTesting(false)
                        .transition(.opacity)
                } else if model.artifacts.failures[ref.id] == nil {
                    Spinner()
                } else {
                    Image(systemName: Self.symbol).font(.system(size: 34)).foregroundStyle(Palette.tertiary)
                }
            }
            .frame(width: width, height: Self.previewHeight)
            .clipped()
        }
        .frame(height: Self.previewHeight)
        .mask(LinearGradient(stops: [.init(color: .black, location: 0.5), .init(color: .clear, location: 1)],
                             startPoint: .top, endPoint: .bottom))
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: html?.updatedAt)
        .accessibilityHidden(true)
    }

    private var footer: some View {
        HStack(spacing: 10) {
            Image(systemName: Self.symbol)
                .font(.system(size: 22))
                .foregroundStyle(Palette.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(model.artifacts.title(ref))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .lineLimit(2)
                Text(L10n.tr("Artifact")).font(.caption2).foregroundStyle(Palette.secondary)
            }
        }
        .padding(.horizontal, 14)
        .padding(.bottom, 12)
    }
}
