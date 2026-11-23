//! Decoder backpressure policy for the experimental RustDesk core.
//!
//! Estimates *local decode queue work*, not end-to-end display latency. It never
//! discards compressed frames. Recovery uses RustDesk's existing video refresh request.
//! No dependencies: the same file is compiled into the core and its native tests.

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub requested_fps: usize,
    pub decode_capacity_fps: usize,
    pub queued_frames: usize,
    pub output_fps: usize,
    pub direct: bool,
    pub now_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub target_fps: usize,
    pub queue_work_ms: usize,
    pub request_refresh: bool,
}

#[derive(Debug, Default)]
pub struct Governor {
    target: Option<usize>,
    healthy_samples: u8,
    capacity_shortfalls: u8,
    last_brake_ms: Option<u64>,
    last_refresh_ms: Option<u64>,
}

impl Governor {
    pub fn update(&mut self, sample: Sample) -> Option<Decision> {
        if sample.decode_capacity_fps == 0 {
            return None;
        }
        let cap = sample.requested_fps.clamp(5, 120);
        let capacity = sample.decode_capacity_fps.min(100_000);
        let sustainable = ((capacity * if sample.direct { 9 } else { 8 }) / 10).clamp(5, cap);
        let queue_ms = sample.queued_frames.saturating_mul(1000) / capacity;
        let mut target = self.target.unwrap_or(sustainable).min(cap);
        let idle = sample.queued_frames == 0 && sample.output_fps == 0;
        let braking = sample.queued_frames > 1 && queue_ms >= 80;

        if braking {
            // Drain work before probing a higher rate. A long queue brakes harder.
            target = if queue_ms >= 250 {
                (target / 2).min(sustainable * 3 / 4)
            } else {
                (target * 4 / 5).min(sustainable)
            }
            .clamp(5, cap);
            self.healthy_samples = 0;
            self.capacity_shortfalls = 0;
            self.last_brake_ms = Some(sample.now_ms);
        } else if !idle {
            if sustainable < target && sample.queued_frames > 1 {
                self.capacity_shortfalls = self.capacity_shortfalls.saturating_add(1);
                self.healthy_samples = 0;
                if self.capacity_shortfalls >= 2 {
                    target = sustainable;
                    self.last_brake_ms = Some(sample.now_ms);
                    self.capacity_shortfalls = 0;
                }
            } else if sample.queued_frames <= 1 {
                self.capacity_shortfalls = 0;
                self.healthy_samples = self.healthy_samples.saturating_add(1);
                let settled = self
                    .last_brake_ms
                    .map_or(true, |last| sample.now_ms.saturating_sub(last) >= 2000);
                if self.healthy_samples >= 3 && settled && sustainable > target {
                    // Probe gradually; the old controller could jump to full capacity.
                    target = (target + 4).min(sustainable);
                    self.healthy_samples = 0;
                }
            } else {
                self.healthy_samples = 0;
                self.capacity_shortfalls = 0;
            }
        } else {
            self.healthy_samples = 0;
            self.capacity_shortfalls = 0;
        }

        let refresh_due = self
            .last_refresh_ms
            .map_or(true, |last| sample.now_ms.saturating_sub(last) >= 5000);
        let refresh = sample.queued_frames > 1 && queue_ms >= 150 && refresh_due;
        if refresh {
            self.last_refresh_ms = Some(sample.now_ms);
        }
        self.target = Some(target);
        Some(Decision {
            target_fps: target,
            queue_work_ms: queue_ms,
            request_refresh: refresh,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(now_ms: u64, capacity: usize, queue: usize) -> Sample {
        Sample {
            requested_fps: 60,
            decode_capacity_fps: capacity,
            queued_frames: queue,
            output_fps: 60,
            direct: true,
            now_ms,
        }
    }

    #[test]
    fn fast_decoder_reaches_requested_sixty_without_default_thirty_cap() {
        let mut governor = Governor::default();
        assert_eq!(governor.update(sample(0, 100, 0)).unwrap().target_fps, 60);
    }

    #[test]
    fn sixty_request_does_not_overrun_a_forty_fps_decoder() {
        assert_eq!(
            Governor::default()
                .update(sample(0, 40, 0))
                .unwrap()
                .target_fps,
            36
        );
    }

    #[test]
    fn two_hundred_ms_backlog_requests_existing_refresh_recovery() {
        let mut governor = Governor::default();
        governor.update(sample(0, 100, 0));
        let decision = governor.update(sample(1000, 40, 8)).unwrap();
        assert_eq!(decision.queue_work_ms, 200);
        assert_eq!(decision.target_fps, 36);
        assert!(decision.request_refresh);
    }

    #[test]
    fn refresh_recovery_is_rate_limited_but_can_run_again() {
        let mut governor = Governor::default();
        assert!(
            governor
                .update(sample(1000, 40, 8))
                .unwrap()
                .request_refresh
        );
        assert!(
            !governor
                .update(sample(2000, 40, 8))
                .unwrap()
                .request_refresh
        );
        assert!(
            governor
                .update(sample(6000, 40, 8))
                .unwrap()
                .request_refresh
        );
    }

    #[test]
    fn recovery_does_not_jump_back_to_sixty_after_one_empty_queue() {
        let mut governor = Governor::default();
        governor.update(sample(0, 100, 0));
        let cut = governor.update(sample(1000, 30, 12)).unwrap().target_fps;
        assert_eq!(
            governor.update(sample(2000, 100, 0)).unwrap().target_fps,
            cut
        );
        assert_eq!(
            governor.update(sample(3000, 100, 0)).unwrap().target_fps,
            cut
        );
        assert_eq!(
            governor.update(sample(4000, 100, 0)).unwrap().target_fps,
            cut + 4
        );
    }

    #[test]
    fn supported_lower_user_cap_applies_immediately() {
        let mut governor = Governor::default();
        governor.update(sample(0, 100, 0));
        let mut low = sample(1000, 100, 0);
        low.requested_fps = 24;
        assert_eq!(governor.update(low).unwrap().target_fps, 24);
    }

    #[test]
    fn idle_desktop_does_not_probe_upward_or_request_refreshs() {
        let mut governor = Governor::default();
        let before = governor.update(sample(0, 30, 0)).unwrap().target_fps;
        for n in 1..10 {
            let mut idle = sample(n * 1000, 100, 0);
            idle.output_fps = 0;
            let decision = governor.update(idle).unwrap();
            assert_eq!(decision.target_fps, before);
            assert!(!decision.request_refresh);
        }
    }

    #[test]
    fn zero_capacity_waits_for_a_valid_measurement() {
        assert!(Governor::default().update(sample(0, 0, 10)).is_none());
    }

    #[test]
    fn relay_preserves_more_decode_headroom_than_direct() {
        let mut relay = sample(0, 60, 0);
        relay.direct = false;
        assert_eq!(Governor::default().update(relay).unwrap().target_fps, 48);
        assert_eq!(
            Governor::default()
                .update(sample(0, 60, 0))
                .unwrap()
                .target_fps,
            54
        );
    }

    #[test]
    fn sustained_recovery_eventually_returns_to_sixty_in_small_steps() {
        let mut governor = Governor::default();
        governor.update(sample(0, 100, 0));
        governor.update(sample(1000, 40, 8));
        for second in 2..10 {
            assert_eq!(
                governor
                    .update(sample(second * 1000, 40, 0))
                    .unwrap()
                    .target_fps,
                36
            );
        }
        let mut previous = 36;
        for second in 10..35 {
            let target = governor
                .update(sample(second * 1000, 100, 0))
                .unwrap()
                .target_fps;
            assert!(target >= previous && target <= previous + 4);
            previous = target;
        }
        assert_eq!(previous, 60);
    }

    #[test]
    fn adversarial_samples_always_obey_user_and_native_limits() {
        let mut governor = Governor::default();
        for requested in [0, 5, 24, 60, 120, usize::MAX] {
            for capacity in [1, 40, 100, usize::MAX] {
                for queue in [0, 1, 4, 120, usize::MAX] {
                    let mut s = sample(1000, capacity, queue);
                    s.requested_fps = requested;
                    let d = governor.update(s).unwrap();
                    assert!((5..=requested.clamp(5, 120)).contains(&d.target_fps));
                }
            }
        }
    }
}
