package com.kodosi.mobile.people

import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.string
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class PeopleStoreTest {
    private val sent = SentCommands()
    private val store = PeopleStore(sent.sink)

    @Test
    fun aBlankReportNoteGoesOutEmpty() = runTest {
        val reporting = async(start = CoroutineStart.UNDISPATCHED) { store.report("ana", " \n ") }
        val command = sent.last("friends.report")
        sent.reply("friends.report")
        reporting.await()
        assertEquals("", command.string("note"))
    }
}
