//! 每终端绑定源 DAO（2B：运行时按 PID 指派）
//!
//! `terminal_bindings` 表把"某个终端进程(PID)"绑定到"某个源(provider_id)"，
//! 供代理在请求时按连接 PID 路由。terminal_id = "<app>:<pid>"。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalBinding {
    pub terminal_id: String,
    pub pid: i64,
    pub app: String,
    pub provider_id: String,
    pub strict: bool,
    pub created_at: i64,
    pub last_seen: i64,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Database {
    /// 写入/更新一个终端绑定（按 app:pid 唯一）
    pub fn upsert_terminal_binding(
        &self,
        pid: i64,
        app: &str,
        provider_id: &str,
        strict: bool,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        let terminal_id = format!("{app}:{pid}");
        let now = now_secs();
        conn.execute(
            "INSERT INTO terminal_bindings
                (terminal_id, pid, app, provider_id, strict, created_at, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(terminal_id) DO UPDATE SET
                provider_id = excluded.provider_id,
                strict = excluded.strict,
                last_seen = excluded.last_seen",
            rusqlite::params![terminal_id, pid, app, provider_id, strict as i64, now],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// 按 PID + app 查绑定（代理热路径用）→ Some((provider_id, strict))
    pub fn get_terminal_binding_by_pid(
        &self,
        pid: i64,
        app: &str,
    ) -> Result<Option<(String, bool)>, AppError> {
        let conn = lock_conn!(self.conn);
        let res = conn
            .query_row(
                "SELECT provider_id, strict FROM terminal_bindings WHERE pid = ?1 AND app = ?2",
                rusqlite::params![pid, app],
                |row| {
                    let provider_id: String = row.get(0)?;
                    let strict: i64 = row.get(1)?;
                    Ok((provider_id, strict != 0))
                },
            )
            .ok();
        Ok(res)
    }

    pub fn list_terminal_bindings(&self) -> Result<Vec<TerminalBinding>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT terminal_id, pid, app, provider_id, strict, created_at, last_seen
                 FROM terminal_bindings ORDER BY last_seen DESC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TerminalBinding {
                    terminal_id: row.get(0)?,
                    pid: row.get(1)?,
                    app: row.get(2)?,
                    provider_id: row.get(3)?,
                    strict: row.get::<_, i64>(4)? != 0,
                    created_at: row.get(5)?,
                    last_seen: row.get(6)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(rows)
    }

    pub fn delete_terminal_binding_by_pid(&self, pid: i64, app: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM terminal_bindings WHERE pid = ?1 AND app = ?2",
            rusqlite::params![pid, app],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// 清理死进程绑定：删除不在给定存活 PID 集合内的所有绑定，返回删除条数
    pub fn prune_terminal_bindings(&self, alive_pids: &[i64]) -> Result<usize, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT terminal_id, pid FROM terminal_bindings")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let all: Vec<(String, i64)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| AppError::Database(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(e.to_string()))?;
        drop(stmt);

        let mut removed = 0usize;
        for (terminal_id, pid) in all {
            if !alive_pids.contains(&pid) {
                conn.execute(
                    "DELETE FROM terminal_bindings WHERE terminal_id = ?1",
                    rusqlite::params![terminal_id],
                )
                .map_err(|e| AppError::Database(e.to_string()))?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}
