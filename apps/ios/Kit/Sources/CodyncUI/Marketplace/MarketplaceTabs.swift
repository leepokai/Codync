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
        // On a phone the section gives up a little width so the page buttons beside the list have room.
        .padding(.horizontal, sizeClass == .compact ? 8 : 0)
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
            flanked(count: agents.count, page: agentPage) { index in
                let shown = Array(CatalogPaging.slice(agents, page: index, size: pageSize))
                LazyVGrid(columns: agentColumns, spacing: 10) {
                    ForEach(shown) { b in agentCard(b) }
                    ForEach(0..<pagePad(count: agents.count, shown: shown.count), id: \.self) { _ in AgentCardSpacer() }
                }
            }
        }
        CatalogPager(page: agentPage, count: agents.count, pageSize: pageSize, go: { agentPage = CatalogPaging.clamp(page: $0, count: agents.count, size: pageSize) }, onSize: changePageSize)
    }

    @ViewBuilder private var connectorsTab: some View {
        MarketRow(title: "Custom connector", subtitle: "Any MCP server: a command or a URL", added: false, addLabel: "New") {
            TileIcon(systemName: "point.3.connected.trianglepath.dotted")
        } add: {
            addingConnector = true
        }
        if connectors.loading {
            SkeletonGrid()
        } else if connectors.items.isEmpty {
            Text(query.isEmpty ? L10n.tr("Couldn't load connectors.") : L10n.tr("No connectors match “{query}”.").replacingOccurrences(of: "{query}", with: query))
                .foregroundStyle(Palette.secondary)
        } else {
            let rows = connectors.items.items
            flanked(count: rows.count, page: connectors.page, hasMore: connectors.hasMore, loading: connectors.loadingMore) { index in
                let shown = Array(CatalogPaging.slice(rows, page: index, size: pageSize))
                // The page past the loaded rows, while the registry fills it.
                if shown.isEmpty {
                    SkeletonGrid()
                } else {
                    ItemGrid {
                        ForEach(shown) { c in
                            MarketRow(title: c.title, subtitle: c.description ?? c.name, added: c.installed) {
                                ServiceLogo(website: c.website, name: c.title, registryName: c.name)
                            } add: {
                                installing = c
                            }
                        }
                        ForEach(0..<pagePad(count: rows.count, shown: shown.count), id: \.self) { _ in MarketRowSpacer() }
                    }
                }
            }
        }
        if !connectors.loading {
            CatalogPager(page: connectors.page, count: connectors.items.items.count, pageSize: pageSize, hasMore: connectors.hasMore,
                         loading: connectors.loadingMore, go: { showConnectorPage($0, animated: true) }, onSize: changePageSize)
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
            flanked(count: shownSkills.count, page: skillPage) { index in
                let shown = Array(CatalogPaging.slice(shownSkills, page: index, size: pageSize))
                ItemGrid {
                    ForEach(shown) { s in
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
                    ForEach(0..<pagePad(count: shownSkills.count, shown: shown.count), id: \.self) { _ in MarketRowSpacer() }
                }
            }
        }
        if !loadingSkills {
            CatalogPager(page: skillPage, count: shownSkills.count, pageSize: pageSize, go: { skillPage = CatalogPaging.clamp(page: $0, count: shownSkills.count, size: pageSize) }, onSize: changePageSize)
        }
    }

    /// How many invisible rows a short last page needs so the bar under the list does not jump.
    private func pagePad(count: Int, shown: Int) -> Int {
        count > pageSize ? max(0, pageSize - shown) : 0
    }

    /// On a phone the pages sit side by side in a paging scroll view, so a swipe turns the page,
    /// with previous and next on either side. A wide layout shows the one page.
    @ViewBuilder private func flanked<Content: View>(count: Int, page: Int, hasMore: Bool = false, loading: Bool = false, @ViewBuilder content: @escaping (Int) -> Content) -> some View {
        let pages = CatalogPaging.pageCount(count, size: pageSize, hasMore: hasMore)
        let current = CatalogPaging.clamp(page: page, count: count, size: pageSize, hasMore: hasMore)
        if sizeClass == .compact {
            ScrollView(.horizontal) {
                LazyHStack(alignment: .top, spacing: 0) {
                    ForEach(0..<pages, id: \.self) { index in
                        content(index).containerRelativeFrame(.horizontal)
                    }
                }
                .scrollTargetLayout()
            }
            .scrollTargetBehavior(.paging)
            .scrollIndicators(.hidden)
            .scrollPosition(id: Binding(get: { current }, set: { if let index = $0, index != current { swiped(to: index) } }))
            .overlay {
                if pages > 1 {
                    HStack(spacing: 0) {
                        PageEdgeButton(title: "Previous page", systemImage: "chevron.left", enabled: current > 0 && !loading) { slide(forward: false) }
                        Color.clear.frame(maxWidth: .infinity).allowsHitTesting(false)
                        PageEdgeButton(title: loading ? "Loading…" : "Next page", systemImage: "chevron.right",
                                       enabled: CatalogPaging.canForward(count: count, page: current, hasMore: hasMore, size: pageSize) && !loading) { slide(forward: true) }
                    }
                    .padding(.horizontal, -44)
                }
            }
        } else {
            content(current)
        }
    }

    /// Two tiles across on a phone. One flexible column stretches each icon into a full-width card.
    private var agentColumns: [GridItem] {
        sizeClass == .compact
            ? [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)]
            : [GridItem(.adaptive(minimum: 160), spacing: 10)]
    }

}
