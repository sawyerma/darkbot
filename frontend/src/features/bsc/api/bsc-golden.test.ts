import {bindingOf} from "@/core/realtime/topicTable";
import {TopicId} from "@/core/realtime";
import {describe,it,expect} from "vitest";
import {readFileSync} from "node:fs";
import {decodeStatus,decodeLaunch,decodeMonitoring,decodeWallets} from "./bscAdapter";
import {statusTone} from "../model/status";
const golden=(name:string)=>Uint8Array.from(Buffer.from(readFileSync(new URL(`../../../../../protocol/golden/bsc/${name}.hex`,import.meta.url),"utf8").trim(),"hex"));
describe("native BSC reporter contract",()=>{
 it("decodes the Rust golden projections without granting readiness or live authority",()=>{
  expect(bindingOf(TopicId.BSC_STATUS)?.payloadType).toBe("darkbot.bsc.v1.BscStatus");
  const s=decodeStatus(golden("status"));expect(s.sourceCommit).toBe("fixture-source");expect(s.guard?.guardState).toBe("UNARMED");expect(s.readiness?.verdict).toBe("STOP");expect(s.liveState).toBe("BLOCKED");expect(s.dataAsOfUnixS).toBe(1792232000n);expect(s.readiness?.checks[0]?.value).toContain("[redacted]");
  expect(decodeLaunch(golden("launch")).contractPresent).toBe(false);
  expect(decodeMonitoring(golden("monitoring")).state).toBe("NOT_EXECUTED");
  expect(decodeWallets(golden("wallets")).launcherBalanceWei).toBeUndefined();
 });
 it("maps qualified pass/running green, failure red, ARMED amber and unknown neutral",()=>{
  for(const state of ["RUNNING","PASS","PASS_READ_ONLY"])expect(statusTone(state)).toBe("green");
  for(const state of ["FAIL","FAILED","STOP","BLOCKED"])expect(statusTone(state)).toBe("red");
  for(const state of ["ARMED","UNARMED","STOPPING"])expect(statusTone(state)).toBe("amber");
  expect(statusTone("UNKNOWN")).toBe("neutral");
 });
});
