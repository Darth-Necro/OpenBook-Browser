// SPDX-License-Identifier: MPL-2.0
//
// Attempt-counter policy + escalating-delay schedule (Build Plan §5.1, §5.3).
//
// The raw monotonic counter lives behind the `HardwareSecretProvider`
// (`increment_counter` / `read_counter` / `reset_counter`). This module layers
// the *policy* on top of it:
//
//   * Increment BEFORE each unlock attempt and persist immediately, so a
//     power-cycle between increment and the (failed) attempt cannot roll the
//     counter back. Reset to 0 ONLY on a successful unlock.
//   * After `max_attempts` failures the vault is erased. The Nth failed attempt
//     (the one that reaches the limit) triggers irreversible crypto-erasure.
//   * Escalating delays on consecutive failures reduce accidental erasure by a
//     legitimate user who mistypes. The returned `delayMs` is advisory guidance
//     to the UI (vault-ui sleeps/greys the field); in hardware mode the TPM /
//     Secure Enclave additionally enforces its own lockout independent of this.

/// Result of evaluating the counter for one unlock attempt, computed AFTER the
/// pre-attempt increment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptBudget {
    /// Counter value after the pre-attempt increment (1-based attempt number).
    pub attempt_number: u32,
    /// Configured maximum number of attempts before erasure.
    pub max_attempts: u32,
    /// Attempts remaining AFTER this one, if it fails. 0 means "this failure
    /// erases the vault".
    pub remaining_after_this: u32,
    /// True if reaching/using this attempt has hit the limit — i.e. a failure
    /// here triggers erasure.
    pub at_limit: bool,
}

impl AttemptBudget {
    /// Compute the budget for the just-incremented counter value.
    pub fn evaluate(attempt_number: u32, max_attempts: u32) -> Self {
        // `attempt_number` is 1-based (first attempt == 1). With max_attempts=6,
        // attempts 1..=5 leave room; attempt 6 is the last and a failure on it
        // erases.
        let at_limit = attempt_number >= max_attempts;
        let remaining_after_this = max_attempts.saturating_sub(attempt_number);
        AttemptBudget {
            attempt_number,
            max_attempts,
            remaining_after_this,
            at_limit,
        }
    }
}

/// Escalating-delay schedule, in milliseconds, indexed by the number of
/// CONSECUTIVE failures that have occurred (i.e. the post-increment counter
/// value). Mirrors the spirit of Secure Enclave's increasing timeouts.
///
/// Schedule (consecutive failures -> delay before the next attempt is allowed):
///   1 -> 0 ms       (first wrong try: no penalty)
///   2 -> 1 s
///   3 -> 5 s
///   4 -> 30 s
///   5 -> 60 s
///   >=6 -> 300 s    (also the point at/after which default max_attempts erases)
///
/// This is intentionally documented and unit-tested so the UX is predictable.
pub fn delay_ms_for_failures(consecutive_failures: u32) -> u64 {
    match consecutive_failures {
        0 | 1 => 0,
        2 => 1_000,
        3 => 5_000,
        4 => 30_000,
        5 => 60_000,
        _ => 300_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_basic_progression() {
        let max = 6;
        let b1 = AttemptBudget::evaluate(1, max);
        assert_eq!(b1.remaining_after_this, 5);
        assert!(!b1.at_limit);

        let b5 = AttemptBudget::evaluate(5, max);
        assert_eq!(b5.remaining_after_this, 1);
        assert!(!b5.at_limit);

        let b6 = AttemptBudget::evaluate(6, max);
        assert_eq!(b6.remaining_after_this, 0);
        assert!(b6.at_limit, "6th attempt at max=6 must be at the limit");
    }

    #[test]
    fn budget_handles_overshoot() {
        // Defensive: if somehow the counter exceeds max, still report at_limit.
        let b = AttemptBudget::evaluate(9, 6);
        assert!(b.at_limit);
        assert_eq!(b.remaining_after_this, 0);
    }

    #[test]
    fn delay_schedule_is_monotonic_nondecreasing() {
        let mut last = 0u64;
        for f in 1..=8u32 {
            let d = delay_ms_for_failures(f);
            assert!(d >= last, "delay must not decrease (f={f})");
            last = d;
        }
    }

    #[test]
    fn delay_schedule_known_values() {
        assert_eq!(delay_ms_for_failures(1), 0);
        assert_eq!(delay_ms_for_failures(2), 1_000);
        assert_eq!(delay_ms_for_failures(3), 5_000);
        assert_eq!(delay_ms_for_failures(4), 30_000);
        assert_eq!(delay_ms_for_failures(5), 60_000);
        assert_eq!(delay_ms_for_failures(6), 300_000);
        assert_eq!(delay_ms_for_failures(100), 300_000);
    }
}
