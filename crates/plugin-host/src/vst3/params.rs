//! Controller parameters as engine descriptors, and the state container.
//!
//! # Parameter model
//!
//! VST3 parameters are normalized `[0, 1]` natively, and the controller
//! converts to display values (`normalizedParamToPlain`, `getParamStringByValue`).
//! The engine's `set_parameter` also takes normalized values (ADR 0008), so a
//! descriptor here is `min 0`, `max 1`, `default` the normalized default, and
//! `Stepped { steps: stepCount + 1 }` for discrete parameters (VST3 maps step
//! `i` of `stepCount` to `i / stepCount`, which is what `Stepped` does).
//! Plain values and text come from the controller on the main thread
//! (`Vst3Instance::normalized_to_plain`, `value_text`), because the mapping
//! is the plugin's own and may not be linear.
//!
//! Hidden (`kIsHidden`), program-change (`kIsProgramChange`) and read-only
//! parameters are not listed and not settable from the host; read-only
//! outputs (meters) still reach the UI through the output changes.

use opendeck_contracts::fx::{ParamCurve, ParamDescriptor, ParamId, ParamUnit};
use vst3::ComPtr;
use vst3::Steinberg::Vst::{
    IEditController, IEditControllerTrait, ParameterInfo, ParameterInfo_::ParameterFlags_,
};

use super::util::{e, is_ok, utf16_to_string};
use crate::PluginError;

/// One controller parameter.
#[derive(Debug, Clone)]
pub(super) struct ParamEntry {
    /// Descriptor in normalized units.
    pub desc: ParamDescriptor,
    /// Listed to the UI and accepted from the host.
    pub settable: bool,
}

/// Reads the controller's parameters, sorted by id without duplicates.
pub(super) fn read_params(controller: &ComPtr<IEditController>) -> Vec<ParamEntry> {
    // SAFETY: plain calls on a live controller; `info` is plain data.
    let count = unsafe { controller.getParameterCount() }.max(0);
    let mut out = Vec::new();
    for index in 0..count {
        // SAFETY: as above.
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        if !is_ok(unsafe { controller.getParameterInfo(index, &mut info) }) {
            continue;
        }
        let flags = info.flags;
        let hidden = flags & e(ParameterFlags_::kIsHidden | ParameterFlags_::kIsProgramChange) != 0;
        let read_only = flags & e(ParameterFlags_::kIsReadOnly) != 0;
        let title = utf16_to_string(&info.title);
        let name = if title.trim().is_empty() {
            utf16_to_string(&info.shortTitle)
        } else {
            title
        };
        let steps = info.stepCount;
        let curve = if (1..=65_534).contains(&steps) {
            ParamCurve::Stepped {
                steps: steps as u16 + 1,
            }
        } else {
            ParamCurve::Linear
        };
        let default = if info.defaultNormalizedValue.is_finite() {
            info.defaultNormalizedValue.clamp(0.0, 1.0)
        } else {
            0.0
        };
        out.push(ParamEntry {
            desc: ParamDescriptor {
                id: ParamId(info.id),
                name: if name.trim().is_empty() {
                    format!("Param {}", info.id)
                } else {
                    name.trim().to_owned()
                },
                unit: ParamUnit::None,
                min: 0.0,
                max: 1.0,
                default: default as f32,
                curve,
            },
            settable: !hidden && !read_only,
        });
    }
    out.sort_by_key(|e| e.desc.id);
    out.dedup_by_key(|e| e.desc.id);
    out
}

const STATE_MAGIC: &[u8; 4] = b"OPV3";
const STATE_VERSION: u8 = 1;

/// Packs component and controller state: `OPV3`, version byte 1, then each
/// part as a little-endian `u32` length and its bytes (component first).
pub(super) fn encode_state(component: &[u8], controller: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(component.len() + controller.len() + 13);
    out.extend_from_slice(STATE_MAGIC);
    out.push(STATE_VERSION);
    for part in [component, controller] {
        out.extend_from_slice(&u32::try_from(part.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(part);
    }
    out
}

/// Splits bytes from [`encode_state`] into `(component, controller)`.
pub(super) fn decode_state(bytes: &[u8]) -> Result<(&[u8], &[u8]), PluginError> {
    let bad = || PluginError::Plugin("not a VST3 state saved by OpenDeck".into());
    let rest = bytes.strip_prefix(STATE_MAGIC).ok_or_else(bad)?;
    let (version, mut rest) = rest.split_first().ok_or_else(bad)?;
    if *version != STATE_VERSION {
        return Err(PluginError::Plugin(format!(
            "unsupported VST3 state version {version}"
        )));
    }
    let mut part = || -> Result<&[u8], PluginError> {
        let (len, tail) = rest.split_first_chunk::<4>().ok_or_else(bad)?;
        let len = u32::from_le_bytes(*len) as usize;
        let (data, tail) = tail.split_at_checked(len).ok_or_else(bad)?;
        rest = tail;
        Ok(data)
    };
    let component = part()?;
    let controller = part()?;
    if !rest.is_empty() {
        return Err(bad());
    }
    Ok((component, controller))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips() {
        let packed = encode_state(b"component", b"");
        assert_eq!(
            decode_state(&packed).unwrap(),
            (&b"component"[..], &b""[..])
        );
        let packed = encode_state(&[], b"ctrl");
        assert_eq!(decode_state(&packed).unwrap(), (&b""[..], &b"ctrl"[..]));
    }

    #[test]
    fn broken_state_is_refused() {
        assert!(decode_state(&[1, 2, 3]).is_err());
        let good = encode_state(b"abc", b"de");
        assert!(decode_state(&good[..good.len() - 1]).is_err(), "truncated");
        let mut long = good.clone();
        long.push(0);
        assert!(decode_state(&long).is_err(), "trailing bytes");
        let mut v2 = good;
        v2[4] = 2;
        assert!(decode_state(&v2).is_err(), "unknown version");
    }
}
