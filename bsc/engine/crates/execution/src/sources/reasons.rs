//! The owner's refusal and blocking reasons, and the LIVE conditions and their
//! scope text.

pub const ECONOMIC_LIVE_NOT_AUTHORIZED: &str = "ECONOMIC_LIVE_NOT_AUTHORIZED";
/// A requested LIVE while the global economic LIVE switch is not ENABLED:
/// the first missing condition (plan v9 §2.5).
pub const ECONOMIC_LIVE_DISABLED: &str = "ECONOMIC_LIVE_DISABLED";
/// C2 may only run operationally after a real G0.8 acquisition PASS and the
/// evaluated G0.4 measurement (PHASENPLAN §10 offline override). R4: a
/// requested C2 LIVE stays BLOCKED; the economic LIVE path reads C2 in
/// SHADOW (its nonces and inclusions), its authority is the grant with the
/// LIVE acquisition (G0.8) and source (G0.4) conditions.
pub const C2_OPERATIONAL_NOT_AUTHORIZED: &str = "C2_OPERATIONAL_NOT_AUTHORIZED";
pub const SOURCE_ACCESS_UNQUALIFIED: &str = "SOURCE_ACCESS_UNQUALIFIED";
/// Four.meme Classic (R12c) and OpenFour (R12d) have their PAPER path but
/// no LIVE path yet: their requested LIVE stays blocked for this reason.
pub const FOUR_EXECUTION_PATH_MISSING: &str = "FOUR_EXECUTION_PATH_MISSING";
/// A switch ENABLED in an earlier boot session: every restart disables it.
pub const RESET_BY_RESTART: &str = "RESET_BY_RESTART";
pub(super) const BLOCK_REASONS: [&str; 6] = [
    ECONOMIC_LIVE_NOT_AUTHORIZED,
    ECONOMIC_LIVE_DISABLED,
    C2_OPERATIONAL_NOT_AUTHORIZED,
    SOURCE_ACCESS_UNQUALIFIED,
    FOUR_EXECUTION_PATH_MISSING,
    RESET_BY_RESTART,
];
/// The request ID a restart reset of the switches records.
pub const RESTART_RESET: &str = "RESTART_RESET";

/// Why a requested LIVE is not live now (the LIVE gate, plan 28.10.); each
/// names the first missing condition, [`Generation::live_blockers`] all of
/// them. They are never stored: LIVE is evaluated as it holds now.
pub const LIVE_GRANT_MISSING: &str = "LIVE_GRANT_MISSING";
pub const LIVE_GRANT_RESET_BY_RESTART: &str = "LIVE_GRANT_RESET_BY_RESTART";
pub const LIVE_GRANT_EXPIRED: &str = "LIVE_GRANT_EXPIRED";
pub const LIVE_NOT_IN_SCOPE: &str = "LIVE_NOT_IN_SCOPE";
pub const LIVE_RUNTIME_NOT_REQUESTED: &str = "LIVE_RUNTIME_NOT_REQUESTED";
pub const WALLET_LAUNCHER_DISABLED: &str = "WALLET_LAUNCHER_DISABLED";
pub const WALLET_BUYER_DISABLED: &str = "WALLET_BUYER_DISABLED";
pub const ECON_SIGN_DISABLED: &str = "ECON_SIGN_DISABLED";
pub const ECON_ARM_DISABLED: &str = "ECON_ARM_DISABLED";
pub const ECON_SEND_DISABLED: &str = "ECON_SEND_DISABLED";
pub const ECON_ATTEMPT_DISABLED: &str = "ECON_ATTEMPT_DISABLED";
pub const BUY_LIMIT_UNREADABLE: &str = "BUY_LIMIT_UNREADABLE";
pub const BUY_LIMIT_EXHAUSTED: &str = "BUY_LIMIT_EXHAUSTED";
pub const ATTEMPT_TOTAL_UNREADABLE: &str = "ATTEMPT_TOTAL_UNREADABLE";
pub const ATTEMPT_TOTAL_EXHAUSTED: &str = "ATTEMPT_TOTAL_EXHAUSTED";
pub const LIVE_GRANT_ATTEMPTS_EXHAUSTED: &str = "LIVE_GRANT_ATTEMPTS_EXHAUSTED";
pub const LIVE_SPEND_UNREADABLE: &str = "LIVE_SPEND_UNREADABLE";
pub const LIVE_GRANT_SPEND_EXHAUSTED: &str = "LIVE_GRANT_SPEND_EXHAUSTED";
/// REV.1.0 R4 (PHASENPLAN: a cumulative worst-case budget, technically
/// bounded, never raised automatically): the campaign's admissions under
/// every grant leave no room for a BUY with its exits
/// (`BSC_EXEC_LIVE_SPEND_TOTAL_WEI`).
pub const LIVE_CAMPAIGN_SPEND_EXHAUSTED: &str = "LIVE_CAMPAIGN_SPEND_EXHAUSTED";
pub const LIVE_VENUE_UNQUALIFIED: &str = "LIVE_VENUE_UNQUALIFIED";
pub const LIVE_SOURCE_UNQUALIFIED: &str = "LIVE_SOURCE_UNQUALIFIED";
pub const LIVE_ADAPTER_UNQUALIFIED: &str = "LIVE_ADAPTER_UNQUALIFIED";
pub const LIVE_ACQUISITION_UNPROVEN: &str = "LIVE_ACQUISITION_UNPROVEN";
/// BSC3-049: a granted builder did not accept the proven acquisition's
/// attempt (exits are exempt: they need no acquisition through it).
pub const LIVE_ACQUISITION_BUILDER_UNPROVEN: &str = "LIVE_ACQUISITION_BUILDER_UNPROVEN";
pub const LIVE_RISK_DECISION_MISSING: &str = "LIVE_RISK_DECISION_MISSING";
/// BSC3-049: the qualification's risk decision was made for another grant
/// scope (or, before it named one, for none).
pub const LIVE_RISK_DECISION_OTHER_SCOPE: &str = "LIVE_RISK_DECISION_OTHER_SCOPE";
/// The grant on file is not the one the executor was armed under.
pub const LIVE_GRANT_CHANGED: &str = "LIVE_GRANT_CHANGED";
/// A BUY's input exceeds the grant's largest input.
pub const LIVE_INPUT_ABOVE_GRANT: &str = "LIVE_INPUT_ABOVE_GRANT";
/// The same signed transaction again: never a second time (no retry).
pub const LIVE_INTENT_SEEN: &str = "LIVE_INTENT_SEEN";
/// BSC3-076: a launch's C3 BUY finds no lease of the owner held (the next
/// reconcile takes one): nothing taken, nothing sent.
pub const LIVE_LEASE_MISSING: &str = "LIVE_LEASE_MISSING";
/// BSC3-076: the lease held is not this BUY's (another nonce, less budget,
/// exit reserve or window) or the owner's state changed since it was taken:
/// nothing taken, nothing sent; the next reconcile takes it again.
pub const LIVE_LEASE_STALE: &str = "LIVE_LEASE_STALE";
/// The grant admits the exits of open positions only (BSC3-058): no BUY.
pub const LIVE_GRANT_EXIT_ONLY: &str = "LIVE_GRANT_EXIT_ONLY";
/// BSC3-069: a grant without the execution risk the operator confirmed
/// (written before it): it admits nothing.
pub const LIVE_GRANT_RISK_MISSING: &str = "LIVE_GRANT_RISK_MISSING";
/// REV.1.0 R4 (BSC3-069): the grant's risk is above the runtime's soft
/// bounds and its scope does not confirm it as an outlier.
pub const LIVE_RISK_OUTLIER_UNACKED: &str = "LIVE_RISK_OUTLIER_UNACKED";
/// The grant confirms an outlier risk, `BSC_EXEC_LIVE_RISK_OUTLIER` is OFF.
pub const LIVE_RISK_OUTLIER_LOCKED: &str = "LIVE_RISK_OUTLIER_LOCKED";
/// REV.1.0 R4: the bound builder policy (`BSC_EXEC_LIVE_BUILDER_POLICY`)
/// is unreadable or its builders are not both verified.
pub const LIVE_BUILDER_POLICY_UNVERIFIED: &str = "LIVE_BUILDER_POLICY_UNVERIFIED";
/// Its oldest fetched source is over 24 h old (an import is never fresh).
pub const LIVE_BUILDER_POLICY_STALE: &str = "LIVE_BUILDER_POLICY_STALE";
/// The signed gas price is below its strictest verified builder floor.
pub const LIVE_GAS_PRICE_BELOW_BUILDER_FLOOR: &str = "LIVE_GAS_PRICE_BELOW_BUILDER_FLOOR";
/// 31.10. order: a grant without its positions and losses bounds (one
/// written before them) arms nothing; exits of open positions go on.
pub const LIVE_GRANT_LIMITS_MISSING: &str = "LIVE_GRANT_LIMITS_MISSING";
/// A BUY refused by the grant's limits (exits go on): as many positions
/// open (pending, open or exiting) as it allows; its positions' or this
/// UTC day's realized net loss at its bound; the last BUY too recent.
pub const LIVE_OPEN_POSITIONS_MAX: &str = "LIVE_OPEN_POSITIONS_MAX";
pub const LIVE_REALIZED_LOSS_MAX: &str = "LIVE_REALIZED_LOSS_MAX";
pub const LIVE_DAILY_LOSS_MAX: &str = "LIVE_DAILY_LOSS_MAX";
pub const LIVE_BUY_INTERVAL: &str = "LIVE_BUY_INTERVAL";
/// BSC3-069: the executor's configuration signs beyond the granted risk.
pub const LIVE_RISK_OUTSIDE_GRANT: &str = "LIVE_RISK_OUTSIDE_GRANT";
/// BSC3-012: the verified LIVE qualification on file is another record than
/// the one the armed path rests on.
pub const LIVE_QUALIFICATION_CHANGED: &str = "LIVE_QUALIFICATION_CHANGED";
/// BSC3-030 review: the position's exit steps reverted as often as
/// `BSC_EXEC_LIVE_EXIT_REVERTS_MAX` allows: its exits wait for the operator.
pub const LIVE_EXIT_REVERTS_EXHAUSTED: &str = "LIVE_EXIT_REVERTS_EXHAUSTED";
/// BSC3-030 review: the position's last reverted exit step used all its gas.
pub const LIVE_EXIT_OUT_OF_GAS: &str = "LIVE_EXIT_OUT_OF_GAS";
/// 01.11. (R1): an open lot whose token is not on the bonding curve as
/// read (`getTokenV8Safe` status other than Tradable: migrated to the DEX,
/// or not a Portal token): no strategy exit can sell it; no new BUY until
/// it ends.
pub const LIVE_POSITION_OFF_CURVE: &str = "LIVE_POSITION_OFF_CURVE";
/// REV.1.0 R5: the lot's token migrated to its DEX (off the curve): its exit
/// is the DEX exit's (`BSC_EXEC_LIVE_DEX_EXIT`), else the operator's.
pub const LIVE_POSITION_MIGRATED: &str = "LIVE_POSITION_MIGRATED";
/// REV.1.0 R2: a C3 BUY of the grant landed behind another trader's BUY,
/// outside its launch's block or without tokens (the ledger's `PLACED`):
/// no new BUY until the operator grants anew; the exits go on.
pub const LIVE_PLACEMENT_VIOLATED: &str = "LIVE_PLACEMENT_VIOLATED";
/// REV.1.0 R3 (BSC3-045): an entry of the LIVE ledger was quarantined (it
/// did not decode): what it said may be missing; no BUY under a grant
/// written before it until the operator grants anew; the exits go on.
pub const LIVE_LEDGER_QUARANTINED: &str = "LIVE_LEDGER_QUARANTINED";
/// 01.11. (R1 review): an open lot whose curve read fails for a reason a
/// retry does not change (an ABI violation, no Portal state): no exit
/// prices it; no new BUY until it ends.
pub const LIVE_POSITION_UNREADABLE: &str = "LIVE_POSITION_UNREADABLE";
/// 01.11. (R1): an open lot whose curve is outside the priced scope as
/// read (another version, an NFT gate, a tax or threshold out of range):
/// no strategy exit prices it; no new BUY until it ends.
pub const LIVE_POSITION_UNPRICED: &str = "LIVE_POSITION_UNPRICED";
/// SC4 d: an open Four.meme lot whose venue Strategy C does not follow
/// (switched on without it): its curve is not tracked, no strategy exit
/// prices it; no new BUY until it ends.
pub const LIVE_POSITION_VENUE_UNFOLLOWED: &str = "LIVE_POSITION_VENUE_UNFOLLOWED";
/// SC4 d: an open Four.meme lot of a family without its execution keys
/// (`BSC_EXEC_FOUR_*`, `BSC_EXEC_OPENFOUR_*`): the path does not arm its
/// venue, no strategy exit sells it; no new BUY until it ends.
pub const LIVE_POSITION_VENUE_UNCONFIGURED: &str = "LIVE_POSITION_VENUE_UNCONFIGURED";
/// R13 review H1: a grant naming C3 while C3 takes foreign launches
/// (`BSC_EXEC_C3_CANDIDATES=FOREIGN_AUTO`): no grant or risk decision names
/// that scope, so the path does not arm (FOREIGN_AUTO is PAPER/SHADOW).
pub const LIVE_C3_FOREIGN_UNGRANTED: &str = "LIVE_C3_FOREIGN_UNGRANTED";
/// REV.1.0 R3b (BSC3-024/041): an unresolved LIVE transaction whose
/// receipt recovery is asked (not answered yet), would need the second
/// source (none configured), or failed (asked again after
/// `BSC_CANONICAL_RECOVERY_RETRY_MS`). Further words are
/// [`crate::chain_follower::RecoveryPending::name`].
pub const LIVE_RECOVERY_ASKED: &str = "RECOVERY_ASKED";
pub const LIVE_RECOVERY_NO_SECOND_SOURCE: &str = "RECOVERY_NO_SECOND_SOURCE";
pub const LIVE_RECOVERY_FAILED: &str = "RECOVERY_FAILED";
/// One waiting for a free read (at most `BSC_CANONICAL_RECOVERY_READS_MAX`
/// at a time); one never asked: of another sender than the configured
/// buyer, or a lease without its window.
pub const LIVE_RECOVERY_QUEUED: &str = "RECOVERY_QUEUED";
pub const LIVE_RECOVERY_OTHER_SENDER: &str = "RECOVERY_OTHER_SENDER";
pub const LIVE_RECOVERY_LEASE_WINDOW_UNKNOWN: &str = "RECOVERY_LEASE_WINDOW_UNKNOWN";
/// REV.1.0 R3b: the operator wants a nonce burnt: no BUY takes the next
/// nonce before it (an exit may: it replaces the transaction as well).
pub const LIVE_NONCE_BURN_FIRST: &str = "LIVE_NONCE_BURN_FIRST";
/// REV.1.0 R4 (BSC3-059): the grant runs longer than
/// `BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS` admits for a BUY (exits go on).
pub const LIVE_GRANT_DURATION_ABOVE_MAX: &str = "LIVE_GRANT_DURATION_ABOVE_MAX";
/// REV.1.0 R9 (BSC3-001): a presence anchor is configured
/// (`BSC_EXEC_LIVE_PRESENCE_ANCHOR`) and the grant is not signed by it: no
/// BUY (exits go on).
pub const LIVE_PRESENCE_UNSIGNED: &str = "LIVE_PRESENCE_UNSIGNED";
/// REV.1.0 R8 (BSC3-049): the LIVE qualification is older than
/// `BSC_EXEC_LIVE_QUALIFICATION_MAX_AGE_MS` admits for a BUY (exits go on).
pub const LIVE_QUALIFICATION_STALE: &str = "LIVE_QUALIFICATION_STALE";
/// BSC3-012: the running `bsc_runtime` is not the build the LIVE
/// qualification names (or it names none, or this one is unreadable).
pub const LIVE_BUILD_UNQUALIFIED: &str = "LIVE_BUILD_UNQUALIFIED";
/// A BUY would leave the grant less than the exits of the open positions
/// and of its own may still cost (BSC3-057): it is refused, the exits are
/// not.
pub const LIVE_EXIT_RESERVE_MISSING: &str = "LIVE_EXIT_RESERVE_MISSING";
/// A LIVE transaction whose bytes may have left is not resolved on chain
/// (BSC3-041): no new arm may take its nonce.
pub const LIVE_RELEASE_UNRESOLVED: &str = "LIVE_RELEASE_UNRESOLVED";
/// The LIVE ledger cannot be read or written: what may have left is not
/// known, nothing arms.
pub const LIVE_LEDGER_UNREADABLE: &str = "LIVE_LEDGER_UNREADABLE";

/// The LIVE gate's conditions in the order an operator meets them, each
/// with the reasons that say it does not hold (plan 28.10.): the LIVE tab's
/// checklist. The scope conditions (from GRANT_ATTEMPTS on) are judged only
/// against a grant.
pub const LIVE_CONDITIONS: [(&str, &[&str]); 20] = [
    ("ECONOMIC_LIVE", &[ECONOMIC_LIVE_DISABLED]),
    (
        "LIVE_GRANT",
        &[
            LIVE_GRANT_MISSING,
            LIVE_GRANT_RESET_BY_RESTART,
            LIVE_GRANT_EXPIRED,
        ],
    ),
    ("WALLET_LAUNCHER", &[WALLET_LAUNCHER_DISABLED]),
    ("WALLET_BUYER", &[WALLET_BUYER_DISABLED]),
    ("ECON_SIGN", &[ECON_SIGN_DISABLED]),
    ("ECON_ARM", &[ECON_ARM_DISABLED]),
    ("ECON_SEND", &[ECON_SEND_DISABLED]),
    ("ECON_ATTEMPT", &[ECON_ATTEMPT_DISABLED]),
    ("RUNTIME_LIVE", &[LIVE_RUNTIME_NOT_REQUESTED]),
    (
        "BUY_LIMIT_DAILY",
        &[BUY_LIMIT_UNREADABLE, BUY_LIMIT_EXHAUSTED],
    ),
    (
        "BUY_LIMIT_TOTAL",
        &[ATTEMPT_TOTAL_UNREADABLE, ATTEMPT_TOTAL_EXHAUSTED],
    ),
    ("GRANT_ATTEMPTS", &[LIVE_GRANT_ATTEMPTS_EXHAUSTED]),
    (
        "GRANT_SPEND",
        &[LIVE_SPEND_UNREADABLE, LIVE_GRANT_SPEND_EXHAUSTED],
    ),
    ("GRANT_BUYS", &[LIVE_GRANT_EXIT_ONLY]),
    (
        "GRANT_RISK",
        &[LIVE_GRANT_RISK_MISSING, LIVE_GRANT_LIMITS_MISSING],
    ),
    ("VENUE_QUALIFIED", &[LIVE_VENUE_UNQUALIFIED]),
    ("SOURCES_QUALIFIED", &[LIVE_SOURCE_UNQUALIFIED]),
    ("ADAPTERS_QUALIFIED", &[LIVE_ADAPTER_UNQUALIFIED]),
    (
        "ACQUISITION_PROVEN",
        &[LIVE_ACQUISITION_UNPROVEN, LIVE_ACQUISITION_BUILDER_UNPROVEN],
    ),
    (
        "RISK_DECISION",
        &[LIVE_RISK_DECISION_MISSING, LIVE_RISK_DECISION_OTHER_SCOPE],
    ),
];

/// The first of [`LIVE_CONDITIONS`] judged only against a grant.
pub(super) const SCOPE_CONDITIONS_FROM: usize = 11;

/// What an exit of a position the grant opened does not need of the LIVE
/// gate: BUY attempts (the attempt switch, the daily and campaign limits,
/// the grant's attempts), room for another BUY input, and a grant that
/// admits exits only (BSC3-058). Its own gas must still fit the grant's
/// spend ([`Store::admit_live`]).
pub(super) const EXIT_EXEMPT: [&str; 12] = [
    LIVE_ACQUISITION_BUILDER_UNPROVEN,
    // 31.10. order: the limits bound new BUYs; the exits of what was bought
    // go on under a grant without them.
    LIVE_GRANT_LIMITS_MISSING,
    // BSC3-049: the risk decision binds the scope of new BUYs; the exits of
    // what was bought need one, for whichever scope.
    LIVE_RISK_DECISION_OTHER_SCOPE,
    ECON_ATTEMPT_DISABLED,
    BUY_LIMIT_UNREADABLE,
    BUY_LIMIT_EXHAUSTED,
    ATTEMPT_TOTAL_UNREADABLE,
    ATTEMPT_TOTAL_EXHAUSTED,
    LIVE_GRANT_ATTEMPTS_EXHAUSTED,
    LIVE_GRANT_SPEND_EXHAUSTED,
    LIVE_SPEND_UNREADABLE,
    LIVE_GRANT_EXIT_ONLY,
];

/// The campaign's total BUY attempt limit (plan 28.10., `0a647175`): every
/// real economic BUY attempt counts, G0.7 included, UNKNOWN ones too, over
/// every day and restart. 0 initially.
pub const BUY_ATTEMPTS_TOTAL: &str = "BSC_BUY_ATTEMPTS_TOTAL";
