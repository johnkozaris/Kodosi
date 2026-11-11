package com.kodosi.mobile

import android.app.Activity
import android.content.Intent
import android.os.Bundle

internal const val SIGN_IN_SCHEME = "com.kodosi"

class SignInCallbackActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val answer = intent.data?.takeIf { it.scheme == SIGN_IN_SCHEME }
        if (answer != null) {
            // Returning to MainActivity closes the browser and delivers its canceled result.
            (application as KodosiApplication).model.account.finishSignIn(answer.toString())
        }
        startActivity(
            Intent(this, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        )
        finish()
    }
}
