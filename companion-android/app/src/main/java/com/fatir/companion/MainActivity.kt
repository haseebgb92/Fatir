package com.fatir.companion

import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import java.text.DecimalFormat

private val FatirGold = Color(0xFFB48A3C)
private val FatirGoldSoft = Color(0xFFF2E6C9)
private val FatirCream = Color(0xFFF7F3EA)
private val FatirSurface = Color(0xFFFFFCF6)
private val FatirInk = Color(0xFF282823)
private val FatirMuted = Color(0xFF777368)
private val FatirGreen = Color(0xFF2F8D5B)
private val FatirRed = Color(0xFFB94A48)
private val FatirBorder = Color(0xFFE8E1D3)

private enum class Screen { CHAT, CHATS, SCHEDULES, FILES, TRANSFERS, SETTINGS }

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { FatirTheme { FatirApp() } }
    }
}

@Composable
private fun FatirTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = lightColorScheme(
            primary = FatirGold,
            onPrimary = Color.White,
            secondary = FatirGreen,
            background = FatirCream,
            onBackground = FatirInk,
            surface = FatirSurface,
            onSurface = FatirInk,
            surfaceVariant = Color(0xFFF1ECE2),
            onSurfaceVariant = FatirMuted,
            outline = FatirBorder,
            error = FatirRed
        ),
        content = content
    )
}

@Composable
private fun FatirApp() {
    val context = androidx.compose.ui.platform.LocalContext.current
    val resolver = context.contentResolver
    val scope = rememberCoroutineScope()
    val store = remember { ConnectionStore(context) }

    var address by remember { mutableStateOf(store.baseUrl.orEmpty()) }
    var token by remember { mutableStateOf(store.token.orEmpty()) }
    var api by remember { mutableStateOf<FatirApi?>(null) }
    var health by remember { mutableStateOf<HealthResponse?>(null) }
    var connecting by remember { mutableStateOf(false) }
    var connectionError by remember { mutableStateOf<String?>(null) }

    var screen by remember { mutableStateOf(Screen.CHAT) }
    val drawer = rememberDrawerState(DrawerValue.Closed)

    val messages = remember { mutableStateListOf<UiMessage>() }
    var sessionId by remember { mutableStateOf<String?>(null) }
    var pending by remember { mutableStateOf<PendingAction?>(null) }
    var composer by remember { mutableStateOf("") }
    var sending by remember { mutableStateOf(false) }
    val linuxAttachments = remember { mutableStateListOf<FileEntry>() }

    var currentPath by remember { mutableStateOf<String?>(null) }
    var history by remember { mutableStateOf<List<String?>>(emptyList()) }
    var entries by remember { mutableStateOf<List<FileEntry>>(emptyList()) }
    var fileLoading by remember { mutableStateOf(false) }
    var fileError by remember { mutableStateOf<String?>(null) }
    var fileRefresh by remember { mutableIntStateOf(0) }

    var chatRows by remember { mutableStateOf<List<ChatSummary>>(emptyList()) }
    var chatLoading by remember { mutableStateOf(false) }
    var chatError by remember { mutableStateOf<String?>(null) }
    var chatRefresh by remember { mutableIntStateOf(0) }

    var scheduleRows by remember { mutableStateOf<List<AgentScheduleItem>>(emptyList()) }
    var scheduleLoading by remember { mutableStateOf(false) }
    var scheduleError by remember { mutableStateOf<String?>(null) }
    var scheduleRefresh by remember { mutableIntStateOf(0) }

    val transfers = remember { mutableStateListOf<TransferItem>() }
    var pendingDownload by remember { mutableStateOf<FileEntry?>(null) }

    suspend fun connectNow(url: String, key: String, save: Boolean) {
        connecting = true
        connectionError = null
        try {
            val candidate = FatirApi(url, key)
            val result = candidate.health()
            if (!result.ok) error("Fatir did not report ready")
            candidate.roots() // authenticated probe: reject a wrong token before entering the app
            api = candidate
            health = result
            if (save) store.save(candidate.baseUrl, key.trim())
        } catch (t: Throwable) {
            api = null
            health = null
            connectionError = t.message ?: "Unable to connect"
        } finally {
            connecting = false
        }
    }

    LaunchedEffect(Unit) {
        val savedUrl = store.baseUrl
        val savedToken = store.token
        if (!savedUrl.isNullOrBlank() && !savedToken.isNullOrBlank()) {
            address = savedUrl
            token = savedToken
            connectNow(savedUrl, savedToken, false)
        }
    }

    val uploadPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null || api == null) return@rememberLauncherForActivityResult
        scope.launch {
            val name = displayName(context, uri) ?: "phone-file"
            try {
                val result = api!!.upload(
                    directory = currentPath,
                    filename = name,
                    mime = resolver.getType(uri),
                    openStream = { resolver.openInputStream(uri) }
                )
                transfers.add(0, TransferItem("Uploaded " + name, result.path, true))
                fileRefresh++
            } catch (t: Throwable) {
                transfers.add(0, TransferItem("Upload failed: " + name, t.message.orEmpty(), false))
            }
        }
    }

    val downloadPicker = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("*/*")) { uri ->
        val selected = pendingDownload
        pendingDownload = null
        if (uri == null || selected == null || api == null) return@rememberLauncherForActivityResult
        scope.launch {
            try {
                val output = resolver.openOutputStream(uri) ?: error("Android could not create the file")
                val bytes = api!!.download(selected.path, output)
                transfers.add(0, TransferItem("Downloaded " + selected.name, humanBytes(bytes), true))
            } catch (t: Throwable) {
                transfers.add(0, TransferItem("Download failed: " + selected.name, t.message.orEmpty(), false))
            }
        }
    }

    if (api == null || health == null) {
        ConnectionScreen(
            address, token, connecting, connectionError,
            { address = it }, { token = it },
            { scope.launch { connectNow(address, token, true) } }
        )
        return
    }

    LaunchedEffect(screen, currentPath, fileRefresh) {
        if (screen == Screen.FILES) {
            fileLoading = true
            fileError = null
            try {
                entries = api!!.listFiles(currentPath).entries
            } catch (t: Throwable) {
                entries = emptyList()
                fileError = t.message ?: "Unable to read Linux files"
            } finally {
                fileLoading = false
            }
        }
    }

    LaunchedEffect(screen, chatRefresh) {
        if (screen == Screen.CHATS) {
            chatLoading = true
            chatError = null
            try {
                chatRows = api!!.chats().chats
            } catch (t: Throwable) {
                chatRows = emptyList()
                chatError = t.message ?: "Unable to load Fatir chats"
            } finally {
                chatLoading = false
            }
        }
    }

    LaunchedEffect(screen, scheduleRefresh) {
        if (screen == Screen.SCHEDULES) {
            scheduleLoading = true
            scheduleError = null
            try {
                scheduleRows = api!!.agentSchedules().schedules
            } catch (t: Throwable) {
                scheduleRows = emptyList()
                scheduleError = t.message ?: "Unable to load scheduled agents"
            } finally {
                scheduleLoading = false
            }
        }
    }

    fun send() {
        val text = composer.trim()
        if (text.isBlank() || sending) return
        composer = ""
        messages += UiMessage(true, text)
        val attached = linuxAttachments.map { it.path }
        linuxAttachments.clear()
        sending = true
        scope.launch {
            try {
                val result = api!!.chat(ChatRequest(sessionId, text, "auto", attached))
                sessionId = result.session_id
                messages += UiMessage(false, result.response.text, result.response.model)
                pending = result.response.pending
                chatRefresh++
            } catch (t: Throwable) {
                messages += UiMessage(false, "Connection error: " + t.message.orEmpty())
            } finally {
                sending = false
            }
        }
    }

    ModalNavigationDrawer(
        drawerState = drawer,
        drawerContent = {
            Drawer(
                device = health!!.device_name,
                selected = screen,
                onSelect = {
                    screen = it
                    scope.launch { drawer.close() }
                }
            )
        }
    ) {
        Box(Modifier.fillMaxSize().background(FatirCream)) {
            when (screen) {
                Screen.CHAT -> ChatScreen(
                    messages, pending, sending, linuxAttachments,
                    onApprove = { action ->
                        scope.launch {
                            try {
                                val response = api!!.approve(action.id)
                                messages += UiMessage(false, response.text, response.model)
                                pending = response.pending
                            } catch (t: Throwable) {
                                messages += UiMessage(false, "Approval failed: " + t.message.orEmpty())
                            }
                        }
                    },
                    onDeny = { action ->
                        scope.launch {
                            try {
                                val response = api!!.deny(action.id)
                                messages += UiMessage(false, response.text, response.model)
                                pending = response.pending
                            } catch (t: Throwable) {
                                messages += UiMessage(false, "Deny failed: " + t.message.orEmpty())
                            }
                        }
                    },
                    onRemoveAttachment = { linuxAttachments.remove(it) }
                )


                Screen.CHATS -> ChatHistoryScreen(
                    chats = chatRows,
                    loading = chatLoading,
                    error = chatError,
                    onRefresh = { chatRefresh++ },
                    onNewChat = {
                        sessionId = null
                        messages.clear()
                        pending = null
                        composer = ""
                        screen = Screen.CHAT
                    },
                    onOpen = { chat ->
                        scope.launch {
                            try {
                                val saved = api!!.chatHistory(chat.session_id)
                                sessionId = saved.session_id
                                messages.clear()
                                saved.messages.forEach { msg ->
                                    messages += UiMessage(msg.role == "user", msg.content, null)
                                }
                                pending = null
                                screen = Screen.CHAT
                            } catch (t: Throwable) {
                                chatError = t.message ?: "Unable to open chat"
                            }
                        }
                    }
                )

                Screen.SCHEDULES -> AgentSchedulesScreen(
                    schedules = scheduleRows,
                    loading = scheduleLoading,
                    error = scheduleError,
                    onRefresh = { scheduleRefresh++ },
                    onOpenChat = { schedule ->
                        scope.launch {
                            try {
                                val saved = api!!.chatHistory(schedule.session_id)
                                sessionId = saved.session_id
                                messages.clear()
                                saved.messages.forEach { msg ->
                                    messages += UiMessage(msg.role == "user", msg.content, null)
                                }
                                pending = null
                                screen = Screen.CHAT
                            } catch (t: Throwable) {
                                scheduleError = t.message ?: "Unable to open scheduled task chat"
                            }
                        }
                    },
                    onRunNow = { schedule ->
                        scope.launch {
                            try {
                                api!!.runAgentSchedule(schedule.id)
                                scheduleRefresh++
                            } catch (t: Throwable) {
                                scheduleError = t.message ?: "Unable to run schedule"
                            }
                        }
                    },
                    onApprovePending = { schedule ->
                        val actionId = schedule.last_pending_action
                        if (!actionId.isNullOrBlank()) {
                            scope.launch {
                                try {
                                    val response = api!!.approve(actionId)
                                    sessionId = schedule.session_id
                                    messages.clear()
                                    val saved = api!!.chatHistory(schedule.session_id)
                                    saved.messages.forEach { msg ->
                                        messages += UiMessage(msg.role == "user", msg.content, null)
                                    }
                                    messages += UiMessage(false, response.text, response.model)
                                    pending = response.pending
                                    scheduleRefresh++
                                    chatRefresh++
                                    screen = Screen.CHAT
                                } catch (t: Throwable) {
                                    scheduleError = t.message ?: "Unable to approve scheduled action"
                                }
                            }
                        }
                    },
                    onCancel = { schedule ->
                        scope.launch {
                            try {
                                api!!.cancelAgentSchedule(schedule.id)
                                scheduleRefresh++
                            } catch (t: Throwable) {
                                scheduleError = t.message ?: "Unable to cancel schedule"
                            }
                        }
                    }
                )

                Screen.FILES -> FilesScreen(
                    currentPath, entries, fileLoading, fileError, history.isNotEmpty(),
                    onBack = {
                        currentPath = history.lastOrNull()
                        history = if (history.isEmpty()) history else history.dropLast(1)
                    },
                    onOpen = {
                        if (it.is_dir) {
                            history = history + currentPath
                            currentPath = it.path
                        }
                    },
                    onAttach = {
                        if (linuxAttachments.none { f -> f.path == it.path }) linuxAttachments += it
                        screen = Screen.CHAT
                    },
                    onDownload = {
                        pendingDownload = it
                        downloadPicker.launch(it.name)
                    },
                    onUpload = { uploadPicker.launch(arrayOf("*/*")) }
                )

                Screen.TRANSFERS -> TransfersScreen(transfers)
                Screen.SETTINGS -> SettingsScreen(
                    health = health!!,
                    baseUrl = api!!.baseUrl,
                    onDisconnect = {
                        store.clear()
                        api = null
                        health = null
                        sessionId = null
                        pending = null
                    }
                )
            }

            TopFloatingBar(
                device = health!!.device_name,
                onMenu = { scope.launch { drawer.open() } },
                modifier = Modifier.align(Alignment.TopCenter)
            )

            if (screen == Screen.CHAT) {
                Composer(
                    composer, { composer = it }, !sending, ::send,
                    onPhoneFile = { uploadPicker.launch(arrayOf("*/*")) },
                    onLinuxFiles = { screen = Screen.FILES },
                    modifier = Modifier.align(Alignment.BottomCenter)
                )
            }
        }
    }
}

@Composable
private fun ConnectionScreen(
    address: String,
    token: String,
    connecting: Boolean,
    error: String?,
    onAddress: (String) -> Unit,
    onToken: (String) -> Unit,
    onConnect: () -> Unit
) {
    Box(Modifier.fillMaxSize().background(FatirCream).padding(28.dp)) {
        Column(
            Modifier.align(Alignment.Center).fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Image(
                painter = painterResource(R.drawable.fatir_mark),
                contentDescription = "Fatir",
                modifier = Modifier.size(86.dp)
            )
            Text("Fatir Companion", fontSize = 24.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 8.dp))
            Text("Your Linux assistant, wherever you are.", color = FatirMuted, modifier = Modifier.padding(top = 5.dp, bottom = 26.dp))

            Surface(
                shape = RoundedCornerShape(24.dp),
                color = FatirSurface,
                border = BorderStroke(1.dp, FatirBorder),
                shadowElevation = 2.dp
            ) {
                Column(Modifier.padding(20.dp)) {
                    OutlinedTextField(
                        value = address,
                        onValueChange = onAddress,
                        label = { Text("Fatir address") },
                        placeholder = { Text("100.x.x.x:32145 or 192.168.x.x:32145") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth()
                    )
                    Spacer(Modifier.height(12.dp))
                    OutlinedTextField(
                        value = token,
                        onValueChange = onToken,
                        label = { Text("Device token") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth()
                    )
                    if (!error.isNullOrBlank()) {
                        Text(error, color = FatirRed, fontSize = 12.sp, modifier = Modifier.padding(top = 10.dp))
                    }
                    Button(
                        onClick = onConnect,
                        enabled = !connecting && address.isNotBlank() && token.isNotBlank(),
                        shape = RoundedCornerShape(14.dp),
                        modifier = Modifier.fillMaxWidth().padding(top = 18.dp).height(52.dp)
                    ) {
                        if (connecting) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp, color = Color.White)
                        else {
                            Icon(Icons.Outlined.Link, null)
                            Spacer(Modifier.width(8.dp))
                            Text("Connect to Fatir")
                        }
                    }
                }
            }
            Text("Works over your private LAN or Tailscale connection", color = FatirMuted, fontSize = 12.sp, modifier = Modifier.padding(top = 18.dp))
        }
    }
}

@Composable
private fun Drawer(device: String, selected: Screen, onSelect: (Screen) -> Unit) {
    ModalDrawerSheet(drawerContainerColor = FatirSurface, modifier = Modifier.width(310.dp)) {
        Column(Modifier.padding(22.dp)) {
            Image(
                painter = painterResource(R.drawable.fatir_mark),
                contentDescription = "Fatir",
                modifier = Modifier.size(54.dp)
            )
            Text("Fatir", fontSize = 20.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 8.dp))
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 8.dp, bottom = 22.dp)) {
                Box(Modifier.size(8.dp).background(FatirGreen, CircleShape))
                Spacer(Modifier.width(8.dp))
                Text(device, color = FatirMuted, fontSize = 13.sp)
            }
            DrawerItem("Chat", Icons.Outlined.Chat, Screen.CHAT, selected, onSelect)
            DrawerItem("Chats", Icons.Outlined.History, Screen.CHATS, selected, onSelect)
            DrawerItem("Schedules", Icons.Outlined.Schedule, Screen.SCHEDULES, selected, onSelect)
            DrawerItem("Linux Files", Icons.Outlined.Folder, Screen.FILES, selected, onSelect)
            DrawerItem("Transfers", Icons.Outlined.SwapVert, Screen.TRANSFERS, selected, onSelect)
            DrawerItem("Settings", Icons.Outlined.Settings, Screen.SETTINGS, selected, onSelect)
        }
    }
}

@Composable
private fun DrawerItem(label: String, icon: ImageVector, screen: Screen, selected: Screen, onSelect: (Screen) -> Unit) {
    NavigationDrawerItem(
        label = { Text(label) },
        selected = selected == screen,
        onClick = { onSelect(screen) },
        icon = { Icon(icon, null) },
        colors = NavigationDrawerItemDefaults.colors(
            selectedContainerColor = FatirGoldSoft,
            selectedTextColor = FatirInk,
            selectedIconColor = FatirGold
        ),
        shape = RoundedCornerShape(14.dp),
        modifier = Modifier.padding(vertical = 3.dp)
    )
}

@Composable
private fun TopFloatingBar(device: String, onMenu: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier
            .fillMaxWidth()
            .statusBarsPadding(),
        color = FatirCream,
        tonalElevation = 0.dp,
        shadowElevation = 0.dp
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(68.dp)
                .padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            IconButton(onClick = onMenu, modifier = Modifier.size(44.dp)) {
                Icon(Icons.Outlined.Menu, "Menu", tint = FatirInk)
            }
            Image(
                painter = painterResource(R.drawable.fatir_mark),
                contentDescription = "Fatir",
                modifier = Modifier
                    .padding(start = 4.dp)
                    .size(42.dp)
            )
            Column(Modifier.weight(1f).padding(start = 10.dp)) {
                Text("Fatir", fontWeight = FontWeight.SemiBold, fontSize = 17.sp)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(7.dp).background(FatirGreen, CircleShape))
                    Spacer(Modifier.width(6.dp))
                    Text(
                        device,
                        color = FatirMuted,
                        fontSize = 11.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                }
            }
            Surface(
                shape = RoundedCornerShape(999.dp),
                color = Color(0xFFE8F3EC)
            ) {
                Text(
                    "Connected",
                    color = FatirGreen,
                    fontSize = 10.sp,
                    fontWeight = FontWeight.Medium,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp)
                )
            }
        }
    }
}

@Composable
private fun ChatScreen(
    messages: List<UiMessage>,
    pending: PendingAction?,
    sending: Boolean,
    attachments: List<FileEntry>,
    onApprove: (PendingAction) -> Unit,
    onDeny: (PendingAction) -> Unit,
    onRemoveAttachment: (FileEntry) -> Unit
) {
    val state = rememberLazyListState()
    LaunchedEffect(messages.size, pending, sending) {
        val count = messages.size + (if (pending != null) 1 else 0)
        if (count > 0) state.animateScrollToItem(count - 1)
    }

    LazyColumn(
        state = state,
        modifier = Modifier.fillMaxSize().padding(top = 104.dp, bottom = 112.dp),
        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        if (messages.isEmpty()) item {
            InfoCard("Your Linux Fatir, from here.", "Ask Fatir to work on the PC, approve actions, attach Linux files, or send something from this phone.")
        }
        items(messages) { MessageBubble(it) }
        if (pending != null) item { ApprovalCard(pending, onApprove, onDeny) }
        if (sending) item {
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(12.dp)) {
                CircularProgressIndicator(Modifier.size(17.dp), strokeWidth = 2.dp)
                Spacer(Modifier.width(10.dp))
                Text("Fatir is working…", color = FatirMuted, fontSize = 13.sp)
            }
        }
        if (attachments.isNotEmpty()) item {
            Column {
                Text("Attached for next message", color = FatirMuted, fontSize = 12.sp)
                attachments.forEach { file ->
                    AssistChip(
                        onClick = { onRemoveAttachment(file) },
                        label = { Text(file.name, maxLines = 1) },
                        leadingIcon = { Icon(Icons.Outlined.InsertDriveFile, null, Modifier.size(17.dp)) },
                        trailingIcon = { Icon(Icons.Outlined.Close, "Remove", Modifier.size(16.dp)) }
                    )
                }
            }
        }
    }
}

@Composable
private fun MessageBubble(message: UiMessage) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = if (message.fromUser) Arrangement.End else Arrangement.Start) {
        Surface(
            shape = RoundedCornerShape(18.dp),
            color = if (message.fromUser) FatirGoldSoft else FatirSurface,
            border = if (message.fromUser) null else BorderStroke(1.dp, FatirBorder),
            modifier = Modifier.widthIn(max = 330.dp)
        ) {
            Column(Modifier.padding(horizontal = 14.dp, vertical = 11.dp)) {
                MarkdownText(message.text)
                if (!message.model.isNullOrBlank()) {
                    Text(message.model!!, color = FatirMuted, fontSize = 10.sp, modifier = Modifier.padding(top = 6.dp))
                }
            }
        }
    }
}

@Composable
private fun ApprovalCard(action: PendingAction, approve: (PendingAction) -> Unit, deny: (PendingAction) -> Unit) {
    Surface(
        shape = RoundedCornerShape(20.dp),
        color = Color(0xFFFFF9EC),
        border = BorderStroke(1.dp, Color(0xFFE6C98A))
    ) {
        Column(Modifier.padding(16.dp)) {
            Text("Approval needed", fontWeight = FontWeight.SemiBold)
            Text(action.summary, modifier = Modifier.padding(top = 7.dp))
            Text(action.tool + " · " + action.risk, color = FatirMuted, fontSize = 11.sp, modifier = Modifier.padding(top = 6.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 14.dp)) {
                OutlinedButton(onClick = { deny(action) }) { Text("Deny") }
                Button(onClick = { approve(action) }) { Text("Approve") }
            }
        }
    }
}

@Composable
private fun Composer(
    text: String,
    onText: (String) -> Unit,
    enabled: Boolean,
    send: () -> Unit,
    onPhoneFile: () -> Unit,
    onLinuxFiles: () -> Unit,
    modifier: Modifier
) {
    var open by remember { mutableStateOf(false) }
    Surface(
        modifier = modifier.fillMaxWidth().navigationBarsPadding().padding(14.dp),
        shape = RoundedCornerShape(24.dp),
        color = FatirSurface,
        shadowElevation = 6.dp,
        border = BorderStroke(1.dp, FatirBorder)
    ) {
        Row(Modifier.padding(8.dp), verticalAlignment = Alignment.Bottom) {
            Box {
                IconButton(onClick = { open = true }) { Icon(Icons.Outlined.Add, "Attach") }
                DropdownMenu(open, { open = false }) {
                    DropdownMenuItem(
                        text = { Text("From this phone") },
                        leadingIcon = { Icon(Icons.Outlined.PhoneAndroid, null) },
                        onClick = { open = false; onPhoneFile() }
                    )
                    DropdownMenuItem(
                        text = { Text("From Linux") },
                        leadingIcon = { Icon(Icons.Outlined.Computer, null) },
                        onClick = { open = false; onLinuxFiles() }
                    )
                }
            }
            TextField(
                value = text,
                onValueChange = onText,
                placeholder = { Text("Ask Fatir anything…") },
                enabled = enabled,
                colors = TextFieldDefaults.colors(
                    focusedContainerColor = Color.Transparent,
                    unfocusedContainerColor = Color.Transparent,
                    disabledContainerColor = Color.Transparent,
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent
                ),
                modifier = Modifier.weight(1f),
                maxLines = 5
            )
            FilledIconButton(
                onClick = send,
                enabled = enabled && text.isNotBlank(),
                colors = IconButtonDefaults.filledIconButtonColors(containerColor = FatirGold)
            ) { Icon(Icons.Outlined.ArrowUpward, "Send") }
        }
    }
}

@Composable
private fun FilesScreen(
    path: String?,
    entries: List<FileEntry>,
    loading: Boolean,
    error: String?,
    canGoBack: Boolean,
    onBack: () -> Unit,
    onOpen: (FileEntry) -> Unit,
    onAttach: (FileEntry) -> Unit,
    onDownload: (FileEntry) -> Unit,
    onUpload: () -> Unit
) {
    Column(Modifier.fillMaxSize().padding(top = 104.dp, bottom = 18.dp)) {
        Row(Modifier.padding(horizontal = 18.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            if (canGoBack) IconButton(onClick = onBack) { Icon(Icons.Outlined.ArrowBack, "Back") }
            Column(Modifier.weight(1f)) {
                Text("Linux Files", fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
                Text(path ?: "Approved locations", color = FatirMuted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            FilledTonalIconButton(onClick = onUpload) { Icon(Icons.Outlined.UploadFile, "Upload from phone") }
        }
        if (loading && entries.isEmpty()) {
            Column(
                Modifier.fillMaxWidth().padding(top = 54.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                CircularProgressIndicator(Modifier.size(28.dp), strokeWidth = 2.dp)
                Text(
                    "Loading Linux files…",
                    color = FatirMuted,
                    fontSize = 13.sp,
                    modifier = Modifier.padding(top = 12.dp)
                )
                Text(
                    "Remote folders can take a moment over Tailscale.",
                    color = FatirMuted,
                    fontSize = 11.sp,
                    modifier = Modifier.padding(top = 3.dp)
                )
            }
        } else if (loading) {
            LinearProgressIndicator(Modifier.fillMaxWidth())
        }
        if (!error.isNullOrBlank()) Text(error, color = FatirRed, modifier = Modifier.padding(18.dp))
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(horizontal = 14.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(7.dp)
        ) {
            items(entries, key = { it.path }) { entry ->
                Surface(
                    shape = RoundedCornerShape(16.dp),
                    color = FatirSurface,
                    border = BorderStroke(1.dp, FatirBorder),
                    modifier = Modifier.fillMaxWidth().clickable(enabled = entry.is_dir) { onOpen(entry) }
                ) {
                    Row(Modifier.padding(horizontal = 13.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            if (entry.is_dir) Icons.Outlined.Folder else Icons.Outlined.InsertDriveFile,
                            null,
                            tint = if (entry.is_dir) FatirGold else FatirMuted
                        )
                        Spacer(Modifier.width(11.dp))
                        Column(Modifier.weight(1f)) {
                            Text(entry.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            if (!entry.is_dir) Text(humanBytes(entry.size), fontSize = 11.sp, color = FatirMuted)
                        }
                        if (entry.is_dir) Icon(Icons.Outlined.ChevronRight, null, tint = FatirMuted)
                        else {
                            IconButton(onClick = { onAttach(entry) }) { Icon(Icons.Outlined.AttachFile, "Attach") }
                            IconButton(onClick = { onDownload(entry) }) { Icon(Icons.Outlined.Download, "Download") }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ChatHistoryScreen(
    chats: List<ChatSummary>,
    loading: Boolean,
    error: String?,
    onRefresh: () -> Unit,
    onNewChat: () -> Unit,
    onOpen: (ChatSummary) -> Unit
) {
    Column(Modifier.fillMaxSize().padding(top = 104.dp, start = 16.dp, end = 16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("Chats", fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
                Text("Shared with Fatir Desktop", color = FatirMuted, fontSize = 12.sp)
            }
            IconButton(onClick = onRefresh) { Icon(Icons.Outlined.Refresh, "Refresh chats") }
            FilledTonalIconButton(onClick = onNewChat) { Icon(Icons.Outlined.AddComment, "New chat") }
        }
        if (loading) LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 12.dp))
        if (!error.isNullOrBlank()) Text(error, color = FatirRed, modifier = Modifier.padding(top = 12.dp))
        if (!loading && chats.isEmpty() && error.isNullOrBlank()) {
            InfoCard("No saved chats yet", "Conversations from Fatir Desktop, Companion and scheduled agents will appear here.")
        } else {
            LazyColumn(
                contentPadding = PaddingValues(top = 14.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                items(chats, key = { it.session_id }) { chat ->
                    Surface(
                        shape = RoundedCornerShape(18.dp),
                        color = FatirSurface,
                        border = BorderStroke(1.dp, FatirBorder),
                        modifier = Modifier.fillMaxWidth().clickable { onOpen(chat) }
                    ) {
                        Column(Modifier.padding(14.dp)) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(chat.title, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                                Text(chat.source.replaceFirstChar { it.uppercase() }, color = FatirGold, fontSize = 10.sp)
                            }
                            if (chat.preview.isNotBlank()) {
                                Text(chat.preview, color = FatirMuted, fontSize = 12.sp, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 5.dp))
                            }
                            Text("${chat.message_count} messages", color = FatirMuted, fontSize = 10.sp, modifier = Modifier.padding(top = 7.dp))
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun AgentSchedulesScreen(
    schedules: List<AgentScheduleItem>,
    loading: Boolean,
    error: String?,
    onRefresh: () -> Unit,
    onOpenChat: (AgentScheduleItem) -> Unit,
    onRunNow: (AgentScheduleItem) -> Unit,
    onApprovePending: (AgentScheduleItem) -> Unit,
    onCancel: (AgentScheduleItem) -> Unit
) {
    Column(Modifier.fillMaxSize().padding(top = 104.dp, start = 16.dp, end = 16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("Scheduled agents", fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
                Text("Persistent Fatir and web automations", color = FatirMuted, fontSize = 12.sp)
            }
            IconButton(onClick = onRefresh) { Icon(Icons.Outlined.Refresh, "Refresh schedules") }
        }
        if (loading) LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 12.dp))
        if (!error.isNullOrBlank()) Text(error, color = FatirRed, modifier = Modifier.padding(top = 12.dp))
        if (!loading && schedules.isEmpty() && error.isNullOrBlank()) {
            InfoCard("No scheduled agents", "Ask Fatir to create one, for example: “Every morning at 9 check Gmail and summarize what needs attention.”")
        } else {
            LazyColumn(
                contentPadding = PaddingValues(top = 14.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                items(schedules, key = { it.id }) { item ->
                    val waiting = item.last_status == "waiting_user" || item.last_status == "waiting_approval"
                    val statusColor = when (item.last_status) {
                        "completed" -> FatirGreen
                        "failed" -> FatirRed
                        "waiting_user", "waiting_approval" -> FatirGold
                        else -> FatirMuted
                    }
                    Surface(
                        shape = RoundedCornerShape(18.dp),
                        color = FatirSurface,
                        border = BorderStroke(1.dp, if (waiting) Color(0xFFE6C98A) else FatirBorder)
                    ) {
                        Column(Modifier.padding(14.dp)) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(item.label, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                                Text(
                                    (item.last_status ?: "scheduled").replace('_', ' '),
                                    color = statusColor,
                                    fontSize = 10.sp,
                                    fontWeight = FontWeight.Medium
                                )
                            }
                            Text(
                                "${item.execution_mode.replace('_', ' ')} · ${item.trigger}",
                                color = FatirMuted,
                                fontSize = 11.sp,
                                modifier = Modifier.padding(top = 4.dp)
                            )
                            if (item.last_result?.isNotBlank() == true) {
                                MarkdownText(item.last_result)
                                Spacer(Modifier.height(6.dp))
                            }
                            if (item.next.isNotBlank()) {
                                Text("Next: ${item.next}", color = FatirMuted, fontSize = 10.sp)
                            }
                            Row(
                                modifier = Modifier.padding(top = 10.dp),
                                horizontalArrangement = Arrangement.spacedBy(6.dp)
                            ) {
                                TextButton(onClick = { onOpenChat(item) }) { Text("Chat") }
                                TextButton(onClick = { onRunNow(item) }) { Text("Run now") }
                                if (!item.last_pending_action.isNullOrBlank()) {
                                    Button(onClick = { onApprovePending(item) }) { Text("Approve") }
                                }
                                TextButton(onClick = { onCancel(item) }) {
                                    Text("Cancel", color = FatirRed)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun TransfersScreen(transfers: SnapshotStateList<TransferItem>) {
    Column(Modifier.fillMaxSize().padding(top = 104.dp, start = 16.dp, end = 16.dp)) {
        Text("Transfers", fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
        Text("Phone ↔ Linux", color = FatirMuted, modifier = Modifier.padding(top = 3.dp, bottom = 14.dp))
        if (transfers.isEmpty()) InfoCard("No transfers yet", "Downloads from Linux and uploads from this phone will appear here.")
        else LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(transfers) { item ->
                Surface(shape = RoundedCornerShape(16.dp), color = FatirSurface, border = BorderStroke(1.dp, FatirBorder)) {
                    Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            if (item.success) Icons.Outlined.CheckCircle else Icons.Outlined.ErrorOutline,
                            null,
                            tint = if (item.success) FatirGreen else FatirRed
                        )
                        Spacer(Modifier.width(12.dp))
                        Column {
                            Text(item.title, fontWeight = FontWeight.Medium)
                            Text(item.detail, color = FatirMuted, fontSize = 11.sp)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SettingsScreen(health: HealthResponse, baseUrl: String, onDisconnect: () -> Unit) {
    Column(Modifier.fillMaxSize().padding(top = 104.dp, start = 18.dp, end = 18.dp)) {
        Text("Settings", fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = FatirSurface,
            border = BorderStroke(1.dp, FatirBorder),
            modifier = Modifier.padding(top = 16.dp)
        ) {
            Column(Modifier.padding(17.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(9.dp).background(FatirGreen, CircleShape))
                    Spacer(Modifier.width(8.dp))
                    Text("Connected", fontWeight = FontWeight.SemiBold)
                }
                SettingLine("Linux PC", health.device_name)
                SettingLine("Address", baseUrl)
                SettingLine("Fatir", health.version)
            }
        }
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = Color(0xFFFFF8E8),
            border = BorderStroke(1.dp, Color(0xFFE7D39E)),
            modifier = Modifier.padding(top = 12.dp)
        ) {
            Text(
                "Fatir is reachable through your private LAN or Tailscale network. Keep port 32145 private and do not forward it on your router.",
                color = FatirMuted,
                fontSize = 13.sp,
                lineHeight = 18.sp,
                modifier = Modifier.padding(16.dp)
            )
        }
        OutlinedButton(onClick = onDisconnect, modifier = Modifier.fillMaxWidth().padding(top = 18.dp)) {
            Text("Disconnect this phone")
        }
    }
}

@Composable
private fun SettingLine(label: String, value: String) {
    Row(Modifier.padding(top = 12.dp)) {
        Text(label, color = FatirMuted, modifier = Modifier.width(82.dp))
        Text(value, modifier = Modifier.weight(1f))
    }
}

@Composable
private fun InfoCard(title: String, body: String) {
    Surface(shape = RoundedCornerShape(20.dp), color = FatirSurface, border = BorderStroke(1.dp, FatirBorder)) {
        Column(Modifier.padding(18.dp)) {
            Text(title, fontWeight = FontWeight.SemiBold)
            Text(body, color = FatirMuted, modifier = Modifier.padding(top = 6.dp), lineHeight = 19.sp)
        }
    }
}

@Composable
private fun MarkdownText(text: String) {
    val annotated = remember(text) {
        buildAnnotatedString {
            var cursor = 0
            while (cursor < text.length) {
                val boldStart = text.indexOf("**", cursor)
                if (boldStart < 0) {
                    append(text.substring(cursor))
                    break
                }
                if (boldStart > cursor) append(text.substring(cursor, boldStart))
                val boldEnd = text.indexOf("**", boldStart + 2)
                if (boldEnd < 0) {
                    append(text.substring(boldStart))
                    break
                }
                withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) {
                    append(text.substring(boldStart + 2, boldEnd))
                }
                cursor = boldEnd + 2
            }
        }
    }
    Text(annotated, lineHeight = 20.sp)
}

private fun displayName(context: Context, uri: Uri): String? {
    var cursor: Cursor? = null
    return try {
        cursor = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
        if (cursor != null && cursor.moveToFirst()) {
            val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (index >= 0) cursor.getString(index) else null
        } else null
    } finally {
        cursor?.close()
    }
}

private fun humanBytes(bytes: Long): String {
    if (bytes < 1024) return bytes.toString() + " B"
    val units = arrayOf("KB", "MB", "GB", "TB")
    var value = bytes.toDouble()
    var index = -1
    while (value >= 1024 && index < units.lastIndex) {
        value /= 1024
        index++
    }
    return DecimalFormat("0.#").format(value) + " " + units[index.coerceAtLeast(0)]
}
