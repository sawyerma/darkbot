import { FILTER_GROUPS, activeFilters, type StrategyForm } from "../model/strategyForm";

/** The V8 module settings (the ten native keys) and the 15 entry filters, always visible
 *  (FE-V8-05/06). The values are the native settings; `disabled` locks the whole form. A choice the
 *  owner has no value for shows the configuration file's (never selectable: no native "unset");
 *  the team-sell exit is shown as what it is: not implemented. The native ranges and the features'
 *  units are the owner's settings state, shown as hints. */
export function StrategyEditor({ form, onChange, disabled, venues, idPrefix, notes = {}, ranges = {}, features = {}, name, market }: {
  form: StrategyForm; onChange: (f: StrategyForm) => void; disabled: boolean; venues: string; idPrefix: string;
  /** Per field: the running strategy's own value where the stored one differs (effective at the next start). */
  notes?: Partial<Record<keyof StrategyForm, string>>;
  /** Per field: its key's native range (StrategySettingSpec). */
  ranges?: Partial<Record<keyof StrategyForm, string>>;
  /** Per filter: its native unit, places and sign (StrategyFeatureSpec). */
  features?: Readonly<Record<string, string>>;
  /** A training module's name (V8 “Module name”, first field). */
  name?: { value: string; onChange: (v: string) => void };
  /** A training module's market choice; without it the venues are shown read-only. */
  market?: { value: string; options: readonly { key: string; label: string }[]; onChange: (key: string) => void };
}) {
  const set = <K extends keyof StrategyForm>(k: K, v: StrategyForm[K]) => onChange({ ...form, [k]: v });
  const field = (k: "stake" | "target" | "stop" | "hold" | "observation" | "maxOpen" | "firstBuyerWindow", label: string) =>
    <div className="field"><label htmlFor={`${idPrefix}-${k}`}>{label}</label><input id={`${idPrefix}-${k}`} className="input" inputMode="decimal" value={form[k]} title={ranges[k]}
      onChange={e => set(k, e.target.value)} data-testid={`${idPrefix}-${k}`} />{notes[k] ? <small data-testid={`${idPrefix}-${k}-running`}>{notes[k]}</small> : null}</div>;
  return <fieldset disabled={disabled} style={{ padding: 0, border: 0, margin: 0, minWidth: 0 }}>
    <div className="editor-fields">
      {name ? <div className="field"><label htmlFor={`${idPrefix}-name`}>Module name</label><input id={`${idPrefix}-name`} className="input" value={name.value} required
        onChange={e => name.onChange(e.target.value)} data-testid={`${idPrefix}-name`} /></div> : null}
      {market ? <div className="field"><label htmlFor={`${idPrefix}-market`}>Market</label><select id={`${idPrefix}-market`} className="select" value={market.value} onChange={e => market.onChange(e.target.value)} data-testid={`${idPrefix}-market`}>
        {market.value === "" ? <option value="" disabled>Choose a market</option> : null}{market.options.map(o => <option key={o.key} value={o.key}>{o.label}</option>)}</select></div>
        : <div className="field"><label htmlFor={`${idPrefix}-market`}>Market</label><input id={`${idPrefix}-market`} className="input" value={venues} readOnly title="The venues Strategy C follows (source modes, Settings)" /></div>}
      {field("stake", "Investment per buy · BNB")}
      {field("target", "Profit target · net ×")}
      {field("stop", "Stop loss · net ×")}
      {field("hold", "Maximum hold time · s")}
      {field("observation", "Observation window · s")}
      {field("firstBuyerWindow", "Early buyers in window")}
      <div className="field"><label htmlFor={`${idPrefix}-csbf`}>Creator sells before entry</label><select id={`${idPrefix}-csbf`} className="select" value={form.creatorSoldBeforeFill} onChange={e => set("creatorSoldBeforeFill", e.target.value)} data-testid={`${idPrefix}-creatorSoldBeforeFill`}>
        <option value="" disabled>— (configuration file)</option><option value="FREE_CANCEL">Cancel before execution</option><option value="TX_LANDS">Transaction may still land</option></select>{notes.creatorSoldBeforeFill ? <small>{notes.creatorSoldBeforeFill}</small> : null}</div>
      <div className="field"><label htmlFor={`${idPrefix}-cse`}>Exit when the creator sells</label><select id={`${idPrefix}-cse`} className="select" value={form.creatorSellExit} onChange={e => set("creatorSellExit", e.target.value)} data-testid={`${idPrefix}-creatorSellExit`}>
        <option value="" disabled>— (configuration file)</option><option value="ENABLED">Enabled</option><option value="DISABLED">Disabled</option></select>{notes.creatorSellExit ? <small data-testid={`${idPrefix}-creatorSellExit-running`}>{notes.creatorSellExit}</small> : null}</div>
      {field("maxOpen", "Maximum open positions")}
      <div className="field"><label htmlFor={`${idPrefix}-team`}>Exit when the creator’s team sells</label><input id={`${idPrefix}-team`} className="input" value="Not implemented" readOnly title="BSC-V8-06: needs a proven team-wallet attribution and native sell signal" /></div>
    </div>
    <section className="direct-filter-block"><h3>Entry filters <span className="small">· {activeFilters(form.filters)} active</span></h3>
      <p className="small">Enable a filter to edit its minimum and maximum. A blank bound is unrestricted.{notes.filters ? ` ${notes.filters}` : ""}</p>
      <div className="filter-layout">{FILTER_GROUPS.map(g => <div className="filter-group" key={g.name}><h4>{g.name}</h4>
        {g.items.map(({ key, label, unit }) => {
          const v = form.filters[key] ?? { on: false, min: "", max: "" };
          const put = (next: Partial<typeof v>) => set("filters", { ...form.filters, [key]: { ...v, ...next } });
          return <div className="filter-row" key={key} data-testid={`${idPrefix}-filter-${key}`} title={features[key]}>
            <label className="check-label"><input type="checkbox" checked={v.on} onChange={e => put({ on: e.target.checked })} aria-label={`${label} filter`} /><span>{label}{unit ? <> <span className="tiny">({unit})</span></> : null}</span></label>
            <label><span className="minmax-label">Min.</span><input className="input" inputMode="decimal" placeholder="–" value={v.min} disabled={!v.on} onChange={e => put({ min: e.target.value })} aria-label={`${label} minimum`} /></label>
            <label><span className="minmax-label">Max.</span><input className="input" inputMode="decimal" placeholder="–" value={v.max} disabled={!v.on} onChange={e => put({ max: e.target.value })} aria-label={`${label} maximum`} /></label>
          </div>;
        })}</div>)}</div>
      <p className="tiny mt">Incomplete market data never counts as a passed filter (DATA_INCOMPLETE).</p>
    </section>
  </fieldset>;
}
