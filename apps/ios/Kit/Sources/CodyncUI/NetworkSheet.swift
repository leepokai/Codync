import CodyncKit
import SwiftUI

/// A computer's networks of the user's other computers, read-only: who is in each, whether each one
/// answers and which bots it offers, who may message this computer's bots there, and whether cloud
/// assistants may reach them.
/// Changes are made at a computer. The computer is the `BotStore` in the environment.
public struct NetworkSheet: View {
    @Environment(BotStore.self) private var store

    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader {
                HStack(spacing: 8) {
                    Image(systemName: "point.3.connected.trianglepath.dotted").font(.title2).accessibilityHidden(true)
                    Text(L10n.tr("Network"))
                    // The network is a beta feature.
                    Text(L10n.tr("Beta"))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Palette.secondary)
                        .padding(.horizontal, 7)
                        .padding(.vertical, 2)
                        .background(Palette.bubbleAgent, in: Capsule())
                }
            }
            CardForm {
                let networks = store.network?.networks ?? []
                ForEach(networks) { network in
                    // One section per network: its computers, then who may message the bots here.
                    CardSection(network.name) {
                        ForEach(network.computers ?? []) { NetworkComputerRow(computer: $0) }
                    }
                    if let rules = network.rules, !rules.isEmpty {
                        CardSection("Who may message bots here", footer: "The first rule that matches decides. Without rules, every computer in the network may message every bot here. Answers to this computer's own messages always get in.") {
                            ForEach(Array(rules.enumerated()), id: \.offset) { _, rule in
                                Text(network.text(of: rule)).font(.subheadline).foregroundStyle(Palette.text)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .padding(.vertical, 4)
                            }
                        }
                    }
                }
                // The note that changes happen at a computer closes the networks, after the last one's rules.
                if !networks.isEmpty {
                    CardSection(footer: "Changes are made on a computer: in Codync's settings there, or with codync-host network.") {}
                }
                if store.network?.assistantsAvailable == true {
                    CardSection(footer: "ChatGPT or Claude in your Codync account can message this computer's bots when it's on. Codync's cloud can read these calls.") {
                        ValueRow("Cloud assistants", value: store.network?.assistants == true ? "On" : "Off")
                    }
                }
            }
            .animation(Motion.layout, value: store.network)
        }
        .background(Palette.background)
        // Presence as of now: the host asks the others when stale, and its event updates this.
        .task { await store.refreshNetwork() }
        .tracked(.screenViewed(.network))
    }
}

private struct NetworkComputerRow: View {
    let computer: NetworkView.Computer

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(computer.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                Spacer(minLength: 8)
                Text(L10n.tr(computer.role.title)).font(.footnote).foregroundStyle(Palette.secondary)
            }
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Circle()
                    .fill(computer.online ? Palette.added : Palette.tertiary)
                    .frame(width: 7, height: 7)
                    .accessibilityHidden(true)
                Text(L10n.tr(presence))
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
            }
            Text(bots)
                .font(.footnote)
                .foregroundStyle(Palette.tertiary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }

    private var presence: String {
        computer.me ? "This computer" : computer.online ? "Online" : "Away"
    }

    private var bots: String {
        let names = (computer.agents ?? []).map(\.name)
        return names.isEmpty ? L10n.tr("No bots") : names.joined(separator: ", ")
    }
}
