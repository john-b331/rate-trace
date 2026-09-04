// Token-bucket simulation: given a parsed rule and a sequence of request
// timestamps, decide which requests are allowed and which get throttled.
//
// The bucket starts full (steady allowance plus burst available
// immediately), drains one token per admitted request, and refills
// continuously at the rule's configured rate. This matches how most
// gateway rate limiters behave in practice, rather than a fixed-window
// counter that resets in lockstep and lets requests through unevenly
// near window boundaries.

use crate::config::RateLimit;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RequestOutcome {
    pub allowed: bool,
    pub tokens_remaining: f64,
}

pub struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    tokens: f64,
    last: Duration,
}

impl TokenBucket {
    pub fn new(rule: &RateLimit) -> Self {
        let capacity = rule.count as f64 + rule.burst as f64;
        let refill_per_sec = rule.count as f64 / rule.period.as_secs_f64();
        TokenBucket {
            capacity,
            refill_per_sec,
            tokens: capacity,
            last: Duration::ZERO,
        }
    }

    /// Advances the bucket to `at` and attempts to admit one request there.
    /// `at` values are expected to be non-decreasing across calls; a value
    /// earlier than the bucket's current position is treated as arriving
    /// at that current position instead of moving time backwards.
    pub fn admit(&mut self, at: Duration) -> RequestOutcome {
        let elapsed = at.saturating_sub(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last = self.last.max(at);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            RequestOutcome {
                allowed: true,
                tokens_remaining: self.tokens,
            }
        } else {
            RequestOutcome {
                allowed: false,
                tokens_remaining: self.tokens,
            }
        }
    }
}

/// Runs every timestamp in `at_times` (assumed non-decreasing) through a
/// fresh bucket for `rule` and returns the outcome for each, in order.
pub fn simulate(rule: &RateLimit, at_times: &[Duration]) -> Vec<RequestOutcome> {
    let mut bucket = TokenBucket::new(rule);
    at_times.iter().map(|&t| bucket.admit(t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_rules;

    fn rule(src: &str, name: &str) -> RateLimit {
        parse_rules(src)
            .unwrap()
            .into_iter()
            .find(|r| r.name == name)
            .unwrap()
    }

    #[test]
    fn burst_allows_immediate_spike() {
        let r = rule("rule x { rate = 1/sec burst = 3 }", "x");
        let times = vec![Duration::ZERO; 4];
        let outcomes = simulate(&r, &times);
        // capacity = count + burst = 1 + 3 = 4, so all four at t=0 fit.
        assert!(outcomes.iter().all(|o| o.allowed));
    }

    #[test]
    fn exceeding_capacity_throttles() {
        let r = rule("rule x { rate = 1/sec burst = 0 }", "x");
        let times = vec![Duration::ZERO, Duration::ZERO];
        let outcomes = simulate(&r, &times);
        assert!(outcomes[0].allowed);
        assert!(!outcomes[1].allowed);
    }

    #[test]
    fn bucket_refills_over_time() {
        let r = rule("rule x { rate = 1/sec burst = 0 }", "x");
        let times = vec![Duration::ZERO, Duration::from_secs(1)];
        let outcomes = simulate(&r, &times);
        assert!(outcomes[0].allowed);
        assert!(outcomes[1].allowed);
    }

    #[test]
    fn partial_refill_is_not_enough_for_a_full_token() {
        let r = rule("rule x { rate = 1/sec burst = 0 }", "x");
        let times = vec![
            Duration::ZERO,
            Duration::from_millis(500),
            Duration::from_millis(600),
        ];
        let outcomes = simulate(&r, &times);
        assert!(outcomes[0].allowed);
        assert!(!outcomes[1].allowed);
        assert!(!outcomes[2].allowed);
    }

    #[test]
    fn refill_never_exceeds_capacity() {
        let r = rule("rule x { rate = 1/sec burst = 2 }", "x");
        let mut bucket = TokenBucket::new(&r);
        let idle = bucket.admit(Duration::from_secs(1000));
        assert!(idle.allowed);
        assert_eq!(idle.tokens_remaining, 2.0); // capacity 3, minus the one just consumed
    }
}
