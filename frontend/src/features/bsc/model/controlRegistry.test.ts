import { describe, expect, it } from "vitest";
import { CANDIDATE_CONTEXTS, INGRESS_SOURCES, VENUE_CONTROLS, INCLUSION_ADAPTERS, SOURCE_MODES, RUNTIME_MODES, FUTURE_ACTIONS, FUTURE_OBSERVATIONS, CONTROL_FIELDS, ingressFields, venueFields, inclusionFields } from "./controlRegistry";
describe("BSC permanent UI control inventory", () => {
  it("pins all roadmap IDs exactly once and preserves independent dimensions", () => {
    expect(CANDIDATE_CONTEXTS.map(x => x.id)).toEqual(["SELF_A", "SELF_B", "FOREIGN_AUTO"]);
    expect(INGRESS_SOURCES.map(x => x.id)).toEqual(["BSC_INGRESS_GETH", "BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC", "BSC_INGRESS_BLOCKREQ", "BSC_INGRESS_48CLUB"]);
    expect(VENUE_CONTROLS.map(x => x.id)).toEqual(["FLAP", "FOUR_CLASSIC", "OPENFOUR"]);
    expect(INCLUSION_ADAPTERS.map(x => x.id)).toEqual(["LOCAL_GETH", "48CLUB", "BLOCKRAZOR", "OWN_BUILDER"]);
    expect(SOURCE_MODES).toEqual(["OFF", "SHADOW", "LIVE"]);
    expect(RUNTIME_MODES).toEqual(["REPLAY", "PAPER", "SHADOW", "LIVE"]);
    for (const group of [CANDIDATE_CONTEXTS, INGRESS_SOURCES, VENUE_CONTROLS, INCLUSION_ADAPTERS, FUTURE_ACTIONS, FUTURE_OBSERVATIONS]) {
      expect(new Set(group.map(x => x.id)).size).toBe(group.length);
      expect(group.every(x => x.phase && x.owner && x.note)).toBe(true);
    }
  });
  it("reserves stable typed projection keys, but provides no economic dispatcher", () => {
    expect(CONTROL_FIELDS.generation).toBe("control.config.generation");
    expect(CONTROL_FIELDS.runtimeEffective).toBe("control.runtime.effective");
    expect(ingressFields("BSC_INGRESS_GETH").effective).toBe("control.ingress.BSC_INGRESS_GETH.effective");
    expect(venueFields("FLAP").deployment).toBe("control.venue.FLAP.deployment_sha256");
    expect(inclusionFields("48CLUB").qualified).toBe("control.inclusion.48CLUB.qualified");
    expect(FUTURE_ACTIONS.some(x => x.id === "SEND_G07")).toBe(true);
  });
});
