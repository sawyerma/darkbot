import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { bindingOf, censusName, labelOf, missingRecords, nativeRows, SECTIONS } from "./placement";
import { formatNative } from "./format";
import type { NativeRecordView } from "./types";

const census = JSON.parse(readFileSync(new URL("../../../../contracts/bsc-native-fields.json", import.meta.url), "utf8")) as { schema: string; symbol: string; sensitivity: string }[];
const record = (schema: string, fields: [string, string, string][]): NativeRecordView =>
  ({ schema, provenance: "NATIVE_CREATED", fields: fields.map(([name, value, unit]) => ({ name, kind: 0, value, unit })) });

describe("BSC field placement (FRONTEND_PHASENPLAN §13)", () => {
  it("places every census field exactly like check:bsc-fields, in an existing section", () => {
    for (const f of census) {
      const b = bindingOf(f.schema, f.symbol);
      expect(b, `${f.schema}.${f.symbol}`).not.toBeNull();
      if (b!.status === "EXCLUDED") { expect(b!.panel).toBe("none"); continue; }
      if (b!.panel === "shared") continue;
      expect(SECTIONS[b!.panel as keyof typeof SECTIONS]?.[b!.section - 1], `${f.schema}.${f.symbol}`).toBeTruthy();
      if (["SECRET_NEVER_BROWSER", "BINARY_EXCLUDED", "PRIVATE_RECORD_FIELD_AUDIT_REQUIRED"].includes(f.sensitivity)) throw Error(`secret placed: ${f.symbol}`);
    }
  });
  it("maps wire names of nested rows and repeated leaves to census symbols", () => {
    expect(censusName("upgrades[2].block")).toBe("upgrades[].block");
    expect(censusName("limitations[3]")).toBe("limitations");
    expect(censusName("local_send_boundaries[0].fortyeightclub[1].write_started_ns")).toBe("local_send_boundaries[].fortyeightclub[].write_started_ns");
  });
  it("renders a native leaf once in its section, never a typed duplicate or an excluded leaf", () => {
    const pre = record("darkbot.g06a.preflight-readonly.v3", [["launcher_address", "0x1", "EVM_address"], ["launcher_pending_nonce", "7", "nonce"], ["portal_code_sha256", "0x2", "hash_32_bytes"]]);
    const eco = record("darkbot.g06a.economics-evm-crosscheck.v2", [["salt", "0xab", "hash_32_bytes"], ["salt_counter", "3", "count"]]);
    expect(nativeRows([pre], "wallets", 6).map(r => r.hint)).toEqual(["launcher_pending_nonce · g06a.preflight-readonly.v3"]);
    expect(nativeRows([pre], "wallets", 1)).toEqual([]); // typed BscWalletsState.launcher_address
    expect(nativeRows([pre], "launch", 2).map(r => r.value)).toEqual(["0x2"]);
    expect(nativeRows([eco], "launch", 3).map(r => r.hint)).toEqual(["salt_counter · g06a.economics-evm-crosscheck.v2"]);
  });
  it("names record families without data and future producers instead of inventing values", () => {
    expect(missingRecords([], "launch", 3).map(m => m.status)).toEqual(expect.arrayContaining(["NO_RECORD"]));
    expect(missingRecords([], "analytics", 8).some(m => m.status === "FUTURE_PHASE_BLOCKED")).toBe(true);
  });
  it("shows wei exactly, never as a float", () => {
    expect(formatNative("1000000000000000001", "wei")).toBe("1.000000000000000001 BNB · 1000000000000000001 wei");
    expect(formatNative("", "wei")).toBe("—");
    expect(formatNative("250", "basis_points")).toBe("250 bps");
  });
});
describe("BSC field labels", () => {
  it("keeps labels readable and the unit out of the label", () => {
    expect(labelOf("_provenance.observed_unix_ms", "unix_milliseconds")).toBe("Provenance · observed");
    expect(labelOf("upgrades[1].implementation", "EVM_address")).toBe("Upgrades #2 · implementation");
    expect(labelOf("transactions.gas_price_wei", "wei")).toBe("Transactions · gas price");
  });
});
