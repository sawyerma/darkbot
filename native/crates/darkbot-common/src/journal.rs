//! Journal-/Replay-Contract (S0-04).
//!
//! Ein Journal ist eine lückenlose Sequenz von Rohereignissen mit Provenienz
//! (Sequenznummer, T_rx, Quelle). Replay liefert dieselbe Sequenz zurück.
//! Lücken sind Fehler, nie stille Sprünge. Die Speicherimplementierung
//! (Datei/Segmente) kommt mit S1; hier nur Contract + In-Memory-Referenz.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Streng monoton, beginnend bei 0, ohne Lücken.
    pub seq: u64,
    /// Monotoner Empfangszeitpunkt (Clock::now_ns) — T_rx.
    pub t_rx_ns: u64,
    /// Kennung der Quelle/Adapters (z. B. 1 = WsRpcControlSource).
    pub source: u16,
    /// Ereignistyp (adapterdefiniert; 0 reserviert).
    pub kind: u16,
    /// Rohpayload, unverändert.
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JournalError {
    SeqGap { expected: u64, got: u64 },
    Io(String),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalError::SeqGap { expected, got } => {
                write!(f, "journal sequence gap: expected {} got {}", expected, got)
            }
            JournalError::Io(s) => write!(f, "journal io: {}", s),
        }
    }
}

impl std::error::Error for JournalError {}

pub trait Journal {
    fn append(&mut self, record: Record) -> Result<(), JournalError>;
    fn next_seq(&self) -> u64;
}

pub trait Replay {
    /// Nächstes Ereignis in Journal-Reihenfolge; `None` am Ende.
    fn next(&mut self) -> Option<Record>;
}

/// In-Memory-Referenz für Tests und Parity-Läufe.
#[derive(Debug, Default)]
pub struct MemJournal {
    records: Vec<Record>,
}

impl MemJournal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn replay(&self) -> MemReplay<'_> {
        MemReplay { records: &self.records, pos: 0 }
    }
}

impl Journal for MemJournal {
    fn append(&mut self, record: Record) -> Result<(), JournalError> {
        let expected = self.next_seq();
        if record.seq != expected {
            return Err(JournalError::SeqGap { expected, got: record.seq });
        }
        self.records.push(record);
        Ok(())
    }

    fn next_seq(&self) -> u64 {
        self.records.len() as u64
    }
}

pub struct MemReplay<'a> {
    records: &'a [Record],
    pos: usize,
}

impl<'a> Replay for MemReplay<'a> {
    fn next(&mut self) -> Option<Record> {
        let r = self.records.get(self.pos).cloned();
        if r.is_some() {
            self.pos += 1;
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u64) -> Record {
        Record { seq, t_rx_ns: seq * 10, source: 1, kind: 1, payload: vec![seq as u8] }
    }

    #[test]
    fn append_and_replay_in_order() {
        let mut j = MemJournal::new();
        j.append(rec(0)).unwrap();
        j.append(rec(1)).unwrap();
        let mut r = j.replay();
        assert_eq!(r.next().map(|x| x.seq), Some(0));
        assert_eq!(r.next().map(|x| x.seq), Some(1));
        assert_eq!(r.next(), None);
    }

    #[test]
    fn gap_is_rejected() {
        let mut j = MemJournal::new();
        j.append(rec(0)).unwrap();
        assert_eq!(
            j.append(rec(2)),
            Err(JournalError::SeqGap { expected: 1, got: 2 })
        );
        assert_eq!(j.len(), 1, "fehlerhafter Append darf nichts schreiben");
    }
}
