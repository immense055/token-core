use super::Result;

use crate::constant::SECP256K1_ENGINE;
use crate::ecc::{DeterministicPrivateKey, DeterministicPublicKey, KeyError};

use crate::{Derive, DeriveJunction, Secp256k1PrivateKey, Secp256k1PublicKey, Ss58Codec};
use bitcoin::util::key::PublicKey;

use bitcoin::util::base58;
use bitcoin::util::base58::Error::InvalidLength;
use bitcoin::util::bip32::{
    ChainCode, ChildNumber, Error as Bip32Error, ExtendedPrivKey, ExtendedPubKey, Fingerprint,
};
use bitcoin::Network;
use byteorder::BigEndian;
use byteorder::ByteOrder;

use std::convert::TryInto;

pub struct Bip32DeterministicPrivateKey(ExtendedPrivKey);

pub struct Bip32DeterministicPublicKey(ExtendedPubKey);

fn transform_bip32_error(err: Bip32Error) -> KeyError {
    match err {
        Bip32Error::Ecdsa(_) => KeyError::InvalidEcdsa,
        Bip32Error::RngError(_) => KeyError::OverflowChildNumber,
        Bip32Error::CannotDeriveFromHardenedKey => KeyError::CannotDeriveFromHardenedKey,
        Bip32Error::InvalidChildNumber(_) => KeyError::InvalidChildNumber,
        Bip32Error::InvalidChildNumberFormat => KeyError::InvalidChildNumber,
        Bip32Error::InvalidDerivationPathFormat => KeyError::InvalidDerivationPathFormat,
    }
}

impl Bip32DeterministicPrivateKey {
    /// Construct a new master key from a seed value
    pub fn from_seed(seed: &[u8]) -> Result<Self> {
        let epk =
            ExtendedPrivKey::new_master(Network::Bitcoin, seed).map_err(transform_bip32_error)?;
        Ok(Bip32DeterministicPrivateKey(epk))
    }
}

impl Derive for Bip32DeterministicPrivateKey {
    fn derive<T: Iterator<Item = DeriveJunction>>(&self, path: T) -> Result<Self> {
        let mut extended_key = self.0.clone();

        for j in path {
            let child_number = j.try_into()?;

            extended_key = extended_key
                .ckd_priv(&SECP256K1_ENGINE, child_number)
                .map_err(transform_bip32_error)?;
        }

        Ok(Bip32DeterministicPrivateKey(extended_key))
    }
}

impl Derive for Bip32DeterministicPublicKey {
    fn derive<Iter: Iterator<Item = DeriveJunction>>(&self, path: Iter) -> Result<Self> {
        let mut extended_key = self.0.clone();

        for j in path {
            let child_number = j.try_into()?;

            extended_key = extended_key
                .ckd_pub(&SECP256K1_ENGINE, child_number)
                .map_err(transform_bip32_error)?;
        }

        Ok(Bip32DeterministicPublicKey(extended_key))
    }
}

impl DeterministicPrivateKey for Bip32DeterministicPrivateKey {
    type DeterministicPublicKey = Bip32DeterministicPublicKey;
    type PrivateKey = Secp256k1PrivateKey;

    fn from_seed(seed: &[u8]) -> Result<Self> {
        let esk =
            ExtendedPrivKey::new_master(Network::Bitcoin, seed).map_err(transform_bip32_error)?;
        Ok(Bip32DeterministicPrivateKey(esk))
    }

    fn private_key(&self) -> Self::PrivateKey {
        Secp256k1PrivateKey::from(self.0.private_key.clone())
    }

    fn deterministic_public_key(&self) -> Self::DeterministicPublicKey {
        let pk = ExtendedPubKey::from_private(&SECP256K1_ENGINE, &self.0);
        Bip32DeterministicPublicKey(pk)
    }
}

impl DeterministicPublicKey for Bip32DeterministicPublicKey {
    type PublicKey = Secp256k1PublicKey;

    fn public_key(&self) -> Self::PublicKey {
        Secp256k1PublicKey::from(self.0.public_key.clone())
    }
}

impl Ss58Codec for Bip32DeterministicPublicKey {
    fn from_ss58check_with_version(s: &str) -> Result<(Self, Vec<u8>)> {
        let data = base58::from_check(s)?;

        if data.len() != 78 {
            return Err(KeyError::InvalidBase58.into());
        }
        let cn_int: u32 = BigEndian::read_u32(&data[9..13]);
        let child_number: ChildNumber = ChildNumber::from(cn_int);

        let epk = ExtendedPubKey {
            network: Network::Bitcoin,
            depth: data[4],
            parent_fingerprint: Fingerprint::from(&data[5..9]),
            child_number,
            chain_code: ChainCode::from(&data[13..45]),
            public_key: PublicKey::from_slice(&data[45..78])
                .map_err(|e| base58::Error::Other(e.to_string()))?,
        };

        let mut network = [0; 4];
        network.copy_from_slice(&data[0..4]);
        Ok((Bip32DeterministicPublicKey(epk), network.to_vec()))
    }

    fn to_ss58check_with_version(&self, version: &[u8]) -> String {
        let mut ret = [0; 78];
        let extended_key = self.0;
        ret[0..4].copy_from_slice(&version[..]);
        ret[4] = extended_key.depth as u8;
        ret[5..9].copy_from_slice(&extended_key.parent_fingerprint[..]);

        BigEndian::write_u32(&mut ret[9..13], u32::from(extended_key.child_number));

        ret[13..45].copy_from_slice(&extended_key.chain_code[..]);
        ret[45..78].copy_from_slice(&extended_key.public_key.key.serialize()[..]);
        base58::check_encode_slice(&ret[..])
    }
}

impl Ss58Codec for Bip32DeterministicPrivateKey {
    fn from_ss58check_with_version(s: &str) -> Result<(Self, Vec<u8>)> {
        let data = base58::from_check(s)?;

        if data.len() != 78 {
            return Err(InvalidLength(data.len()).into());
        }

        let cn_int: u32 = BigEndian::read_u32(&data[9..13]);
        let child_number: ChildNumber = ChildNumber::from(cn_int);

        let network = Network::Bitcoin;
        let epk = ExtendedPrivKey {
            network,
            depth: data[4],
            parent_fingerprint: Fingerprint::from(&data[5..9]),
            child_number,
            chain_code: ChainCode::from(&data[13..45]),
            private_key: bitcoin::PrivateKey {
                compressed: true,
                network,
                key: secp256k1::SecretKey::from_slice(&data[46..78])
                    .map_err(|e| base58::Error::Other(e.to_string()))?,
            },
        };
        let mut network = [0; 4];
        network.copy_from_slice(&data[0..4]);
        Ok((Bip32DeterministicPrivateKey(epk), network.to_vec()))
    }

    fn to_ss58check_with_version(&self, version: &[u8]) -> String {
        let mut ret = [0; 78];
        let extended_key = &self.0;

        ret[0..4].copy_from_slice(&version[..]);
        ret[4] = extended_key.depth as u8;
        ret[5..9].copy_from_slice(&extended_key.parent_fingerprint[..]);

        BigEndian::write_u32(&mut ret[9..13], u32::from(extended_key.child_number));

        ret[13..45].copy_from_slice(&extended_key.chain_code[..]);
        ret[45] = 0;
        ret[46..78].copy_from_slice(&extended_key.private_key[..]);
        base58::check_encode_slice(&ret[..])
    }
}

#[cfg(test)]
mod tests {
    use crate::{Bip32DeterministicPrivateKey, Bip32DeterministicPublicKey, Derive, DerivePath};
    use std::str::FromStr;

    #[test]
    fn test_key_at_paths_with_seed() {
        /*
        let seed = default_seed();
        let paths = vec![
            "m/44'/0'/0'/0/0",
            "m/44'/0'/0'/0/1",
            "m/44'/0'/0'/1/0",
            "m/44'/0'/0'/1/1",
        ];
        let esk = Bip32DeterministicPrivateKey::from_seed(&seed).unwrap();
        let pub_keys = paths
            .iter()
            .map(|path| {
                esk.derive(DerivePath::from_str(path).unwrap().into_iter())
                    .unwrap()
                    .private_key()
                    .public_key()
                    .to_compressed()
                    .to_hex()
            })
            .collect::<Vec<String>>();
        let expected_pub_keys = vec![
            "026b5b6a9d041bc5187e0b34f9e496436c7bff261c6c1b5f3c06b433c61394b868",
            "024fb7df3961e08f01025e434ea19708a4317d2fe59775cddd38df6e8a2d30697d",
            "0352470ace48f25b01b9c341e3b0e033fc32a203fb7a81a0453f97d94eca819a35",
            "022f4c38f7bbaa00fc886db62f975b34201c2bfed146e98973caf03268941801db",
        ];
        assert_eq!(pub_keys, expected_pub_keys);
        */
    }

    #[test]
    fn extended_key_test() {
        /*
        let seed = default_seed();
        let esk = Bip32DeterministicPrivateKey::from_seed(&seed).unwrap();

        let _xpub_key = esk.extended_pub_key().unwrap();
        let mut index_xpub_key =esk
            .derive(DerivePath::from_str("m/44'/0'/0'").unwrap().into_iter())
            .unwrap()
            .extended_pub_key()
            .unwrap();
        let xpub = index_xpub_key.to_string();
        assert_eq!(xpub, "xpub6CqzLtyKdJN53jPY13W6GdyB8ZGWuFZuBPU4Xh9DXm6Q1cULVLtsyfXSjx4G77rNdCRBgi83LByaWxjtDaZfLAKT6vFUq3EhPtNwTpJigx8");
        let private_key = Secp256k1PrivateKey::from_seed(&seed).unwrap();
        let mut xprv_key = private_key
            .derive(DerivePath::from_str("m/44'/0'/0'").unwrap().into_iter())
            .unwrap()
            .extended_priv_key()
            .unwrap();

        let xprv = xprv_key.to_string();
        assert_eq!(xprv, "xprv9yrdwPSRnvomqFK4u1y5uW2SaXS2Vnr3pAYTjJjbyRZR8p9BwoadRsCxtgUFdAKeRPbwvGRcCSYMV69nNK4N2kadevJ6L5iQVy1SwGKDTHQ");
        */
    }

    #[test]
    fn derive_pub_key_test() {
        /*
        let xpub = "xpub6CqzLtyKdJN53jPY13W6GdyB8ZGWuFZuBPU4Xh9DXm6Q1cULVLtsyfXSjx4G77rNdCRBgi83LByaWxjtDaZfLAKT6vFUq3EhPtNwTpJigx8";
        let xpub_key = Bip32DeterministicPublicKey::from_extended(xpub).unwrap();

        let path = DerivePath::from_str("0/0").unwrap();
        let index_pub_key = xpub_key.derive(path.into_iter()).unwrap();

        assert_eq!(
            index_pub_key.public_key().to_bytes().to_hex(),
            "026b5b6a9d041bc5187e0b34f9e496436c7bff261c6c1b5f3c06b433c61394b868"
        );

        let err = ExtendedPubKey::from_ss58check_with_version("invalid_xpub")
            .err()
            .unwrap();
        assert_eq!(format!("{}", err), "invalid base58 character 0x6c");
        */
    }
}