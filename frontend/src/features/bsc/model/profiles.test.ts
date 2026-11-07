import { describe, expect, it } from "vitest";
import { create } from "@bufbuild/protobuf";
import { Availability, CreatorSellExit, CreatorSoldBeforeFill, TrainingProfileStatus, V8_CONTRACTS } from "../api/bscV8Adapter";
import { appliedProfile, copyName, marketOptions, moduleParams, profilesOf, type ProfileView } from "./profiles";
import { FILTER_KEYS, filterOrder, formOfValues } from "./strategyForm";
import { settableInit } from "./strategySettings";

// TEST_ONLY: a catalog as the native profile owner answers it (BSC_v2's V8-01 contract): owner ids,
// every value of a revision, an archived and a deleted module.
const VALUES = { inputWei: "10000000000000000", targetNetMultiple: "2", stopNetMultiple: "0.5", observationMs: 3000n, holdMs: 600000n, maxOpen: 5, firstBuyerWindow: 5,
  creatorSoldBeforeFill: CreatorSoldBeforeFill.FREE_CANCEL, creatorSellExit: CreatorSellExit.ENABLED,
  filters: [{ feature: "unique_buyers", enabled: true, min: "2" }, { feature: "buy_count", enabled: true, min: "3" }] };
const profile = (profileId: string, name: string, status = TrainingProfileStatus.ACTIVE, over = {}) =>
  ({ profileId, name, revision: 2n, configSha256: "ab".repeat(32), venues: ["BSC_VENUE_FLAP"], status, values: VALUES, requestId: `r-${profileId}`, ...over });
const catalog = create(V8_CONTRACTS.training_profiles, { availability: Availability.AVAILABLE, catalogVersion: 4n, provenance: { owner: "bsc_runtime strategy profiles", provenance: "NATIVE_CREATED" },
  profiles: [profile("p000001", "Flap fast"), profile("p000002", "Old", TrainingProfileStatus.ARCHIVED), profile("p000003", "Gone", TrainingProfileStatus.DELETED),
    profile("p000004", "Busy", TrainingProfileStatus.ACTIVE, { inUse: true, inUseReason: "run t1" })] });
const answer = { kind: "ANSWER" as const, requestId: "q", result: { case: "trainingProfiles" as const, value: catalog } };

describe("V8 Training modules (BSC-V8-01): the native catalog and its requests", () => {
  it("shows the owner's modules (never a deleted one) with every value as the native strings", () => {
    const v = profilesOf(answer);
    if (!v.ok) throw new Error("catalog expected");
    expect(v.catalogVersion).toBe(4n);
    expect(v.profiles.map(p => [p.id, p.status, p.inUse])).toEqual([["p000001", "ACTIVE", false], ["p000002", "ARCHIVED", false], ["p000004", "ACTIVE", true]]);
    const [first] = v.profiles as ProfileView[];
    expect(first?.values.BSC_STRATEGY_C_FILTERS).toBe("unique_buyers:2:,buy_count:3:");
    expect(formOfValues(first!.values).form).toMatchObject({ stake: "0.01", target: "2", hold: "600", firstBuyerWindow: "5", creatorSoldBeforeFill: "FREE_CANCEL" });
    expect(profilesOf({ kind: "LOADING" })).toEqual({ ok: false });
    expect(profilesOf({ kind: "NOT_IMPLEMENTED", reason: "no native owner yet: BSC-V8-01" })).toEqual({ ok: false });
    const down = create(V8_CONTRACTS.training_profiles, { availability: Availability.UNAVAILABLE, unavailableReason: "catalog unreadable" });
    expect(profilesOf({ ...answer, result: { case: "trainingProfiles", value: down } })).toEqual({ ok: false, reason: "catalog unreadable" });
  });
  it("sends a revision whole: a name no other module has, a market, every value; the filters complete in the module's order", () => {
    const v = profilesOf(answer);
    if (!v.ok) throw new Error("catalog expected");
    const [first, ...others] = v.profiles;
    const form = formOfValues(first!.values).form;
    const order = filterOrder(first!.values.BSC_STRATEGY_C_FILTERS ?? "");
    const ok = moduleParams({ name: "Flap fast", venues: ["BSC_VENUE_FLAP", "BSC_VENUE_OPENFOUR"], form }, others, order);
    if ("error" in ok) throw new Error(ok.error);
    expect([ok.name, ok.venues, ok.filters]).toEqual(["Flap fast", ["BSC_VENUE_FLAP", "BSC_VENUE_OPENFOUR"], "unique_buyers:2:,buy_count:3:"]);
    expect(ok.values).toMatchObject({ inputWei: "10000000000000000", observationMs: 3000n, holdMs: 600000n, maxOpen: 5, firstBuyerWindow: 5, creatorSoldBeforeFill: "FREE_CANCEL", creatorSellExit: "ENABLED" });
    expect(ok.values.filters).toHaveLength(FILTER_KEYS.length);
    expect(ok.values.filters.slice(0, 3)).toEqual([{ feature: "unique_buyers", enabled: true, min: "2" }, { feature: "buy_count", enabled: true, min: "3" }, { feature: "sell_count", enabled: false }]);
    expect(settableInit(ok.values)).toMatchObject({ creatorSoldBeforeFill: CreatorSoldBeforeFill.FREE_CANCEL, creatorSellExit: CreatorSellExit.ENABLED });
    expect(moduleParams({ name: " ", venues: ["BSC_VENUE_FLAP"], form }, others, order)).toEqual({ error: "Enter a module name" });
    expect(moduleParams({ name: "Old", venues: ["BSC_VENUE_FLAP"], form }, others, order)).toEqual({ error: "A module named “Old” exists" });
    expect(moduleParams({ name: "New", venues: [], form }, others, order)).toEqual({ error: "Choose a market" });
    // No native unset in a profile either: every value is required.
    expect(moduleParams({ name: "New", venues: ["BSC_VENUE_FLAP"], form: { ...form, creatorSellExit: "" } }, others, order))
      .toEqual({ error: "BSC_STRATEGY_C_CREATOR_SELL_EXIT is required: a module carries every value" });
    expect(moduleParams({ name: "New", venues: ["BSC_VENUE_FLAP"], form: { ...form, maxOpen: "" } }, others, order)).toHaveProperty("error", "BSC_STRATEGY_C_MAX_OPEN is required: a module carries every value");
  });
  it("offers every venue explicitly, each alone and the module's own set; numbers a copy's name; reads the applied receipt", () => {
    const label = (id: string) => id.replace("BSC_VENUE_", "");
    const offered = ["BSC_VENUE_FLAP", "BSC_VENUE_FOUR_CLASSIC", "BSC_VENUE_OPENFOUR"];
    expect(marketOptions(offered, ["BSC_VENUE_FLAP"], label).map(o => [o.label, o.venues.join(",")])).toEqual([
      ["All markets", offered.join(",")], ["FLAP", "BSC_VENUE_FLAP"], ["FOUR_CLASSIC", "BSC_VENUE_FOUR_CLASSIC"], ["OPENFOUR", "BSC_VENUE_OPENFOUR"]]);
    expect(marketOptions(offered, ["BSC_VENUE_FLAP", "BSC_VENUE_OPENFOUR"], label).at(-1)).toEqual({ key: "BSC_VENUE_FLAP,BSC_VENUE_OPENFOUR", label: "FLAP + OPENFOUR", venues: ["BSC_VENUE_FLAP", "BSC_VENUE_OPENFOUR"] });
    const v = profilesOf(answer);
    if (!v.ok) throw new Error("catalog expected");
    expect(copyName("Flap fast", v.profiles)).toBe("Flap fast (copy)");
    expect(copyName("Flap fast", [...v.profiles, { ...v.profiles[0]!, name: "Flap fast (copy)" }])).toBe("Flap fast (copy 2)");
    expect(appliedProfile("outcome=APPLIED action=CREATE profile_id=p000005 revision=1 catalog=5 request_id=r9")).toBe("p000005");
    expect(appliedProfile("outcome=REJECTED_STALE action=UPDATE profile_id=p000001 revision=- catalog=4 request_id=r9 reason=stale")).toBeUndefined();
    expect(appliedProfile("outcome=APPLIED action=DELETE profile_id=- revision=- catalog=5 request_id=r9")).toBeUndefined();
  });
});
