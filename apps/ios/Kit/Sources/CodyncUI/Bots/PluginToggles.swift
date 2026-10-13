import CodyncKit
import SwiftUI

/// A card of on/off switches for the connectors or skills installed on the computer.
struct PluginToggles: View {
    let title: String
    let empty: String
    let items: [(id: String, name: String, detail: String)]
    @Binding var selection: [String]
    /// Shows an item's details from an info button beside its switch.
    var info: ((_ id: String) -> Void)?

    var body: some View {
        CardSection(title) {
            if items.isEmpty {
                Text(empty).foregroundStyle(Palette.secondary)
            }
            ForEach(items, id: \.id) { item in
                HStack(spacing: 4) {
                    SwitchRow(item.name, detail: item.detail.isEmpty ? nil : item.detail, isOn: Binding(
                        get: { selection.contains(item.id) },
                        set: { on in
                            selection.removeAll { $0 == item.id }
                            if on { selection.append(item.id) }
                        }
                    ))
                    if let info {
                        IconButton("About \(item.name)", systemImage: "info.circle") { info(item.id) }
                    }
                }
            }
        }
    }
}
