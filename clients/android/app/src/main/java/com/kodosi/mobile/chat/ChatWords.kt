package com.kodosi.mobile.chat

import android.icu.text.ListFormatter
import android.icu.text.MeasureFormat
import android.icu.text.NumberFormat
import android.icu.util.Currency
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import android.text.format.DateUtils
import androidx.annotation.PluralsRes
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.AgentModel
import com.kodosi.mobile.terminals.AgentPermissions
import com.kodosi.mobile.terminals.label
import com.kodosi.mobile.ui.AppSymbol
import java.time.Instant
import java.time.ZoneId
import java.util.Locale

internal object ChatWords {
    fun symbol(kind: ChatToolKind): AppSymbol = when (kind) {
        ChatToolKind.COMMAND -> AppSymbol.TERMINAL
        ChatToolKind.READ -> AppSymbol.TEXT
        ChatToolKind.EDIT -> AppSymbol.PENCIL
        ChatToolKind.WRITE -> AppSymbol.COMPOSE
        ChatToolKind.SEARCH -> AppSymbol.SEARCH
        ChatToolKind.WEB -> AppSymbol.BROWSER
        ChatToolKind.AGENT -> AppSymbol.FRIENDS
        ChatToolKind.SKILL -> AppSymbol.SPARKLE
        ChatToolKind.QUESTION -> AppSymbol.CHAT
        ChatToolKind.PLAN -> AppSymbol.TASKS
        ChatToolKind.OTHER -> AppSymbol.BRACES
    }

    fun modeSymbol(mode: ChatMode): AppSymbol = when (mode) {
        ChatMode.ASK -> AppSymbol.LOCK
        ChatMode.EDITS -> AppSymbol.PENCIL
        ChatMode.PLAN -> AppSymbol.TASKS
        ChatMode.AUTO -> AppSymbol.SPARKLE
        ChatMode.RULES -> AppSymbol.LIST
        ChatMode.UNRESTRICTED -> AppSymbol.BOLT
    }

    fun subject(tool: ChatTool): String? = tool.target.lineSequence().first().ifEmpty {
        if (tool.kind == ChatToolKind.SKILL || tool.kind == ChatToolKind.OTHER) tool.name else null
    }

    // Agents report full model IDs (e.g. "claude-sonnet-4-5-20250929") while the launcher uses
    // short IDs ("sonnet"). Match the short ID and append short version segments ("Sonnet 4.5").
    fun modelName(id: String, models: List<AgentModel>?): String {
        val known = models ?: return id
        known.firstOrNull { it.id == id }?.let { return it.label }
        val parts = id.substringBefore('[').split('-')
        for (model in known) {
            val at = parts.indexOf(model.id)
            if (at < 0) continue
            val version = parts.drop(at + 1).takeWhile { part -> part.length in 1..2 && part.all(Char::isDigit) }
            return if (version.isEmpty()) model.label else "${model.label} ${version.joinToString(".")}"
        }
        return id
    }

    fun monospaced(kind: ChatToolKind): Boolean = kind in setOf(
        ChatToolKind.COMMAND,
        ChatToolKind.READ,
        ChatToolKind.EDIT,
        ChatToolKind.WRITE,
        ChatToolKind.SEARCH
    )

    fun counts(tools: List<ChatTool>): List<Pair<ChatToolKind, Int>> {
        val counts = LinkedHashMap<ChatToolKind, Int>()
        val files = mutableMapOf<ChatToolKind, MutableSet<String>>()
        for (tool in tools) {
            val file = tool.kind == ChatToolKind.READ || tool.kind == ChatToolKind.EDIT ||
                tool.kind == ChatToolKind.WRITE
            if (file && tool.target.isNotEmpty()) {
                val seen = files.getOrPut(tool.kind) { mutableSetOf() }.apply { add(tool.target) }
                counts[tool.kind] = seen.size
            } else {
                counts[tool.kind] = (counts[tool.kind] ?: 0) + 1
            }
        }
        return counts.toList()
    }

    fun main(tools: List<ChatTool>): ChatToolKind? {
        val kinds = tools.groupingBy { it.kind }.eachCount()
        return tools.map { it.kind }.distinct().maxByOrNull { kinds.getValue(it) }
    }

    @PluralsRes
    private fun clause(kind: ChatToolKind): Int = when (kind) {
        ChatToolKind.COMMAND -> R.plurals.chat_ran_commands
        ChatToolKind.READ -> R.plurals.chat_read_files
        ChatToolKind.EDIT -> R.plurals.chat_changed_files
        ChatToolKind.WRITE -> R.plurals.chat_wrote_files
        ChatToolKind.SEARCH -> R.plurals.chat_made_searches
        ChatToolKind.WEB -> R.plurals.chat_fetched_pages
        ChatToolKind.AGENT -> R.plurals.chat_asked_helpers
        ChatToolKind.SKILL -> R.plurals.chat_used_skills
        ChatToolKind.QUESTION -> R.plurals.chat_asked_questions
        ChatToolKind.PLAN -> R.plurals.chat_made_plans
        ChatToolKind.OTHER -> R.plurals.chat_used_tools
    }

    @Composable
    fun summary(tools: List<ChatTool>): String {
        val locale = locale()
        val clauses = counts(tools).map { (kind, count) -> pluralStringResource(clause(kind), count, count) }
        return remember(clauses, locale) { sentence(ListFormatter.getInstance(locale).format(clauses), locale) }
    }

    /** Counts of the calls that failed, were denied or were stopped, such as "1 failed" or "1 denied". */
    @Composable
    fun setbacks(tools: List<ChatTool>): List<Pair<ChatToolState, String>> = listOf(
        ChatToolState.FAILED to R.plurals.chat_count_failed,
        ChatToolState.DENIED to R.plurals.chat_count_denied,
        ChatToolState.STOPPED to R.plurals.chat_count_stopped
    ).mapNotNull { (state, words) ->
        tools.count { it.state == state }.takeIf { it > 0 }?.let { state to pluralStringResource(words, it, it) }
    }

    @Composable
    fun step(tool: ChatTool): String {
        val question = tool.detail as? ChatToolDetail.Question
        val first = question?.questions?.firstOrNull()
        if (tool.kind == ChatToolKind.QUESTION && question != null && first != null) {
            return question.answers.firstOrNull()?.let { "${first.question} · $it" } ?: first.question
        }
        val subject = subject(tool)
        val (alone, about) = if (tool.state.isOpen) doing(tool.kind) else done(tool.kind)
        return if (subject == null) stringResource(alone) else stringResource(about, subject)
    }

    private fun doing(kind: ChatToolKind): Pair<Int, Int> = when (kind) {
        ChatToolKind.COMMAND -> R.string.chat_running to R.string.chat_running_subject

        ChatToolKind.READ -> R.string.chat_reading to R.string.chat_reading_subject

        ChatToolKind.EDIT -> R.string.chat_changing to R.string.chat_changing_subject

        ChatToolKind.WRITE -> R.string.chat_writing to R.string.chat_writing_subject

        ChatToolKind.SEARCH -> R.string.chat_searching to R.string.chat_searching_subject

        ChatToolKind.WEB -> R.string.chat_fetching to R.string.chat_fetching_subject

        ChatToolKind.AGENT -> R.string.chat_asking_helper to R.string.chat_asking_helper_subject

        ChatToolKind.PLAN -> R.string.chat_planning to R.string.chat_planning_subject

        ChatToolKind.SKILL, ChatToolKind.QUESTION, ChatToolKind.OTHER ->
            R.string.chat_using to R.string.chat_using_subject
    }

    private fun done(kind: ChatToolKind): Pair<Int, Int> = when (kind) {
        ChatToolKind.COMMAND -> R.string.chat_ran to R.string.chat_ran_subject

        ChatToolKind.READ -> R.string.chat_read to R.string.chat_read_subject

        ChatToolKind.EDIT -> R.string.chat_changed to R.string.chat_changed_subject

        ChatToolKind.WRITE -> R.string.chat_wrote to R.string.chat_wrote_subject

        ChatToolKind.SEARCH -> R.string.chat_searched to R.string.chat_searched_subject

        ChatToolKind.WEB -> R.string.chat_fetched to R.string.chat_fetched_subject

        ChatToolKind.AGENT -> R.string.chat_asked_helper to R.string.chat_asked_helper_subject

        ChatToolKind.PLAN -> R.string.chat_planned to R.string.chat_planned_subject

        ChatToolKind.SKILL, ChatToolKind.QUESTION, ChatToolKind.OTHER ->
            R.string.chat_used to R.string.chat_used_subject
    }

    @Composable
    fun now(view: ChatView): String = when (val now = view.now) {
        ChatNow.Writing -> stringResource(R.string.chat_writing)

        ChatNow.Compacting -> stringResource(R.string.chat_compacting)

        is ChatNow.Tool -> (view.item(now.item)?.body as? ChatBody.Tool)?.tool?.takeIf { it.state.isOpen }
            ?.let { step(it) } ?: stringResource(R.string.chat_thinking)

        ChatNow.Thinking, null -> stringResource(R.string.chat_thinking)
    }

    @Composable
    fun mode(mode: ChatMode): String = when (mode) {
        ChatMode.ASK -> AgentPermissions.ASK.label()
        ChatMode.AUTO -> AgentPermissions.AUTO.label()
        ChatMode.UNRESTRICTED -> AgentPermissions.UNRESTRICTED.label()
        ChatMode.EDITS -> stringResource(R.string.chat_mode_edits)
        ChatMode.PLAN -> stringResource(R.string.chat_mode_plan)
        ChatMode.RULES -> stringResource(R.string.chat_mode_rules)
    }

    @Composable
    fun duration(ms: Long): String {
        val locale = locale()
        return remember(ms, locale) {
            val seconds = ms / 1000
            val parts = listOfNotNull(
                (seconds / 3600).takeIf { it > 0 }?.let { Measure(it, MeasureUnit.HOUR) },
                (seconds / 60 % 60).takeIf { it > 0 }?.let { Measure(it, MeasureUnit.MINUTE) },
                (seconds % 60).takeIf { it > 0 || seconds == 0L }?.let { Measure(it, MeasureUnit.SECOND) }
            )
            MeasureFormat.getInstance(locale, MeasureFormat.FormatWidth.NARROW).formatMeasures(*parts.toTypedArray())
        }
    }

    @Composable
    fun cost(dollars: Double): String {
        val locale = locale()
        return remember(dollars, locale) {
            NumberFormat.getCurrencyInstance(locale).apply {
                currency = Currency.getInstance("USD")
                minimumFractionDigits = 2
                maximumFractionDigits = 2
            }.format(dollars)
        }
    }

    @Composable
    fun time(at: Long): String {
        val context = LocalContext.current
        val locale = locale()
        return remember(at, locale) { DateUtils.formatDateTime(context, at, DateUtils.FORMAT_SHOW_TIME) }
    }

    @Composable
    fun day(at: Long): String {
        val context = LocalContext.current
        val locale = locale()
        val today = stringResource(R.string.today)
        val yesterday = stringResource(R.string.yesterday)
        return remember(at, locale, today, yesterday) {
            val zone = ZoneId.systemDefault()
            val date = Instant.ofEpochMilli(at).atZone(zone).toLocalDate()
            val now = Instant.now().atZone(zone).toLocalDate()
            when (date) {
                now -> today

                now.minusDays(1) -> yesterday

                else -> DateUtils.formatDateTime(
                    context,
                    at,
                    DateUtils.FORMAT_SHOW_WEEKDAY or DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_NO_YEAR
                )
            }
        }
    }

    fun sameDay(first: Long, second: Long): Boolean {
        val zone = ZoneId.systemDefault()
        return Instant.ofEpochMilli(first).atZone(zone).toLocalDate() ==
            Instant.ofEpochMilli(second).atZone(zone).toLocalDate()
    }

    @Composable
    private fun locale(): Locale = LocalConfiguration.current.locales[0]

    private fun sentence(text: String, locale: Locale): String = text.replaceFirstChar { it.titlecase(locale) }
}
