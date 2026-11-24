use super::state::OperationOutcome;
use components::Tile;
use components::design::theme::Tone;
use components::icons::Icon;

/// The outcome of an operation as a solid status tile (`.tile.ok`,
/// `.tile.bad`) with its check or cross icon; size it, then show or paint it.
pub(super) fn status_tile(outcome: OperationOutcome) -> Tile {
    let (tone, icon) = match outcome {
        OperationOutcome::Success => (Tone::Success, Icon::CheckCircle),
        OperationOutcome::Warning => (Tone::Warning, Icon::ExclamationTriangle),
        OperationOutcome::Failure => (Tone::Danger, Icon::XCircle),
    };
    Tile::new(icon).tone(tone)
}
