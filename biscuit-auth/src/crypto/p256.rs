#![allow(non_snake_case)]
use crate::{error::Format, format::schema};

use super::error;
use super::Signature;

use p256::ecdsa::{signature::Signer, signature::Verifier, SigningKey, VerifyingKey};
use p256::elliptic_curve::rand_core::{CryptoRng, OsRng, RngCore};
use p256::elliptic_curve::sec1::FromEncodedPoint;
use p256::pkcs8::{DecodePublicKey, EncodePrivateKey, EncodePublicKey};
use pkcs8::DecodePrivateKey;
use std::convert::TryFrom;
use std::hash::Hash;

/// pair of cryptographic keys used to sign a token's block
#[derive(Debug)]
pub struct KeyPair {
    kp: SigningKey,
}

impl KeyPair {
    pub fn new() -> Self {
        Self::new_with_rng(&mut OsRng)
    }

    pub fn new_with_rng<T: RngCore + CryptoRng>(rng: &mut T) -> Self {
        let kp = SigningKey::random(rng);

        KeyPair { kp }
    }

    pub fn from(key: &PrivateKey) -> Self {
        let kp = SigningKey::from_pkcs8_der(&key.0.to_bytes()).unwrap();

        KeyPair { kp }
    }

    /// deserializes from a byte array
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, error::Format> {
        let kp = SigningKey::from_pkcs8_der(bytes)
            .map_err(|s| s.to_string())
            .map_err(Format::InvalidKey)?;

        Ok(KeyPair { kp })
    }

    pub fn sign(&self, data: &[u8]) -> Result<Signature, error::Format> {
        Ok(Signature(
            self.kp
                .try_sign(&data)
                .map_err(|s| s.to_string())
                .map_err(error::Signature::InvalidSignatureGeneration)
                .map_err(error::Format::Signature)?
                .to_vec(),
        ))
    }

    pub fn private(&self) -> PrivateKey {
        let secret = SigningKey::from_bytes(&self.kp.to_bytes()).unwrap();
        PrivateKey(secret)
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.kp.verifying_key())
    }

    pub fn algorithm(&self) -> crate::format::schema::public_key::Algorithm {
        crate::format::schema::public_key::Algorithm::P256
    }
}

impl std::default::Default for KeyPair {
    fn default() -> Self {
        Self::new()
    }
}

/// the private part of a [KeyPair]
#[derive(Debug)]
pub struct PrivateKey(SigningKey);

impl PrivateKey {
    /// serializes to a byte array
    pub fn to_bytes(&self) -> zeroize::Zeroizing<Vec<u8>> {
        self.0.to_pkcs8_der().unwrap().to_bytes()
        //self.0.to_bytes().into()
    }

    /// serializes to an hex-encoded string
    pub fn to_bytes_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    /// deserializes from a byte array
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, error::Format> {
        SigningKey::from_pkcs8_der(&bytes)
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
        PublicKey((&self.0).verifying_key())
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

/// the public part of a [KeyPair]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicKey(VerifyingKey);

impl PublicKey {
    /// serializes to a byte array
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_encoded_point(true).to_bytes().into()
    }

    /// serializes to an hex-encoded string
    pub fn to_bytes_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    /// deserializes from a byte array
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, error::Format> {
        let k = VerifyingKey::from_sec1_bytes(bytes)
            .map_err(|s| s.to_string())
            .map_err(Format::InvalidKey)?;

        Ok(Self(k.into()))
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
        let sig = p256::ecdsa::Signature::try_from(&signature.0[..]).map_err(|e| {
            error::Format::BlockSignatureDeserializationError(format!(
                "block signature deserialization error: {:?}",
                e
            ))
        })?;

        self.0
            .verify(&data, &sig)
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

impl Hash for PublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (crate::format::schema::public_key::Algorithm::Ed25519 as i32).hash(state);
        self.to_bytes().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization() {
        let kp = KeyPair::new();
        let private = kp.private();
        let public = kp.public();
        let private_hex = private.to_bytes_hex();
        let public_hex = public.to_bytes_hex();

        println!("private: {private_hex}");
        println!("public: {public_hex}");

        let message = "hello world";
        let signature = kp.sign(message.as_bytes()).unwrap();
        println!("signature: {}", hex::encode(&signature.0));

        let deserialized_priv = PrivateKey::from_bytes_hex(&private_hex).unwrap();
        let deserialized_pub = PublicKey::from_bytes_hex(&public_hex).unwrap();

        // assert_eq!(private, deserialized_priv);
        assert_eq!(public, deserialized_pub);

        deserialized_pub
            .verify_signature(message.as_bytes(), &signature)
            .unwrap();
        panic!();
    }
}
