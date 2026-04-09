#![allow(dead_code)]

/// v5.8 — Peer Discovery
///
/// Fix Known Gap: "Peer discovery tự động"
///
/// Ba thành phần:
///
/// 1. PeerStore — lưu/load danh sách peers vào `~/.pkt/peers.txt`
///    - load() → Vec<String>           (mỗi dòng = "host:port")
///    - save(peers)                    (overwrite)
///    - add(addr)                      (append nếu chưa có)
///    - remove(addr)                   (xóa khỏi file)
///
/// 2. DnsSeedResolver — resolve DNS seeds → IP:port strings
///    - resolve() → Vec<String>        (dùng std::net::ToSocketAddrs, no extra deps)
///    - Silently skip seeds không resolve được
///
/// 3. PeerDiscovery — combine store + DNS, PEX (GetPeers/Peers)
///    - bootstrap() → Vec<String>      (stored + dns, deduped)
///    - record_peer(addr)              (persist newly discovered peer)
///    - remove_peer(addr)              (remove bad/offline peer)
///    - pex_query(addr) → Vec<String>  (TCP GetPeers → Peers response)
///
/// CLI integration:
///   run_node() tự động gọi PeerDiscovery::bootstrap() để connect initial peers
///   Sau khi nhận Peers message từ peer → record_peer() từng addr

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;

use crate::message::Message;

// ─── PeerStore ────────────────────────────────────────────────────────────────

/// Lưu trữ danh sách peers đã biết vào file văn bản.
/// Format: mỗi dòng = "host:port"  (vd: "1.2.3.4:8333" hoặc "node.example.com:8333")
pub struct PeerStore {
    path: PathBuf,
}

impl PeerStore {
    /// Tạo PeerStore với đường dẫn mặc định `~/.pkt/peers.txt`
    pub fn default_path() -> Self {
        let path = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".pkt")
            .join("peers.txt");
        Self { path }
    }

    /// Tạo PeerStore với đường dẫn tùy chỉnh (dùng cho tests)
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Load danh sách peers từ file. Trả về Vec rỗng nếu file chưa tồn tại.
    pub fn load(&self) -> Vec<String> {
        let Ok(file) = std::fs::File::open(&self.path) else {
            return Vec::new();
        };
        BufReader::new(file)
            .lines()
            .filter_map(|l| l.ok())
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    }

    /// Lưu danh sách peers vào file (overwrite).
    pub fn save(&self, peers: &[String]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = std::fs::File::create(&self.path).map_err(|e| e.to_string())?;
        for peer in peers {
            writeln!(file, "{}", peer).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Thêm peer vào store nếu chưa tồn tại (idempotent).
    pub fn add(&self, addr: &str) -> Result<(), String> {
        let mut peers = self.load();
        if !peers.iter().any(|p| p == addr) {
            peers.push(addr.to_string());
            self.save(&peers)?;
        }
        Ok(())
    }

    /// Xóa peer khỏi store.
    pub fn remove(&self, addr: &str) -> Result<(), String> {
        let peers: Vec<String> = self.load()
            .into_iter()
            .filter(|p| p != addr)
            .collect();
        self.save(&peers)
    }

    /// Số peers trong store.
    pub fn len(&self) -> usize {
        self.load().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ─── DnsSeedResolver ──────────────────────────────────────────────────────────

/// Resolve danh sách DNS seed hosts → "ip:port" strings.
///
/// Dùng `std::net::ToSocketAddrs` — chỉ cần stdlib, không cần thêm dep.
/// Silently skip hosts không resolve được (offline, NXDOMAIN, v.v.)
pub struct DnsSeedResolver {
    pub seeds: Vec<String>,  // "hostname" (không có port)
    pub port:  u16,
}

impl DnsSeedResolver {
    pub fn new(seeds: &[&str], port: u16) -> Self {
        Self {
            seeds: seeds.iter().map(|s| s.to_string()).collect(),
            port,
        }
    }

    /// Resolve tất cả seeds, trả về Vec<"ip:port">.
    /// Mỗi DNS name có thể resolve thành nhiều IPs (round-robin DNS seed).
    pub fn resolve(&self) -> Vec<String> {
        let mut addrs: Vec<String> = Vec::new();
        for seed in &self.seeds {
            let target = format!("{}:{}", seed, self.port);
            if let Ok(resolved) = target.to_socket_addrs() {
                for addr in resolved {
                    addrs.push(addr.to_string());
                }
            }
            // Silently skip on error (NXDOMAIN, network unreachable, etc.)
        }
        addrs
    }

    /// Kiểm tra xem seed có resolve được không.
    pub fn can_resolve(&self, seed: &str) -> bool {
        let target = format!("{}:{}", seed, self.port);
        target.to_socket_addrs().map(|mut it| it.next().is_some()).unwrap_or(false)
    }
}

// ─── PeerDiscovery ────────────────────────────────────────────────────────────

/// Kết hợp PeerStore + DnsSeedResolver + PEX để bootstrap danh sách peers.
pub struct PeerDiscovery {
    pub store:    PeerStore,
    pub resolver: DnsSeedResolver,
}

impl PeerDiscovery {
    /// Tạo PeerDiscovery với default PeerStore và DNS seeds từ NetworkParams.
    pub fn new(seeds: &[&str], port: u16) -> Self {
        Self {
            store:    PeerStore::default_path(),
            resolver: DnsSeedResolver::new(seeds, port),
        }
    }

    /// Bootstrap: trả về danh sách peers để connect khi khởi động node.
    ///
    /// Ưu tiên: stored peers > DNS seeds
    /// Dedup theo địa chỉ string, giữ thứ tự.
    pub fn bootstrap(&self) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut result: Vec<String>   = Vec::new();

        // 1. Stored peers (đã từng kết nối thành công)
        for peer in self.store.load() {
            if seen.insert(peer.clone()) {
                result.push(peer);
            }
        }

        // 2. DNS seeds (nếu store rỗng hoặc muốn thêm)
        for addr in self.resolver.resolve() {
            if seen.insert(addr.clone()) {
                result.push(addr);
            }
        }

        result
    }

    /// Ghi nhận peer mới phát hiện qua PEX hoặc kết nối trực tiếp.
    pub fn record_peer(&self, addr: &str) {
        let _ = self.store.add(addr);
    }

    /// Xóa peer không còn hoạt động khỏi store.
    pub fn remove_peer(&self, addr: &str) {
        let _ = self.store.remove(addr);
    }

    /// Gửi GetPeers đến một node, nhận Peers response.
    /// Trả về danh sách địa chỉ peers mà node đó biết.
    pub fn pex_query(addr: &str) -> Vec<String> {
        let Ok(mut stream) = TcpStream::connect(addr) else { return Vec::new() };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));

        let msg = Message::GetPeers;
        if stream.write_all(&msg.serialize()).is_err() { return Vec::new(); }

        let mut line = String::new();
        if BufReader::new(stream).read_line(&mut line).is_err() { return Vec::new(); }

        match Message::deserialize(line.trim_end_matches('\n').as_bytes()) {
            Some(Message::Peers { addrs }) => addrs,
            _ => Vec::new(),
        }
    }

    /// Full bootstrap + PEX: query each bootstrap peer for its peer list.
    /// Kết quả là union của stored + dns + tất cả peers thu thập được từ PEX.
    pub fn deep_bootstrap(&self) -> Vec<String> {
        let initial = self.bootstrap();
        let mut seen: HashSet<String> = initial.iter().cloned().collect();
        let mut all  = initial.clone();

        for peer in &initial {
            for discovered in Self::pex_query(peer) {
                if seen.insert(discovered.clone()) {
                    all.push(discovered.clone());
                    // Persist newly discovered peer
                    let _ = self.store.add(&discovered);
                }
            }
        }

        all
    }
}

// ─── Convenience factory ──────────────────────────────────────────────────────

/// Tạo PeerDiscovery từ PktNetworkParams (dùng bootstrap_peers làm DNS seeds).
pub fn from_network(params: &crate::pkt_genesis::PktNetworkParams) -> PeerDiscovery {
    let seeds: Vec<&str> = params.bootstrap_peers.iter()
        .map(|s: &&str| {
            // Strip port — chỉ lấy hostname phần
            s.split(':').next().unwrap_or(s)
        })
        .collect();
    PeerDiscovery::new(&seeds, params.p2p_port)
}

/// Bootstrap peers cho node khởi động: DNS + store, không PEX (nhanh).
pub fn quick_bootstrap(params: &crate::pkt_genesis::PktNetworkParams) -> Vec<String> {
    from_network(params).bootstrap()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_store() -> (PeerStore, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "pkt_peers_test_{}.txt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        (PeerStore::with_path(path.clone()), path)
    }

    #[test]
    fn test_peer_store_empty_on_missing_file() {
        let (store, path) = temp_store();
        assert!(!path.exists());
        assert_eq!(store.load(), Vec::<String>::new());
    }

    #[test]
    fn test_peer_store_save_and_load() {
        let (store, path) = temp_store();
        let peers = vec!["1.2.3.4:8333".to_string(), "5.6.7.8:8333".to_string()];
        store.save(&peers).expect("save ok");
        assert!(path.exists());
        let loaded = store.load();
        assert_eq!(loaded, peers);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_peer_store_add_idempotent() {
        let (store, path) = temp_store();
        store.add("1.2.3.4:8333").expect("add ok");
        store.add("1.2.3.4:8333").expect("add dup ok");
        store.add("9.9.9.9:8333").expect("add 2nd ok");
        let loaded = store.load();
        assert_eq!(loaded.len(), 2, "duplicate không được thêm");
        assert!(loaded.contains(&"1.2.3.4:8333".to_string()));
        assert!(loaded.contains(&"9.9.9.9:8333".to_string()));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_peer_store_remove() {
        let (store, path) = temp_store();
        store.save(&[
            "1.2.3.4:8333".to_string(),
            "5.6.7.8:8333".to_string(),
        ]).expect("save ok");
        store.remove("1.2.3.4:8333").expect("remove ok");
        let loaded = store.load();
        assert_eq!(loaded, vec!["5.6.7.8:8333".to_string()]);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_peer_store_remove_nonexistent_ok() {
        let (store, path) = temp_store();
        store.save(&["1.2.3.4:8333".to_string()]).expect("save ok");
        // Xóa peer không tồn tại không gây lỗi
        store.remove("99.99.99.99:8333").expect("remove nonexistent ok");
        assert_eq!(store.load().len(), 1);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_dns_resolver_empty_seeds_returns_empty() {
        let resolver = DnsSeedResolver::new(&[], 8333);
        assert_eq!(resolver.resolve(), Vec::<String>::new());
    }

    #[test]
    fn test_dns_resolver_invalid_seed_returns_empty() {
        let resolver = DnsSeedResolver::new(&["this.seed.does.not.exist.invalid"], 8333);
        // Không panic — trả về empty
        let addrs = resolver.resolve();
        assert!(addrs.is_empty(), "invalid DNS seed nên trả về empty, không panic");
    }

    #[test]
    fn test_peer_discovery_bootstrap_from_store() {
        let (store, path) = temp_store();
        store.save(&[
            "10.0.0.1:8333".to_string(),
            "10.0.0.2:8333".to_string(),
        ]).expect("save ok");

        let disc = PeerDiscovery {
            store,
            resolver: DnsSeedResolver::new(&[], 8333), // no DNS
        };
        let peers = disc.bootstrap();
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&"10.0.0.1:8333".to_string()));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_peer_discovery_bootstrap_dedup() {
        let (store, path) = temp_store();
        store.save(&["10.0.0.1:8333".to_string()]).expect("save ok");

        let disc = PeerDiscovery {
            store,
            resolver: DnsSeedResolver::new(&[], 8333),
        };
        let peers = disc.bootstrap();
        // Không có duplicate dù store và dns có thể overlap
        let unique: HashSet<_> = peers.iter().collect();
        assert_eq!(peers.len(), unique.len(), "bootstrap kết quả không có duplicate");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_peer_discovery_record_and_remove() {
        let (store, path) = temp_store();
        let disc = PeerDiscovery {
            store,
            resolver: DnsSeedResolver::new(&[], 8333),
        };
        disc.record_peer("192.168.1.1:8333");
        disc.record_peer("192.168.1.2:8333");
        assert_eq!(disc.store.load().len(), 2);

        disc.remove_peer("192.168.1.1:8333");
        assert_eq!(disc.store.load().len(), 1);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_pex_query_offline_returns_empty() {
        // Kết nối đến port không có ai listen → trả về empty, không panic
        let result = PeerDiscovery::pex_query("127.0.0.1:19999");
        assert_eq!(result, Vec::<String>::new());
    }
}
