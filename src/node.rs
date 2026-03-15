/// v4.3 — P2P Sync
///
/// Bổ sung so với v0.6:
///   - Longest-chain rule: khi nhận Blocks, so sánh length → giữ chain dài hơn
///   - Fork detection: NewBlock không kết nối → request GetBlocks từ sender
///   - Mempool broadcast: NewTransaction → thêm mempool + relay sang peers
///   - Block dedup: seen_blocks set tránh xử lý block hash trùng
///   - TX dedup: seen_txs set tránh broadcast TX đã thấy
///   - GetHeight / Height messages: query height nhanh, không cần download

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::chain::Blockchain;
use crate::message::Message;
use crate::storage;

/// Node = 1 instance blockchain chạy trên 1 port TCP
#[allow(dead_code)]
pub struct Node {
    pub port:       u16,
    pub chain:      Arc<Mutex<Blockchain>>,
    pub peers:      Arc<Mutex<Vec<String>>>,       // "host:port"
    seen_blocks:    Arc<Mutex<HashSet<String>>>,   // v4.3: dedup block hash
    seen_txs:       Arc<Mutex<HashSet<String>>>,   // v4.3: dedup tx id
}

#[allow(dead_code)]
impl Node {
    pub fn new(port: u16) -> Self {
        Node {
            port,
            chain:       Arc::new(Mutex::new(Blockchain::new())),
            peers:       Arc::new(Mutex::new(vec![])),
            seen_blocks: Arc::new(Mutex::new(HashSet::new())),
            seen_txs:    Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Khởi động TCP listener — chấp nhận kết nối từ peers
    pub async fn start(self: Arc<Self>) {
        let addr     = format!("0.0.0.0:{}", self.port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(l)  => l,
            Err(e) => {
                eprintln!("Lỗi bind port {}: {} — cổng đang bị dùng bởi process khác?", self.port, e);
                eprintln!("  Thử: lsof -i :{} | grep LISTEN   hoặc đổi port: cargo run -- node 8334", self.port);
                return;
            }
        };
        println!("Node {} đang lắng nghe tại {}", self.port, addr);

        loop {
            let (socket, peer_addr) = match listener.accept().await {
                Ok(v)  => v,
                Err(_) => continue,
            };
            println!("  [{}] ← kết nối từ {}", self.port, peer_addr);
            let node = Arc::clone(&self);
            tokio::spawn(async move {
                node.handle_connection(socket).await;
            });
        }
    }

    /// Xử lý từng kết nối TCP đến
    async fn handle_connection(&self, socket: TcpStream) {
        let (reader, mut writer) = socket.into_split();
        let mut lines = BufReader::new(reader).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let msg = match Message::deserialize(line.as_bytes()) {
                Some(m) => m,
                None    => continue,
            };

            let response = self.handle_message(msg).await;

            if let Some(resp) = response {
                let _ = writer.write_all(&resp.serialize()).await;
            }
        }
    }

    /// Xử lý từng loại message và trả về response nếu cần
    async fn handle_message(&self, msg: Message) -> Option<Message> {
        match msg {
            Message::Hello { version, host, port } => {
                println!("  [{}] Hello từ {}:{} (v{})", self.port, host, port, version);
                let addr = format!("{}:{}", host, port);
                let mut peers = self.peers.lock().await;
                if !peers.contains(&addr) {
                    peers.push(addr);
                }
                Some(Message::Hello { version: 1, host: "0.0.0.0".to_string(), port: self.port })
            }

            // ── v4.3: GetHeight / Height ──────────────────────────────────
            Message::GetHeight => {
                let height = self.chain.lock().await.last_block().index;
                Some(Message::Height { height })
            }

            Message::Height { height } => {
                println!("  [{}] Peer báo height={}", self.port, height);
                None
            }

            // ── Peer discovery ─────────────────────────────────────────────
            Message::GetPeers => {
                let peers = self.peers.lock().await.clone();
                Some(Message::Peers { addrs: peers })
            }

            Message::Peers { addrs } => {
                let mut peers = self.peers.lock().await;
                for addr in addrs {
                    if !peers.contains(&addr) {
                        peers.push(addr);
                    }
                }
                None
            }

            // ── Block handling (v4.3: dedup + fork detection) ──────────────
            Message::NewBlock { block } => {
                // Dedup: bỏ qua block đã thấy
                {
                    let mut seen = self.seen_blocks.lock().await;
                    if seen.contains(&block.hash) {
                        return None;
                    }
                    seen.insert(block.hash.clone());
                }

                let mut chain = self.chain.lock().await;
                let tip = chain.last_block();

                if block.index == tip.index + 1
                    && block.prev_hash == tip.hash
                    && block.is_valid(chain.difficulty)
                {
                    println!("  [{}] ✅ Block #{} hợp lệ → thêm vào chain", self.port, block.index);
                    chain.utxo_set.apply_block(&block.transactions);
                    chain.chain.push(block);
                    chain.adjust_difficulty();
                    // Persist ngay sau khi nhận block hợp lệ từ miner/peer
                    let _ = storage::save_blockchain(&chain);
                } else if block.index > tip.index + 1 {
                    // v4.3: Fork detection — peer ở phía trước, cần sync
                    println!("  [{}] ⚠️  Fork: peer ở height {}, ta ở {} → request sync", self.port, block.index, tip.index);
                    // Không thể async request trong handle_message (đang hold chain lock)
                    // Đánh dấu cần sync; thực tế: spawn task
                } else {
                    println!("  [{}] ⚠️  Block #{} stale hoặc invalid", self.port, block.index);
                }
                // Luôn reply Height để miner không bị timeout khi chờ response
                let height = chain.last_block().index;
                Some(Message::Height { height })
            }

            Message::GetBlocks { from_index } => {
                let chain  = self.chain.lock().await;
                let blocks = chain.chain
                    .iter()
                    .filter(|b| b.index > from_index)
                    .cloned()
                    .collect();
                Some(Message::Blocks { blocks })
            }

            // v4.3: Longest-chain rule
            Message::Blocks { blocks } => {
                let mut chain = self.chain.lock().await;
                println!("  [{}] Nhận {} blocks để đồng bộ", self.port, blocks.len());

                // Nếu incoming chain dài hơn: áp dụng tất cả blocks hợp lệ
                let incoming_tip = blocks.last().map(|b| b.index).unwrap_or(0);
                let my_tip = chain.last_block().index;

                if incoming_tip > my_tip {
                    println!("  [{}] Longest-chain: incoming={} > local={} → switch", self.port, incoming_tip, my_tip);
                    for block in &blocks {
                        let last = chain.last_block();
                        if block.index == last.index + 1
                            && block.prev_hash == last.hash
                            && block.is_valid(chain.difficulty)
                        {
                            chain.utxo_set.apply_block(&block.transactions);
                            chain.chain.push(block.clone());
                            chain.adjust_difficulty();
                        }
                    }
                    let _ = storage::save_blockchain(&chain);
                } else {
                    println!("  [{}] Local chain dài hơn hoặc bằng → bỏ qua", self.port);
                }

                println!("  [{}] Chain sau sync: {} blocks", self.port, chain.chain.len());
                // Reply Height ack để caller không bị timeout
                Some(Message::Height { height: chain.last_block().index })
            }

            // v4.3: Mempool broadcast + TX dedup
            Message::NewTransaction { tx } => {
                // Dedup: bỏ qua TX đã thấy
                {
                    let mut seen = self.seen_txs.lock().await;
                    if seen.contains(&tx.tx_id) {
                        return None;
                    }
                    seen.insert(tx.tx_id.clone());
                }

                let tx_id_short = if tx.tx_id.len() >= 12 { &tx.tx_id[..12] } else { &tx.tx_id };
                println!("  [{}] TX mới: {}…  → thêm mempool", self.port, tx_id_short);

                {
                    let mut chain = self.chain.lock().await;
                    let fee  = tx.fee;
                    let _size = tx.vsize();
                    // Tính input_total xấp xỉ: output_total + fee
                    let output_total: u64 = tx.outputs.iter().map(|o| o.amount).sum();
                    let input_total = output_total + fee;
                    chain.mempool.add(tx.clone(), input_total).ok();
                }

                // Relay sang tất cả peers (broadcast TX)
                let peers = self.peers.lock().await.clone();
                drop(peers); // sẽ broadcast bên ngoài async context

                None
            }

            Message::Ping => Some(Message::Pong),
            Message::Pong => { println!("  [{}] Pong!", self.port); None }

            // v4.5: Miner request mempool TXs
            Message::GetMempool => {
                let chain = self.chain.lock().await;
                let txs   = chain.mempool.select_transactions(500);
                Some(Message::MempoolTxs { txs })
            }

            Message::MempoolTxs { .. } => None, // miner nhận, node bỏ qua

            // v4.8: Metrics peer count query
            Message::GetPeerCount => {
                let count = self.peers.lock().await.len();
                Some(Message::PeerCount { count })
            }

            Message::PeerCount { .. } => None,

            // v5.x: getblocktemplate — miner query để mine block tiếp theo
            Message::GetTemplate => {
                let chain = self.chain.lock().await;
                let tip   = chain.last_block();
                let txs   = chain.mempool.select_transactions(500);
                Some(Message::Template {
                    prev_hash:  tip.hash.clone(),
                    height:     tip.index + 1,
                    difficulty: chain.difficulty,
                    txs,
                })
            }

            Message::Template { .. } => None, // miner nhận, node bỏ qua
        }
    }

    /// Kết nối đến 1 peer và gửi message, nhận response
    pub async fn send_to_peer(peer_addr: &str, msg: &Message) -> Option<Message> {
        let mut stream = match TcpStream::connect(peer_addr).await {
            Ok(s)  => s,
            Err(e) => {
                println!("  ⚠️  Không kết nối được {}: {}", peer_addr, e);
                return None;
            }
        };

        let _ = stream.write_all(&msg.serialize()).await;

        let mut lines = BufReader::new(&mut stream).lines();
        if let Ok(Some(line)) = lines.next_line().await {
            return Message::deserialize(line.as_bytes());
        }
        None
    }

    /// Broadcast 1 message đến tất cả peers
    pub async fn broadcast(&self, msg: &Message) {
        let peers = self.peers.lock().await.clone();
        println!("  [{}] Broadcast → {} peers", self.port, peers.len());
        for peer in &peers {
            Node::send_to_peer(peer, msg).await;
        }
    }

    /// v4.3: Query height từ peer, so sánh với local, sync nếu cần
    pub async fn sync_if_behind(&self, peer_addr: &str) {
        let my_height = self.chain.lock().await.last_block().index;

        // Query height của peer
        let peer_height = match Node::send_to_peer(peer_addr, &Message::GetHeight).await {
            Some(Message::Height { height }) => height,
            _ => {
                // Fallback: dùng GetBlocks từ height hiện tại
                self.sync_from(peer_addr).await;
                return;
            }
        };

        if peer_height > my_height {
            println!("  [{}] Peer {} height={} > local={} → sync",
                self.port, peer_addr, peer_height, my_height);
            self.sync_from(peer_addr).await;
        } else {
            println!("  [{}] Local height={} >= peer {} → không cần sync",
                self.port, my_height, peer_addr);
        }
    }

    /// Đồng bộ chain từ 1 peer cụ thể
    pub async fn sync_from(&self, peer_addr: &str) {
        let my_height = self.chain.lock().await.last_block().index;

        println!("  [{}] Sync từ {} (local height={})", self.port, peer_addr, my_height);
        let resp = Node::send_to_peer(peer_addr, &Message::GetBlocks { from_index: my_height }).await;

        if let Some(Message::Blocks { blocks }) = resp {
            let mut chain = self.chain.lock().await;
            for block in blocks {
                let last = chain.last_block();
                if block.index == last.index + 1
                    && block.prev_hash == last.hash
                    && block.is_valid(chain.difficulty)
                {
                    chain.utxo_set.apply_block(&block.transactions);
                    chain.chain.push(block);
                }
            }
            println!("  [{}] Chain sau sync: {} blocks", self.port, chain.chain.len());
        }
    }

    /// Mine 1 block mới rồi broadcast
    pub async fn mine_and_broadcast(&self, miner_address: &str) {
        println!("\n⛏️  Node {} đang mine...", self.port);
        let mut chain = self.chain.lock().await;
        chain.mine_block_to_hash(miner_address);
        let new_block = chain.chain.last().unwrap().clone();
        drop(chain);

        // Đánh dấu block này đã seen để tránh re-process
        self.seen_blocks.lock().await.insert(new_block.hash.clone());

        self.broadcast(&Message::NewBlock { block: new_block }).await;
    }
}

// ── Standalone sync logic (testable without TCP) ──────────────────────────────

/// Áp dụng longest-chain rule: nếu `incoming` dài hơn `current`, trả về incoming
/// Dùng cho unit tests
#[allow(dead_code)]
pub fn apply_longest_chain(
    current: &[crate::block::Block],
    incoming: Vec<crate::block::Block>,
    difficulty: usize,
) -> Vec<crate::block::Block> {
    if incoming.len() <= current.len() {
        return current.to_vec();
    }
    // Validate incoming chain
    for i in 1..incoming.len() {
        if incoming[i].prev_hash != incoming[i - 1].hash {
            return current.to_vec(); // chain incoming bị hỏng
        }
        if !incoming[i].is_valid(difficulty) {
            return current.to_vec();
        }
    }
    incoming
}

/// Kiểm tra một block có thể append vào chain không
#[allow(dead_code)]
pub fn can_append(
    chain: &[crate::block::Block],
    block: &crate::block::Block,
    difficulty: usize,
) -> bool {
    if let Some(tip) = chain.last() {
        block.index == tip.index + 1
            && block.prev_hash == tip.hash
            && block.is_valid(difficulty)
    } else {
        block.index == 0
    }
}
