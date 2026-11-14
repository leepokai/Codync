import CodyncKit
import SwiftUI

/// An artifact to open: its bot and id, and the title to show until its details arrive.
struct ArtifactTarget: Identifiable, Equatable {
    let botId: String
    let id: String
    let title: String
}

/// A temporary `<title>.html` for the share sheet.
struct ArtifactFile: Identifiable {
    let url: URL
    var id: URL { url }
}

extension View {
    /// The share sheet for an artifact's `<title>.html` while `file` is set; the temporary copy goes when it closes.
    func artifactShareSheet(_ file: Binding<ArtifactFile?>) -> some View { modifier(ArtifactShareSheet(file: file)) }
}

private struct ArtifactShareSheet: ViewModifier {
    @Binding var file: ArtifactFile?
    @Environment(BotStore.self) private var model

    func body(content: Content) -> some View {
        content.codyncSheet(item: $file) { shared in
            FileExportSheet(url: shared.url) {
                file = nil
                model.removeArtifactFile(shared.url)
            }
        }
    }
}

/// An artifact full size, in a web view of its own: close, the title (tap to rename) and share on top, and a box at the
/// bottom for messages about the artifact, which go to the bot as replies to its card. The viewer stays open, and a
/// version the bot publishes meanwhile replaces the one shown.
struct ArtifactView: View {
    let target: ArtifactTarget
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var renaming: Artifact?
    @State private var sharing: ArtifactFile?
    @State private var shareError: String?

    private var botId: String { target.botId }
    private var artifact: Artifact? { model.artifacts.details[target.id] }
    private var title: String { artifact?.title ?? target.title }

    var body: some View {
        VStack(spacing: 0) {
            // The sheet's grabber takes taps in its top ~30 pt, so the centred title (tap to rename) sits below them.
            header.padding(.top, 16)
            content
        }
        .safeAreaInset(edge: .bottom) {
            // An artifact published without a card (a routine run, another bot's request) has nothing to reply to.
            if let artifact, artifact.entryId != nil {
                Composer(botId: botId, replyingTo: .constant(nil), artifact: artifact)
            }
        }
        .background(Palette.background)
        .onAppear { model.loadArtifact(target.id, botId: botId) }
        .onChange(of: model.artifacts.newest(target.id)) { model.loadArtifact(target.id, botId: botId) }
        .codyncSheet(item: $renaming) { ArtifactRenameSheet(artifact: $0) }
        .artifactShareSheet($sharing)
        .tracked(.screenViewed(.artifact))
        .codyncDialog("Couldn't open the artifact",
                      isPresented: Binding(get: { shareError != nil }, set: { if !$0 { shareError = nil } }),
                      message: shareError, cancel: "OK") { [] }
    }

    private var header: some View {
        ScreenHeader {
            IconButton("Close artifact", systemImage: "xmark") { dismiss() }
        } title: {
            Button { renaming = artifact } label: {
                Text(title)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                    .padding(.horizontal, 48)
            }
            .buttonStyle(PressScale())
            .disabled(artifact == nil)
            .accessibilityHint(L10n.tr("Rename artifact"))
        } trailing: {
            IconButton("Share artifact", systemImage: "square.and.arrow.up") { share() }
                .disabled(model.artifacts.html[target.id] == nil)
        }
    }

    @ViewBuilder private var content: some View {
        let html = model.artifacts.html[target.id]
        ZStack {
            if let html {
                ArtifactWebView(artifactId: target.id, html: html)
                    .transition(.opacity)
            } else if let failure = model.artifacts.failures[target.id] {
                VStack(spacing: 6) {
                    Text(L10n.tr("Couldn't open the artifact")).font(.headline).foregroundStyle(Palette.text)
                    Text(L10n.tr(failure)).font(.subheadline).foregroundStyle(Palette.secondary).textSelection(.enabled)
                }
                .multilineTextAlignment(.center)
                .padding(24)
            } else {
                Spinner(size: 20)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: html?.updatedAt)
    }

    private func share() {
        Task {
            do {
                sharing = ArtifactFile(url: try await model.artifactFile(target.id, botId: botId))
            } catch {
                shareError = error.localizedDescription
            }
        }
    }
}
