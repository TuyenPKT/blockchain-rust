#![allow(dead_code)]
//! v14.4 — Shell Completions
//!
//! Sinh completion script cho bash / zsh / fish.
//!
//! Sử dụng:
//!   cargo run -- completions bash   > ~/.local/share/bash-completion/completions/pkt
//!   cargo run -- completions zsh    > ~/.zfunc/_pkt
//!   cargo run -- completions fish   > ~/.config/fish/completions/pkt.fish
//!
//! Hoặc source trực tiếp:
//!   source <(cargo run -- completions bash)
//!   source <(cargo run -- completions zsh)

// ── Danh sách lệnh & sub-lệnh ────────────────────────────────────────────────

/// Tất cả top-level subcommands
pub const TOP_COMMANDS: &[&str] = &[
    "wallet", "mine", "cpumine", "gpumine", "node", "pktscan",
    "explorer", "testnet", "genesis", "metrics", "monitor",
    "bench", "blake3", "token", "contract", "staking", "deploy",
    "apikey", "hw-info", "qr", "completions",
];

pub const WALLET_CMDS: &[&str]      = &["new", "show", "restore"];
pub const EXPLORER_CMDS: &[&str]    = &["chain", "block", "tx", "balance", "utxo"];
pub const GENESIS_NETS: &[&str]     = &["mainnet", "testnet", "regtest"];
pub const BENCH_TARGETS: &[&str]    = &["all", "hash", "tps", "mining", "merkle", "utxo", "mempool"];
pub const GPU_BACKENDS: &[&str]     = &["software", "opencl", "cuda"];
pub const TOKEN_CMDS: &[&str]       = &["create", "list", "info", "mint", "transfer", "balance"];
pub const CONTRACT_CMDS: &[&str]    = &["deploy", "list", "info", "call", "state", "estimate"];
pub const CONTRACT_TPLS: &[&str]    = &["counter", "token", "voting"];
pub const STAKING_CMDS: &[&str]     = &["validators", "register", "delegate", "undelegate",
                                         "rewards", "claim", "info", "slash"];
pub const DEPLOY_CMDS: &[&str]      = &["init", "dockerfile", "compose", "systemd",
                                         "env", "nginx", "frontend", "config"];
pub const APIKEY_CMDS: &[&str]      = &["new", "list"];
pub const SHELL_NAMES: &[&str]      = &["bash", "zsh", "fish"];

// ── Shell enum ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Unknown(String),
}

impl Shell {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "bash" => Shell::Bash,
            "zsh"  => Shell::Zsh,
            "fish" => Shell::Fish,
            other  => Shell::Unknown(other.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Shell::Bash       => "bash",
            Shell::Zsh        => "zsh",
            Shell::Fish       => "fish",
            Shell::Unknown(s) => s.as_str(),
        }
    }

    pub fn is_supported(&self) -> bool {
        !matches!(self, Shell::Unknown(_))
    }
}

// ── Bash completions ──────────────────────────────────────────────────────────

pub fn generate_bash() -> String {
    let top   = TOP_COMMANDS.join(" ");
    let bench = BENCH_TARGETS.join(" ");
    let gpu   = GPU_BACKENDS.join(" ");
    let shells = SHELL_NAMES.join(" ");

    format!(r#"# PKT Blockchain — bash completions
# Source này hoặc lưu vào ~/.local/share/bash-completion/completions/pkt
# source <(cargo run -- completions bash)

_pkt_completions() {{
    local cur prev words
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    words="${{COMP_WORDS[*]}}"

    # Sub-command completions
    case "${{prev}}" in
        wallet)
            COMPREPLY=($(compgen -W "{wallet}" -- "${{cur}}"))
            return ;;
        explorer)
            COMPREPLY=($(compgen -W "{explorer}" -- "${{cur}}"))
            return ;;
        genesis)
            COMPREPLY=($(compgen -W "{genesis}" -- "${{cur}}"))
            return ;;
        bench)
            COMPREPLY=($(compgen -W "{bench}" -- "${{cur}}"))
            return ;;
        gpumine)
            # 4th arg là backend
            if [[ "${{#COMP_WORDS[@]}}" -ge 5 ]]; then
                COMPREPLY=($(compgen -W "{gpu}" -- "${{cur}}"))
            fi
            return ;;
        token)
            COMPREPLY=($(compgen -W "{token}" -- "${{cur}}"))
            return ;;
        contract)
            COMPREPLY=($(compgen -W "{contract}" -- "${{cur}}"))
            return ;;
        staking)
            COMPREPLY=($(compgen -W "{staking}" -- "${{cur}}"))
            return ;;
        deploy)
            COMPREPLY=($(compgen -W "{deploy}" -- "${{cur}}"))
            return ;;
        apikey)
            COMPREPLY=($(compgen -W "{apikey}" -- "${{cur}}"))
            return ;;
        completions)
            COMPREPLY=($(compgen -W "{shells}" -- "${{cur}}"))
            return ;;
    esac

    # Top-level
    COMPREPLY=($(compgen -W "{top}" -- "${{cur}}"))
}}

complete -F _pkt_completions pkt
# Dùng với cargo run:
complete -F _pkt_completions cargo
"#,
        wallet   = WALLET_CMDS.join(" "),
        explorer = EXPLORER_CMDS.join(" "),
        genesis  = GENESIS_NETS.join(" "),
        bench    = bench,
        gpu      = gpu,
        token    = TOKEN_CMDS.join(" "),
        contract = CONTRACT_CMDS.join(" "),
        staking  = STAKING_CMDS.join(" "),
        deploy   = DEPLOY_CMDS.join(" "),
        apikey   = APIKEY_CMDS.join(" "),
        shells   = shells,
        top      = top,
    )
}

// ── Zsh completions ───────────────────────────────────────────────────────────

pub fn generate_zsh() -> String {
    let top_desc: Vec<String> = TOP_COMMANDS.iter().map(|c| {
        let desc = match *c {
            "wallet"      => "Quản lý ví PKT (new/show/restore)",
            "mine"        => "PoW miner đơn giản",
            "cpumine"     => "CPU multi-thread miner (rayon)",
            "gpumine"     => "GPU miner (software/opencl/cuda)",
            "node"        => "Chạy P2P node",
            "pktscan"     => "Block explorer + API server",
            "explorer"    => "Block explorer CLI",
            "testnet"     => "Local testnet nhiều nodes",
            "genesis"     => "Xem genesis config",
            "metrics"     => "Hiển thị metrics node",
            "monitor"     => "Health check server",
            "bench"       => "Benchmark suite",
            "blake3"      => "BLAKE3 vs SHA-256 benchmark",
            "token"       => "Quản lý ERC-20 token",
            "contract"    => "Deploy và gọi smart contract",
            "staking"     => "Staking và delegation",
            "deploy"      => "Sinh config Dockerfile/systemd/nginx",
            "apikey"      => "Quản lý API keys",
            "hw-info"     => "Phát hiện hardware và miner config",
            "qr"          => "Hiển thị QR code địa chỉ ví",
            "completions" => "Sinh shell completion script",
            _             => "",
        };
        format!("        '{}:{}'", c, desc)
    }).collect();

    format!(r#"#compdef pkt
# PKT Blockchain — zsh completions
# Lưu vào ~/.zfunc/_pkt, thêm ~/.zshrc: fpath=(~/.zfunc $fpath); autoload -U compinit; compinit
# Hoặc: source <(cargo run -- completions zsh)

_pkt() {{
    local context state state_descr line
    typeset -A opt_args

    _arguments \
        '1:command:->command' \
        '*::args:->args'

    case $state in
        command)
            local commands
            commands=(
{top_desc}
            )
            _describe 'command' commands
            ;;
        args)
            case $line[1] in
                wallet)
                    _values 'subcommand' new show restore ;;
                explorer)
                    _values 'subcommand' chain block tx balance utxo ;;
                genesis)
                    _values 'network' mainnet testnet regtest ;;
                bench)
                    _values 'target' all hash tps mining merkle utxo mempool ;;
                gpumine)
                    _values 'backend' software opencl cuda ;;
                token)
                    _values 'subcommand' create list info mint transfer balance ;;
                contract)
                    _values 'subcommand' deploy list info call state estimate ;;
                staking)
                    _values 'subcommand' validators register delegate undelegate rewards claim info slash ;;
                deploy)
                    _values 'subcommand' init dockerfile compose systemd env nginx frontend config ;;
                apikey)
                    _values 'subcommand' new list ;;
                completions)
                    _values 'shell' bash zsh fish ;;
            esac
            ;;
    esac
}}

_pkt
"#,
        top_desc = top_desc.join("\n"),
    )
}

// ── Fish completions ──────────────────────────────────────────────────────────

pub fn generate_fish() -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push("# PKT Blockchain — fish completions".to_string());
    lines.push("# Lưu vào ~/.config/fish/completions/pkt.fish".to_string());
    lines.push("# Hoặc: cargo run -- completions fish | source".to_string());
    lines.push(String::new());

    // Disable file completion cho pkt
    lines.push("complete -c pkt -f".to_string());
    lines.push(String::new());

    // Top-level commands
    lines.push("# Top-level commands".to_string());
    let cmd_descs: &[(&str, &str)] = &[
        ("wallet",      "Quản lý ví PKT (new/show/restore)"),
        ("mine",        "PoW miner đơn giản"),
        ("cpumine",     "CPU multi-thread miner (rayon)"),
        ("gpumine",     "GPU miner (software/opencl/cuda)"),
        ("node",        "Chạy P2P node"),
        ("pktscan",     "Block explorer + API server"),
        ("explorer",    "Block explorer CLI"),
        ("testnet",     "Local testnet nhiều nodes"),
        ("genesis",     "Xem genesis config (mainnet/testnet/regtest)"),
        ("metrics",     "Hiển thị metrics node"),
        ("monitor",     "Health check server"),
        ("bench",       "Benchmark suite"),
        ("blake3",      "BLAKE3 vs SHA-256 benchmark"),
        ("token",       "Quản lý ERC-20 token"),
        ("contract",    "Deploy và gọi smart contract"),
        ("staking",     "Staking và delegation"),
        ("deploy",      "Sinh config Dockerfile/systemd/nginx"),
        ("apikey",      "Quản lý API keys"),
        ("hw-info",     "Phát hiện hardware và miner config"),
        ("qr",          "Hiển thị QR code địa chỉ ví (BIP21)"),
        ("completions", "Sinh shell completion script"),
    ];
    for (cmd, desc) in cmd_descs {
        lines.push(format!(
            "complete -c pkt -n '__fish_use_subcommand' -a '{}' -d '{}'",
            cmd, desc
        ));
    }
    lines.push(String::new());

    // Helper macro tạo sub-completions
    let sub_groups: &[(&str, &[(&str, &str)])] = &[
        ("wallet", &[
            ("new",     "Tạo ví mới (BIP39 12 từ)"),
            ("show",    "Xem ví + seed phrase"),
            ("restore", "Khôi phục ví từ mnemonic"),
        ]),
        ("explorer", &[
            ("chain",   "Hiển thị toàn bộ chain"),
            ("block",   "Block theo height"),
            ("tx",      "TX theo txid"),
            ("balance", "Số dư địa chỉ"),
            ("utxo",    "UTXO của địa chỉ"),
        ]),
        ("genesis", &[
            ("mainnet", "Mainnet genesis params"),
            ("testnet", "Testnet genesis params"),
            ("regtest", "Regtest genesis params"),
        ]),
        ("bench", &[
            ("all",     "Chạy tất cả benchmarks"),
            ("hash",    "Hash throughput"),
            ("tps",     "Transactions per second"),
            ("mining",  "Block mining latency"),
            ("merkle",  "Merkle tree performance"),
            ("utxo",    "UTXO scan vs index"),
            ("mempool", "Mempool selection"),
        ]),
        ("token", &[
            ("create",   "Tạo token mới"),
            ("list",     "Danh sách tokens"),
            ("info",     "Thông tin token"),
            ("mint",     "Mint thêm token"),
            ("transfer", "Transfer token"),
            ("balance",  "Xem balance token"),
        ]),
        ("contract", &[
            ("deploy",   "Deploy smart contract"),
            ("list",     "Danh sách contracts"),
            ("info",     "Thông tin contract"),
            ("call",     "Gọi method của contract"),
            ("state",    "Xem state contract"),
            ("estimate", "Ước tính gas"),
        ]),
        ("staking", &[
            ("validators", "Danh sách validators"),
            ("register",   "Đăng ký validator"),
            ("delegate",   "Delegate stake"),
            ("undelegate", "Undelegate stake"),
            ("rewards",    "Xem rewards"),
            ("claim",      "Claim rewards"),
            ("info",       "Thông tin validator"),
            ("slash",      "Slash validator"),
        ]),
        ("deploy", &[
            ("init",       "Khởi tạo deploy config"),
            ("dockerfile", "Sinh Dockerfile"),
            ("compose",    "Sinh docker-compose.yml"),
            ("systemd",    "Sinh systemd service"),
            ("env",        "Sinh .env file"),
            ("nginx",      "Sinh nginx config"),
            ("frontend",   "Sinh frontend bundle"),
            ("config",     "Xem config hiện tại"),
        ]),
        ("apikey", &[
            ("new",  "Tạo API key mới"),
            ("list", "Danh sách API keys"),
        ]),
        ("completions", &[
            ("bash", "Sinh bash completion script"),
            ("zsh",  "Sinh zsh completion script"),
            ("fish", "Sinh fish completion script"),
        ]),
    ];

    for (parent, subs) in sub_groups {
        lines.push(format!("# {} subcommands", parent));
        for (sub, desc) in *subs {
            lines.push(format!(
                "complete -c pkt -n '__fish_seen_subcommand_from {}' -a '{}' -d '{}'",
                parent, sub, desc
            ));
        }
        lines.push(String::new());
    }

    // gpumine backend (4th positional)
    lines.push("# gpumine backend".to_string());
    for (backend, desc) in &[
        ("software", "CPU rayon fallback"),
        ("opencl",   "OpenCL GPU (--features opencl)"),
        ("cuda",     "CUDA GPU (--features cuda)"),
    ] {
        lines.push(format!(
            "complete -c pkt -n '__fish_seen_subcommand_from gpumine' -a '{}' -d '{}'",
            backend, desc
        ));
    }

    lines.join("\n")
}

// ── Completion metadata ────────────────────────────────────────────────────────

/// Thông tin install cho từng shell
pub struct InstallHint {
    pub shell:   &'static str,
    pub steps:   Vec<&'static str>,
}

pub fn install_hint(shell: &Shell) -> InstallHint {
    match shell {
        Shell::Bash => InstallHint {
            shell: "bash",
            steps: vec![
                "# Cách 1 — source tạm thời:",
                "  source <(cargo run -- completions bash)",
                "",
                "# Cách 2 — lưu vĩnh viễn:",
                "  cargo run -- completions bash > ~/.local/share/bash-completion/completions/pkt",
                "  # Sau đó mở terminal mới hoặc: source ~/.bashrc",
            ],
        },
        Shell::Zsh => InstallHint {
            shell: "zsh",
            steps: vec![
                "# Cách 1 — source tạm thời:",
                "  source <(cargo run -- completions zsh)",
                "",
                "# Cách 2 — lưu vào ~/.zfunc:",
                "  mkdir -p ~/.zfunc",
                "  cargo run -- completions zsh > ~/.zfunc/_pkt",
                "  # Thêm vào ~/.zshrc (nếu chưa có):",
                "  #   fpath=(~/.zfunc $fpath)",
                "  #   autoload -U compinit && compinit",
                "  source ~/.zshrc",
            ],
        },
        Shell::Fish => InstallHint {
            shell: "fish",
            steps: vec![
                "# Cách 1 — source tạm thời:",
                "  cargo run -- completions fish | source",
                "",
                "# Cách 2 — lưu vĩnh viễn:",
                "  cargo run -- completions fish > ~/.config/fish/completions/pkt.fish",
                "  # Fish tự load khi khởi động shell mới",
            ],
        },
        Shell::Unknown(_) => InstallHint {
            shell: "unknown",
            steps: vec!["Hỗ trợ: bash, zsh, fish"],
        },
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Sinh completion script cho shell chỉ định
pub fn generate(shell: &Shell) -> Option<String> {
    match shell {
        Shell::Bash       => Some(generate_bash()),
        Shell::Zsh        => Some(generate_zsh()),
        Shell::Fish       => Some(generate_fish()),
        Shell::Unknown(_) => None,
    }
}

/// `cargo run -- completions <bash|zsh|fish>`
pub fn cmd_completions(args: &[String]) {
    let shell_name = args.first().map(|s| s.as_str()).unwrap_or("");

    if shell_name.is_empty() {
        eprintln!("Usage: cargo run -- completions <bash|zsh|fish>");
        eprintln!();
        eprintln!("Hỗ trợ:");
        for s in SHELL_NAMES {
            eprintln!("  {}", s);
        }
        return;
    }

    let shell = Shell::from_str(shell_name);

    match generate(&shell) {
        Some(script) => {
            print!("{}", script);

            // In install hint ra stderr
            let hint = install_hint(&shell);
            eprintln!();
            eprintln!("# ── Cách cài đặt ({}) ──────────────────────────", hint.shell);
            for line in &hint.steps {
                eprintln!("{}", line);
            }
        }
        None => {
            eprintln!("Shell không được hỗ trợ: '{}'", shell_name);
            eprintln!("Hỗ trợ: {}", SHELL_NAMES.join(", "));
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Shell::from_str ──────────────────────────────────────────────────

    #[test]
    fn test_shell_from_str_bash() {
        assert_eq!(Shell::from_str("bash"), Shell::Bash);
    }

    #[test]
    fn test_shell_from_str_zsh() {
        assert_eq!(Shell::from_str("zsh"), Shell::Zsh);
    }

    #[test]
    fn test_shell_from_str_fish() {
        assert_eq!(Shell::from_str("fish"), Shell::Fish);
    }

    #[test]
    fn test_shell_from_str_case_insensitive() {
        assert_eq!(Shell::from_str("BASH"), Shell::Bash);
        assert_eq!(Shell::from_str("Zsh"),  Shell::Zsh);
        assert_eq!(Shell::from_str("FISH"), Shell::Fish);
    }

    #[test]
    fn test_shell_from_str_unknown() {
        assert_eq!(Shell::from_str("powershell"), Shell::Unknown("powershell".to_string()));
    }

    #[test]
    fn test_shell_name_bash() {
        assert_eq!(Shell::Bash.name(), "bash");
    }

    #[test]
    fn test_shell_name_zsh() {
        assert_eq!(Shell::Zsh.name(), "zsh");
    }

    #[test]
    fn test_shell_name_fish() {
        assert_eq!(Shell::Fish.name(), "fish");
    }

    #[test]
    fn test_shell_is_supported() {
        assert!(Shell::Bash.is_supported());
        assert!(Shell::Zsh.is_supported());
        assert!(Shell::Fish.is_supported());
        assert!(!Shell::Unknown("ps1".to_string()).is_supported());
    }

    // ── generate_bash ────────────────────────────────────────────────────

    #[test]
    fn test_bash_not_empty() {
        assert!(!generate_bash().is_empty());
    }

    #[test]
    fn test_bash_contains_function() {
        assert!(generate_bash().contains("_pkt_completions"));
    }

    #[test]
    fn test_bash_contains_complete() {
        assert!(generate_bash().contains("complete -F"));
    }

    #[test]
    fn test_bash_contains_wallet() {
        assert!(generate_bash().contains("wallet"));
    }

    #[test]
    fn test_bash_contains_all_top_commands() {
        let script = generate_bash();
        for cmd in TOP_COMMANDS {
            assert!(script.contains(cmd), "bash thiếu lệnh: {}", cmd);
        }
    }

    #[test]
    fn test_bash_contains_bench_targets() {
        let script = generate_bash();
        for t in BENCH_TARGETS {
            assert!(script.contains(t), "bash bench thiếu: {}", t);
        }
    }

    #[test]
    fn test_bash_contains_gpu_backends() {
        let script = generate_bash();
        for b in GPU_BACKENDS {
            assert!(script.contains(b), "bash gpu thiếu: {}", b);
        }
    }

    #[test]
    fn test_bash_contains_shell_names() {
        let script = generate_bash();
        for s in SHELL_NAMES {
            assert!(script.contains(s), "bash shells thiếu: {}", s);
        }
    }

    // ── generate_zsh ─────────────────────────────────────────────────────

    #[test]
    fn test_zsh_not_empty() {
        assert!(!generate_zsh().is_empty());
    }

    #[test]
    fn test_zsh_contains_compdef() {
        assert!(generate_zsh().contains("#compdef"));
    }

    #[test]
    fn test_zsh_contains_arguments() {
        assert!(generate_zsh().contains("_arguments"));
    }

    #[test]
    fn test_zsh_contains_all_top_commands() {
        let script = generate_zsh();
        for cmd in TOP_COMMANDS {
            assert!(script.contains(cmd), "zsh thiếu lệnh: {}", cmd);
        }
    }

    #[test]
    fn test_zsh_contains_token_cmds() {
        let script = generate_zsh();
        for c in TOKEN_CMDS {
            assert!(script.contains(c), "zsh token thiếu: {}", c);
        }
    }

    #[test]
    fn test_zsh_contains_staking_cmds() {
        let script = generate_zsh();
        for c in STAKING_CMDS {
            assert!(script.contains(c), "zsh staking thiếu: {}", c);
        }
    }

    #[test]
    fn test_zsh_contains_deploy_cmds() {
        let script = generate_zsh();
        for c in DEPLOY_CMDS {
            assert!(script.contains(c), "zsh deploy thiếu: {}", c);
        }
    }

    // ── generate_fish ─────────────────────────────────────────────────────

    #[test]
    fn test_fish_not_empty() {
        assert!(!generate_fish().is_empty());
    }

    #[test]
    fn test_fish_contains_complete() {
        assert!(generate_fish().contains("complete -c pkt"));
    }

    #[test]
    fn test_fish_contains_no_file_completion() {
        assert!(generate_fish().contains("complete -c pkt -f"));
    }

    #[test]
    fn test_fish_contains_all_top_commands() {
        let script = generate_fish();
        for cmd in TOP_COMMANDS {
            assert!(script.contains(cmd), "fish thiếu lệnh: {}", cmd);
        }
    }

    #[test]
    fn test_fish_contains_contract_cmds() {
        let script = generate_fish();
        for c in CONTRACT_CMDS {
            assert!(script.contains(c), "fish contract thiếu: {}", c);
        }
    }

    #[test]
    fn test_fish_contains_gpu_backends() {
        let script = generate_fish();
        for b in GPU_BACKENDS {
            assert!(script.contains(b), "fish gpu thiếu: {}", b);
        }
    }

    #[test]
    fn test_fish_contains_shell_completions_group() {
        let script = generate_fish();
        assert!(script.contains("seen_subcommand_from completions"));
    }

    // ── generate() dispatcher ────────────────────────────────────────────

    #[test]
    fn test_generate_bash() {
        assert!(generate(&Shell::Bash).is_some());
    }

    #[test]
    fn test_generate_zsh() {
        assert!(generate(&Shell::Zsh).is_some());
    }

    #[test]
    fn test_generate_fish() {
        assert!(generate(&Shell::Fish).is_some());
    }

    #[test]
    fn test_generate_unknown_returns_none() {
        assert!(generate(&Shell::Unknown("ps1".to_string())).is_none());
    }

    #[test]
    fn test_generate_bash_zsh_differ() {
        assert_ne!(generate(&Shell::Bash), generate(&Shell::Zsh));
    }

    #[test]
    fn test_generate_fish_differs_from_bash() {
        assert_ne!(generate(&Shell::Fish), generate(&Shell::Bash));
    }

    // ── constants ────────────────────────────────────────────────────────

    #[test]
    fn test_top_commands_not_empty() {
        assert!(!TOP_COMMANDS.is_empty());
    }

    #[test]
    fn test_shell_names_count() {
        assert_eq!(SHELL_NAMES.len(), 3);
    }

    #[test]
    fn test_bench_targets_has_all() {
        assert!(BENCH_TARGETS.contains(&"all"));
    }

    #[test]
    fn test_contract_templates_nonempty() {
        assert!(!CONTRACT_TPLS.is_empty());
    }

    #[test]
    fn test_top_commands_contains_qr() {
        assert!(TOP_COMMANDS.contains(&"qr"));
    }

    #[test]
    fn test_top_commands_contains_completions() {
        assert!(TOP_COMMANDS.contains(&"completions"));
    }

    #[test]
    fn test_top_commands_contains_hw_info() {
        assert!(TOP_COMMANDS.contains(&"hw-info"));
    }

    // ── install_hint ─────────────────────────────────────────────────────

    #[test]
    fn test_install_hint_bash_has_steps() {
        let h = install_hint(&Shell::Bash);
        assert!(!h.steps.is_empty());
        assert_eq!(h.shell, "bash");
    }

    #[test]
    fn test_install_hint_zsh_mentions_zfunc() {
        let h = install_hint(&Shell::Zsh);
        assert!(h.steps.iter().any(|s| s.contains(".zfunc")));
    }

    #[test]
    fn test_install_hint_fish_mentions_config() {
        let h = install_hint(&Shell::Fish);
        assert!(h.steps.iter().any(|s| s.contains(".config/fish")));
    }

    #[test]
    fn test_install_hint_unknown_has_steps() {
        let h = install_hint(&Shell::Unknown("x".to_string()));
        assert!(!h.steps.is_empty());
    }

    // ── cmd_completions smoke test ────────────────────────────────────────

    #[test]
    fn test_cmd_completions_no_panic_bash() {
        cmd_completions(&["bash".to_string()]);
    }

    #[test]
    fn test_cmd_completions_no_panic_zsh() {
        cmd_completions(&["zsh".to_string()]);
    }

    #[test]
    fn test_cmd_completions_no_panic_fish() {
        cmd_completions(&["fish".to_string()]);
    }

    #[test]
    fn test_cmd_completions_no_panic_unknown() {
        cmd_completions(&["powershell".to_string()]);
    }

    #[test]
    fn test_cmd_completions_no_panic_empty() {
        cmd_completions(&[]);
    }
}
