// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// CRC-32C (Castagnoli) of V2 records.

fn record_crc32c(slot: &[u8], payload_len: usize) -> u32 {
    let mut crc = 0xffff_ffffu32;
    crc = crc32c_update(crc, &slot[8..SLOT_OFF_CRC32C]);
    crc = crc32c_update(
        crc,
        &slot[SLOT_HEADER_SIZE_V2..SLOT_HEADER_SIZE_V2 + payload_len],
    );
    !crc
}

fn crc32c_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0x82f6_3b78 & mask);
        }
    }
    crc
}

pub fn crc32c(bytes: &[u8]) -> u32 {
    !crc32c_update(0xffff_ffff, bytes)
}
