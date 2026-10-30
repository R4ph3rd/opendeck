//! The built-in effect factory.

use crate::catalog::{self, CATALOG, EffectSpec};
use crate::core::{Dsp, Fx};
use crate::effects::{delays, loops, modfx, reverb, tone};
use opendeck_contracts::fx::{Effect, EffectError, EffectFactory, EffectInfo, EffectKey};

/// Creates the built-in effects (provider id [`crate::PROVIDER_ID`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct BuiltinFactory;

impl BuiltinFactory {
    /// New factory.
    pub fn new() -> Self {
        Self
    }
}

fn boxed<D: Dsp + Default>(spec: &'static EffectSpec) -> Box<dyn Effect> {
    Box::new(Fx::new(spec, D::default()))
}

impl EffectFactory for BuiltinFactory {
    fn provider(&self) -> &str {
        crate::PROVIDER_ID
    }

    fn effects(&self) -> Vec<EffectInfo> {
        CATALOG.iter().map(EffectSpec::info).collect()
    }

    fn create(&self, id: &str) -> Result<Box<dyn Effect>, EffectError> {
        let spec = catalog::find(id).ok_or_else(|| {
            EffectError::UnknownEffect(EffectKey {
                provider: crate::PROVIDER_ID.to_owned(),
                id: id.to_owned(),
            })
        })?;
        Ok(match id {
            "delay" => boxed::<delays::DelayFx>(spec),
            "echo" => boxed::<delays::EchoFx>(spec),
            "ping_pong" => boxed::<delays::PingPongFx>(spec),
            "spiral" => boxed::<delays::SpiralFx>(spec),
            "reverb" => boxed::<reverb::ReverbFx>(spec),
            "helix" => boxed::<loops::HelixFx>(spec),
            "flanger" => boxed::<modfx::FlangerFx>(spec),
            "phaser" => boxed::<modfx::PhaserFx>(spec),
            "filter" => boxed::<modfx::FilterFx>(spec),
            "trans" => boxed::<modfx::TransFx>(spec),
            "roll" => boxed::<loops::RollFx>(spec),
            "reverse_roll" => boxed::<loops::ReverseRollFx>(spec),
            "pitch" => boxed::<tone::PitchFx>(spec),
            "vinyl_brake" => boxed::<loops::VinylBrakeFx>(spec),
            "noise" => boxed::<tone::NoiseFx>(spec),
            "bit_crusher" => boxed::<tone::BitCrusherFx>(spec),
            "colour_filter" => boxed::<tone::ColourFilterFx>(spec),
            _ => {
                return Err(EffectError::Failed(format!(
                    "catalog entry {id} has no implementation"
                )));
            }
        })
    }
}
