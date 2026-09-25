use serde_json::{json, Value};

pub fn classify(tool: &str, error: &str) -> Value {
    let e = error.to_lowercase();
    let (category, retryable, guidance) = if e.contains("permission denied") || e.contains("not permitted") || e.contains("polkit") || e.contains("authentication is required") {
        ("permission", false, "Do not retry the same command blindly. Use the appropriate privileged/PolicyKit path or ask for the required user action.")
    } else if e.contains("no such file") || e.contains("not found") || e.contains("unable to locate package") || e.contains("command not found") {
        ("not_found", false, "Inspect current paths/packages/app launchers first. Do not invent a replacement name.")
    } else if e.contains("timed out") || e.contains("timeout") {
        ("timeout", true, "Re-observe current state before retrying. Prefer a higher-level or batched operation instead of repeating the same low-level action.")
    } else if e.contains("connection") || e.contains("dns") || e.contains("network") || e.contains("http 5") || e.contains("http 429") {
        ("network", true, "Inspect network/request state and retry only when the failure is transient. Preserve task state if the dependency remains unavailable.")
    } else if e.contains("captcha") || e.contains("verification") || e.contains("2fa") || e.contains("otp") || e.contains("login") || e.contains("unauthorized") || e.contains("forbidden") {
        ("authentication", false, "Use human takeover or the credential broker as appropriate. Never guess credentials or bypass a challenge.")
    } else if tool.starts_with("desktop_") && (e.contains("at-spi") || e.contains("accessibility") || e.contains("window")) {
        ("desktop_state", true, "Run desktop_doctor/capabilities or re-enumerate windows before another action. Use X11 visual fallback only when semantic/keyboard control is inadequate.")
    } else if tool.starts_with("browser_") && (e.contains("uid") || e.contains("page") || e.contains("target") || e.contains("element")) {
        ("browser_state", true, "Take a fresh MCP snapshot and re-resolve the semantic target; never reuse a stale uid or coordinates.")
    } else if e.contains("compile") || e.contains("build failed") || e.contains("error[") {
        ("build", true, "Read the exact compiler/build output, fix the first root error, then rebuild. Do not stack speculative changes.")
    } else {
        ("unknown", true, "Inspect the current state and choose a different evidence-based route instead of repeating the identical failing action.")
    };
    json!({"category":category,"retryable":retryable,"tool":tool,"guidance":guidance})
}

pub fn guidance_text(tool: &str, error: &str) -> String {
    let v=classify(tool,error);
    format!(
        "FATIR V1 RECOVERY: category={} retryable={}. {}",
        v.get("category").and_then(Value::as_str).unwrap_or("unknown"),
        v.get("retryable").and_then(Value::as_bool).unwrap_or(false),
        v.get("guidance").and_then(Value::as_str).unwrap_or("Inspect current state before retrying.")
    )
}
