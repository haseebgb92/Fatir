package com.fatir.companion

import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
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

    private val controlClient = OkHttpClient.Builder()
        // Chat work is detached from a single HTTP request and polled by request
        // id, so control sockets can fail fast and reconnect without losing work.
        .connectTimeout(12, TimeUnit.SECONDS)
        .readTimeout(35, TimeUnit.SECONDS)
        .writeTimeout(35, TimeUnit.SECONDS)
        .retryOnConnectionFailure(true)
        .build()

    private val transferClient = OkHttpClient.Builder()
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

    suspend fun chatResumable(
        requestBody: ChatRequest,
        onStatus: (ChatRunStatus) -> Unit = {}
    ): ChatEnvelope {
        val requestId = requestBody.request_id ?: UUID.randomUUID().toString()
        val prepared = requestBody.copy(request_id = requestId)

        var initial: ChatRunStatus? = null
        var submitAttempt = 0
        while (initial == null) {
            try {
                initial = submitChat(prepared)
            } catch (e: IOException) {
                submitAttempt++
                if (submitAttempt >= 5) throw e
                onStatus(
                    ChatRunStatus(
                        request_id = requestId,
                        session_id = prepared.session_id.orEmpty(),
                        status = "reconnecting",
                        message = "Connection interrupted — reconnecting…"
                    )
                )
                delay(backoffMillis(submitAttempt))
            }
        }

        var current = initial ?: throw IOException("Fatir did not acknowledge the request")
        onStatus(current)
        var pollFailures = 0
        while (true) {
            when (current.status) {
                "completed" -> {
                    val response = current.response ?: throw IOException("Fatir completed without a response")
                    return ChatEnvelope(current.session_id, response)
                }
                "error" -> throw IOException(current.error ?: current.message.ifBlank { "Fatir hit a problem" })
                "timeout" -> throw IOException(current.error ?: "FATIR_TIMEOUT")
                "cancelled" -> throw IOException("FATIR_STOPPED")
            }

            delay(900)
            try {
                current = chatRun(requestId)
                pollFailures = 0
                onStatus(current)
            } catch (e: IOException) {
                pollFailures++
                onStatus(current.copy(status = "reconnecting", message = "Connection interrupted — reconnecting…"))
                delay(backoffMillis(pollFailures))
            }
        }
    }

    suspend fun submitChat(requestBody: ChatRequest): ChatRunStatus = withContext(Dispatchers.IO) {
        val request = authedRequest("$baseUrl/api/v1/chat/runs")
            .post(json.encodeToString(requestBody).jsonBody())
            .build()
        executeJson(request)
    }

    suspend fun chatRun(requestId: String): ChatRunStatus = withContext(Dispatchers.IO) {
        executeJson(
            authedRequest("$baseUrl/api/v1/chat/runs/$requestId")
                .get()
                .build()
        )
    }

    suspend fun cancelChatRun(requestId: String): ChatRunStatus = withContext(Dispatchers.IO) {
        executeJson(
            authedRequest("$baseUrl/api/v1/chat/runs/$requestId/cancel")
                .post(ByteArray(0).toRequestBody(null))
                .build()
        )
    }


    suspend fun chatSessions(limit: Int = 50): ChatSessionsResponse = withContext(Dispatchers.IO) {
        val url = "$baseUrl/api/v1/chats".toHttpUrl().newBuilder()
            .addQueryParameter("limit", limit.coerceIn(1, 200).toString())
            .build()
        executeJson(authedRequest(url.toString()).get().build())
    }

    suspend fun chatMessages(sessionId: String, limit: Int = 300): ChatMessagesResponse = withContext(Dispatchers.IO) {
        val url = "$baseUrl/api/v1/chats/messages".toHttpUrl().newBuilder()
            .addQueryParameter("session_id", sessionId)
            .addQueryParameter("limit", limit.coerceIn(1, 1000).toString())
            .build()
        executeJson(authedRequest(url.toString()).get().build())
    }

    suspend fun agentSchedules(): AgentSchedulesResponse = withContext(Dispatchers.IO) {
        executeJson(authedRequest("$baseUrl/api/v1/agent/schedules").get().build())
    }

    suspend fun runAgentSchedule(id: String): JsonObject = withContext(Dispatchers.IO) {
        val url = "$baseUrl/api/v1/agent/schedule/run".toHttpUrl().newBuilder()
            .addQueryParameter("id", id)
            .build()
        val request = authedRequest(url.toString())
            .post(ByteArray(0).toRequestBody(null))
            .build()
        executeJson(request)
    }

    suspend fun approve(actionId: String): AgentResponse = action("/api/v1/approve", actionId)

    suspend fun deny(actionId: String): AgentResponse = action("/api/v1/deny", actionId)

    suspend fun remoteDesktopStatus(): RemoteDesktopStatus = withContext(Dispatchers.IO) {
        executeJson(authedRequest("$baseUrl/api/v1/remote/desktop/status").get().build())
    }

    suspend fun startRemoteDesktop(): RemoteDesktopStatus = withContext(Dispatchers.IO) {
        executeJson(
            authedRequest("$baseUrl/api/v1/remote/desktop/start")
                .post(ByteArray(0).toRequestBody(null))
                .build()
        )
    }

    suspend fun stopRemoteDesktop(): RemoteDesktopStatus = withContext(Dispatchers.IO) {
        executeJson(
            authedRequest("$baseUrl/api/v1/remote/desktop/stop")
                .post(ByteArray(0).toRequestBody(null))
                .build()
        )
    }

    fun remoteDesktopWebSocketUrl(): String {
        val http = "$baseUrl/api/v1/remote/desktop/ws".toHttpUrl()
        val scheme = if (http.isHttps) "wss" else "ws"
        return http.newBuilder().scheme(scheme).build().toString()
    }

    fun authorizationHeader(): String = "Bearer $token"

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
        transferClient.newCall(authedRequest(url.toString()).get().build()).execute().use { response ->
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
        executeJson(request, transferClient)
    }

    suspend fun downloadToFile(path: String, file: File): File = withContext(Dispatchers.IO) {
        file.parentFile?.mkdirs()
        FileOutputStream(file).use { output -> download(path, output) }
        file
    }

    fun mediaUrl(path: String): String =
        "$baseUrl/api/v1/files/download".toHttpUrl().newBuilder()
            .addQueryParameter("path", path)
            .build()
            .toString()

    fun mediaHeaders(): Map<String, String> = mapOf("Authorization" to "Bearer $token")

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

    private inline fun <reified T> executeJson(
        request: Request,
        httpClient: OkHttpClient = controlClient
    ): T {
        httpClient.newCall(request).execute().use { response ->
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

    private fun backoffMillis(attempt: Int): Long {
        val power = (attempt - 1).coerceIn(0, 3)
        return (1_000L shl power).coerceAtMost(8_000L)
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
