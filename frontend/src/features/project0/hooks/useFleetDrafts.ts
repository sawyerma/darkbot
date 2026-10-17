import { useCallback, useState } from "react";
import type { LiquidatorDraft } from "../model/contracts";
import { validDecimal } from "../model/format";

/**
 * V0 owns UNSAVED browser-memory drafts only. Closing the page discards them.
 * Backend-approved profiles, shared capital reservations and run/job states must
 * NOT be persisted through React localStorage or treated as executable.
 */
const blank = (id: string, name: string): LiquidatorDraft => ({
  id, name, origin: "manual", enabledStrategies: ["CLASSIC"],
  capitalUsd: "", minimumNetUsd: "", maximumFeeUsd: "", maximumInFlight: "1",
});

export function validateDraft(d: LiquidatorDraft): string | null {
  if (!d.name.trim()) return "A bot name is required.";
  if (d.enabledStrategies.length === 0) return "Choose at least one supported strategy.";
  if (!validDecimal(d.capitalUsd)) return "Capital must be positive.";
  if (!validDecimal(d.minimumNetUsd, true)) return "Minimum net profit must be nonnegative.";
  if (!validDecimal(d.maximumFeeUsd, true)) return "Fee limit must be nonnegative.";
  if (!/^[1-9]\d*$/.test(d.maximumInFlight)) return "Maximum in-flight must be a positive integer.";
  return null;
}

export function useFleetDrafts() {
  const [drafts, setDrafts] = useState<LiquidatorDraft[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [sequence, setSequence] = useState(0);
  const selected = drafts.find(d => d.id === selectedId) ?? null;
  const update = useCallback((id: string, patch: Partial<LiquidatorDraft>) => {
    setDrafts(current => current.map(d => d.id === id ? { ...d, ...patch } : d));
  }, []);
  const create = useCallback((source?: Pick<LiquidatorDraft, "capitalUsd" | "minimumNetUsd">) => {
    const id = String(sequence + 1);
    const d = { ...blank(id, "Liquidator " + id.padStart(2, "0")), ...source,
      origin: source ? "analytics_unverified" as const : "manual" as const };
    setSequence(n => n + 1);
    setDrafts(current => [...current, d]);
    setSelectedId(id);
  }, [sequence]);
  const duplicate = useCallback(() => {
    if (!selected) return;
    const id = String(sequence + 1);
    setSequence(n => n + 1);
    setDrafts(current => [...current, { ...selected, id, name: selected.name + " copy" }]);
    setSelectedId(id);
  }, [selected, sequence]);
  const remove = useCallback(() => {
    if (!selectedId) return;
    setDrafts(current => current.filter(d => d.id !== selectedId));
    setSelectedId(drafts.find(d => d.id !== selectedId)?.id ?? null);
  }, [selectedId, drafts]);
  return { drafts, selected, selectedId, select: setSelectedId, update, create, duplicate, remove };
}
