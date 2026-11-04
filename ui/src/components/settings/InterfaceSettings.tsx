import {
  Button,
  Input,
  SegmentedControl,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@opendeck/design-system";
import { useState } from "react";
import { UI_SCALE_CHOICES, type UiScalePref } from "../../lib/uiScale";
import { DEFAULT_LAYOUT, usePrefsStore, type Theme } from "../../state/prefs";
import { ZOOM_SECONDS } from "../../viz/waveformMath";
import { useVizStore } from "../../viz/useTrackData";
import { Row, Section } from "./parts";

/** Settings, Interface: theme, waveform zoom and panel layouts. */
export function InterfaceSettings() {
  const theme = usePrefsStore((s) => s.theme);
  const setTheme = usePrefsStore((s) => s.setTheme);
  const uiScale = usePrefsStore((s) => s.uiScale);
  const setUiScale = usePrefsStore((s) => s.setUiScale);
  const zoom = usePrefsStore((s) => s.waveformZoom);
  const setZoom = usePrefsStore((s) => s.setWaveformZoom);
  const presets = usePrefsStore((s) => s.layoutPresets);
  const save = usePrefsStore((s) => s.saveLayoutPreset);
  const restore = usePrefsStore((s) => s.restoreLayout);
  const remove = usePrefsStore((s) => s.deleteLayoutPreset);
  const [name, setName] = useState("");

  const chooseZoom = (value: string) => {
    const index = Number(value);
    setZoom(index);
    for (let deck = 0; deck < 4; deck++) {
      useVizStore.getState().setZoom(deck, index);
    }
  };

  return (
    <>
      <Section title="Appearance">
        <Row label="Theme">
          <SegmentedControl
            aria-label="Theme"
            value={theme}
            onValueChange={(v) => setTheme(v as Theme)}
            options={[
              { value: "dark", label: "Dark" },
              { value: "light", label: "Light" },
            ]}
          />
        </Row>
        <Row label="Interface scale" hint="Auto follows the window height, up to 200%.">
          <Select
            value={String(uiScale)}
            onValueChange={(v) => setUiScale(v === "auto" ? "auto" : (Number(v) as UiScalePref))}
          >
            <SelectTrigger aria-label="Interface scale" className="w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {UI_SCALE_CHOICES.map((choice) => (
                <SelectItem key={choice} value={String(choice)}>
                  {choice === "auto" ? "Auto" : `${choice * 100}%`}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Row>
        <Row label="Default waveform zoom" hint="Window shown on the deck waveforms at launch.">
          <Select value={String(zoom)} onValueChange={chooseZoom}>
            <SelectTrigger aria-label="Default waveform zoom" className="w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ZOOM_SECONDS.map((seconds, index) => (
                <SelectItem key={seconds} value={String(index)}>
                  {seconds} s
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Row>
      </Section>

      <Section
        title="Layouts"
        description="Save the panel sizes and restore them later. The 2-deck layout is the default; 4 decks are reserved."
      >
        <ul className="flex flex-col gap-1" aria-label="Layout presets">
          <li className="flex items-center gap-2">
            <span className="flex-1 text-sm">2-deck default</span>
            <Button size="sm" onClick={() => restore(DEFAULT_LAYOUT)}>
              Restore
            </Button>
          </li>
          {presets.map((preset) => (
            <li key={preset.name} className="flex items-center gap-2">
              <span className="flex-1 truncate text-sm">{preset.name}</span>
              <Button size="sm" onClick={() => restore(preset.layout)}>
                Restore
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Delete ${preset.name}`}
                onClick={() => remove(preset.name)}
              >
                Delete
              </Button>
            </li>
          ))}
        </ul>
        <form
          className="flex gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            save(name);
            setName("");
          }}
        >
          <Input
            aria-label="Layout name"
            placeholder="Layout name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Button type="submit" disabled={name.trim() === ""}>
            Save current layout
          </Button>
        </form>
      </Section>
    </>
  );
}
