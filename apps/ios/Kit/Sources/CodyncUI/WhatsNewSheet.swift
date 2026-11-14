import CodyncKit
import SwiftUI

/// After an update the maintainer called important: each due release's notes (docs/features/whats-new.md).
public struct WhatsNewSheet: View {
    let announcements: [Announcement]

    public init(announcements: [Announcement]) { self.announcements = announcements }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader("What's new")
            CardForm {
                ForEach(announcements) { announcement in
                    CardSection(announcement.version) {
                        ForEach(announcement.notes, id: \.self) { note in
                            Text(L10n.tr(note))
                                .foregroundStyle(Palette.text)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
        }
        .background(Palette.background)
    }
}
