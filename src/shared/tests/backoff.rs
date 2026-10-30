use super::super::graphql::retry_backoff_ms;
use super::super::INITIAL_RETRY_BACKOFF_MS;

#[test]
fn backoff_stays_within_equal_jitter_bounds() {
    for attempt in 1..=4_u32 {
        let cap = INITIAL_RETRY_BACKOFF_MS * (1 << (attempt - 1));
        for _ in 0..64 {
            let backoff = retry_backoff_ms(attempt);
            assert!(
                backoff >= cap / 2,
                "attempt {attempt}: {backoff} < {}",
                cap / 2
            );
            assert!(backoff <= cap, "attempt {attempt}: {backoff} > {cap}");
        }
    }
}

#[test]
fn backoff_is_not_deterministic() {
    let samples: Vec<u64> = (0..32).map(|_| retry_backoff_ms(4)).collect();
    assert!(
        samples.iter().any(|s| *s != samples[0]),
        "32 samples identical: jitter is not being applied"
    );
}
