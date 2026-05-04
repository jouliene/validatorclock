const VISIBLE_ROUND_DEPTH: u32 = 5;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct RoundWindow {
    latest_round_id: u32,
}

impl RoundWindow {
    pub(super) fn ending_at(latest_round_id: u32) -> Self {
        Self { latest_round_id }
    }

    pub(super) fn rounds(self) -> impl Iterator<Item = u32> {
        (0..VISIBLE_ROUND_DEPTH)
            .rev()
            .filter_map(move |index| self.latest_round_id.checked_sub(index * 2))
    }
}
