//! cryptographic operations
//!
//! Biscuit tokens are based on a chain of Ed25519 signatures.
//! This provides the fundamental operation for offline delegation: from a message
//! and a valid signature, it is possible to add a new message and produce a valid
//! signature for the whole.
//!
//! The implementation is based on [ed25519_dalek](https://github.com/dalek-cryptography/ed25519-dalek).
#![allow(non_snake_case)]
use crate::{error::Format, format::schema};

use super::error;
use super::Signature;
use ed25519_dalek::Signer;
use ed25519_dalek::*;
use rand_core::{CryptoRng, RngCore};
use std::{convert::TryInto, hash::Hash, ops::Drop};
use zeroize::Zeroize;

/// pair of cryptographic keys used to sign a token's block
#[derive(Debug)]
pub struct KeyPair {
    kp: ed25519_dalek::Keypair,
}

impl KeyPair {
    pub fn new() -> Self {
        Self::new_with_rng(&mut rand::rngs::OsRng)
    }

    pub fn new_with_rng<T: RngCore + CryptoRng>(rng: &mut T) -> Self {
        let kp = ed25519_dalek::Keypair::generate(rng);

        KeyPair { kp }
    }

    pub fn from(key: &PrivateKey) -> Self {
        let secret = SecretKey::from_bytes(&key.0.to_bytes()).unwrap();

        let public = (&key.0).into();

        KeyPair {
            kp: ed25519_dalek::Keypair { secret, public },
        }
    }

    /// deserializes from a byte array
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, error::Format> {
        let secret = SecretKey::from_bytes(bytes)
            .map_err(|s| s.to_string())
            .map_err(Format::InvalidKey)?;

        let public = (&secret).into();

        Ok(KeyPair {
            kp: ed25519_dalek::Keypair { secret, public },
        })
    }

    pub fn sign(&self, data: &[u8]) -> Result<Signature, error::Format> {
        Ok(Signature(
            self.kp
                .try_sign(&data)
                .map_err(|s| s.to_string())
                .map_err(error::Signature::InvalidSignatureGeneration)
                .map_err(error::Format::Signature)?
                .to_bytes()
                .to_vec(),
        ))
    }

    pub fn private(&self) -> PrivateKey {
        let secret = SecretKey::from_bytes(&self.kp.secret.to_bytes()).unwrap();
        PrivateKey(secret)
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.kp.public)
    }

    pub fn algorithm(&self) -> crate::format::schema::public_key::Algorithm {
        crate::format::schema::public_key::Algorithm::Ed25519
    }
}

impl std::default::Default for KeyPair {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for KeyPair {
    fn drop(&mut self) {
        self.kp.secret.zeroize();
    }
}

/// the private part of a [KeyPair]
#[derive(Debug)]
pub struct PrivateKey(ed25519_dalek::SecretKey);

impl PrivateKey {
    /// serializes to a byte array
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_bytes().to_vec()
    }

    /// serializes to an hex-encoded string
    pub fn to_bytes_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    /// deserializes from a byte array
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, error::Format> {
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| Format::InvalidKeySize(bytes.len()))?;
        SecretKey::from_bytes(&bytes)
            .map(PrivateKey)
            .map_err(|s| s.to_string())
            .map_err(Format::InvalidKey)
    }

    /// deserializes from an hex-encoded string
    pub fn from_bytes_hex(str: &str) -> Result<Self, error::Format> {
        let bytes = hex::decode(str).map_err(|e| error::Format::InvalidKey(e.to_string()))?;
        Self::from_bytes(&bytes)
    }

    /// returns the matching public key
    pub fn public(&self) -> PublicKey {
        PublicKey((&self.0).into())
    }

    pub fn algorithm(&self) -> crate::format::schema::public_key::Algorithm {
        crate::format::schema::public_key::Algorithm::Ed25519
    }
}

impl std::clone::Clone for PrivateKey {
    fn clone(&self) -> Self {
        PrivateKey::from_bytes(&self.to_bytes()).unwrap()
    }
}

impl Drop for PrivateKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// the public part of a [KeyPair]
#[derive(Debug, Clone, Copy, Eq)]
pub struct PublicKey(ed25519_dalek::PublicKey);

impl PublicKey {
    /// serializes to a byte array
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// serializes to an hex-encoded string
    pub fn to_bytes_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    /// deserializes from a byte array
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, error::Format> {
        ed25519_dalek::PublicKey::from_bytes(bytes)
            .map(PublicKey)
            .map_err(|s| s.to_string())
            .map_err(Format::InvalidKey)
    }

    /// deserializes from an hex-encoded string
    pub fn from_bytes_hex(str: &str) -> Result<Self, error::Format> {
        let bytes = hex::decode(str).map_err(|e| error::Format::InvalidKey(e.to_string()))?;
        Self::from_bytes(&bytes)
    }

    pub fn from_proto(key: &schema::PublicKey) -> Result<Self, error::Format> {
        if key.algorithm != schema::public_key::Algorithm::Ed25519 as i32 {
            return Err(error::Format::DeserializationError(format!(
                "deserialization error: unexpected key algorithm {}",
                key.algorithm
            )));
        }

        PublicKey::from_bytes(&key.key)
    }

    pub fn to_proto(&self) -> schema::PublicKey {
        schema::PublicKey {
            algorithm: schema::public_key::Algorithm::Ed25519 as i32,
            key: self.to_bytes().to_vec(),
        }
    }

    pub fn verify_signature(
        &self,
        data: &[u8],
        signature: &Signature,
    ) -> Result<(), error::Format> {
        let sig = ed25519_dalek::Signature::from_bytes(&signature.0).map_err(|e| {
            error::Format::BlockSignatureDeserializationError(format!(
                "block signature deserialization error: {:?}",
                e
            ))
        })?;

        self.0
            .verify_strict(&data, &sig)
            .map_err(|s| s.to_string())
            .map_err(error::Signature::InvalidSignature)
            .map_err(error::Format::Signature)
    }

    pub fn algorithm(&self) -> crate::format::schema::public_key::Algorithm {
        crate::format::schema::public_key::Algorithm::Ed25519
    }

    pub fn print(&self) -> String {
        format!("ed25519/{}", hex::encode(&self.to_bytes()))
    }
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bytes() == other.0.to_bytes()
    }
}

impl Hash for PublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (crate::format::schema::public_key::Algorithm::Ed25519 as i32).hash(state);
        self.0.to_bytes().hash(state);
    }
}
