import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  SettingsLayout,
  SettingsNav,
  type SettingsNavItem,
} from "@opendeck/design-system";
import { AudioLines, Keyboard, Library, MonitorCog, Piano, Wrench } from "lucide-react";
import { SETTINGS_PAGES, useAppStore, type SettingsPage } from "../state/app";
import { MidiSettings } from "./MidiSettings";
import { AdvancedSettings } from "./settings/AdvancedSettings";
import { AudioSettingsPage } from "./settings/AudioSettings";
import { InterfaceSettings } from "./settings/InterfaceSettings";
import { KeyboardSettings } from "./settings/KeyboardSettings";
import { LibrarySettings } from "./settings/LibrarySettings";

const PAGES: SettingsNavItem[] = [
  { id: "audio", label: "Audio", icon: <AudioLines aria-hidden /> },
  { id: "midi", label: "MIDI", icon: <Piano aria-hidden /> },
  { id: "library", label: "Library", icon: <Library aria-hidden /> },
  { id: "interface", label: "Interface", icon: <MonitorCog aria-hidden /> },
  { id: "keyboard", label: "Keyboard", icon: <Keyboard aria-hidden /> },
  { id: "advanced", label: "Advanced", icon: <Wrench aria-hidden /> },
];

/** Settings dialog: a page list on the left, the page on the right. */
export function SettingsDialog() {
  const open = useAppStore((s) => s.settingsOpen);
  const setOpen = useAppStore((s) => s.setSettingsOpen);
  const page = useAppStore((s) => s.settingsPage);
  const setPage = useAppStore((s) => s.setSettingsPage);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="flex h-[70vh] flex-col gap-0 p-0 sm:max-w-4xl">
        <DialogHeader className="border-b border-border p-4">
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Changes to audio restart the output. Other changes apply at once.
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1">
          <SettingsLayout
            sidebar={
              <SettingsNav
                items={PAGES}
                value={page}
                onValueChange={(id) =>
                  setPage(SETTINGS_PAGES.find((p: SettingsPage) => p === id) ?? "audio")
                }
              />
            }
          >
            {open && page === "audio" ? <AudioSettingsPage onClose={() => setOpen(false)} /> : null}
            {open && page === "midi" ? <MidiSettings /> : null}
            {open && page === "library" ? <LibrarySettings /> : null}
            {open && page === "interface" ? <InterfaceSettings /> : null}
            {open && page === "keyboard" ? <KeyboardSettings /> : null}
            {open && page === "advanced" ? <AdvancedSettings /> : null}
          </SettingsLayout>
        </div>
      </DialogContent>
    </Dialog>
  );
}
