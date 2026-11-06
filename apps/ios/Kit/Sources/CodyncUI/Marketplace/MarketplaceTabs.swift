import CodyncKit
import SwiftUI

extension MarketplaceView {
    var extensionRail: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(ExtensionSection.allCases) { item in
                let on = section == item
                Button {
                    withAnimation(Motion.morph) { section = item }
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: item.icon)
                        Text(L10n.tr(item.title))
                    }
                    .font(.subheadline.weight(on ? .semibold : .regular))
                    .foregroundStyle(on ? Palette.text : Palette.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(on ? Palette.bubbleAgent : Color.clear, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
                .buttonStyle(PressScale())
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .frame(width: 168)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(L10n.tr("Extensions sections"))
    }

    @ViewBuilder var tabBody: some View {
        VStack(alignment: .leading, spacing: 12) {
            switch section {
            case .agents: agentsTab
            case .connectors: connectorsTab
            case .skills: skillsTab
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .id(section)
        .transition(.opacity)
    }

    @ViewBuilder private var agentsTab: some View {
        if model.hello?.backends.contains(where: \.needsNode) == true {
            NodeNotice { addingNode = true }
        }
        if agents.isEmpty {
            Text(query.isEmpty ? L10n.tr("No agents yet.") : L10n.tr("No agents match “{query}”.").replacingOccurrences(of: "{query}", with: query))
                .foregroundStyle(Palette.secondary)
        } else {
            LazyVGrid(columns: agentColumns, spacing: 10) {
                ForEach(agents) { b in agentCard(b) }
            }
        }
    }

    @ViewBuilder private var connectorsTab: some View {
        MarketRow(title: "Custom connector", subtitle: "Any MCP server: a command or a URL", added: false, addLabel: "New") {
            TileIcon(systemName: "point.3.connected.trianglepath.dotted")
        } add: {
            addingConnector = true
        }
        if connectors.loading {
            SkeletonGrid()
        } else if connectors.items.isEmpty, !connectors.hasMore {
            Text(query.isEmpty ? L10n.tr("Couldn't load connectors.") : L10n.tr("No connectors match “{query}”.").replacingOccurrences(of: "{query}", with: query))
                .foregroundStyle(Palette.secondary)
        } else {
            ItemGrid {
                ForEach(connectors.items.items) { c in
                    MarketRow(title: c.title, subtitle: c.description ?? c.name, added: c.installed,
                              open: c.installed ? nil : { viewingConnector = c }) {
                        ServiceLogo(website: c.website, name: c.title, registryName: c.name)
                    } add: {
                        installing = c
                    }
                }
            }
            if let cursor = connectors.cursor {
                LoadMoreRow(cursor: cursor, failed: connectors.error != nil) { await loadMoreConnectors() }
            }
        }
    }

    @ViewBuilder private var skillsTab: some View {
        MarketRow(title: "Your own skill", subtitle: "Write instructions a bot can follow", added: false, addLabel: "New") {
            TileIcon(systemName: "square.and.pencil")
        } add: {
            writingSkill = true
        }
        if loadingSkills {
            SkeletonGrid()
        } else if shownSkills.isEmpty {
            if !query.isEmpty {
                Text(L10n.tr("No skills match “{query}”.").replacingOccurrences(of: "{query}", with: query))
                    .foregroundStyle(Palette.secondary)
            }
        } else {
            ItemGrid {
                ForEach(shownSkills) { s in
                    let added = s.installed || model.installedSkills.contains { $0.id == s.source.lowercased() }
                    MarketRow(title: s.name, subtitle: s.description, added: added, busy: busySkill == s.source,
                              open: { viewingSkill = SkillRef(s, installed: added) }) {
                        SkillGlyph()
                    } add: {
                        busySkill = s.source
                        Task {
                            do { try await model.installSkill(source: s.source) } catch { report(error) }
                            busySkill = nil
                        }
                    }
                }
            }
        }
    }

    /// Two tiles across on a phone. One flexible column stretches each icon into a full-width card.
    private var agentColumns: [GridItem] {
        sizeClass == .compact
            ? [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)]
            : [GridItem(.adaptive(minimum: 160), spacing: 10)]
    }
}

/// The end of a list the registry continues. In view, it asks for the page at `cursor`, and again each
/// time the cursor moves on while it stays in view. After a failure it shows Try again, and asks again
/// once it scrolls back into view.
private struct LoadMoreRow: View {
    let cursor: String
    let failed: Bool
    let load: () async -> Void
    @State private var inView = false

    var body: some View {
        Group {
            if failed {
                IconButton("Try again", systemImage: "arrow.clockwise") { Task { await load() } }
                    .frame(maxWidth: .infinity)
            } else {
                SkeletonGrid()
            }
        }
        .onScrollVisibilityChange(threshold: 0.01) { inView = $0 }
        .task(id: inView ? cursor : nil) {
            if inView { await load() }
        }
    }
}
