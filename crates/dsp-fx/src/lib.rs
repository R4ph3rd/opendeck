//! OpenDeck built-in effects.
//!
//! Every effect implements `opendeck_contracts::fx::Effect` and is exposed
//! through an `EffectFactory` under the provider id [`PROVIDER_ID`]. The engine
//! never depends on this crate: app-core registers the factory (ADR 0006).
//!
//! Parameters, ranges, beat-sync limits and factory presets are data in
//! [`catalog`]; `docs/fx-catalog.md` is generated from the same data and a
//! test keeps them equal. Effects write the fully wet signal; wet/dry and
//! bypass belong to the host ([`slot::FxSlot`] is a reference host).
//! See `CONTEXT/dsp-fx.md`.

pub mod catalog;
mod core;
mod dsp;
mod effects;
mod factory;
pub mod preset;
pub mod slot;

pub use factory::BuiltinFactory;

/// Provider id of the built-in effect factory, used in persisted effect keys.
pub const PROVIDER_ID: &str = "builtin";

#[cfg(test)]
mod tests;
