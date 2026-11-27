import CodyncKit
import SwiftUI

extension Image {
    /// The codync-network mark (host/crates/network/assets/icon.png); SF Symbols has none.
    public static var codyncNetwork: Image { Image("codync-network", bundle: .module) }
}

/// A computer's networks of the user's other computers, read-only: who is in each, whether each one
/// answers and which bots it offers, who may message this computer's bots there, and whether cloud
/// assistants may reach them.
/// Changes are made at a computer. The computer is the `BotStore` in the environment.
public struct NetworkSheet: View {
    @Environment(BotStore.self) private var store
    @State private var showOlder = false

    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader {
                HStack(spacing: 8) {
                    Image.codyncNetwork.resizable().scaledToFit().frame(width: 26, height: 26).accessibilityHidden(true)
                    Text(L10n.tr("Network"))
                    // The network is a beta feature.
                    BetaTag()
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
                        CardSection("Who may message bots here", footer: network.rulesFooter) {
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
                if let sessions = store.network?.sessions, !sessions.isEmpty {
                    // In the host's order: running first, then the most recent.
                    let older = sessions.filter { $0.older == true }
                    CardSection("Agents", footer: "Bots and other agents message these sessions at their address; each reads its mail itself.") {
                        ForEach(sessions.filter { $0.older != true }) { NetworkSessionRow(session: $0) }
                        if !older.isEmpty {
                            Disclosure(isExpanded: $showOlder) {
                                ForEach(older) { NetworkSessionRow(session: $0) }
                            } label: {
                                Text(L10n.fill(L10n.tr("Older sessions: {count}"), ["count": "\(older.count)"]))
                                    .font(.subheadline).foregroundStyle(Palette.secondary)
                            }
                            .accessibilityIdentifier("network.olderSessions")
                        }
                    }
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

/// A coding agent's session: whether it runs, its logo and title (else address), then its address, what it does and how it gets its mail.
private struct NetworkSessionRow: View {
    let session: NetworkView.Session

    var body: some View {
        HStack(spacing: 8) {
            // No state, no dot: unknown isn't offline.
            Circle()
                .fill(session.runs == true ? Palette.added : Palette.tertiary)
                .frame(width: 7, height: 7)
                .opacity(session.state == nil ? 0 : 1)
                .accessibilityLabel(session.dotTitle.map { L10n.tr($0) } ?? "")
                .accessibilityHidden(session.dotTitle == nil)
            AgentIcon(registry: session.registry, size: 18)
            VStack(alignment: .leading, spacing: 2) {
                Text(session.title ?? session.address).font(.body).foregroundStyle(Palette.text).lineLimit(1)
                if !detail.isEmpty {
                    Text(detail).font(.footnote).foregroundStyle(Palette.secondary).lineLimit(1)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
    }

    private var detail: String {
        [session.title == nil ? nil : session.address, session.stateTitle.map { L10n.tr($0) }, session.tierTitle.map { L10n.tr($0) }]
            .compactMap { $0 }.joined(separator: " · ")
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
