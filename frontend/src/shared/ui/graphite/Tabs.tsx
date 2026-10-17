import { useCallback, type KeyboardEvent } from "react";

export interface TabItem<K extends string> {
  key: K;
  label: string;
}

interface Props<K extends string> {
  items: readonly TabItem<K>[];
  value: K;
  onChange: (key: K) => void;
  variant?: "pills" | "sub";
  ariaLabel: string;
}

// Pill-Tabs (.g-tabs) und Unterreiter (.g-subtabs) mit Tastaturnavigation (Pfeile, Home, End).
export function Tabs<K extends string>({ items, value, onChange, variant = "pills", ariaLabel }: Props<K>) {
  const onKey = useCallback(
    (e: KeyboardEvent<HTMLDivElement>) => {
      const idx = items.findIndex((i) => i.key === value);
      if (idx < 0 || items.length === 0) return;
      let next = idx;
      if (e.key === "ArrowRight") next = (idx + 1) % items.length;
      else if (e.key === "ArrowLeft") next = (idx - 1 + items.length) % items.length;
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = items.length - 1;
      else return;
      e.preventDefault();
      const nextItem = items[next];
      if (nextItem) {
        onChange(nextItem.key);
        const buttons = e.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]');
        buttons[next]?.focus();
      }
    },
    [items, value, onChange],
  );

  return (
    <div className={variant === "pills" ? "g-tabs" : "g-subtabs"} role="tablist" aria-label={ariaLabel} onKeyDown={onKey}>
      {items.map((it) => {
        const selected = it.key === value;
        return (
          <button
            key={it.key}
            type="button"
            role="tab"
            aria-selected={selected}
            tabIndex={selected ? 0 : -1}
            className={selected ? "g-selected" : undefined}
            onClick={() => onChange(it.key)}
          >
            {it.label}
          </button>
        );
      })}
    </div>
  );
}
