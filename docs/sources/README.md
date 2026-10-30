# External sources

Third-party reference documents are kept here locally and are git-ignored, because their copyright belongs to their owners. Docs and mapping files cite them by name.

| File | Source | Used for |
|---|---|---|
| `ddj-flx4-manual.md` | DDJ-FLX4 operating manual (Mode d'emploi, French, DRI1805A), text supplied by the project owner | FX control model, audio specs, `docs/fx-catalog.md` |
| `DDJ-FLX4_MIDI_message_List_E1.pdf` | Pioneer DJ, "DDJ-FLX4 List of MIDI messages", edition E1, supplied by the project owner | `mappings/ddj-flx4.json`, `docs/midi-open-questions.md` |

To regenerate the text extract used by agents: `pdftotext -layout docs/sources/DDJ-FLX4_MIDI_message_List_E1.pdf docs/sources/ddj-flx4-midi.txt`.
