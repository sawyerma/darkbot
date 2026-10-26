//! Runtime-code facts and contract-address derivation shared by the G0.1
//! freeze binaries: the dispatcher/immutable sweep, the EIP-1967
//! implementation slot, EIP-1167 clones and EIP-1014 CREATE2 through alloy.
//! Nothing here hashes by hand.

use alloy_primitives::{Address, B256, U256, hex, keccak256};
use std::collections::{BTreeMap, BTreeSet};

/// The EIP-1967 implementation slot: keccak256("eip1967.proxy.implementation") - 1.
pub fn eip1967_implementation_slot() -> B256 {
    let mut slot = U256::from_be_bytes(keccak256("eip1967.proxy.implementation").0);
    slot -= U256::from(1u8);
    B256::from(slot.to_be_bytes::<32>())
}

// ---- bytecode ---------------------------------------------------------------

/// Solidity's CBOR metadata trailer: its length is the last two bytes and it
/// starts with a CBOR map. Without a plausible trailer, the code is whole.
pub fn strip_metadata(code: &[u8]) -> &[u8] {
    if code.len() < 2 {
        return code;
    }
    let len = usize::from(u16::from_be_bytes([
        code[code.len() - 2],
        code[code.len() - 1],
    ]));
    match code.len().checked_sub(len + 2) {
        Some(start) if len > 0 && (0xa0..=0xbf).contains(&code[start]) => &code[..start],
        _ => code,
    }
}

/// A linear opcode sweep of runtime code (metadata stripped): the small push
/// immediates a dispatcher compares selectors with, the 32-byte immediates
/// (event topics, immutables) and the contract-creating instructions. A
/// sweep also walks embedded data; its facts are supporting evidence.
#[derive(Debug, Default)]
pub struct CodeScan {
    /// Value of every PUSH1..PUSH4 immediate -> the widths it was pushed with.
    small: BTreeMap<u32, BTreeSet<u8>>,
    words: BTreeSet<[u8; 32]>,
    addresses: BTreeSet<Address>,
    pub create: bool,
    pub create2: bool,
}

impl CodeScan {
    pub fn new(code: &[u8]) -> Self {
        let code = strip_metadata(code);
        let mut scan = Self::default();
        let mut pc = 0;
        while pc < code.len() {
            let op = code[pc];
            if (0x60..=0x7f).contains(&op) {
                let width = usize::from(op - 0x5f);
                let end = (pc + 1 + width).min(code.len());
                let imm = &code[pc + 1..end];
                if imm.len() == width {
                    if width <= 4 {
                        let mut be = [0u8; 4];
                        be[4 - width..].copy_from_slice(imm);
                        scan.small
                            .entry(u32::from_be_bytes(be))
                            .or_default()
                            .insert(width as u8);
                    } else if width == 20 {
                        scan.addresses.insert(Address::from_slice(imm));
                    } else if width == 32 {
                        let mut w = [0u8; 32];
                        w.copy_from_slice(imm);
                        scan.words.insert(w);
                        if w[..12] == [0u8; 12] {
                            scan.addresses.insert(Address::from_slice(&w[12..]));
                        }
                    }
                }
                pc = end;
            } else {
                match op {
                    0xf0 => scan.create = true,
                    0xf5 => scan.create2 = true,
                    _ => {}
                }
                pc += 1;
            }
        }
        scan
    }

    /// How a dispatcher holds `selector`: `PUSH4_PRESENT`, a shorter push of
    /// a selector with leading zero bytes, or `ABSENT`.
    pub fn selector(&self, selector: [u8; 4]) -> String {
        let value = u32::from_be_bytes(selector);
        let minimal = 4 - selector.iter().take_while(|b| **b == 0).count().min(3);
        match self.small.get(&value) {
            Some(widths) if widths.contains(&4) => "PUSH4_PRESENT".into(),
            Some(widths) if minimal < 4 && widths.contains(&(minimal as u8)) => {
                format!("PUSH{minimal}_PRESENT")
            }
            _ => "ABSENT".into(),
        }
    }

    pub fn has_word(&self, word: B256) -> bool {
        self.words.contains(&word.0)
    }

    /// `address` as a PUSH20 immediate or right-aligned in a PUSH32 (an
    /// immutable).
    pub fn embeds(&self, address: Address) -> bool {
        self.addresses.contains(&address)
    }

    /// Every 20-byte value the code pushes as an address (PUSH20, or
    /// right-aligned in a PUSH32), in ascending order.
    pub fn addresses(&self) -> impl Iterator<Item = &Address> {
        self.addresses.iter()
    }

    pub fn create_opcodes(&self) -> String {
        format!(
            "CREATE={} CREATE2={} (linear sweep, metadata stripped)",
            self.create, self.create2
        )
    }
}

// ---- EIP-1167 clones and CREATE2 ---------------------------------------------

/// EIP-1167 minimal proxy runtime: prefix, 20-byte target, suffix.
const EIP1167_RUNTIME_PREFIX: [u8; 10] = hex!("363d3d373d3d3d363d73");
const EIP1167_RUNTIME_SUFFIX: [u8; 15] = hex!("5af43d82803e903d91602b57fd5bf3");
/// The creation code that returns the 45-byte runtime.
const EIP1167_CREATION_PREFIX: [u8; 10] = hex!("3d602d80600a3d3981f3");

/// The target of an EIP-1167 minimal proxy runtime.
pub fn eip1167_target(code: &[u8]) -> Option<Address> {
    (code.len() == 45
        && code[..10] == EIP1167_RUNTIME_PREFIX
        && code[30..] == EIP1167_RUNTIME_SUFFIX)
        .then(|| Address::from_slice(&code[10..30]))
}

/// The EIP-1167 creation code of a clone of `template`.
pub fn clone_init_code(template: Address) -> Vec<u8> {
    let mut code = Vec::with_capacity(55);
    code.extend_from_slice(&EIP1167_CREATION_PREFIX);
    code.extend_from_slice(&EIP1167_RUNTIME_PREFIX);
    code.extend_from_slice(template.as_slice());
    code.extend_from_slice(&EIP1167_RUNTIME_SUFFIX);
    code
}

/// The CREATE2 address of a clone of `template` deployed by `deployer`
/// with `salt` (EIP-1014 through alloy), and the init code hash.
pub fn predict_clone(deployer: Address, salt: B256, template: Address) -> (B256, Address) {
    let init_hash = keccak256(clone_init_code(template));
    (init_hash, deployer.create2(salt, init_hash))
}

pub fn hex_bytes(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

pub fn addr(a: Address) -> String {
    hex_bytes(a.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dispatcher_sweep_finds_push4_and_short_pushes_only() {
        // PUSH4 519ebb10, PUSH3 00fdd58e as 0xfdd58e, a PUSH32 holding
        // 87f27655 inside (data, not a PUSH4), CREATE2, then metadata.
        let mut code = hex!("63519ebb10" "62fdd58e" "7f87f2765500000000000000000000000000000000000000000000000000000000" "f5").to_vec();
        let metadata = hex!(
            "a2646970667358221220f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f064736f6c63430008140033"
        );
        code.extend_from_slice(&metadata[..metadata.len() - 2]);
        let len = (metadata.len() - 2) as u16;
        code.extend_from_slice(&len.to_be_bytes());
        let scan = CodeScan::new(&code);
        assert_eq!(scan.selector(hex!("519ebb10")), "PUSH4_PRESENT");
        assert_eq!(scan.selector(hex!("00fdd58e")), "PUSH3_PRESENT");
        assert_eq!(scan.selector(hex!("87f27655")), "ABSENT");
        assert_eq!(scan.selector(hex!("fdd58e00")), "ABSENT");
        assert!(scan.create2 && !scan.create);
        // The f0 bytes of the metadata are not instructions.
        assert!(!CodeScan::new(&code).create);
        // A truncated push at the end is not an immediate.
        assert_eq!(
            CodeScan::new(&hex!("63519ebb")).selector(hex!("519ebb10")),
            "ABSENT"
        );
    }

    #[test]
    fn a_clone_runtime_names_its_template_and_create2_is_alloys() {
        let template = Address::from(hex!("7330d8865f4b6800b72bdd73e2007833a5d45c94"));
        let runtime = hex!(
            "363d3d373d3d3d363d737330d8865f4b6800b72bdd73e2007833a5d45c945af43d82803e903d91602b57fd5bf3"
        );
        assert_eq!(eip1167_target(&runtime), Some(template));
        assert_eq!(eip1167_target(&runtime[..44]), None);
        assert_eq!(&clone_init_code(template)[10..], &runtime[..]);
        // EIP-1014 example 1: deployer 0, salt 0, init code 0x00.
        assert_eq!(
            Address::ZERO
                .create2(B256::ZERO, keccak256([0u8]))
                .to_string()
                .to_lowercase(),
            "0x4d1a2e2bb4f88f0250f26ffff098b0b30b26bf38"
        );
    }
}
