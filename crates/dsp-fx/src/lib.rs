//! OpenDeck built-in effects.
//!
//! Every effect implements `opendeck_contracts::fx::Effect` and is exposed
//! through an `EffectFactory` under the provider id [`PROVIDER_ID`]. The engine
//! never depends on this crate: app-core registers the factory (ADR 0006).
//! Parameters come from `docs/fx-catalog.md`, never invented.
//! See `CONTEXT/dsp-fx.md`.

/// Provider id of the built-in effect factory, used in persisted effect keys.
pub const PROVIDER_ID: &str = "builtin";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_is_stable() {
        assert_eq!(PROVIDER_ID, "builtin");
    }
}
