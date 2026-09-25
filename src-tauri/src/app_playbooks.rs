use serde_json::{json, Value};

pub fn lookup(app:&str)->Value{
    let a=app.to_lowercase();
    let (family,shortcuts,notes)=if a.contains("android studio")||a.contains("intellij")||a.contains("pycharm")||a.contains("webstorm") {
        ("JetBrains",json!({"search_everywhere":"Shift+Shift","terminal":"Alt+F12","build":"Ctrl+F9","run":"Shift+F10","settings":"Ctrl+Alt+S"}),"Prefer AT-SPI for named menus/dialogs, keyboard shortcuts for editor/terminal, visual fallback only for custom editor surfaces.")
    }else if a.contains("nemo")||a.contains("files"){
        ("File manager",json!({"location":"Ctrl+L","new_tab":"Ctrl+T","rename":"F2","properties":"Alt+Enter","search":"Ctrl+F"}),"Use semantic file list controls when available; destructive delete remains approval-gated.")
    }else if a.contains("code")||a.contains("vscode")||a.contains("antigravity"){
        ("Code editor",json!({"command_palette":"Ctrl+Shift+P","terminal":"Ctrl+`","quick_open":"Ctrl+P","search":"Ctrl+Shift+F"}),"Prefer command palette and keyboard navigation before visual clicks.")
    }else if a.contains("gimp"){
        ("Image editor",json!({"open":"Ctrl+O","save":"Ctrl+Shift+S","export":"Ctrl+Shift+E","undo":"Ctrl+Z"}),"Canvas operations may require visual fallback; menus/dialogs should use semantic/keyboard paths.")
    }else if a.contains("krita"){
        ("Image editor",json!({"open":"Ctrl+O","save":"Ctrl+S","save_as":"Ctrl+Shift+S","undo":"Ctrl+Z","canvas_only":"Tab"}),"Prefer menus/shortcuts for document operations; use visual fallback only for canvas-specific actions that have no semantic target.")
    }else if a.contains("libreoffice"){
        ("Office",json!({"open":"Ctrl+O","save":"Ctrl+S","find":"Ctrl+F","export_pdf":"Ctrl+Shift+S"}),"Use accessible document controls where exposed and verify file output after export/save.")
    }else if a.contains("vlc"){
        ("Media player",json!({"play_pause":"Space","fullscreen":"F","stop":"S","volume_up":"Ctrl+Up","volume_down":"Ctrl+Down"}),"Prefer keyboard media controls; verify playback state visually or semantically when available.")
    }else if a.contains("calculator")||a.contains("gnome-calculator"){
        ("Calculator",json!({"clear":"Escape"}),"Prefer accessible buttons/text and read the displayed result after calculation.")
    }else if a.contains("system settings")||a.contains("cinnamon-settings")||a.contains("settings"){
        ("System settings",json!({"search":"Ctrl+F"}),"Use semantic controls for settings pages. Privileged/security-sensitive changes remain approval-gated.")
    }else if a.contains("file roller")||a.contains("archive manager")||a.contains("engrampa"){
        ("Archive manager",json!({"open":"Ctrl+O","extract":"Ctrl+E"}),"Prefer Fatir's direct archive tools for deterministic extraction; use GUI control when the user explicitly wants the application UI.")
    }else if a.contains("xviewer")||a.contains("image viewer")||a.contains("pix"){
        ("Image viewer",json!({"open":"Ctrl+O","fullscreen":"F11","next":"Right","previous":"Left"}),"Keyboard navigation is usually more reliable than visual clicks; verify the displayed image/title when relevant.")
    }else if a.contains("terminal")||a.contains("gnome-terminal")||a.contains("konsole"){
        ("Terminal",json!({"new_tab":"Ctrl+Shift+T","copy":"Ctrl+Shift+C","paste":"Ctrl+Shift+V"}),"For command execution prefer Fatir terminal_session_* tools over GUI typing when possible.")
    }else{
        ("Generic Linux app",json!({}),"Discover capabilities first: AT-SPI semantics → keyboard/window control → visual fallback. Never assume coordinates persist across runs.")
    };
    json!({"app":app,"family":family,"shortcuts":shortcuts,"notes":notes,"generic_control_order":["AT-SPI","keyboard/window","visual X11 fallback"],"verification_required":true})
}
