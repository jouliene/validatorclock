use super::*;

#[test]
fn round_window_lists_same_color_rounds_oldest_to_latest() {
    let rounds = RoundWindow::ending_at(10).rounds().collect::<Vec<_>>();
    assert_eq!(rounds, vec![2, 4, 6, 8, 10]);
}

#[test]
fn round_window_truncates_before_round_zero() {
    let rounds = RoundWindow::ending_at(3).rounds().collect::<Vec<_>>();
    assert_eq!(rounds, vec![1, 3]);

    let rounds = RoundWindow::ending_at(0).rounds().collect::<Vec<_>>();
    assert_eq!(rounds, vec![0]);
}
