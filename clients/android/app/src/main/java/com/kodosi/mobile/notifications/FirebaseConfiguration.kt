package com.kodosi.mobile.notifications

import android.content.Context
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.kodosi.mobile.BuildConfig

object FirebaseConfiguration {
    val available: Boolean get() = BuildConfig.FIREBASE_APPLICATION_ID.isNotEmpty()

    fun initialize(context: Context) {
        if (!available) return
        FirebaseApp.initializeApp(
            context,
            FirebaseOptions.Builder()
                .setApplicationId(BuildConfig.FIREBASE_APPLICATION_ID)
                .setApiKey(BuildConfig.FIREBASE_API_KEY)
                .setProjectId(BuildConfig.FIREBASE_PROJECT_ID)
                .setGcmSenderId(BuildConfig.FIREBASE_SENDER_ID)
                .build()
        )
    }
}
