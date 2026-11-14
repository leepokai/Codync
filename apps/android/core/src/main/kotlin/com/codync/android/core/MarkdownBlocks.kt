package com.codync.android.core

/**
 * Block-level Markdown as replies use it: paragraphs, headings, nested lists, quotes, rules,
 * tables and fenced code. A line-for-line port of CodyncUI `MarkdownBlocks.swift` (the desktop
 * app has the same), so every client splits a reply the same way.
 */
object MarkdownBlocks {
    sealed interface Block {
        data class Paragraph(val text: String) : Block
        data class Heading(val text: String, val level: Int) : Block
        data class Bullet(val text: String, val marker: String, val depth: Int = 0) : Block
        data class Quote(val text: String) : Block
        data class Code(val text: String, val language: String) : Block
        data class Table(val header: List<String>, val rows: List<List<String>>) : Block
        data object Rule : Block
    }

    fun parse(source: String): List<Block> {
        val blocks = mutableListOf<Block>()
        val paragraph = mutableListOf<String>()
        val table = mutableListOf<String>()
        var code: MutableList<String>? = null
        var language = ""

        fun flush() {
            if (paragraph.isNotEmpty()) { blocks += Block.Paragraph(paragraph.joinToString("\n")); paragraph.clear() }
            if (table.isNotEmpty()) {
                // A header and its `|---|` line make a table; anything less stays text.
                blocks += if (table.size >= 2 && isTableRule(table[1])) Block.Table(cells(table[0]), table.drop(2).map(::cells))
                    else Block.Paragraph(table.joinToString("\n"))
                table.clear()
            }
        }

        for (raw in source.split("\n")) {
            val line = raw.trim(' ', '\t')
            if (line.startsWith("```")) {
                val open = code
                if (open != null) { blocks += Block.Code(open.joinToString("\n"), language); code = null }
                else { flush(); code = mutableListOf(); language = line.drop(3) }
                continue
            }
            code?.let { it += raw; continue }
            if (line.startsWith("|")) {
                if (paragraph.isNotEmpty()) flush()
                table += line
                continue
            }
            if (table.isNotEmpty()) flush()
            val depth = minOf(raw.takeWhile { it == ' ' || it == '\t' }.length / 2, 3)
            val hashes = line.indexOfFirst { it != '#' }
            val dot = line.indexOf('.')
            when {
                line.isEmpty() -> flush()
                line == "---" || line == "***" || line == "___" -> { flush(); blocks += Block.Rule }
                line.startsWith("#") && hashes > 0 && line[hashes] == ' ' -> {
                    flush(); blocks += Block.Heading(line.substring(hashes).trim(), hashes)
                }
                listOf("- ", "* ", "+ ", "• ").any(line::startsWith) -> {
                    flush(); blocks += Block.Bullet(line.drop(2), if (depth > 0) "◦" else "•", depth)
                }
                dot > 0 && line.substring(0, dot).all(Char::isDigit) && line.substring(dot + 1).startsWith(" ") -> {
                    flush(); blocks += Block.Bullet(line.substring(dot + 2), line.substring(0, dot + 1), depth)
                }
                line.startsWith(">") -> { flush(); blocks += Block.Quote(line.drop(1).trim()) }
                else -> paragraph += raw
            }
        }
        code?.let { blocks += Block.Code(it.joinToString("\n"), language) }
        flush()
        return blocks
    }

    private fun isTableRule(line: String) = '-' in line && line.all { it in "|-: " }

    private fun cells(line: String): List<String> {
        var row = line
        if (row.startsWith("|")) row = row.drop(1)
        if (row.endsWith("|")) row = row.dropLast(1)
        return row.split("|").map(String::trim)
    }
}
