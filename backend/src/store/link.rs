use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::file_name::FileName;
use super::record::ImportId;

pub(super) struct Signer(Hmac<Sha256>);

impl Signer {
    pub(super) fn random() -> Self {
        let mut secret = [0; 32];
        getrandom::fill(&mut secret).expect("the operating system provides random bytes");
        Self(Hmac::new_from_slice(&secret).expect("HMAC accepts a key of any length"))
    }

    pub(super) fn url(&self, id: ImportId, name: &FileName, exp: i64) -> String {
        let tag = self.mac(id, name, exp).finalize().into_bytes();
        let sig: String = tag.iter().map(|byte| format!("{byte:02x}")).collect();
        format!("/api/files/{id}/{name}?exp={exp}&sig={sig}")
    }

    pub(super) fn verify(&self, id: ImportId, name: &FileName, exp: i64, sig: &str) -> bool {
        decode_lowercase_hex(sig)
            .is_some_and(|tag| self.mac(id, name, exp).verify_slice(&tag).is_ok())
    }

    fn mac(&self, id: ImportId, name: &FileName, exp: i64) -> Hmac<Sha256> {
        let mut mac = self.0.clone();
        mac.update(format!("{id}/{name}/{exp}").as_bytes());
        mac
    }
}

fn decode_lowercase_hex(text: &str) -> Option<Vec<u8>> {
    let nibble = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    };
    text.as_bytes()
        .chunks(2)
        .map(|pair| match *pair {
            [high, low] => Some(nibble(high)? << 4 | nibble(low)?),
            _ => None,
        })
        .collect()
}
