use alloy_rlp::{Decodable, Encodable};
use bytes::{Buf, BufMut, Bytes, BytesMut};

const UPGRADE_STATUS_MESSAGE_ID: u8 = 0x0b;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeStatus {
    pub extension: UpgradeStatusExtension,
}

impl Encodable for UpgradeStatus {
    fn encode(&self, out: &mut dyn BufMut) {
        UPGRADE_STATUS_MESSAGE_ID.encode(out);
        vec![&self.extension].encode(out);
    }
}

impl Decodable for UpgradeStatus {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let message_id = u8::decode(buf)?;
        if message_id != UPGRADE_STATUS_MESSAGE_ID {
            return Err(alloy_rlp::Error::Custom("Invalid message ID"));
        }
        let extension: Vec<UpgradeStatusExtension> = Decodable::decode(buf)?;
        if extension.len() != 1 {
            return Err(alloy_rlp::Error::Custom("Invalid extension length"));
        }
        Ok(Self { extension: extension[0] })
    }
}

impl UpgradeStatus {
    pub fn into_rlpx(self) -> Bytes {
        let mut out = BytesMut::new();
        self.encode(&mut out);
        out.freeze()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpgradeStatusExtension {
    pub disable_peer_tx_broadcast: bool,
}

impl Encodable for UpgradeStatusExtension {
    fn encode(&self, out: &mut dyn BufMut) {
        vec![self.disable_peer_tx_broadcast].encode(out);
    }
}

impl Decodable for UpgradeStatusExtension {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        if buf.first().copied() == Some(0x80) {
            buf.advance(1);
            return Ok(Self { disable_peer_tx_broadcast: false });
        }
        let vals = <Vec<bool>>::decode(buf)?;
        if vals.len() != 1 {
            return Err(alloy_rlp::Error::Custom("Invalid bool length"));
        }
        Ok(Self { disable_peer_tx_broadcast: vals[0] })
    }
}
