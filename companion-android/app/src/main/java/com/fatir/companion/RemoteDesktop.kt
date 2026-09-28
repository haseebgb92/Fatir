package com.fatir.companion

import android.content.ClipData
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material.icons.outlined.DesktopWindows
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.OpenInNew
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

private const val FATIR_REMOTE_PACKAGE = "com.fatir.remote"
private const val FATIR_REMOTE_ACTIVITY = "com.limelight.PcView"

@Composable
internal fun RemoteDesktopScreen(
    api: FatirApi,
    onExit: () -> Unit
) {
    val context = LocalContext.current
    var launchError by remember { mutableStateOf<String?>(null) }
    val hostAddress = remember(api.baseUrl) {
        api.baseUrl.toHttpUrlOrNull()?.host.orEmpty()
    }

    fun openRemote() {
        try {
            val intent = Intent(Intent.ACTION_MAIN).apply {
                component = ComponentName(FATIR_REMOTE_PACKAGE, FATIR_REMOTE_ACTIVITY)
                addCategory(Intent.CATEGORY_LAUNCHER)
                addFlags(Intent.FLAG_ACTIVITY_REORDER_TO_FRONT)
            }
            context.startActivity(intent)
            launchError = null
        } catch (_: Throwable) {
            launchError = "Fatir Remote is not installed yet."
        }
    }

    LaunchedEffect(Unit) {
        openRemote()
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
    ) {
        Column(
            Modifier
                .align(Alignment.Center)
                .widthIn(max = 520.dp)
                .padding(28.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Icon(
                Icons.Outlined.DesktopWindows,
                contentDescription = null,
                modifier = Modifier.size(56.dp),
                tint = MaterialTheme.colorScheme.primary
            )

            Text(
                "Fatir Remote",
                fontSize = 24.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(top = 14.dp)
            )

            Text(
                "Sunshine + Moonlight streaming",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontSize = 13.sp,
                modifier = Modifier.padding(top = 4.dp)
            )

            Surface(
                shape = MaterialTheme.shapes.large,
                color = MaterialTheme.colorScheme.surfaceVariant,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 22.dp)
            ) {
                Column(Modifier.padding(18.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Outlined.Lock, null, Modifier.size(19.dp))
                        Spacer(Modifier.width(10.dp))
                        Text("Secure pairing", fontWeight = FontWeight.SemiBold)
                    }
                    Text(
                        "Fatir does not store your Sunshine web-admin username or password. " +
                            "Fatir Remote pairs directly with Sunshine using Moonlight's one-time PIN/certificate flow.",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        fontSize = 12.sp,
                        lineHeight = 18.sp,
                        modifier = Modifier.padding(top = 8.dp)
                    )
                }
            }

            if (hostAddress.isNotBlank()) {
                OutlinedButton(
                    onClick = {
                        val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        clipboard.setPrimaryClip(ClipData.newPlainText("Fatir PC address", hostAddress))
                        Toast.makeText(context, "PC address copied", Toast.LENGTH_SHORT).show()
                    },
                    modifier = Modifier.padding(top = 14.dp)
                ) {
                    Icon(Icons.Outlined.ContentCopy, null, Modifier.size(17.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Copy PC address · $hostAddress")
                }
            }

            if (launchError != null) {
                Text(
                    launchError!!,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(top = 14.dp)
                )
            }

            Button(
                onClick = ::openRemote,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 18.dp)
            ) {
                Icon(Icons.Outlined.OpenInNew, null)
                Spacer(Modifier.width(8.dp))
                Text("Open Fatir Remote")
            }

            Text(
                "First connection: add this PC in Fatir Remote, tap Pair, then enter the 4-digit PIN in Sunshine's PIN page. " +
                    "After pairing, the trusted client certificate stays on your phone.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontSize = 11.sp,
                lineHeight = 16.sp,
                modifier = Modifier.padding(top = 14.dp)
            )

            TextButton(onClick = onExit, modifier = Modifier.padding(top = 8.dp)) {
                Text("Back to Companion")
            }
        }
    }
}
