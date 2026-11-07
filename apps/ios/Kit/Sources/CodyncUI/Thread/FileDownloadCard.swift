import CodyncKit
import SwiftUI
import UIKit

struct FileDownloadCard: View {
    let entry: Entry
    let file: SharedFile
    @Environment(BotStore.self) private var model
    @State private var viewing = false
    @State private var audio = SharedAudio()

    var body: some View {
        let download = model.fileDownloads.states[file.id]
        let caption = Self.detail(download, play: audio.state, elapsed: audio.elapsed, duration: audio.duration, size: file.size)
        VStack(alignment: .leading, spacing: 12) {
            if file.showsAsImage { preview }
            row(caption: caption)
        }
        .padding(14)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 16))
        .codyncSheet(isPresented: $viewing) { AttachmentViewer(shared: file, entryId: entry.id) }
        .onDisappear { audio.stop() }
        // One element reads the whole card; its buttons become named actions.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(file.name), \(caption)")
        .accessibilityActions {
            if file.showsAsAudio {
                Button(audio.state == .playing ? L10n.tr("Pause") : L10n.tr("Play"), action: play)
            }
            if file.showsAsImage {
                Button(L10n.tr("View")) { viewing = true }
            }
            Button(busy ? L10n.tr("Cancel download") : L10n.tr("Download"), action: self.download)
        }
        .audioSeekAdjustable(file.showsAsAudio ? audio : nil)
    }

    /// The picture itself; tap for full size.
    private var preview: some View {
        AttachmentImage(shared: file, entryId: entry.id, maxPixels: 720)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .contentShape(Rectangle())
            .onTapGesture { viewing = true }
            .frame(maxWidth: .infinity, maxHeight: 280, alignment: .leading)
    }

    private func row(caption: String) -> some View {
        HStack(spacing: 12) {
            if file.showsAsAudio { playButton } else {
                Image(systemName: AttachmentIcon.symbol(file.name)).font(.system(size: 24)).foregroundStyle(Palette.secondary)
                    .frame(width: 24)
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(file.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(2)
                Text(caption).font(.caption2).foregroundStyle(audio.state == .failed ? Palette.danger : Palette.secondary)
                if audio.duration != nil { progress }
            }
            Spacer(minLength: 0)
            Button(action: download) {
                Image(systemName: busy ? "xmark" : "arrow.down.to.line")
                    .font(.system(size: 20)).frame(width: 44, height: 44).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    /// A voice note's play/pause, in place of its file icon. The shared button's 44 pt hit area overflows the
    /// 24 pt glyph's slot, so the name lines up with the other file cards.
    private var playButton: some View {
        AudioPlayButton(state: audio.state, action: play).padding(-10)
    }

    /// The position in the sound, once its length is known.
    private var progress: some View {
        AudioProgress(elapsed: audio.elapsed, duration: audio.duration ?? 1, seek: audio.seek).padding(.top, 3)
    }

    private func play() {
        Task { await audio.toggle(file, entryId: entry.id, model: model) }
    }

    private func download() {
        model.fileDownloads.toggle(client: model.client, entry: entry, file: file)
    }

    private var busy: Bool { model.fileDownloads.isDownloading(file.id) }

    /// What the card says under the name: a failed play, then the download, then the sound's position, then its size.
    static func detail(_ download: FileDownloads.State?, play: SharedAudio.State = .idle,
                       elapsed: TimeInterval = 0, duration: TimeInterval? = nil, size bytes: Int64) -> String {
        if play == .failed { return L10n.tr("Couldn't play. Tap to retry.") }
        let size = ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
        switch download {
        case .downloading(let received):
            let received = ByteCountFormatter.string(fromByteCount: received, countStyle: .file)
            return L10n.tr("{received} of {size}")
                .replacingOccurrences(of: "{received}", with: received)
                .replacingOccurrences(of: "{size}", with: size)
        case .failed(let message): return "\(L10n.tr(message)) \(L10n.tr("Tap to retry."))"
        case .saved, nil: break
        }
        if let duration { return "\(SharedAudio.timeText(elapsed)) / \(SharedAudio.timeText(duration))" }
        if case .saved = download { return "\(size) · \(L10n.tr("Downloaded"))" }
        return size
    }
}

/// A shared sound's play/pause: a spinner while it loads, and the 44 pt tap target of every card-sized control.
/// Both a file card and a bubble's voice note show it.
struct AudioPlayButton: View {
    let state: SharedAudio.State
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Group {
                if state == .loading {
                    Spinner(size: 24)
                } else {
                    Image(systemName: state == .playing ? "pause.circle" : "play.circle")
                        .font(.system(size: 24)).foregroundStyle(Palette.text)
                        .contentTransition(.symbolEffect(.replace))
                }
            }
            .frame(width: 44, height: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// How far a shared sound has played: a 4 pt bar filling left to right. Tap or drag it to move there; its touch
/// area runs 20 pt over and under the bar without taking up layout, as the play button's does.
struct AudioProgress: View {
    let elapsed: TimeInterval
    let duration: TimeInterval
    let seek: (TimeInterval) -> Void

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(Palette.tertiary)
                Capsule().fill(Palette.text).frame(width: proxy.size.width * min(1, elapsed / max(1, duration)))
            }
            .frame(height: 4)
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                seek(duration * min(max(drag.location.x / proxy.size.width, 0), 1))
            })
        }
        .frame(height: 44)
        .padding(.vertical, -20)
    }
}

extension View {
    /// VoiceOver's swipe up or down on a voice note moves it five seconds; another file's card isn't adjustable.
    @ViewBuilder func audioSeekAdjustable(_ audio: SharedAudio?) -> some View {
        if let audio {
            accessibilityAdjustableAction { direction in
                audio.seek(to: audio.elapsed + (direction == .increment ? 5 : -5))
            }
        } else {
            self
        }
    }
}

extension View {
    /// The share sheet for a file of this chat once it's downloaded (`FileDownloads.export`). Off (`active`) while
    /// a sheet over this view presents it instead.
    func fileExportSheet(_ model: BotStore, botId: String, active: Bool = true) -> some View {
        codyncSheet(item: Binding<FileDownloads.Export?>(get: {
            guard active, let item = model.fileDownloads.export, item.botId == botId else { return nil }
            return item
        }, set: { if $0 == nil { model.fileDownloads.dismissExport() } })) { item in
            FileExportSheet(url: item.url) { model.fileDownloads.dismissExport() }
        }
    }
}

/// iOS owns save/export destinations through its native share sheet.
struct FileExportSheet: UIViewControllerRepresentable {
    let url: URL
    let completed: () -> Void
    func makeUIViewController(context: Context) -> UIActivityViewController {
        let controller = UIActivityViewController(activityItems: [url], applicationActivities: nil)
        controller.completionWithItemsHandler = { _, _, _, _ in completed() }
        return controller
    }
    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
