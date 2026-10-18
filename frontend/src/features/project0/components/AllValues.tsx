import { useState } from "react";
import type { JsonObject, JsonValue } from "@bufbuild/protobuf";
import { allValues } from "../api/project0Adapter";
import { DetailRow } from "./PanelPrimitives";
import { formatField } from "../model/statusFormat";
import { useProject0Store } from "../store/project0Store";

const isObj = (v: JsonValue): v is JsonObject => v !== null && typeof v === "object" && !Array.isArray(v);
const isNamedValue = (v: JsonValue) => isObj(v) && "name" in v && "value" in v;
const isParameter = (v: JsonValue) => isObj(v) && "field" in v && "unit" in v && "value" in v;

function ListEntry({ name, items }: { name: string; items: JsonValue[] }) {
  if (items.length === 0) return <DetailRow title={name} value="none" />;
  if (items.every(isNamedValue))
    return <>{(items as JsonObject[]).map((c) => (
      <DetailRow key={String(c.name)} title={String(c.name)} value={formatField(String(c.name), c.value)} />
    ))}</>;
  if (items.every(isParameter))
    return <>{(items as JsonObject[]).map((p) => (
      <DetailRow key={String(p.field)} title={String(p.field)}
        value={`${p.value} ${p.unit} · default ${p.default_value} · ${p.env} · ${p.source}`} />
    ))}</>;
  if (items.every((x) => !isObj(x))) return <DetailRow title={name} value={items.map(String).join(", ")} />;
  return <>{(items as JsonObject[]).map((x, i) => <Entries key={i} obj={x} />)}</>;
}

function Entry({ name, value }: { name: string; value: JsonValue }) {
  if (Array.isArray(value)) return <><p className="p0-label">{name}</p><ListEntry name={name} items={value} /></>;
  if (isObj(value)) return <><p className="p0-label">{name}</p><Entries obj={value} /></>;
  return <DetailRow title={name} value={formatField(name, value)} />;
}

function Entries({ obj }: { obj: JsonObject }) {
  return <>{Object.entries(obj).map(([k, v]) => (v === undefined ? null : <Entry key={k} name={k} value={v} />))}</>;
}

/** Every value Project 0 reported, built from the contract; rendered only while open. */
export function AllValues() {
  useProject0Store((s) => s.capture.meta);
  useProject0Store((s) => s.system.meta);
  const [open, setOpen] = useState(false);
  const values = open ? allValues() : null;
  return (
    <details open={open} onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>All values reported by Project 0</summary>
      {values ? (
        <>
          <p className="p0-label">project0.status</p>
          {values.system ? <Entries obj={values.system} /> : <p className="p0-quiet">Not received.</p>}
          <p className="p0-label">project0.capture.state</p>
          {values.capture ? <Entries obj={values.capture} /> : <p className="p0-quiet">Not received.</p>}
        </>
      ) : null}
    </details>
  );
}
