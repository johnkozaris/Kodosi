package com.kodosi.mobile

import android.content.Context
import com.kodosi.mobile.runtime.Words
import com.kodosi.mobile.terminals.TerminalWords

/** The app's sentences for the stores, in the language that the app shows. */
class AndroidWords(private val context: Context) :
    Words,
    TerminalWords {
    override val unavailable: String get() = context.getString(R.string.runtime_unavailable)
    override val didNotWork: String get() = context.getString(R.string.did_not_work)
    override val accountChanged: String get() = context.getString(R.string.account_changed)
    override val otherAccount: String get() = context.getString(R.string.other_account)
    override val signInEnded: String get() = context.getString(R.string.sign_in_ended)
    override val accountDeleted: String get() = context.getString(R.string.account_deleted)
    override val fellBehind: String get() = context.getString(R.string.terminal_fell_behind)
    override val couldNotConnect: String get() = context.getString(R.string.terminal_could_not_connect)
    override val invalidControl: String get() = context.getString(R.string.terminal_invalid_control)
    val storage: String get() = context.getString(R.string.storage_failed)
    val damaged: String get() = context.getString(R.string.runtime_damaged)
    val couldNotStart: String get() = context.getString(R.string.runtime_could_not_start)

    override fun rejected(code: Int): String = context.getString(R.string.did_not_work_code, code)

    fun unreadable(detail: String): String = context.getString(R.string.event_unreadable, detail)
}
