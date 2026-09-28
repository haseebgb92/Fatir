#!/usr/bin/env python3
from pathlib import Path
import shutil
import sys

if len(sys.argv) < 3:
    raise SystemExit("usage: apply-fatir-remote.py <moonlight-root> <fatir-logo>")

root = Path(sys.argv[1]).resolve()
logo = Path(sys.argv[2]).resolve()

def replace(path: Path, old: str, new: str):
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"patch marker not found in {path}: {old[:80]!r}")
    path.write_text(text.replace(old, new, 1))

# Keep upstream Java packages intact, but give the fork its own Android application ID.
gradle = root / "app/build.gradle"
text = gradle.read_text()
text = text.replace('compileSdk 37', 'compileSdk 36')
text = text.replace("com.squareup.okhttp3:okhttp:5.5.0", "com.squareup.okhttp3:okhttp:5.4.0")
text = text.replace('versionName "12.2"', 'versionName "12.2-fatir.1"')
text = text.replace('resValue "string", "app_label", "Moonlight (Debug)"',
                    'resValue "string", "app_label", "Fatir Remote"')
text = text.replace('resValue "string", "app_label_root", "Moonlight (Root Debug)"',
                    'resValue "string", "app_label_root", "Fatir Remote (Root)"')
text = text.replace('resValue "string", "app_label", "Moonlight"',
                    'resValue "string", "app_label", "Fatir Remote"')
text = text.replace('resValue "string", "app_label_root", "Moonlight (Root)"',
                    'resValue "string", "app_label_root", "Fatir Remote (Root)"')
text = text.replace('applicationId "com.limelight.root"', 'applicationId "com.fatir.remote.root"')
text = text.replace('applicationId "com.limelight"', 'applicationId "com.fatir.remote"')
text = text.replace('applicationIdSuffix ".debug"', '')
gradle.write_text(text)

manifest = root / "app/src/main/AndroidManifest.xml"
text = manifest.read_text()
text = text.replace(
    '<application\n        android:allowBackup="true"',
    '<application\n        android:allowBackup="false"'
)
text = text.replace('android:icon="@mipmap/ic_launcher"', 'android:icon="@drawable/fatir_mark"')
manifest.write_text(text)

drawable = root / "app/src/main/res/drawable-nodpi"
drawable.mkdir(parents=True, exist_ok=True)
shutil.copy2(logo, drawable / "fatir_mark.png")

overlay = r'''package com.limelight;

import android.graphics.Color;
import android.graphics.drawable.GradientDrawable;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.View;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;

final class FatirRemoteOverlay {
    private static int dp(Game game, float value) {
        return Math.round(value * game.getResources().getDisplayMetrics().density);
    }

    private static GradientDrawable background(int color, float radius, Game game) {
        GradientDrawable drawable = new GradientDrawable();
        drawable.setColor(color);
        drawable.setCornerRadius(dp(game, radius));
        return drawable;
    }

    static void install(Game game) {
        FrameLayout root = game.findViewById(android.R.id.content);
        if (root == null) return;

        final int white = Color.WHITE;
        final int panelColor = 0xD91C1C1E;
        final int railColor = 0x553F3F46;
        final int railActive = 0xAAE6B84A;

        LinearLayout menu = new LinearLayout(game);
        menu.setOrientation(LinearLayout.VERTICAL);
        menu.setPadding(dp(game, 8), dp(game, 8), dp(game, 8), dp(game, 8));
        menu.setBackground(background(panelColor, 14, game));
        menu.setVisibility(View.GONE);
        menu.setElevation(dp(game, 12));

        FrameLayout.LayoutParams menuLp = new FrameLayout.LayoutParams(
                dp(game, 154), FrameLayout.LayoutParams.WRAP_CONTENT);
        menuLp.gravity = Gravity.TOP | Gravity.START;
        menuLp.leftMargin = dp(game, 10);
        menuLp.topMargin = dp(game, 58);
        root.addView(menu, menuLp);

        Button keyboard = action(game, "Keyboard");
        keyboard.setOnClickListener(v -> {
            game.fatirRemoteToggleKeyboard();
            menu.setVisibility(View.GONE);
        });
        menu.addView(keyboard);

        Button stats = action(game, "Stats");
        stats.setOnClickListener(v -> game.fatirRemoteToggleStats());
        menu.addView(stats);

        Button disconnect = action(game, "Disconnect");
        disconnect.setOnClickListener(v -> game.fatirRemoteDisconnect());
        menu.addView(disconnect);

        TextView hamburger = new TextView(game);
        hamburger.setText("☰");
        hamburger.setTextColor(white);
        hamburger.setTextSize(23);
        hamburger.setGravity(Gravity.CENTER);
        hamburger.setBackground(background(panelColor, 15, game));
        hamburger.setElevation(dp(game, 14));
        hamburger.setContentDescription("Fatir Remote controls");
        hamburger.setOnClickListener(v ->
                menu.setVisibility(menu.getVisibility() == View.VISIBLE ? View.GONE : View.VISIBLE));

        FrameLayout.LayoutParams hamburgerLp = new FrameLayout.LayoutParams(dp(game, 46), dp(game, 46));
        hamburgerLp.gravity = Gravity.TOP | Gravity.START;
        hamburgerLp.leftMargin = dp(game, 10);
        hamburgerLp.topMargin = dp(game, 8);
        root.addView(hamburger, hamburgerLp);

        View scrollRail = new View(game);
        scrollRail.setBackground(background(railColor, 10, game));
        scrollRail.setElevation(dp(game, 13));
        scrollRail.setContentDescription("Remote desktop scroll strip");

        final float[] lastY = new float[1];
        final float[] carry = new float[1];
        scrollRail.setOnTouchListener((v, event) -> {
            switch (event.getActionMasked()) {
                case MotionEvent.ACTION_DOWN:
                    lastY[0] = event.getY();
                    carry[0] = 0f;
                    v.setBackground(background(railActive, 10, game));
                    return true;
                case MotionEvent.ACTION_MOVE:
                    float dy = event.getY() - lastY[0];
                    lastY[0] = event.getY();
                    carry[0] += dy;

                    // Dragging down the rail scrolls the remote page down.
                    // High-resolution wheel units give smooth scrolling instead of detents.
                    if (Math.abs(carry[0]) >= 1.5f) {
                        int amount = Math.round(-carry[0] * 8f);
                        amount = Math.max(Short.MIN_VALUE + 1, Math.min(Short.MAX_VALUE, amount));
                        game.fatirRemoteScroll((short) amount);
                        carry[0] = 0f;
                    }
                    return true;
                case MotionEvent.ACTION_UP:
                case MotionEvent.ACTION_CANCEL:
                    v.setBackground(background(railColor, 10, game));
                    return true;
                default:
                    return true;
            }
        });

        FrameLayout.LayoutParams railLp = new FrameLayout.LayoutParams(
                dp(game, 34), FrameLayout.LayoutParams.MATCH_PARENT);
        railLp.gravity = Gravity.END | Gravity.CENTER_VERTICAL;
        railLp.topMargin = dp(game, 68);
        railLp.bottomMargin = dp(game, 24);
        railLp.rightMargin = dp(game, 5);
        root.addView(scrollRail, railLp);
    }

    private static Button action(Game game, String label) {
        Button button = new Button(game);
        button.setText(label);
        button.setTextColor(Color.WHITE);
        button.setAllCaps(false);
        button.setTextSize(13);
        button.setGravity(Gravity.START | Gravity.CENTER_VERTICAL);
        button.setBackgroundColor(Color.TRANSPARENT);
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, dp(game, 44));
        button.setLayoutParams(lp);
        return button;
    }

    private FatirRemoteOverlay() {}
}
'''

overlay_path = root / "app/src/main/java/com/limelight/FatirRemoteOverlay.java"
overlay_path.write_text(overlay)

game = root / "app/src/main/java/com/limelight/Game.java"
game_text = game.read_text()

# Moonlight 12.2 references two API 37-only latency/input helpers. Fatir Remote
# builds against stable API 36, so call those helpers reflectively when they exist.
old_keyboard = '''        // Android has native keyboard capture support starting in API 36.1
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.BAKLAVA && Build.VERSION.SDK_INT_FULL >= Build.VERSION_CODES_FULL.BAKLAVA_1) {
            WindowManager.LayoutParams windowLayoutParams = getWindow().getAttributes();
            windowLayoutParams.setKeyboardCaptureEnabled(enabled);
            getWindow().setAttributes(windowLayoutParams);
        }
        else {'''
new_keyboard = '''        // Newer Android releases expose native keyboard capture. Use reflection so
        // this source remains buildable with the stable API 36 SDK.
        if (Build.VERSION.SDK_INT >= 36) {
            try {
                WindowManager.LayoutParams windowLayoutParams = getWindow().getAttributes();
                Method captureMethod = WindowManager.LayoutParams.class.getMethod(
                        "setKeyboardCaptureEnabled", boolean.class);
                captureMethod.invoke(windowLayoutParams, enabled);
                getWindow().setAttributes(windowLayoutParams);
                return;
            } catch (ReflectiveOperationException ignored) {
                // Fall through to Samsung's compatibility path below.
            }
        }
        {'''
if old_keyboard not in game_text:
    raise RuntimeError("API 37 keyboard capture marker not found")
game_text = game_text.replace(old_keyboard, new_keyboard, 1)

old_throttle = '''        // Disable producer throttling on the underlying surface for reduced latency
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.CINNAMON_BUN) {
            holder.getSurface().setProducerThrottlingEnabled(false);
        }'''
new_throttle = '''        // Disable producer throttling when the platform exposes the API. Reflection
        // preserves the optimization on newer Android without requiring API 37 to compile.
        if (Build.VERSION.SDK_INT >= 36) {
            try {
                Method throttleMethod = Surface.class.getMethod(
                        "setProducerThrottlingEnabled", boolean.class);
                throttleMethod.invoke(holder.getSurface(), false);
            } catch (ReflectiveOperationException ignored) {
            }
        }'''
if old_throttle not in game_text:
    raise RuntimeError("API 37 producer throttling marker not found")
game_text = game_text.replace(old_throttle, new_throttle, 1)

install_marker = '        performanceOverlayView = findViewById(R.id.performanceOverlay);'
if install_marker not in game_text:
    raise RuntimeError("Game.java overlay install marker not found")
game_text = game_text.replace(
    install_marker,
    install_marker + '\n\n        // Fatir Remote: lightweight controls layered above the upstream stream surface.\n        FatirRemoteOverlay.install(this);',
    1
)

helper_marker = '    private void stopConnection() {'
if helper_marker not in game_text:
    raise RuntimeError("Game.java helper marker not found")

helpers = r'''    // Fatir Remote overlay hooks. Keep these package-private so the overlay can
    // call Moonlight's existing encrypted input path without duplicating protocol code.
    void fatirRemoteScroll(short amount) {
        if (conn != null && connected && amount != 0) {
            conn.sendMouseHighResScroll(amount);
        }
    }

    void fatirRemoteToggleKeyboard() {
        toggleKeyboard();
    }

    void fatirRemoteToggleStats() {
        if (performanceOverlayView != null) {
            performanceOverlayView.setVisibility(
                    performanceOverlayView.getVisibility() == View.VISIBLE ? View.GONE : View.VISIBLE);
        }
    }

    void fatirRemoteDisconnect() {
        finish();
    }

'''
game_text = game_text.replace(helper_marker, helpers + helper_marker, 1)
game.write_text(game_text)

add_pc = root / "app/src/main/java/com/limelight/preferences/AddComputerManually.java"
add_text = add_pc.read_text()

host_marker = '''        // Bind to the ComputerManager service
        bindService(new Intent(AddComputerManually.this,
                    ComputerManagerService.class), serviceConnection, Service.BIND_AUTO_CREATE);
    }'''
host_replacement = '''        // Bind to the ComputerManager service
        bindService(new Intent(AddComputerManually.this,
                    ComputerManagerService.class), serviceConnection, Service.BIND_AUTO_CREATE);

        // Internal Fatir handoff. This activity remains non-exported: only PcView
        // receives the cross-app intent, then forwards the already-known host here.
        String fatirHost = getIntent().getStringExtra("FatirHost");
        if (fatirHost != null && !fatirHost.trim().isEmpty()) {
            fatirHost = fatirHost.trim();
            hostText.setText(fatirHost);
            computersToAdd.add(fatirHost);
        }
    }'''
if host_marker not in add_text:
    raise RuntimeError("FatirHost handoff marker not found")
add_text = add_text.replace(host_marker, host_replacement, 1)

success_marker = '''        else {
            AddComputerManually.this.runOnUiThread(new Runnable() {'''
success_replacement = '''        else {
            getSharedPreferences("FatirRemote", MODE_PRIVATE)
                    .edit()
                    .putString("LastHost", rawUserInput)
                    .apply();

            AddComputerManually.this.runOnUiThread(new Runnable() {'''
if success_marker not in add_text:
    raise RuntimeError("FatirHost success marker not found")
add_text = add_text.replace(success_marker, success_replacement, 1)
add_pc.write_text(add_text)

pc_view = root / "app/src/main/java/com/limelight/PcView.java"
pc_text = pc_view.read_text()
complete_marker = '''        initializeViews();
    }'''
complete_replacement = '''        initializeViews();

        String fatirHost = getIntent().getStringExtra("FatirHost");
        if (fatirHost != null && !fatirHost.trim().isEmpty()) {
            fatirHost = fatirHost.trim();
            String lastHost = getSharedPreferences("FatirRemote", MODE_PRIVATE)
                    .getString("LastHost", "");
            if (!fatirHost.equals(lastHost)) {
                Intent addIntent = new Intent(PcView.this, AddComputerManually.class);
                addIntent.putExtra("FatirHost", fatirHost);
                startActivity(addIntent);
            }
        }
    }'''
if complete_marker not in pc_text:
    raise RuntimeError("PcView FatirHost bridge marker not found")
pc_text = pc_text.replace(complete_marker, complete_replacement, 1)
pc_view.write_text(pc_text)

print("Fatir Remote patches applied")
