import {bindingOf} from "@/core/realtime/topicTable";
import {TopicId} from "@/core/realtime";
import {describe,it,expect} from "vitest";
import {readFileSync} from "node:fs";
import {decodeStatus,decodeLaunch,decodeMonitoring,decodeWallets,decodeLive} from "./bscAdapter";
import {statusTone} from "../model/status";
import {nativeRows,sectionRecords} from "../model/placement";
const golden=(name:string)=>Uint8Array.from(Buffer.from(readFileSync(new URL(`../../../../../protocol/golden/bsc/${name}.hex`,import.meta.url),"utf8").trim(),"hex"));
describe("native BSC reporter contract",()=>{
 it("decodes the Rust golden projections without granting readiness or live authority",()=>{
  expect(bindingOf(TopicId.BSC_STATUS)?.payloadType).toBe("darkbot.bsc.v1.BscStatus");
  const s=decodeStatus(golden("status"));expect(s.sourceCommit).toBe("fixture-source");expect(s.guard?.guardState).toBe("UNARMED");expect(s.readiness?.verdict).toBe("BLOCKED");expect(s.readiness?.mode).toBe("OFFLINE_ZERO_EGRESS");expect(s.readiness?.candidateBinding).toBe("NO_SELECTION");expect(s.liveState).toBe("BLOCKED");expect(s.dataAsOfUnixS).toBe(1792232000n);expect(s.readiness?.checks[0]?.value).toContain("[redacted]");
  expect(decodeLaunch(golden("launch")).contractPresent).toBe(false);
  expect(decodeMonitoring(golden("monitoring")).state).toBe("NOT_EXECUTED");
  expect(decodeWallets(golden("wallets")).launcherBalanceWei).toBeUndefined();
  // Plan 28.10.: without the owner's state the LIVE gate is UNKNOWN, never CLEAR.
  expect(bindingOf(TopicId.BSC_LIVE_STATE)?.payloadType).toBe("darkbot.bsc.v1.BscLiveState");
  const l=decodeLive(golden("live"));expect(l.gate).toBe("UNKNOWN");expect(l.warnings).toContain("OWNER_STATE_ABSENT");expect(l.qualification).toBe("NONE");expect(l.pathState).toBe("NOT_RUNNING");expect(l.grant).toBeUndefined();
 });
 it("shows the readiness record leaves without a typed field once, in Launch / BUY section 8",()=>{
  const records=decodeLaunch(golden("launch")).records;
  expect(sectionRecords(records,"launch",8).map(r=>r.schema)).toEqual(["darkbot.g07.readiness"]);
  // status, mode, observed time and checks keep their typed rows; only the remaining leaves are added.
  expect(nativeRows(records,"launch",8)).toEqual([{name:"Live authorized",value:"false",hint:"live_authorized · g07.readiness"}]);
 });
 it("maps qualified pass/running green, failure red, open states amber and unknown neutral",()=>{
  for(const state of ["RUNNING","PASS","PASS_READ_ONLY"])expect(statusTone(state)).toBe("green");
  for(const state of ["FAIL","FAILED","STOP","BLOCKED"])expect(statusTone(state)).toBe("red");
  for(const state of ["ARMED","UNARMED","STOPPING"])expect(statusTone(state)).toBe("amber");
  expect(statusTone("UNKNOWN")).toBe("neutral");
  // Native G0.7 classifications are never green; G0.8 ADAPT/UNRESOLVED are open, not neutral.
  expect(statusTone("STOP_CAPTURE_INCOMPLETE__RECONCILE_G08__NO_AUTOMATIC_RETRY")).toBe("red");
  for(const state of ["SUBMITTED_OR_SUBMISSION_UNKNOWN__RUN_G08__NO_AUTOMATIC_RETRY","PROVISIONAL_RELEASED__RECEIPT_WINDOW_OPEN__RECONCILE_G08__NO_AUTOMATIC_RETRY","PROVISIONAL_RELEASED__PROVIDER_RESULTS_PENDING__RECONCILE_G08__NO_AUTOMATIC_RETRY","ADAPT","UNRESOLVED","STALE"])expect(statusTone(state)).toBe("amber");
  for(const state of ["BOUND","NOT_EXECUTED","SOMETHING_NEW"])expect(statusTone(state)).toBe("neutral");
 });
});
