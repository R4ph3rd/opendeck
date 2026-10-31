import { Badge, Button, Kbd } from "@opendeck/design-system";
import { useEffect, useMemo, useState } from "react";
import { findConflicts, isBindable, keyLabel, shortcutName } from "../../lib/shortcuts";
import { useShortcutStore } from "../../state/shortcuts";
import { Section } from "./parts";

/** Settings, Keyboard: every shortcut with its key; rebind by pressing a key. */
export function KeyboardSettings() {
  const shortcuts = useShortcutStore((s) => s.shortcuts);
  const overrides = useShortcutStore((s) => s.overrides);
  const bind = useShortcutStore((s) => s.bind);
  const unbind = useShortcutStore((s) => s.unbind);
  const resetOne = useShortcutStore((s) => s.resetOne);
  const resetAll = useShortcutStore((s) => s.resetAll);
  const [capturing, setCapturing] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const conflicts = useMemo(() => findConflicts(shortcuts), [shortcuts]);
  const names = useMemo(() => new Map(shortcuts.map((s) => [s.id, shortcutName(s)])), [shortcuts]);

  // While capturing, the next key press becomes the binding. Escape cancels.
  useEffect(() => {
    if (capturing === null) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (event.code === "Escape") {
        setCapturing(null);
        setMessage(null);
        return;
      }
      if (event.ctrlKey || event.metaKey || event.altKey) {
        setMessage("Combinations with Ctrl, Alt or Cmd are not supported.");
        return;
      }
      if (!isBindable(event.code)) {
        setMessage(`${keyLabel(event.code)} cannot be used.`);
        return;
      }
      bind(capturing, event.code);
      setCapturing(null);
      setMessage(null);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [capturing, bind]);

  return (
    <Section
      title="Keyboard shortcuts"
      description="Keys are physical positions, so they stay put on any keyboard layout."
    >
      <div className="flex items-center justify-between gap-2">
        <p role="status" className="text-xs text-fg-muted">
          {capturing !== null
            ? (message ?? `Press a key for ${names.get(capturing) ?? "this action"}, or Escape to cancel.`)
            : "Choose Rebind, then press the new key."}
        </p>
        <Button size="sm" onClick={resetAll} disabled={Object.keys(overrides).length === 0}>
          Reset all to defaults
        </Button>
      </div>
      <ul className="flex flex-col" aria-label="Shortcuts">
        {shortcuts.map((s) => {
          const clash = conflicts.get(s.id);
          const changed = s.id in overrides;
          return (
            <li
              key={s.id}
              data-testid={`shortcut-${s.id}`}
              className="flex items-center gap-2 border-b border-border py-1"
            >
              <span className="min-w-0 flex-1 truncate text-sm">{shortcutName(s)}</span>
              {clash ? (
                <Badge variant="danger" title={clash.map((id) => names.get(id)).join(", ")}>
                  Conflict with {names.get(clash[0] ?? "") ?? "another action"}
                </Badge>
              ) : null}
              <Kbd>{keyLabel(s.code)}</Kbd>
              <Button
                size="sm"
                variant={capturing === s.id ? "primary" : "secondary"}
                aria-label={`Rebind ${shortcutName(s)}`}
                onClick={() => {
                  setMessage(null);
                  setCapturing(capturing === s.id ? null : s.id);
                }}
              >
                {capturing === s.id ? "Press a key" : "Rebind"}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Clear ${shortcutName(s)}`}
                disabled={s.code === ""}
                onClick={() => unbind(s.id)}
              >
                Clear
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Reset ${shortcutName(s)}`}
                disabled={!changed}
                onClick={() => resetOne(s.id)}
              >
                Reset
              </Button>
            </li>
          );
        })}
      </ul>
    </Section>
  );
}
