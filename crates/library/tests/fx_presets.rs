//! User effect presets in the `fx_presets` table.

use opendeck_library::{Library, StoredFxPreset};

#[test]
fn fx_presets_round_trip() {
    let lib = Library::open_in_memory().unwrap();
    let mut p = StoredFxPreset {
        provider: "builtin".into(),
        effect_id: "echo".into(),
        name: "Mine".into(),
        params: "[[0,0.5]]".into(),
        wet: 0.4,
        beat_division: 0.5,
    };
    lib.save_fx_preset(&p).unwrap();
    p.wet = 0.7;
    lib.save_fx_preset(&p).unwrap();
    assert_eq!(lib.list_fx_presets().unwrap(), vec![p.clone()]);
    p.wet = 2.0;
    assert!(lib.save_fx_preset(&p).is_err());
    assert!(lib.delete_fx_preset("builtin", "echo", "Mine").unwrap());
    assert!(!lib.delete_fx_preset("builtin", "echo", "Mine").unwrap());
    assert!(lib.list_fx_presets().unwrap().is_empty());
}
