package com.kodosi.mobile.canvas

import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

class BrowserAddress(address: String) {
    val state = TextFieldState(shownAddress(address))
    private var shown by mutableStateOf(state.text.toString())

    val edited: Boolean get() = state.text.toString() != shown

    fun follow(address: String) {
        val page = shownAddress(address)
        if (!edited && page != shown) place(page)
    }

    fun place(address: String) {
        shown = address
        state.setTextAndPlaceCursorAtEnd(address)
    }
}

/** The blank page of a new tab leaves the field empty, ready for an address. */
private fun shownAddress(address: String) = if (address == BLANK_PAGE) "" else address

private const val BLANK_PAGE = "about:blank"
