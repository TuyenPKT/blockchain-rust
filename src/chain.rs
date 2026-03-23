use crate::block::Block;
use crate::transaction::{Transaction, TxOutput};
use crate::utxo::UtxoSet;
use crate::wallet::Wallet;
use crate::mempool::Mempool;
use crate::script::{Script, ScriptInterpreter};
use crate::taproot::{TaprootOutput, schnorr_sign, schnorr_verify};
use crate::covenant::{CtvTemplate, ctv_template_hash};
use crate::fee_market::FeeEstimator;
use crate::token::TokenRegistry;
use crate::token_tx::extract_token_txs;
use crate::staking::StakingPool;
use crate::reward::RewardEngine;

const DIFFICULTY_ADJUSTMENT_INTERVAL: u64 = 5;
const BLOCK_TIME_TARGET_SECS: i64 = 60;  // 1 phút/block — phù hợp hashrate ~4 MH/s
const MAX_DIFFICULTY: usize = 8;          // diff=9 cần ~286 phút ở 4 MH/s → bất khả thi
const MAX_BLOCK_TX: usize = 100;

pub struct Blockchain {
    pub chain:           Vec<Block>,
    pub difficulty:      usize,
    pub utxo_set:        UtxoSet,
    pub mempool:         Mempool,
    pub fee_estimator:   FeeEstimator,
    /// v10.4 — Token balances updated on every block via OP_RETURN token TXs.
    pub token_registry:  TokenRegistry,
    /// v10.5 — Staking pool: distribute rewards to delegators each block.
    pub staking_pool:    StakingPool,
}

impl Blockchain {
    pub fn new() -> Self {
        let mut genesis = Block::new(
            0, vec![],
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        );
        genesis.hash = Block::calculate_hash(0, genesis.timestamp, &vec![], &genesis.prev_hash, 0);
        Blockchain {
            chain: vec![genesis], difficulty: 3,
            utxo_set: UtxoSet::new(), mempool: Mempool::new(),
            fee_estimator: FeeEstimator::new(),
            token_registry: TokenRegistry::new(),
            staking_pool: StakingPool::new(),
        }
    }

    #[allow(dead_code)] pub fn create_and_submit(
        &mut self,
        wallet:         &Wallet,
        to_pubkey_hash: &str,
        amount:         u64,
        fee:            u64,
    ) -> Result<String, String> {
        let my_hash = hex::encode(Script::pubkey_hash(&wallet.public_key.serialize()));
        let balance  = self.utxo_set.balance_of(&my_hash);
        let needed   = amount + fee;
        if balance < needed {
            return Err(format!("❌ Không đủ tiền: có {} sat, cần {} sat", balance, needed));
        }

        let utxos = self.utxo_set.utxos_of(&my_hash);
        let mut inputs_raw = vec![];
        let mut collected  = 0u64;
        for utxo in &utxos {
            inputs_raw.push((utxo.tx_id.clone(), utxo.output_index));
            collected += utxo.output.amount;
            if collected >= needed { break; }
        }

        let mut outputs = vec![TxOutput::p2pkh(amount, to_pubkey_hash)];
        let change = collected - amount - fee;
        if change > 0 { outputs.push(TxOutput::p2pkh(change, &my_hash)); }

        let mut tx       = Transaction::new_unsigned(inputs_raw, outputs, fee);
        let signing_data = tx.signing_data();
        let sig_hex      = wallet.sign(&signing_data);
        let pubkey_hex   = wallet.public_key_hex();
        for input in &mut tx.inputs {
            input.script_sig = Script::p2pkh_sig(&sig_hex, &pubkey_hex);
        }
        tx.tx_id = tx.calculate_id();

        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, collected)?;
        Ok(tx_id)
    }

    /// Gửi tiền đến P2SH address (ví dụ: multisig)
    /// to_script_hash = HASH160(redeemScript) hex
    #[allow(dead_code)]
    pub fn send_to_p2sh(
        &mut self,
        sender:         &Wallet,
        to_script_hash: &str, // 20-byte hex
        amount:         u64,
        fee:            u64,
    ) -> Result<String, String> {
        let my_hash = hex::encode(Script::pubkey_hash(&sender.public_key.serialize()));
        let balance  = self.utxo_set.balance_of(&my_hash);
        let needed   = amount + fee;
        if balance < needed {
            return Err(format!("❌ Không đủ tiền: có {} sat, cần {} sat", balance, needed));
        }

        let utxos = self.utxo_set.utxos_of(&my_hash);
        let mut inputs_raw = vec![];
        let mut collected  = 0u64;
        for utxo in &utxos {
            inputs_raw.push((utxo.tx_id.clone(), utxo.output_index));
            collected += utxo.output.amount;
            if collected >= needed { break; }
        }

        // Output: P2SH (không phải P2PKH)
        let mut outputs = vec![TxOutput::p2sh(amount, to_script_hash)];
        let change = collected - amount - fee;
        if change > 0 { outputs.push(TxOutput::p2pkh(change, &my_hash)); }

        let mut tx       = Transaction::new_unsigned(inputs_raw, outputs, fee);
        let signing_data = tx.signing_data();
        let sig_hex      = sender.sign(&signing_data);
        let pubkey_hex   = sender.public_key_hex();
        for input in &mut tx.inputs {
            input.script_sig = Script::p2pkh_sig(&sig_hex, &pubkey_hex);
        }
        tx.tx_id = tx.calculate_id();
        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, collected)?;
        Ok(tx_id)
    }

    /// Tiêu UTXO P2SH bằng M-of-N multisig
    /// signers: danh sách ví ký (phải >= M)
    /// redeem_script: script gốc (phải match hash trong UTXO)
    #[allow(dead_code)]
    pub fn spend_p2sh(
        &mut self,
        signers:       &[&Wallet],  // M wallets ký
        redeem_script: &Script,
        utxo_tx_id:    &str,
        utxo_index:    usize,
        to_pubkey_hash: &str,
        amount:         u64,
        fee:            u64,
    ) -> Result<String, String> {
        // Kiểm tra UTXO tồn tại
        let utxo_amount = self.utxo_set.get_amount(utxo_tx_id, utxo_index)
            .ok_or("❌ UTXO không tồn tại")?;
        if utxo_amount < amount + fee {
            return Err(format!("❌ UTXO chỉ có {} sat", utxo_amount));
        }

        let outputs = vec![TxOutput::p2pkh(amount, to_pubkey_hash)];
        let change  = utxo_amount - amount - fee;
        let mut all_outputs = outputs;
        if change > 0 {
            // change về lại P2SH cùng script (để demo đơn giản)
            let rs_hash = hex::encode(Script::script_hash(&redeem_script.to_bytes()));
            all_outputs.push(TxOutput::p2sh(change, &rs_hash));
        }

        let mut tx       = Transaction::new_unsigned(
            vec![(utxo_tx_id.to_string(), utxo_index)],
            all_outputs,
            fee,
        );
        let signing_data = tx.signing_data();

        // Mỗi signer ký cùng 1 signing_data
        let sig_hexes: Vec<String> = signers.iter()
            .map(|w| w.sign(&signing_data))
            .collect();

        // scriptSig P2SH: OP_0 <sig1> ... <sigM> <redeemScript>
        let script_sig = Script::p2sh_sig(&sig_hexes, redeem_script);
        for input in &mut tx.inputs {
            input.script_sig = script_sig.clone();
        }
        tx.tx_id = tx.calculate_id();
        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, utxo_amount)?;
        Ok(tx_id)
    }


    /// Gửi tiền đến P2WPKH address ← v1.1
    #[allow(dead_code)]
    pub fn send_to_p2wpkh(
        &mut self,
        sender:         &Wallet,
        to_pubkey_hash: &str, // 20-byte hex
        amount:         u64,
        fee:            u64,
    ) -> Result<String, String> {
        let my_hash = hex::encode(Script::pubkey_hash(&sender.public_key.serialize()));
        let balance  = self.utxo_set.balance_of(&my_hash);
        if balance < amount + fee {
            return Err(format!("❌ Không đủ: {} sat", balance));
        }
        let utxos = self.utxo_set.utxos_of(&my_hash);
        let mut inputs_raw = vec![];
        let mut collected  = 0u64;
        for u in &utxos {
            inputs_raw.push((u.tx_id.clone(), u.output_index));
            collected += u.output.amount;
            if collected >= amount + fee { break; }
        }
        let mut outputs = vec![TxOutput::p2wpkh(amount, to_pubkey_hash)];
        let change = collected - amount - fee;
        if change > 0 { outputs.push(TxOutput::p2pkh(change, &my_hash)); }

        // scriptSig rỗng — dùng legacy key để fund SegWit output
        let mut tx       = Transaction::new_unsigned(inputs_raw, outputs, fee);
        let signing_data = tx.signing_data();
        let sig_hex      = sender.sign(&signing_data);
        let pubkey_hex   = sender.public_key_hex();
        for input in &mut tx.inputs {
            input.script_sig = Script::p2pkh_sig(&sig_hex, &pubkey_hex);
        }
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();

        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, collected)?;
        Ok(tx_id)
    }

    /// Tiêu UTXO P2WPKH — witness thay thế scriptSig ← v1.1
    #[allow(dead_code)]
    pub fn spend_p2wpkh(
        &mut self,
        sender:         &Wallet,
        utxo_tx_id:     &str,
        utxo_index:     usize,
        to_pubkey_hash: &str,
        amount:         u64,
        fee:            u64,
    ) -> Result<String, String> {
        let utxo_amount = self.utxo_set.get_amount(utxo_tx_id, utxo_index)
            .ok_or("❌ UTXO không tồn tại")?;
        if utxo_amount < amount + fee {
            return Err(format!("❌ UTXO chỉ có {} sat", utxo_amount));
        }
        let my_hash = hex::encode(Script::pubkey_hash(&sender.public_key.serialize()));
        let change  = utxo_amount - amount - fee;
        let mut outputs = vec![TxOutput::p2pkh(amount, to_pubkey_hash)];
        if change > 0 { outputs.push(TxOutput::p2wpkh(change, &my_hash)); }

        let mut tx = Transaction::new_unsigned(
            vec![(utxo_tx_id.to_string(), utxo_index)],
            outputs, fee,
        );

        // BIP143: signing_data bao gồm input_amount
        let sig_data   = tx.segwit_signing_data(0, utxo_amount);
        let sig_bytes  = hex::decode(sender.sign(&sig_data)).unwrap_or_default();
        let pub_bytes  = sender.public_key.serialize().to_vec();

        // witness = [sig_bytes, pubkey_bytes] — KHÔNG có scriptSig
        for input in &mut tx.inputs {
            input.script_sig = Script::empty();
            input.witness    = vec![sig_bytes.clone(), pub_bytes.clone()];
        }
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();

        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, utxo_amount)?;
        Ok(tx_id)
    }


    /// Tạo funding TX cho Lightning channel ← v1.2
    /// Output: 2-of-2 P2WSH multisig (local_pubkey + remote_pubkey)
    /// Returns: (tx_id, output_index)
    #[allow(dead_code)]
    pub fn open_lightning_channel(
        &mut self,
        local_wallet:    &Wallet,
        remote_pubkey_hex: &str,
        capacity:        u64,
        fee:             u64,
    ) -> Result<(String, usize), String> {
        let my_hash = hex::encode(Script::pubkey_hash(&local_wallet.public_key.serialize()));
        let balance  = self.utxo_set.balance_of(&my_hash);
        if balance < capacity + fee {
            return Err(format!("❌ Không đủ: {} sat", balance));
        }

        let utxos = self.utxo_set.utxos_of(&my_hash);
        let mut inputs_raw = vec![];
        let mut collected  = 0u64;
        for u in &utxos {
            inputs_raw.push((u.tx_id.clone(), u.output_index));
            collected += u.output.amount;
            if collected >= capacity + fee { break; }
        }

        // Funding output: 2-of-2 P2SH multisig (local + remote)
        let pubkeys = vec![local_wallet.public_key_hex(), remote_pubkey_hex.to_string()];
        let redeem  = Script::multisig_redeem(2, &pubkeys);
        let rs_hash = hex::encode(Script::script_hash(&redeem.to_bytes()));

        let mut outputs = vec![TxOutput::p2sh(capacity, &rs_hash)];
        let change = collected - capacity - fee;
        if change > 0 { outputs.push(TxOutput::p2pkh(change, &my_hash)); }

        let mut tx       = Transaction::new_unsigned(inputs_raw, outputs, fee);
        let signing_data = tx.signing_data();
        let sig_hex      = local_wallet.sign(&signing_data);
        let pubkey_hex   = local_wallet.public_key_hex();
        for input in &mut tx.inputs {
            input.script_sig = Script::p2pkh_sig(&sig_hex, &pubkey_hex);
        }
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();

        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, collected)?;
        Ok((tx_id, 0)) // output index 0 = funding output
    }

    /// Settle Lightning channel on-chain (cooperative close)
    /// Cả 2 bên đồng ý → broadcast closing TX phân chia balance
    #[allow(dead_code)]
    pub fn close_lightning_channel(
        &mut self,
        local_wallet:      &Wallet,
        remote_wallet:     &Wallet, // cả 2 bên ký
        funding_tx_id:     &str,
        funding_index:     usize,
        local_amount:      u64,
        remote_amount:     u64,
        fee:               u64,
    ) -> Result<String, String> {
        let funding_amount = self.utxo_set.get_amount(funding_tx_id, funding_index)
            .ok_or("❌ Funding UTXO không tồn tại")?;

        if local_amount + remote_amount + fee > funding_amount {
            return Err(format!("❌ local+remote+fee > funding: {}+{}+{} > {}",
                local_amount, remote_amount, fee, funding_amount));
        }

        let local_hash  = hex::encode(Script::pubkey_hash(&local_wallet.public_key.serialize()));
        let remote_hash = hex::encode(Script::pubkey_hash(&remote_wallet.public_key.serialize()));

        let mut outputs = vec![];
        if local_amount  > 0 { outputs.push(TxOutput::p2wpkh(local_amount,  &local_hash)); }
        if remote_amount > 0 { outputs.push(TxOutput::p2wpkh(remote_amount, &remote_hash)); }

        // RedeemScript: 2-of-2 multisig (giống lúc open)
        let pubkeys = vec![local_wallet.public_key_hex(), remote_wallet.public_key_hex()];
        let redeem  = Script::multisig_redeem(2, &pubkeys);

        let mut tx = Transaction::new_unsigned(
            vec![(funding_tx_id.to_string(), funding_index)],
            outputs, fee,
        );
        let signing_data = tx.signing_data();

        // Cả 2 bên ký closing TX
        let sigs = vec![local_wallet.sign(&signing_data), remote_wallet.sign(&signing_data)];
        let script_sig = Script::p2sh_sig(&sigs, &redeem);
        for input in &mut tx.inputs {
            input.script_sig = script_sig.clone();
        }
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();

        let tx_id = tx.tx_id.clone();
        let funding_amount = funding_amount;
        self.mempool.add(tx, funding_amount)?;
        Ok(tx_id)
    }


    /// Gửi tiền đến P2TR address (key-path only) ← v1.3
    #[allow(dead_code)]
    pub fn send_to_p2tr(
        &mut self,
        sender:            &Wallet,
        taproot_output:    &TaprootOutput,
        amount:            u64,
        fee:               u64,
    ) -> Result<String, String> {
        let my_hash = hex::encode(Script::pubkey_hash(&sender.public_key.serialize()));
        let balance  = self.utxo_set.balance_of(&my_hash);
        if balance < amount + fee {
            return Err(format!("❌ Không đủ: {} sat", balance));
        }
        let utxos = self.utxo_set.utxos_of(&my_hash);
        let mut inputs_raw = vec![];
        let mut collected  = 0u64;
        for u in &utxos {
            inputs_raw.push((u.tx_id.clone(), u.output_index));
            collected += u.output.amount;
            if collected >= amount + fee { break; }
        }
        let tweaked_hex = hex::encode(taproot_output.output_key_xonly());
        let mut outputs = vec![TxOutput::p2tr(amount, &tweaked_hex)];
        let change = collected - amount - fee;
        if change > 0 { outputs.push(TxOutput::p2pkh(change, &my_hash)); }

        let mut tx = Transaction::new_unsigned(inputs_raw, outputs, fee);
        let sd     = tx.signing_data();
        let sig    = sender.sign(&sd);
        let pk     = sender.public_key_hex();
        for input in &mut tx.inputs {
            input.script_sig = Script::p2pkh_sig(&sig, &pk);
        }
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();
        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, collected)?;
        Ok(tx_id)
    }

    /// Tiêu P2TR UTXO bằng key path (Schnorr signature) ← v1.3
    /// Đây là cách đơn giản nhất — không reveal script tree
    #[allow(dead_code)]
    pub fn spend_p2tr_keypath(
        &mut self,
        sender:            &Wallet,
        taproot_output:    &TaprootOutput,
        utxo_tx_id:        &str,
        utxo_index:        usize,
        to_pubkey_hash:    &str,
        amount:            u64,
        fee:               u64,
    ) -> Result<String, String> {
        let utxo_amount = self.utxo_set.get_amount(utxo_tx_id, utxo_index)
            .ok_or("❌ UTXO không tồn tại")?;
        if utxo_amount < amount + fee {
            return Err(format!("❌ UTXO chỉ có {} sat", utxo_amount));
        }
        let change = utxo_amount - amount - fee;
        let mut outputs = vec![TxOutput::p2pkh(amount, to_pubkey_hash)];
        if change > 0 {
            let tweaked_hex = hex::encode(taproot_output.output_key_xonly());
            outputs.push(TxOutput::p2tr(change, &tweaked_hex));
        }

        let mut tx = Transaction::new_unsigned(
            vec![(utxo_tx_id.to_string(), utxo_index)], outputs, fee,
        );
        // BIP340 sighash: segwit_signing_data bao gồm amount
        let sighash = tx.segwit_signing_data(0, utxo_amount);
        // Phải sign bằng tweaked secret key (Q = P + tweak*G)
        let signing_sk = taproot_output.tweaked_secret.as_ref()
            .unwrap_or(&sender.secret_key);
        let schnorr_sig = schnorr_sign(signing_sk, &sighash);

        for input in &mut tx.inputs {
            input.script_sig = Script::empty();
            input.witness    = vec![schnorr_sig.to_vec()]; // chỉ 1 item: 64-byte Schnorr sig
        }
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();
        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, utxo_amount)?;
        Ok(tx_id)
    }

    /// Mine block — miner nhận reward + fees
    #[allow(dead_code)]
    pub fn mine_block(&mut self, miner_wallet: &Wallet) {
        let miner_hash = hex::encode(Script::pubkey_hash(&miner_wallet.public_key.serialize()));
        self.mine_block_to_hash(&miner_hash);
    }

    /// Mine block — chỉ cần pubkey_hash (dùng cho node.rs)
    pub fn mine_block_to_hash(&mut self, miner_hash: &str) {
        let selected     = self.mempool.select_transactions(MAX_BLOCK_TX);
        let selected_ids: Vec<String> = selected.iter().map(|t| t.tx_id.clone()).collect();

        let mut valid_txs = vec![];
        for tx in selected {
            if self.validate_tx_script(&tx) { valid_txs.push(tx); }
        }

        let total_fee: u64 = valid_txs.iter().map(|t| t.fee).sum();
        let height   = self.chain.last().map(|b| b.index + 1).unwrap_or(0);

        // v10.5 — staking: distribute block subsidy + add reward outputs to coinbase
        let subsidy = RewardEngine::subsidy_at(height);
        let staking_payouts = self.staking_pool.collect_block_rewards(subsidy);
        let mut coinbase = Transaction::coinbase_at(miner_hash, total_fee, height);
        for (addr, amount) in &staking_payouts {
            coinbase.outputs.push(TxOutput::p2pkh(*amount, addr));
        }
        if !staking_payouts.is_empty() {
            coinbase.tx_id  = coinbase.calculate_txid();
            coinbase.wtx_id = coinbase.calculate_wtxid();
        }

        let mut all_txs = vec![coinbase];
        all_txs.extend(valid_txs);

        let prev      = self.chain.last().unwrap();
        let mut block = Block::new(prev.index + 1, all_txs, prev.hash.clone());

        self.adjust_difficulty();
        println!("⛏️  Mining block #{} (difficulty={}, fee={} sat)...",
            block.index, self.difficulty, total_fee);
        let start = std::time::Instant::now();
        block.mine(self.difficulty);
        println!("✅ nonce={} | hash={}... | {}ms\n",
            block.nonce, &block.hash[..12], start.elapsed().as_millis());

        self.utxo_set.apply_block(&block.transactions);
        self.fee_estimator.record_block(&block.transactions);
        self.apply_token_txs(&block.transactions);
        self.chain.push(block);
        self.mempool.remove_confirmed(&selected_ids);
    }

    /// Thêm block từ external (node sync) — không dùng mempool
    #[allow(dead_code)]
    pub fn add_block(&mut self, transactions: Vec<Transaction>, miner_hash: &str) {
        let mut valid_txs = vec![];
        for tx in transactions {
            if tx.is_coinbase { continue; }
            if self.validate_tx_script(&tx) { valid_txs.push(tx); }
        }
        let total_fee: u64 = valid_txs.iter().map(|t| t.fee).sum();
        let height   = self.chain.last().map(|b| b.index + 1).unwrap_or(0);

        // v10.5 — staking: distribute block subsidy + add reward outputs to coinbase
        let subsidy = RewardEngine::subsidy_at(height);
        let staking_payouts = self.staking_pool.collect_block_rewards(subsidy);
        let mut coinbase = Transaction::coinbase_at(miner_hash, total_fee, height);
        for (addr, amount) in &staking_payouts {
            coinbase.outputs.push(TxOutput::p2pkh(*amount, addr));
        }
        if !staking_payouts.is_empty() {
            coinbase.tx_id  = coinbase.calculate_txid();
            coinbase.wtx_id = coinbase.calculate_wtxid();
        }

        let mut all_txs = vec![coinbase];
        all_txs.extend(valid_txs);

        let prev      = self.chain.last().unwrap();
        let mut block = Block::new(prev.index + 1, all_txs, prev.hash.clone());
        self.adjust_difficulty();
        println!("⛏️  Mining block #{} (difficulty={})...", block.index, self.difficulty);
        let start = std::time::Instant::now();
        block.mine(self.difficulty);
        println!("✅ nonce={} | hash={}... | {}ms\n",
            block.nonce, &block.hash[..12], start.elapsed().as_millis());
        self.utxo_set.apply_block(&block.transactions);
        self.fee_estimator.record_block(&block.transactions);
        self.apply_token_txs(&block.transactions);
        self.chain.push(block);
    }

    /// Push một block đã được mine xong (từ parallel miner) vào chain.
    /// Không re-mine — chỉ apply state: UTXO, fee estimator, token txs.
    pub fn commit_mined_block(&mut self, block: crate::block::Block) {
        self.adjust_difficulty();
        self.utxo_set.apply_block(&block.transactions);
        self.fee_estimator.record_block(&block.transactions);
        self.apply_token_txs(&block.transactions);
        self.chain.push(block);
    }

    // ── v10.4 — Token TX processing ───────────────────────────────────────────

    /// Extract OP_RETURN token TXs từ block, validate, và apply vào token_registry.
    /// Invalid TXs (unknown token, insufficient balance) bị skip — không fail block.
    pub fn apply_token_txs(&mut self, txs: &[Transaction]) {
        for ttx in extract_token_txs(txs) {
            // Validate: token exists
            if self.token_registry.tokens.get(&ttx.token_id).is_none() {
                continue;
            }
            // Validate: sender has sufficient balance
            if self.token_registry.balance_of(&ttx.token_id, &ttx.from) < ttx.amount {
                continue;
            }
            // Apply transfer (ignore error — already validated above)
            let _ = self.token_registry.transfer(&ttx.token_id, &ttx.from, &ttx.to, ttx.amount);
        }
    }

    /// v11.0 — public alias for write_api to verify TX signatures before mempool admission
    pub fn verify_tx_scripts(&self, tx: &Transaction) -> bool {
        self.validate_tx_script(tx)
    }

    fn validate_tx_script(&self, tx: &Transaction) -> bool {
        for (idx, input) in tx.inputs.iter().enumerate() {
            if !self.utxo_set.is_unspent(&input.tx_id, input.output_index) {
                println!("⚠️  TX bỏ qua: UTXO đã tiêu"); return false;
            }
            let key = format!("{}:{}", input.tx_id, input.output_index);
            let utxo_output = match self.utxo_set.utxos.get(&key) {
                Some(o) => o, None => return false,
            };

            if utxo_output.script_pubkey.is_ctv() {
                // ← v1.4: CTV — verify spending TX matches template hash
                if !self.validate_ctv(tx, utxo_output) {
                    println!("⚠️  TX bỏ qua: CTV template hash không khớp"); return false;
                }
            } else if utxo_output.script_pubkey.is_p2tr() {
                // ← v1.3: P2TR key path — verify Schnorr signature
                if !self.validate_p2tr_keypath(tx, idx, utxo_output) {
                    println!("⚠️  TX bỏ qua: P2TR Schnorr thất bại"); return false;
                }
            } else if utxo_output.script_pubkey.is_p2wpkh() {
                // ← v1.1: P2WPKH — xác thực witness
                if !self.validate_p2wpkh(tx, idx, utxo_output) {
                    println!("⚠️  TX bỏ qua: P2WPKH witness thất bại"); return false;
                }
            } else {
                // Legacy: P2PKH, P2SH, P2PK
                let signing_data = tx.signing_data();
                let mut interp = ScriptInterpreter::new();
                if !interp.execute(&input.script_sig, &utxo_output.script_pubkey, &signing_data) {
                    println!("⚠️  TX bỏ qua: Script execution thất bại"); return false;
                }
            }
        }
        let total_in: u64 = tx.inputs.iter()
            .filter_map(|i| self.utxo_set.get_amount(&i.tx_id, i.output_index))
            .sum();
        if total_in < tx.total_output() + tx.fee {
            println!("⚠️  TX bỏ qua: input < output + fee"); return false;
        }
        true
    }

    /// Xác thực P2WPKH input:
    ///   witness[0] = sig bytes, witness[1] = pubkey bytes
    ///   hash160(pubkey) phải == hash trong scriptPubKey
    fn validate_p2wpkh(&self, tx: &Transaction, input_index: usize, utxo: &crate::transaction::TxOutput) -> bool {
        use secp256k1::{Secp256k1, PublicKey, Message, ecdsa::Signature};

        let input = &tx.inputs[input_index];
        if input.witness.len() < 2 { return false; }
        let sig_bytes = &input.witness[0];
        let pub_bytes = &input.witness[1];

        // 1. Kiểm tra hash160(pubkey) khớp scriptPubKey
        let pubkey_hash = Script::pubkey_hash(pub_bytes);
        let expected_hash = match utxo.script_pubkey.p2wpkh_hash() {
            Some(h) => h, None => return false,
        };
        if &pubkey_hash != expected_hash { return false; }

        // 2. Lấy amount của UTXO để tạo BIP143 sighash
        let utxo_amount = match self.utxo_set.get_amount(&input.tx_id, input.output_index) {
            Some(a) => a, None => return false,
        };

        // 3. Xác thực ECDSA với BIP143 sighash
        let signing_data = tx.segwit_signing_data(input_index, utxo_amount);
        let secp   = Secp256k1::new();
        let pubkey = match PublicKey::from_slice(pub_bytes)        { Ok(k) => k, Err(_) => return false };
        let hash   = blake3::hash(&signing_data);
        let msg    = match Message::from_slice(hash.as_bytes())    { Ok(m) => m, Err(_) => return false };
        let sig    = match Signature::from_compact(sig_bytes)      { Ok(s) => s, Err(_) => return false };
        secp.verify_ecdsa(&msg, &sig, &pubkey).is_ok()
    }

    /// Xác thực P2TR key path spend (Schnorr)
    fn validate_p2tr_keypath(&self, tx: &Transaction, input_index: usize, utxo: &crate::transaction::TxOutput) -> bool {
        let input = &tx.inputs[input_index];
        if input.witness.is_empty() { return false; }
        let sig_bytes: [u8; 64] = match input.witness[0].as_slice().try_into() {
            Ok(s) => s, Err(_) => return false,
        };
        // Lấy tweaked x-only pubkey từ scriptPubKey
        let xonly_bytes: [u8; 32] = match utxo.script_pubkey.p2tr_xonly() {
            Some(d) => match d.as_slice().try_into() { Ok(b) => b, Err(_) => return false },
            None => return false,
        };
        let utxo_amount = match self.utxo_set.get_amount(&input.tx_id, input.output_index) {
            Some(a) => a, None => return false,
        };
        let sighash = tx.segwit_signing_data(input_index, utxo_amount);
        schnorr_verify(&xonly_bytes, &sighash, &sig_bytes)
    }

    /// Xác thực CTV spend ← v1.4
    /// Tính template hash của spending TX và so sánh với committed hash
    fn validate_ctv(&self, spending_tx: &Transaction, utxo: &crate::transaction::TxOutput) -> bool {
        let expected_hash: [u8; 32] = match utxo.script_pubkey.ctv_template_hash() {
            Some(h) => match h.as_slice().try_into() { Ok(b) => b, Err(_) => return false },
            None    => return false,
        };
        let template = CtvTemplate {
            version:     2,
            locktime:    0,
            sequences:   spending_tx.inputs.iter().map(|i| i.sequence).collect(),
            outputs:     spending_tx.outputs.clone(),
            input_count: spending_tx.inputs.len(),
            input_index: 0,
        };
        ctv_template_hash(&template) == expected_hash
    }

    /// Fund a CTV vault ← v1.4
    /// Gửi tiền vào scriptPubKey commit vào unvault_template_hash
    #[allow(dead_code)]
    pub fn fund_vault(
        &mut self,
        sender:          &Wallet,
        vault_script:    &Script,
        amount:          u64,
        fee:             u64,
    ) -> Result<String, String> {
        let my_hash = hex::encode(Script::pubkey_hash(&sender.public_key.serialize()));
        let balance = self.utxo_set.balance_of(&my_hash);
        if balance < amount + fee {
            return Err(format!("❌ Không đủ: {} sat", balance));
        }
        let utxos     = self.utxo_set.utxos_of(&my_hash);
        let mut collected = 0u64;
        let mut inputs_raw = vec![];
        for u in &utxos {
            inputs_raw.push((u.tx_id.clone(), u.output_index));
            collected += u.output.amount;
            if collected >= amount + fee { break; }
        }
        let vault_out  = crate::transaction::TxOutput {
            amount,
            script_pubkey: vault_script.clone(),
        };
        let mut outputs = vec![vault_out];
        let change = collected - amount - fee;
        if change > 0 { outputs.push(TxOutput::p2pkh(change, &my_hash)); }

        let mut tx = Transaction::new_unsigned(inputs_raw, outputs, fee);
        let sd     = tx.signing_data();
        let sig    = sender.sign(&sd);
        let pk     = sender.public_key_hex();
        for input in &mut tx.inputs {
            input.script_sig = Script::p2pkh_sig(&sig, &pk);
        }
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();
        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, collected)?;
        Ok(tx_id)
    }

    /// Spend một CTV UTXO — TX phải match template hash chính xác ← v1.4
    #[allow(dead_code)]
    pub fn spend_ctv(
        &mut self,
        ctv_tx_id:    &str,
        ctv_index:    usize,
        template:     &CtvTemplate,
    ) -> Result<String, String> {
        let utxo_amount = self.utxo_set.get_amount(ctv_tx_id, ctv_index)
            .ok_or("❌ CTV UTXO không tồn tại")?;

        let mut tx = Transaction::new_unsigned(
            vec![(ctv_tx_id.to_string(), ctv_index)],
            template.outputs.clone(),
            0,
        );
        // CTV không cần scriptSig — chỉ cần TX structure đúng
        tx.inputs[0].sequence = template.sequences[0];
        tx.tx_id  = tx.calculate_txid();
        tx.wtx_id = tx.calculate_wtxid();
        let tx_id = tx.tx_id.clone();
        self.mempool.add(tx, utxo_amount)?;
        Ok(tx_id)
    }

    pub fn adjust_difficulty(&mut self) {
        let len = self.chain.len() as u64;
        if len == 0 || len % DIFFICULTY_ADJUSTMENT_INTERVAL != 0 { return; }
        let last  = self.chain.last().unwrap();
        let first = &self.chain[(len - DIFFICULTY_ADJUSTMENT_INTERVAL) as usize];
        let time_taken    = last.timestamp - first.timestamp;
        let time_expected = BLOCK_TIME_TARGET_SECS * DIFFICULTY_ADJUSTMENT_INTERVAL as i64;
        if time_taken <= 0 {
            // Blocks cực nhanh (< 1s cho toàn interval) — tăng mạnh
            self.difficulty += 2;
            println!("📈📈 Difficulty → {} (blocks quá nhanh)", self.difficulty);
        } else if time_taken < time_expected / 4 {
            // Nhanh hơn target > 4x → tăng 2
            self.difficulty += 2;
            println!("📈📈 Difficulty → {} ({}s < target {}s / 4)", self.difficulty, time_taken, time_expected);
        } else if time_taken < time_expected / 2 {
            // Nhanh hơn target 2–4x → tăng 1
            self.difficulty += 1;
            println!("📈 Difficulty → {} ({}s vs target {}s)", self.difficulty, time_taken, time_expected);
        } else if time_taken > time_expected * 4 && self.difficulty > 1 {
            // Chậm hơn 4x → giảm 2
            self.difficulty -= if self.difficulty > 2 { 2 } else { 1 };
            println!("📉📉 Difficulty → {} ({}s > target {}s x4)", self.difficulty, time_taken, time_expected);
        } else if time_taken > time_expected * 2 && self.difficulty > 1 {
            // Chậm hơn 2x → giảm 1
            self.difficulty -= 1;
            println!("📉 Difficulty → {} ({}s vs target {}s)", self.difficulty, time_taken, time_expected);
        }
        if self.difficulty > MAX_DIFFICULTY {
            self.difficulty = MAX_DIFFICULTY;
        }
    }

    #[allow(dead_code)]
    pub fn last_block(&self) -> &Block { self.chain.last().unwrap() }

    #[allow(dead_code)]
    pub fn balance_of_wallet(&self, wallet: &Wallet) -> u64 {
        let hash = hex::encode(Script::pubkey_hash(&wallet.public_key.serialize()));
        self.utxo_set.balance_of(&hash)
    }

    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        for i in 1..self.chain.len() {
            let current  = &self.chain[i];
            let previous = &self.chain[i - 1];
            if !current.is_valid(self.difficulty) {
                println!("❌ Block {} không hợp lệ!", current.index); return false;
            }
            if current.prev_hash != previous.hash {
                println!("❌ Block {} bị ngắt kết nối!", current.index); return false;
            }
            if !current.has_coinbase() {
                println!("❌ Block {} thiếu coinbase!", current.index); return false;
            }
        }
        true
    }
}
