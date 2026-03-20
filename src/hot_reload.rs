#![allow(dead_code)]
//! v16.3 — Hot Reload Dev Mode [DX]
//!
//! `cargo run -- dev [--watch DIR] [--port P] [--cmd CMD]`
//!
//! Watch `src/` (hoặc DIR) → khi file .rs thay đổi → `cargo build` → restart server.
//! Print elapsed time mỗi lần rebuild.
//!
//! Flow:
//!   1. Spawn server (cargo run -- <cmd> <port>)
//!   2. Watch DIR với notify (kqueue/inotify/FSEvents tuỳ OS)
//!   3. Debounce 300ms (tránh rebuild liên tục khi save nhiều file)
//!   4. cargo build → in elapsed + result
//!   5. Nếu success → kill server cũ → spawn server mới
//!   6. Lặp lại

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};

// ── Config ────────────────────────────────────────────────────────────────────

pub struct DevConfig {
    pub watch_dir:    String,   // default "src"
    pub port:         u16,      // default 8080
    pub cmd:          String,   // subcommand sau rebuild (default "pktscan")
    pub debounce_ms:  u64,      // default 300ms
}

impl Default for DevConfig {
    fn default() -> Self {
        DevConfig {
            watch_dir:   "src".to_string(),
            port:        8080,
            cmd:         "pktscan".to_string(),
            debounce_ms: 300,
        }
    }
}

/// Parse `dev [--watch DIR] [--port P] [--cmd CMD] [--debounce MS]`
pub fn parse_dev_args(args: &[String]) -> DevConfig {
    let mut cfg = DevConfig::default();
    let mut i   = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--watch" | "-w" => {
                if let Some(v) = args.get(i + 1) { cfg.watch_dir = v.clone(); i += 1; }
            }
            "--port" | "-p" => {
                if let Some(v) = args.get(i + 1) {
                    if let Ok(p) = v.parse() { cfg.port = p; }
                    i += 1;
                }
            }
            "--cmd" | "-c" => {
                if let Some(v) = args.get(i + 1) { cfg.cmd = v.clone(); i += 1; }
            }
            "--debounce" => {
                if let Some(v) = args.get(i + 1) {
                    if let Ok(ms) = v.parse() { cfg.debounce_ms = ms; }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    cfg
}

// ── Build result ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct BuildResult {
    pub success:     bool,
    pub elapsed_ms:  u64,
    pub error_count: usize,
    pub stderr_tail: String,   // last N lines của stderr (để in khi lỗi)
}

impl BuildResult {
    pub fn elapsed_display(&self) -> String {
        format_elapsed(self.elapsed_ms)
    }
}

/// Chạy `cargo build` thật sự; trả về BuildResult.
pub fn run_cargo_build() -> BuildResult {
    let t0     = Instant::now();
    let output = Command::new("cargo")
        .arg("build")
        .output();

    let elapsed_ms = t0.elapsed().as_millis() as u64;

    match output {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            BuildResult {
                success:     out.status.success(),
                elapsed_ms,
                error_count: parse_error_count(&stderr),
                stderr_tail: tail_lines(&stderr, 8),
            }
        }
        Err(e) => BuildResult {
            success:     false,
            elapsed_ms,
            error_count: 1,
            stderr_tail: format!("failed to spawn cargo: {}", e),
        },
    }
}

/// Parse số lỗi từ `cargo build` stderr.
pub fn parse_error_count(stderr: &str) -> usize {
    stderr.lines()
        .filter(|l| l.starts_with("error[") || l.starts_with("error: "))
        .count()
}

/// Parse cargo build output: trả về (success, has_warnings).
pub fn parse_build_output(stderr: &str) -> (bool, bool) {
    let success  = stderr.contains("Finished")
        || (!stderr.contains("error[") && !stderr.contains("error: "));
    let warnings = stderr.contains("warning:");
    (success, warnings)
}

/// Lấy N dòng cuối cùng của string.
pub fn tail_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

// ── Format ────────────────────────────────────────────────────────────────────

/// Format elapsed time: "450ms" hoặc "1.23s".
pub fn format_elapsed(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.2}s", ms as f64 / 1000.0)
    }
}

// ── File listing ──────────────────────────────────────────────────────────────

/// Liệt kê tất cả file .rs trong dir (recursive).
/// Dùng để hiển thị "watching N files" khi khởi động.
pub fn list_watch_files(dir: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs_files(Path::new(dir), &mut files);
    files.sort();
    files
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

// ── Server process ────────────────────────────────────────────────────────────

/// Spawn server subprocess: `<current_exe> <cmd> <port>`
pub fn spawn_server(cmd: &str, port: u16) -> Option<Child> {
    let exe = std::env::current_exe().ok()?;
    Command::new(&exe)
        .arg(cmd)
        .arg(port.to_string())
        .spawn()
        .ok()
}

/// Kill + wait cho process cũ.
pub fn kill_server(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

// ── Hot reload event ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadEvent {
    FileChanged(PathBuf),
    BuildSuccess { elapsed_ms: u64 },
    BuildFailure { elapsed_ms: u64, errors: usize },
    ServerRestarted,
}

impl ReloadEvent {
    pub fn is_success(&self) -> bool {
        matches!(self, ReloadEvent::BuildSuccess { .. })
    }
}

// ── Main dev loop ─────────────────────────────────────────────────────────────

pub fn run_dev(config: DevConfig) {
    let files = list_watch_files(&config.watch_dir);

    println!();
    println!("  ╔═══════════════════════════════════════════╗");
    println!("  ║         PKT Hot Reload Dev  v16.3         ║");
    println!("  ╚═══════════════════════════════════════════╝");
    println!();
    println!("  Watch  : {}/  ({} .rs files)", config.watch_dir, files.len());
    println!("  Server : {} --port {}", config.cmd, config.port);
    println!("  Debounce: {}ms", config.debounce_ms);
    println!();

    // Spawn initial server
    let mut server = spawn_server(&config.cmd, config.port);
    if server.is_some() {
        println!("  ▶  Server started (port {})", config.port);
    } else {
        eprintln!("  ⚠  Could not spawn server — watch-only mode");
    }

    // Set up file watcher
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(move |res| { let _ = tx.send(res); }) {
        Ok(w) => w,
        Err(e) => { eprintln!("  ✗  Watcher error: {}", e); return; }
    };

    if let Err(e) = watcher.watch(Path::new(&config.watch_dir), RecursiveMode::Recursive) {
        eprintln!("  ✗  Watch error: {}", e);
        return;
    }

    println!("  Watching {}/ — Press Ctrl+C to stop.\n", config.watch_dir);

    let debounce = Duration::from_millis(config.debounce_ms);
    let mut last_event = Instant::now().checked_sub(debounce).unwrap_or(Instant::now());

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                // Filter: only .rs files
                let is_rs = event.paths.iter().any(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("rs")
                });
                if !is_rs { continue; }

                // Debounce: skip if too soon after last event
                if last_event.elapsed() < debounce { continue; }
                last_event = Instant::now();

                // Print which file changed
                if let Some(p) = event.paths.first() {
                    let short = p.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("?");
                    println!("  ⟳  {} changed — rebuilding…", short);
                }

                // Build
                let result = run_cargo_build();
                if result.success {
                    println!("  ✓  Rebuilt in {}  →  restarting server", result.elapsed_display());
                    if let Some(ref mut s) = server { kill_server(s); }
                    server = spawn_server(&config.cmd, config.port);
                    if server.is_some() {
                        println!("  ▶  Server restarted (port {})\n", config.port);
                    }
                } else {
                    println!("  ✗  Build failed in {} ({} error(s))",
                        result.elapsed_display(), result.error_count);
                    if !result.stderr_tail.is_empty() {
                        for line in result.stderr_tail.lines() {
                            println!("     {}", line);
                        }
                    }
                    println!();
                }
            }
            Ok(Err(e)) => eprintln!("  ⚠  Watch error: {}", e),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Check if server died unexpectedly
                if let Some(ref mut s) = server {
                    if let Ok(Some(_)) = s.try_wait() {
                        println!("  ⚠  Server exited — waiting for next change…");
                        server = None;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    // Cleanup
    if let Some(ref mut s) = server { kill_server(s); }
    println!("\n  Dev mode stopped.");
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_dev_args ────────────────────────────────────────────────────────

    #[test]
    fn parse_defaults() {
        let cfg = parse_dev_args(&[]);
        assert_eq!(cfg.watch_dir,   "src");
        assert_eq!(cfg.port,        8080);
        assert_eq!(cfg.cmd,         "pktscan");
        assert_eq!(cfg.debounce_ms, 300);
    }

    #[test]
    fn parse_watch_long() {
        let args = vec!["--watch".to_string(), "frontend".to_string()];
        assert_eq!(parse_dev_args(&args).watch_dir, "frontend");
    }

    #[test]
    fn parse_watch_short() {
        let args = vec!["-w".to_string(), "src".to_string()];
        assert_eq!(parse_dev_args(&args).watch_dir, "src");
    }

    #[test]
    fn parse_port_long() {
        let args = vec!["--port".to_string(), "9090".to_string()];
        assert_eq!(parse_dev_args(&args).port, 9090);
    }

    #[test]
    fn parse_port_short() {
        let args = vec!["-p".to_string(), "3000".to_string()];
        assert_eq!(parse_dev_args(&args).port, 3000);
    }

    #[test]
    fn parse_cmd_long() {
        let args = vec!["--cmd".to_string(), "devnet".to_string()];
        assert_eq!(parse_dev_args(&args).cmd, "devnet");
    }

    #[test]
    fn parse_cmd_short() {
        let args = vec!["-c".to_string(), "api".to_string()];
        assert_eq!(parse_dev_args(&args).cmd, "api");
    }

    #[test]
    fn parse_debounce() {
        let args = vec!["--debounce".to_string(), "500".to_string()];
        assert_eq!(parse_dev_args(&args).debounce_ms, 500);
    }

    #[test]
    fn parse_all_flags() {
        let args: Vec<String> = "--watch frontend --port 7777 --cmd devnet --debounce 200"
            .split_whitespace().map(|s| s.to_string()).collect();
        let cfg = parse_dev_args(&args);
        assert_eq!(cfg.watch_dir,   "frontend");
        assert_eq!(cfg.port,        7777);
        assert_eq!(cfg.cmd,         "devnet");
        assert_eq!(cfg.debounce_ms, 200);
    }

    #[test]
    fn parse_invalid_port_uses_default() {
        let args = vec!["--port".to_string(), "notaport".to_string()];
        assert_eq!(parse_dev_args(&args).port, 8080);
    }

    #[test]
    fn parse_unknown_flag_ignored() {
        let args = vec!["--unknown".to_string(), "value".to_string()];
        let cfg  = parse_dev_args(&args);
        assert_eq!(cfg.watch_dir, "src");
    }

    // ── format_elapsed ────────────────────────────────────────────────────────

    #[test]
    fn format_elapsed_zero() {
        assert_eq!(format_elapsed(0), "0ms");
    }

    #[test]
    fn format_elapsed_under_1s() {
        assert_eq!(format_elapsed(450), "450ms");
        assert_eq!(format_elapsed(999), "999ms");
    }

    #[test]
    fn format_elapsed_exactly_1s() {
        assert_eq!(format_elapsed(1000), "1.00s");
    }

    #[test]
    fn format_elapsed_over_1s() {
        assert_eq!(format_elapsed(1230), "1.23s");
        assert_eq!(format_elapsed(5500), "5.50s");
    }

    #[test]
    fn format_elapsed_large() {
        assert_eq!(format_elapsed(30000), "30.00s");
    }

    // ── parse_error_count ─────────────────────────────────────────────────────

    #[test]
    fn error_count_zero_on_clean_output() {
        let clean = "   Compiling blockchain-rust v0.1.0\n    Finished `dev` profile\n";
        assert_eq!(parse_error_count(clean), 0);
    }

    #[test]
    fn error_count_one_error() {
        let output = "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n";
        assert_eq!(parse_error_count(output), 1);
    }

    #[test]
    fn error_count_multiple_errors() {
        let output = "error[E0308]: type\nerror[E0425]: not found\nerror: aborting\n";
        assert_eq!(parse_error_count(output), 3);
    }

    // ── parse_build_output ────────────────────────────────────────────────────

    #[test]
    fn parse_output_success_with_finished() {
        let stderr = "   Compiling foo\n    Finished `dev` profile [unoptimized]\n";
        let (ok, _warnings) = parse_build_output(stderr);
        assert!(ok);
    }

    #[test]
    fn parse_output_failure_with_error() {
        let stderr = "error[E0308]: mismatched types\nerror: aborting\n";
        let (ok, _) = parse_build_output(stderr);
        assert!(!ok);
    }

    #[test]
    fn parse_output_warnings_detected() {
        let stderr = "warning: unused variable `x`\n    Finished `dev`\n";
        let (ok, warnings) = parse_build_output(stderr);
        assert!(ok);
        assert!(warnings);
    }

    #[test]
    fn parse_output_no_warnings() {
        let stderr = "    Finished `dev` profile\n";
        let (ok, warnings) = parse_build_output(stderr);
        assert!(ok);
        assert!(!warnings);
    }

    // ── tail_lines ────────────────────────────────────────────────────────────

    #[test]
    fn tail_lines_fewer_than_n() {
        let s = "a\nb\nc";
        assert_eq!(tail_lines(s, 10), "a\nb\nc");
    }

    #[test]
    fn tail_lines_exact_n() {
        let s = "a\nb\nc";
        assert_eq!(tail_lines(s, 3), "a\nb\nc");
    }

    #[test]
    fn tail_lines_more_than_n() {
        let s = "a\nb\nc\nd\ne";
        let result = tail_lines(s, 3);
        assert_eq!(result, "c\nd\ne");
    }

    #[test]
    fn tail_lines_empty_string() {
        assert_eq!(tail_lines("", 5), "");
    }

    // ── list_watch_files (real filesystem) ───────────────────────────────────

    #[test]
    fn list_watch_files_src_has_rs_files() {
        let files = list_watch_files("src");
        assert!(!files.is_empty(), "src/ must have at least one .rs file");
    }

    #[test]
    fn list_watch_files_src_has_main_rs() {
        let files = list_watch_files("src");
        assert!(
            files.iter().any(|f| f.file_name().and_then(|n| n.to_str()) == Some("main.rs")),
            "src/ must contain main.rs"
        );
    }

    #[test]
    fn list_watch_files_src_has_chain_rs() {
        let files = list_watch_files("src");
        assert!(
            files.iter().any(|f| f.file_name().and_then(|n| n.to_str()) == Some("chain.rs")),
            "src/ must contain chain.rs"
        );
    }

    #[test]
    fn list_watch_files_only_rs_extension() {
        let files = list_watch_files("src");
        for f in &files {
            assert_eq!(
                f.extension().and_then(|e| e.to_str()),
                Some("rs"),
                "list_watch_files must only return .rs files, got: {}",
                f.display()
            );
        }
    }

    #[test]
    fn list_watch_files_sorted() {
        let files = list_watch_files("src");
        let sorted = {
            let mut v = files.clone();
            v.sort();
            v
        };
        assert_eq!(files, sorted, "list_watch_files must return sorted paths");
    }

    #[test]
    fn list_watch_files_nonexistent_dir_returns_empty() {
        let files = list_watch_files("/nonexistent/path/that/does/not/exist");
        assert!(files.is_empty());
    }

    #[test]
    fn list_watch_files_count_reasonable() {
        let files = list_watch_files("src");
        // Có ít nhất 50 modules (theo CONTEXT.md)
        assert!(files.len() >= 50,
            "src/ should have at least 50 .rs files, got {}", files.len());
    }

    // ── ReloadEvent ───────────────────────────────────────────────────────────

    #[test]
    fn reload_event_build_success_is_success() {
        let ev = ReloadEvent::BuildSuccess { elapsed_ms: 1000 };
        assert!(ev.is_success());
    }

    #[test]
    fn reload_event_build_failure_not_success() {
        let ev = ReloadEvent::BuildFailure { elapsed_ms: 500, errors: 2 };
        assert!(!ev.is_success());
    }

    #[test]
    fn reload_event_file_changed_not_success() {
        let ev = ReloadEvent::FileChanged(PathBuf::from("src/main.rs"));
        assert!(!ev.is_success());
    }

    // ── BuildResult ───────────────────────────────────────────────────────────

    #[test]
    fn build_result_elapsed_display_ms() {
        let r = BuildResult { success: true, elapsed_ms: 450, error_count: 0, stderr_tail: String::new() };
        assert_eq!(r.elapsed_display(), "450ms");
    }

    #[test]
    fn build_result_elapsed_display_secs() {
        let r = BuildResult { success: true, elapsed_ms: 3200, error_count: 0, stderr_tail: String::new() };
        assert_eq!(r.elapsed_display(), "3.20s");
    }
}
