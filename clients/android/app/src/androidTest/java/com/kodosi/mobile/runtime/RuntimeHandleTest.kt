package com.kodosi.mobile.runtime

import android.content.Context
import android.content.ContextWrapper
import android.content.SharedPreferences
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.kodosi.mobile.AndroidWords
import java.io.File
import java.util.UUID
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class RuntimeHandleTest {
    @Test
    fun theViewerStartsWithTheAndroidEnvironmentAndRequestsSignIn() = runBlocking {
        val target = InstrumentationRegistry.getInstrumentation().targetContext
        val namespace = "runtime-test-${UUID.randomUUID()}"
        val directory = File(target.cacheDir, namespace)
        val saved = mutableSetOf<String>()
        val context = object : ContextWrapper(target) {
            override fun getApplicationContext(): Context = this

            override fun getNoBackupFilesDir(): File = directory

            override fun getSharedPreferences(name: String, mode: Int): SharedPreferences {
                val isolated = "$namespace-$name"
                saved += isolated
                return target.getSharedPreferences(isolated, mode)
            }
        }
        try {
            RuntimeEnvironment.install(context)
            for (label in listOf(null, "Android \uD83D\uDCF1")) {
                if (label != null) {
                    assertTrue(NativeRuntime.setEnvironment("KODOSI_DEVICE_LABEL", label.encodeToByteArray()))
                }
                val runtime = RuntimeHandle()
                try {
                    assertEquals(RuntimeHandle.Start.Started, runtime.start(context, "en"))
                    val commands = CommandSink(AndroidWords(context)).apply { transport = runtime::send }
                    var ready = false
                    var signIn = false
                    withTimeout(60_000) {
                        while (!ready || !signIn) {
                            val event = RuntimeEvent.parse(runtime.events.receive())
                            when (event.type) {
                                "system.ready" -> ready = true

                                "auth.required" -> {
                                    signIn = true
                                    commands.setAccount(event.accountUserId, event.accountEpoch)
                                }

                                "system.error" -> error(event.string("message").orEmpty())
                            }
                        }
                        val request = UuidV7.generate()
                        assertEquals(Ffi.OK, commands.send("auth.login.cancel", fields("requestId" to request)))
                        withTimeout(60_000) {
                            while (true) {
                                val event = RuntimeEvent.parse(runtime.events.receive())
                                if (event.string("requestId") != request) continue
                                if (event.type == "auth.error") error(event.string("message").orEmpty())
                                if (event.type == "auth.result") break
                            }
                        }
                    }
                } finally {
                    runtime.stop()
                }
            }
        } finally {
            saved.forEach(target::deleteSharedPreferences)
            directory.deleteRecursively()
        }
    }
}
