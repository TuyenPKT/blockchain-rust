//! pkt-api — PKT blockchain REST API server (standalone binary)
//!
//! v19.0: stub — web server chưa được migrate sang crate này.
//! Dùng main binary trong thời gian chờ:
//!
//!   cargo run -p blockchain-rust -- pktscan [port]
//!
//! Roadmap migration (v19.x):
//!   v19.2 — JSON-RPC server sẽ live ở đây
//!   v19.4 — libp2p node sẽ live ở đây

use pkt_sdk::SDK_VERSION;

fn main() {
    println!("pkt-api v{} (pkt-sdk v{})", env!("CARGO_PKG_VERSION"), SDK_VERSION);
    println!();
    println!("Web server chưa được migrate sang crate này.");
    println!("Dùng main binary:");
    println!("  cargo run -p blockchain-rust -- pktscan [port]");
    println!();
    println!("Roadmap:");
    println!("  v19.2 — JSON-RPC server");
    println!("  v19.4 — libp2p P2P node");
}
