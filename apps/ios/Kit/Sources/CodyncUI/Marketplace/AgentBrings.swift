import CodyncKit
import SwiftUI

/// Skills, connectors, rules and plugins the agent already has, under Sign in on its sheet.
struct AgentBrings: View {
    let backendId: String
    @Environment(BotStore.self) private var model
    @State private var gear: AgentGear?
    @State private var failed = false
    @State private var skills = false
    @State private var connectors = false
    @State private var rules = false
    @State private var plugins = false

    var body: some View {
        // A stack, not a Group: `.task` on a Group with nothing in it never runs, so nothing would load.
        VStack(alignment: .leading, spacing: 0) {
            if failed {
                CardSection {
                    Text(L10n.tr("Couldn't load what this agent brings."))
                        .font(.footnote)
                        .foregroundStyle(Palette.danger)
                }
            } else if let gear, gear.shows {
                CardSection("What the agent brings") {
                    VStack(alignment: .leading, spacing: 14) {
                        row("Skills", gear.skills, $skills)
                        row("Connectors", gear.connectors, $connectors)
                        row("Rules", gear.rules, $rules)
                        row("Plugins", gear.plugins, $plugins)
                    }
                }
            }
        }
        // Loaded again when the computer comes back online, so a failed load doesn't stay.
        .task(id: [backendId, model.connection == .online ? "online" : "waiting"]) { await load() }
    }

    @ViewBuilder private func row(_ title: String, _ items: [AgentGearItem], _ open: Binding<Bool>) -> some View {
        if !items.isEmpty {
            Disclosure(isExpanded: open) {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(item.name).font(.subheadline.weight(.semibold)).lineLimit(1)
                            let note = item.description.isEmpty ? item.detail : item.description
                            if !note.isEmpty {
                                Text(note).font(.caption).foregroundStyle(Palette.secondary).lineLimit(2)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 8)
                        .padding(.leading, 36)
                        .overlay(alignment: .top) {
                            if index > 0 {
                                Rectangle().fill(Palette.border).frame(height: 0.5).padding(.leading, 36)
                            }
                        }
                    }
                }
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: icon(title))
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Palette.text)
                        .frame(width: 26, height: 26)
                        .background(Palette.bubbleAgent, in: Circle())
                    Text(L10n.tr(title))
                    Spacer(minLength: 8)
                    Text("\(items.count)")
                        .font(.caption.weight(.semibold))
                        .monospacedDigit()
                        .foregroundStyle(Palette.secondary)
                        .padding(.horizontal, 7)
                        .padding(.vertical, 2)
                        .background(Palette.bubbleAgent, in: Capsule())
                }
                .layoutPriority(1)
            }
        }
    }

    private func icon(_ title: String) -> String {
        switch title {
        case "Skills": "book.pages"
        case "Connectors": "point.3.connected.trianglepath.dotted"
        case "Rules": "list.bullet.rectangle"
        default: "square.grid.2x2"
        }
    }

    private func load() async {
        guard model.connection == .online else { return }
        failed = false
        gear = nil
        do {
            gear = try await model.agentBrings(backendId)
        } catch is CancellationError {
        } catch {
            failed = true
        }
    }
}

extension AgentGear {
    fileprivate var shows: Bool { !skills.isEmpty || !connectors.isEmpty || !rules.isEmpty || !plugins.isEmpty }
}
