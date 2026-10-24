import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { decodeOverview } from "./overviewAdapter";

// Rust writes these bytes (gateway/tests/control_goldens.rs); the adapter must read them exactly.
const bytes = Uint8Array.from(Buffer.from(readFileSync(new URL("../../../../../protocol/golden/control/overview_aggregates.hex", import.meta.url), "utf8").trim(), "hex"));

describe("overview.aggregates golden bytes", () => {
  it("accepts the full i128 range for signed USD micro and the pushed readiness job", () => {
    const view = decodeOverview(bytes);
    const bsc = view.projects[0]!;
    expect(bsc.finance?.realizedNetPnlUsdMicro).toBe("-170141183460469231731687303715884105728");
    expect(bsc.finance?.totalCapitalUsdMicro).toBe(18446744073709551615n);
    expect(bsc.jobs).toEqual([{ job: "readiness", state: "STOPPED", launchId: "launch", resultCode: "BLOCKED", controllerAvailable: true }]);
    expect(view.global.available).toBe(false);
  });
});
