use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{anyhow, Context, Result};
use bip39::Mnemonic;
use bitcoin::bip32::{ChainCode, ChildNumber, Xpriv};
use bitcoin::secp256k1::ffi::types::AlignedType;
use bitcoin::secp256k1::{self, Secp256k1};
use bitcoin::NetworkKind;
use hmac::Hmac;
use ldk_node::KeyValue;
use serde::Deserialize;
use sha2::Sha512;

type HmacSha512 = Hmac<Sha512>;

#[derive(Deserialize, Debug)]
pub struct StaticChannelBackup {
    pub channels: Vec<ChannelBackup>,
    pub monitors: Vec<EncodedChannelMonitorBackup>,
}

impl StaticChannelBackup {
    pub fn channel_ids(&self) -> HashSet<String> {
        self.channels.iter().map(|c| c.channel_id.clone()).collect()
    }
}

#[derive(Deserialize, Debug)]
pub struct EncodedChannelMonitorBackup {
    pub key: String,

    #[serde(with = "hex")]
    pub value: Vec<u8>,
}

impl From<EncodedChannelMonitorBackup> for KeyValue {
    fn from(backup: EncodedChannelMonitorBackup) -> Self {
        KeyValue {
            key: backup.key,
            value: backup.value,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct ChannelBackup {
    pub channel_id: String,
    pub peer_id: String,
    pub peer_socket_address: String,
}

pub fn load_scb_guess_type<P>(path: P, mnemonic: &Mnemonic) -> Result<StaticChannelBackup>
where
    P: AsRef<Path>,
{
    load_scb(path.as_ref())
        .or_else(|_| load_scb_encrypted(path, mnemonic))
        .context("failed to load SCB")
}

pub fn load_scb<P>(path: P) -> Result<StaticChannelBackup>
where
    P: AsRef<Path>,
{
    serde_json::from_reader(BufReader::new(
        File::open(path).context("failed to open SCB file")?,
    ))
    .context("failed to parse SCB file")
}

pub fn load_scb_encrypted<P>(path: P, mnemonic: &Mnemonic) -> Result<StaticChannelBackup>
where
    P: AsRef<Path>,
{
    let encrypted = std::fs::read_to_string(path).context("failed to read SCB file")?;
    let plaintext = decrypt_scb_str(&encrypted, mnemonic)?;
    serde_json::from_str(&plaintext).context("failed to parse SCB file")
}

fn master_key(mnemonic: &Mnemonic) -> Xpriv {
    use hmac::Mac;

    let seed = mnemonic.to_seed("");

    let mut mac = HmacSha512::new_from_slice(b"Bitcoin seed").unwrap();
    mac.update(&seed);
    let hmac_result = mac.finalize().into_bytes();

    let chain_code: [u8; 32] = hmac_result[32..64].try_into().unwrap();

    Xpriv {
        network: NetworkKind::Main, // Not used in this context, safe to hardcode.
        depth: 0,
        parent_fingerprint: Default::default(),
        child_number: ChildNumber::from_normal_idx(0).unwrap(),
        private_key: secp256k1::SecretKey::from_slice(&hmac_result[0..32]).unwrap(),
        chain_code: ChainCode::from(chain_code),
    }
}

fn derive_scb_key(mnemonic: &Mnemonic) -> Key<Aes256Gcm> {
    let mut buf: Vec<AlignedType> = Vec::with_capacity(Secp256k1::preallocate_size());
    buf.resize(Secp256k1::preallocate_size(), AlignedType::zeroed());
    let secp = Secp256k1::preallocated_new(buf.as_mut_slice()).unwrap();

    let root = master_key(mnemonic);

    let app_key = root
        .derive_priv(
            &secp,
            &vec![ChildNumber::from_hardened_idx(128029).unwrap()],
        )
        .unwrap();
    let ret = app_key
        .derive_priv(&secp, &vec![ChildNumber::from_hardened_idx(0).unwrap()])
        .unwrap();

    Key::<Aes256Gcm>::from(ret.private_key.secret_bytes())
}

fn decrypt(nonce: &[u8], ciphertext: &[u8], key: &Key<Aes256Gcm>) -> Result<Vec<u8>> {
    use aes_gcm::KeyInit;

    let cipher = Aes256Gcm::new(key);
    cipher
        .decrypt(&Nonce::from_slice(nonce), ciphertext)
        .map_err(|e| anyhow!("{}", e))
        .context("failed to decrypt ciphertext")
}

fn decrypt_scb(nonce: &[u8], ciphertext: &[u8], mnemonic: &Mnemonic) -> Result<Vec<u8>> {
    let key = derive_scb_key(mnemonic);
    decrypt(nonce, ciphertext, &key)
}

fn decrypt_scb_str(xs: &str, mnemonic: &Mnemonic) -> Result<String> {
    let parts = xs.split('-').collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(anyhow!("invalid SCB format"));
    }

    let nonce = hex::decode(parts[0]).context("failed to decode nonce")?;
    let ciphertext = hex::decode(parts[1]).context("failed to decode encrypted data")?;

    let plaintext = decrypt_scb(&nonce, &ciphertext, mnemonic)?;

    Ok(String::from_utf8(plaintext)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decrypt() {
        let xs = "3fd21f9a393d8345ddbdd449-ba05c3dbafdfb7eea574373b7763d0c81c599b2cd1735e59a1c5571379498f4da8fe834c3403824ab02b61005abc1f563c638f425c65420e82941efe94794555c8b145a0603733ee115277f860011e6a17fd8c22f1d73a096ff7275582aac19b430940b40a2559c7ff59a063305290ef7c9ba46f9de17b0ddbac9030b0";
        let mnemonic = "limit reward expect search tissue call visa fit thank cream brave jump";
        let mnemonic = Mnemonic::parse(mnemonic).unwrap();

        let plaintext = decrypt_scb_str(xs, &mnemonic).unwrap();

        assert_eq!(plaintext, "{\"node_id\":\"037e702144c4fa485d42f0f69864e943605823763866cf4bf619d2d2cf2eda420b\",\"channels\":[],\"monitors\":[]}\n");
    }
}
