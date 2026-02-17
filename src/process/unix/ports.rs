use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::Duration;

use colored::Colorize;

/// Get a map of PID -> list of listening TCP ports.
/// Runs a single `lsof` command and parses the output.
/// Returns an empty map on any failure (never crashes).
pub fn get_listening_ports() -> HashMap<i64, Vec<u16>> {
    get_listening_ports_inner().unwrap_or_default()
}

/// Check if a TCP port is open by attempting a connection.
/// Uses a short timeout so it won't block.
pub fn is_port_open(port: u16) -> bool {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    TcpStream::connect_timeout(&addr, Duration::from_millis(150)).is_ok()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn get_listening_ports_inner() -> Option<HashMap<i64, Vec<u16>>> {
    let output = Command::new("lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-P", "-n"])
        .output()
        .ok()?;

    if !output.status.success() {
        return try_ss_fallback();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(parse_lsof_output(&stdout))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn get_listening_ports_inner() -> Option<HashMap<i64, Vec<u16>>> {
    None
}

/// Fallback for Linux systems without lsof — try `ss -tlnp`
fn try_ss_fallback() -> Option<HashMap<i64, Vec<u16>>> {
    let output = Command::new("ss")
        .args(["-tlnp"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(parse_ss_output(&stdout))
}

/// Parse lsof output lines like:
/// ```text
/// COMMAND   PID USER   FD   TYPE  DEVICE SIZE/OFF NODE NAME
/// node    12345 user   23u  IPv6  0x...  0t0      TCP  *:3000 (LISTEN)
/// ```
fn parse_lsof_output(output: &str) -> HashMap<i64, Vec<u16>> {
    let mut map: HashMap<i64, Vec<u16>> = HashMap::new();

    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        let pid = match parts[1].parse::<i64>() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // NAME is the last column(s), e.g. "*:3000" or "127.0.0.1:8080"
        let name = parts[8];
        if let Some(port) = extract_port_from_lsof_name(name) {
            let entry = map.entry(pid).or_default();
            if !entry.contains(&port) {
                entry.push(port);
            }
        }
    }

    map
}

/// Extract port from lsof NAME field like "*:3000", "127.0.0.1:8080", "[::1]:443"
fn extract_port_from_lsof_name(name: &str) -> Option<u16> {
    let port_str = name.rsplit(':').next()?;
    port_str.parse::<u16>().ok()
}

/// Parse ss output lines like:
/// ```text
/// State  Recv-Q Send-Q Local Address:Port  Peer Address:Port  Process
/// LISTEN 0      128    0.0.0.0:3000        0.0.0.0:*          users:(("node",pid=12345,fd=23))
/// ```
fn parse_ss_output(output: &str) -> HashMap<i64, Vec<u16>> {
    let mut map: HashMap<i64, Vec<u16>> = HashMap::new();

    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }

        let local_addr = parts[3];
        let port = match local_addr.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) {
            Some(p) => p,
            None => continue,
        };

        let process_field = parts.last().unwrap_or(&"");
        if let Some(pid) = extract_pid_from_ss(process_field) {
            let entry = map.entry(pid).or_default();
            if !entry.contains(&port) {
                entry.push(port);
            }
        }
    }

    map
}

/// Extract PID from ss process field like `users:(("node",pid=12345,fd=23))`
fn extract_pid_from_ss(field: &str) -> Option<i64> {
    let pid_start = field.find("pid=")?;
    let after_pid = &field[pid_start + 4..];
    let pid_str: String = after_pid.chars().take_while(|c| c.is_ascii_digit()).collect();
    pid_str.parse::<i64>().ok()
}

/// Format ports with color: green if open (TCP connect succeeds), red if closed.
/// Returns a colored string like "3000, 8080" or "-" if no ports.
pub fn format_ports_colored(ports: &[u16]) -> String {
    if ports.is_empty() {
        return "-".bright_black().to_string();
    }
    let mut sorted = ports.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted
        .iter()
        .map(|&p| {
            if is_port_open(p) {
                p.to_string().green().to_string()
            } else {
                p.to_string().red().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format a list of ports for display (plain, no color).
pub fn format_ports(ports: &[u16]) -> String {
    if ports.is_empty() {
        return String::from("-");
    }
    let mut sorted = ports.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
