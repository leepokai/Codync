import Foundation

/// Columns the phone can edit. Same rules as `host/src/workflows/outline.rs`.
public struct WorkflowOutline: Equatable, Sendable {
    public var trigger: String
    public var main: [String]
    public var condition: String?
    public var yes: [String]
    public var no: [String]
    public var after: [String]
    public var loose: [String]

    public init(trigger: String, main: [String] = [], condition: String? = nil, yes: [String] = [], no: [String] = [],
                after: [String] = [], loose: [String] = []) {
        self.trigger = trigger
        self.main = main
        self.condition = condition
        self.yes = yes
        self.no = no
        self.after = after
        self.loose = loose
    }
}

/// Outline, reading order and the lines an outline rewrites. The host is the authority on save.
public enum WorkflowLayout {
    /// `outline` when the phone can add and move steps, `fields` otherwise.
    /// An unknown kind is `readOnly` so a save would not drop it. Annotations alone stay `outline`.
    public static func editMode(of graph: WorkflowGraph) -> WorkflowEditMode {
        if graph.hasUnknownKind { return .readOnly }
        return outline(of: graph) == nil ? .fields : .outline
    }

    public static func outline(of graph: WorkflowGraph) -> WorkflowOutline? {
        let steps = graph.nodes.filter(\.kind.isStep)
        let triggers = steps.filter { $0.kind == .trigger }
        guard triggers.count == 1, steps.filter({ $0.kind == .condition }).count <= 1 else { return nil }
        let trigger = triggers[0].id
        let kinds = Dictionary(uniqueKeysWithValues: steps.map { ($0.id, $0.kind) })
        let nodes = Dictionary(uniqueKeysWithValues: steps.map { ($0.id, $0) })
        if graph.edges.contains(where: { edge in
            edge.port != "text" || kinds[edge.from] == nil || kinds[edge.to] == nil
        }) {
            return nil
        }
        if !loopEdges(in: graph).isEmpty { return nil }
        var reached: Set<String> = [trigger]
        var queue = [trigger]
        while let id = queue.popLast() {
            for edge in graph.edges where edge.from == id && reached.insert(edge.to).inserted {
                queue.append(edge.to)
            }
        }
        var prompts: [String: String] = [:]
        for node in steps where node.kind == .prompt {
            let targets = graph.edges.filter { $0.from == node.id }.map(\.to)
            if targets.isEmpty || !targets.contains(where: { reached.contains($0) }) { continue }
            guard targets.count == 1, targets.allSatisfy({ reached.contains($0) }) else { return nil }
            prompts[node.id] = targets[0]
        }
        let loose = steps.map(\.id).filter { !reached.contains($0) && prompts[$0] == nil }
        let looseSet = Set(loose)
        if graph.edges.contains(where: { looseSet.contains($0.from) && !looseSet.contains($0.to) }) { return nil }
        let core = Set(reached.filter { kinds[$0] != .output })
        guard let chain = coreChain(trigger: trigger, core: core, kinds: kinds, edges: graph.edges) else { return nil }
        func order(_ lhs: String, _ rhs: String) -> Bool {
            let a = nodes[lhs]?.at ?? WorkflowPoint(x: 0, y: 0)
            let b = nodes[rhs]?.at ?? WorkflowPoint(x: 0, y: 0)
            if a.x != b.x { return a.x < b.x }
            if a.y != b.y { return a.y < b.y }
            return lhs < rhs
        }
        var placements: [String: [String]] = [:]
        for node in steps where node.kind == .output && reached.contains(node.id) {
            let feeds = graph.edges.filter { $0.to == node.id && core.contains($0.from) }
            switch feeds.count {
            case 1 where feeds[0].from == (chain.condition ?? ""):
                guard let branch = feeds[0].branch else { return nil }
                placements["start:\(branch)", default: []].append(node.id)
            case 1:
                placements["after:\(feeds[0].from)", default: []].append(node.id)
            case 2 where chain.condition != nil:
                let ends = [laneEnd(chain, "yes"), laneEnd(chain, "no")]
                let matches = { (edge: WorkflowEdge) in ends.contains { $0.from == edge.from && $0.branch == edge.branch } }
                guard matches(feeds[0]), matches(feeds[1]),
                      !(feeds[0].from == feeds[1].from && feeds[0].branch == feeds[1].branch) else { return nil }
                placements["start:after", default: []].append(node.id)
            default:
                return nil
            }
        }
        func withOutputs(_ name: String, _ items: [String]) -> [String] {
            var result = placements["start:\(name)"] ?? []
            result.sort(by: order)
            if name == "main" {
                var extra = placements["after:\(trigger)"] ?? []
                extra.sort(by: order)
                result.append(contentsOf: extra)
            }
            for id in items {
                result.append(id)
                var extra = placements["after:\(id)"] ?? []
                extra.sort(by: order)
                result.append(contentsOf: extra)
            }
            return result
        }
        var outline = WorkflowOutline(
            trigger: trigger,
            main: withOutputs("main", chain.main),
            condition: chain.condition,
            yes: withOutputs("yes", chain.yes),
            no: withOutputs("no", chain.no),
            after: withOutputs("after", chain.after),
            loose: loose
        )
        let pairs = prompts.sorted { order($0.key, $1.key) }
        for (prompt, target) in pairs {
            if target == chain.condition {
                outline.main.append(prompt)
                continue
            }
            guard let key = listKey(in: outline, id: target) else { return nil }
            var list = outline[keyPath: key]
            guard let index = list.firstIndex(of: target) else { return nil }
            list.insert(prompt, at: index)
            outline[keyPath: key] = list
        }
        return outline
    }

    /// Rewrites step lines and lays the columns out. Notes, strokes and loose lines stay.
    public static func graph(from outline: WorkflowOutline, keeping graph: WorkflowGraph) -> WorkflowGraph {
        let kinds = Dictionary(uniqueKeysWithValues: graph.nodes.map { ($0.id, $0.kind) })
        var edges: [WorkflowEdge] = []
        func link(_ from: String, _ to: String, branch: String?) {
            let id = branch.map { "\(from).\($0)>\(to):text" } ?? "\(from)>\(to):text"
            edges.append(WorkflowEdge(id: id, from: from, to: to, branch: branch))
        }
        func walk(_ items: [String], start: [(from: String, branch: String?)]) -> (sources: [(from: String, branch: String?)], prompts: [String]) {
            var sources = start
            var prompts: [String] = []
            for id in items {
                if kinds[id] == .prompt {
                    prompts.append(id)
                    continue
                }
                for source in sources { link(source.from, id, branch: source.branch) }
                for prompt in prompts { link(prompt, id, branch: nil) }
                prompts = []
                if kinds[id] != .output { sources = [(id, nil)] }
            }
            return (sources, prompts)
        }
        let walked = walk(outline.main, start: [(outline.trigger, nil)])
        var sources = walked.sources
        var prompts = walked.prompts
        if let condition = outline.condition {
            for source in sources { link(source.from, condition, branch: source.branch) }
            for prompt in prompts { link(prompt, condition, branch: nil) }
            let yes = walk(outline.yes, start: [(condition, "yes")])
            let no = walk(outline.no, start: [(condition, "no")])
            if !outline.after.isEmpty {
                sources = yes.sources + no.sources
                prompts = []
                _ = walk(outline.after, start: sources)
            }
        }
        let loose = Set(outline.loose)
        edges.append(contentsOf: graph.edges.filter { loose.contains($0.from) && loose.contains($0.to) })
        var next = graph
        next.edges = edges
        place(&next, outline)
        return next
    }

    /// Top to bottom: a layer goes left to right, then top to bottom. Back edges wait for the next pass.
    public static func readingOrder(of graph: WorkflowGraph) -> [String] {
        let back = loopEdges(in: graph)
        let steps = graph.nodes.filter(\.kind.isStep)
        var indegree = Dictionary(uniqueKeysWithValues: steps.map { ($0.id, 0) })
        var outs: [String: [String]] = [:]
        for edge in graph.edges where !back.contains(edge.id) {
            guard indegree[edge.to] != nil, indegree[edge.from] != nil else { continue }
            indegree[edge.to, default: 0] += 1
            outs[edge.from, default: []].append(edge.to)
        }
        func point(_ id: String) -> WorkflowPoint {
            steps.first { $0.id == id }?.at ?? WorkflowPoint(x: 0, y: 0)
        }
        func before(_ lhs: String, _ rhs: String) -> Bool {
            let a = point(lhs)
            let b = point(rhs)
            if a.x != b.x { return a.x < b.x }
            if a.y != b.y { return a.y < b.y }
            return lhs < rhs
        }
        var ready = indegree.filter { $0.value == 0 }.map(\.key).sorted(by: before)
        var order: [String] = []
        while let id = ready.first {
            ready.removeFirst()
            order.append(id)
            for next in outs[id] ?? [] {
                indegree[next, default: 0] -= 1
                if indegree[next] == 0 { ready.append(next) }
            }
            ready.sort(by: before)
        }
        for node in steps where !order.contains(node.id) { order.append(node.id) }
        return order
    }

    /// Edges that point at a node still on the depth-first stack.
    public static func loopEdges(in graph: WorkflowGraph) -> Set<String> {
        var outs: [String: [WorkflowEdge]] = [:]
        var incoming: Set<String> = []
        for edge in graph.edges {
            outs[edge.from, default: []].append(edge)
            incoming.insert(edge.to)
        }
        var back: Set<String> = []
        var state: [String: Int] = [:]
        func walk(_ id: String) {
            state[id] = 1
            for edge in outs[id] ?? [] {
                switch state[edge.to] ?? 0 {
                case 1: back.insert(edge.id)
                case 0: walk(edge.to)
                default: break
                }
            }
            state[id] = 2
        }
        var starts = graph.nodes.map(\.id).filter { !incoming.contains($0) }
        starts.append(contentsOf: graph.nodes.map(\.id).filter { state[$0] == nil })
        for id in starts where state[id] == nil { walk(id) }
        return back
    }

    private struct Chain {
        var main: [String]
        var condition: String?
        var yes: [String]
        var no: [String]
        var after: [String]
    }

    private struct End {
        var from: String
        var branch: String?
    }

    private static func laneEnd(_ chain: Chain, _ branch: String) -> End {
        let list = branch == "yes" ? chain.yes : chain.no
        if let last = list.last { return End(from: last, branch: nil) }
        return End(from: chain.condition ?? "", branch: branch)
    }

    private static func listKey(in outline: WorkflowOutline, id: String) -> WritableKeyPath<WorkflowOutline, [String]>? {
        if outline.main.contains(id) { return \.main }
        if outline.yes.contains(id) { return \.yes }
        if outline.no.contains(id) { return \.no }
        if outline.after.contains(id) { return \.after }
        return nil
    }

    private static func coreChain(trigger: String, core: Set<String>, kinds: [String: WorkflowNodeKind], edges: [WorkflowEdge]) -> Chain? {
        var out: [String: [WorkflowEdge]] = [:]
        var incoming: [String: Int] = [:]
        for edge in edges where core.contains(edge.from) && core.contains(edge.to) {
            out[edge.from, default: []].append(edge)
            incoming[edge.to, default: 0] += 1
        }
        for id in core {
            let ways = out[id] ?? []
            if kinds[id] == .condition {
                let yes = ways.filter { $0.branch == "yes" }.count
                let no = ways.filter { $0.branch == "no" }.count
                if ways.contains(where: { $0.branch == nil }) || yes > 1 || no > 1 { return nil }
            } else if ways.count > 1 {
                return nil
            }
        }
        if incoming[trigger] != nil { return nil }
        var seen: Set<String> = [trigger]
        func next(_ id: String) -> String? { out[id]?.first?.to }
        guard let start = run(next(trigger), stopAtCondition: true, seen: &seen, incoming: incoming, kinds: kinds, next: next) else { return nil }
        if start.rest != nil { return nil }
        guard let condition = start.condition else {
            guard seen.count == core.count else { return nil }
            return Chain(main: start.list, condition: nil, yes: [], no: [], after: [])
        }
        func branch(_ name: String) -> String? { out[condition]?.first { $0.branch == name }?.to }
        guard let yes = run(branch("yes"), stopAtCondition: false, seen: &seen, incoming: incoming, kinds: kinds, next: next),
              let no = run(branch("no"), stopAtCondition: false, seen: &seen, incoming: incoming, kinds: kinds, next: next),
              yes.rest == no.rest else { return nil }
        var after: [String] = []
        if let join = yes.rest {
            if incoming[join] != 2 || kinds[join] == .condition || seen.contains(join) { return nil }
            seen.insert(join)
            after.append(join)
            guard let rest = run(next(join), stopAtCondition: false, seen: &seen, incoming: incoming, kinds: kinds, next: next),
                  rest.rest == nil else { return nil }
            after.append(contentsOf: rest.list)
        }
        guard seen.count == core.count else { return nil }
        return Chain(main: start.list, condition: condition, yes: yes.list, no: no.list, after: after)
    }

    private struct Walk {
        var list: [String]
        var condition: String?
        var rest: String?
    }

    private static func run(_ first: String?, stopAtCondition: Bool, seen: inout Set<String>, incoming: [String: Int],
                            kinds: [String: WorkflowNodeKind], next: (String) -> String?) -> Walk? {
        var list: [String] = []
        var current = first
        while let id = current, incoming[id] ?? 0 == 1 {
            guard seen.insert(id).inserted else { return nil }
            if kinds[id] == .condition {
                guard stopAtCondition else { return nil }
                return Walk(list: list, condition: id, rest: nil)
            }
            list.append(id)
            current = next(id)
        }
        return Walk(list: list, condition: nil, rest: current)
    }

    private static func place(_ graph: inout WorkflowGraph, _ outline: WorkflowOutline) {
        func set(_ id: String, column: Int, y: Double) {
            guard let index = graph.nodes.firstIndex(where: { $0.id == id }) else { return }
            graph.nodes[index].at = WorkflowPoint(x: 80 + Double(column) * 400, y: y)
        }
        func placeList(_ items: [String], column: Int, y: Double) -> Int {
            var column = column
            for id in items {
                column += 1
                set(id, column: column, y: y)
            }
            return column
        }
        set(outline.trigger, column: 0, y: 80)
        var column = placeList(outline.main, column: 0, y: 80)
        guard let condition = outline.condition else { return }
        column += 1
        set(condition, column: column, y: 80)
        let yes = placeList(outline.yes, column: column, y: 80)
        let no = placeList(outline.no, column: column, y: 300)
        _ = placeList(outline.after, column: max(yes, no), y: 80)
    }
}
