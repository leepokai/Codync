import Foundation
import Testing
@testable import CodyncKit

// Graphs match host/src/workflows/outline.rs and the shapes described for
// host/tests/fixtures/workflow-graph.json. That fixture file is not in the tree;
// these cases are the same graphs, inlined.

private func node(_ id: String, _ kind: WorkflowNodeKind, x: Double = 0, y: Double = 0,
                  fields: [String: JSONValue] = [:]) -> WorkflowNode {
    WorkflowNode(id: id, kind: kind, at: WorkflowPoint(x: x, y: y), fields: fields)
}

private func edge(_ from: String, _ to: String, branch: String? = nil) -> WorkflowEdge {
    let id = branch.map { "\(from).\($0)>\(to):text" } ?? "\(from)>\(to):text"
    return WorkflowEdge(id: id, from: from, to: to, branch: branch)
}

private func trigger(_ id: String) -> WorkflowNode {
    node(id, .trigger, fields: ["trigger": .string("manual")])
}

@Test func stepRanksFollowTheDeepestParent() {
    let graph = WorkflowGraph(
        nodes: [
            trigger("t"),
            node("p", .prompt, x: 0, fields: ["text": .string("hi")]),
            node("a", .agent, x: 80),
            node("b", .agent, x: 200),
            node("o", .output, x: 140, fields: ["target": .string("notify")]),
        ],
        edges: [edge("t", "p"), edge("p", "a"), edge("p", "b"), edge("a", "o"), edge("b", "o")]
    )
    #expect(graph.stepRanks() == [["p"], ["a", "b"], ["o"]])
}

@Test func straightLineIsAnOutline() {
    let graph = WorkflowGraph(
        nodes: [trigger("t"), node("a", .agent), node("o", .output, fields: ["target": .string("notify")])],
        edges: [edge("t", "a"), edge("a", "o")]
    )
    let outline = WorkflowLayout.outline(of: graph)
    #expect(outline?.main == ["a", "o"])
    #expect(outline?.condition == nil)
}

@Test func conditionBranchIsAnOutline() {
    let graph = WorkflowGraph(
        nodes: [
            trigger("t"),
            node("c", .condition, fields: ["op": .string("empty")]),
            node("yes", .agent),
            node("no", .output, fields: ["target": .string("notify")]),
        ],
        edges: [edge("t", "c"), edge("c", "yes", branch: "yes"), edge("c", "no", branch: "no")]
    )
    let outline = WorkflowLayout.outline(of: graph)
    #expect(outline?.main == [])
    #expect(outline?.condition == "c")
    #expect(outline?.yes == ["yes"])
    #expect(outline?.no == ["no"])
}

@Test func promptSitsBeforeTheStepItFeeds() {
    let graph = WorkflowGraph(
        nodes: [trigger("t"), node("p", .prompt, fields: ["text": .string("Look here")]), node("a", .agent)],
        edges: [edge("t", "a"), edge("p", "a")]
    )
    let outline = WorkflowLayout.outline(of: graph)
    #expect(outline?.main == ["p", "a"])
}

@Test func twoConditionsAndALoopAreNoOutline() {
    let two = WorkflowGraph(
        nodes: [trigger("t"), node("c1", .condition), node("c2", .condition)],
        edges: []
    )
    #expect(WorkflowLayout.outline(of: two) == nil)

    let loop = WorkflowGraph(
        nodes: [trigger("t"), node("a", .agent), node("b", .agent)],
        edges: [edge("t", "a"), edge("a", "b"), edge("b", "a")]
    )
    #expect(WorkflowLayout.outline(of: loop) == nil)
    #expect(WorkflowLayout.loopEdges(in: loop) == Set(["b>a:text"]))
}

@Test func annotationsAloneStayAnOutline() {
    let note = node("n", .note, x: 12, y: 34, fields: ["text": .string("aside")])
    let graph = WorkflowGraph(
        nodes: [trigger("t"), node("a", .agent), note],
        edges: [edge("t", "a")]
    )
    let outline = WorkflowLayout.outline(of: graph)
    #expect(outline?.main == ["a"])

    let linked = WorkflowGraph(
        nodes: [trigger("t"), node("a", .agent), node("n", .note)],
        edges: [edge("t", "a"), WorkflowEdge(id: "n>a:text", from: "n", to: "a")]
    )
    #expect(WorkflowLayout.outline(of: linked) == nil)
}

@Test func helloWorkflowsFlagIsOptional() throws {
    let missing = Data(#"{"hostId":"h","name":"Mac","version":"2","os":"macOS","backends":[],"rev":1}"#.utf8)
    let hello = try JSONDecoder().decode(Hello.self, from: missing)
    #expect(hello.workflows == nil)
    let present = Data(#"{"hostId":"h","name":"Mac","version":"2","os":"macOS","backends":[],"rev":1,"workflows":true}"#.utf8)
    #expect(try JSONDecoder().decode(Hello.self, from: present).workflows == true)
}

@Test func stepLineNamesTheKindAndWhatItDoes() {
    let prompt = node("p", .prompt, fields: ["text": .string("你们是什么模型？")])
    #expect(WorkflowCopy.kind(prompt) == "Prompt")
    #expect(WorkflowCopy.summary(prompt, botName: { _ in nil }, schedule: nil) == "你们是什么模型？")

    let agent = node("a", .agent, fields: ["botId": .string("b"), "task": .string("")])
    #expect(WorkflowCopy.kind(agent) == "Agent")
    #expect(WorkflowCopy.who(agent, botName: { _ in "王林" }) == "王林")
    #expect(WorkflowCopy.summary(agent, botName: { _ in "王林" }, schedule: nil) == "Do this step with the input.")
    #expect(WorkflowCopy.summary(node("m", .agent), botName: { _ in nil }, schedule: nil) == "Choose a bot")

    let tasked = node("t", .agent, fields: ["botId": .string("b"), "task": .string("Answer")])
    #expect(WorkflowCopy.summary(tasked, botName: { _ in "王林" }, schedule: nil) == "Answer")

    let output = node("o", .output, fields: ["target": .string("notify")])
    #expect(WorkflowCopy.kind(output) == "Output")
    #expect(WorkflowCopy.summary(output, botName: { _ in nil }, schedule: nil) == "Notification")
}

@Test func parentsFollowIncomingEdgesOnce() {
    let graph = WorkflowGraph(
        nodes: [node("p", .prompt), node("a", .agent), node("b", .agent), node("o", .output)],
        edges: [edge("p", "a"), edge("p", "b"), edge("a", "o"), edge("b", "o"), edge("a", "o")]
    )
    #expect(graph.parents(of: "p").isEmpty)
    #expect(graph.parents(of: "a") == ["p"])
    #expect(graph.parents(of: "o") == ["a", "b"])
}

@Test func stepNumbersFollowTheList() {
    let graph = WorkflowGraph(
        nodes: [trigger("t"), node("p", .prompt), node("a", .agent), node("b", .agent), node("o", .output)],
        edges: [edge("t", "p"), edge("p", "a"), edge("p", "b"), edge("a", "o"), edge("b", "o")]
    )
    let numbers = graph.stepNumbers()
    #expect(numbers["t"] == nil)
    #expect(numbers["p"] == 1)
    #expect(numbers["a"] == 2)
    #expect(numbers["b"] == 3)
    #expect(numbers["o"] == 4)
    #expect(graph.parents(of: "o").compactMap { numbers[$0] } == [2, 3])
    #expect(graph.parents(of: "p").compactMap { numbers[$0] }.isEmpty)
}

@Test func workflowEventParseDropsUndecodablePayloads() {
    let graph = HostClient.parseEvent(Data(#"{"type":"workflow","change":"graph","workflowId":"w"}"#.utf8))
    guard case let .workflow(event) = graph else {
        Issue.record("graph event")
        return
    }
    #expect(event.change == .graph && event.workflowId == "w")

    let enabled = HostClient.parseEvent(Data(#"{"type":"workflow","change":"enabled","workflowId":"w","enabled":false}"#.utf8))
    guard case let .workflow(flag) = enabled else {
        Issue.record("enabled event")
        return
    }
    #expect(flag.change == .enabled)

    let badChange = HostClient.parseEvent(Data(#"{"type":"workflow","change":"nope","workflowId":"w"}"#.utf8))
    let missing = HostClient.parseEvent(Data(#"{"type":"workflow","workflowId":"w"}"#.utf8))
    let broken = HostClient.parseEvent(Data(#"{"type":"workflow","change":"#.utf8))
    for parsed in [badChange, missing, broken] {
        switch parsed {
        case nil:
            break
        case .undecodable:
            Issue.record("a workflow event must not rewind the stream")
        default:
            Issue.record("expected the event to be dropped")
        }
    }
}

@Test func soloStepDecodesFromTheHostsWorkflowStepAnswer() throws {
    // host/src/workflows/run.rs `step_json`, for an approval run on its own: no bot, output or end yet.
    struct Answer: Decodable { var step: WorkflowStep }
    let json = #"{"step":{"id":"s","nodeId":"a","attempt":1,"requestId":"r","kind":"approval","botId":null,"status":"approval","input":"Ship it?","output":null,"branch":null,"passes":0,"truncated":false,"error":null,"startedAt":5,"endedAt":null,"title":"Approval"}}"#
    let step = try JSONDecoder().decode(Answer.self, from: Data(json.utf8)).step
    #expect(step.id == "s" && step.status == "approval" && step.botId == nil && step.endedAt == nil)
}

@Test func searchFindsWorkflowsByTitleOrIdPrefix() {
    let list = [
        WorkflowSummary(id: "a1b2", title: "Nightly build", revision: 1, updatedAt: 0),
        WorkflowSummary(id: "c3d4", title: "Release notes", revision: 1, updatedAt: 0),
    ]
    #expect(WorkflowSummary.matching(list, " ") == list)
    #expect(WorkflowSummary.matching(list, "BUILD").map(\.id) == ["a1b2"])
    #expect(WorkflowSummary.matching(list, " c3 ").map(\.id) == ["c3d4"])
    #expect(WorkflowSummary.matching(list, "b2").isEmpty)
}
