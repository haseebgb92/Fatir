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
    val session_id: String? = null,
    val text: String,
    val mode: String = "auto",
    val attachments: List<String> = emptyList()
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
    val model: String? = null
)

data class TransferItem(
    val title: String,
    val detail: String,
    val success: Boolean
)


@Serializable
data class ChatSummary(
    val session_id: String,
    val title: String,
    val preview: String = "",
    val message_count: Int = 0,
    val created_at: String? = null,
    val updated_at: String? = null,
    val source: String = "desktop"
)

@Serializable
data class ChatListResponse(
    val chats: List<ChatSummary> = emptyList()
)

@Serializable
data class ChatHistoryMessage(
    val role: String,
    val content: String
)

@Serializable
data class ChatHistoryResponse(
    val session_id: String,
    val messages: List<ChatHistoryMessage> = emptyList()
)

@Serializable
data class AgentScheduleItem(
    val id: String,
    val label: String,
    val prompt: String = "",
    val trigger_kind: String = "",
    val trigger: String = "",
    val calendar: String = "",
    val execution_mode: String = "agent",
    val credential_ids: List<String> = emptyList(),
    val verification_required: Boolean = true,
    val session_id: String = "",
    val created_at: String = "",
    val last_run_at: String? = null,
    val last_status: String? = null,
    val last_result: String? = null,
    val last_model: String? = null,
    val last_pending_action: String? = null,
    val timer_state: String = "",
    val next: String = ""
)

@Serializable
data class AgentScheduleListResponse(
    val schedules: List<AgentScheduleItem> = emptyList()
)

@Serializable
data class IdRequest(
    val id: String
)
