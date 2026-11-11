package com.kodosi.mobile.runtime

import android.annotation.SuppressLint
import android.content.Context
import android.content.SharedPreferences
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.IOException
import java.security.KeyStore
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@SuppressLint("UseKtx")
internal object KeystoreSecrets {
    private const val ALIAS = "com.kodosi.mobile.credentials"

    @Volatile
    private var credentials: SharedPreferences? = null

    fun initialize(context: Context) {
        credentials = context.applicationContext.getSharedPreferences("sealed-credentials", Context.MODE_PRIVATE)
    }

    private fun saved() = checkNotNull(credentials)

    @Synchronized
    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(
                KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setKeySize(256)
                    .setRandomizedEncryptionRequired(true)
                    .build()
            )
        }.generateKey()
    }

    @JvmStatic
    fun read(name: String): ByteArray? {
        val encoded = saved().getString(name, null) ?: return null
        val sealed = Base64.getDecoder().decode(encoded)
        if (sealed.size < 29 || sealed[0] != 1.toByte()) throw IOException("Unreadable credential")
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, sealed, 1, 12))
        cipher.updateAAD(name.encodeToByteArray())
        return cipher.doFinal(sealed, 13, sealed.size - 13)
    }

    @JvmStatic
    fun write(name: String, data: ByteArray) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        cipher.updateAAD(name.encodeToByteArray())
        val encoded = Base64.getEncoder().encodeToString(byteArrayOf(1) + cipher.iv + cipher.doFinal(data))
        // The KTX editor does not return whether a synchronous commit succeeded.
        if (!saved().edit().putString(name, encoded).commit()) throw IOException("Credential storage failed")
    }

    @JvmStatic
    fun delete(name: String) {
        if (!saved().edit().remove(name).commit()) throw IOException("Credential removal failed")
    }
}
