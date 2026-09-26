package com.fatir.companion

import android.app.Activity
import android.content.Context
import android.content.pm.ActivityInfo
import android.graphics.Color
import android.graphics.RectF
import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color as ComposeColor
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.Player
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.DefaultLoadControl
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.ui.AspectRatioFrameLayout
import androidx.media3.ui.PlayerView
import java.io.EOFException
import java.util.concurrent.TimeUnit
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.min
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import okhttp3.*
import okio.ByteString
import okio.ByteString.Companion.toByteString

/**
 * RFB is intentionally used only as a very small input/control channel.
 * Screen pixels are delivered separately as H.264/MPEG-TS and decoded by
 * Android's Media3/MediaCodec path.
 */
internal class RemoteDesktopClient(private val api: FatirApi) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val http = OkHttpClient.Builder()
        .connectTimeout(12, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(20, TimeUnit.SECONDS)
        .retryOnConnectionFailure(true)
        .build()

    @Volatile private var stopped = false
    @Volatile private var socket: WebSocket? = null

    @Volatile var framebufferWidth: Int = 0
        private set
    @Volatile var framebufferHeight: Int = 0
        private set
    @Volatile var desktopName: String = "Linux desktop"
        private set

    var onState: ((String) -> Unit)? = null
    var onError: ((String) -> Unit)? = null

    fun start() {
        stopped = false
        scope.launch {
            var attempt = 0
            while (!stopped) {
                try {
                    onState?.invoke(if (attempt == 0) "Connecting controls…" else "Reconnecting controls…")
                    runSession()
                    if (!stopped) throw EOFException("Remote desktop control connection closed")
                } catch (t: Throwable) {
                    if (stopped) break
                    attempt++
                    onError?.invoke(t.message ?: "Remote desktop controls were interrupted")
                    onState?.invoke("Reconnecting controls…")
                    delay(backoff(attempt))
                }
            }
            onState?.invoke("Disconnected")
        }
    }

    fun stop() {
        stopped = true
        socket?.close(1000, "Remote desktop closed")
        socket = null
        scope.coroutineContext.cancelChildren()
    }

    fun click(x: Int, y: Int) {
        pointer(x, y, 1)
        pointer(x, y, 0)
    }

    fun doubleClick(x: Int, y: Int) {
        click(x, y)
        scope.launch {
            delay(80)
            click(x, y)
        }
    }

    fun rightClick(x: Int, y: Int) {
        pointer(x, y, 4)
        pointer(x, y, 0)
    }

    fun movePointer(x: Int, y: Int, dragging: Boolean = false) {
        pointer(x, y, if (dragging) 1 else 0)
    }

    fun releasePointer(x: Int, y: Int) {
        pointer(x, y, 0)
    }

    fun scroll(x: Int, y: Int, direction: Int, steps: Int = 1) {
        val mask = if (direction < 0) 8 else 16
        repeat(steps.coerceIn(1, 8)) {
            pointer(x, y, mask)
            pointer(x, y, 0)
        }
    }

    fun sendText(text: String) {
        text.codePoints().forEach { code ->
            val keysym = if (code <= 0xff) code else (0x01000000 or code)
            key(keysym, true)
            key(keysym, false)
        }
    }

    fun sendSpecial(keysym: Int) {
        key(keysym, true)
        key(keysym, false)
    }

    private suspend fun runSession() {
        val stream = WsByteStream()
        val opened = CompletableDeferred<WebSocket>()

        val request = Request.Builder()
            .url(api.remoteDesktopWebSocketUrl())
            .header("Authorization", api.authorizationHeader())
            .build()

        val ws = http.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                if (!opened.isCompleted) opened.complete(webSocket)
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                stream.offer(bytes.toByteArray())
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                stream.close(t)
                if (!opened.isCompleted) opened.completeExceptionally(t)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                stream.close()
            }
        })
        socket = ws

        withTimeout(12_000) { opened.await() }
        handshake(stream)
        onState?.invoke("Connected")

        try {
            // With no FramebufferUpdateRequest x11vnc sends no desktop pixels.
            // We only consume asynchronous bell/clipboard messages while input
            // events travel in the opposite direction.
            while (!stopped) {
                when (stream.readU8()) {
                    1 -> {
                        stream.readU8()
                        stream.readU16()
                        val colors = stream.readU16().coerceAtMost(65_536)
                        stream.readExactly(colors * 6)
                    }
                    2 -> Unit
                    3 -> {
                        stream.readExactly(3)
                        val length = stream.readU32().toInt().coerceAtMost(4 * 1024 * 1024)
                        stream.readExactly(length)
                    }
                    0 -> error("Unexpected framebuffer data on control-only channel")
                    else -> error("Unsupported remote desktop control message")
                }
            }
        } finally {
            ws.close(1000, "session end")
            socket = null
            stream.close()
        }
    }

    private suspend fun handshake(stream: WsByteStream) {
        val version = stream.readExactly(12).toString(Charsets.US_ASCII)
        if (!version.startsWith("RFB ")) error("Unexpected remote desktop protocol")
        sendRaw("RFB 003.008\n".toByteArray(Charsets.US_ASCII))

        val securityCount = stream.readU8()
        if (securityCount == 0) {
            val length = stream.readU32().toInt().coerceAtMost(16_384)
            val reason = stream.readExactly(length).toString(Charsets.UTF_8)
            error("Remote desktop rejected connection: $reason")
        }
        val securityTypes = stream.readExactly(securityCount)
        if (!securityTypes.any { it.toInt() and 0xff == 1 }) {
            error("Remote desktop did not offer the expected local security mode")
        }
        sendRaw(byteArrayOf(1))

        val securityResult = stream.readU32()
        if (securityResult != 0L) {
            val reason = runCatching {
                val length = stream.readU32().toInt().coerceAtMost(16_384)
                stream.readExactly(length).toString(Charsets.UTF_8)
            }.getOrNull()
            error(reason ?: "Remote desktop authentication failed")
        }

        sendRaw(byteArrayOf(1)) // shared session
        val init = stream.readExactly(24)
        framebufferWidth = u16(init, 0)
        framebufferHeight = u16(init, 2)
        val nameLength = u32(init, 20).toInt().coerceAtMost(65_536)
        desktopName = stream.readExactly(nameLength).toString(Charsets.UTF_8)
    }

    private fun pointer(x: Int, y: Int, mask: Int) {
        val width = framebufferWidth
        val height = framebufferHeight
        if (width <= 0 || height <= 0) return

        val msg = ByteArray(6)
        msg[0] = 5
        msg[1] = mask.toByte()
        putU16(msg, 2, x.coerceIn(0, width - 1))
        putU16(msg, 4, y.coerceIn(0, height - 1))
        sendRaw(msg)
    }

    private fun key(keysym: Int, down: Boolean) {
        val msg = ByteArray(8)
        msg[0] = 4
        msg[1] = if (down) 1 else 0
        putI32(msg, 4, keysym)
        sendRaw(msg)
    }

    private fun sendRaw(bytes: ByteArray) {
        socket?.send(bytes.toByteString())
    }

    private fun backoff(attempt: Int): Long {
        val power = (attempt - 1).coerceIn(0, 3)
        return (1_000L shl power).coerceAtMost(8_000L)
    }

    companion object {
        const val KEY_BACKSPACE = 0xff08
        const val KEY_TAB = 0xff09
        const val KEY_ENTER = 0xff0d
        const val KEY_ESCAPE = 0xff1b
        const val KEY_LEFT = 0xff51
        const val KEY_UP = 0xff52
        const val KEY_RIGHT = 0xff53
        const val KEY_DOWN = 0xff54
        const val KEY_DELETE = 0xffff

        private fun u16(bytes: ByteArray, offset: Int): Int =
            ((bytes[offset].toInt() and 0xff) shl 8) or (bytes[offset + 1].toInt() and 0xff)

        private fun u32(bytes: ByteArray, offset: Int): Long =
            ((bytes[offset].toLong() and 0xff) shl 24) or
                ((bytes[offset + 1].toLong() and 0xff) shl 16) or
                ((bytes[offset + 2].toLong() and 0xff) shl 8) or
                (bytes[offset + 3].toLong() and 0xff)

        private fun putU16(bytes: ByteArray, offset: Int, value: Int) {
            bytes[offset] = ((value ushr 8) and 0xff).toByte()
            bytes[offset + 1] = (value and 0xff).toByte()
        }

        private fun putI32(bytes: ByteArray, offset: Int, value: Int) {
            bytes[offset] = ((value ushr 24) and 0xff).toByte()
            bytes[offset + 1] = ((value ushr 16) and 0xff).toByte()
            bytes[offset + 2] = ((value ushr 8) and 0xff).toByte()
            bytes[offset + 3] = (value and 0xff).toByte()
        }
    }
}

private class WsByteStream {
    private val channel = Channel<ByteArray>(Channel.UNLIMITED)
    private var current = ByteArray(0)
    private var offset = 0

    fun offer(bytes: ByteArray) {
        if (bytes.isNotEmpty()) channel.trySend(bytes)
    }

    fun close(cause: Throwable? = null) {
        channel.close(cause)
    }

    suspend fun readExactly(count: Int): ByteArray {
        if (count == 0) return ByteArray(0)
        val out = ByteArray(count)
        var written = 0
        while (written < count) {
            if (offset >= current.size) {
                current = channel.receiveCatching().getOrNull()
                    ?: throw EOFException("Remote desktop stream closed")
                offset = 0
            }
            val take = min(count - written, current.size - offset)
            current.copyInto(out, written, offset, offset + take)
            written += take
            offset += take
        }
        return out
    }

    suspend fun readU8(): Int = readExactly(1)[0].toInt() and 0xff

    suspend fun readU16(): Int {
        val b = readExactly(2)
        return ((b[0].toInt() and 0xff) shl 8) or (b[1].toInt() and 0xff)
    }

    suspend fun readU32(): Long {
        val b = readExactly(4)
        return ((b[0].toLong() and 0xff) shl 24) or
            ((b[1].toLong() and 0xff) shl 16) or
            ((b[2].toLong() and 0xff) shl 8) or
            (b[3].toLong() and 0xff)
    }
}

internal class RemoteDesktopHostView(
    context: Context,
    private val client: RemoteDesktopClient,
    player: ExoPlayer
) : FrameLayout(context) {
    private val playerView = PlayerView(context).apply {
        useController = false
        resizeMode = AspectRatioFrameLayout.RESIZE_MODE_FIT
        setBackgroundColor(Color.BLACK)
        this.player = player
        layoutParams = LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT)
    }

    private val controls = RemoteDesktopControlSurface(context, client) { zoom, panX, panY ->
        playerView.scaleX = zoom
        playerView.scaleY = zoom
        playerView.translationX = panX
        playerView.translationY = panY
    }.apply {
        layoutParams = LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT)
    }

    init {
        setBackgroundColor(Color.BLACK)
        addView(playerView)
        addView(controls)
    }

    fun setDragLock(enabled: Boolean) = controls.setDragLock(enabled)
    fun zoomIn() = controls.zoomIn()
    fun zoomOut() = controls.zoomOut()
    fun fit() = controls.fit()
}

private class RemoteDesktopControlSurface(
    context: Context,
    private val client: RemoteDesktopClient,
    private val onTransform: (zoom: Float, panX: Float, panY: Float) -> Unit
) : View(context) {
    private var zoom = 1f
    private var panX = 0f
    private var panY = 0f
    private var dragLock = false

    private var downX = 0f
    private var downY = 0f
    private var downAt = 0L
    private var moved = false
    private var multiTouch = false
    private var lastTapAt = 0L
    private var pendingSingleTap: Runnable? = null
    private var lastTwoFingerY = 0f
    private var lastTwoFingerSpan = 0f
    private var twoFingerStartAt = 0L
    private var twoFingerMoved = false

    init {
        isFocusable = true
        isFocusableInTouchMode = true
        setBackgroundColor(Color.TRANSPARENT)
    }

    fun setDragLock(enabled: Boolean) {
        dragLock = enabled
    }

    fun zoomIn() {
        zoom = (zoom * 1.25f).coerceAtMost(4f)
        applyTransform()
    }

    fun zoomOut() {
        zoom = (zoom / 1.25f).coerceAtLeast(1f)
        if (zoom == 1f) {
            panX = 0f
            panY = 0f
        }
        applyTransform()
    }

    fun fit() {
        zoom = 1f
        panX = 0f
        panY = 0f
        applyTransform()
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (client.framebufferWidth <= 0 || client.framebufferHeight <= 0) return true

        if (event.pointerCount >= 2) {
            handleTwoFinger(event)
            return true
        }

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                multiTouch = false
                moved = false
                downX = event.x
                downY = event.y
                downAt = SystemClock.uptimeMillis()
                parent?.requestDisallowInterceptTouchEvent(true)
            }
            MotionEvent.ACTION_MOVE -> {
                if (multiTouch) return true
                val distance = hypot(event.x - downX, event.y - downY)
                if (distance > 8f) moved = true
                val point = mapToFramebuffer(event.x, event.y)
                client.movePointer(point.first, point.second, dragLock)
            }
            MotionEvent.ACTION_UP -> {
                parent?.requestDisallowInterceptTouchEvent(false)
                if (multiTouch) {
                    multiTouch = false
                    return true
                }

                val point = mapToFramebuffer(event.x, event.y)
                if (dragLock && moved) {
                    client.releasePointer(point.first, point.second)
                } else if (!moved) {
                    val now = SystemClock.uptimeMillis()
                    if (now - downAt > 650) {
                        pendingSingleTap?.let { removeCallbacks(it) }
                        pendingSingleTap = null
                        client.rightClick(point.first, point.second)
                        lastTapAt = 0
                    } else if (now - lastTapAt < 320) {
                        pendingSingleTap?.let { removeCallbacks(it) }
                        pendingSingleTap = null
                        client.doubleClick(point.first, point.second)
                        lastTapAt = 0
                    } else {
                        lastTapAt = now
                        val tapAt = now
                        val click = Runnable {
                            if (lastTapAt == tapAt) {
                                client.click(point.first, point.second)
                                lastTapAt = 0
                            }
                            pendingSingleTap = null
                        }
                        pendingSingleTap = click
                        postDelayed(click, 320)
                    }
                }
            }
            MotionEvent.ACTION_CANCEL -> {
                val point = mapToFramebuffer(event.x, event.y)
                client.releasePointer(point.first, point.second)
            }
        }
        return true
    }

    private fun handleTwoFinger(event: MotionEvent) {
        multiTouch = true
        val x0 = event.getX(0)
        val y0 = event.getY(0)
        val x1 = event.getX(1)
        val y1 = event.getY(1)
        val centerX = (x0 + x1) / 2f
        val centerY = (y0 + y1) / 2f
        val span = hypot(x1 - x0, y1 - y0)

        when (event.actionMasked) {
            MotionEvent.ACTION_POINTER_DOWN -> {
                lastTwoFingerY = centerY
                lastTwoFingerSpan = span
                twoFingerStartAt = SystemClock.uptimeMillis()
                twoFingerMoved = false
            }
            MotionEvent.ACTION_MOVE -> {
                val spanRatio = if (lastTwoFingerSpan > 0f) span / lastTwoFingerSpan else 1f
                val vertical = centerY - lastTwoFingerY

                if (abs(spanRatio - 1f) > 0.025f) {
                    twoFingerMoved = true
                    val oldZoom = zoom
                    zoom = (zoom * spanRatio).coerceIn(1f, 4f)

                    if (zoom == 1f) {
                        panX = 0f
                        panY = 0f
                    } else if (oldZoom != zoom) {
                        panX += (width / 2f - centerX) * (spanRatio - 1f)
                        panY += (height / 2f - centerY) * (spanRatio - 1f)
                    }
                    applyTransform()
                } else if (abs(vertical) > 18f) {
                    twoFingerMoved = true
                    val point = mapToFramebuffer(centerX, centerY)
                    client.scroll(point.first, point.second, if (vertical > 0) -1 else 1)
                }

                lastTwoFingerY = centerY
                lastTwoFingerSpan = span
            }
            MotionEvent.ACTION_POINTER_UP -> {
                val duration = SystemClock.uptimeMillis() - twoFingerStartAt
                if (!twoFingerMoved && duration < 350) {
                    val point = mapToFramebuffer(centerX, centerY)
                    client.rightClick(point.first, point.second)
                }
            }
        }
    }

    private fun applyTransform() {
        onTransform(zoom, panX, panY)
    }

    private fun destination(): RectF {
        val frameWidth = client.framebufferWidth.coerceAtLeast(1)
        val frameHeight = client.framebufferHeight.coerceAtLeast(1)
        val fit = min(width.toFloat() / frameWidth.toFloat(), height.toFloat() / frameHeight.toFloat())
        val scale = fit * zoom
        val dw = frameWidth * scale
        val dh = frameHeight * scale
        val left = (width - dw) / 2f + panX
        val top = (height - dh) / 2f + panY
        return RectF(left, top, left + dw, top + dh)
    }

    private fun mapToFramebuffer(x: Float, y: Float): Pair<Int, Int> {
        val frameWidth = client.framebufferWidth.coerceAtLeast(1)
        val frameHeight = client.framebufferHeight.coerceAtLeast(1)
        val dest = destination()

        val fx = ((x - dest.left) / dest.width() * frameWidth)
            .toInt()
            .coerceIn(0, frameWidth - 1)
        val fy = ((y - dest.top) / dest.height() * frameHeight)
            .toInt()
            .coerceIn(0, frameHeight - 1)
        return fx to fy
    }
}

@Composable
internal fun RemoteDesktopScreen(
    api: FatirApi,
    onExit: () -> Unit
) {
    val scope = rememberCoroutineScope()
    val keyboard = LocalSoftwareKeyboardController.current
    val context = LocalContext.current
    val activity = context as? Activity

    DisposableEffect(activity) {
        val previous = activity?.requestedOrientation
        val window = activity?.window
        val controller = if (window != null) WindowCompat.getInsetsController(window, window.decorView) else null

        activity?.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        controller?.systemBarsBehavior =
            WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller?.hide(WindowInsetsCompat.Type.systemBars())

        onDispose {
            controller?.show(WindowInsetsCompat.Type.systemBars())
            if (activity != null) {
                activity.requestedOrientation = previous ?: ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            }
        }
    }

    val focusRequester = remember { FocusRequester() }
    val client = remember(api) { RemoteDesktopClient(api) }

    val player = remember(api) {
        val httpFactory = DefaultHttpDataSource.Factory()
            .setDefaultRequestProperties(mapOf("Authorization" to api.authorizationHeader()))
            .setConnectTimeoutMs(12_000)
            .setReadTimeoutMs(30_000)

        val loadControl = DefaultLoadControl.Builder()
            .setBufferDurationsMs(
                120,  // min buffer
                450,  // max buffer
                80,   // playback start
                120   // after rebuffer
            )
            .setPrioritizeTimeOverSizeThresholds(true)
            .build()

        ExoPlayer.Builder(context)
            .setLoadControl(loadControl)
            .setMediaSourceFactory(
                DefaultMediaSourceFactory(context).setDataSourceFactory(httpFactory)
            )
            .build()
    }

    var status by remember { mutableStateOf("Starting…") }
    var videoState by remember { mutableStateOf("Video starting…") }
    var error by remember { mutableStateOf<String?>(null) }
    var dragLock by remember { mutableStateOf(false) }
    var keyboardOpen by remember { mutableStateOf(false) }
    var keyboardSink by remember { mutableStateOf("") }
    var hostView by remember { mutableStateOf<RemoteDesktopHostView?>(null) }
    var serverReady by remember { mutableStateOf(false) }

    DisposableEffect(client, player) {
        client.onState = { value ->
            android.os.Handler(android.os.Looper.getMainLooper()).post {
                status = value
                if (value == "Connected") error = null
            }
        }
        client.onError = { value ->
            android.os.Handler(android.os.Looper.getMainLooper()).post { error = value }
        }

        val playerListener = object : Player.Listener {
            override fun onPlaybackStateChanged(playbackState: Int) {
                videoState = when (playbackState) {
                    Player.STATE_BUFFERING -> "H.264 buffering…"
                    Player.STATE_READY -> if (player.isPlaying) "H.264 · 30 fps" else "H.264 ready"
                    Player.STATE_ENDED -> "Video stream ended"
                    else -> "Video starting…"
                }
            }

            override fun onIsPlayingChanged(isPlaying: Boolean) {
                if (isPlaying) videoState = "H.264 · 30 fps"
            }

            override fun onPlayerError(errorValue: androidx.media3.common.PlaybackException) {
                videoState = "Video reconnecting…"
                scope.launch {
                    delay(750)
                    runCatching {
                        player.prepare()
                        player.playWhenReady = true
                    }
                }
            }
        }
        player.addListener(playerListener)

        onDispose {
            client.stop()
            player.removeListener(playerListener)
            player.release()
            scope.launch(Dispatchers.IO) { runCatching { api.stopRemoteDesktop() } }
        }
    }

    LaunchedEffect(Unit) {
        error = null
        status = "Starting Linux desktop…"
        try {
            val result = api.startRemoteDesktop()
            if (!result.available) {
                error = result.message
            } else {
                serverReady = true
                client.start()

                player.setMediaItem(
                    MediaItem.Builder()
                        .setUri(api.remoteDesktopVideoUrl())
                        .setMimeType(MimeTypes.VIDEO_MP2T)
                        .build()
                )
                player.prepare()
                player.playWhenReady = true
            }
        } catch (t: Throwable) {
            error = t.message ?: "Unable to start Remote Desktop"
            status = "Unavailable"
        }
    }

    Box(Modifier.fillMaxSize().background(ComposeColor.Black)) {
        if (serverReady) {
            AndroidView(
                factory = { ctx ->
                    RemoteDesktopHostView(ctx, client, player).also { hostView = it }
                },
                modifier = Modifier.fillMaxSize()
            )
        }

        Surface(
            color = ComposeColor(0xE6222222),
            shape = MaterialTheme.shapes.large,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(10.dp)
        ) {
            Row(
                Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                IconButton(onClick = onExit) {
                    Icon(Icons.Outlined.Close, "Close", tint = ComposeColor.White)
                }

                Column(Modifier.weight(1f)) {
                    Text("Remote Desktop", color = ComposeColor.White, fontSize = 14.sp)
                    Text(
                        if (error.isNullOrBlank()) {
                            if (status == "Connected") "$videoState · controls connected" else status
                        } else error!!,
                        color = if (error.isNullOrBlank()) ComposeColor(0xFFBDBDBD) else ComposeColor(0xFFFFA7A7),
                        fontSize = 10.sp,
                        maxLines = 2
                    )
                }

                if (status == "Connected" && videoState.startsWith("H.264")) {
                    Box(
                        Modifier
                            .size(8.dp)
                            .background(
                                ComposeColor(0xFF45C17A),
                                androidx.compose.foundation.shape.CircleShape
                            )
                    )
                }
            }
        }

        Surface(
            color = ComposeColor(0xE6222222),
            shape = MaterialTheme.shapes.large,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(10.dp)
        ) {
            Row(
                Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(2.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                IconButton(onClick = {
                    keyboardOpen = !keyboardOpen
                    if (keyboardOpen) {
                        scope.launch {
                            delay(80)
                            focusRequester.requestFocus()
                            keyboard?.show()
                        }
                    } else {
                        keyboard?.hide()
                    }
                }) {
                    Icon(Icons.Outlined.Keyboard, "Keyboard", tint = ComposeColor.White)
                }

                IconButton(onClick = { client.sendSpecial(RemoteDesktopClient.KEY_BACKSPACE) }) {
                    Icon(Icons.Outlined.Backspace, "Backspace", tint = ComposeColor.White)
                }

                IconButton(onClick = { client.sendSpecial(RemoteDesktopClient.KEY_ENTER) }) {
                    Icon(Icons.Outlined.KeyboardReturn, "Enter", tint = ComposeColor.White)
                }

                FilledTonalButton(
                    onClick = {
                        dragLock = !dragLock
                        hostView?.setDragLock(dragLock)
                    },
                    contentPadding = PaddingValues(horizontal = 10.dp, vertical = 0.dp)
                ) {
                    Text(if (dragLock) "Drag ON" else "Drag")
                }

                IconButton(onClick = { hostView?.zoomOut() }) {
                    Icon(Icons.Outlined.Remove, "Zoom out", tint = ComposeColor.White)
                }
                IconButton(onClick = { hostView?.fit() }) {
                    Icon(Icons.Outlined.FitScreen, "Fit", tint = ComposeColor.White)
                }
                IconButton(onClick = { hostView?.zoomIn() }) {
                    Icon(Icons.Outlined.Add, "Zoom in", tint = ComposeColor.White)
                }
            }
        }

        if (keyboardOpen) {
            BasicTextField(
                value = keyboardSink,
                onValueChange = { value ->
                    if (value.isNotEmpty()) client.sendText(value)
                    keyboardSink = ""
                },
                cursorBrush = SolidColor(ComposeColor.Transparent),
                modifier = Modifier
                    .align(Alignment.BottomStart)
                    .size(2.dp)
                    .focusRequester(focusRequester)
            )
        }

        if (!serverReady && error.isNullOrBlank()) {
            CircularProgressIndicator(
                modifier = Modifier.align(Alignment.Center),
                color = ComposeColor.White
            )
        }

        if (!error.isNullOrBlank() && !serverReady) {
            Column(
                Modifier.align(Alignment.Center).padding(28.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                Icon(
                    Icons.Outlined.DesktopWindows,
                    null,
                    tint = ComposeColor.White,
                    modifier = Modifier.size(48.dp)
                )
                Text(
                    "Remote Desktop unavailable",
                    color = ComposeColor.White,
                    modifier = Modifier.padding(top = 12.dp)
                )
                Text(
                    error!!,
                    color = ComposeColor(0xFFBDBDBD),
                    fontSize = 12.sp,
                    modifier = Modifier.padding(top = 6.dp)
                )
                TextButton(onClick = onExit) { Text("Back") }
            }
        }
    }
}
