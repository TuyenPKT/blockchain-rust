#![allow(dead_code)]

/// v3.5 — Cross-chain Messaging (IBC-style)
///
/// Simplified Inter-Blockchain Communication (IBC) protocol.
/// Two sovereign chains exchange packets through light clients and channels.
///
/// ─── IBC Stack ───────────────────────────────────────────────────────────────
///
///   Application   Token transfer, NFT bridge, arbitrary data
///   ─────────────────────────────────────────────────────────────────────────
///   Channel (ICS-04)   Ordered/unordered packet delivery, sequence numbers
///   Connection (ICS-03) Authenticated link between two light clients
///   Client (ICS-02)    Light client tracking counterparty chain headers
///   ─────────────────────────────────────────────────────────────────────────
///   Transport         TCP/IP (production) or in-process relay (this demo)
///
/// ─── Connection Handshake (4-way) ────────────────────────────────────────────
///
///   Chain A: ConnOpenInit     → A:INIT
///   Chain B: ConnOpenTry      → B:TRYOPEN
///   Chain A: ConnOpenAck      → A:OPEN
///   Chain B: ConnOpenConfirm  → B:OPEN
///
/// ─── Channel Handshake (4-way) ───────────────────────────────────────────────
///
///   Chain A: ChanOpenInit     → A:INIT
///   Chain B: ChanOpenTry      → B:TRYOPEN
///   Chain A: ChanOpenAck      → A:OPEN
///   Chain B: ChanOpenConfirm  → B:OPEN
///
/// ─── Packet Lifecycle ────────────────────────────────────────────────────────
///
///   1. App on A calls send_packet → commitment stored on A, state_root updated
///   2. Relayer reads packet, submits recv_packet to B with commitment proof
///   3. B verifies proof, processes packet, stores receipt + ack
///   4. Relayer reads ack from B, submits ack_packet to A
///   5. A verifies ack, clears commitment → packet lifecycle complete
///
/// ─── Timeout ─────────────────────────────────────────────────────────────────
///
///   If B.height >= packet.timeout_height before recv:
///     Relayer submits timeout_packet to A
///     A clears commitment → app can refund/retry
///
/// References: ICS-02, ICS-03, ICS-04; cosmos/ibc-go; IBC spec v1.0

use sha2::{Sha256, Digest};
use std::collections::HashMap;

// ─── Constants ────────────────────────────────────────────────────────────────

pub const IBC_VERSION: &str = "ibc-1";
pub const DEFAULT_TIMEOUT: u64 = 100;  // blocks

// ─── Light Client ─────────────────────────────────────────────────────────────

/// Tracks headers of a counterparty chain to verify packet proofs
#[derive(Clone, Debug)]
pub struct ClientState {
    pub client_id:      String,
    pub chain_id:       String,
    pub latest_height:  u64,
    pub latest_hash:    [u8; 32],
    pub frozen:         bool,
    pub headers:        HashMap<u64, [u8; 32]>,  // height → block hash
}

impl ClientState {
    pub fn new(client_id: &str, chain_id: &str, height: u64, hash: [u8; 32]) -> Self {
        let mut headers = HashMap::new();
        headers.insert(height, hash);
        ClientState {
            client_id:     client_id.to_string(),
            chain_id:      chain_id.to_string(),
            latest_height: height,
            latest_hash:   hash,
            frozen:        false,
            headers,
        }
    }

    pub fn update(&mut self, height: u64, hash: [u8; 32]) -> Result<(), String> {
        if self.frozen {
            return Err("Client is frozen (misbehaviour detected)".to_string());
        }
        if height <= self.latest_height {
            return Err(format!("Height {} ≤ latest {}", height, self.latest_height));
        }
        self.headers.insert(height, hash);
        self.latest_height = height;
        self.latest_hash = hash;
        Ok(())
    }

    /// Verify that a commitment existed at a given height
    /// Production: full Merkle proof verification against stored root
    /// Here: check that height is tracked (relayer submitted header)
    pub fn verify_commitment(&self, height: u64) -> bool {
        !self.frozen && self.headers.contains_key(&height)
    }
}

// ─── Connection ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum ConnState { Init, TryOpen, Open }

impl ConnState {
    pub fn label(&self) -> &str {
        match self {
            ConnState::Init    => "INIT",
            ConnState::TryOpen => "TRYOPEN",
            ConnState::Open    => "OPEN",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Connection {
    pub id:                    String,
    pub client_id:             String,
    pub counterparty_chain_id: String,
    pub counterparty_conn_id:  Option<String>,
    pub state:                 ConnState,
    pub version:               String,
}

// ─── Channel ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum ChanState { Init, TryOpen, Open, Closed }

impl ChanState {
    pub fn label(&self) -> &str {
        match self {
            ChanState::Init    => "INIT",
            ChanState::TryOpen => "TRYOPEN",
            ChanState::Open    => "OPEN",
            ChanState::Closed  => "CLOSED",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ordering { Ordered, Unordered }

impl Ordering {
    pub fn label(&self) -> &str {
        match self {
            Ordering::Ordered   => "ORDERED",
            Ordering::Unordered => "UNORDERED",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub id:                      String,
    pub port:                    String,
    pub connection_id:           String,
    pub counterparty_channel_id: Option<String>,
    pub counterparty_port:       String,
    pub ordering:                Ordering,
    pub state:                   ChanState,
    pub next_seq_send:           u64,
    pub next_seq_recv:           u64,
}

// ─── Packet ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Packet {
    pub sequence:       u64,
    pub src_port:       String,
    pub src_channel:    String,
    pub dst_port:       String,
    pub dst_channel:    String,
    pub data:           Vec<u8>,
    pub timeout_height: u64,  // packet invalid if dst_chain.height >= this
}

impl Packet {
    /// Packet commitment = H(sequence ‖ ports ‖ channels ‖ data ‖ timeout)
    /// Stored on source chain; verified by destination chain
    pub fn commitment(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"packet_commitment_v1");
        h.update(&self.sequence.to_le_bytes());
        h.update(self.src_port.as_bytes());
        h.update(self.src_channel.as_bytes());
        h.update(self.dst_port.as_bytes());
        h.update(self.dst_channel.as_bytes());
        h.update(&self.data);
        h.update(&self.timeout_height.to_le_bytes());
        let out = h.finalize();
        let mut r = [0u8; 32];
        r.copy_from_slice(&out);
        r
    }
}

// ─── IBC-capable Chain ────────────────────────────────────────────────────────

pub struct IbcChain {
    pub chain_id:    String,
    pub height:      u64,
    pub state_root:  [u8; 32],

    pub clients:     HashMap<String, ClientState>,
    pub connections: HashMap<String, Connection>,
    pub channels:    HashMap<String, Channel>,

    /// Packet commitments on source chain: (chan_id, seq) → commitment hash
    pub commitments: HashMap<(String, u64), [u8; 32]>,
    /// Packet receipts on destination chain: (chan_id, seq) → received
    pub receipts:    HashMap<(String, u64), bool>,
    /// Acknowledgements: (chan_id, seq) → ack bytes
    pub acks:        HashMap<(String, u64), Vec<u8>>,

    pub events:      Vec<String>,
    conn_counter:    u64,
    chan_counter:    u64,
}

impl IbcChain {
    pub fn new(chain_id: &str) -> Self {
        IbcChain {
            chain_id:    chain_id.to_string(),
            height:      1,
            state_root:  [0u8; 32],
            clients:     HashMap::new(),
            connections: HashMap::new(),
            channels:    HashMap::new(),
            commitments: HashMap::new(),
            receipts:    HashMap::new(),
            acks:        HashMap::new(),
            events:      Vec::new(),
            conn_counter: 0,
            chan_counter:  0,
        }
    }

    pub fn advance(&mut self, n: u64) {
        self.height += n;
        self.recompute_state_root();
    }

    fn recompute_state_root(&mut self) {
        let mut h = Sha256::new();
        h.update(self.chain_id.as_bytes());
        h.update(&self.height.to_le_bytes());
        let mut keys: Vec<_> = self.commitments.keys().cloned().collect();
        keys.sort();
        for k in &keys {
            h.update(k.0.as_bytes());
            h.update(&k.1.to_le_bytes());
            h.update(&self.commitments[k]);
        }
        let out = h.finalize();
        self.state_root.copy_from_slice(&out);
    }

    fn log(&mut self, msg: &str) {
        self.events.push(format!("[{}@{}] {}", self.chain_id, self.height, msg));
    }

    /// Block header hash (relayer submits this to counterparty light client)
    pub fn header_hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(self.chain_id.as_bytes());
        h.update(&self.height.to_le_bytes());
        h.update(&self.state_root);
        let out = h.finalize();
        let mut r = [0u8; 32];
        r.copy_from_slice(&out);
        r
    }

    // ── Light Client ──────────────────────────────────────────────────────────

    pub fn create_client(&mut self, client_id: &str, counterparty_chain_id: &str, height: u64, hash: [u8; 32]) {
        let client = ClientState::new(client_id, counterparty_chain_id, height, hash);
        self.clients.insert(client_id.to_string(), client);
        self.log(&format!("CreateClient {} tracking {} @ height {}", client_id, counterparty_chain_id, height));
    }

    pub fn update_client(&mut self, client_id: &str, height: u64, hash: [u8; 32]) -> Result<(), String> {
        let client = self.clients.get_mut(client_id).ok_or("Client not found")?;
        client.update(height, hash)?;
        self.log(&format!("UpdateClient {} → height {} hash={}...", client_id, height, &hex::encode(&hash[..4])));
        Ok(())
    }

    // ── Connection Handshake ──────────────────────────────────────────────────

    pub fn conn_open_init(&mut self, client_id: &str, counterparty_chain_id: &str) -> Result<String, String> {
        if !self.clients.contains_key(client_id) {
            return Err(format!("Client {} not found", client_id));
        }
        let conn_id = format!("connection-{}", self.conn_counter);
        self.conn_counter += 1;
        let conn = Connection {
            id:                    conn_id.clone(),
            client_id:             client_id.to_string(),
            counterparty_chain_id: counterparty_chain_id.to_string(),
            counterparty_conn_id:  None,
            state:                 ConnState::Init,
            version:               IBC_VERSION.to_string(),
        };
        self.log(&format!("ConnOpenInit: {} client={} counterparty_chain={}", conn_id, client_id, counterparty_chain_id));
        self.connections.insert(conn_id.clone(), conn);
        Ok(conn_id)
    }

    pub fn conn_open_try(&mut self, client_id: &str, counterparty_chain_id: &str, counterparty_conn_id: &str) -> Result<String, String> {
        if !self.clients.contains_key(client_id) {
            return Err(format!("Client {} not found", client_id));
        }
        let conn_id = format!("connection-{}", self.conn_counter);
        self.conn_counter += 1;
        let conn = Connection {
            id:                    conn_id.clone(),
            client_id:             client_id.to_string(),
            counterparty_chain_id: counterparty_chain_id.to_string(),
            counterparty_conn_id:  Some(counterparty_conn_id.to_string()),
            state:                 ConnState::TryOpen,
            version:               IBC_VERSION.to_string(),
        };
        self.log(&format!("ConnOpenTry: {} client={} counterparty={}/{}", conn_id, client_id, counterparty_chain_id, counterparty_conn_id));
        self.connections.insert(conn_id.clone(), conn);
        Ok(conn_id)
    }

    pub fn conn_open_ack(&mut self, conn_id: &str, counterparty_conn_id: &str) -> Result<(), String> {
        let conn = self.connections.get_mut(conn_id).ok_or("Connection not found")?;
        if conn.state != ConnState::Init {
            return Err(format!("Expected INIT, got {}", conn.state.label()));
        }
        conn.counterparty_conn_id = Some(counterparty_conn_id.to_string());
        conn.state = ConnState::Open;
        self.log(&format!("ConnOpenAck: {} → OPEN (counterparty={})", conn_id, counterparty_conn_id));
        Ok(())
    }

    pub fn conn_open_confirm(&mut self, conn_id: &str) -> Result<(), String> {
        let conn = self.connections.get_mut(conn_id).ok_or("Connection not found")?;
        if conn.state != ConnState::TryOpen {
            return Err(format!("Expected TRYOPEN, got {}", conn.state.label()));
        }
        conn.state = ConnState::Open;
        self.log(&format!("ConnOpenConfirm: {} → OPEN", conn_id));
        Ok(())
    }

    // ── Channel Handshake ─────────────────────────────────────────────────────

    pub fn chan_open_init(&mut self, port: &str, conn_id: &str, counterparty_port: &str, ordering: Ordering) -> Result<String, String> {
        if self.connections.get(conn_id).map(|c| &c.state) != Some(&ConnState::Open) {
            return Err(format!("Connection {} not OPEN", conn_id));
        }
        let chan_id = format!("channel-{}", self.chan_counter);
        self.chan_counter += 1;
        let chan = Channel {
            id:                      chan_id.clone(),
            port:                    port.to_string(),
            connection_id:           conn_id.to_string(),
            counterparty_channel_id: None,
            counterparty_port:       counterparty_port.to_string(),
            ordering:                ordering.clone(),
            state:                   ChanState::Init,
            next_seq_send:           1,
            next_seq_recv:           1,
        };
        self.log(&format!("ChanOpenInit: {} port={} conn={} ordering={}", chan_id, port, conn_id, ordering.label()));
        self.channels.insert(chan_id.clone(), chan);
        Ok(chan_id)
    }

    pub fn chan_open_try(&mut self, port: &str, conn_id: &str, counterparty_port: &str, counterparty_chan_id: &str, ordering: Ordering) -> Result<String, String> {
        if self.connections.get(conn_id).map(|c| &c.state) != Some(&ConnState::Open) {
            return Err(format!("Connection {} not OPEN", conn_id));
        }
        let chan_id = format!("channel-{}", self.chan_counter);
        self.chan_counter += 1;
        let chan = Channel {
            id:                      chan_id.clone(),
            port:                    port.to_string(),
            connection_id:           conn_id.to_string(),
            counterparty_channel_id: Some(counterparty_chan_id.to_string()),
            counterparty_port:       counterparty_port.to_string(),
            ordering:                ordering.clone(),
            state:                   ChanState::TryOpen,
            next_seq_send:           1,
            next_seq_recv:           1,
        };
        self.log(&format!("ChanOpenTry: {} port={} counterparty={}/{} ordering={}", chan_id, port, counterparty_port, counterparty_chan_id, ordering.label()));
        self.channels.insert(chan_id.clone(), chan);
        Ok(chan_id)
    }

    pub fn chan_open_ack(&mut self, chan_id: &str, counterparty_chan_id: &str) -> Result<(), String> {
        let chan = self.channels.get_mut(chan_id).ok_or("Channel not found")?;
        if chan.state != ChanState::Init {
            return Err(format!("Expected INIT, got {}", chan.state.label()));
        }
        chan.counterparty_channel_id = Some(counterparty_chan_id.to_string());
        chan.state = ChanState::Open;
        self.log(&format!("ChanOpenAck: {} → OPEN (counterparty={})", chan_id, counterparty_chan_id));
        Ok(())
    }

    pub fn chan_open_confirm(&mut self, chan_id: &str) -> Result<(), String> {
        let chan = self.channels.get_mut(chan_id).ok_or("Channel not found")?;
        if chan.state != ChanState::TryOpen {
            return Err(format!("Expected TRYOPEN, got {}", chan.state.label()));
        }
        chan.state = ChanState::Open;
        self.log(&format!("ChanOpenConfirm: {} → OPEN", chan_id));
        Ok(())
    }

    // ── Packet Operations ─────────────────────────────────────────────────────

    /// Commit a packet on the source chain
    pub fn send_packet(&mut self, chan_id: &str, data: Vec<u8>, timeout_height: u64) -> Result<Packet, String> {
        let (seq, dst_port, dst_channel, src_port) = {
            let chan = self.channels.get_mut(chan_id).ok_or("Channel not found")?;
            if chan.state != ChanState::Open {
                return Err(format!("Channel {} not OPEN", chan_id));
            }
            let seq = chan.next_seq_send;
            chan.next_seq_send += 1;
            (seq, chan.counterparty_port.clone(), chan.counterparty_channel_id.clone().unwrap_or_default(), chan.port.clone())
        };

        let packet = Packet {
            sequence:       seq,
            src_port,
            src_channel:    chan_id.to_string(),
            dst_port,
            dst_channel,
            data:           data.clone(),
            timeout_height,
        };

        let commitment = packet.commitment();
        self.commitments.insert((chan_id.to_string(), seq), commitment);
        self.recompute_state_root();
        self.log(&format!("SendPacket: {}/seq={} data={}b timeout=@{}", chan_id, seq, data.len(), timeout_height));
        Ok(packet)
    }

    /// Receive a packet on the destination chain (called by relayer with proof)
    pub fn recv_packet(&mut self, packet: &Packet, proof: &[u8; 32]) -> Result<Vec<u8>, String> {
        let chan_id = packet.dst_channel.clone();

        if self.height >= packet.timeout_height {
            return Err(format!("Packet timed out: chain height {} ≥ timeout {}", self.height, packet.timeout_height));
        }
        if self.channels.get(&chan_id).map(|c| &c.state) != Some(&ChanState::Open) {
            return Err(format!("Channel {} not OPEN", chan_id));
        }
        if self.receipts.get(&(chan_id.clone(), packet.sequence)).copied().unwrap_or(false) {
            return Err(format!("Packet {}/{} already received", chan_id, packet.sequence));
        }
        // Verify commitment proof: relayer provides commitment hash, we verify it matches
        if packet.commitment() != *proof {
            return Err("Commitment proof mismatch".to_string());
        }

        // Process: produce acknowledgement ("ack:" prefix as simplified app handler)
        let ack = [b"ack:".to_vec(), packet.data.clone()].concat();
        self.receipts.insert((chan_id.clone(), packet.sequence), true);
        self.acks.insert((chan_id.clone(), packet.sequence), ack.clone());
        self.recompute_state_root();
        self.log(&format!("RecvPacket: {}/seq={} ack={}b", chan_id, packet.sequence, ack.len()));
        Ok(ack)
    }

    /// Clear commitment on source chain once ack is received (called by relayer)
    pub fn ack_packet(&mut self, chan_id: &str, seq: u64, ack: &[u8]) -> Result<(), String> {
        if !self.commitments.contains_key(&(chan_id.to_string(), seq)) {
            return Err(format!("No commitment for {}/{}", chan_id, seq));
        }
        self.commitments.remove(&(chan_id.to_string(), seq));
        self.recompute_state_root();
        self.log(&format!("AckPacket: {}/seq={} ack={}b → commitment cleared", chan_id, seq, ack.len()));
        Ok(())
    }

    /// Clear commitment on source chain when packet has timed out on destination
    pub fn timeout_packet(&mut self, packet: &Packet) -> Result<(), String> {
        let key = (packet.src_channel.clone(), packet.sequence);
        if !self.commitments.contains_key(&key) {
            return Err(format!("No commitment for {}/{}", packet.src_channel, packet.sequence));
        }
        self.commitments.remove(&key);
        self.recompute_state_root();
        self.log(&format!("TimeoutPacket: {}/seq={} → commitment cleared (app may refund)", packet.src_channel, packet.sequence));
        Ok(())
    }

    pub fn print_events_since(&self, from: usize) {
        for e in &self.events[from..] {
            println!("  {}", e);
        }
    }
}

// ─── Relayer ──────────────────────────────────────────────────────────────────

/// Off-chain process: watches both chains, relays packets and proofs
pub struct Relayer {
    pub chain_a:        IbcChain,
    pub chain_b:        IbcChain,
    pub relayed_pkts:   u64,
    pub relayed_acks:   u64,
    pub relayed_hdrs:   u64,
}

impl Relayer {
    pub fn new(chain_a: IbcChain, chain_b: IbcChain) -> Self {
        Relayer { chain_a, chain_b, relayed_pkts: 0, relayed_acks: 0, relayed_hdrs: 0 }
    }

    /// Submit chain_a's latest header to chain_b's light client
    pub fn update_b_client(&mut self, client_id_on_b: &str) {
        let height = self.chain_a.height;
        let hash   = self.chain_a.header_hash();
        let _ = self.chain_b.update_client(client_id_on_b, height, hash);
        self.relayed_hdrs += 1;
    }

    /// Submit chain_b's latest header to chain_a's light client
    pub fn update_a_client(&mut self, client_id_on_a: &str) {
        let height = self.chain_b.height;
        let hash   = self.chain_b.header_hash();
        let _ = self.chain_a.update_client(client_id_on_a, height, hash);
        self.relayed_hdrs += 1;
    }

    /// Full 4-way connection handshake
    pub fn connection_handshake(&mut self, client_id_on_a: &str, client_id_on_b: &str) -> Result<(String, String), String> {
        let chain_b_id = self.chain_b.chain_id.clone();
        let chain_a_id = self.chain_a.chain_id.clone();
        let conn_a = self.chain_a.conn_open_init(client_id_on_a, &chain_b_id)?;
        let conn_b = self.chain_b.conn_open_try(client_id_on_b, &chain_a_id, &conn_a)?;
        self.chain_a.conn_open_ack(&conn_a, &conn_b)?;
        self.chain_b.conn_open_confirm(&conn_b)?;
        Ok((conn_a, conn_b))
    }

    /// Full 4-way channel handshake
    pub fn channel_handshake(&mut self, port_a: &str, port_b: &str, conn_a: &str, conn_b: &str, ordering: Ordering) -> Result<(String, String), String> {
        let chan_a = self.chain_a.chan_open_init(port_a, conn_a, port_b, ordering.clone())?;
        let chan_b = self.chain_b.chan_open_try(port_b, conn_b, port_a, &chan_a, ordering)?;
        self.chain_a.chan_open_ack(&chan_a, &chan_b)?;
        self.chain_b.chan_open_confirm(&chan_b)?;
        Ok((chan_a, chan_b))
    }

    /// Relay packet from chain_a to chain_b (proof = commitment hash)
    pub fn relay_packet_a_to_b(&mut self, packet: &Packet) -> Result<Vec<u8>, String> {
        let proof = packet.commitment();
        let ack = self.chain_b.recv_packet(packet, &proof)?;
        self.relayed_pkts += 1;
        Ok(ack)
    }

    /// Relay ack from chain_b back to chain_a
    pub fn relay_ack_to_a(&mut self, chan_id: &str, seq: u64, ack: &[u8]) -> Result<(), String> {
        self.chain_a.ack_packet(chan_id, seq, ack)?;
        self.relayed_acks += 1;
        Ok(())
    }
}

// ─── Transfer App (ICS-20 simplified) ────────────────────────────────────────

/// Token transfer packet data (ICS-20 style)
pub struct TransferPacketData {
    pub denom:    String,
    pub amount:   u64,
    pub sender:   String,
    pub receiver: String,
}

impl TransferPacketData {
    pub fn encode(&self) -> Vec<u8> {
        format!("transfer:{}:{}:{}:{}", self.denom, self.amount, self.sender, self.receiver)
            .into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(bytes).ok()?;
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 5 && parts[0] == "transfer" {
            Some(TransferPacketData {
                denom:    parts[1].to_string(),
                amount:   parts[2].parse().ok()?,
                sender:   parts[3].to_string(),
                receiver: parts[4].to_string(),
            })
        } else {
            None
        }
    }
}
