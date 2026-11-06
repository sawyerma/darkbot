import { Drawer, Icon, type IconName } from "./V8";

const AREAS: ReadonlyArray<[IconName, string, string]> = [
  ["home", "Overview", "Key information at a glance."],
  ["training", "Training", "Test custom modules without real money."],
  ["live", "Live", "Manage live trading and results."],
  ["wallet", "Wallets", "Manage balances, transfers and PnL."],
];

/** The reference's quick guide ("How it works"). */
export function GuideDrawer({ onClose }: { onClose: () => void }) {
  return <Drawer title="How DarkBot works" kicker="QUICK GUIDE" onClose={onClose} footer={<button type="button" className="btn primary" onClick={onClose}>Close</button>}>
    {AREAS.map(([icon, title, sub]) => <div className="rowitem" key={title}><span className="symbol"><Icon name={icon} size="s" /></span><div className="grow"><div className="item-title">{title}</div><div className="item-sub">{sub}</div></div></div>)}
    <div className="rowitem"><span className="symbol"><Icon name="settings" size="s" /></span><div className="grow"><div className="item-title">Settings</div><div className="item-sub">Data connections, native switches, readiness and evidence.</div></div></div>
  </Drawer>;
}
