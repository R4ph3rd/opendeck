//! Structure and bookkeeping checks on `mappings/ddj-flx4.json`: every entry
//! cites its row, the pad table is complete, outputs do not collide, and every
//! `verify` marker is tracked in `docs/midi-open-questions.md`.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use opendeck_contracts::control::{ControlAction, FeedbackTarget};
use opendeck_contracts::pad::PadMode;
use opendeck_midi::mapping::{Mapping, MessageKind};

fn mapping() -> Mapping {
    opendeck_midi::flx4_mapping().expect("FLX4 mapping loads")
}

fn is_ref(r: &str) -> bool {
    if let Some((section, row)) = r.split_once('-') {
        return matches!(section, "1" | "2" | "3" | "4" | "5")
            && !row.is_empty()
            && row.bytes().all(|b| b.is_ascii_digit());
    }
    r.starts_with("illumination:") || r.starts_with("settings:")
}

#[test]
fn every_entry_cites_a_row_of_the_message_list() {
    let m = mapping();
    assert!(m.source.contains("List of MIDI messages"));
    for x in &m.modifiers {
        assert!(
            x.source_ref.as_deref().is_some_and(is_ref),
            "modifier {}",
            x.id
        );
    }
    for c in &m.controls {
        assert!(
            c.source_ref.as_deref().is_some_and(is_ref),
            "control {}",
            c.id
        );
        assert!(c.label.is_some(), "control {} has no label", c.id);
    }
    for f in &m.feedback {
        assert!(
            f.source_ref.as_deref().is_some_and(is_ref),
            "feedback {:?}",
            f.target
        );
    }
    for u in &m.unmapped {
        assert!(is_ref(&u.source_ref), "unmapped {}", u.label);
        assert!(!u.reason.is_empty());
    }
}

#[test]
fn counts() {
    let m = mapping();
    println!(
        "{} modifiers, {} controls, {} feedback, {} unmapped, {} verify",
        m.modifiers.len(),
        m.controls.len(),
        m.feedback.len(),
        m.unmapped.len(),
        m.verify_count()
    );
    assert_eq!(m.modifiers.len(), 2);
    // 2 decks x (8 mode buttons + 128 pads) plus the rest of the deck and global controls.
    assert_eq!(m.controls.len(), 359);
    assert_eq!(m.feedback.len(), 304);
}

#[test]
fn pad_table_is_complete() {
    let m = mapping();
    let mut seen = HashSet::new();
    for c in &m.controls {
        if let ControlAction::Pad {
            deck,
            mode,
            index,
            shifted,
        } = c.action
        {
            assert!(index < 8);
            assert!(seen.insert((deck.index(), mode.index(), index, shifted)));
            let opendeck_midi::mapping::MidiInput::Note { channel, number } = c.midi else {
                panic!("pad {} is not a note", c.id)
            };
            // List 5-5: channels 8/9 (deck 1) and 10/11 (deck 2), note = mode * 16 + pad.
            let want_channel = 8 + 2 * deck.index() as u8 + u8::from(shifted);
            assert_eq!(channel, want_channel, "{}", c.id);
            assert_eq!(number, (mode.index() as u8) * 16 + index, "{}", c.id);
        }
    }
    assert_eq!(seen.len(), 2 * 8 * 8 * 2);
    for mode in PadMode::ALL {
        let buttons = m
            .controls
            .iter()
            .filter(
                |c| matches!(c.action, ControlAction::PadModeSelect { mode: x, .. } if x == mode),
            )
            .count();
        assert_eq!(buttons, 2, "one mode button per deck for {mode:?}");
    }
}

#[test]
fn shift_partners_match_the_mode_buttons() {
    let m = mapping();
    // Q01: a mode button and its +SHIFT note select the modes that `shift_partner` pairs.
    let mut by_note: BTreeMap<(u8, u8), PadMode> = BTreeMap::new();
    for c in &m.controls {
        if let (
            ControlAction::PadModeSelect { mode, .. },
            opendeck_midi::mapping::MidiInput::Note { channel, number },
        ) = (c.action, c.midi)
        {
            by_note.insert((channel, number), mode);
        }
    }
    for (base, shifted) in [(0x1B, 0x69), (0x1E, 0x6B), (0x20, 0x6D), (0x22, 0x6F)] {
        assert_eq!(by_note[&(1, base)].shift_partner(), by_note[&(1, shifted)]);
        assert_eq!(by_note[&(2, base)].shift_partner(), by_note[&(2, shifted)]);
    }
}

#[test]
fn feedback_addresses_do_not_collide() {
    let m = mapping();
    let mut seen: BTreeMap<(MessageKind, u8, u8), FeedbackTarget> = BTreeMap::new();
    for f in &m.feedback {
        let key = (f.midi.kind, f.midi.channel, f.midi.number);
        if let Some(other) = seen.insert(key, f.target) {
            panic!("{key:?} is driven by {other:?} and {:?}", f.target);
        }
    }
}

#[test]
fn input_addresses_are_channel_messages_of_the_list() {
    let m = mapping();
    for c in &m.controls {
        assert!((1..=16).contains(&c.midi.channel()), "{}", c.id);
    }
    assert_eq!(m.devices.input, ["DDJ-FLX4"]);
    assert_eq!(m.devices.output, ["DDJ-FLX4"]);
}

/// Question ids at the start of each `verify` text must have a `TODO(verify)`
/// row in the open-questions doc, and the doc's counts must match the file.
#[test]
fn verify_markers_are_tracked_in_the_open_questions_doc() {
    let m = mapping();
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/midi-open-questions.md"
    ))
    .expect("docs/midi-open-questions.md exists");

    let open: BTreeSet<String> = doc
        .lines()
        .filter(|l| l.starts_with("| Q") && l.contains("TODO(verify)"))
        .filter_map(|l| l.split('|').nth(1))
        .map(|id| id.trim().to_owned())
        .collect();
    assert!(open.len() >= 10, "doc lists {} open questions", open.len());

    let mut used = BTreeSet::new();
    for item in m.verify_items() {
        let ids: Vec<&str> = item
            .text
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| {
                w.len() == 3 && w.starts_with('Q') && w[1..].bytes().all(|b| b.is_ascii_digit())
            })
            .collect();
        assert!(
            item.text.starts_with('Q') && !ids.is_empty(),
            "verify of {} {} must start with a question id: {}",
            item.kind,
            item.id,
            item.text
        );
        for id in ids {
            assert!(
                open.contains(id),
                "{id} ({} {}) has no TODO(verify) row",
                item.kind,
                item.id
            );
            used.insert(id.to_owned());
        }
    }

    let ids: Vec<String> = used.iter().cloned().collect();
    let expected = format!(
        "Verify markers in `ddj-flx4.json`: {}, over {} questions ({})",
        m.verify_count(),
        used.len(),
        ids.join(", ")
    );
    assert!(
        doc.contains(&expected),
        "docs/midi-open-questions.md must say: {expected}"
    );
}
