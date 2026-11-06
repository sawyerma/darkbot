import { FILTER_GROUPS, activeFilters, type StrategyForm } from "../model/strategyForm";

/** The V8 module settings and the 15 entry filters, always visible (FE-V8-05/06). The values are
 *  the native settings; `disabled` locks the whole form. First-buyer window and team-sell exit are
 *  shown as what they are: not configurable / not implemented. */
export function StrategyEditor({ form, onChange, disabled, venues, firstBuyerWindow, idPrefix, notes = {} }: {
  form: StrategyForm; onChange: (f: StrategyForm) => void; disabled: boolean; venues: string; firstBuyerWindow: string; idPrefix: string;
  /** Per field: the running strategy's own value where the stored one differs (effective at the next start). */
  notes?: Partial<Record<keyof StrategyForm, string>>;
}) {
  const set = <K extends keyof StrategyForm>(k: K, v: StrategyForm[K]) => onChange({ ...form, [k]: v });
  const field = (k: "stake" | "target" | "stop" | "hold" | "observation" | "maxOpen", label: string) =>
    <div className="field"><label htmlFor={`${idPrefix}-${k}`}>{label}</label><input id={`${idPrefix}-${k}`} className="input" inputMode="decimal" value={form[k]}
      onChange={e => set(k, e.target.value)} data-testid={`${idPrefix}-${k}`} />{notes[k] ? <small data-testid={`${idPrefix}-${k}-running`}>{notes[k]}</small> : null}</div>;
  return <fieldset disabled={disabled} style={{ padding: 0, border: 0, margin: 0, minWidth: 0 }}>
    <div className="editor-fields">
      <div className="field"><label htmlFor={`${idPrefix}-market`}>Market</label><input id={`${idPrefix}-market`} className="input" value={venues} readOnly title="The venues Strategy C follows (source modes, Settings)" /></div>
      {field("stake", "Investment per buy · BNB")}
      {field("target", "Profit target · net ×")}
      {field("stop", "Stop loss · net ×")}
      {field("hold", "Maximum hold time · s")}
      {field("observation", "Observation window · s")}
      <div className="field"><label htmlFor={`${idPrefix}-fbw`}>Early buyers in window</label><input id={`${idPrefix}-fbw`} className="input" value={firstBuyerWindow} readOnly title="BSC_STRATEGY_C_FIRST_BUYER_WINDOW: in the runtime configuration, not configurable here yet (BSC-V8-05)" /><small>Not configurable (BSC-V8-05)</small></div>
      <div className="field"><label htmlFor={`${idPrefix}-csbf`}>Creator sells before entry</label><select id={`${idPrefix}-csbf`} className="select" value={form.creatorSoldBeforeFill} onChange={e => set("creatorSoldBeforeFill", e.target.value)} data-testid={`${idPrefix}-creatorSoldBeforeFill`}>
        <option value="">— (configuration file)</option><option value="FREE_CANCEL">Cancel before execution</option><option value="TX_LANDS">Transaction may still land</option></select>{notes.creatorSoldBeforeFill ? <small>{notes.creatorSoldBeforeFill}</small> : null}</div>
      <div className="field"><label htmlFor={`${idPrefix}-cse`}>Exit when the creator sells</label><select id={`${idPrefix}-cse`} className="select" value={form.creatorSellExit} onChange={e => set("creatorSellExit", e.target.value)} data-testid={`${idPrefix}-creatorSellExit`}>
        <option value="">— (configuration file)</option><option value="ENABLED">Enabled</option><option value="DISABLED">Disabled</option></select>{notes.creatorSellExit ? <small data-testid={`${idPrefix}-creatorSellExit-running`}>{notes.creatorSellExit}</small> : null}</div>
      {field("maxOpen", "Maximum open positions")}
      <div className="field"><label htmlFor={`${idPrefix}-team`}>Exit when the creator’s team sells</label><input id={`${idPrefix}-team`} className="input" value="Not implemented" readOnly title="BSC-V8-06: needs a proven team-wallet attribution and native sell signal" /></div>
    </div>
    <section className="direct-filter-block"><h3>Entry filters <span className="small">· {activeFilters(form.filters)} active</span></h3>
      <p className="small">Enable a filter to edit its minimum and maximum. A blank bound is unrestricted.{notes.filters ? ` ${notes.filters}` : ""}</p>
      <div className="filter-layout">{FILTER_GROUPS.map(g => <div className="filter-group" key={g.name}><h4>{g.name}</h4>
        {g.items.map(({ key, label, unit }) => {
          const v = form.filters[key] ?? { on: false, min: "", max: "" };
          const put = (next: Partial<typeof v>) => set("filters", { ...form.filters, [key]: { ...v, ...next } });
          return <div className="filter-row" key={key} data-testid={`${idPrefix}-filter-${key}`}>
            <label className="check-label"><input type="checkbox" checked={v.on} onChange={e => put({ on: e.target.checked })} aria-label={`${label} filter`} /><span>{label}{unit ? <> <span className="tiny">({unit})</span></> : null}</span></label>
            <label><span className="minmax-label">Min.</span><input className="input" inputMode="decimal" placeholder="–" value={v.min} disabled={!v.on} onChange={e => put({ min: e.target.value })} aria-label={`${label} minimum`} /></label>
            <label><span className="minmax-label">Max.</span><input className="input" inputMode="decimal" placeholder="–" value={v.max} disabled={!v.on} onChange={e => put({ max: e.target.value })} aria-label={`${label} maximum`} /></label>
          </div>;
        })}</div>)}</div>
      <p className="tiny mt">Incomplete market data never counts as a passed filter (DATA_INCOMPLETE).</p>
    </section>
  </fieldset>;
}
