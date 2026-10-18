//! Compatibility port of cothink-system's custom crypto helpers.
//! The KDF and deterministic salt mode are legacy algorithms, not recommendations
//! for new protocols. SimplifiedZkProof is not a real zero-knowledge proof.
use aes::{
    Aes256,
    cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray},
};
use hmac::{Hmac, Mac};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use sha3::{Sha3_256, Sha3_512};
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};
type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HashAlgorithm {
    #[default]
    Sha256,
    Sha512,
    Sha3_256,
    Sha3_512,
}
impl HashAlgorithm {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
            Self::Sha3_256 => "sha3_256",
            Self::Sha3_512 => "sha3_512",
        }
    }
    fn digest(self, d: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha256 => Sha256::digest(d).to_vec(),
            Self::Sha512 => Sha512::digest(d).to_vec(),
            Self::Sha3_256 => Sha3_256::digest(d).to_vec(),
            Self::Sha3_512 => Sha3_512::digest(d).to_vec(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct EncryptionConfig {
    pub hash_algo: HashAlgorithm,
    pub fold_depth: usize,
    pub kdf_iterations: u32,
    pub cipher_key_size: usize,
    pub auth_key_size: usize,
    pub salt_size: usize,
    pub version: u8,
}
impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            hash_algo: HashAlgorithm::Sha256,
            fold_depth: 5,
            kdf_iterations: 100_000,
            cipher_key_size: 32,
            auth_key_size: 32,
            salt_size: 16,
            version: 1,
        }
    }
}

pub struct InFoldingRhash {
    pub config: EncryptionConfig,
}
impl InFoldingRhash {
    pub fn new(config: EncryptionConfig) -> Self {
        Self { config }
    }
    fn constant(&self, idx: usize, len: usize) -> [u8; 8] {
        let mut d = Vec::with_capacity(8);
        d.extend_from_slice(&(idx as u16).to_be_bytes());
        d.extend_from_slice(&(len as u16).to_be_bytes());
        d.extend_from_slice(&(self.config.version as u32).to_be_bytes());
        Sha256::digest(d)[..8].try_into().unwrap()
    }
    fn fold(&self, prev: &[u8], seg: &[u8], idx: usize) -> Vec<u8> {
        let c = self.constant(idx, seg.len());
        let mut m = Vec::new();
        m.extend_from_slice(prev);
        m.extend_from_slice(seg);
        m.extend_from_slice(&(idx as u32).to_be_bytes());
        m.extend_from_slice(&c);
        let nested = self.config.hash_algo.digest(prev);
        m.extend_from_slice(&nested[..nested.len().min(16)]);
        self.config.hash_algo.digest(&m)
    }
    pub fn refactorize(&self, data: &[u8], salt: Option<&[u8]>) -> Vec<u8> {
        let zero = vec![0; self.config.salt_size];
        let salt = salt.unwrap_or(&zero);
        let mut init = Vec::new();
        init.extend_from_slice(salt);
        init.extend_from_slice(data);
        let mut state = self.config.hash_algo.digest(&init);
        let depth = self.config.fold_depth;
        let size = data.len() / depth.max(1) + 1;
        for i in 0..depth {
            let segment = if data.is_empty() {
                &[][..]
            } else {
                let start = (i * size) % data.len();
                &data[start..(start + size).min(data.len())]
            };
            state = self.fold(&state, segment, i);
        }
        state
    }
    pub fn refactorize_with_counter(&self, data: &[u8], salt: &[u8], counter: u32) -> Vec<u8> {
        let mut v = data.to_vec();
        v.extend_from_slice(&counter.to_be_bytes());
        self.refactorize(&v, Some(salt))
    }
}

pub struct DeterministicKdf {
    pub config: EncryptionConfig,
    rhash: InFoldingRhash,
}
impl DeterministicKdf {
    pub fn new(config: EncryptionConfig) -> Self {
        Self {
            rhash: InFoldingRhash::new(config.clone()),
            config,
        }
    }
    pub fn derive_key(
        &self,
        password: &[u8],
        salt: &[u8],
        length: usize,
        iterations: Option<u32>,
    ) -> Zeroizing<Vec<u8>> {
        let mut cur = Zeroizing::new(password.to_vec());
        for i in 0..iterations.unwrap_or(self.config.kdf_iterations) {
            let folded = self.rhash.refactorize(&cur, Some(salt));
            let mut data = folded;
            data.extend_from_slice(&i.to_be_bytes());
            cur = Zeroizing::new(Sha256::digest(data).to_vec());
        }
        let mut material = cur.to_vec();
        let mut counter = 0;
        while material.len() < length {
            material.extend_from_slice(&self.rhash.refactorize_with_counter(&cur, salt, counter));
            counter += 1;
        }
        material.truncate(length);
        Zeroizing::new(material)
    }
    pub fn derive_cipher_and_auth_keys(
        &self,
        password: &[u8],
        salt: &[u8],
    ) -> (Zeroizing<Vec<u8>>, Zeroizing<Vec<u8>>) {
        let all = self.derive_key(
            password,
            salt,
            self.config.cipher_key_size + self.config.auth_key_size,
            None,
        );
        (
            Zeroizing::new(all[..self.config.cipher_key_size].to_vec()),
            Zeroizing::new(all[self.config.cipher_key_size..].to_vec()),
        )
    }
    pub fn verify_consistency(&self, p: &[u8], s: &[u8]) -> bool {
        self.derive_key(p, s, 32, None) == self.derive_key(p, s, 32, None)
    }
}

fn ctr(input: &[u8], key: &[u8], iv: &[u8]) -> Vec<u8> {
    let cipher = Aes256::new_from_slice(key).expect("AES-256 key length validated");
    let mut ctr: [u8; 16] = iv.try_into().expect("AES CTR IV length");
    let mut out = Vec::with_capacity(input.len());
    for chunk in input.chunks(16) {
        let mut block = GenericArray::clone_from_slice(&ctr);
        cipher.encrypt_block(&mut block);
        out.extend(chunk.iter().zip(block.iter()).map(|(a, b)| a ^ b));
        for b in ctr.iter_mut().rev() {
            let (n, carry) = b.overflowing_add(1);
            *b = n;
            if !carry {
                break;
            }
        }
    }
    ctr.zeroize();
    out
}
pub struct DeterministicEncryption {
    pub config: EncryptionConfig,
    kdf: DeterministicKdf,
    rhash: InFoldingRhash,
}
impl Default for DeterministicEncryption {
    fn default() -> Self {
        Self::new(EncryptionConfig::default())
    }
}
impl DeterministicEncryption {
    pub fn new(config: EncryptionConfig) -> Self {
        Self {
            kdf: DeterministicKdf::new(config.clone()),
            rhash: InFoldingRhash::new(config.clone()),
            config,
        }
    }
    fn iv(&self, salt: &[u8]) -> Vec<u8> {
        let mut d = salt.to_vec();
        d.extend_from_slice(b"IV_GEN");
        self.rhash.refactorize(&d, Some(salt))[..16].to_vec()
    }
    pub fn encrypt(
        &self,
        plain: &[u8],
        password: &[u8],
        salt: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        if self.config.cipher_key_size != 32 || self.config.auth_key_size == 0 {
            return Err("AES-256 requires a 32-byte cipher key and nonempty auth key".into());
        }
        let mut random = vec![0; self.config.salt_size];
        let s = if let Some(s) = salt {
            s
        } else {
            OsRng.fill_bytes(&mut random);
            &random
        };
        if s.len() != self.config.salt_size {
            return Err("invalid salt length".into());
        }
        let (k, a) = self.kdf.derive_cipher_and_auth_keys(password, s);
        let ciphertext = ctr(plain, &k, &self.iv(s));
        let mut msg = vec![self.config.version];
        msg.extend_from_slice(s);
        msg.extend_from_slice(&ciphertext);
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&a).map_err(|e| e.to_string())?;
        mac.update(&msg);
        msg.extend_from_slice(&mac.finalize().into_bytes());
        Ok(msg)
    }
    pub fn encrypt_deterministic(&self, p: &[u8], password: &[u8]) -> Result<Vec<u8>, String> {
        let salt = Sha256::digest(password);
        self.encrypt(p, password, Some(&salt[..self.config.salt_size.min(32)]))
    }
    pub fn decrypt(&self, msg: &[u8], password: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        let hs = 1 + self.config.salt_size;
        if msg.len() < hs + 32 {
            return Err("invalid message format".into());
        }
        if msg[0] != self.config.version {
            return Err(format!("unsupported version: {}", msg[0]));
        }
        let salt = &msg[1..hs];
        let split = msg.len() - 32;
        let (k, a) = self.kdf.derive_cipher_and_auth_keys(password, salt);
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&a).map_err(|e| e.to_string())?;
        mac.update(&msg[..split]);
        mac.verify_slice(&msg[split..])
            .map_err(|_| "authentication failed".to_string())?;
        Ok(Zeroizing::new(ctr(&msg[hs..split], &k, &self.iv(salt))))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    Plaintext = 0,
    Encrypted = 1,
    Signed = 2,
    Commitment = 3,
    Proof = 4,
    Session = 5,
}
#[derive(Clone, Debug)]
pub struct SecureMessage {
    pub msg_type: MessageType,
    pub content: Vec<u8>,
    pub timestamp: u64,
    pub sequence: u32,
    pub metadata: BTreeMap<String, String>,
}
pub struct SecureMessageSequence {
    counter: u32,
    last: u64,
}
impl Default for SecureMessageSequence {
    fn default() -> Self {
        Self {
            counter: 0,
            last: 0,
        }
    }
}
impl SecureMessageSequence {
    pub fn create(
        &mut self,
        t: MessageType,
        content: Vec<u8>,
        metadata: BTreeMap<String, String>,
    ) -> SecureMessage {
        self.counter += 1;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last = self
            .last
            .max(now)
            .saturating_add(u64::from(now <= self.last));
        SecureMessage {
            msg_type: t,
            content,
            timestamp: self.last,
            sequence: self.counter,
            metadata,
        }
    }
    pub fn verify(&self, m: &SecureMessage, expected: u32) -> bool {
        m.sequence == expected
    }
    pub fn serialize(&self, m: &SecureMessage) -> Vec<u8> {
        let mut v = vec![m.msg_type as u8];
        v.extend_from_slice(&m.timestamp.to_be_bytes());
        v.extend_from_slice(&m.sequence.to_be_bytes());
        let meta = serde_json::to_vec(&m.metadata).unwrap_or_default();
        v.extend_from_slice(&(meta.len() as u16).to_be_bytes());
        v.extend(meta);
        v.extend(&m.content);
        v
    }
}

pub struct CryptographicCommitment {
    rhash: InFoldingRhash,
    items: BTreeMap<String, Vec<u8>>,
}
impl CryptographicCommitment {
    pub fn new(c: EncryptionConfig) -> Self {
        Self {
            rhash: InFoldingRhash::new(c),
            items: BTreeMap::new(),
        }
    }
    pub fn commit(&mut self, data: &[u8], randomness: Option<&[u8]>) -> (String, Vec<u8>) {
        let r = randomness
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Sha256::digest(data).to_vec());
        let mut v = data.to_vec();
        v.extend(&r);
        let commitment = self.rhash.refactorize(&v, None);
        let id = hex(&Sha256::digest(&commitment))[..16].to_string();
        self.items.insert(id.clone(), commitment);
        (id, r)
    }
    pub fn reveal(&self, id: &str, data: &[u8], r: &[u8]) -> bool {
        let Some(stored) = self.items.get(id) else {
            return false;
        };
        let mut v = data.to_vec();
        v.extend(r);
        constant_eq(stored, &self.rhash.refactorize(&v, None))
    }
}
pub struct SimplifiedZkProof {
    rhash: InFoldingRhash,
}
impl SimplifiedZkProof {
    pub fn new(c: EncryptionConfig) -> Self {
        Self {
            rhash: InFoldingRhash::new(c),
        }
    }
    pub fn generate_challenge(&self, s: &[u8]) -> (Vec<u8>, Vec<u8>) {
        (self.rhash.refactorize(s, None), Sha256::digest(s).to_vec())
    }
    pub fn generate_response(&self, c: &[u8], k: &[u8]) -> Vec<u8> {
        let mut d = c.to_vec();
        d.extend(k);
        self.rhash.refactorize(&d, None)
    }
    pub fn verify_response(&self, commitment: &[u8], challenge: &[u8], response: &[u8]) -> bool {
        let mut d = commitment.to_vec();
        d.extend(challenge);
        constant_eq(response.get(..32).unwrap_or(response), &Sha256::digest(d))
    }
}
pub struct DeterministicMac {
    rhash: InFoldingRhash,
    kdf: DeterministicKdf,
    config: EncryptionConfig,
}
impl DeterministicMac {
    pub fn new(c: EncryptionConfig) -> Self {
        Self {
            rhash: InFoldingRhash::new(c.clone()),
            kdf: DeterministicKdf::new(c.clone()),
            config: c,
        }
    }
    pub fn compute(&self, msg: &[u8], key: &[u8], salt: Option<&[u8]>) -> Vec<u8> {
        let derived = Sha256::digest(key);
        let owned;
        let s = if let Some(s) = salt {
            s
        } else {
            owned = derived[..self.config.salt_size.min(32)].to_vec();
            &owned
        };
        let mac_key = self.kdf.derive_key(key, s, 32, None);
        let mut v = msg.to_vec();
        v.extend(&*mac_key);
        self.rhash.refactorize(&v, Some(s))
    }
    pub fn verify(&self, m: &[u8], tag: &[u8], k: &[u8], s: Option<&[u8]>) -> bool {
        constant_eq(tag, &self.compute(m, k, s))
    }
}
pub struct MultiPartyKeyExchange {
    config: EncryptionConfig,
    contributions: Vec<Zeroizing<Vec<u8>>>,
}
impl MultiPartyKeyExchange {
    pub fn new(c: EncryptionConfig) -> Self {
        Self {
            config: c,
            contributions: vec![],
        }
    }
    pub fn add(&mut self, c: &[u8]) {
        self.contributions.push(Zeroizing::new(c.to_vec()))
    }
    pub fn derive(&self, salt: &[u8], len: usize) -> Result<Zeroizing<Vec<u8>>, String> {
        if self.contributions.is_empty() {
            return Err("no contributions added".into());
        }
        let mut all = Zeroizing::new(Vec::new());
        for c in &self.contributions {
            all.extend_from_slice(c)
        }
        Ok(DeterministicKdf::new(self.config.clone()).derive_key(&all, salt, len, None))
    }
    pub fn reset(&mut self) {
        self.contributions.clear()
    }
}
pub struct CryptographicChain {
    rhash: InFoldingRhash,
    state: Vec<u8>,
    operations: Vec<(String, Vec<u8>)>,
}
impl CryptographicChain {
    pub fn new(c: EncryptionConfig) -> Self {
        Self {
            rhash: InFoldingRhash::new(c),
            state: vec![],
            operations: vec![],
        }
    }
    pub fn add_operation(&mut self, op: &str, data: &[u8]) -> Vec<u8> {
        let mut v = self.state.clone();
        v.extend(op.as_bytes());
        v.extend(data);
        self.state = self.rhash.refactorize(&v, None);
        self.operations.push((op.into(), data.into()));
        self.state.clone()
    }
    pub fn hash(&self) -> String {
        hex(&Sha256::digest(&self.state))
    }
    pub fn verify(&self, o: &Self) -> bool {
        constant_eq(&self.state, &o.state)
    }
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }
}
#[derive(Debug, PartialEq)]
pub struct EncryptionStats {
    pub plaintext_size: usize,
    pub ciphertext_size: usize,
    pub expansion_ratio: f64,
    pub hash_algo: String,
    pub fold_depth: usize,
    pub kdf_iterations: u32,
    pub deterministic: bool,
}
pub fn analyze_encryption(plain: &[u8], encrypted: &[u8], c: &EncryptionConfig) -> EncryptionStats {
    EncryptionStats {
        plaintext_size: plain.len(),
        ciphertext_size: encrypted.len(),
        expansion_ratio: if plain.is_empty() {
            0.0
        } else {
            encrypted.len() as f64 / plain.len() as f64
        },
        hash_algo: c.hash_algo.name().into(),
        fold_depth: c.fold_depth,
        kdf_iterations: c.kdf_iterations,
        deterministic: true,
    }
}
#[derive(Debug, PartialEq)]
pub struct Entropy {
    pub total_bytes: usize,
    pub unique_bytes: usize,
    pub entropy: f64,
    pub distribution: [usize; 256],
}
pub fn analyze_entropy(data: &[u8]) -> Option<Entropy> {
    if data.is_empty() {
        return None;
    }
    let mut counts = [0usize; 256];
    for b in data {
        counts[*b as usize] += 1
    }
    let n = data.len() as f64;
    let entropy = counts
        .iter()
        .filter(|x| **x > 0)
        .map(|x| {
            let p = *x as f64 / n;
            -p * p.log2()
        })
        .sum();
    Some(Entropy {
        total_bytes: data.len(),
        unique_bytes: counts.iter().filter(|x| **x > 0).count(),
        entropy,
        distribution: counts,
    })
}
#[derive(Debug, PartialEq)]
pub struct OutputComparison {
    pub length_match: bool,
    pub identical: Option<bool>,
    pub different_bits: usize,
    pub bit_difference_ratio: Option<f64>,
    pub avalanche_effect: Option<&'static str>,
}
pub fn compare_outputs(a: &[u8], b: &[u8]) -> OutputComparison {
    if a.len() != b.len() {
        return OutputComparison {
            length_match: false,
            identical: None,
            different_bits: 0,
            bit_difference_ratio: None,
            avalanche_effect: None,
        };
    }
    let bits: usize = a
        .iter()
        .zip(b)
        .map(|(x, y)| (x ^ y).count_ones() as usize)
        .sum();
    let ratio = if a.is_empty() {
        0.0
    } else {
        bits as f64 / (a.len() * 8) as f64
    };
    OutputComparison {
        length_match: true,
        identical: Some(bits == 0),
        different_bits: bits,
        bit_difference_ratio: Some(ratio),
        avalanche_effect: Some(if bits > a.len() * 4 { "strong" } else { "weak" }),
    }
}
fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut x = 0u8;
    for (i, j) in a.iter().zip(b) {
        x |= i ^ j
    }
    x == 0
}
fn hex(b: &[u8]) -> String {
    const H: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char)
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> EncryptionConfig {
        EncryptionConfig {
            kdf_iterations: 2,
            ..Default::default()
        }
    }

    #[test]
    fn all_hash_modes_and_empty_input_work() {
        for algo in [
            HashAlgorithm::Sha256,
            HashAlgorithm::Sha512,
            HashAlgorithm::Sha3_256,
            HashAlgorithm::Sha3_512,
        ] {
            let r = InFoldingRhash::new(EncryptionConfig {
                hash_algo: algo,
                ..config()
            });
            assert!(!r.refactorize(b"payload", None).is_empty());
            assert!(!r.refactorize(b"", None).is_empty());
        }
    }
    #[test]
    fn kdf_is_repeatable_and_password_sensitive() {
        let k = DeterministicKdf::new(config());
        assert_eq!(
            k.derive_key(b"pw", b"salt", 48, None),
            k.derive_key(b"pw", b"salt", 48, None)
        );
        assert_ne!(
            k.derive_key(b"pw", b"salt", 32, None),
            k.derive_key(b"other", b"salt", 32, None)
        );
    }
    #[test]
    fn encrypt_decrypt_authenticate_and_deterministic_compatibility() {
        let e = DeterministicEncryption::new(config());
        let salt = [7u8; 16];
        let one = e.encrypt(b"message", b"password", Some(&salt)).unwrap();
        assert_eq!(
            one,
            e.encrypt(b"message", b"password", Some(&salt)).unwrap()
        );
        assert_eq!(&*e.decrypt(&one, b"password").unwrap(), b"message");
        assert_eq!(
            e.encrypt_deterministic(b"message", b"password").unwrap(),
            e.encrypt_deterministic(b"message", b"password").unwrap()
        );
        assert!(e.decrypt(&one, b"wrong").is_err());
        let mut tampered = one;
        tampered[2] ^= 1;
        assert!(e.decrypt(&tampered, b"password").is_err());
    }
    #[test]
    fn commitments_mac_and_chain_verify() {
        let c = config();
        let mut commits = CryptographicCommitment::new(c.clone());
        let (id, randomness) = commits.commit(b"secret", Some(b"nonce"));
        assert!(commits.reveal(&id, b"secret", &randomness));
        assert!(!commits.reveal(&id, b"changed", &randomness));
        let mac = DeterministicMac::new(c.clone());
        let tag = mac.compute(b"message", b"key", Some(b"salt"));
        assert!(mac.verify(b"message", &tag, b"key", Some(b"salt")));
        assert!(!mac.verify(b"other", &tag, b"key", Some(b"salt")));
        let mut a = CryptographicChain::new(c.clone());
        let mut b = CryptographicChain::new(c);
        a.add_operation("write", b"x");
        b.add_operation("write", b"x");
        assert!(a.verify(&b));
    }
    #[test]
    fn sequencing_exchange_and_analyzers_handle_edges() {
        let mut seq = SecureMessageSequence::default();
        let msg = seq.create(MessageType::Encrypted, vec![1], BTreeMap::new());
        assert!(seq.verify(&msg, 1));
        assert!(!seq.serialize(&msg).is_empty());
        let mut mp = MultiPartyKeyExchange::new(config());
        assert!(mp.derive(b"salt", 32).is_err());
        mp.add(b"one");
        let first = mp.derive(b"salt", 32).unwrap();
        mp.reset();
        assert!(mp.derive(b"salt", 32).is_err());
        assert_eq!(first.len(), 32);
        assert_eq!(analyze_entropy(b""), None);
        assert_eq!(analyze_entropy(b"aaaa").unwrap().entropy, 0.0);
        assert_eq!(compare_outputs(b"", b"").bit_difference_ratio, Some(0.0));
        assert_eq!(compare_outputs(b"a", b"b").different_bits, 2);
    }
    #[test]
    fn simplified_zk_helper_preserves_its_non_zk_source_behavior() {
        let p = SimplifiedZkProof::new(config());
        let (commitment, key) = p.generate_challenge(b"secret");
        let response = p.generate_response(b"challenge", &key);
        assert!(!p.verify_response(&commitment, b"challenge", &response));
        assert!(p.verify_response(
            &commitment,
            b"challenge",
            &Sha256::digest([commitment.as_slice(), b"challenge"].concat())
        ));
    }
}
