import CodyncKit
import SwiftUI

/// A sound the chat plays, the same bubble for both kinds: a voice note inside a message (the player at the top,
/// the message under it) and a sound shared on its own (the file's name in the message's place). The way a
/// messaging app shows a voice note, with or without a caption.
struct VoiceNoteBubble<Below: View>: View {
    let entry: Entry
    let file: SharedFile
    /// What VoiceOver calls the player: "Voice note", or the sound's own name when it stands alone.
    let name: String
    let below: Below
    @Environment(BotStore.self) private var model
    @State private var audio = SharedAudio()

    init(entry: Entry, file: SharedFile, name: String, @ViewBuilder below: () -> Below) {
        self.entry = entry
        self.file = file
        self.name = name
        self.below = below()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            player
            below
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .onDisappear { audio.stop() }
        // The sound is fetched as the bubble shows, so the first tap plays at once — unless the fetch would cost
        // (an expensive network and a big note), where a tap still fetches. A chat shown before the computer
        // connects fetches nothing.
        .task(id: [model.connection == .online, NetworkCost.shared.expensive]) {
            if SharedAudio.preloads(online: model.connection == .online,
                                    expensive: NetworkCost.shared.expensive, bytes: file.size) {
                await audio.preload(file, entryId: entry.id, model: model)
            }
        }
    }

    /// The play/pause button, the progress bar and its clock on one row; a failed play or download puts its error
    /// in the bar's place. Download is in the message's menu.
    private var player: some View {
        let download = model.fileDownloads.states[file.id]
        let caption = FileDownloadCard.detail(download, play: audio.state, elapsed: audio.elapsed, duration: audio.duration, size: file.size)
        let failed = if case .failed = download { true } else { audio.state == .failed }
        // The 44 pt hit area overflows the 24 pt glyph, so the glyph lines up with the text under it.
        // A note that can't play hides the bar and puts the error in its place.
        return HStack(spacing: 12) {
            AudioPlayButton(state: audio.state, action: play).padding(-10)
            if !failed, let duration = audio.duration { AudioProgress(elapsed: audio.elapsed, duration: duration, seek: audio.seek) }
            // Steady digits, so the bar's end doesn't move as the clock runs. A failure's text fills the bar's
            // slot and centres in it, the row's 12 pt gap on both sides of the text.
            Text(caption).font(.caption2.monospacedDigit()).foregroundStyle(failed ? Palette.danger : Palette.secondary)
                .frame(maxWidth: failed ? .infinity : nil, alignment: failed ? .center : .leading)
                .padding(.trailing, failed ? 12 : 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // One element reads the whole player; its buttons become named actions.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(caption.isEmpty ? name : "\(name), \(caption)")
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
