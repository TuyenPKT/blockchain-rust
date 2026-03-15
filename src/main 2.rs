mod block;
mod chain;
mod transaction;
mod utxo;
mod wallet;
mod message;
mod node;
mod mempool;
mod hd_wallet;
mod script;
mod lightning;
mod taproot;
mod covenant;
mod confidential;
mod coinjoin;
mod atomic_swap;
mod zk_proof;
mod pow_ghost;
mod bft;
mod sharding;

use wallet::Wallet;
use script::Script;
use zk_proof::{
    SchnorrZkProof, HashPreimageProof,
    R1csCircuit, groth16_setup, groth16_prove, groth16_verify,
    ZkSimulator,
};

// ── Entry point ───────────────────────────────────────────────
//
// Usage:
//   cargo run                              → demo v1.6 CoinJoin
//   cargo run -- node 8333                 → chạy node P2P port 8333
//   cargo run -- node 8334 180.93.1.235:8333   → chạy node + kết nối peer

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("node") {
        let port: u16 = args.get(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(8333);
        let peer = args.get(3).cloned();
        run_node(port, peer);
        return;
    }
    demo_v21();
}

fn run_node(port: u16, peer: Option<String>) {
    use std::sync::Arc;
    use node::Node;
    use message::Message;

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let n = Arc::new(Node::new(port));

        // Spawn listener
        let n2 = Arc::clone(&n);
        tokio::spawn(async move { n2.start().await });

        // Kết nối đến peer nếu có
        if let Some(peer_addr) = peer {
            println!("🔗 Kết nối đến peer: {}", peer_addr);
            // Gửi Hello với host và port của mình
            let my_host = local_ip();
            let hello = Message::Hello { version: 1, host: my_host, port };
            if let Some(resp) = Node::send_to_peer(&peer_addr, &hello).await {
                println!("  Response: {:?}", resp);
            }
            // Đồng bộ chain
            n.sync_from(&peer_addr).await;
            // Thêm peer vào danh sách
            n.peers.lock().await.push(peer_addr);
        }

        println!("⛓  Node khởi động tại port {}. Nhấn Ctrl+C để dừng.", port);
        println!("  Commands trong terminal khác:");
        println!("    curl http://localhost:{} (nếu thêm REST sau)", port);

        // Chạy mãi
        loop { tokio::time::sleep(tokio::time::Duration::from_secs(10)).await; }
    });
}

/// Lấy IP local (không phải loopback) để gửi cho peer
fn local_ip() -> String {
    // Mở UDP socket giả để lấy local IP routing đến internet
    use std::net::UdpSocket;
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| { s.connect("8.8.8.8:80")?; s.local_addr() })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn demo_v21() {
    use sharding::{BeaconChain, ShardTx, shard_of, NUM_SHARDS, display_assignment};

    println!("=== Blockchain Rust v2.1 — Sharding ===\n");

    // ════════════════════════════════════════════════════════
    // DEMO 1: Shard assignment — địa chỉ thuộc shard nào
    // ════════════════════════════════════════════════════════
    println!("═══ Demo 1: Shard Assignment — Địa Chỉ → Shard ═══\n");

    println!("  Network: {} shards\n", NUM_SHARDS);
    let addresses = ["alice", "bob", "carol", "dave", "eve", "frank"];
    for addr in &addresses {
        println!("  {} → shard {}", addr, shard_of(addr));
    }
    println!("\n  → Địa chỉ được hash → mod {} để xác định shard ✅", NUM_SHARDS);

    // ════════════════════════════════════════════════════════
    // DEMO 2: Validator assignment theo epoch
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 2: Validator Assignment — Epoch Rotation ═══\n");

    let mut beacon = BeaconChain::new();
    let validators = ["alice", "bob", "carol", "dave", "eve", "frank", "grace", "heidi"];

    for epoch in 0..2u64 {
        beacon.assign_validators(&validators, epoch);
        println!("  Epoch {}:", epoch);
        display_assignment(&beacon.validator_assignment);
        println!("  → Validators xoay vòng mỗi epoch để tránh collusion ✅\n");
    }

    // ════════════════════════════════════════════════════════
    // DEMO 3: Intra-shard TX (cùng shard)
    // ════════════════════════════════════════════════════════
    println!("═══ Demo 3: Intra-Shard TX (Cùng Shard) ═══\n");

    // Fund accounts
    let alice_shard = shard_of("alice");
    let bob_shard   = shard_of("bob");
    beacon.shards.get_mut(&alice_shard).unwrap().fund("alice", 1_000);
    beacon.shards.get_mut(&bob_shard).unwrap().fund("bob", 500);

    // Tìm 2 địa chỉ cùng shard
    let (same_from, same_to) = find_same_shard_pair();
    let same_shard = shard_of(same_from);

    beacon.shards.get_mut(&same_shard).unwrap().fund(same_from, 1_000);
    beacon.shards.get_mut(&same_shard).unwrap().fund(same_to, 200);

    let before_from = beacon.shards[&same_shard].balance_of(same_from);
    let before_to   = beacon.shards[&same_shard].balance_of(same_to);

    println!("  {} (shard {}) balance: {}", same_from, same_shard, before_from);
    println!("  {} (shard {}) balance: {}", same_to, same_shard, before_to);

    let tx = ShardTx::new(same_from, same_to, 300);
    println!("\n  TX: {} → {} gửi 300 (intra-shard {})", same_from, same_to, same_shard);

    beacon.shards.get_mut(&same_shard).unwrap()
        .produce_block("proposer_a", vec![tx], vec![])
        .expect("block ok");

    println!("  {} sau TX: {}", same_from, beacon.shards[&same_shard].balance_of(same_from));
    println!("  {} sau TX: {}", same_to,   beacon.shards[&same_shard].balance_of(same_to));
    println!("  → Intra-shard TX xử lý trong 1 block, không cần cross-shard ✅");

    // ════════════════════════════════════════════════════════
    // DEMO 4: Cross-shard TX — Alice (shard X) gửi cho Carol (shard Y)
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 4: Cross-Shard TX — Alice → Carol ═══\n");

    // Đảm bảo alice và carol ở shard khác nhau
    let a_shard = shard_of("alice");
    let c_shard = shard_of("carol");

    beacon.shards.get_mut(&a_shard).unwrap().fund("alice", 2_000);
    beacon.shards.get_mut(&c_shard).unwrap().fund("carol", 100);

    println!("  alice shard: {}", a_shard);
    println!("  carol shard: {}", c_shard);
    println!("  alice balance (shard {}): {}", a_shard, beacon.shards[&a_shard].balance_of("alice"));
    println!("  carol balance (shard {}): {}", c_shard, beacon.shards[&c_shard].balance_of("carol"));

    let cross_tx = ShardTx::new("alice", "carol", 500);
    println!("\n  [Step 1] alice gửi 500 → carol (cross-shard {} → {})", a_shard, c_shard);
    println!("    TX hash: {}...", &cross_tx.tx_hash[..12]);
    println!("    cross_shard: {}", cross_tx.is_cross_shard());

    // Shard A debit alice, emit receipt
    beacon.shards.get_mut(&a_shard).unwrap()
        .produce_block("proposer_a", vec![cross_tx], vec![])
        .expect("shard A block ok");

    let receipt_count = beacon.shards[&a_shard].pending_receipts_out.len();
    println!("\n  [Step 2] Shard {} debit alice, emit {} receipt(s)", a_shard, receipt_count);
    println!("    alice balance sau debit: {}", beacon.shards[&a_shard].balance_of("alice"));
    println!("    carol balance (chưa nhận): {}", beacon.shards[&c_shard].balance_of("carol"));

    // Beacon collect receipt
    let beacon_hash = beacon.produce_beacon_block("beacon_proposer");
    let relayed = beacon.blocks.last().unwrap().receipts.len();
    println!("\n  [Step 3] Beacon include {} receipt(s), hash={}...", relayed, &beacon_hash[..12]);

    // Shard B consume receipt → credit carol
    beacon.deliver_receipts("proposer_b");
    println!("\n  [Step 4] Shard {} consume receipt → credit carol", c_shard);
    println!("    carol balance sau nhận: {}", beacon.shards[&c_shard].balance_of("carol"));
    println!("  → Cross-shard TX hoàn thành qua 4 bước ✅");

    // ════════════════════════════════════════════════════════
    // DEMO 5: Beacon chain — finalize nhiều shard headers
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 5: Beacon Chain — Finalize Shard Headers ═══\n");

    // Produce thêm shard blocks để beacon finalize
    for shard_id in 0..NUM_SHARDS {
        let proposer = format!("proposer_{}", shard_id);
        let sc = beacon.shards.get_mut(&shard_id).unwrap();
        let _ = sc.produce_block(&proposer, vec![], vec![]);
    }

    let bh = beacon.produce_beacon_block("beacon_proposer_2");
    let bb = beacon.blocks.last().unwrap();
    println!("  Beacon block #{}: hash={}...", bb.height, &bh[..12]);
    println!("  Shard headers finalized:");
    let mut ids: Vec<_> = bb.shard_headers.keys().copied().collect();
    ids.sort();
    for id in ids {
        println!("    Shard {}: {}...", id, &bb.shard_headers[&id][..12]);
    }
    println!("  → Beacon xác nhận trạng thái tất cả {} shards trong 1 block ✅", NUM_SHARDS);

    // ════════════════════════════════════════════════════════
    // DEMO 6: Sharding vs Non-Sharding — So sánh throughput
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 6: Sharding vs Non-Sharding — So Sánh ═══");
    println!("  ┌─────────────────────┬──────────────────────┬──────────────────────┐");
    println!("  │ Thuộc tính          │ Non-Sharded          │ Sharded (v2.1)       │");
    println!("  ├─────────────────────┼──────────────────────┼──────────────────────┤");
    println!("  │ TPS                 │ ~15 TPS              │ ~15 × {} = {} TPS  │", NUM_SHARDS, 15 * NUM_SHARDS);
    println!("  │ State size/node     │ Full state           │ 1/{} full state     │", NUM_SHARDS);
    println!("  │ Parallel processing │ ❌ Sequential        │ ✅ {} shards song song│", NUM_SHARDS);
    println!("  │ Cross-shard TX      │ N/A (1 chain)        │ 4-step receipt relay │");
    println!("  │ Validator load      │ Tất cả TXs           │ 1/{} TXs/validator  │", NUM_SHARDS);
    println!("  │ Beacon chain        │ Không cần            │ Coordinate shards    │");
    println!("  │ Complexity          │ Đơn giản             │ Cao hơn              │");
    println!("  │ Ví dụ               │ Bitcoin, Ethereum 1  │ Ethereum 2.0 Phase 1 │");
    println!("  └─────────────────────┴──────────────────────┴──────────────────────┘");

    println!("\n═══ Sharding Summary ═══");
    println!("  ✅ {} shards chạy song song — throughput tuyến tính", NUM_SHARDS);
    println!("  ✅ Shard assignment: địa chỉ → H(addr) mod N");
    println!("  ✅ Validator rotation: xoay vòng mỗi epoch chống collusion");
    println!("  ✅ Intra-shard TX: xử lý trong 1 block không cần relay");
    println!("  ✅ Cross-shard TX: debit → receipt → beacon relay → credit");
    println!("  ✅ Beacon chain: finalize shard headers, relay receipts");
    println!("  ✅ State partitioning: mỗi node chỉ lưu 1/{} state", NUM_SHARDS);
}

/// Tìm 2 địa chỉ cùng shard để demo intra-shard TX
fn find_same_shard_pair() -> (&'static str, &'static str) {
    use sharding::shard_of;
    let candidates = ["alice", "bob", "carol", "dave", "eve", "frank", "grace", "heidi",
                      "ivan", "judy", "kevin", "linda", "mike", "nancy", "oscar", "peggy"];
    for i in 0..candidates.len() {
        for j in (i+1)..candidates.len() {
            if shard_of(candidates[i]) == shard_of(candidates[j]) {
                return (candidates[i], candidates[j]);
            }
        }
    }
    ("alice", "bob") // fallback
}

#[allow(dead_code)]
fn demo_v20() {
    use bft::{BftChain, BftValidatorSet, ValidatorInfo, TendermintEngine, ConsensusResult, verify_safety};

    println!("=== Blockchain Rust v2.0 — BFT Consensus (Tendermint-style) ===\n");

    // ════════════════════════════════════════════════════════
    // DEMO 1: Happy path — 4 validators, đủ quorum
    // ════════════════════════════════════════════════════════
    println!("═══ Demo 1: Happy Path — 4 Validators, Đủ Quorum ═══\n");

    let mut vset = BftValidatorSet::new();
    vset.add(ValidatorInfo::new("alice",  100));
    vset.add(ValidatorInfo::new("bob",    100));
    vset.add(ValidatorInfo::new("carol",  100));
    vset.add(ValidatorInfo::new("dave",   100));

    println!("  Validators: alice, bob, carol, dave (power=100 each)");
    println!("  Total power: {}", vset.total_power());
    println!("  Quorum threshold: {}/3 × {} = {}\n",
        2, vset.total_power(), vset.total_power() * 2 / 3 + 1);

    let engine = TendermintEngine::new(&vset, 1, "genesis_hash", "Alice→Bob 10 ATOM");
    let results = engine.run(3);

    for r in &results {
        match r {
            ConsensusResult::Committed { block, round, precommit_count } => {
                println!("  [Round {}] PROPOSE → PREVOTE → PRECOMMIT → COMMIT ✅", round);
                println!("    Proposer:   {}", block.proposer);
                println!("    Block hash: {}...", &block.hash[..12]);
                println!("    Precommits: {}/{} validators", precommit_count, vset.active_honest().len());
                println!("    Finalized:  INSTANT (không cần xác nhận thêm)");
            }
            ConsensusResult::RoundTimeout { round, reason, .. } => {
                println!("  [Round {}] TIMEOUT: {}", round, reason);
            }
        }
    }

    // ════════════════════════════════════════════════════════
    // DEMO 2: Faulty proposer → timeout → round +1 → commit
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 2: Faulty Proposer → Round Rotation ═══\n");

    let mut vset2 = BftValidatorSet::new();
    vset2.add(ValidatorInfo::new("alice",  100));
    vset2.add(ValidatorInfo::new("bob",    100));
    vset2.add(ValidatorInfo::new("carol",  100));
    vset2.add(ValidatorInfo::new("dave",   100));

    // Alice là proposer round 0 nhưng bị faulty
    vset2.validators[0].is_faulty = true;
    println!("  Alice (round-0 proposer) bị faulty — không broadcast proposal");

    let engine2 = TendermintEngine::new(&vset2, 1, "genesis_hash", "Carol→Dave 5 ATOM");
    let results2 = engine2.run(3);

    for r in &results2 {
        match r {
            ConsensusResult::RoundTimeout { round, reason, .. } => {
                println!("  [Round {}] TIMEOUT → {}", round, reason);
                println!("            Chuyển sang round {} với proposer mới...", round + 1);
            }
            ConsensusResult::Committed { block, round, precommit_count } => {
                println!("  [Round {}] COMMIT ✅  proposer={}, precommits={}",
                    round, block.proposer, precommit_count);
                println!("    → Liveness đảm bảo dù proposer faulty ✅");
            }
        }
    }

    // ════════════════════════════════════════════════════════
    // DEMO 3: BftChain — mine nhiều blocks liên tiếp
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 3: BftChain — 5 Blocks Liên Tiếp ═══\n");

    let mut chain = BftChain::new();
    chain.validator_set.add(ValidatorInfo::new("alice",  200));
    chain.validator_set.add(ValidatorInfo::new("bob",    150));
    chain.validator_set.add(ValidatorInfo::new("carol",  100));
    chain.validator_set.add(ValidatorInfo::new("dave",    50));

    println!("  Validators: alice(200), bob(150), carol(100), dave(50)");
    println!("  Total power: {}\n", chain.validator_set.total_power());

    let payloads = [
        "Alice→Bob 5 ATOM",
        "Carol→Dave 2 ATOM",
        "Bob→Alice 1 ATOM",
        "Dave→Carol 3 ATOM",
        "Alice→Carol 4 ATOM",
    ];

    for payload in &payloads {
        let ok = chain.commit_next(payload, 3);
        let b  = chain.blocks.last().unwrap();
        println!("  Block #{}: proposer={}, round={}, hash={}...",
            b.height, b.proposer, b.round, &b.hash[..12]);
        println!("    payload:    \"{}\"", payload);
        println!("    commits:    {}/{} validators",
            b.commit_votes.len(),
            chain.validator_set.active_honest().len());
        println!("    finalized:  {}\n", if ok { "✅ INSTANT" } else { "❌ FAILED" });
    }

    println!("  Chain height: {}", chain.height());
    println!("  Safety check: {}", if verify_safety(&chain) { "✅ PASSED" } else { "❌ FAILED" });

    // ════════════════════════════════════════════════════════
    // DEMO 4: Byzantine fault tolerance — f < n/3
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 4: Byzantine Fault Tolerance — f < n/3 ═══\n");

    // 4 validators, 1 faulty (1 < 4/3 = 1.33) → vẫn an toàn
    let mut vset3 = BftValidatorSet::new();
    vset3.add(ValidatorInfo::new("alice", 100));
    vset3.add(ValidatorInfo::new("bob",   100));
    vset3.add(ValidatorInfo::new("carol", 100));
    vset3.add(ValidatorInfo::new("dave",  100));  // dave faulty
    vset3.validators[3].is_faulty = true;

    let honest_power: u64 = vset3.active_honest().iter().map(|v| v.power).sum();
    println!("  4 validators, 1 faulty (dave)");
    println!("  Honest power: {}/{} = {:.0}%",
        honest_power, vset3.total_power(),
        honest_power as f64 / vset3.total_power() as f64 * 100.0);
    println!("  BFT condition: f={} < n/3={:.1} ✅", 1, 4.0/3.0);

    let engine3 = TendermintEngine::new(&vset3, 1, "prev", "TX with 1 faulty node");
    let results3 = engine3.run(4);
    for r in &results3 {
        if let ConsensusResult::Committed { block, round, precommit_count } = r {
            println!("  Committed at round {}: {}/{} honest validators ✅",
                round, precommit_count, vset3.active_honest().len());
            let _ = block;
        }
    }

    // 4 validators, 2 faulty → f=2 >= n/3=1.33 → không thể commit
    let mut vset4 = BftValidatorSet::new();
    vset4.add(ValidatorInfo::new("alice", 100));
    vset4.add(ValidatorInfo::new("bob",   100));
    vset4.add(ValidatorInfo::new("carol", 100));  // faulty
    vset4.add(ValidatorInfo::new("dave",  100));  // faulty
    vset4.validators[2].is_faulty = true;
    vset4.validators[3].is_faulty = true;

    let honest_power2: u64 = vset4.active_honest().iter().map(|v| v.power).sum();
    println!("\n  4 validators, 2 faulty (carol + dave)");
    println!("  Honest power: {}/{} = {:.0}%",
        honest_power2, vset4.total_power(),
        honest_power2 as f64 / vset4.total_power() as f64 * 100.0);
    println!("  BFT condition: f=2 >= n/3=1.33 → KHÔNG đủ quorum ⚠️");

    let engine4 = TendermintEngine::new(&vset4, 1, "prev", "TX with 2 faulty nodes");
    let results4 = engine4.run(3);
    let any_commit = results4.iter().any(|r| matches!(r, ConsensusResult::Committed { .. }));
    println!("  Kết quả: {}", if !any_commit { "Không commit được ✅ (safety preserved)" }
                              else           { "❌ BFT bị vi phạm" });

    // ════════════════════════════════════════════════════════
    // DEMO 5: Vote verification
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 5: Vote Verification ═══\n");

    use bft::Vote;
    use bft::VoteType;

    let vote = Vote::new(VoteType::Prevote, 1, 0, Some("deadbeef1234".to_string()), "alice");
    println!("  Alice prevote: height=1, round=0, block=deadbeef1234...");
    println!("  Signature:     {}...", hex::encode(&vote.signature[..6]));
    println!("  Verify valid:  {}", if vote.verify() { "✅" } else { "❌" });

    // Giả mạo validator khác ký
    let mut fake_vote = vote.clone();
    fake_vote.validator = "mallory".to_string();
    println!("  Fake (swap validator): {}", if !fake_vote.verify() { "✅ bị từ chối" } else { "❌" });

    // Giả mạo block hash
    let mut tampered = vote.clone();
    tampered.block_hash = Some("000000000000".to_string());
    println!("  Tampered (swap hash):  {}", if !tampered.verify() { "✅ bị từ chối" } else { "❌" });

    // ════════════════════════════════════════════════════════
    // DEMO 6: BFT vs PoW vs PoS — So sánh
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 6: BFT vs PoW vs PoS — So Sánh ═══");
    println!("  ┌──────────────────┬───────────────────┬───────────────────┬───────────────────┐");
    println!("  │ Thuộc tính       │ PoW (v0.3/v1.9)   │ PoS (Ethereum)    │ BFT (v2.0)        │");
    println!("  ├──────────────────┼───────────────────┼───────────────────┼───────────────────┤");
    println!("  │ Finality         │ Probabilistic     │ Checkpoint (2/3)  │ Instant ✅        │");
    println!("  │ Fault tolerance  │ 51% hash power    │ 33% stake         │ 33% validators    │");
    println!("  │ Block production │ Mining race       │ Stake-weighted    │ Round-robin       │");
    println!("  │ Fork possibility │ ✅ Có             │ Hiếm              │ ❌ Không          │");
    println!("  │ Liveness         │ Luôn tiến         │ Luôn tiến         │ f < n/3           │");
    println!("  │ Throughput       │ Thấp (~7 TPS)     │ Trung bình        │ Cao (1000+ TPS)   │");
    println!("  │ Network req.     │ Asynchronous      │ Partial sync      │ Synchronous       │");
    println!("  │ Ví dụ            │ Bitcoin, Ethereum │ Ethereum 2.0      │ Cosmos, Tendermint│");
    println!("  └──────────────────┴───────────────────┴───────────────────┴───────────────────┘");

    println!("\n═══ BFT Consensus Summary ═══");
    println!("  ✅ 4 phases: Propose → Prevote → Precommit → Commit");
    println!("  ✅ Instant finality: block committed = irreversible");
    println!("  ✅ Safety: không bao giờ commit 2 block khác nhau cùng height");
    println!("  ✅ Liveness: round rotation khi proposer faulty");
    println!("  ✅ BFT threshold: tolerates f < n/3 faulty validators");
    println!("  ✅ Vote signature verification: giả mạo bị phát hiện");
    println!("  ✅ Weighted voting: voting power tỉ lệ stake");
}

#[allow(dead_code)]
fn demo_v19() {
    use pow_ghost::{GhostChain, UncleBlock, adjust_difficulty, BASE_REWARD};

    println!("=== Blockchain Rust v1.9 — Advanced PoW: GHOST Protocol + Uncle Blocks ===\n");

    // ════════════════════════════════════════════════════════
    // DEMO 1: Mine chain cơ bản (không có uncles)
    // ════════════════════════════════════════════════════════
    println!("═══ Demo 1: Mining Chain (difficulty=2) ═══\n");

    let mut chain = GhostChain::new(2);
    println!("  Genesis: hash={}...", &chain.tip().hash[..12]);

    let miners = ["alice", "bob", "carol", "alice", "bob"];
    for (i, miner) in miners.iter().enumerate() {
        let payload = format!("TX block {}", i + 1);
        let hash = chain.mine_next(miner, &payload, 1_700_000_000 + i as u64 * 12);
        let b = chain.tip();
        println!("  Block #{}: miner={}, uncles={}, reward={} gwei, hash={}...",
            b.height, b.miner, b.uncles.len(), b.miner_reward, &hash[..12]);
    }

    println!("\n  Chain height:  {}", chain.height());
    println!("  GHOST weight:  {} (blocks + uncles)", chain.ghost_weight());
    println!("  Uncle rate:    {:.1}%", chain.uncle_rate() * 100.0);
    println!("  Total rewards: {} gwei", chain.total_rewards());

    // ════════════════════════════════════════════════════════
    // DEMO 2: Uncle blocks — orphan miner vẫn nhận reward
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 2: Uncle Blocks — Orphan Miner Nhận Reward ═══\n");

    let mut chain2 = GhostChain::new(2);

    // Mine block 1
    chain2.mine_next("alice", "block 1", 1_700_000_001);
    // Mine block 2
    chain2.mine_next("bob", "block 2", 1_700_000_013);

    // Giả lập: dave mine 1 orphan block cạnh tranh với block #2
    // (cùng parent là block #1, nhưng không win race)
    let orphan = UncleBlock::new(
        2,                           // uncle height = 2
        "dave",                      // miner của uncle
        chain2.main_chain[1].hash.clone(), // parent = block #1
        9999,                        // nonce khác
        2,
    );
    println!("  Dave mine orphan block tại height 2 (thua race với Bob)");
    println!("  Uncle hash: {}...", &orphan.hash[..12]);
    println!("  Uncle miner: {}", orphan.miner);

    chain2.add_orphan(orphan.clone());
    println!("  → Uncle thêm vào uncle_pool: {} uncle(s)\n", chain2.uncle_pool.len());

    // Mine block 3 — carol include uncle của dave
    let hash3 = chain2.mine_next("carol", "block 3 (includes uncle)", 1_700_000_025);
    let b3 = chain2.tip();

    println!("  Block #3: miner=carol, hash={}...", &hash3[..12]);
    println!("  Uncles included: {}", b3.uncles.len());

    if !b3.uncles.is_empty() {
        let u = &b3.uncles[0];
        let uncle_reward = u.reward(b3.height);
        let nephew_bonus = BASE_REWARD / 32;
        println!("    Uncle[0]: miner={}, height={}", u.miner, u.height);
        println!("    Uncle reward (dave):   {} gwei  ({}/8 × base)", uncle_reward, 8 - (b3.height - u.height));
        println!("    Nephew bonus (carol):  +{} gwei (1/32 × base)", nephew_bonus);
        println!("    Carol total reward:    {} gwei", b3.miner_reward);
    }

    println!("\n  → Dave nhận reward dù block bị orphan ✅");
    println!("  → Carol nhận thêm nephew bonus khi include uncle ✅");

    // ════════════════════════════════════════════════════════
    // DEMO 3: GHOST weight — tại sao dùng subtree weight thay longest chain
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 3: GHOST Weight vs Longest Chain ═══\n");

    let mut chain3 = GhostChain::new(2);

    // Mine 4 blocks, thêm vài uncles
    for i in 0..4u64 {
        // Tạo orphan ở mỗi height
        let parent_hash = chain3.tip().hash.clone();
        let uncle = UncleBlock::new(
            chain3.height() + 1,
            format!("orphan_miner_{}", i),
            parent_hash,
            42000 + i,
            2,
        );
        chain3.add_orphan(uncle);
        chain3.mine_next("main_miner", &format!("block {}", i+1), 1_700_000_000 + i * 12);
    }

    println!("  Main chain length:  {}", chain3.height());
    println!("  Total uncles:       {}", chain3.uncle_count);
    println!("  GHOST weight:       {}", chain3.ghost_weight());
    println!("  Uncle rate:         {:.1}%", chain3.uncle_rate() * 100.0);
    println!();
    println!("  Longest chain = {} blocks", chain3.height());
    println!("  GHOST weight   = {} (blocks + uncles confirmed)", chain3.ghost_weight());
    println!("  → GHOST chain nặng hơn longest chain → bảo mật hơn ✅");

    // ════════════════════════════════════════════════════════
    // DEMO 4: Rewards per miner
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 4: Rewards Per Miner ═══\n");

    let mut rewards_list: Vec<_> = chain2.rewards_per_miner().into_iter().collect();
    rewards_list.sort_by(|a, b| b.1.cmp(&a.1));
    for (miner, reward) in &rewards_list {
        println!("  {}: {} gwei", miner, reward);
    }
    println!("  → Dave nhận uncle reward dù không win block ✅");

    // ════════════════════════════════════════════════════════
    // DEMO 5: Difficulty adjustment theo uncle rate
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 5: Difficulty Adjustment ═══\n");

    let cases = [
        (0.00, "uncle rate 0%  (rất thấp)"),
        (0.05, "uncle rate 5%  (bình thường)"),
        (0.12, "uncle rate 12% (cao — network quá nhanh)"),
    ];

    for (rate, label) in &cases {
        let new_diff = adjust_difficulty(3, *rate);
        println!("  {} → difficulty {} → {}",
            label, 3,
            if new_diff > 3 { format!("{} (tăng ✅)", new_diff) }
            else if new_diff < 3 { format!("{} (giảm)", new_diff) }
            else { format!("{} (giữ nguyên)", new_diff) }
        );
    }

    // ════════════════════════════════════════════════════════
    // DEMO 6: GHOST vs Basic PoW — So sánh
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 6: GHOST vs Basic PoW — So Sánh ═══");
    println!("  ┌──────────────────────┬────────────────────────┬────────────────────────┐");
    println!("  │ Thuộc tính           │ Basic PoW (v0.3)       │ GHOST PoW (v1.9)       │");
    println!("  ├──────────────────────┼────────────────────────┼────────────────────────┤");
    println!("  │ Fork selection       │ Longest chain          │ Heaviest subtree       │");
    println!("  │ Orphan blocks        │ Bị bỏ qua              │ Include làm uncle      │");
    println!("  │ Orphan miner reward  │ ❌ Không có            │ ✅ 7/8 base reward     │");
    println!("  │ Nephew bonus         │ ❌ Không có            │ ✅ +1/32 mỗi uncle     │");
    println!("  │ Security             │ Thấp khi block nhanh   │ Cao hơn (uncle work)   │");
    println!("  │ Block time           │ Phải chậm (10 phút)    │ Có thể nhanh (~15s)    │");
    println!("  │ Uncle depth limit    │ N/A                    │ Tối đa 6 block         │");
    println!("  │ Max uncles/block     │ N/A                    │ 2 uncles               │");
    println!("  │ Ví dụ thực tế        │ Bitcoin                │ Ethereum (trước merge) │");
    println!("  └──────────────────────┴────────────────────────┴────────────────────────┘");

    println!("\n═══ GHOST PoW Summary ═══");
    println!("  ✅ Uncle blocks: orphan miner nhận 7/8 base reward");
    println!("  ✅ Nephew bonus: block proposer nhận +1/32 mỗi uncle include");
    println!("  ✅ GHOST weight: chain selection theo subtree work (bảo mật hơn longest chain)");
    println!("  ✅ Uncle depth: tối đa 6 block (Ethereum spec)");
    println!("  ✅ Max 2 uncles/block (Ethereum spec)");
    println!("  ✅ Difficulty adjustment theo uncle rate");
    println!("  ✅ Rewards per miner tracking (main + uncle)");
}

#[allow(dead_code)]
fn demo_v18() {
    println!("=== Blockchain Rust v1.8 — Zero-Knowledge Proof (ZK-SNARK) ===\n");

    // ════════════════════════════════════════════════════════
    // DEMO 1: Schnorr ZK Proof of Discrete Log (CORRECT ZK)
    // ════════════════════════════════════════════════════════
    println!("═══ Demo 1: Schnorr ZK Proof — Discrete Log ═══");
    println!("  Statement: \"Tôi biết sk sao cho pk = sk * G\"");
    println!("  (ZK thật sự — cùng cơ sở với Taproot v1.3)\n");

    let alice = Wallet::new();
    let msg   = b"zk_domain_blockchain_v18";

    let proof = SchnorrZkProof::prove(&alice.secret_key, msg);
    let ok    = proof.verify();

    println!("  Public key (statement): {}...", hex::encode(&proof.statement[..8]));
    println!("  Proof (R_x || s):       {}...", hex::encode(&proof.signature[..8]));
    println!("  Verify (không cần sk):  {}", if ok { "✅" } else { "❌" });
    println!("  Zero-knowledge:         {}", if proof.is_zero_knowledge() { "✅ sk không trong proof" } else { "❌" });

    // Thử với sk sai → fail
    let mallory = Wallet::new();
    let fake_proof = SchnorrZkProof {
        statement:  proof.statement,
        msg:        msg.to_vec(),
        signature:  SchnorrZkProof::prove(&mallory.secret_key, msg).signature,
    };
    println!("  Fake proof (sk sai):    {}", if !fake_proof.verify() { "✅ bị từ chối" } else { "❌" });

    // ════════════════════════════════════════════════════════
    // DEMO 2: Hash Preimage ZK Proof (Simplified)
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 2: Hash Preimage ZK — \"Tôi biết x sao cho H(x) = y\" ═══");
    println!("  (Simplified sigma protocol + Fiat-Shamir)\n");

    let secret = b"super_secret_preimage_42";
    let public_hash = zk_proof::HashPreimageProof::prove(secret).public_hash;

    println!("  Secret x:        \"{}\" (ẩn)", std::str::from_utf8(secret).unwrap());
    println!("  Public H(x):     {}...", hex::encode(&public_hash[..8]).chars().take(16).collect::<String>());
    println!("  Proving...\n");

    let zk_proof = HashPreimageProof::prove(secret);
    println!("  commit_A:   {}... (randomness commitment)", hex::encode(&zk_proof.commit_a[..8]));
    println!("  commit_W:   {}... (witness commitment)", hex::encode(&zk_proof.commit_w[..8]));
    println!("  challenge:  {}... (Fiat-Shamir)", hex::encode(&zk_proof.challenge[..8]));
    println!("  response:   {}...", hex::encode(&zk_proof.response[..8]));
    println!("  (secret x KHÔNG xuất hiện trong proof)\n");

    println!("  Verify (không cần x):  {}", if zk_proof.verify() { "✅" } else { "❌" });
    println!("  Verify với witness:    {}", if zk_proof.verify_with_witness(secret) { "✅" } else { "❌" });

    // Proof cho secret khác → verify fail với witness
    let wrong_proof = HashPreimageProof::prove(b"wrong_secret");
    println!("  Wrong proof verify:    {}",
        if !wrong_proof.verify_with_witness(secret) { "✅ bị từ chối" } else { "❌" });

    // ════════════════════════════════════════════════════════
    // DEMO 3: R1CS Circuit + Groth16-style SNARK
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 3: R1CS Circuit + Groth16 SNARK ═══");
    println!("  Circuit: y = x² + x + 1");
    println!("  Witness: x = 3 (bí mật)");
    println!("  Public:  y = 13 (ai cũng biết)\n");

    let circuit = R1csCircuit::hash_preimage_demo();
    println!("  Circuit constraints: {}", circuit.constraint_count());
    println!("  Variables:           {} (z = [1, y, x, x²])", circuit.num_variables);
    println!("  Public inputs:       {} (chỉ y)", circuit.num_public);

    // Kiểm tra witness thỏa mãn circuit
    let x_val: i64 = 3;
    let x2_val = x_val * x_val;
    let y_val  = x2_val + x_val + 1;  // = 13
    let z      = vec![1i64, y_val, x_val, x2_val]; // [1, y, x, x²]

    println!("\n  x = {}, x² = {}, y = x²+x+1 = {}", x_val, x2_val, y_val);
    println!("  Circuit satisfied: {}", if circuit.is_satisfied(&z) { "✅" } else { "❌" });

    // Trusted Setup
    println!("\n  [Trusted Setup] Generating (pk, vk)...");
    let (pk, vk) = groth16_setup(&circuit);
    println!("  Proving key:       generated ✅");
    println!("  Verification key:  generated ✅");
    println!("  (Toxic waste τ đã bị xóa)\n");

    // Prove
    let witness = vec![x_val, x2_val];       // [x, x²] — bí mật
    let public  = vec![y_val];               // [y] — public
    let snark_proof = groth16_prove(&pk, &circuit, &witness, &public)
        .expect("Prove ok");

    println!("  [Prove] Prover biết x = {}", x_val);
    println!("  π_A: {}...", hex::encode(&snark_proof.pi_a[..8]));
    println!("  π_B: {}...", hex::encode(&snark_proof.pi_b[..8]));
    println!("  π_C: {}...", hex::encode(&snark_proof.pi_c[..8]));
    println!("  (x = {} không xuất hiện trong proof!)", x_val);

    // Verify
    let snark_ok = groth16_verify(&vk, &snark_proof);
    println!("\n  [Verify] Verifier chỉ biết y = {}", y_val);
    println!("  Groth16 verify: {}", if snark_ok { "✅" } else { "❌" });

    // Wrong witness → proof generation fails
    let bad_witness = vec![5i64, 25i64]; // x=5, x²=25 → y=31 ≠ 13
    let bad_public  = vec![y_val];       // nhưng claim y=13
    match groth16_prove(&pk, &circuit, &bad_witness, &bad_public) {
        Err(e) => println!("  Wrong witness: {} ✅", e),
        Ok(_)  => println!("  ❌ Wrong witness vẫn proof được!"),
    }

    // ════════════════════════════════════════════════════════
    // DEMO 4: ZK Simulator (chứng minh ZK property)
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 4: ZK Simulator ═══");
    println!("  Nếu simulator tồn tại → proof là zero-knowledge\n");

    let real_proof = SchnorrZkProof::prove(&alice.secret_key, msg);
    let simulated  = ZkSimulator::simulate_schnorr(&real_proof.statement, msg);

    println!("  Real proof:      {}...", hex::encode(&real_proof.signature[..8]));
    println!("  Simulated proof: {}...", hex::encode(&simulated[..8]));
    println!("  Computationally indistinguishable: {}",
        if ZkSimulator::are_computationally_indistinguishable(
            &real_proof.signature, &simulated
        ) { "✅" } else { "❌" });
    println!("  → Verifier không phân biệt được → ZK property ✅");

    // ════════════════════════════════════════════════════════
    // DEMO 5: So sánh các loại proof
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 5: ZK Proof Systems So Sánh ═══");
    println!("  ┌──────────────────┬──────────┬──────────┬────────────┬──────────┐");
    println!("  │ System           │ Proof    │ Verify   │ Setup      │ ZK       │");
    println!("  ├──────────────────┼──────────┼──────────┼────────────┼──────────┤");
    println!("  │ Schnorr          │ 64 bytes │ O(1) EC  │ Không cần  │ ✅ Thật  │");
    println!("  │ Groth16          │ 128 bytes│ O(1) pair│ Trusted    │ ✅ Thật  │");
    println!("  │ PLONK            │ ~500B    │ O(1) pair│ Universal  │ ✅ Thật  │");
    println!("  │ ZK-STARK         │ ~100KB   │ O(log n) │ Không cần  │ ✅ Thật  │");
    println!("  │ Hash-based (v1.8)│ 128 bytes│ O(1)     │ Không cần  │ Simplified│");
    println!("  └──────────────────┴──────────┴──────────┴────────────┴──────────┘");

    println!("\n═══ ZK-SNARK Summary ═══");
    println!("  ✅ Schnorr ZK: Completeness + Soundness + ZK (thật sự)");
    println!("  ✅ Hash Preimage: Fiat-Shamir sigma protocol (simplified)");
    println!("  ✅ R1CS Circuit: x * x = x², x² + x + 1 = y (constraints)");
    println!("  ✅ Groth16 API: Setup → Prove → Verify");
    println!("  ✅ Wrong witness bị từ chối tại Prove (không qua Verify)");
    println!("  ✅ Simulator tồn tại → ZK property chứng minh");
    println!("  ✅ Proof không chứa witness (x không xuất hiện trong π)");
}

#[allow(dead_code)]
fn demo_v17() {
    use atomic_swap::{AtomicSwap, SwapVerifier};
    println!("=== Blockchain Rust v1.7 — Atomic Swap ===\n");

    let alice = Wallet::new();
    let bob   = Wallet::new();

    println!("  Alice muốn đổi: 2_000_000 sat BTC (chain A)");
    println!("  Bob   muốn đổi: 1_000_000 sat LTC (chain B)");
    println!("  Timelock A (BTC): block 100  (dài hơn)");
    println!("  Timelock B (LTC): block 50   (ngắn hơn)\n");

    // ════════════════════════════════════════════════════════
    // DEMO 1: Happy path — swap thành công
    // ════════════════════════════════════════════════════════
    println!("═══ Demo 1: Happy Path — Swap thành công ═══\n");

    let mut swap = AtomicSwap::propose(
        2_000_000,  // alice_amount (BTC)
        1_000_000,  // bob_amount (LTC)
        100,        // locktime_a (dài)
        50,         // locktime_b (ngắn)
    );

    println!("  [Step 1] Alice tạo secret & hash_lock");
    println!("           hash_lock: {}...", &swap.hash_lock_hex()[..16]);
    println!("           (secret giữ bí mật cho đến khi claim)\n");

    println!("  [Step 2] Alice lock BTC trên chain A:");
    swap.alice_lock(&alice, &bob).expect("Alice lock ok");

    println!("\n  [Step 3] Bob kiểm tra HTLC_A, thấy hash_lock khớp");
    println!("           Bob đồng ý tham gia swap\n");

    println!("  [Step 4] Bob lock LTC trên chain B:");
    swap.bob_lock(&alice, &bob).expect("Bob lock ok");

    // Verify 2 HTLC dùng cùng hash_lock
    let htlc_a = swap.htlc_a.as_ref().unwrap();
    let htlc_b = swap.htlc_b.as_ref().unwrap();
    println!("\n  Verify linked (cùng hash_lock): {}",
        if SwapVerifier::verify_linked(htlc_a, htlc_b) { "✅" } else { "❌" });
    println!("  Verify timelock order (T_A > T_B): {}",
        if SwapVerifier::verify_timelock_order(htlc_a, htlc_b) { "✅" } else { "❌" });

    println!("\n  [Step 5] Alice reveal secret, claim LTC từ HTLC_B:");
    let preimage = swap.alice_claim(&alice).expect("Alice claim ok");

    println!("\n  [Step 6] Bob thấy preimage on-chain, claim BTC từ HTLC_A:");
    swap.bob_claim(&bob, &preimage).expect("Bob claim ok");

    // Verify preimage
    println!("\n  Verify preimage: {}",
        if SwapVerifier::verify_preimage(&preimage, &swap.hash_lock) { "✅" } else { "❌" });
    println!("  Trạng thái cuối: {:?}", swap.state);

    // ════════════════════════════════════════════════════════
    // DEMO 2: Bob không lock → Alice refund sau T1
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 2: Bob Không Lock → Alice Refund ═══\n");

    let alice2 = Wallet::new();
    let bob2   = Wallet::new();

    let mut swap2 = AtomicSwap::propose(2_000_000, 1_000_000, 100, 50);

    println!("  Alice lock BTC...");
    swap2.alice_lock(&alice2, &bob2).expect("ok");

    println!("\n  Bob biến mất — không lock LTC");
    println!("  Alice đợi đến block 100 rồi refund:\n");

    // Thử refund trước locktime → bị từ chối
    match swap2.alice_refund(&alice2, 50) {
        Err(e) => println!("  Refund sớm (block 50): {} ✅", e),
        Ok(_)  => println!("  ❌ Refund sớm không bị chặn!"),
    }

    // Refund đúng sau locktime
    match swap2.alice_refund(&alice2, 100) {
        Ok(_)  => println!("  Refund đúng hạn (block 100): ✅"),
        Err(e) => println!("  ❌ {}", e),
    }
    println!("  Trạng thái: {:?}", swap2.state);

    // ════════════════════════════════════════════════════════
    // DEMO 3: Alice không claim kịp T2 → Bob refund LTC
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 3: Alice Không Claim Kịp → Bob Refund ═══\n");

    let alice3 = Wallet::new();
    let bob3   = Wallet::new();

    let mut swap3 = AtomicSwap::propose(2_000_000, 1_000_000, 100, 50);
    swap3.alice_lock(&alice3, &bob3).expect("ok");
    swap3.bob_lock(&alice3, &bob3).expect("ok");

    println!("  Cả 2 đã lock. Alice lỡ deadline T2 (block 50)...\n");

    // Bob refund sau locktime_b
    match swap3.bob_refund(&bob3, 50) {
        Ok(_)  => println!("  Bob refund LTC (block 50): ✅"),
        Err(e) => println!("  ❌ {}", e),
    }
    println!("  Trạng thái: {:?}", swap3.state);
    println!("  (Alice vẫn có thể refund BTC sau block 100)");

    // ════════════════════════════════════════════════════════
    // DEMO 4: Sơ đồ protocol
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 4: Protocol Timeline ═══");
    println!("
  Alice (BTC chain)            Bob (LTC chain)
  ─────────────────            ───────────────
  secret s, h=H(s)
        │
        ├── lock HTLC_A ──────────────────►
        │   amount=2M, hash=h, T=100
        │                                  verify h
        │                                  lock HTLC_B
        │◄─────────────────────────────────┤
        │                     amount=1M, hash=h, T=50
        │
        ├── reveal s, claim HTLC_B ───────►
        │   (LTC về tay Alice)             s lộ on-chain
        │                                  claim HTLC_A
        │                         (BTC về tay Bob) ✅
        │
  Fail: Bob không lock → Alice refund sau T=100 ↩
  Fail: Alice không claim → Bob refund sau T=50  ↩
    ");

    println!("═══ Atomic Swap Summary ═══");
    println!("  ✅ HTLC: hash_lock + timelock đảm bảo atomicity");
    println!("  ✅ Không cần trust: trustless hoàn toàn");
    println!("  ✅ Happy path: cả 2 nhận tiền");
    println!("  ✅ Refund path A: Bob không lock → Alice refund sau T1");
    println!("  ✅ Refund path B: Alice không claim → Bob refund sau T2");
    println!("  ✅ T1 > T2: đảm bảo Bob đủ thời gian claim sau Alice");
    println!("  ✅ Cross-chain: cùng hash_lock, 2 chain độc lập");
}

#[allow(dead_code)]
fn demo_v16() {
    println!("=== Blockchain Rust v1.6 — CoinJoin ===\n");
    use coinjoin::{CoinJoinParticipant, CoinJoinSession, PayJoinSession, TxAnalysis, CoinJoinTranscript};

    // ════════════════════════════════════════════════════════
    // DEMO 1: CoinJoin cơ bản (3 participants, equal amounts)
    // ════════════════════════════════════════════════════════
    println!("═══ Demo 1: CoinJoin — 3 Participants ═══");
    println!("  Denomination: 1_000_000 sat (0.01 BTC)");
    println!("  Fee per input: 1_000 sat\n");

    let alice = Wallet::new();
    let bob   = Wallet::new();
    let carol = Wallet::new();

    let alice_hash = hex::encode(Script::pubkey_hash(&alice.public_key.serialize()));
    let bob_hash   = hex::encode(Script::pubkey_hash(&bob.public_key.serialize()));
    let carol_hash = hex::encode(Script::pubkey_hash(&carol.public_key.serialize()));

    let denomination  = 1_000_000u64;
    let fee_per_input = 1_000u64;

    // Mỗi người có UTXO lớn hơn denomination
    let alice_utxo = 3_001_000u64; // có 3 BTC, gộp 1 BTC
    let bob_utxo   = 1_001_000u64; // có 1 BTC, gộp 1 BTC (không có change)
    let carol_utxo = 2_001_000u64; // có 2 BTC, gộp 1 BTC

    let p_alice = CoinJoinParticipant::new(
        alice, "alice_utxo_abc".to_string(), 0, alice_utxo, denomination
    ).expect("Alice UTXO đủ");

    let p_bob = CoinJoinParticipant::new(
        bob, "bob_utxo_def".to_string(), 0, bob_utxo, denomination
    ).expect("Bob UTXO đủ");

    let p_carol = CoinJoinParticipant::new(
        carol, "carol_utxo_ghi".to_string(), 0, carol_utxo, denomination
    ).expect("Carol UTXO đủ");

    println!("  Alice:  UTXO {} sat, denomination {} sat", alice_utxo, denomination);
    println!("  Bob:    UTXO {} sat, denomination {} sat", bob_utxo, denomination);
    println!("  Carol:  UTXO {} sat, denomination {} sat\n", carol_utxo, denomination);

    let mut session = CoinJoinSession::new(denomination, fee_per_input);
    session.join(p_alice).expect("Alice joined");
    session.join(p_bob).expect("Bob joined");
    session.join(p_carol).expect("Carol joined");

    println!("  Participants: {}", session.participant_count());

    let cj_tx = session.build().expect("CoinJoin TX built");

    println!("  TX id:        {}...", &cj_tx.tx.tx_id[..16]);
    println!("  Inputs:       {}", cj_tx.tx.inputs.len());
    println!("  Outputs:      {} ({} equal @ {} sat + {} change)",
        cj_tx.tx.outputs.len(),
        cj_tx.participant_count,
        cj_tx.denomination,
        cj_tx.tx.outputs.len() - cj_tx.participant_count,
    );
    println!("  Fee total:    {} sat", fee_per_input * 3);
    println!("  Anonymity set: {} (observer cần thử {}! = {} combinations)",
        cj_tx.anonymity_set(),
        cj_tx.anonymity_set(),
        (1..=cj_tx.anonymity_set() as u64).product::<u64>()
    );

    println!("\n  On-chain outputs (observer thấy):");
    for (i, out) in cj_tx.tx.outputs.iter().enumerate() {
        if out.amount == denomination {
            println!("    [{}] {} sat ← equal (ai nhận?)", i, out.amount);
        } else {
            println!("    [{}] {} sat  (change)", i, out.amount);
        }
    }

    // ════════════════════════════════════════════════════════
    // DEMO 2: CoinJoin Transcript (audit log — không lộ identity)
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 2: CoinJoin Transcript (Audit Log) ═══");

    let transcript = CoinJoinTranscript::from_session(&session, &cj_tx);
    println!("  Session ID:        {}...", transcript.session_id);
    println!("  TX ID:             {}...", transcript.tx_id);
    println!("  Denomination:      {} sat", transcript.denomination);
    println!("  Participants:      {}", transcript.participant_count);
    println!("  Total fee:         {} sat", transcript.total_fee);
    println!("  Anonymity set:     {}", transcript.anonymity_set);
    println!("  (Identities KHÔNG được lưu — chỉ lưu metadata on-chain)");

    // ════════════════════════════════════════════════════════
    // DEMO 3: Denomination mismatch → bị từ chối
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 3: Denomination Mismatch Prevention ═══");

    let dave = Wallet::new();
    let wrong_denom = 500_000u64; // 0.005 BTC ≠ 0.01 BTC

    let p_dave = CoinJoinParticipant::new(
        dave, "dave_utxo".to_string(), 0, 2_000_000, wrong_denom
    ).expect("Dave UTXO đủ");

    let mut session2 = CoinJoinSession::new(denomination, fee_per_input);
    match session2.join(p_dave) {
        Err(e) => println!("  ✅ Denomination mismatch bị từ chối: {}", e),
        Ok(_)  => println!("  ❌ Lỗi: denomination không khớp vẫn được chấp nhận"),
    }

    // ════════════════════════════════════════════════════════
    // DEMO 4: PayJoin (P2EP) — Sender + Receiver đều góp input
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 4: PayJoin / P2EP ═══");
    println!("  Alice (sender) gửi 2 BTC cho Bob (receiver)");
    println!("  Bob đóng góp 0.5 BTC UTXO của mình vào TX\n");

    let alice2 = Wallet::new();
    let bob2   = Wallet::new();

    let payment_amount = 2_000_000u64;  // 0.02 BTC
    let payjoin_fee    = 2_000u64;

    let alice_utxo2 = 3_000_000u64;  // Alice có 3 BTC
    let bob_utxo2   = 500_000u64;    // Bob có 0.5 BTC — sẽ gộp vào TX

    let bob2_hash = hex::encode(Script::pubkey_hash(&bob2.public_key.serialize()));

    let mut pj = PayJoinSession::new(payment_amount, payjoin_fee);
    pj.add_sender(alice2, "alice_utxo_xyz".to_string(), 0, alice_utxo2);
    pj.add_receiver(bob2, "bob_utxo_xyz".to_string(), 0, bob_utxo2);

    let pj_tx = pj.build().expect("PayJoin TX built");

    println!("  TX id: {}...", &pj_tx.tx_id[..16]);
    println!("  Inputs: {}", pj_tx.inputs.len());
    println!("  Outputs:");
    for (i, out) in pj_tx.outputs.iter().enumerate() {
        let is_receiver = out.amount == payment_amount + bob_utxo2;
        if is_receiver {
            println!("    [{}] {} sat ← Bob nhận (payment {} + UTXO {} absorbed)",
                i, out.amount, payment_amount, bob_utxo2);
        } else {
            println!("    [{}] {} sat ← Alice change", i, out.amount);
        }
    }

    println!("\n  Observer analysis:");
    println!("    {}", TxAnalysis::naive_change_heuristic(&pj_tx));
    println!("    {}", TxAnalysis::common_input_heuristic(&pj_tx));

    // ════════════════════════════════════════════════════════
    // DEMO 5: So sánh Normal TX vs CoinJoin vs PayJoin
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 5: Privacy Comparison ═══");

    println!("  ┌─────────────────┬──────────┬──────────────────────────────────┐");
    println!("  │ TX Type         │ Inputs   │ Privacy                          │");
    println!("  ├─────────────────┼──────────┼──────────────────────────────────┤");
    println!("  │ Normal TX       │ 1        │ ❌ input → output rõ ràng        │");
    println!("  │ CoinJoin        │ N        │ ✅ N! combinations, không trace  │");
    println!("  │ PayJoin (P2EP)  │ 2        │ ✅ heuristics đều thất bại       │");
    println!("  └─────────────────┴──────────┴──────────────────────────────────┘");

    // ════════════════════════════════════════════════════════
    // DEMO 6: Large CoinJoin (10 participants) — anonymity set = 10!
    // ════════════════════════════════════════════════════════
    println!("\n═══ Demo 6: Large CoinJoin (5 participants) ═══");

    let large_denom = 500_000u64;  // 0.005 BTC
    let large_fee   = 500u64;
    let mut large_session = CoinJoinSession::new(large_denom, large_fee);

    for i in 0..5usize {
        let w = Wallet::new();
        let utxo_amount = large_denom + large_fee + (i as u64 * 100_000);
        let p = CoinJoinParticipant::new(
            w,
            format!("utxo_{}", i),
            0,
            utxo_amount,
            large_denom,
        ).unwrap();
        large_session.join(p).unwrap();
    }

    let large_cj = large_session.build().expect("Large CoinJoin built");
    let combinations: u64 = (1..=large_cj.participant_count as u64).product();

    println!("  Participants:   {}", large_cj.participant_count);
    println!("  Denomination:   {} sat", large_cj.denomination);
    println!("  Anonymity set:  {}", large_cj.anonymity_set());
    println!("  Combinations:   {}! = {}", large_cj.participant_count, combinations);
    println!("  TX inputs:      {}", large_cj.tx.inputs.len());
    println!("  TX outputs:     {}", large_cj.tx.outputs.len());
    println!("  Observer: {}",  large_cj.observer_analysis());

    println!("\n═══ CoinJoin Summary ═══");
    println!("  ✅ Equal-amount outputs: phá vỡ input → output linkage");
    println!("  ✅ Coordinator: thu thập inputs, build TX, participants ký");
    println!("  ✅ Denomination enforcement: tất cả phải gộp cùng amount");
    println!("  ✅ Change outputs: tiền thừa trả về từng người");
    println!("  ✅ Anonymity set = N participants, {} combinations", combinations);
    println!("  ✅ PayJoin / P2EP: sender + receiver gộp inputs");
    println!("  ✅ Heuristics thất bại: CIOH, change detection");
    println!("  ✅ Transcript: audit log không lộ identity");
    let _ = (alice_hash, bob_hash, carol_hash, bob2_hash);
}
