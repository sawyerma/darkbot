import { useState } from "react";
import type { TopicStatus } from "@/core/realtime";
import type { CapabilityView, Fe06View, LiquidatorView } from "../model/owner";
import { ECON_SWITCHES, WALLET_PREFIX, economicBlock, liveGates } from "../model/owner";
import { DetailRow, Notice, Section } from "./PanelPrimitives";
import { analyticsCommand, economicCommand, useAnalyticsControl } from "@/core/realtime/analyticsControl";

const utc = (s: bigint) => new Date(Number(s) * 1000).toISOString().replace(".000Z", "Z");
const used = (n: bigint | null, limit: bigint) => `${n ?? "unknown"} of ${limit}`;

/**
 * P8-CTRL.1/.8: one economic switch or signer as the owner decided it, and the one request that
 * flips it (`p0ctl capability set <name> ENABLED|DISABLED`, CAS on the capability configuration
 * version and the owner epoch this view showed). A click is no success: the row changes only with
 * the owner's receipt.
 */
export function SwitchRow({ row, title, blocked, version, epoch }: {
  row: CapabilityView; title: string; blocked: string; version: bigint | null; epoch: bigint | null;
}) {
  const next = row.requested === "ENABLED" ? "DISABLED" : "ENABLED";
  const verb = next === "ENABLED" ? "Enable" : "Disable";
  const cmd = economicCommand(row.name, next, epoch);
  const problem = blocked || ("problem" in cmd ? cmd.problem : "");
  return <div className="p0-line">
    <span>{title}</span>
    <span className="p0-right">
      {`${row.effective}${row.requested !== row.effective ? ` (requested ${row.requested})` : ""}${row.blockReason ? ` · ${row.blockReason}` : ""} `}
      <button className="p0-secondary" aria-label={`${verb} ${row.name}`} disabled={problem !== ""} title={problem || undefined}
        onClick={() => { if ("params" in cmd) void analyticsCommand("project0.capability.set", { params: cmd.params, configVersion: version ?? undefined }); }}>
        {verb}
      </button>
    </span>
  </div>;
}

/**
 * P8-CTRL.2/.3/.5/.8 on the Liquidator tab: every economic switch, the LIVE gates as the owner
 * projected them, the FE-06 grant with its scope, validity and attempt usage (EXIT_ONLY named),
 * its revocation, and the scopes previewed for this owner epoch with the grant of exactly the
 * shown digest. Nothing here decides: LIVE stays what the owner made effective.
 */
export function EconomicControl({ owner, live }: { owner: LiquidatorView; live: boolean }) {
  const { busy } = useAnalyticsControl();
  const version = owner.capabilityConfigVersion;
  const epoch = owner.capabilityEpoch;
  const blocked = economicBlock({ busy, live, version, epoch });
  const rows = ECON_SWITCHES.map((n) => owner.capabilities.find((c) => c.name === n)).filter((c): c is CapabilityView => c != null);
  if (rows.length === 0 && !owner.fe06) return null;
  return <>
    <DetailRow title="Economic control · owner epoch / configuration version"
      value={`${epoch ?? "unknown"} / ${version ?? "unknown"}${live ? "" : " · last reported, not current"} · every switch DISABLED at an owner start`}/>
    {rows.map((r) => <SwitchRow key={r.name} row={r} title={r.name} blocked={blocked} version={version} epoch={epoch}/>)}
    {liveGates(owner).map((g) => <DetailRow key={g.gate} title={`LIVE gate · ${g.gate}`} value={`${g.met ? "met" : "missing"} · ${g.detail}`}/>)}
    {owner.fe06 ? <Fe06Rows fe06={owner.fe06} blocked={blocked} version={version} epoch={epoch}/> : <DetailRow title="FE-06" value="Not reported"/>}
  </>;
}

function Fe06Rows({ fe06: f, blocked, version, epoch }: { fe06: Fe06View; blocked: string; version: bigint | null; epoch: bigint | null }) {
  const [armed, setArmed] = useState<string | null>(null);
  const send = (capability: "FE06_GRANT" | "FE06_REVOKE", digest: string) => {
    setArmed(null);
    const cmd = economicCommand(capability, digest, epoch);
    if ("params" in cmd) void analyticsCommand("project0.capability.set", { params: cmd.params, configVersion: version ?? undefined });
  };
  return <>
    <DetailRow title="FE-06 grant" value={f.grantDigest == null ? `none · ${f.blockReason}`
      : `${f.inForce ? "in force" : "not in force"} · ${f.scopeMode} · ${f.grantDigest}${f.blockReason ? ` · ${f.blockReason}` : ""}`}/>
    {f.grantDigest != null ? <>
      <DetailRow title="FE-06 grant · valid" value={`${f.validFromUnixS == null ? "—" : utc(f.validFromUnixS)} … ${f.validUntilUnixS == null ? "—" : utc(f.validUntilUnixS)} · campaign ${f.campaignId}`}/>
      <DetailRow title="FE-06 attempts · today / campaign / grant"
        value={`${used(f.attemptsToday, f.maxAttemptsPerUtcDay)} / ${used(f.attemptsCampaign, f.maxAttemptsCampaign)} / ${used(f.attemptsGrant, f.maxAttemptsGrant)} · UNKNOWN and sent attempts count`}/>
      {f.scopeMode === "EXIT_ONLY" ? <Notice>EXIT_ONLY: exits of the scope&apos;s open positions only — no new liquidation; ECON_LIVE or ECON_SEND DISABLED, a halt or a revocation blocks every new exit too.</Notice> : null}
      <div className="p0-actions">
        {armed === `revoke:${f.grantDigest}`
          ? <button className="p0-primary" disabled={blocked !== ""} title={blocked || undefined} onClick={() => send("FE06_REVOKE", f.grantDigest!)}>Confirm revoke · {f.grantDigest}</button>
          : <button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined} onClick={() => setArmed(`revoke:${f.grantDigest}`)}>Revoke grant</button>}
      </div>
    </> : null}
    {f.previews.map((p) => <div key={p.digest}>
      <DetailRow title={`FE-06 scope preview · ${p.scopeMode}`} value={`${p.digest} · owner epoch ${p.ownerEpoch} · valid until ${utc(p.validUntilUnixS)}`}/>
      <pre className="p0-scope" aria-label={`FE-06 scope ${p.digest}`}>{p.terminal}</pre>
      {armed === `grant:${p.digest}` ? <>
        <Notice>Grant exactly the scope above ({p.scopeMode}, digest {p.digest}, owner epoch {String(p.ownerEpoch)}, until {utc(p.validUntilUnixS)} UTC). A grant is a permission, never a transaction: LIVE still needs every switch, every signer and the FE-04 approval; the owner checks the scope again when it takes the grant.{f.blockReason ? ` Owner: ${f.blockReason}.` : ""}</Notice>
        <div className="p0-actions"><button className="p0-primary" disabled={blocked !== ""} title={blocked || undefined} onClick={() => send("FE06_GRANT", p.digest)}>Confirm grant · {p.digest}</button></div>
      </> : <div className="p0-actions">
        <button className="p0-secondary" disabled={blocked !== "" || p.ownerEpoch !== epoch} title={blocked || (p.ownerEpoch !== epoch ? "Previewed for another owner epoch" : undefined)}
          onClick={() => setArmed(`grant:${p.digest}`)}>Grant exactly this scope · {p.digest}</button>
      </div>}
    </div>)}
  </>;
}

/**
 * P8-CTRL.1/.8 on the Wallets tab: one switch per configured signer with its validated roles —
 * requested/effective, the block reason, the configuration version, the owner epoch and the
 * owner's receipt. ENABLED allows only the bound economic path; never a transfer or key export.
 */
export function SignerRows({ owner, live }: { owner: LiquidatorView; live: boolean }) {
  const { busy } = useAnalyticsControl();
  const version = owner.capabilityConfigVersion;
  const epoch = owner.capabilityEpoch;
  const blocked = economicBlock({ busy, live, version, epoch });
  const signers = owner.capabilities.filter((c) => c.name.startsWith(WALLET_PREFIX));
  if (signers.length === 0) return <DetailRow title="Signers" value="No configured signer reported"/>;
  return <>{signers.map((s) => <SwitchRow key={s.name} row={s} blocked={blocked} version={version} epoch={epoch}
    title={`${s.name.slice(WALLET_PREFIX.length)} · ${s.roles.length ? s.roles.join(" · ") : "no validated role"}`}/>)}</>;
}

/**
 * P8-CTRL.8: the Wallets tab's signers beside the frozen balance panel — the owner's switch per
 * configured public key with its roles, the version and epoch a change states, the owner's last
 * decision and the command's receipt. Not current (stale, detached): shown as last reported, no
 * switch is sent.
 */
export function SignerSection({ owner, topicStatus }: { owner: LiquidatorView | null; topicStatus: TopicStatus }) {
  const { receipt } = useAnalyticsControl();
  const live = topicStatus === "LIVE";
  const d = owner?.lastCapabilityDecision ?? null;
  return <Section title="Signers" aside={<span className="p0-badge">{owner && live ? `owner epoch ${owner.capabilityEpoch ?? "unknown"}` : `${topicStatus}${owner ? " · last reported" : ""}`}</span>}>
    {!owner ? <p className="p0-quiet">No owner state received; no signer can be switched.</p> : <>
      {live ? null : <p className="p0-quiet">Last reported state ({topicStatus}); not current — no switch is sent.</p>}
      <SignerRows owner={owner} live={live}/>
      <DetailRow title="Capability configuration version" value={owner.capabilityConfigVersion == null ? "unknown" : String(owner.capabilityConfigVersion)}/>
      {d ? <DetailRow title={`Capability decision · #${d.seq}`} value={`${d.applied ? "APPLIED" : "REFUSED"} · ${d.request} · ${d.detail}`}/> : null}
      {receipt?.action === "project0.capability.set" ? <Notice><span data-testid="p0-signer-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""}</span></Notice> : null}
      <p className="p0-note">ENABLED lets the owner use a signer only for the bound economic path (with ECON_SIGN, LIVE, an FE-06 grant and the FE-04 approval) — never a transfer, a sweep or a key export. Every switch is DISABLED at an owner start; balances are read independently of it.</p>
    </>}
  </Section>;
}
