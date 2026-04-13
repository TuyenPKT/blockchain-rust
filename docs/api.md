# PKTScan API Reference

> Testnet API — base URL: `https://testnet.oceif.com/blockchain-rust`
> Local dev: `http://127.0.0.1:8081`

## Network Summary

| Method | Path | Mô tả |
|--------|------|-------|
| `GET` | `/api/testnet/summary` | height, hashrate, block_time_avg, difficulty, mempool_count, total_value_pkt, block_reward_pkt, rich_top5 |

## Blocks

| Method | Path | Mô tả |
|--------|------|-------|
| `GET` | `/api/testnet/headers?limit=N` | Recent block headers |
| `GET` | `/api/testnet/block/:height` | Block detail + tx list |

## Transactions

| Method | Path | Mô tả |
|--------|------|-------|
| `GET` | `/api/testnet/txs?limit=N&cursor=H` | Recent transactions |
| `GET` | `/api/testnet/tx/:txid` | TX detail: inputs, outputs, fee, size, confirmations |
| `GET` | `/api/testnet/mempool?limit=N` | Mempool transactions |
| `POST` | `/api/testnet/tx/broadcast` | Broadcast signed TX (raw hex) |

## Addresses

| Method | Path | Mô tả |
|--------|------|-------|
| `GET` | `/api/testnet/addr/:address?limit=N` | Balance + tx history (supports EVM 0x, bech32, Base58) |
| `GET` | `/api/testnet/balance/:address` | Balance only |
| `GET` | `/api/testnet/address/:address/txs?page=N&limit=N` | TX history paginated |
| `GET` | `/api/testnet/address/:address/utxos` | UTXO list |
| `GET` | `/api/testnet/utxos/:script_hex` | UTXOs by script hex |

## Rich List

| Method | Path | Mô tả |
|--------|------|-------|
| `GET` | `/api/testnet/richlist?limit=N` | Top holders by balance |
| `GET` | `/api/testnet/rich-list?limit=N` | Alias |

## Analytics

| Method | Path | Mô tả |
|--------|------|-------|
| `GET` | `/api/testnet/analytics?metric=hashrate\|block_time\|difficulty&window=N` | Time-series data |

## Search

| Method | Path | Mô tả |
|--------|------|-------|
| `GET` | `/api/testnet/search?q=<hash\|height\|addr>` | Auto-detect: block/tx/address |

## Labels

| Method | Path | Mô tả |
|--------|------|-------|
| `GET` | `/api/testnet/labels` | Known address labels (miners, exchanges) |

## Health

| Method | Path | Mô tả |
|--------|------|-------|
| `GET` | `/api/health` | `{"status":"ok","height":N}` |

---

## Address formats hỗ trợ

| Format | Ví dụ | Ghi chú |
|--------|-------|---------|
| EVM (EIP-55) | `0x5c70c728Ad6AD7...` | PKT testnet default |
| bech32 testnet | `tpkt1q...` | |
| bech32 mainnet | `pkt1q...` | |
| Base58Check | `p7LMkZBs...` | Legacy |
| Script hex | `76a914...88ac` | Raw P2PKH |

---

## Response fields quan trọng

### `/api/testnet/summary`
```json
{
  "height": 77,
  "hashrate": 1234567,
  "block_time_avg": 60.5,
  "difficulty": 0.001234,
  "mempool_count": 3,
  "total_value_pkt": 3492.0,
  "block_reward": 49836605440,
  "block_reward_pkt": 46.44,
  "rich_top5": [...]
}
```

### `/api/testnet/addr/:address`
```json
{
  "address": "0x5c70c728...",
  "balance": 8700000000000,
  "txs": [{"height": 2, "txid": "5bbe7b..."}],
  "count": 5
}
```

### `/api/testnet/tx/:txid`
```json
{
  "txid": "...",
  "height": 2,
  "timestamp": 1775528900,
  "fee": 0,
  "size": 256,
  "confirmations": 75,
  "inputs": [{"txid": "...", "vout": 0, "address": "0x...", "value": 1073741824}],
  "outputs": [{"address": "0x...", "value": 1073741824, "type": "P2PKH"}]
}
```

> **Note:** `value` luôn là **paklets** (1 PKT = 2^30 = 1,073,741,824 paklets)

---

*Tổng: ~15 endpoints · Cập nhật lần cuối: v24.0.9.11*
