package com.codync.android

import com.codync.android.core.EditMode
import com.codync.android.core.Graph
import com.codync.android.core.NodeKind
import com.codync.android.core.Point
import com.codync.android.core.WorkflowDetail
import com.codync.android.core.WorkflowEdge
import com.codync.android.core.WorkflowNode
import com.codync.android.core.resolvedMode

/**
 * The phone's step list. Same rules as `host/src/workflows/outline.rs`.
 * A graph the phone can express is edited as columns; any other shape keeps its lines.
 * Shapes covered by the tests match `host/tests/fixtures/workflow-graph.json`.
 */
internal data class Outline(
    val trigger: String,
    val main: List<String> = emptyList(),
    val condition: String? = null,
    val yes: List<String> = emptyList(),
    val no: List<String> = emptyList(),
    val after: List<String> = emptyList(),
    val loose: List<String> = emptyList(),
)

internal enum class OutlineColumn { Main, Yes, No, After, Loose }

/** Host mode, downgraded to fields when this phone cannot rebuild the lines. */
internal fun WorkflowDetail.phoneMode(): EditMode {
    val host = resolvedMode()
    if (host != EditMode.Outline) return host
    return if (outlineOf(graph) != null) EditMode.Outline else EditMode.Fields
}

internal fun editMode(graph: Graph): EditMode {
    if (graph.nodes.any { it.kind == NodeKind.Unknown } || graph.edges.any { it.type != "text" }) return EditMode.ReadOnly
    return if (outlineOf(graph) != null) EditMode.Outline else EditMode.Fields
}

/** Prompts at the end of a column that nothing downstream reads. */
internal fun unfedPrompts(outline: Outline, kinds: Map<String, NodeKind>): Set<String> {
    fun trailing(items: List<String>, feeds: Boolean) = if (feeds) emptyList() else items.takeLastWhile { kinds[it] == NodeKind.Prompt }
    return (trailing(outline.main, outline.condition != null) + trailing(outline.yes, false) +
        trailing(outline.no, false) + trailing(outline.after, false)).toSet()
}

internal fun Outline.columnOf(id: String): OutlineColumn? = when (id) {
    in main -> OutlineColumn.Main
    in yes -> OutlineColumn.Yes
    in no -> OutlineColumn.No
    in after -> OutlineColumn.After
    in loose -> OutlineColumn.Loose
    else -> null
}

internal fun Outline.items(column: OutlineColumn): List<String> = when (column) {
    OutlineColumn.Main -> main
    OutlineColumn.Yes -> yes
    OutlineColumn.No -> no
    OutlineColumn.After -> after
    OutlineColumn.Loose -> loose
}

internal fun Outline.with(column: OutlineColumn, items: List<String>): Outline = when (column) {
    OutlineColumn.Main -> copy(main = items)
    OutlineColumn.Yes -> copy(yes = items)
    OutlineColumn.No -> copy(no = items)
    OutlineColumn.After -> copy(after = items)
    OutlineColumn.Loose -> copy(loose = items)
}

internal fun Outline.moved(id: String, down: Boolean): Outline? {
    val column = columnOf(id) ?: return null
    val items = items(column).toMutableList()
    val index = items.indexOf(id)
    val next = index + if (down) 1 else -1
    if (index < 0 || next !in items.indices) return null
    items[index] = items[next]
    items[next] = id
    return with(column, items)
}

internal fun Outline.inserted(column: OutlineColumn, after: String?, id: String): Outline {
    val items = items(column).toMutableList()
    val at = if (after == null) items.size else (items.indexOf(after) + 1).coerceAtLeast(0)
    items.add(at.coerceAtMost(items.size), id)
    return with(column, items)
}

/** Drop a step. Removing the condition keeps both branches, one after the other, on the main column. */
internal fun Outline.without(id: String): Outline = if (condition == id) {
    copy(condition = null, main = main + yes + no + after, yes = emptyList(), no = emptyList(), after = emptyList())
} else copy(main = main - id, yes = yes - id, no = no - id, after = after - id, loose = loose - id)

/** Edges that return to a step still on the walk. Those don't wait on the upstream step. */
internal fun loopEdges(graph: Graph): Set<String> {
    val outs = mutableMapOf<String, MutableList<WorkflowEdge>>()
    val incoming = mutableSetOf<String>()
    for (edge in graph.edges) {
        outs.getOrPut(edge.from) { mutableListOf() }.add(edge)
        incoming.add(edge.to)
    }
    val back = mutableSetOf<String>()
    val state = mutableMapOf<String, Int>()
    fun walk(id: String) {
        state[id] = 1
        for (edge in outs[id].orEmpty()) {
            when (state[edge.to] ?: 0) {
                1 -> back.add(edge.id)
                0 -> walk(edge.to)
            }
        }
        state[id] = 2
    }
    val starts = graph.nodes.map { it.id }.filter { it !in incoming }.toMutableList()
    starts.addAll(graph.nodes.map { it.id }.filter { state[it] == null })
    for (id in starts) if (state[id] == null) walk(id)
    return back
}

private data class CoreChain(
    val main: List<String>,
    val condition: String?,
    val yes: List<String>,
    val no: List<String>,
    val after: List<String>,
)

/** The outline of [graph], or nothing when the phone must leave the lines alone. */
internal fun outlineOf(graph: Graph): Outline? {
    val steps = graph.nodes.filter { it.kind.isStep }
    val triggers = steps.filter { it.kind == NodeKind.Trigger }
    if (triggers.size != 1 || steps.count { it.kind == NodeKind.Condition } > 1) return null
    if (graph.nodes.any { it.kind == NodeKind.Unknown }) return null
    val trigger = triggers[0].id
    val kinds = steps.associate { it.id to it.kind }
    val nodes = steps.associateBy { it.id }
    if (graph.edges.any { it.type != "text" || it.from !in kinds || it.to !in kinds }) return null
    if (loopEdges(graph).isNotEmpty()) return null
    val reached = mutableSetOf(trigger)
    val queue = ArrayDeque<String>()
    queue.add(trigger)
    while (queue.isNotEmpty()) {
        val id = queue.removeLast()
        for (edge in graph.edges) if (edge.from == id && reached.add(edge.to)) queue.add(edge.to)
    }
    val prompts = mutableMapOf<String, String>()
    for (node in steps) {
        if (node.kind != NodeKind.Prompt) continue
        val targets = graph.edges.filter { it.from == node.id }.map { it.to }
        if (targets.isEmpty() || targets.none { it in reached }) continue
        if (targets.size != 1 || targets.any { it !in reached }) return null
        prompts[node.id] = targets[0]
    }
    val loose = steps.map { it.id }.filter { it !in reached && it !in prompts }
    val looseSet = loose.toSet()
    if (graph.edges.any { it.from in looseSet && it.to !in looseSet }) return null
    val core = reached.filter { kinds[it] != NodeKind.Output }.toSet()
    val chain = coreChain(trigger, core, kinds, graph.edges) ?: return null
    fun order(left: String, right: String): Int {
        val a = nodes[left]?.at
        val b = nodes[right]?.at
        val ax = a?.x ?: 0.0
        val ay = a?.y ?: 0.0
        val bx = b?.x ?: 0.0
        val by = b?.y ?: 0.0
        return ax.compareTo(bx).takeIf { it != 0 } ?: ay.compareTo(by)
    }
    val placements = mutableMapOf<String, MutableList<String>>()
    for (node in steps) {
        if (node.kind != NodeKind.Output || node.id !in reached) continue
        val feeds = graph.edges.filter { it.to == node.id && it.from in core }
        when {
            feeds.size == 1 && feeds[0].from == (chain.condition ?: "") -> {
                val branch = feeds[0].branch ?: return null
                placements.getOrPut("start:$branch") { mutableListOf() }.add(node.id)
            }
            feeds.size == 1 -> placements.getOrPut("after:${feeds[0].from}") { mutableListOf() }.add(node.id)
            feeds.size == 2 && chain.condition != null -> {
                val ends = listOf(laneEnd(chain, "yes"), laneEnd(chain, "no"))
                fun matches(edge: WorkflowEdge) = ends.any { it.first == edge.from && it.second == edge.branch }
                if (!matches(feeds[0]) || !matches(feeds[1]) || (feeds[0].from == feeds[1].from && feeds[0].branch == feeds[1].branch)) return null
                placements.getOrPut("start:after") { mutableListOf() }.add(node.id)
            }
            else -> return null
        }
    }
    fun withOutputs(name: String, items: List<String>): List<String> {
        val result = placements["start:$name"].orEmpty().sortedWith(::order).toMutableList()
        if (name == "main") result.addAll(placements["after:$trigger"].orEmpty().sortedWith(::order))
        for (id in items) {
            result.add(id)
            result.addAll(placements["after:$id"].orEmpty().sortedWith(::order))
        }
        return result
    }
    val outline = Outline(
        trigger = trigger,
        main = withOutputs("main", chain.main),
        condition = chain.condition,
        yes = withOutputs("yes", chain.yes),
        no = withOutputs("no", chain.no),
        after = withOutputs("after", chain.after),
        loose = loose,
    ).let { placed ->
        var current = placed
        val pairs = prompts.entries.sortedWith { a, b -> order(a.key, b.key) }
        for ((prompt, target) in pairs) {
            if (target == chain.condition) {
                current = current.copy(main = current.main + prompt)
                continue
            }
            val column = current.columnOf(target) ?: return null
            val list = current.items(column).toMutableList()
            val index = list.indexOf(target)
            if (index < 0) return null
            list.add(index, prompt)
            current = current.with(column, list)
        }
        current
    }
    return outline
}

private fun laneEnd(chain: CoreChain, branch: String): Pair<String, String?> {
    val list = if (branch == "yes") chain.yes else chain.no
    val last = list.lastOrNull()
    return if (last != null) last to null else (chain.condition ?: "") to branch
}

private fun coreChain(trigger: String, core: Set<String>, kinds: Map<String, NodeKind>, edges: List<WorkflowEdge>): CoreChain? {
    val out = mutableMapOf<String, MutableList<WorkflowEdge>>()
    val incoming = mutableMapOf<String, Int>()
    for (edge in edges) {
        if (edge.from !in core || edge.to !in core) continue
        out.getOrPut(edge.from) { mutableListOf() }.add(edge)
        incoming[edge.to] = (incoming[edge.to] ?: 0) + 1
    }
    for (id in core) {
        val ways = out[id].orEmpty()
        if (kinds[id] == NodeKind.Condition) {
            if (ways.any { it.branch == null } || ways.count { it.branch == "yes" } > 1 || ways.count { it.branch == "no" } > 1) return null
        } else if (ways.size > 1) return null
    }
    if (trigger in incoming) return null
    val seen = mutableSetOf(trigger)
    fun next(id: String) = out[id]?.firstOrNull()?.to
    val start = runChain(next(trigger), stopAtCondition = true, seen, incoming, kinds) { next(it) } ?: return null
    if (start.rest != null) return null
    val condition = start.condition ?: return if (seen.size == core.size) {
        CoreChain(start.list, null, emptyList(), emptyList(), emptyList())
    } else null
    fun branch(name: String) = out[condition]?.firstOrNull { it.branch == name }?.to
    val yes = runChain(branch("yes"), stopAtCondition = false, seen, incoming, kinds) { next(it) } ?: return null
    val no = runChain(branch("no"), stopAtCondition = false, seen, incoming, kinds) { next(it) } ?: return null
    if (yes.rest != no.rest) return null
    val after = mutableListOf<String>()
    val join = yes.rest
    if (join != null) {
        if (incoming[join] != 2 || kinds[join] == NodeKind.Condition || join in seen) return null
        seen.add(join)
        after.add(join)
        val rest = runChain(next(join), stopAtCondition = false, seen, incoming, kinds) { next(it) } ?: return null
        if (rest.rest != null) return null
        after.addAll(rest.list)
    }
    if (seen.size != core.size) return null
    return CoreChain(start.list, condition, yes.list, no.list, after)
}

private data class ChainRun(val list: List<String>, val condition: String?, val rest: String?)

private fun runChain(
    first: String?,
    stopAtCondition: Boolean,
    seen: MutableSet<String>,
    incoming: Map<String, Int>,
    kinds: Map<String, NodeKind>,
    next: (String) -> String?,
): ChainRun? {
    val list = mutableListOf<String>()
    var current = first
    while (current != null && (incoming[current] ?: 0) == 1) {
        val id = current
        if (!seen.add(id)) return null
        if (kinds[id] == NodeKind.Condition) {
            if (!stopAtCondition) return null
            return ChainRun(list, id, null)
        }
        list.add(id)
        current = next(id)
    }
    return ChainRun(list, null, current)
}

/** Rewrites the lines of an outline and lays the steps out in columns. Notes and loose lines stay. */
internal fun graphFromOutline(graph: Graph, outline: Outline): Graph {
    val kinds = graph.nodes.associate { it.id to it.kind }
    val edges = mutableListOf<WorkflowEdge>()
    fun link(from: String, to: String, branch: String?) {
        val id = if (branch == null) "$from>$to:text" else "$from.$branch>$to:text"
        edges.add(WorkflowEdge(id, from, to, "text", branch))
    }
    data class Source(val from: String, val branch: String?)
    fun walk(items: List<String>, start: List<Source>): Pair<List<Source>, List<String>> {
        var sources = start
        val prompts = mutableListOf<String>()
        for (id in items) {
            if (kinds[id] == NodeKind.Prompt) {
                prompts.add(id)
                continue
            }
            for (source in sources) link(source.from, id, source.branch)
            for (prompt in prompts) link(prompt, id, null)
            prompts.clear()
            if (kinds[id] != NodeKind.Output) sources = listOf(Source(id, null))
        }
        return sources to prompts.toList()
    }
    val (mainSources, mainPrompts) = walk(outline.main, listOf(Source(outline.trigger, null)))
    val condition = outline.condition
    if (condition != null) {
        for (source in mainSources) link(source.from, condition, source.branch)
        for (prompt in mainPrompts) link(prompt, condition, null)
        val (yes, _) = walk(outline.yes, listOf(Source(condition, "yes")))
        val (no, _) = walk(outline.no, listOf(Source(condition, "no")))
        if (outline.after.isNotEmpty()) walk(outline.after, yes + no)
    }
    val loose = outline.loose.toSet()
    edges.addAll(graph.edges.filter { it.from in loose && it.to in loose })
    return place(graph.copy(edges = edges), outline)
}

private fun place(graph: Graph, outline: Outline): Graph {
    val nodes = graph.nodes.toMutableList()
    fun set(id: String, column: Int, y: Double) {
        val index = nodes.indexOfFirst { it.id == id }
        if (index >= 0) nodes[index] = nodes[index].copy(at = Point(80.0 + column * 400.0, y))
    }
    fun placeList(items: List<String>, start: Int, y: Double): Int {
        var column = start
        for (id in items) {
            column += 1
            set(id, column, y)
        }
        return column
    }
    set(outline.trigger, 0, 80.0)
    var column = placeList(outline.main, 0, 80.0)
    val condition = outline.condition ?: return graph.copy(nodes = nodes)
    column += 1
    set(condition, column, 80.0)
    val yes = placeList(outline.yes, column, 80.0)
    val no = placeList(outline.no, column, 300.0)
    placeList(outline.after, maxOf(yes, no), 80.0)
    return graph.copy(nodes = nodes)
}

/** Rows for the line view. A step sits one row below the deepest numbered step that feeds it. */
internal fun stepRanks(graph: Graph): List<List<String>> {
    val numbers = stepNumbers(graph)
    val order = readingOrder(graph).filter { it in numbers }
    val numbered = order.toSet()
    val memo = mutableMapOf<String, Int>()
    val stack = mutableSetOf<String>()
    fun rank(id: String): Int {
        memo[id]?.let { return it }
        if (!stack.add(id)) return 0
        val value = graph.parentsOf(id).filter { it in numbered }.maxOfOrNull { rank(it) + 1 } ?: 0
        stack.remove(id)
        memo[id] = value
        return value
    }
    val rows = mutableListOf<MutableList<String>>()
    for (id in order) {
        val level = rank(id)
        while (rows.size <= level) rows.add(mutableListOf())
        rows[level].add(id)
    }
    return rows.filter { it.isNotEmpty() }
}

/** 1, 2, 3… for the steps on the list, in reading order. Triggers and notes are not numbered. */
internal fun stepNumbers(graph: Graph): Map<String, Int> {
    val map = linkedMapOf<String, Int>()
    for (id in readingOrder(graph)) {
        val kind = graph.nodes.firstOrNull { it.id == id }?.kind ?: continue
        if (kind == NodeKind.Trigger || kind.isAnnotation) continue
        map[id] = map.size + 1
    }
    return map
}

/** Top to bottom on a detail page: a layer's steps go left to right, then top to bottom. */
internal fun readingOrder(graph: Graph): List<String> {
    val back = loopEdges(graph)
    val steps = graph.nodes.filter { it.kind.isStep }
    val indegree = steps.associate { it.id to 0 }.toMutableMap()
    val outs = mutableMapOf<String, MutableList<String>>()
    for (edge in graph.edges) {
        if (edge.id in back) continue
        if (edge.to in indegree && edge.from in indegree) {
            indegree[edge.to] = (indegree[edge.to] ?: 0) + 1
            outs.getOrPut(edge.from) { mutableListOf() }.add(edge.to)
        }
    }
    val pos = steps.associate { it.id to (it.at.x to it.at.y) }
    fun compare(left: String, right: String): Int {
        val a = pos[left] ?: (0.0 to 0.0)
        val b = pos[right] ?: (0.0 to 0.0)
        return a.first.compareTo(b.first).takeIf { it != 0 } ?: a.second.compareTo(b.second).takeIf { it != 0 } ?: left.compareTo(right)
    }
    val ready = indegree.filter { it.value == 0 }.keys.sortedWith(::compare).toMutableList()
    val order = mutableListOf<String>()
    while (ready.isNotEmpty()) {
        val id = ready.removeAt(0)
        order.add(id)
        for (next in outs[id].orEmpty()) {
            val left = (indegree[next] ?: 0) - 1
            indegree[next] = left
            if (left == 0) ready.add(next)
        }
        ready.sortWith(::compare)
    }
    for (node in steps) if (node.id !in order) order.add(node.id)
    return order
}

/** Where a step reads from. A node id, a condition branch, or the join after both branches. */
internal sealed interface SourceTarget {
    data class Node(val id: String) : SourceTarget
    data object Yes : SourceTarget
    data object No : SourceTarget
    data object Both : SourceTarget

    val token: String
        get() = when (this) {
            is Node -> id
            Yes -> "*yes"
            No -> "*no"
            Both -> "*both"
        }

    companion object {
        fun from(token: String): SourceTarget = when (token) {
            "*yes" -> Yes
            "*no" -> No
            "*both" -> Both
            else -> Node(token)
        }
    }
}

/** The step this one reads. A prompt has none. */
internal fun Outline.sourceTarget(step: String, kinds: Map<String, NodeKind>): SourceTarget? {
    var found: SourceTarget? = null
    fun walk(items: List<String>, start: SourceTarget): SourceTarget {
        var source = start
        for (item in items) {
            val kind = kinds[item]
            if (kind == NodeKind.Prompt) continue
            if (item == step) found = source
            if (kind != NodeKind.Output) source = SourceTarget.Node(item)
        }
        return source
    }
    val feeding = walk(main, SourceTarget.Node(trigger))
    condition?.let { id ->
        if (id == step) found = feeding
        walk(yes, SourceTarget.Yes)
        walk(no, SourceTarget.No)
        walk(after, SourceTarget.Both)
    }
    return found
}

/** Moves [step] so it reads from [target]. The result stays an outline. */
internal fun Outline.retargeted(step: String, target: SourceTarget): Outline? {
    if (step == trigger || (target is SourceTarget.Node && target.id == step)) return null
    if (condition == step) return retargetCondition(target)
    val next = detach(step)
    return when (target) {
        SourceTarget.Both -> if (next.condition == null) null else next.copy(after = listOf(step) + next.after)
        SourceTarget.Yes -> if (next.condition == null) null else next.copy(yes = listOf(step) + next.yes)
        SourceTarget.No -> if (next.condition == null) null else next.copy(no = listOf(step) + next.no)
        is SourceTarget.Node -> if (target.id == next.trigger) next.copy(main = listOf(step) + next.main) else next.place(step, target.id)
    }
}

private fun Outline.retargetCondition(target: SourceTarget): Outline? {
    val source = (target as? SourceTarget.Node)?.id ?: return null
    if (source == trigger) return copy(main = emptyList(), after = main + after)
    val index = main.indexOf(source)
    if (index < 0) return null
    return copy(main = main.take(index + 1), after = main.drop(index + 1) + after)
}

private fun Outline.detach(step: String) = copy(main = main - step, yes = yes - step, no = no - step, after = after - step, loose = loose - step)

private fun Outline.place(step: String, afterId: String): Outline? {
    fun into(items: List<String>): List<String>? {
        val index = items.indexOf(afterId)
        if (index < 0) return null
        return items.toMutableList().apply { add(index + 1, step) }
    }
    into(main)?.let { return copy(main = it) }
    into(yes)?.let { return copy(yes = it) }
    into(no)?.let { return copy(no = it) }
    into(after)?.let { return copy(after = it) }
    return null
}

/** Applies a new source. An outline stays an outline; any other graph only changes this step's incoming lines. */
internal fun Graph.withSource(step: String, token: String): Graph? {
    val target = SourceTarget.from(token)
    val kinds = nodes.associate { it.id to it.kind }
    outlineOf(this)?.let { outline ->
        if (outline.sourceTarget(step, kinds) == target) return null
        return graphFromOutline(this, outline.retargeted(step, target) ?: return null)
    }
    return rewired(step, target)
}

private fun Graph.rewired(step: String, target: SourceTarget): Graph? {
    val node = nodes.firstOrNull { it.id == step } ?: return null
    if (node.kind == NodeKind.Trigger) return null
    val edges = edges.filter { it.to != step }.toMutableList()
    fun link(from: String, branch: String?) {
        val id = if (branch == null) "$from>$step:text" else "$from.$branch>$step:text"
        edges.add(WorkflowEdge(id, from, step, branch = branch))
    }
    when (target) {
        is SourceTarget.Node -> {
            if (target.id == step || nodes.none { it.id == target.id }) return null
            link(target.id, null)
        }
        SourceTarget.Yes, SourceTarget.No -> {
            val condition = nodes.firstOrNull { it.kind == NodeKind.Condition }?.id ?: return null
            val branch = if (target == SourceTarget.No) "no" else "yes"
            if (edges.any { it.from == condition && it.branch == branch }) return null
            link(condition, branch)
        }
        SourceTarget.Both -> {
            val condition = nodes.firstOrNull { it.kind == NodeKind.Condition }?.id ?: return null
            val tails = branchTails(edges, nodes, condition, step) ?: return null
            val linked = mutableSetOf<String>()
            for ((from, branch) in tails) {
                if (linked.add("$from|${branch.orEmpty()}")) link(from, branch)
            }
        }
    }
    return copy(edges = edges)
}

/** The node that can feed a step after one branch. An empty branch is the condition itself. */
private fun branchTails(edges: List<WorkflowEdge>, nodes: List<WorkflowNode>, condition: String, step: String): List<Pair<String, String?>>? {
    fun kind(id: String) = nodes.firstOrNull { it.id == id }?.kind
    fun tail(branch: String): Pair<String, String?>? {
        val first = edges.firstOrNull { it.from == condition && it.branch == branch && it.to != step }
            ?: return condition to branch
        var current = first.to
        val seen = mutableSetOf(condition, step)
        while (current !in seen) {
            if (kind(current) == NodeKind.Output) return null
            seen.add(current)
            val outs = edges.filter { it.from == current && it.to != step }
            if (outs.size != 1 || kind(outs[0].to) == NodeKind.Output) break
            current = outs[0].to
        }
        if (kind(current) == NodeKind.Output) return null
        return current to null
    }
    val yes = tail("yes") ?: return null
    val no = tail("no") ?: return null
    return listOf(yes, no)
}
