#![allow(dead_code)]
//! v16.1 — Dev Docs Generator [DX]
//!
//! `cargo run -- docs [--out DIR]`
//!
//! Sinh 3 file markdown từ metadata thật của hệ thống:
//!   docs/api.md          ← danh sách API endpoints thật
//!   docs/cli.md          ← danh sách CLI commands thật
//!   docs/architecture.md ← danh sách modules thật
//!
//! Không dùng template engine hay external crate.

use std::path::{Path, PathBuf};

// ── API Endpoint ──────────────────────────────────────────────────────────────

pub struct ApiEndpoint {
    pub method:      &'static str,
    pub path:        &'static str,
    pub description: &'static str,
    pub section:     &'static str,
}

/// Danh sách API endpoints thật của PKTScan (nguồn: pktscan_api.rs + openapi.rs).
pub static API_ENDPOINTS: &[ApiEndpoint] = &[
    // Chain
    ApiEndpoint { method: "GET",  path: "/api/stats",                   description: "Network stats: height, peers, hashrate, mempool",        section: "Chain"    },
    ApiEndpoint { method: "GET",  path: "/api/blocks",                  description: "Latest blocks (limit/offset/cursor pagination)",          section: "Chain"    },
    ApiEndpoint { method: "GET",  path: "/api/block/:height",           description: "Block detail by height",                                  section: "Chain"    },
    ApiEndpoint { method: "GET",  path: "/api/txs",                     description: "Transactions with filter: min/max amount, since/until",   section: "Chain"    },
    ApiEndpoint { method: "GET",  path: "/api/tx/:txid",                description: "Transaction detail + confirmations",                      section: "Chain"    },
    ApiEndpoint { method: "GET",  path: "/api/address/:addr",           description: "Balance + UTXOs + tx history",                            section: "Chain"    },
    ApiEndpoint { method: "GET",  path: "/api/mempool",                 description: "Pending transactions + fee stats",                        section: "Chain"    },
    ApiEndpoint { method: "GET",  path: "/api/search?q=",               description: "Search blocks / txs / addresses",                         section: "Chain"    },
    ApiEndpoint { method: "GET",  path: "/api/analytics/:metric",       description: "Time-series: block_time / hashrate / fee_market / tx_throughput", section: "Chain" },
    // Export
    ApiEndpoint { method: "GET",  path: "/api/blocks.csv",              description: "CSV export of blocks",                                    section: "Export"   },
    ApiEndpoint { method: "GET",  path: "/api/txs.csv",                 description: "CSV export of transactions",                              section: "Export"   },
    // Pool
    ApiEndpoint { method: "GET",  path: "/api/pool/stats",              description: "Mining pool aggregate stats",                             section: "Pool"     },
    ApiEndpoint { method: "GET",  path: "/api/pool/miners",             description: "Per-miner share breakdown",                               section: "Pool"     },
    // Tokens
    ApiEndpoint { method: "GET",  path: "/api/tokens",                  description: "List all tokens",                                         section: "Tokens"   },
    ApiEndpoint { method: "GET",  path: "/api/token/:id",               description: "Token detail",                                            section: "Tokens"   },
    ApiEndpoint { method: "GET",  path: "/api/token/:id/holders",       description: "Token holder list (paginated)",                           section: "Tokens"   },
    ApiEndpoint { method: "GET",  path: "/api/token/:id/balance/:addr", description: "Token balance of address",                                section: "Tokens"   },
    // Contracts
    ApiEndpoint { method: "GET",  path: "/api/contracts",               description: "List deployed contracts",                                 section: "Contracts"},
    ApiEndpoint { method: "GET",  path: "/api/contract/:addr",          description: "Contract detail",                                         section: "Contracts"},
    ApiEndpoint { method: "GET",  path: "/api/contract/:addr/state",    description: "Full contract state",                                     section: "Contracts"},
    ApiEndpoint { method: "GET",  path: "/api/contract/:addr/state/:key", description: "Single state key",                                     section: "Contracts"},
    // Staking
    ApiEndpoint { method: "GET",  path: "/api/staking/stats",           description: "Staking pool aggregate stats",                            section: "Staking"  },
    ApiEndpoint { method: "GET",  path: "/api/staking/validators",      description: "All validators sorted by stake",                          section: "Staking"  },
    ApiEndpoint { method: "GET",  path: "/api/staking/validator/:addr", description: "Validator detail",                                        section: "Staking"  },
    ApiEndpoint { method: "GET",  path: "/api/staking/delegator/:addr", description: "Delegator detail",                                        section: "Staking"  },
    // DeFi
    ApiEndpoint { method: "GET",  path: "/api/defi/feeds",              description: "List oracle price feeds",                                 section: "DeFi"     },
    ApiEndpoint { method: "GET",  path: "/api/defi/feed/:id",           description: "Feed detail",                                             section: "DeFi"     },
    ApiEndpoint { method: "GET",  path: "/api/defi/feed/:id/history",   description: "Feed price history",                                      section: "DeFi"     },
    ApiEndpoint { method: "GET",  path: "/api/defi/loans",              description: "Active loans",                                            section: "DeFi"     },
    // WebSocket
    ApiEndpoint { method: "WS",   path: "/ws",                          description: "Live feed: NewBlock / NewTx / Stats events",              section: "WebSocket"},
    // Meta
    ApiEndpoint { method: "GET",  path: "/api/openapi.json",            description: "OpenAPI 3.0.3 spec (self-describing)",                    section: "Meta"     },
    ApiEndpoint { method: "GET",  path: "/graphql",                     description: "GraphQL endpoint (async-graphql)",                        section: "Meta"     },
    // Write
    ApiEndpoint { method: "POST", path: "/api/tx/submit",               description: "Submit signed transaction",                               section: "Write"    },
    ApiEndpoint { method: "POST", path: "/api/contract/deploy",         description: "Deploy WASM contract",                                    section: "Write"    },
    // Admin
    ApiEndpoint { method: "GET",  path: "/api/admin/audit",             description: "Audit log (requires API key)",                            section: "Admin"    },
    ApiEndpoint { method: "POST", path: "/api/apikey",                  description: "Create / revoke API key",                                 section: "Admin"    },
    // Static assets
    ApiEndpoint { method: "GET",  path: "/",                            description: "PKTScan web frontend (embedded index.html)",              section: "Static"   },
    ApiEndpoint { method: "GET",  path: "/static/app.js",               description: "Frontend JS (embedded)",                                  section: "Static"   },
    ApiEndpoint { method: "GET",  path: "/static/charts.js",            description: "Charts JS (embedded)",                                    section: "Static"   },
    ApiEndpoint { method: "GET",  path: "/static/detail.js",            description: "Block/TX detail JS (embedded)",                           section: "Static"   },
    ApiEndpoint { method: "GET",  path: "/static/address.js",           description: "Address detail JS (embedded)",                            section: "Static"   },
    ApiEndpoint { method: "GET",  path: "/static/live.js",              description: "WebSocket live feed JS (embedded)",                       section: "Static"   },
];

// ── CLI Command ───────────────────────────────────────────────────────────────

pub struct CliCommand {
    pub name:        &'static str,
    pub args:        &'static str,
    pub description: &'static str,
}

/// Danh sách CLI commands thật (nguồn: main.rs dispatch + help text).
pub static CLI_COMMANDS: &[CliCommand] = &[
    CliCommand { name: "mine",        args: "[addr] [n] [peer]",          description: "PoW miner. Đọc địa chỉ từ ~/.pkt/wallet.key nếu không có addr." },
    CliCommand { name: "node",        args: "<port> [peer]",              description: "Chạy P2P node. Kết nối peer tùy chọn." },
    CliCommand { name: "wallet",      args: "new|show|address",           description: "Quản lý ví ECDSA: tạo mới, xem thông tin, lấy địa chỉ." },
    CliCommand { name: "api",         args: "[port]",                     description: "REST API server (mặc định 3000)." },
    CliCommand { name: "pktscan",     args: "[port]",                     description: "PKTScan block explorer + API server (mặc định 8080)." },
    CliCommand { name: "explorer",    args: "chain|block|tx|balance|utxo", description: "CLI block explorer." },
    CliCommand { name: "testnet",     args: "[n] [port]",                 description: "Local testnet: n nodes, base port." },
    CliCommand { name: "genesis",     args: "[regtest|testnet|mainnet]",  description: "Xem network config." },
    CliCommand { name: "metrics",     args: "[node:port]",                description: "Hiển thị hashrate, peers, mempool, block time." },
    CliCommand { name: "monitor",     args: "[port]",                     description: "Health endpoint (mặc định 3001)." },
    CliCommand { name: "bench",       args: "[all|hash|chain|mempool]",   description: "Benchmark các module." },
    CliCommand { name: "blake3",      args: "",                           description: "Benchmark BLAKE3 hasher." },
    CliCommand { name: "hw-info",     args: "",                           description: "Hiển thị hardware config (CPU/GPU)." },
    CliCommand { name: "cpumine",     args: "[addr] [diff] [blocks]",     description: "Multi-thread CPU miner." },
    CliCommand { name: "gpumine",     args: "[addr] [diff] [blocks] [backend]", description: "GPU miner (opencl/cuda/software)." },
    CliCommand { name: "qr",          args: "<address> [amount] [label]", description: "Sinh QR code trong terminal (BIP21 URI)." },
    CliCommand { name: "completions", args: "<bash|zsh|fish>",            description: "Sinh shell completion script." },
    CliCommand { name: "charts",      args: "[metric] [--window N]",      description: "Hiển thị sparkline chart trong terminal." },
    CliCommand { name: "token",       args: "list|info|transfer|mint",    description: "Quản lý token on-chain." },
    CliCommand { name: "contract",    args: "list|call|deploy|state",     description: "Tương tác với smart contract." },
    CliCommand { name: "staking",     args: "info|delegate|undelegate|claim", description: "Staking / unstaking PKT." },
    CliCommand { name: "deploy",      args: "[--network] [--config]",     description: "Deploy config: network params, ports, peers." },
    CliCommand { name: "apikey",      args: "create|list|revoke",         description: "Quản lý API keys." },
    CliCommand { name: "reward",      args: "",                           description: "Hiển thị block reward schedule." },
    CliCommand { name: "devnet",      args: "[--port P] [--blocks N] [--difficulty D]", description: "One-command devnet: node + miner + API cùng process." },
    CliCommand { name: "docs",        args: "[--out DIR]",                description: "Sinh docs/api.md + docs/cli.md + docs/architecture.md." },
];

// ── Module Info ───────────────────────────────────────────────────────────────

pub struct ModuleInfo {
    pub name:        &'static str,
    pub file:        &'static str,
    pub version:     &'static str,
    pub description: &'static str,
    pub era:         &'static str,
}

/// Danh sách modules thật (nguồn: CLAUDE.md file structure + src/).
pub static MODULES: &[ModuleInfo] = &[
    ModuleInfo { name: "block",             file: "src/block.rs",             version: "v1.1",  era: "Era 1",  description: "Block, SHA-256/BLAKE3, Merkle root, PoW mining" },
    ModuleInfo { name: "chain",             file: "src/chain.rs",             version: "v10.5", era: "Era 1",  description: "Blockchain, UTXO, validation, staking rewards" },
    ModuleInfo { name: "transaction",       file: "src/transaction.rs",       version: "v1.1",  era: "Era 1",  description: "TxInput (witness), TxOutput, txid/wtxid" },
    ModuleInfo { name: "utxo",             file: "src/utxo.rs",              version: "v3.0",  era: "Era 1",  description: "UtxoSet, balance lookup, owner_bytes_of()" },
    ModuleInfo { name: "wallet",            file: "src/wallet.rs",            version: "v0.5",  era: "Era 2",  description: "ECDSA keypair, Bitcoin-style address" },
    ModuleInfo { name: "mempool",           file: "src/mempool.rs",           version: "v0.7",  era: "Era 2",  description: "Mempool, fee-rate sort, select_transactions()" },
    ModuleInfo { name: "node",              file: "src/node.rs",              version: "v0.6",  era: "Era 2",  description: "TCP P2P node, peer discovery, sync" },
    ModuleInfo { name: "hd_wallet",         file: "src/hd_wallet.rs",         version: "v9.0",  era: "Era 2",  description: "BIP32/39/44 HD wallet, Ed25519" },
    ModuleInfo { name: "script",            file: "src/script.rs",            version: "v0.9",  era: "Era 3",  description: "Opcode, Script engine, ScriptInterpreter" },
    ModuleInfo { name: "lightning",         file: "src/lightning.rs",         version: "v1.1",  era: "Era 3",  description: "Payment channel, HTLC, CommitmentTx" },
    ModuleInfo { name: "taproot",           file: "src/taproot.rs",           version: "v1.2",  era: "Era 3",  description: "Schnorr sig, MAST, P2TR, MuSig2" },
    ModuleInfo { name: "covenant",          file: "src/covenant.rs",          version: "v1.4",  era: "Era 4",  description: "CTV, Vault, CongestionBatch" },
    ModuleInfo { name: "confidential",      file: "src/confidential.rs",      version: "v1.5",  era: "Era 4",  description: "Pedersen commitment, RangeProof, ECDH" },
    ModuleInfo { name: "coinjoin",          file: "src/coinjoin.rs",          version: "v1.6",  era: "Era 4",  description: "CoinJoin, PayJoin privacy" },
    ModuleInfo { name: "atomic_swap",       file: "src/atomic_swap.rs",       version: "v1.7",  era: "Era 4",  description: "HTLC cross-chain atomic swap" },
    ModuleInfo { name: "zk_proof",          file: "src/zk_proof.rs",          version: "v1.8",  era: "Era 5",  description: "Schnorr ZK, R1CS, Groth16" },
    ModuleInfo { name: "pow_ghost",         file: "src/pow_ghost.rs",         version: "v1.9",  era: "Era 5",  description: "GHOST protocol, uncle blocks" },
    ModuleInfo { name: "bft",              file: "src/bft.rs",               version: "v2.0",  era: "Era 5",  description: "Tendermint BFT, validator set" },
    ModuleInfo { name: "sharding",          file: "src/sharding.rs",          version: "v2.1",  era: "Era 5",  description: "BeaconChain, ShardChain, CrossShardReceipt" },
    ModuleInfo { name: "zk_rollup",         file: "src/zk_rollup.rs",         version: "v2.2",  era: "Era 6",  description: "ZK-Rollup batch, L1 verifier" },
    ModuleInfo { name: "optimistic_rollup", file: "src/optimistic_rollup.rs", version: "v2.3",  era: "Era 6",  description: "Optimistic rollup, fraud proof" },
    ModuleInfo { name: "recursive_zk",      file: "src/recursive_zk.rs",      version: "v2.4",  era: "Era 6",  description: "IVC, recursive proof, aggregation" },
    ModuleInfo { name: "zkevm",            file: "src/zkevm.rs",             version: "v2.5",  era: "Era 6",  description: "zkEVM executor, proof, verifier" },
    ModuleInfo { name: "smart_contract",    file: "src/smart_contract.rs",    version: "v2.6",  era: "Era 7",  description: "WASM runtime, ContractRegistry, GasMeter" },
    ModuleInfo { name: "oracle",            file: "src/oracle.rs",            version: "v2.7",  era: "Era 7",  description: "Oracle feed, registry, lending protocol" },
    ModuleInfo { name: "governance",        file: "src/governance.rs",        version: "v2.8",  era: "Era 7",  description: "Governor, Proposal, TimelockQueue" },
    ModuleInfo { name: "ai_agent",          file: "src/ai_agent.rs",          version: "v2.9",  era: "Era 7",  description: "AgentEngine, AgentRule, safety limits" },
    ModuleInfo { name: "dilithium",         file: "src/dilithium.rs",         version: "v3.0",  era: "Era 8",  description: "CRYSTALS-Dilithium (FIPS 204), Module-LWE" },
    ModuleInfo { name: "sphincs",           file: "src/sphincs.rs",           version: "v3.1",  era: "Era 8",  description: "SPHINCS+ (FIPS 205), WOTS+, XMSS, FORS" },
    ModuleInfo { name: "kyber",            file: "src/kyber.rs",             version: "v3.2",  era: "Era 8",  description: "ML-KEM (FIPS 203), KEM keygen/encap/decap" },
    ModuleInfo { name: "hybrid_sig",        file: "src/hybrid_sig.rs",        version: "v3.3",  era: "Era 8",  description: "ECDSA + Dilithium hybrid, migration phases" },
    ModuleInfo { name: "self_amend",        file: "src/self_amend.rs",        version: "v3.4",  era: "Era 9",  description: "On-chain protocol upgrade vote" },
    ModuleInfo { name: "ibc",              file: "src/ibc.rs",               version: "v3.5",  era: "Era 9",  description: "IBC relayer, channel/connection handshake" },
    ModuleInfo { name: "did",              file: "src/did.rs",               version: "v3.6",  era: "Era 9",  description: "DID, VerifiableCredential, AuthChallenge" },
    ModuleInfo { name: "fhe_contract",      file: "src/fhe_contract.rs",      version: "v3.7",  era: "Era 9",  description: "FHE keygen, EncryptedVoteContract" },
    ModuleInfo { name: "sovereign_rollup",  file: "src/sovereign_rollup.rs",  version: "v3.8",  era: "Era 9",  description: "DaLayer, SovereignRollup, DAS" },
    ModuleInfo { name: "pktscan_api",       file: "src/pktscan_api.rs",       version: "v9.6",  era: "Era 10", description: "REST API server, CORS, cache, Zero-Trust middleware" },
    ModuleInfo { name: "pktscan_ws",        file: "src/pktscan_ws.rs",        version: "v8.1",  era: "Era 10", description: "WebSocket hub, live event broadcast" },
    ModuleInfo { name: "storage",           file: "src/storage.rs",           version: "v4.2",  era: "Era 10", description: "RocksDB persistent storage" },
    ModuleInfo { name: "cpu_miner",         file: "src/cpu_miner.rs",         version: "v6.1",  era: "Era 11", description: "Multi-thread CPU miner (rayon)" },
    ModuleInfo { name: "blake3_hash",       file: "src/blake3_hash.rs",       version: "v6.0",  era: "Era 11", description: "BLAKE3 PoW hash engine" },
    ModuleInfo { name: "pkt_bandwidth",     file: "src/pkt_bandwidth.rs",     version: "v13.2", era: "Era 20", description: "PacketCrypt bandwidth scoring, announcements" },
    ModuleInfo { name: "pkt_address",       file: "src/pkt_address.rs",       version: "v13.3", era: "Era 20", description: "PKT bech32/bech32m address encode/decode" },
    ModuleInfo { name: "pkt_genesis",       file: "src/pkt_genesis.rs",       version: "v13.4", era: "Era 20", description: "PKT coin params, genesis block, halving schedule" },
    ModuleInfo { name: "tui_dashboard",     file: "src/tui_dashboard.rs",     version: "v14.0", era: "Era 21", description: "Terminal UI dashboard (ratatui)" },
    ModuleInfo { name: "tui_wallet",        file: "src/tui_wallet.rs",        version: "v14.1", era: "Era 21", description: "Wallet TUI: balance/send/receive/history tabs" },
    ModuleInfo { name: "web_frontend",      file: "src/web_frontend.rs",      version: "v14.2", era: "Era 21", description: "Embedded web frontend (index.html/app.js/style.css)" },
    ModuleInfo { name: "qr_code",           file: "src/qr_code.rs",           version: "v14.3", era: "Era 21", description: "QR code terminal render, BIP21 URI" },
    ModuleInfo { name: "shell_completions", file: "src/shell_completions.rs", version: "v14.4", era: "Era 21", description: "Bash/Zsh/Fish shell completion scripts" },
    ModuleInfo { name: "web_charts",        file: "src/web_charts.rs",        version: "v14.5", era: "Era 21", description: "Sparkline engine (▁▂▃▄▅▆▇█), Chart.js web charts" },
    ModuleInfo { name: "block_detail",      file: "src/block_detail.rs",      version: "v14.6", era: "Era 21", description: "Block/TX detail page, hash-router JS" },
    ModuleInfo { name: "address_detail",    file: "src/address_detail.rs",    version: "v14.7", era: "Era 21", description: "Address detail page, TxDirection, UTXO list" },
    ModuleInfo { name: "ws_live",           file: "src/ws_live.rs",           version: "v14.8", era: "Era 21", description: "WebSocket live feed types, toast notifications" },
    ModuleInfo { name: "devnet",            file: "src/devnet.rs",            version: "v16.0", era: "Era 23", description: "One-command devnet: node + miner + API" },
    ModuleInfo { name: "docs_gen",          file: "src/docs_gen.rs",          version: "v16.1", era: "Era 23", description: "Dev docs generator: api.md / cli.md / architecture.md" },
];

// ── Generators ────────────────────────────────────────────────────────────────

/// Sinh `api.md` — bảng endpoints nhóm theo section.
pub fn generate_api_md() -> String {
    let mut out = String::new();
    out.push_str("# PKTScan API Reference\n\n");
    out.push_str("> Auto-generated by `cargo run -- docs`. Do not edit manually.\n\n");

    // Collect unique sections (preserve order)
    let mut sections: Vec<&str> = vec![];
    for ep in API_ENDPOINTS {
        if !sections.contains(&ep.section) {
            sections.push(ep.section);
        }
    }

    for section in sections {
        out.push_str(&format!("## {}\n\n", section));
        out.push_str("| Method | Path | Description |\n");
        out.push_str("|--------|------|-------------|\n");
        for ep in API_ENDPOINTS.iter().filter(|e| e.section == section) {
            out.push_str(&format!("| `{}` | `{}` | {} |\n", ep.method, ep.path, ep.description));
        }
        out.push('\n');
    }

    out.push_str(&format!("---\n*{} endpoints total*\n", API_ENDPOINTS.len()));
    out
}

/// Sinh `cli.md` — bảng CLI commands.
pub fn generate_cli_md() -> String {
    let mut out = String::new();
    out.push_str("# PKTScan CLI Reference\n\n");
    out.push_str("> Auto-generated by `cargo run -- docs`. Do not edit manually.\n\n");
    out.push_str("## Usage\n\n```\ncargo run -- <command> [args]\n```\n\n");
    out.push_str("## Commands\n\n");
    out.push_str("| Command | Args | Description |\n");
    out.push_str("|---------|------|-------------|\n");
    for cmd in CLI_COMMANDS {
        let args = if cmd.args.is_empty() { "—".to_string() } else { format!("`{}`", cmd.args) };
        out.push_str(&format!("| `{}` | {} | {} |\n", cmd.name, args, cmd.description));
    }
    out.push('\n');
    out.push_str(&format!("---\n*{} commands total*\n", CLI_COMMANDS.len()));
    out
}

/// Sinh `architecture.md` — module list nhóm theo era.
pub fn generate_arch_md() -> String {
    let mut out = String::new();
    out.push_str("# PKTScan Architecture\n\n");
    out.push_str("> Auto-generated by `cargo run -- docs`. Do not edit manually.\n\n");
    out.push_str("## Overview\n\n");
    out.push_str("Blockchain từ Bitcoin 0.1 → 2030, mỗi version build trên nền version trước.\n\n");
    out.push_str("```\nsrc/          ← Rust modules\nfrontend/     ← Embedded JS/CSS assets\ndocs/         ← Generated docs (this file)\n```\n\n");

    // Collect unique eras (preserve order)
    let mut eras: Vec<&str> = vec![];
    for m in MODULES {
        if !eras.contains(&m.era) {
            eras.push(m.era);
        }
    }

    out.push_str("## Modules by Era\n\n");
    for era in eras {
        out.push_str(&format!("### {}\n\n", era));
        out.push_str("| Module | File | Version | Description |\n");
        out.push_str("|--------|------|---------|-------------|\n");
        for m in MODULES.iter().filter(|m| m.era == era) {
            out.push_str(&format!("| `{}` | `{}` | {} | {} |\n",
                m.name, m.file, m.version, m.description));
        }
        out.push('\n');
    }

    out.push_str(&format!("---\n*{} modules total*\n", MODULES.len()));
    out
}

// ── File writer ───────────────────────────────────────────────────────────────

/// Ghi 3 file vào `out_dir`.  Trả về danh sách path đã ghi.
pub fn write_docs(out_dir: &str) -> Result<Vec<PathBuf>, String> {
    let dir = Path::new(out_dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {}", out_dir, e))?;

    let files: &[(&str, fn() -> String)] = &[
        ("api.md",          generate_api_md),
        ("cli.md",          generate_cli_md),
        ("architecture.md", generate_arch_md),
    ];

    let mut written = Vec::new();
    for (name, gen) in files {
        let path    = dir.join(name);
        let content = gen();
        std::fs::write(&path, &content)
            .map_err(|e| format!("write {}: {}", path.display(), e))?;
        written.push(path);
    }

    Ok(written)
}

// ── CLI ───────────────────────────────────────────────────────────────────────

/// `cargo run -- docs [--out DIR]`
pub fn cmd_docs(args: &[String]) {
    let out_dir = args.windows(2)
        .find(|w| w[0] == "--out" || w[0] == "-o")
        .map(|w| w[1].as_str())
        .unwrap_or("docs");

    println!();
    println!("  PKT Docs Generator  v16.1");
    println!("  Output: {}/", out_dir);
    println!();

    match write_docs(out_dir) {
        Ok(paths) => {
            for p in &paths {
                let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                println!("  ✓  {}  ({} bytes)", p.display(), size);
            }
            println!();
            println!("  {} files written.", paths.len());
        }
        Err(e) => eprintln!("  ✗  Error: {}", e),
    }
    println!();
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// Tests dùng data thật: static lists thật của hệ thống + write_docs() thật.
// Không hardcode kết quả — assert dựa trên invariant của hệ thống.

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── API_ENDPOINTS (real data) ─────────────────────────────────────────────

    #[test]
    fn api_endpoints_non_empty() {
        assert!(!API_ENDPOINTS.is_empty());
    }

    #[test]
    fn api_endpoints_has_stats() {
        assert!(API_ENDPOINTS.iter().any(|e| e.path == "/api/stats"),
            "must have /api/stats endpoint");
    }

    #[test]
    fn api_endpoints_has_blocks() {
        assert!(API_ENDPOINTS.iter().any(|e| e.path == "/api/blocks"));
    }

    #[test]
    fn api_endpoints_has_websocket() {
        assert!(API_ENDPOINTS.iter().any(|e| e.path == "/ws" && e.method == "WS"));
    }

    #[test]
    fn api_endpoints_has_openapi() {
        assert!(API_ENDPOINTS.iter().any(|e| e.path == "/api/openapi.json"));
    }

    #[test]
    fn api_endpoints_all_have_description() {
        for ep in API_ENDPOINTS {
            assert!(!ep.description.is_empty(), "endpoint {} has empty description", ep.path);
        }
    }

    #[test]
    fn api_endpoints_all_have_section() {
        for ep in API_ENDPOINTS {
            assert!(!ep.section.is_empty(), "endpoint {} has empty section", ep.path);
        }
    }

    #[test]
    fn api_endpoints_no_duplicate_paths() {
        let mut seen: Vec<(&str, &str)> = vec![];
        for ep in API_ENDPOINTS {
            let key = (ep.method, ep.path);
            assert!(!seen.contains(&key), "duplicate endpoint: {} {}", ep.method, ep.path);
            seen.push(key);
        }
    }

    // ── CLI_COMMANDS (real data) ──────────────────────────────────────────────

    #[test]
    fn cli_commands_non_empty() {
        assert!(!CLI_COMMANDS.is_empty());
    }

    #[test]
    fn cli_commands_has_devnet() {
        assert!(CLI_COMMANDS.iter().any(|c| c.name == "devnet"),
            "CLI must list devnet command");
    }

    #[test]
    fn cli_commands_has_docs() {
        assert!(CLI_COMMANDS.iter().any(|c| c.name == "docs"));
    }

    #[test]
    fn cli_commands_has_wallet() {
        assert!(CLI_COMMANDS.iter().any(|c| c.name == "wallet"));
    }

    #[test]
    fn cli_commands_all_have_description() {
        for cmd in CLI_COMMANDS {
            assert!(!cmd.description.is_empty(), "command '{}' has empty description", cmd.name);
        }
    }

    #[test]
    fn cli_commands_no_duplicate_names() {
        let mut seen: Vec<&str> = vec![];
        for cmd in CLI_COMMANDS {
            assert!(!seen.contains(&cmd.name), "duplicate command: {}", cmd.name);
            seen.push(cmd.name);
        }
    }

    // ── MODULES (real data) ───────────────────────────────────────────────────

    #[test]
    fn modules_non_empty() {
        assert!(!MODULES.is_empty());
    }

    #[test]
    fn modules_has_chain() {
        assert!(MODULES.iter().any(|m| m.name == "chain"));
    }

    #[test]
    fn modules_has_devnet() {
        assert!(MODULES.iter().any(|m| m.name == "devnet"));
    }

    #[test]
    fn modules_has_docs_gen() {
        assert!(MODULES.iter().any(|m| m.name == "docs_gen"));
    }

    #[test]
    fn modules_all_files_start_with_src() {
        for m in MODULES {
            assert!(m.file.starts_with("src/"), "module {} file '{}' should start with src/", m.name, m.file);
        }
    }

    #[test]
    fn modules_all_have_era() {
        for m in MODULES {
            assert!(!m.era.is_empty(), "module {} has empty era", m.name);
        }
    }

    #[test]
    fn modules_no_duplicate_names() {
        let mut seen: Vec<&str> = vec![];
        for m in MODULES {
            assert!(!seen.contains(&m.name), "duplicate module: {}", m.name);
            seen.push(m.name);
        }
    }

    // ── generate_api_md ───────────────────────────────────────────────────────

    #[test]
    fn api_md_has_title() {
        let md = generate_api_md();
        assert!(md.starts_with("# PKTScan API Reference"));
    }

    #[test]
    fn api_md_contains_stats_endpoint() {
        let md = generate_api_md();
        assert!(md.contains("/api/stats"), "api.md must reference /api/stats");
    }

    #[test]
    fn api_md_contains_websocket_section() {
        let md = generate_api_md();
        assert!(md.contains("WebSocket"), "api.md must have WebSocket section");
    }

    #[test]
    fn api_md_has_markdown_table() {
        let md = generate_api_md();
        assert!(md.contains("| Method | Path |"), "api.md must have markdown table header");
    }

    #[test]
    fn api_md_has_endpoint_count() {
        let md = generate_api_md();
        let expected = format!("{} endpoints total", API_ENDPOINTS.len());
        assert!(md.contains(&expected), "api.md should state endpoint count");
    }

    #[test]
    fn api_md_contains_live_js() {
        let md = generate_api_md();
        assert!(md.contains("/static/live.js"), "api.md must reference live.js static asset");
    }

    // ── generate_cli_md ───────────────────────────────────────────────────────

    #[test]
    fn cli_md_has_title() {
        let md = generate_cli_md();
        assert!(md.starts_with("# PKTScan CLI Reference"));
    }

    #[test]
    fn cli_md_contains_devnet_command() {
        let md = generate_cli_md();
        assert!(md.contains("devnet"), "cli.md must reference devnet command");
    }

    #[test]
    fn cli_md_contains_docs_command() {
        let md = generate_cli_md();
        assert!(md.contains("docs"), "cli.md must reference docs command");
    }

    #[test]
    fn cli_md_has_markdown_table() {
        let md = generate_cli_md();
        assert!(md.contains("| Command | Args |"));
    }

    #[test]
    fn cli_md_has_command_count() {
        let md = generate_cli_md();
        let expected = format!("{} commands total", CLI_COMMANDS.len());
        assert!(md.contains(&expected));
    }

    // ── generate_arch_md ──────────────────────────────────────────────────────

    #[test]
    fn arch_md_has_title() {
        let md = generate_arch_md();
        assert!(md.starts_with("# PKTScan Architecture"));
    }

    #[test]
    fn arch_md_contains_chain_module() {
        let md = generate_arch_md();
        assert!(md.contains("chain.rs"), "architecture.md must reference chain.rs");
    }

    #[test]
    fn arch_md_contains_era_sections() {
        let md = generate_arch_md();
        assert!(md.contains("### Era 1"));
        assert!(md.contains("### Era 23"));
    }

    #[test]
    fn arch_md_has_module_count() {
        let md = generate_arch_md();
        let expected = format!("{} modules total", MODULES.len());
        assert!(md.contains(&expected));
    }

    #[test]
    fn arch_md_contains_docs_gen_module() {
        let md = generate_arch_md();
        assert!(md.contains("docs_gen"), "architecture.md must list docs_gen itself");
    }

    // ── write_docs (real file I/O) ────────────────────────────────────────────

    #[test]
    fn write_docs_creates_three_files() {
        let dir = format!("/tmp/pkt_docs_test_{}", std::process::id());
        let paths = write_docs(&dir).expect("write_docs should succeed");
        assert_eq!(paths.len(), 3, "should write exactly 3 files");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_docs_api_md_exists_and_non_empty() {
        let dir = format!("/tmp/pkt_docs_api_{}", std::process::id());
        let paths = write_docs(&dir).expect("write_docs failed");
        let api_path = paths.iter().find(|p| p.ends_with("api.md")).expect("api.md missing");
        let content = fs::read_to_string(api_path).expect("read api.md failed");
        assert!(!content.is_empty());
        assert!(content.contains("/api/stats"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_docs_cli_md_exists_and_non_empty() {
        let dir = format!("/tmp/pkt_docs_cli_{}", std::process::id());
        let paths = write_docs(&dir).expect("write_docs failed");
        let cli_path = paths.iter().find(|p| p.ends_with("cli.md")).expect("cli.md missing");
        let content = fs::read_to_string(cli_path).expect("read cli.md failed");
        assert!(!content.is_empty());
        assert!(content.contains("devnet"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_docs_arch_md_exists_and_non_empty() {
        let dir = format!("/tmp/pkt_docs_arch_{}", std::process::id());
        let paths = write_docs(&dir).expect("write_docs failed");
        let arch_path = paths.iter().find(|p| p.ends_with("architecture.md")).expect("architecture.md missing");
        let content = fs::read_to_string(arch_path).expect("read architecture.md failed");
        assert!(!content.is_empty());
        assert!(content.contains("chain.rs"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_docs_files_are_valid_markdown() {
        let dir = format!("/tmp/pkt_docs_md_{}", std::process::id());
        let paths = write_docs(&dir).expect("write_docs failed");
        for p in &paths {
            let content = fs::read_to_string(p).unwrap();
            // Markdown file must start with # heading
            assert!(content.trim_start().starts_with('#'),
                "{} should start with a markdown heading", p.display());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_docs_idempotent() {
        // Calling twice should succeed (overwrite)
        let dir = format!("/tmp/pkt_docs_idem_{}", std::process::id());
        write_docs(&dir).expect("first write failed");
        write_docs(&dir).expect("second write (overwrite) failed");
        let _ = fs::remove_dir_all(&dir);
    }
}
