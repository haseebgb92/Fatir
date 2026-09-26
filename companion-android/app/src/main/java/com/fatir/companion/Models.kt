package com.fatir.companion

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

@Serializable
data class HealthResponse(
    val ok: Boolean,
    val name: String,
    val version: String,
    val device_name: String
)

@Serializable
data class RemoteDesktopStatus(
    val available: Boolean,
    val running: Boolean,
    val backend: String,
    val display: String,
    val message: String
)

@Serializable
data class FileEntry(
    val name: String,
    val path: String,
    val is_dir: Boolean,
    val size: Long = 0,
    val modified_unix: Long? = null,
    val mime: String? = null
)

@Serializable
data class FileListResponse(
    val path: String? = null,
    val entries: List<FileEntry> = emptyList()
)

@Serializable
data class RootsResponse(
    val roots: List<String> = emptyList()
)

@Serializable
data class ResourceRef(
    val label: String,
    val path: String,
    val kind: String
)

@Serializable
data class TraceItem(
    val title: String,
    val detail: String,
    val status: String,
    val resources: List<ResourceRef> = emptyList()
)

@Serializable
data class PendingAction(
    val id: String,
    val tool: String,
    val arguments: JsonElement = JsonObject(emptyMap()),
    val risk: String,
    val summary: String
)

@Serializable
data class AgentResponse(
    val text: String,
    val model: String,
    val trace: List<TraceItem> = emptyList(),
    val pending: PendingAction? = null
)

@Serializable
data class ChatEnvelope(
    val session_id: String,
    val response: AgentResponse
)

@Serializable
data class ChatRequest(
    val request_id: String? = null,
    val session_id: String? = null,
    val text: String,
    val mode: String = "auto",
    val attachments: List<String> = emptyList()
)

@Serializable
data class ChatRunStatus(
    val request_id: String,
    val session_id: String,
    val status: String,
    val message: String = "",
    val updated_unix: Long = 0,
    val response: AgentResponse? = null,
    val error: String? = null
)

@Serializable
data class ActionRequest(
    val action_id: String,
    val mode: String = "auto"
)

@Serializable
data class UploadResponse(
    val ok: Boolean,
    val path: String,
    val bytes: Long,
    val name: String
)

data class UiMessage(
    val fromUser: Boolean,
    val text: String,
    val model: String? = null,
    val attachments: List<ResourceRef> = emptyList()
)

data class TransferItem(
    val title: String,
    val detail: String,
    val success: Boolean
)


@Serializable
data class ChatSessionSummary(
    val session_id: String,
    val title: String,
    val preview: String,
    val updated_at: String,
    val messages: Int
)

@Serializable
data class ChatSessionsResponse(
    val sessions: List<ChatSessionSummary> = emptyList()
)

@Serializable
data class ChatTurn(
    val id: String,
    val session_id: String,
    val role: String,
    val text: String,
    val model: String? = null,
    val resources: List<ResourceRef> = emptyList(),
    val created_at: String
)

@Serializable
data class ChatMessagesResponse(
    val session_id: String,
    val messages: List<ChatTurn> = emptyList()
)

@Serializable
data class AgentScheduleItem(
    val id: String,
    val label: String,
    val prompt: String = "",
    val trigger_kind: String = "",
    val trigger: String = "",
    val browser_mode: String = "none",
    val credential_ids: List<String> = emptyList(),
    val timer_state: String = "unknown",
    val next: String = "",
    val created_at: String = "",
    val last_run_at: String? = null,
    val last_status: String? = null,
    val last_result: String? = null,
    val last_session_id: String? = null
)

@Serializable
data class AgentSchedulesResponse(
    val schedules: List<AgentScheduleItem> = emptyList()
)
