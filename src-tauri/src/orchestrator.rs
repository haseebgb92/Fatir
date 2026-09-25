use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct VerificationDebt {
    pub tool: String,
    pub summary: String,
}

pub fn is_browser_mutation(tool: &str) -> bool {
    matches!(tool,
        "browser_click" | "browser_click_text" | "browser_click_element" |
        "browser_fill_by_label" | "browser_fill_element" | "browser_type" | "browser_key" |
        "browser_select_tab" | "browser_close_tab" | "browser_new_tab")
}

pub fn is_desktop_mutation(tool: &str) -> bool {
    matches!(tool,
        "desktop_activate" | "desktop_activate_named" | "desktop_set_text" |
        "desktop_key" | "desktop_type" | "desktop_visual_action" | "desktop_focus_window")
}

pub fn requires_post_verification(tool: &str) -> bool {
    is_browser_mutation(tool) || is_desktop_mutation(tool)
}

pub fn is_verifier(tool: &str) -> bool {
    matches!(tool,
        "browser_elements" | "browser_page_summary" | "browser_observe" |
        "browser_extract_structure" | "browser_network_recent" | "browser_diagnostics" |
        "browser_tabs" | "desktop_windows" | "desktop_elements" | "desktop_find" |
        "desktop_get_text" | "desktop_wait_for" | "desktop_observe" | "desktop_capabilities")
}

pub fn verification_instruction(debt: &VerificationDebt) -> String {
    let family = if debt.tool.starts_with("browser_") { "browser" } else { "desktop" };
    format!(
        "FATIR V1 VERIFICATION GATE: the last {family} action ({}) changed UI state and has not been independently verified yet. Before giving a success answer, inspect the resulting state with an appropriate read-only verifier and confirm the user's requested outcome. If the outcome is not visible, recover or report the concrete blocker instead of claiming success. Last action: {}",
        debt.tool, debt.summary
    )
}

pub fn complexity_score(text: &str) -> u8 {
    let h = text.to_lowercase();
    let mut score = 0u8;
    let action_words = [" then ", " and then ", "after that", "finally", "also ", "open ", "click ", "type ", "send ", "build ", "install ", "analyze ", "compare ", "download ", "upload ", "edit "];
    for word in action_words { if h.contains(word) { score = score.saturating_add(1); } }
    if h.len() > 220 { score = score.saturating_add(1); }
    if h.contains("across") || h.contains("multiple") || h.contains("workflow") { score = score.saturating_add(2); }
    score.min(10)
}

pub fn surface_for_tool(tool: &str) -> &'static str {
    if tool.starts_with("browser_") || tool.starts_with("headless_browser_") || tool == "amazon_keyword_research" { "browser" }
    else if tool.starts_with("desktop_") { "desktop" }
    else if tool.starts_with("terminal_session_") || matches!(tool,"run_shell_command"|"run_privileged_command"|"run_shell_with_credentials") { "terminal" }
    else if tool.starts_with("project_") { "project" }
    else if tool.starts_with("task_") { "task" }
    else if tool.starts_with("background_job_") || tool.starts_with("scheduled_job_") { "background" }
    else { "system" }
}

pub fn status() -> Value {
    json!({
        "engine":"Fatir V1 task engine",
        "verification_gate":true,
        "recovery_classification":true,
        "headless_policy":"explicit-only",
        "browser_desktop_isolation":true,
        "surfaces":["browser","desktop","terminal","project","system","background"]
    })
}
