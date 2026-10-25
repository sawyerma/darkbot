import { describe, expect, it } from "vitest";
import { TopicId } from "@/core/realtime";
import type { Project0Tab } from "../model/contracts";
import { holdPanelTopics } from "./project0Adapter";

/** Review E4: a tab that renders owner values holds their topics (before: listeners only, never
 * subscribed — Overview, Liquidator and Wallets stayed IDLE). */
describe("Project 0 panel topics", () => {
  const held = (tab: Project0Tab) => {
    const acquired: TopicId[] = [];
    let released = 0;
    const release = holdPanelTopics(tab, (t) => {
      acquired.push(t);
      return () => { released += 1; };
    });
    release();
    return { acquired, released };
  };

  it("acquires every owner topic the visible tab renders and releases it", () => {
    expect(held("overview").acquired).toEqual(expect.arrayContaining([TopicId.PROJECT0_FINANCE_SUMMARY, TopicId.PROJECT0_LIQUIDATOR_STATE]));
    expect(held("liquidator").acquired).toEqual(expect.arrayContaining([TopicId.PROJECT0_LIQUIDATOR_STATE, TopicId.PROJECT0_FINANCE_SUMMARY]));
    expect(held("wallets").acquired).toEqual(expect.arrayContaining([TopicId.PROJECT0_WALLETS_STATE, TopicId.PROJECT0_FINANCE_SUMMARY]));
    expect(held("analytics").acquired).toEqual([TopicId.PROJECT0_ANALYTICS_JOBS]);
    expect(held("capture").acquired).toEqual(expect.arrayContaining([TopicId.PROJECT0_CAPTURE_STATE, TopicId.PROJECT0_ANALYTICS_JOBS]));
    const w = held("wallets");
    expect(w.released).toBe(w.acquired.length);
  });
});
