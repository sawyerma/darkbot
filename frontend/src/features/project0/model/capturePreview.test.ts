import { describe, expect, it } from "vitest";
import { REPORT_MARKERS } from "./contracts";
import { markerSelector, runStartPreview, type RunStartDraft } from "./capturePreview";

const all = REPORT_MARKERS.map(m => m.id);
const draft = (d: Partial<RunStartDraft> = {}): RunStartDraft => ({ duration: "4h", source: "logs", mode: "econ4h", verify: "off", markers: all, ...d });
const APPROVAL = "--network-audit <FILE> --mainnet-approved <TOKEN>";

describe("CLI preview mirrors p0ctl run start", () => {
  it("uses only flags run start accepts, never --mode, and names the always-required approval", () => {
    expect(runStartPreview(draft())).toEqual({
      command: `p0ctl run start --duration 4h ${APPROVAL} --capture-source logs --chain-verify off --report-markers all`, problem: null });
  });
  it("DEC-08: every chain verification mode is accepted; a history read names the operator's provider evidence", () => {
    for (const verify of ["index", "strict", "crosscheck"] as const)
      expect(runStartPreview(draft({ verify }))).toEqual({
        command: `p0ctl run start --duration 4h ${APPROVAL} --capture-source logs --chain-verify ${verify} --provider-evidence <EVIDENCE> --report-markers all`, problem: null });
  });
  it("names report markers in contract order and keeps none distinct from all", () => {
    expect(markerSelector([])).toBe("none");
    expect(markerSelector([...all].reverse())).toBe("all");
    expect(markerSelector(["end_liquidation", "lending_account_liquidate"])).toBe("lending_account_liquidate,end_liquidation");
  });
  it("accepts exactly the durations runtime_config::duration_seconds accepts", () => {
    for (const ok of ["30s", "01h", "4611686018s"]) expect(runStartPreview(draft({ duration: ok })).command).not.toBeNull();
    for (const bad of ["m", "0h", "00m", "1.5h", "-1h", "4611686019s", "99999999999d", "until-stopped"])
      expect(runStartPreview(draft({ duration: bad })).command, bad).toBeNull();
  });
  it("refuses what run start refuses instead of printing a command that fails", () => {
    for (const mode of ["market", "shadow"] as const) expect(runStartPreview(draft({ mode })).command).toBeNull();
  });
});
