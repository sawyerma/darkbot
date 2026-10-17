//! Program-attributed liquidation event decoding.
use super::*;

// ------------------------------------------------------------------------------------------
// LiquidationReceiverEvent decoder (fail closed, program-attributed)
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReceiverEvent {
    pub event_index: u32,
    pub target: Pubkey,
    pub receiver: Pubkey,
    pub seized_equity_usd: f64,
    pub repaid_equity_usd: f64,
    pub protocol_flat_fee_lamports: u32,
}

impl ReceiverEvent {
    pub fn gross_edge_usd(&self) -> f64 {
        self.seized_equity_usd - self.repaid_equity_usd
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogDecode {
    pub events: Vec<ReceiverEvent>,
    /// Other liquidation types decoded provisionally from the same attributed logs.
    pub additional_events: Vec<truth::ProvisionalLogEvent>,
    /// Marginfi-emitted data that could hide a receiver event: receiver discriminator with
    /// too-short/non-finite payload, or undecodable base64 while Marginfi is executing.
    pub decoder_errors: u64,
    /// Receiver-event-shaped data not emitted by the Marginfi program itself (spoof protection).
    pub foreign_program_events: u64,
    /// Undecodable `Program data:` lines emitted by other programs.
    pub base64_errors: u64,
    /// The runtime truncated the log; later events may be missing.
    pub truncated: bool,
    /// Every decoded event in log order: its position is the canonical classifier's
    /// `event_index` (report entity identity across live and strict evidence).
    pub sequence: Vec<LogEventRef>,
}

/// One decoded event of `LogDecode`: index into `events` or `additional_events`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogEventRef {
    Receiver(usize),
    Additional(usize),
}

pub(super) enum Payload {
    NotReceiverEvent,
    Invalid,
    Event {
        target: Pubkey,
        receiver: Pubkey,
        seized: f64,
        repaid: f64,
        flat_fee: u32,
    },
}

pub(super) fn parse_receiver_payload(b: &[u8]) -> Payload {
    if b.len() < 8 || b[..8] != LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR {
        return Payload::NotReceiverEvent;
    }
    if b.len() < LIQUIDATION_RECEIVER_EVENT_LEN {
        return Payload::Invalid;
    }
    let mut target = [0u8; 32];
    target.copy_from_slice(&b[8..40]);
    let mut receiver = [0u8; 32];
    receiver.copy_from_slice(&b[40..72]);
    let mut seized = [0u8; 8];
    seized.copy_from_slice(&b[72..80]);
    let mut repaid = [0u8; 8];
    repaid.copy_from_slice(&b[80..88]);
    let mut fee = [0u8; 4];
    fee.copy_from_slice(&b[88..92]);
    let (seized, repaid) = (f64::from_le_bytes(seized), f64::from_le_bytes(repaid));
    if !seized.is_finite() || !repaid.is_finite() {
        return Payload::Invalid;
    }
    Payload::Event {
        target: Pubkey::new_from_array(target),
        receiver: Pubkey::new_from_array(receiver),
        seized,
        repaid,
        flat_fee: u32::from_le_bytes(fee),
    }
}

/// Decode `LiquidationReceiverEvent`s from log messages. An event counts only when the
/// `Program data:` line was emitted while `program` is the currently executing program
/// (invoke/success/failed stack), so another program cannot inject fake receiver events.
pub fn decode_receiver_events(logs: &[String], program: &Pubkey) -> LogDecode {
    let program_s = program.to_string();
    let mut out = LogDecode::default();
    let mut stack: Vec<&str> = Vec::new();
    for line in logs {
        if line.starts_with("Log truncated") {
            out.truncated = true;
            break;
        }
        if let Some(data) = line.strip_prefix(PROGRAM_DATA_PREFIX) {
            let program_executing = stack.last().copied() == Some(program_s.as_str());
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data.trim()) else {
                if program_executing {
                    out.decoder_errors += 1;
                } else {
                    out.base64_errors += 1;
                }
                continue;
            };
            match parse_receiver_payload(&bytes) {
                Payload::NotReceiverEvent if program_executing => {
                    // Reuse the existing canonical event decoder: no second RPC scanner.
                    match truth::provisional_event(&bytes) {
                        Ok(Some(ev)) => {
                            out.sequence
                                .push(LogEventRef::Additional(out.additional_events.len()));
                            out.additional_events.push(ev);
                        }
                        Ok(None) => {}
                        Err(_) => out.decoder_errors += 1,
                    }
                }
                Payload::NotReceiverEvent => {}
                Payload::Invalid if program_executing => out.decoder_errors += 1,
                Payload::Invalid => out.foreign_program_events += 1,
                Payload::Event {
                    target,
                    receiver,
                    seized,
                    repaid,
                    flat_fee,
                } => {
                    if !program_executing {
                        out.foreign_program_events += 1;
                        continue;
                    }
                    let event_index = out.events.len() as u32;
                    out.sequence.push(LogEventRef::Receiver(out.events.len()));
                    out.events.push(ReceiverEvent {
                        event_index,
                        target,
                        receiver,
                        seized_equity_usd: seized,
                        repaid_equity_usd: repaid,
                        protocol_flat_fee_lamports: flat_fee,
                    });
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("Program ") {
            let mut parts = rest.split_whitespace();
            let (Some(id), Some(word)) = (parts.next(), parts.next()) else {
                continue;
            };
            if word == "invoke" {
                stack.push(id);
            } else if (word == "success" || word.starts_with("failed"))
                && stack.last().copied() == Some(id)
            {
                stack.pop();
            }
        }
    }
    out
}
