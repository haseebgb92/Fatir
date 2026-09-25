package com.fatir.companion

import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.MediaType
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.HttpUrl.Companion.toHttpUrl
import okio.BufferedSink

class FatirApi(
    rawBaseUrl: String,
    private val token: String
) {
    val baseUrl: String = normalizeBaseUrl(rawBaseUrl)

    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
    }

    private val client = OkHttpClient.Builder()
        // Browser/desktop tasks can legitimately take well over OkHttp's
        // 10-second default read timeout while Fatir is acting and verifying.
        .connectTimeout(12, TimeUnit.SECONDS)
        .readTimeout(10, TimeUnit.MINUTES)
        .writeTimeout(10, TimeUnit.MINUTES)
        .retryOnConnectionFailure(true)
        .build()

    suspend fun health(): HealthResponse = withContext(Dispatchers.IO) {
        val request = Request.Builder()
            .url("$baseUrl/api/v1/health")
            .get()
            .build()
        executeJson(request)
    }

    suspend fun chat(requestBody: ChatRequest): ChatEnvelope = withContext(Dispatchers.IO) {
        val request = authedRequest("$baseUrl/api/v1/chat")
            .post(json.encodeToString(requestBody).jsonBody())
            .build()
        executeJson(request)
    }

    suspend fun approve(actionId: String): AgentResponse = action("/api/v1/approve", actionId)

    suspend fun deny(actionId: String): AgentResponse = action("/api/v1/deny", actionId)


    suspend fun chats(): ChatListResponse = withContext(Dispatchers.IO) {
        executeJson(authedRequest("$baseUrl/api/v1/chats").get().build())
    }

    suspend fun chatHistory(sessionId: String): ChatHistoryResponse = withContext(Dispatchers.IO) {
        val url = "$baseUrl/api/v1/chats/history".toHttpUrl().newBuilder()
            .addQueryParameter("session_id", sessionId)
            .build()
        executeJson(authedRequest(url.toString()).get().build())
    }

    suspend fun agentSchedules(): AgentScheduleListResponse = withContext(Dispatchers.IO) {
        executeJson(authedRequest("$baseUrl/api/v1/agent-schedules").get().build())
    }

    suspend fun runAgentSchedule(id: String) = scheduleAction("/api/v1/agent-schedules/run", id)

    suspend fun approveAgentSchedule(id: String): AgentResponse = withContext(Dispatchers.IO) {
        val payload = json.encodeToString(IdRequest(id))
        val request = authedRequest("$baseUrl/api/v1/agent-schedules/approve")
            .post(payload.jsonBody())
            .build()
        executeJson(request)
    }

    suspend fun denyAgentSchedule(id: String): AgentResponse = withContext(Dispatchers.IO) {
        val payload = json.encodeToString(IdRequest(id))
        val request = authedRequest("$baseUrl/api/v1/agent-schedules/deny")
            .post(payload.jsonBody())
            .build()
        executeJson(request)
    }

    suspend fun cancelAgentSchedule(id: String) = scheduleAction("/api/v1/agent-schedules/cancel", id)

    suspend fun roots(): RootsResponse = withContext(Dispatchers.IO) {
        executeJson(authedRequest("$baseUrl/api/v1/files/roots").get().build())
    }

    suspend fun listFiles(path: String?): FileListResponse = withContext(Dispatchers.IO) {
        val url = "$baseUrl/api/v1/files/list".toHttpUrl().newBuilder().apply {
            if (!path.isNullOrBlank()) addQueryParameter("path", path)
        }.build()
        executeJson(authedRequest(url.toString()).get().build())
    }

    suspend fun download(path: String, output: OutputStream): Long = withContext(Dispatchers.IO) {
        val url = "$baseUrl/api/v1/files/download".toHttpUrl().newBuilder()
            .addQueryParameter("path", path)
            .build()
        client.newCall(authedRequest(url.toString()).get().build()).execute().use { response ->
            if (!response.isSuccessful) throw apiError(response.code, response.body?.string())
            val body = response.body ?: throw IOException("Empty download response")
            body.byteStream().use { input ->
                output.use { target ->
                    input.copyTo(target)
                }
            }
        }
    }

    suspend fun upload(
        directory: String?,
        filename: String,
        mime: String?,
        openStream: () -> InputStream?
    ): UploadResponse = withContext(Dispatchers.IO) {
        val url = "$baseUrl/api/v1/files/upload".toHttpUrl().newBuilder().apply {
            if (!directory.isNullOrBlank()) addQueryParameter("directory", directory)
        }.build()

        val body = StreamingRequestBody(mime?.toMediaTypeOrNull(), openStream)
        val request = authedRequest(url.toString())
            .header("X-Fatir-Filename", filename)
            .post(body)
            .build()
        executeJson(request)
    }

    private suspend fun scheduleAction(path: String, id: String): kotlinx.serialization.json.JsonElement = withContext(Dispatchers.IO) {
        val payload = json.encodeToString(IdRequest(id))
        val request = authedRequest("$baseUrl$path")
            .post(payload.jsonBody())
            .build()
        executeJson(request)
    }

    private suspend fun action(path: String, actionId: String): AgentResponse = withContext(Dispatchers.IO) {
        val payload = json.encodeToString(ActionRequest(actionId))
        val request = authedRequest("$baseUrl$path")
            .post(payload.jsonBody())
            .build()
        executeJson(request)
    }

    private fun authedRequest(url: String): Request.Builder =
        Request.Builder()
            .url(url)
            .header("Authorization", "Bearer $token")
            .header("Accept", "application/json")

    private inline fun <reified T> executeJson(request: Request): T {
        client.newCall(request).execute().use { response ->
            val raw = response.body?.string().orEmpty()
            if (!response.isSuccessful) throw apiError(response.code, raw)
            return json.decodeFromString(raw)
        }
    }

    private fun apiError(code: Int, body: String?): IOException {
        val text = body?.takeIf { it.isNotBlank() } ?: "HTTP $code"
        return IOException("Fatir returned $code: $text")
    }

    private fun String.jsonBody(): RequestBody =
        toRequestBody("application/json; charset=utf-8".toMediaTypeOrNull())

    private class StreamingRequestBody(
        private val type: MediaType?,
        private val openStream: () -> InputStream?
    ) : RequestBody() {
        override fun contentType(): MediaType? = type
        override fun contentLength(): Long = -1

        override fun writeTo(sink: BufferedSink) {
            val input = openStream() ?: throw IOException("Unable to open Android file")
            input.use { source ->
                val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                while (true) {
                    val count = source.read(buffer)
                    if (count < 0) break
                    sink.write(buffer, 0, count)
                }
            }
        }
    }

    companion object {
        fun normalizeBaseUrl(raw: String): String {
            var value = raw.trim().trimEnd('/')
            if (!value.startsWith("http://") && !value.startsWith("https://")) {
                value = "http://$value"
            }
            if (value.substringAfter("://").substringBefore('/').contains(':').not()) {
                value += ":32145"
            }
            return value
        }
    }
}
