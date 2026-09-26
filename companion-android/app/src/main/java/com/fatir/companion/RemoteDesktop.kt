package com.fatir.companion

import android.app.Activity
import android.content.Context
import android.content.pm.ActivityInfo
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
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
import java.io.EOFException
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.TimeUnit
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.min
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import okhttp3.*
import okio.ByteString
import okio.ByteString.Companion.toByteString

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

    var onFrame: ((Bitmap) -> Unit)? = null
    var onState: ((String) -> Unit)? = null
    var onError: ((String) -> Unit)? = null

    private var bitmap: Bitmap? = null

    fun start() {
        stopped = false
        scope.launch {
            var attempt = 0
            while (!stopped) {
                try {
                    onState?.invoke(if (attempt == 0) "Connecting…" else "Reconnecting…")
                    runSession()
                    if (!stopped) throw EOFException("Remote desktop connection closed")
                } catch (t: Throwable) {
                    if (stopped) break
                    attempt++
                    onError?.invoke(t.message ?: "Remote desktop connection interrupted")
                    onState?.invoke("Reconnecting…")
                    delay(backoff(attempt))
                }
            }
            onState?.invoke("Disconnected")
        }
    }

    fun currentFrame(): Bitmap? = bitmap

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
        val closed = CompletableDeferred<Unit>()

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
                if (!closed.isCompleted) closed.complete(Unit)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                stream.close()
                if (!closed.isCompleted) closed.complete(Unit)
            }
        })
        socket = ws

        withTimeout(12_000) { opened.await() }
        handshake(stream)
        onState?.invoke("Connected")

        try {
            serverLoop(stream)
        } finally {
            ws.close(1000, "session end")
            socket = null
            stream.close()
            if (!closed.isCompleted) closed.complete(Unit)
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
        createBitmap(framebufferWidth, framebufferHeight)

        sendPixelFormat()
        sendEncodings()
        requestFramebuffer(incremental = false)
    }

    private suspend fun serverLoop(stream: WsByteStream) {
        while (!stopped) {
            when (stream.readU8()) {
                0 -> readFramebufferUpdate(stream)
                1 -> { // SetColorMapEntries; not used by our true-color format.
                    stream.readU8()
                    stream.readU16()
                    val colors = stream.readU16().coerceAtMost(65_536)
                    stream.readExactly(colors * 6)
                }
                2 -> Unit // bell
                3 -> {
                    stream.readExactly(3)
                    val length = stream.readU32().toInt().coerceAtMost(4 * 1024 * 1024)
                    stream.readExactly(length)
                }
                else -> error("Unsupported remote desktop server message")
            }
        }
    }

    private suspend fun readFramebufferUpdate(stream: WsByteStream) {
        stream.readU8() // padding
        val rectCount = stream.readU16()
        var changed = false

        repeat(rectCount) {
            val header = stream.readExactly(12)
            val x = u16(header, 0)
            val y = u16(header, 2)
            val width = u16(header, 4)
            val height = u16(header, 6)
            val encoding = i32(header, 8)

            when (encoding) {
                0 -> {
                    val bytesNeeded = width.toLong() * height.toLong() * 4L
                    if (bytesNeeded > 64L * 1024L * 1024L) error("Remote desktop frame is too large")
                    val raw = stream.readExactly(bytesNeeded.toInt())
                    applyRawRectangle(x, y, width, height, raw)
                    changed = true
                }
                -223 -> { // DesktopSize pseudo-encoding
                    framebufferWidth = width
                    framebufferHeight = height
                    createBitmap(width, height)
                    changed = true
                }
                else -> error("Unsupported RFB encoding $encoding")
            }
        }

        if (changed) bitmap?.let { onFrame?.invoke(it) }
        requestFramebuffer(incremental = true)
    }

    private fun applyRawRectangle(x: Int, y: Int, width: Int, height: Int, raw: ByteArray) {
        val target = bitmap ?: return
        if (x < 0 || y < 0 || x + width > target.width || y + height > target.height) return
        val pixels = IntArray(width * height)
        var src = 0
        for (i in pixels.indices) {
            val b = raw[src].toInt() and 0xff
            val g = raw[src + 1].toInt() and 0xff
            val r = raw[src + 2].toInt() and 0xff
            pixels[i] = (0xff shl 24) or (r shl 16) or (g shl 8) or b
            src += 4
        }
        target.setPixels(pixels, 0, width, x, y, width, height)
    }

    private fun createBitmap(width: Int, height: Int) {
        if (width <= 0 || height <= 0 || width > 8192 || height > 8192) error("Invalid desktop size")
        bitmap?.recycle()
        bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).also {
            it.eraseColor(Color.BLACK)
        }
    }

    private fun sendPixelFormat() {
        val msg = ByteArray(20)
        msg[0] = 0
        msg[4] = 32 // bits per pixel
        msg[5] = 24 // depth
        msg[6] = 0 // little-endian
        msg[7] = 1 // true color
        putU16(msg, 8, 255)
        putU16(msg, 10, 255)
        putU16(msg, 12, 255)
        msg[14] = 16
        msg[15] = 8
        msg[16] = 0
        sendRaw(msg)
    }

    private fun sendEncodings() {
        val msg = ByteArray(12)
        msg[0] = 2
        putU16(msg, 2, 2)
        putI32(msg, 4, 0) // Raw
        putI32(msg, 8, -223) // DesktopSize
        sendRaw(msg)
    }

    private fun requestFramebuffer(incremental: Boolean) {
        val width = framebufferWidth
        val height = framebufferHeight
        if (width <= 0 || height <= 0) return
        val msg = ByteArray(10)
        msg[0] = 3
        msg[1] = if (incremental) 1 else 0
        putU16(msg, 2, 0)
        putU16(msg, 4, 0)
        putU16(msg, 6, width)
        putU16(msg, 8, height)
        sendRaw(msg)
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

        private fun i32(bytes: ByteArray, offset: Int): Int =
            ByteBuffer.wrap(bytes, offset, 4).order(ByteOrder.BIG_ENDIAN).int

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

internal class RemoteDesktopSurface(
    context: Context,
    private val client: RemoteDesktopClient
) : View(context) {
    private val paint = Paint(Paint.FILTER_BITMAP_FLAG)
    private var bitmap: Bitmap? = null
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
        setBackgroundColor(Color.BLACK)
        bitmap = client.currentFrame()
        client.onFrame = { frame ->
            post {
                bitmap = frame
                invalidate()
            }
        }
    }

    fun setDragLock(enabled: Boolean) {
        dragLock = enabled
    }

    fun zoomIn() {
        zoom = (zoom * 1.25f).coerceAtMost(4f)
        invalidate()
    }

    fun zoomOut() {
        zoom = (zoom / 1.25f).coerceAtLeast(1f)
        if (zoom == 1f) { panX = 0f; panY = 0f }
        invalidate()
    }

    fun fit() {
        zoom = 1f
        panX = 0f
        panY = 0f
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val frame = bitmap ?: return
        val dest = destination(frame)
        canvas.drawBitmap(frame, null, dest, paint)
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        val frame = bitmap ?: return true

        if (event.pointerCount >= 2) {
            handleTwoFinger(event, frame)
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
                val point = mapToFramebuffer(event.x, event.y, frame)
                client.movePointer(point.first, point.second, dragLock)
            }
            MotionEvent.ACTION_UP -> {
                parent?.requestDisallowInterceptTouchEvent(false)
                if (multiTouch) {
                    multiTouch = false
                    return true
                }
                val point = mapToFramebuffer(event.x, event.y, frame)
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
                val point = mapToFramebuffer(event.x, event.y, frame)
                client.releasePointer(point.first, point.second)
            }
        }
        return true
    }

    private fun handleTwoFinger(event: MotionEvent, frame: Bitmap) {
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
                    invalidate()
                } else if (abs(vertical) > 18f) {
                    twoFingerMoved = true
                    val point = mapToFramebuffer(centerX, centerY, frame)
                    client.scroll(point.first, point.second, if (vertical > 0) -1 else 1)
                }
                lastTwoFingerY = centerY
                lastTwoFingerSpan = span
            }
            MotionEvent.ACTION_POINTER_UP -> {
                val duration = SystemClock.uptimeMillis() - twoFingerStartAt
                if (!twoFingerMoved && duration < 350) {
                    val point = mapToFramebuffer(centerX, centerY, frame)
                    client.rightClick(point.first, point.second)
                }
            }
        }
    }

    private fun destination(frame: Bitmap): RectF {
        val fit = min(width.toFloat() / frame.width.toFloat(), height.toFloat() / frame.height.toFloat())
        val scale = fit * zoom
        val dw = frame.width * scale
        val dh = frame.height * scale
        val left = (width - dw) / 2f + panX
        val top = (height - dh) / 2f + panY
        return RectF(left, top, left + dw, top + dh)
    }

    private fun mapToFramebuffer(x: Float, y: Float, frame: Bitmap): Pair<Int, Int> {
        val dest = destination(frame)
        val fx = ((x - dest.left) / dest.width() * frame.width).toInt().coerceIn(0, frame.width - 1)
        val fy = ((y - dest.top) / dest.height() * frame.height).toInt().coerceIn(0, frame.height - 1)
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
    val activity = LocalContext.current as? Activity

    DisposableEffect(activity) {
        val previous = activity?.requestedOrientation
        activity?.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        onDispose {
            if (activity != null) {
                activity.requestedOrientation = previous ?: ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            }
        }
    }
    val focusRequester = remember { FocusRequester() }
    val client = remember(api) { RemoteDesktopClient(api) }

    var status by remember { mutableStateOf("Starting…") }
    var error by remember { mutableStateOf<String?>(null) }
    var dragLock by remember { mutableStateOf(false) }
    var keyboardOpen by remember { mutableStateOf(false) }
    var keyboardSink by remember { mutableStateOf("") }
    var surface by remember { mutableStateOf<RemoteDesktopSurface?>(null) }
    var serverReady by remember { mutableStateOf(false) }

    DisposableEffect(client) {
        client.onState = { value ->
            android.os.Handler(android.os.Looper.getMainLooper()).post {
                status = value
                if (value == "Connected") error = null
            }
        }
        client.onError = { value -> android.os.Handler(android.os.Looper.getMainLooper()).post { error = value } }
        onDispose {
            client.stop()
            scope.launch(Dispatchers.IO) { runCatching { api.stopRemoteDesktop() } }
        }
    }

    LaunchedEffect(Unit) {
        error = null
        status = "Starting Linux desktop…"
        try {
            val result = api.startRemoteDesktop()
            if (!result.available) error(result.message)
            else {
                serverReady = true
                client.start()
            }
        } catch (t: Throwable) {
            error = t.message ?: "Unable to start Remote Desktop"
            status = "Unavailable"
        }
    }

    Box(Modifier.fillMaxSize().background(ComposeColor.Black)) {
        if (serverReady) {
            AndroidView(
                factory = { context ->
                    RemoteDesktopSurface(context, client).also { surface = it }
                },
                modifier = Modifier.fillMaxSize()
            )
        }

        Surface(
            color = ComposeColor(0xE6222222),
            shape = MaterialTheme.shapes.large,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .statusBarsPadding()
                .padding(10.dp)
        ) {
            Row(
                Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                IconButton(onClick = onExit) { Icon(Icons.Outlined.Close, "Close", tint = ComposeColor.White) }
                Column(Modifier.weight(1f)) {
                    Text("Remote Desktop", color = ComposeColor.White, fontSize = 14.sp)
                    Text(
                        if (error.isNullOrBlank()) status else error!!,
                        color = if (error.isNullOrBlank()) ComposeColor(0xFFBDBDBD) else ComposeColor(0xFFFFA7A7),
                        fontSize = 10.sp,
                        maxLines = 2
                    )
                }
                if (status == "Connected") {
                    Box(Modifier.size(8.dp).background(ComposeColor(0xFF45C17A), androidx.compose.foundation.shape.CircleShape))
                }
            }
        }

        Surface(
            color = ComposeColor(0xE6222222),
            shape = MaterialTheme.shapes.large,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
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
                    } else keyboard?.hide()
                }) { Icon(Icons.Outlined.Keyboard, "Keyboard", tint = ComposeColor.White) }

                IconButton(onClick = { client.sendSpecial(RemoteDesktopClient.KEY_BACKSPACE) }) {
                    Icon(Icons.Outlined.Backspace, "Backspace", tint = ComposeColor.White)
                }
                IconButton(onClick = { client.sendSpecial(RemoteDesktopClient.KEY_ENTER) }) {
                    Icon(Icons.Outlined.KeyboardReturn, "Enter", tint = ComposeColor.White)
                }

                FilledTonalButton(
                    onClick = {
                        dragLock = !dragLock
                        surface?.setDragLock(dragLock)
                    },
                    contentPadding = PaddingValues(horizontal = 10.dp, vertical = 0.dp)
                ) {
                    Text(if (dragLock) "Drag ON" else "Drag")
                }

                IconButton(onClick = { surface?.zoomOut() }) {
                    Icon(Icons.Outlined.Remove, "Zoom out", tint = ComposeColor.White)
                }
                IconButton(onClick = { surface?.fit() }) {
                    Icon(Icons.Outlined.FitScreen, "Fit", tint = ComposeColor.White)
                }
                IconButton(onClick = { surface?.zoomIn() }) {
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
                Icon(Icons.Outlined.DesktopWindows, null, tint = ComposeColor.White, modifier = Modifier.size(48.dp))
                Text("Remote Desktop unavailable", color = ComposeColor.White, modifier = Modifier.padding(top = 12.dp))
                Text(error!!, color = ComposeColor(0xFFBDBDBD), fontSize = 12.sp, modifier = Modifier.padding(top = 6.dp))
                TextButton(onClick = onExit) { Text("Back") }
            }
        }
    }
}
