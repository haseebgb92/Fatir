package com.fatir.companion

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.webkit.MimeTypeMap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.InsertDriveFile
import androidx.compose.material.icons.outlined.OpenInNew
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.FileProvider
import androidx.media3.common.MediaItem
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.ui.PlayerView
import java.io.File
import java.io.FileInputStream
import java.util.zip.ZipInputStream
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal enum class AttachmentKind { IMAGE, VIDEO, AUDIO, PDF, TEXT, DOCUMENT, OTHER }

internal fun attachmentKind(resource: ResourceRef): AttachmentKind {
    val ext = resource.path.substringAfterLast('.', "").lowercase()
    return when (ext) {
        "jpg", "jpeg", "png", "webp", "gif", "bmp", "heic", "heif" -> AttachmentKind.IMAGE
        "mp4", "webm", "mkv", "mov", "m4v", "3gp" -> AttachmentKind.VIDEO
        "mp3", "m4a", "wav", "ogg", "opus", "aac", "flac" -> AttachmentKind.AUDIO
        "pdf" -> AttachmentKind.PDF
        "txt", "md", "json", "xml", "csv", "log", "js", "ts", "jsx", "tsx", "kt", "kts",
        "rs", "py", "php", "css", "html", "htm", "yaml", "yml", "toml", "ini", "conf",
        "sh", "bash", "zsh", "sql" -> AttachmentKind.TEXT
        "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf" -> AttachmentKind.DOCUMENT
        else -> AttachmentKind.OTHER
    }
}

internal fun attachmentCategoryLabel(resource: ResourceRef): String = when (attachmentKind(resource)) {
    AttachmentKind.IMAGE -> "Images"
    AttachmentKind.VIDEO -> "Videos"
    AttachmentKind.AUDIO -> "Audio"
    AttachmentKind.PDF, AttachmentKind.TEXT, AttachmentKind.DOCUMENT -> "Documents"
    AttachmentKind.OTHER -> "Other"
}

@Composable
internal fun AttachmentViewerDialog(
    api: FatirApi,
    resource: ResourceRef,
    onDismiss: () -> Unit
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val kind = remember(resource.path) { attachmentKind(resource) }
    var cachedFile by remember(resource.path) { mutableStateOf<File?>(null) }
    var loading by remember(resource.path) { mutableStateOf(kind !in setOf(AttachmentKind.VIDEO, AttachmentKind.AUDIO)) }
    var error by remember(resource.path) { mutableStateOf<String?>(null) }

    suspend fun ensureCached(): File {
        cachedFile?.takeIf { it.exists() && it.length() > 0 }?.let { return it }
        val file = previewCacheFile(context, resource)
        if (!file.exists() || file.length() == 0L) {
            api.downloadToFile(resource.path, file)
        }
        cachedFile = file
        return file
    }

    LaunchedEffect(resource.path) {
        if (kind !in setOf(AttachmentKind.VIDEO, AttachmentKind.AUDIO)) {
            loading = true
            error = null
            try {
                ensureCached()
            } catch (t: Throwable) {
                error = t.message ?: "Unable to load file"
            } finally {
                loading = false
            }
        }
    }

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false)
    ) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.background
        ) {
            Column(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding()) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    IconButton(onClick = onDismiss) { Icon(Icons.Outlined.Close, "Close") }
                    Column(Modifier.weight(1f)) {
                        Text(resource.label.ifBlank { resource.path.substringAfterLast('/') }, fontWeight = FontWeight.SemiBold, maxLines = 1)
                        Text(attachmentCategoryLabel(resource), color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 11.sp)
                    }
                    IconButton(onClick = {
                        scope.launch {
                            try {
                                val local = ensureCached()
                                openExternally(context, local, resource)
                            } catch (t: Throwable) {
                                error = t.message ?: "Unable to open file"
                            }
                        }
                    }) { Icon(Icons.Outlined.OpenInNew, "Open externally") }
                }
                HorizontalDivider()

                Box(Modifier.weight(1f).fillMaxWidth()) {
                    when {
                        error != null -> {
                            Column(Modifier.align(Alignment.Center).padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                                Icon(Icons.Outlined.InsertDriveFile, null, modifier = Modifier.size(42.dp))
                                Text(error.orEmpty(), modifier = Modifier.padding(top = 12.dp))
                                TextButton(onClick = {
                                    error = null
                                    loading = true
                                    scope.launch {
                                        try { ensureCached() }
                                        catch (t: Throwable) { error = t.message ?: "Unable to load file" }
                                        finally { loading = false }
                                    }
                                }) { Text("Retry") }
                            }
                        }
                        loading -> CircularProgressIndicator(Modifier.align(Alignment.Center))
                        kind == AttachmentKind.IMAGE && cachedFile != null -> ImagePreview(cachedFile!!)
                        kind == AttachmentKind.VIDEO -> MediaPreview(api, resource, video = true)
                        kind == AttachmentKind.AUDIO -> MediaPreview(api, resource, video = false)
                        kind == AttachmentKind.PDF && cachedFile != null -> PdfPreview(cachedFile!!)
                        kind == AttachmentKind.TEXT && cachedFile != null -> TextPreview(cachedFile!!)
                        kind == AttachmentKind.DOCUMENT && cachedFile != null -> DocumentPreview(cachedFile!!)
                        else -> UnsupportedPreview(resource)
                    }
                }
            }
        }
    }
}

@Composable
private fun ImagePreview(file: File) {
    val bitmap = remember(file.path, file.lastModified()) { BitmapFactory.decodeFile(file.absolutePath) }
    if (bitmap == null) {
        UnsupportedPreview(ResourceRef(file.name, file.absolutePath, "file"))
        return
    }
    var scale by remember { mutableFloatStateOf(1f) }
    var x by remember { mutableFloatStateOf(0f) }
    var y by remember { mutableFloatStateOf(0f) }
    val transform = rememberTransformableState { zoom, pan, _ ->
        scale = (scale * zoom).coerceIn(1f, 6f)
        x += pan.x
        y += pan.y
    }
    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.surfaceVariant)) {
        Image(
            bitmap = bitmap.asImageBitmap(),
            contentDescription = file.name,
            contentScale = ContentScale.Fit,
            modifier = Modifier
                .fillMaxSize()
                .graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                    translationX = x
                    translationY = y
                }
                .transformable(transform)
        )
    }
}

@Composable
private fun MediaPreview(api: FatirApi, resource: ResourceRef, video: Boolean) {
    val context = LocalContext.current
    val player = remember(resource.path) {
        val http = DefaultHttpDataSource.Factory()
            .setDefaultRequestProperties(api.mediaHeaders())
            .setAllowCrossProtocolRedirects(true)
        val mediaSourceFactory = DefaultMediaSourceFactory(context)
            .setDataSourceFactory(http)
        ExoPlayer.Builder(context)
            .setMediaSourceFactory(mediaSourceFactory)
            .build()
            .apply {
                setMediaItem(MediaItem.fromUri(api.mediaUrl(resource.path)))
                prepare()
            }
    }
    DisposableEffect(player) { onDispose { player.release() } }

    Column(
        Modifier.fillMaxSize().padding(if (video) 0.dp else 24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        if (!video) {
            Text("Audio", fontSize = 26.sp, fontWeight = FontWeight.SemiBold)
            Text(resource.label, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 6.dp, bottom = 18.dp))
        }
        AndroidView(
            factory = { ctx -> PlayerView(ctx).apply { this.player = player; useController = true } },
            update = { it.player = player },
            modifier = if (video) Modifier.fillMaxSize() else Modifier.fillMaxWidth().height(120.dp)
        )
    }
}

private data class RenderedPdfPage(val bitmap: Bitmap, val pages: Int)

@Composable
private fun PdfPreview(file: File) {
    var pageIndex by remember(file.path) { mutableIntStateOf(0) }
    var rendered by remember(file.path, pageIndex) { mutableStateOf<RenderedPdfPage?>(null) }
    var error by remember(file.path, pageIndex) { mutableStateOf<String?>(null) }

    LaunchedEffect(file.path, pageIndex) {
        try {
            rendered = renderPdfPage(file, pageIndex)
            error = null
        } catch (t: Throwable) {
            error = t.message ?: "Unable to render PDF"
        }
    }

    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().padding(8.dp), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = { if (pageIndex > 0) pageIndex-- }, enabled = pageIndex > 0) { Text("Previous") }
            Text("Page ${pageIndex + 1} / ${rendered?.pages ?: "…"}", modifier = Modifier.padding(horizontal = 12.dp))
            TextButton(
                onClick = { if (rendered != null && pageIndex + 1 < rendered!!.pages) pageIndex++ },
                enabled = rendered != null && pageIndex + 1 < rendered!!.pages
            ) { Text("Next") }
        }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (error != null) Text(error.orEmpty(), modifier = Modifier.align(Alignment.Center))
            else if (rendered == null) CircularProgressIndicator(Modifier.align(Alignment.Center))
            else Image(
                rendered!!.bitmap.asImageBitmap(),
                contentDescription = "PDF page ${pageIndex + 1}",
                contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxSize()
            )
        }
    }
}

@Composable
private fun TextPreview(file: File) {
    var text by remember(file.path) { mutableStateOf<String?>(null) }
    var error by remember(file.path) { mutableStateOf<String?>(null) }
    LaunchedEffect(file.path) {
        try {
            text = withContext(Dispatchers.IO) {
                val bytes = file.inputStream().use { input ->
                    val buffer = ByteArray(2_000_000)
                    var total = 0
                    while (total < buffer.size) {
                        val count = input.read(buffer, total, buffer.size - total)
                        if (count <= 0) break
                        total += count
                    }
                    buffer.copyOf(total)
                }
                String(bytes, Charsets.UTF_8) + if (file.length() > bytes.size) "\n\n… preview truncated …" else ""
            }
        } catch (t: Throwable) { error = t.message }
    }
    Box(Modifier.fillMaxSize()) {
        when {
            error != null -> Text(error.orEmpty(), modifier = Modifier.align(Alignment.Center))
            text == null -> CircularProgressIndicator(Modifier.align(Alignment.Center))
            else -> SelectionContainer {
                Text(
                    text.orEmpty(),
                    fontFamily = FontFamily.Monospace,
                    fontSize = 12.sp,
                    modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp)
                )
            }
        }
    }
}

@Composable
private fun DocumentPreview(file: File) {
    val ext = file.extension.lowercase()
    var text by remember(file.path) { mutableStateOf<String?>(null) }
    var error by remember(file.path) { mutableStateOf<String?>(null) }

    LaunchedEffect(file.path) {
        if (ext in setOf("docx", "xlsx", "pptx")) {
            try { text = extractOpenXmlText(file, ext) }
            catch (t: Throwable) { error = t.message ?: "Could not extract document text" }
        }
    }

    when {
        ext in setOf("docx", "xlsx", "pptx") && error == null && text == null ->
            Box(Modifier.fillMaxSize()) { CircularProgressIndicator(Modifier.align(Alignment.Center)) }
        !text.isNullOrBlank() -> SelectionContainer {
            Text(
                text.orEmpty(),
                modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(18.dp),
                lineHeight = 20.sp
            )
        }
        else -> UnsupportedPreview(ResourceRef(file.name, file.absolutePath, "file"))
    }
}

@Composable
private fun UnsupportedPreview(resource: ResourceRef) {
    Column(
        Modifier.fillMaxSize().padding(28.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Icon(Icons.Outlined.InsertDriveFile, null, modifier = Modifier.size(52.dp))
        Text(resource.label.ifBlank { resource.path.substringAfterLast('/') }, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 14.dp))
        Text("This format does not have a safe inline renderer. Use Open externally above.", color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 8.dp))
    }
}

private suspend fun renderPdfPage(file: File, index: Int): RenderedPdfPage = withContext(Dispatchers.IO) {
    val descriptor = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    PdfRenderer(descriptor).use { renderer ->
        val safeIndex = index.coerceIn(0, (renderer.pageCount - 1).coerceAtLeast(0))
        renderer.openPage(safeIndex).use { page ->
            val width = 1400
            val height = (width.toFloat() * page.height / page.width).toInt().coerceAtLeast(1)
            val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
            bitmap.eraseColor(android.graphics.Color.WHITE)
            page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
            RenderedPdfPage(bitmap, renderer.pageCount)
        }
    }
}

private suspend fun extractOpenXmlText(file: File, ext: String): String = withContext(Dispatchers.IO) {
    val wanted: (String) -> Boolean = when (ext) {
        "docx" -> { name -> name == "word/document.xml" }
        "pptx" -> { name -> name.startsWith("ppt/slides/slide") && name.endsWith(".xml") }
        "xlsx" -> { name -> name == "xl/sharedStrings.xml" || (name.startsWith("xl/worksheets/sheet") && name.endsWith(".xml")) }
        else -> { _ -> false }
    }
    val out = StringBuilder()
    ZipInputStream(FileInputStream(file)).use { zip ->
        while (true) {
            val entry = zip.nextEntry ?: break
            if (wanted(entry.name)) {
                val raw = zip.readBytes().toString(Charsets.UTF_8)
                val clean = raw
                    .replace(Regex("<[^>]+>"), " ")
                    .replace("&amp;", "&")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace(Regex("\\s+"), " ")
                    .trim()
                if (clean.isNotBlank()) {
                    if (out.isNotEmpty()) out.append("\n\n")
                    out.append(clean)
                }
                if (out.length > 200_000) break
            }
            zip.closeEntry()
        }
    }
    if (out.isEmpty()) "No readable text was found in this document." else out.take(200_000)
}

private fun previewCacheFile(context: Context, resource: ResourceRef): File {
    val dir = File(context.cacheDir, "fatir-preview").apply { mkdirs() }
    val ext = resource.path.substringAfterLast('.', "").lowercase().take(12)
    val stem = resource.path.hashCode().toUInt().toString(16)
    return File(dir, if (ext.isBlank()) stem else "$stem.$ext")
}

private fun openExternally(context: Context, file: File, resource: ResourceRef) {
    val uri = FileProvider.getUriForFile(context, context.packageName + ".files", file)
    val ext = resource.path.substringAfterLast('.', "").lowercase()
    val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
    val intent = Intent(Intent.ACTION_VIEW)
        .setDataAndType(uri, mime)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    context.startActivity(Intent.createChooser(intent, "Open with"))
}
