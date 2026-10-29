//! DEC-08: the one signature-history reader of the INDEX and CROSSCHECK verifiers and of the
//! historical winner consumer. `getSignaturesForAddress` returns an address's signatures newest
//! first, at most `PAGE_LIMIT` per page, continued by the `before` cursor. The pager owns the
//! cursor, the page layout, the descending slot order, the de-duplication (a repeated signature
//! must be identical, else the history is inconsistent) and the end of the history: the caller's
//! stop rule over a whole page, or an empty page (the history is exhausted).
//!
//! Transport-agnostic: the caller fetches the page the pager names — its budgets, pacing,
//! 429/Retry-After handling and typed cache are the transport's — and hands it back decoded
//! (`chain::decode_signature_page`, the one wire decoder).
use darkbot_solana::chain::SignatureEntry;
use std::collections::HashMap;

/// The provider maximum of one `getSignaturesForAddress` page (the RPC API's `limit` bound).
pub const PAGE_LIMIT: usize = 1000;

/// How a page ended the reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageEnd {
    /// More history follows at `before()`.
    More,
    /// The stop rule held for this page (kept in full).
    Stopped,
    /// An empty page: the address has no older signatures.
    Exhausted,
}

/// The cursor and the accepted rows of one signature history.
#[derive(Debug, Default)]
pub struct SignaturePager {
    before: Option<String>,
    last_slot: Option<u64>,
    pages: u64,
    rows: Vec<SignatureEntry>,
    index: HashMap<String, usize>,
    end: Option<PageEnd>,
}

impl SignaturePager {
    pub fn new() -> Self {
        Self::default()
    }
    /// The cursor of the next page (`None`: the newest page).
    pub fn before(&self) -> Option<&str> {
        self.before.as_deref()
    }
    /// Pages accepted so far.
    pub fn pages(&self) -> u64 {
        self.pages
    }
    /// The accepted rows in history order (newest first), each signature once.
    pub fn rows(&self) -> &[SignatureEntry] {
        &self.rows
    }
    /// How the history ended; `None` while more pages may follow.
    pub fn end(&self) -> Option<PageEnd> {
        self.end
    }
    /// Accepts the page fetched at `before()`. Its rows must not rise in slot (also across pages);
    /// a signature seen before (on this or an earlier page) must repeat identically
    /// (`INDEX_DUPLICATE_CONFLICT`); the page must
    /// move the cursor (`INDEX_CURSOR_STALLED`) and hold at most `PAGE_LIMIT` rows
    /// (`INDEX_PAGE_LAYOUT`). `stop(page)` ends the history after the whole page is kept.
    pub fn accept(
        &mut self,
        page: Vec<SignatureEntry>,
        stop: impl Fn(&[SignatureEntry]) -> bool,
    ) -> Result<PageEnd, &'static str> {
        if self.end.is_some() {
            return Err("INDEX_PAGE_AFTER_END");
        }
        if page.len() > PAGE_LIMIT {
            return Err("INDEX_PAGE_LAYOUT");
        }
        let Some(last) = page.last() else {
            self.end = Some(PageEnd::Exhausted);
            return Ok(PageEnd::Exhausted);
        };
        if self.before.as_deref() == Some(last.signature.as_str()) {
            return Err("INDEX_CURSOR_STALLED");
        }
        let mut previous = self.last_slot;
        let mut seen: HashMap<&str, &SignatureEntry> = HashMap::new();
        for row in &page {
            if seen
                .insert(row.signature.as_str(), row)
                .is_some_and(|first| first != row)
            {
                return Err("INDEX_DUPLICATE_CONFLICT");
            }
            if previous.is_some_and(|p| row.slot > p) {
                return Err("INDEX_SLOT_ORDER");
            }
            previous = Some(row.slot);
            if let Some(at) = self.index.get(&row.signature) {
                if self.rows[*at] != *row {
                    return Err("INDEX_DUPLICATE_CONFLICT");
                }
            }
        }
        let next = last.signature.clone();
        let stopped = stop(&page);
        for row in page {
            if !self.index.contains_key(&row.signature) {
                self.index.insert(row.signature.clone(), self.rows.len());
                self.rows.push(row);
            }
        }
        self.last_slot = previous;
        self.before = Some(next);
        self.pages += 1;
        let end = if stopped {
            PageEnd::Stopped
        } else {
            PageEnd::More
        };
        if end == PageEnd::Stopped {
            self.end = Some(end);
        }
        Ok(end)
    }
}

/// Tests reading through the process's one provider budget (`provider::shared()`, History at
/// its default 2 requests/s for the whole test binary) hold this lock, so their reads never queue
/// behind each other past the read timeout (flake `RPC_ADMISSION_BUDGET` of a crosscheck source).
#[cfg(test)]
pub(crate) fn shared_budget_tests() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_solana::chain::TxStatus;

    fn row(signature: &str, slot: u64, block_time: Option<i64>) -> SignatureEntry {
        SignatureEntry {
            signature: signature.into(),
            slot,
            block_time,
            status: TxStatus::Success,
        }
    }

    /// DEC-08: pages continue at the last signature, rows stay newest first across pages, a
    /// repeat is kept once, and the stop rule ends the history after its whole page.
    #[test]
    fn pages_continue_at_the_cursor_and_stop_after_the_page() {
        let mut p = SignaturePager::new();
        assert_eq!(p.before(), None);
        let never = |_: &[SignatureEntry]| false;
        assert_eq!(
            p.accept(vec![row("a", 9, Some(90)), row("b", 8, Some(80))], never),
            Ok(PageEnd::More)
        );
        assert_eq!(p.before(), Some("b"));
        let crossed = |page: &[SignatureEntry]| page.iter().all(|r| r.block_time < Some(75));
        assert_eq!(
            p.accept(vec![row("b", 8, Some(80)), row("c", 7, Some(70))], crossed),
            Ok(PageEnd::More),
            "b is still at 80"
        );
        assert_eq!(
            p.accept(vec![row("d", 6, Some(60)), row("e", 5, Some(50))], crossed),
            Ok(PageEnd::Stopped)
        );
        let kept: Vec<&str> = p.rows().iter().map(|r| r.signature.as_str()).collect();
        assert_eq!(kept, ["a", "b", "c", "d", "e"]);
        assert_eq!((p.pages(), p.end()), (3, Some(PageEnd::Stopped)));
        assert_eq!(
            p.accept(vec![row("f", 4, None)], never),
            Err("INDEX_PAGE_AFTER_END")
        );
    }

    /// DEC-08: an empty page exhausts the history; a rising slot, a repeat that differs, a
    /// stalled cursor and an oversized page fail closed (the provider's history is unreliable).
    #[test]
    fn an_inconsistent_history_fails_closed() {
        let never = |_: &[SignatureEntry]| false;
        let mut p = SignaturePager::new();
        assert_eq!(p.accept(vec![], never), Ok(PageEnd::Exhausted));
        let mut p = SignaturePager::new();
        p.accept(vec![row("a", 9, Some(90))], never).unwrap();
        assert_eq!(
            p.accept(vec![row("b", 10, Some(91))], never),
            Err("INDEX_SLOT_ORDER")
        );
        assert_eq!(
            p.accept(vec![row("a", 9, Some(91)), row("x", 7, None)], never),
            Err("INDEX_DUPLICATE_CONFLICT")
        );
        assert_eq!(
            p.accept(vec![row("a", 9, Some(90))], never),
            Err("INDEX_CURSOR_STALLED")
        );
        // A repeat inside one page must be identical too (an identical one is kept once).
        let mut q = SignaturePager::new();
        let mut failed_a = row("a", 9, Some(90));
        failed_a.status = TxStatus::Unknown("X");
        assert_eq!(
            q.accept(vec![row("a", 9, Some(90)), failed_a], never),
            Err("INDEX_DUPLICATE_CONFLICT")
        );
        assert_eq!(
            q.accept(vec![row("a", 9, Some(90)), row("a", 9, Some(90))], never),
            Ok(PageEnd::More)
        );
        assert_eq!(q.rows().len(), 1);
        let big: Vec<SignatureEntry> = (0..=PAGE_LIMIT as u64)
            .map(|i| row(&format!("s{i}"), 8, None))
            .collect();
        assert_eq!(p.accept(big, never), Err("INDEX_PAGE_LAYOUT"));
        assert_eq!(p.rows().len(), 1, "a refused page keeps nothing");
    }
}
