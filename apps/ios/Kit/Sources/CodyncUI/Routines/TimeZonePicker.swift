import CodyncKit
import SwiftUI

/// Every IANA time zone with its current GMT offset, narrowed as you type; picking one closes it.
struct TimeZonePicker: View {
    let selection: String
    let onPick: (String) -> Void
    @State private var query = ""
    private static let top = "top"

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Time zone")
            SearchField("Search time zones", text: $query)
                .padding(.horizontal, 20)
                .padding(.bottom, 12)
            ScrollViewReader { proxy in
                ScrollView {
                    let zones = TimeZones.search(query)
                    VStack(spacing: 0) {
                        Color.clear.frame(height: 0).id(Self.top)
                        if zones.isEmpty {
                            Text(L10n.tr("No matching time zones")).foregroundStyle(Palette.secondary).padding(.top, 8)
                        }
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(zones, id: \.self) { zone in
                                row(zone)
                                if zone != zones.last { Hairline().padding(.horizontal, 16) }
                            }
                        }
                        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                    }
                    .padding(.horizontal, 20)
                    .padding(.bottom, 20)
                }
                .scrollDismissesKeyboard(.interactively)
                .onAppear { proxy.scrollTo(selection, anchor: .center) }
                // A shorter list from a deep scroll position would otherwise show blank.
                .onChange(of: query) { proxy.scrollTo(Self.top, anchor: .top) }
            }
        }
        .font(.body)
        .background(Palette.background)
    }

    private func row(_ zone: String) -> some View {
        Button { onPick(zone) } label: {
            HStack(spacing: 10) {
                Text(zone).foregroundStyle(Palette.text).lineLimit(1)
                Spacer(minLength: 8)
                Text(TimeZones.offset(zone)).foregroundStyle(Palette.secondary)
                Image(systemName: "checkmark").font(.callout.weight(.semibold)).foregroundStyle(Palette.accent)
                    .opacity(zone == selection ? 1 : 0)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        .accessibilityAddTraits(zone == selection ? .isSelected : [])
    }
}
