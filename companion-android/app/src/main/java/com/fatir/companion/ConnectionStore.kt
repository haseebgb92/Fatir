package com.fatir.companion

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class ConnectionStore(context: Context) {
    private val prefs = context.getSharedPreferences("fatir_companion", Context.MODE_PRIVATE)

    init {
        migrateLegacyToken()
    }

    val baseUrl: String?
        get() = prefs.getString("base_url", null)

    val token: String?
        get() = prefs.getString("token_enc", null)?.let(::decrypt)

    fun save(baseUrl: String, token: String) {
        prefs.edit()
            .putString("base_url", baseUrl)
            .putString("token_enc", encrypt(token))
            .remove("token")
            .apply()
    }

    fun clear() {
        prefs.edit().clear().apply()
    }

    private fun migrateLegacyToken() {
        if (prefs.contains("token_enc")) return
        val legacy = prefs.getString("token", null)?.takeIf { it.isNotBlank() } ?: return
        prefs.edit()
            .putString("token_enc", encrypt(legacy))
            .remove("token")
            .apply()
    }

    private fun encrypt(value: String): String {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val ciphertext = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        val iv = Base64.encodeToString(cipher.iv, Base64.NO_WRAP)
        val body = Base64.encodeToString(ciphertext, Base64.NO_WRAP)
        return "v1:$iv:$body"
    }

    private fun decrypt(value: String): String? {
        return runCatching {
            val parts = value.split(':', limit = 3)
            require(parts.size == 3 && parts[0] == "v1")
            val iv = Base64.decode(parts[1], Base64.NO_WRAP)
            val ciphertext = Base64.decode(parts[2], Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv))
            String(cipher.doFinal(ciphertext), Charsets.UTF_8)
        }.getOrNull()
    }

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }

        val generator = KeyGenerator.getInstance(
            KeyProperties.KEY_ALGORITHM_AES,
            "AndroidKeyStore"
        )
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build()
        )
        return generator.generateKey()
    }

    companion object {
        private const val KEY_ALIAS = "fatir_companion_connection_key"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
    }
}
