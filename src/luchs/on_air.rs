use crate::LayerSnapshot;

use super::deck_state::DeckRole;

/// Minimum on_air byte to count a deck as "contributing to master output".
/// The on_air byte is a 0-255 fader-position weight from the TimePacket.
const ON_AIR_THRESHOLD: u8 = 1;

/// Pick the on-air deck across `layers[0..4]`.
///
/// Primary strategy: highest on-air byte above threshold wins. Ties (within 4)
/// go to the lowest deck index.
///
/// Fallback: if no deck has any on_air weight (e.g. simulator that hasn't
/// initialised fader byte), but exactly one deck is playing, use that one.
/// This keeps the UI usable without sacrificing correctness when the bridge
/// reports proper fader bytes.
pub fn pick_on_air(layers: &[LayerSnapshot]) -> Option<usize> {
    let mut best: Option<(u8, usize)> = None;
    for (i, snap) in layers.iter().take(4).enumerate() {
        if snap.on_air >= ON_AIR_THRESHOLD {
            match best {
                None => best = Some((snap.on_air, i)),
                Some((current_w, _)) if snap.on_air > current_w + 4 => {
                    best = Some((snap.on_air, i));
                }
                _ => {}
            }
        }
    }
    if let Some((_, i)) = best {
        return Some(i);
    }

    // Fallback: exactly one deck is playing → treat as on-air.
    let playing: Vec<usize> = layers
        .iter()
        .take(4)
        .enumerate()
        .filter(|(_, s)| s.state.is_playing())
        .map(|(i, _)| i)
        .collect();
    if playing.len() == 1 {
        Some(playing[0])
    } else {
        None
    }
}

/// Pick the "next" deck — i.e. a deck that is loaded with a fresh track and
/// likely about to be mixed in. We mark it `Next` only when there's a strong
/// signal: the channel fader has been brought up (non-zero `on_air` byte) on a
/// non-playing deck while another deck is currently on-air. Plain loaded-and-
/// idle decks fall through to `Idle` (38% opacity per spec).
///
/// This intentionally undershoots: we'd rather miss a `Next` than label every
/// idle loaded deck as Next and lose the visual contrast the spec wants.
pub fn pick_next(layers: &[LayerSnapshot], on_air: Option<usize>) -> Option<usize> {
    let on_air = on_air?;
    const NEXT_FADER_THRESHOLD: u8 = 16;
    for (i, snap) in layers.iter().take(4).enumerate() {
        if i == on_air {
            continue;
        }
        let loaded = !snap.title.is_empty() || snap.track_id != 0;
        let playing = snap.state.is_playing();
        let fader_up = snap.on_air >= NEXT_FADER_THRESHOLD;
        if loaded && !playing && fader_up {
            return Some(i);
        }
    }
    None
}

/// Compute role for every deck index. Empty decks (no track loaded) are flagged
/// `Empty` so the UI can choose to render nothing.
pub fn compute_roles(layers: &[LayerSnapshot]) -> [DeckRole; 4] {
    let on_air = pick_on_air(layers);
    let next = pick_next(layers, on_air);

    let mut roles = [DeckRole::Empty; 4];
    for (i, snap) in layers.iter().take(4).enumerate() {
        let loaded = !snap.title.is_empty() || snap.track_id != 0 || !snap.name.is_empty();
        roles[i] = if !loaded {
            DeckRole::Empty
        } else if Some(i) == on_air {
            DeckRole::OnAir
        } else if Some(i) == next {
            DeckRole::Next
        } else {
            DeckRole::Idle
        };
    }
    roles
}
