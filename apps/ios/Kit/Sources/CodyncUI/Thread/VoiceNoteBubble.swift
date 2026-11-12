import CodyncKit
import SwiftUI

/// An agent message that carries a voice note: the player sits at the top of the bubble and the message under
/// it, on the bubble's own background — a voice note with a caption, the way a messaging app shows it.
struct VoiceNoteBubble: View {
    let entry: Entry
    let file: SharedFile
    @Environment(BotStore.self) private var model
    @State private var audio = SharedAudio()

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            player
            MarkdownText(entry.data.text ?? "").equatable()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .onDisappear { audio.stop() }
    }

    /// The play/pause button, the progress bar and its clock on one row.
    private var player: some View {
        let download = model.fileDownloads.states[file.id]
        let caption = FileDownloadCard.detail(download, play: audio.state, elapsed: audio.elapsed, duration: audio.duration, size: file.size)
        // The 44 pt hit area overflows the 24 pt glyph, so the glyph lines up with the text under it, as on a card.
        return HStack(spacing: 12) {
            AudioPlayButton(state: audio.state, action: play).padding(-10)
            if let duration = audio.duration { AudioProgress(elapsed: audio.elapsed, duration: duration, seek: audio.seek) }
            // Steady digits, so the bar's end doesn't move as the clock runs.
            Text(caption).font(.caption2.monospacedDigit()).foregroundStyle(audio.state == .failed ? Palette.danger : Palette.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // One element reads the whole player; its buttons become named actions.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(L10n.tr("Voice note")), \(caption)")
        .accessibilityAction { play() }
        .accessibilityActions {
            Button(audio.state == .playing ? L10n.tr("Pause") : L10n.tr("Play"), action: play)
            Button(model.fileDownloads.isDownloading(file.id) ? L10n.tr("Cancel download") : L10n.tr("Download"), action: self.download)
        }
        .audioSeekAdjustable(audio)
    }

    private func play() {
        Task { await audio.toggle(file, entryId: entry.id, model: model) }
    }

    private func download() {
        Motion.animate { model.fileDownloads.toggle(client: model.client, entryId: entry.id, botId: entry.botId, file: file) }
    }
}
