//! Re-determines the builder gas-floor policy of the G0.7 attempt-1
//! candidate from the official public documentation pages and writes the
//! native builder-policy record (DBREC001, no JSON). Read-only: plain HTTPS
//! GETs of documentation pages; no builder API, no bundle, no account, no
//! secret.
//!
//! Nothing is recalculated: the pages must still state what the frozen
//! policy bound (BlockRazor's explicit bundle floor, read from its rule
//! sentence and equal to the frozen floor; no explicit 48Club minimum and
//! its fee-based ranking rule). Any other finding is a STOP for a manual
//! re-determination and nothing is written. Each source is recorded with
//! its fetch time, HTTP status and the SHA-256 of the body read, and the
//! record's freshness is the fetch time of its oldest source.
//!
//! usage: g06a_builder_policy_record <frozen_builder_policy.dbrec> <out.dbrec>
//! (run from the repository root; exit 0 written, 1 STOP, 2 usage)

use darkbot_bsc_record::schema::g06a::builder_policy as b;
use darkbot_bsc_record::schema::{PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL, decode_any};
use darkbot_bsc_record::{Draft, Value, sha256};
use std::io::Write;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const BLOCKRAZOR: &str = "https://blockrazor.io/support/";
/// The 48Club documentation is rendered by Next.js; its server-component
/// payload (`RSC: 1`) carries the documentation text verbatim.
const FORTYEIGHTCLUB: [&str; 3] = [
    "https://web3.48.club/builder",
    "https://web3.48.club/docs/mev-builder",
    "https://web3.48.club/docs/mev-builder/bundle-status",
];
const BLOCKRAZOR_RULE: &str = "the average gas price of a bundle must not be less than ";
const FORTYEIGHTCLUB_RANKING: &str =
    "sum of included tx gas fees after adjustments plus direct transfers";
/// Any of these in a 48Club page may document a gas-price requirement.
const FLOOR_TERMS: [&str; 5] = ["gwei", "minimum", "min gas", "floor", "lowest gas"];

struct Page {
    url: &'static str,
    status: u64,
    fetched_unix_ms: u64,
    body: Vec<u8>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn fetch(url: &'static str, rsc: bool) -> Result<Page, String> {
    let mut curl = Command::new("curl");
    // -q first: no .curlrc. HTTPS only, also across redirects.
    curl.env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .args(["-q", "-sS", "-L", "--proto", "=https", "--proto-redir", "=https"])
        .args(["--max-time", "30", "--write-out", "\n%{http_code}"]);
    if rsc {
        curl.args(["-H", "RSC: 1"]);
    }
    let out = curl
        .arg(url)
        .output()
        .map_err(|e| format!("execute curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("{url}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let split = out
        .stdout
        .iter()
        .rposition(|b| *b == b'\n')
        .ok_or_else(|| format!("{url}: no status"))?;
    let status = std::str::from_utf8(&out.stdout[split + 1..])
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(|| format!("{url}: no status"))?;
    Ok(Page {
        url,
        status,
        fetched_unix_ms: now_ms(),
        body: out.stdout[..split].to_vec(),
    })
}

/// `raw` without its `<script>` and `<style>` blocks (structured data and
/// code are not the page's visible statement).
fn without_blocks(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    loop {
        let lower = rest.to_ascii_lowercase();
        let open = ["<script", "<style"].iter().filter_map(|t| lower.find(t).map(|i| (i, *t))).min();
        let Some((at, tag)) = open else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..at]);
        let close = format!("</{}>", &tag[1..]);
        match lower[at..].find(&close) {
            Some(end) => rest = &rest[at + end + close.len()..],
            None => return out,
        }
    }
}

/// The visible text of an HTML page or component payload: script and style
/// blocks and tags removed, common entities decoded, whitespace collapsed.
fn text(body: &[u8]) -> String {
    let raw = without_blocks(&String::from_utf8_lossy(body));
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `0.05` gwei as wei (at most nine decimals).
fn gwei_to_wei(number: &str) -> Option<u128> {
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty()
        || fraction.len() > 9
        || !whole.bytes().chain(fraction.bytes()).all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let scaled = format!("{whole}{fraction:0<9}");
    scaled.parse().ok()
}

/// BlockRazor's floor sentence and the floor it states, in wei.
fn blockrazor_floor(page_text: &str) -> Result<(String, u128), String> {
    let lower = page_text.to_lowercase();
    let at = lower
        .find(BLOCKRAZOR_RULE)
        .ok_or("BlockRazor no longer states its bundle gas-price rule in the checked wording")?;
    let rest = &page_text[at + BLOCKRAZOR_RULE.len()..];
    let number = rest.split_whitespace().next().unwrap_or("");
    let unit: String = rest
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_alphabetic())
        .collect();
    if !unit.eq_ignore_ascii_case("gwei") {
        return Err(format!("BlockRazor states its floor in {unit:?}, not gwei"));
    }
    let wei = gwei_to_wei(number).ok_or_else(|| format!("BlockRazor floor {number:?} is not a number"))?;
    let sentence = format!("{}{number} gwei", &page_text[at..at + BLOCKRAZOR_RULE.len()]);
    Ok((sentence, wei))
}

/// A 48Club page's finding: no documented gas-price requirement, and on the
/// bundle-status page the fee-based ranking rule.
fn fortyeightclub_finding(page: &Page) -> Result<String, String> {
    let lower = text(&page.body).to_lowercase();
    if let Some(term) = FLOOR_TERMS.iter().find(|t| lower.contains(*t)) {
        return Err(format!(
            "{} mentions {term:?}: a possible 48Club gas-price requirement needs a manual review",
            page.url
        ));
    }
    if page.url.ends_with("/bundle-status") {
        if !lower.contains(FORTYEIGHTCLUB_RANKING) {
            return Err(format!("{} no longer states the fee-based ranking rule", page.url));
        }
        return Ok(format!(
            "ranking: priority is the \"{FORTYEIGHTCLUB_RANKING}\"; no gas-price minimum documented"
        ));
    }
    Ok("no gas-price minimum documented".to_string())
}

/// `YYYY-MM-DD` (UTC) of a Unix time in milliseconds.
fn utc_date(unix_ms: u64) -> String {
    let days = (unix_ms / 86_400_000) as i64;
    // Civil date from days since 1970-01-01 (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn word(x: u128) -> Value {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&x.to_be_bytes());
    Value::U256(w)
}

fn u128_of(w: [u8; 32]) -> Option<u128> {
    w[..16].iter().all(|b| *b == 0).then(|| u128::from_be_bytes(w[16..].try_into().unwrap()))
}

fn run(frozen_path: &str, out_path: &str) -> Result<String, String> {
    let frozen_bytes = std::fs::read(frozen_path).map_err(|e| format!("{frozen_path}: {e}"))?;
    let frozen = decode_any(&frozen_bytes).map_err(|e| format!("{frozen_path}: {e}"))?;
    if frozen.schema() != &b::SCHEMA {
        return Err(format!("{frozen_path} is not the frozen builder-policy import"));
    }
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    let frozen_floor = u128_of(frozen.u256(b::STRICTEST_FLOOR_WEI).map_err(e)?)
        .ok_or("frozen floor exceeds 128 bits")?;
    let stage1 = u128_of(frozen.u256(b::STAGE1_GAS_PRICE_WEI).map_err(e)?)
        .ok_or("frozen stage-1 gas price exceeds 128 bits")?;

    let mut pages = vec![fetch(BLOCKRAZOR, false)?];
    for url in FORTYEIGHTCLUB {
        pages.push(fetch(url, true)?);
    }
    if let Some(p) = pages.iter().find(|p| p.status != 200 || p.body.is_empty()) {
        return Err(format!("{} answered HTTP {} ({} bytes)", p.url, p.status, p.body.len()));
    }
    let (rule, floor) = blockrazor_floor(&text(&pages[0].body))?;
    if floor != frozen_floor {
        return Err(format!(
            "BlockRazor now states a floor of {floor} wei, the frozen policy binds {frozen_floor} wei: a relevant parameter changed, new economic decision required"
        ));
    }
    if stage1 < floor {
        return Err(format!("frozen stage-1 gas price {stage1} is below the floor {floor}"));
    }
    let mut findings = vec![format!("states: \"{rule}\"")];
    for page in &pages[1..] {
        findings.push(fortyeightclub_finding(page)?);
    }
    let oldest = pages.iter().map(|p| p.fetched_unix_ms).min().unwrap_or(0);
    let date = utc_date(oldest);

    let text_of = |tag| frozen.text(tag).map(str::to_string).map_err(e);
    let mut d = Draft::new(&b::NATIVE);
    d.set(PROVENANCE_TOOL, "g06a_builder_policy_record")
        .set(PROVENANCE_OBSERVED_UNIX_MS, now_ms())
        .set(b::LEGACY_SCHEMA, b::NATIVE.name)
        .set(b::OBSERVED_DATE, date.as_str())
        .set(
            12,
            "G0.7 attempt-1 requalification (Portal v5.25.0): builder floor/ranking re-determined from the official documentation",
        )
        .set(b::STATUS, "DRY3_POLICY_READY")
        .set(b::BLOCKRAZOR_VERIFIED, true)
        .set(21, word(floor))
        .set(22, rule.as_str())
        .set(23, BLOCKRAZOR)
        .set(24, date.as_str())
        .set(b::FORTYEIGHTCLUB_VERIFIED, true)
        .set(31, false)
        .set(
            33,
            format!(
                "Bundle-status documentation: priority is the \"{FORTYEIGHTCLUB_RANKING}\". The MEV builder documentation states no explicit minimum bundle gas price."
            ),
        )
        .set(35, date.as_str())
        .set(36, text_of(36)?)
        .set(b::STRICTEST_FLOOR_WEI, word(floor))
        .set(41, "BlockRazor")
        .set(b::STAGE1_GAS_PRICE_WEI, word(stage1))
        .set(
            43,
            format!(
                "Unchanged frozen attempt-1 policy: {stage1} wei is not below the strictest explicit verified floor (BlockRazor, {floor} wei), re-determined from the official documentation on {date}. A DarkBot signed-gas policy, not a claimed 48Club minimum."
            ),
        )
        .set(b::DRY3_READY, true);
    for url in FORTYEIGHTCLUB {
        d.push(34, url);
    }
    for claim in frozen.texts(45).map_err(e)? {
        d.push(45, claim);
    }
    // The 2026-10-11 minimum-spend probe is historical and dated; carried as is.
    for tag in 50..=54u16 {
        for (t, value) in frozen.fields() {
            if *t == tag {
                d.push(tag, value.clone());
            }
        }
    }
    for (page, finding) in pages.iter().zip(&findings) {
        let mut s = Draft::new(&b::source::SCHEMA);
        s.set(b::source::URL, page.url)
            .set(b::source::HTTP_STATUS, page.status)
            .set(b::source::FETCHED_UNIX_MS, page.fetched_unix_ms)
            .set(b::source::BODY_SHA256, Value::B256(sha256(&page.body)))
            .set(b::source::BODY_BYTES, page.body.len() as u64)
            .set(b::source::FINDING, finding.as_str());
        d.push(b::SOURCES, s.into_nested());
    }
    let record = d.encode().map_err(|x| x.0)?;
    let view = darkbot_bsc_record::decode(&b::NATIVE, &record).map_err(e)?.render();
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out_path)
        .and_then(|mut f| f.write_all(&record).and_then(|()| f.sync_all()))
        .map_err(|x| format!("create-new {out_path}: {x}"))?;
    Ok(view)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [frozen, out] = args.as_slice() else {
        eprintln!("usage: g06a_builder_policy_record <frozen_builder_policy.dbrec> <out.dbrec>");
        return ExitCode::from(2);
    };
    match run(frozen, out) {
        Ok(view) => {
            print!("{view}");
            println!("BUILDER_POLICY=RE-DETERMINED");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("STOP: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blockrazor_floor_is_read_from_its_rule_and_never_assumed() {
        let page = "<script type=\"application/ld+json\">{\"q\":\"the average gas price of a bundle must not be less than 9 gwei?\"}</script><p>How to understand that the average gas price of a bundle must not be less than 0.05 gwei? Assume</p>";
        let (rule, wei) = blockrazor_floor(&text(page.as_bytes())).unwrap();
        assert_eq!(wei, 50_000_000);
        assert_eq!(rule, "the average gas price of a bundle must not be less than 0.05 gwei");
        let raised = page.replace("0.05", "0.1");
        assert_eq!(blockrazor_floor(&text(raised.as_bytes())).unwrap().1, 100_000_000);
        assert!(blockrazor_floor("bundles are ranked by fees").is_err());
        assert!(blockrazor_floor("must not be less than 0.05 bnb").is_err());
        assert_eq!(gwei_to_wei("1"), Some(1_000_000_000));
        assert_eq!(gwei_to_wei("0.0000000001"), None);
        assert_eq!(gwei_to_wei("x"), None);
    }

    #[test]
    fn a_48club_page_that_may_state_a_requirement_stops() {
        let page = |url, body: &str| Page { url, status: 200, fetched_unix_ms: 0, body: body.as_bytes().to_vec() };
        let ranking = format!("priority (the {FORTYEIGHTCLUB_RANKING}).");
        assert!(fortyeightclub_finding(&page(FORTYEIGHTCLUB[2], &ranking)).is_ok());
        assert!(fortyeightclub_finding(&page(FORTYEIGHTCLUB[2], "priority by fees")).is_err());
        let floor = format!("{ranking} Bundles need at least 1 Gwei.");
        assert!(fortyeightclub_finding(&page(FORTYEIGHTCLUB[2], &floor)).is_err());
        assert!(fortyeightclub_finding(&page(FORTYEIGHTCLUB[1], "a minimum of 0.1")).is_err());
        assert!(fortyeightclub_finding(&page(FORTYEIGHTCLUB[1], "atomic bundles")).is_ok());
    }

    #[test]
    fn dates_are_utc_calendar_days() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(951_782_400_000), "2000-02-29");
        assert_eq!(utc_date(1_792_241_229_533), "2026-10-17");
    }
}
