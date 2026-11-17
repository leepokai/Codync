import CodyncKit
import SwiftUI

/// After an update the maintainer called important: each highlight's clip over its name and a sentence about it,
/// then the smaller changes (docs/features/whats-new.md).
public struct WhatsNewSheet: View {
    let announcements: [Announcement]

    public init(announcements: [Announcement]) { self.announcements = announcements }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader("What's new") {
                if let version = announcements.first?.version {
                    Text(version).font(.footnote).monospacedDigit().foregroundStyle(Palette.secondary)
                }
            }
            CardForm {
                ForEach(announcements) { announcement in
                    ForEach(announcement.features) { FeatureStory(feature: $0) }
                    if !announcement.notes.isEmpty { notes(announcement.notes) }
                }
            }
        }
        .background(Palette.background)
    }

    private func notes(_ notes: [String]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(L10n.tr("Also in this release"))
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Palette.text)
                .accessibilityAddTraits(.isHeader)
            ForEach(notes, id: \.self) { note in
                Text(L10n.tr(note))
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

/// One highlight: its looping clip, then its name and what it does.
private struct FeatureStory: View {
    let feature: Announcement.Feature

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let clip = feature.clip {
                LoopingClip(url: clip)
                    .aspectRatio(16 / 9, contentMode: .fit)
                    .background(.black)
                    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .overlay(alignment: .bottomTrailing) { film }
                    .accessibilityHidden(feature.film == nil)
            }
            HStack(spacing: 7) {
                Text(L10n.tr(feature.title))
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .accessibilityAddTraits(.isHeader)
                if feature.beta { BetaTag() }
            }
            Text(L10n.tr(feature.body))
                .foregroundStyle(Palette.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    /// Opens the full video, with sound.
    @ViewBuilder private var film: some View {
        if let url = feature.film {
            Link(destination: url) {
                Image(systemName: "play.fill")
                    .font(.footnote)
                    .foregroundStyle(.white)
                    .frame(width: 32, height: 32)
                    .background(.ultraThinMaterial.opacity(0.9), in: Circle())
                    .environment(\.colorScheme, .dark)
            }
            .buttonStyle(.plain)
            .padding(8)
            .accessibilityLabel(L10n.tr("Watch the full video"))
            .trackedLinks(.whatsNewFilm)
        }
    }
}
