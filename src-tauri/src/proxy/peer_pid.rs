//! peer 源端口 → 拥有该连接的客户端 PID（2B：运行时按 PID 指派）
//!
//! 代理在 accept 时拿到对端 `SocketAddr`（客户端源端口），本模块把"源端口"映射到
//! "拥有该 TCP 连接的进程 PID"。仅 Windows 实现（用户平台）：解析 `netstat -ano`，
//! 整表缓存 ~2s，避免每请求开进程。非 Windows 返回 None（绑定不生效，回退全局逻辑）。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// 放进 request extensions 的对端地址（accept 时捕获）。
#[derive(Clone, Copy, Debug)]
pub struct PeerAddr(pub SocketAddr);

const TTL: Duration = Duration::from_secs(2);

struct PortPidCache {
    at: Instant,
    map: HashMap<u16, u32>,
}

static CACHE: LazyLock<Mutex<Option<PortPidCache>>> = LazyLock::new(|| Mutex::new(None));

/// 返回"本地源端口 == peer_port"的 TCP 连接所属 PID（仅 Windows；其它平台 None）。
pub fn resolve_pid(peer_port: u16) -> Option<u32> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = peer_port;
        None
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(guard) = CACHE.lock() {
            if let Some(c) = guard.as_ref() {
                if c.at.elapsed() < TTL {
                    return c.map.get(&peer_port).copied();
                }
            }
        }
        let map = build_port_pid_map();
        let pid = map.get(&peer_port).copied();
        if let Ok(mut guard) = CACHE.lock() {
            *guard = Some(PortPidCache {
                at: Instant::now(),
                map,
            });
        }
        pid
    }
}

#[cfg(target_os = "windows")]
fn build_port_pid_map() -> HashMap<u16, u32> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut map: HashMap<u16, u32> = HashMap::new();
    let output = std::process::Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(output) = output else {
        return map;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    // 行形如:  TCP    127.0.0.1:54321    127.0.0.1:15721    ESTABLISHED    12345
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 5 || !f[0].eq_ignore_ascii_case("tcp") {
            continue;
        }
        let local = f[1];
        let Ok(pid) = f[f.len() - 1].parse::<u32>() else {
            continue;
        };
        if let Some(idx) = local.rfind(':') {
            if let Ok(port) = local[idx + 1..].parse::<u16>() {
                // 同一源端口对应唯一本地连接；首个即可
                map.entry(port).or_insert(pid);
            }
        }
    }
    map
}
