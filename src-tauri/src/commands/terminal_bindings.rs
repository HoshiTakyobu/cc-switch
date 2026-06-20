//! 每终端绑定源命令（2B：运行时按 PID 指派 + 进程枚举）

use crate::database::TerminalBinding;
use crate::store::AppState;
use serde::Serialize;

/// 面板展示用：一个正在运行的 CLI 终端进程 + 其当前绑定（若有）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningTerminal {
    pub pid: i64,
    pub name: String,
    pub command_line: String,
    pub started_at: Option<i64>,
    pub bound_provider_id: Option<String>,
    pub strict: bool,
}

#[tauri::command]
pub async fn list_terminal_bindings(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<TerminalBinding>, String> {
    state.db.list_terminal_bindings().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_terminal_binding(
    state: tauri::State<'_, AppState>,
    pid: i64,
    app: String,
    #[allow(non_snake_case)] providerId: String,
    strict: bool,
) -> Result<(), String> {
    state
        .db
        .upsert_terminal_binding(pid, &app, &providerId, strict)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_terminal_binding(
    state: tauri::State<'_, AppState>,
    pid: i64,
    app: String,
) -> Result<(), String> {
    state
        .db
        .delete_terminal_binding_by_pid(pid, &app)
        .map_err(|e| e.to_string())
}

/// 列出当前渠道（claude/codex）正在运行的 CLI 进程，并标出已绑定的源。
/// 顺便清理已死进程的绑定。
#[tauri::command]
pub async fn list_running_terminals(
    state: tauri::State<'_, AppState>,
    app: String,
) -> Result<Vec<RunningTerminal>, String> {
    let procs = enumerate_cli_processes(&app);
    let alive: Vec<i64> = procs.iter().map(|p| p.pid).collect();
    let _ = state.db.prune_terminal_bindings(&alive);
    let bindings = state.db.list_terminal_bindings().map_err(|e| e.to_string())?;

    let result = procs
        .into_iter()
        .map(|p| {
            let b = bindings.iter().find(|b| b.pid == p.pid && b.app == app);
            RunningTerminal {
                pid: p.pid,
                name: p.name,
                command_line: p.command_line,
                started_at: p.started_at,
                bound_provider_id: b.map(|b| b.provider_id.clone()),
                strict: b.map(|b| b.strict).unwrap_or(true),
            }
        })
        .collect();
    Ok(result)
}

struct PsProc {
    pid: i64,
    name: String,
    command_line: String,
    started_at: Option<i64>,
}

/// 枚举命令行匹配当前渠道的 CLI 进程（仅 Windows；其它平台返回空）。
fn enumerate_cli_processes(app: &str) -> Vec<PsProc> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Vec::new()
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let pat = if app == "codex" { "codex" } else { "claude" };
        let script = format!(
            "$ErrorActionPreference='SilentlyContinue'; \
             Get-CimInstance Win32_Process | \
             Where-Object {{ $_.CommandLine -and $_.CommandLine -match '{pat}' -and $_.CommandLine -notmatch 'cc-switch|odex-control|CodexControl' }} | \
             Select-Object ProcessId,Name,CommandLine,@{{N='StartedAt';E={{[int64]((($_.CreationDate).ToUniversalTime()-(Get-Date '1970-01-01')).TotalSeconds)}}}} | \
             ConvertTo-Json -Compress -Depth 3"
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let Ok(out) = out else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let arr: Vec<serde_json::Value> = match val {
            serde_json::Value::Array(a) => a,
            other @ serde_json::Value::Object(_) => vec![other],
            _ => Vec::new(),
        };
        arr.into_iter()
            .filter_map(|o| {
                let pid = o.get("ProcessId").and_then(|v| v.as_i64())?;
                let name = o
                    .get("Name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let command_line = o
                    .get("CommandLine")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let started_at = o.get("StartedAt").and_then(|v| v.as_i64());
                Some(PsProc {
                    pid,
                    name,
                    command_line,
                    started_at,
                })
            })
            .collect()
    }
}
