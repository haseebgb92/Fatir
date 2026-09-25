package com.fatir.companion

import android.content.Context

class ConnectionStore(context: Context) {
    private val prefs = context.getSharedPreferences("fatir_companion", Context.MODE_PRIVATE)

    val baseUrl: String?
        get() = prefs.getString("base_url", null)

    val token: String?
        get() = prefs.getString("token", null)

    fun save(baseUrl: String, token: String) {
        prefs.edit()
            .putString("base_url", baseUrl)
            .putString("token", token)
            .apply()
    }

    fun clear() {
        prefs.edit().clear().apply()
    }
}
