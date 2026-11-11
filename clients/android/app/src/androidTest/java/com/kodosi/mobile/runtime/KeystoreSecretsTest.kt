package com.kodosi.mobile.runtime

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.security.GeneralSecurityException
import java.util.UUID
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class KeystoreSecretsTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val names = mutableListOf<String>()

    @Before
    fun initialize() = KeystoreSecrets.initialize(context)

    @After
    fun removeOnlyTheTestEntries() = names.forEach(KeystoreSecrets::delete)

    @Test
    fun aCredentialSurvivesReopeningWithoutPlaintextStorage() {
        val name = name()
        val value = "test-only credential".encodeToByteArray()
        KeystoreSecrets.write(name, value)
        KeystoreSecrets.initialize(context)
        assertArrayEquals(value, KeystoreSecrets.read(name))
        val sealed = context.getSharedPreferences("sealed-credentials", Context.MODE_PRIVATE).getString(name, null)
        assertFalse(sealed.orEmpty().contains(value.decodeToString()))
        KeystoreSecrets.delete(name)
        assertNull(KeystoreSecrets.read(name))
    }

    @Test
    fun aCredentialCannotBeSubstitutedForAnotherName() {
        val first = name()
        val second = name()
        KeystoreSecrets.write(first, byteArrayOf(1, 2, 3))
        val saved = context.getSharedPreferences("sealed-credentials", Context.MODE_PRIVATE)
        assertTrue(saved.edit().putString(second, saved.getString(first, null)).commit())
        assertTrue(runCatching { KeystoreSecrets.read(second) }.exceptionOrNull() is GeneralSecurityException)
    }

    private fun name(): String = "instrumentation-${UUID.randomUUID()}".also(names::add)
}
