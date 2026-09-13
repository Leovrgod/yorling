//! Keyboard-driven cursor motion. Input updates replace one shared intent;
//! they never enqueue frames for the worker to replay later.

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, ns_string};

use crate::display_link::{DisplayFrame, DisplayLink};
use crate::interceptor::{
    CGPoint, emit_mouse_move_to, get_active_display_bounds, get_cursor_position,
};

const MOUSE_MOVE_SPEED_FAST: f64 = 1400.0;
const MOUSE_MOVE_SPEED_SLOW: f64 = 360.0;
const MOUSE_MOVE_TICK: Duration = Duration::from_micros(8_333);
const MOUSE_MOVE_RESPONSE: f64 = 18.0;
// A late frame must not turn scheduling delay into a large cursor jump.
const MAX_FRAME_TIME: Duration = Duration::from_micros(16_666);
const RESET_AFTER_STALL: Duration = Duration::from_millis(100);
const DISPLAY_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) fn set_input_thread_priority() {
    // SAFETY: changes only the calling thread, with a valid QoS and priority.
    let result = unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0)
    };
    if result != 0 {
        log::warn!("Could not set input thread QoS: {result}");
    }
}

/// Held only while direction keys are moving the cursor. Drop pairs every
/// begin/end, including worker shutdown and unwinding; idle system sleep stays allowed.
struct InputActivity(Retained<ProtocolObject<dyn NSObjectProtocol>>);

impl InputActivity {
    fn new() -> Self {
        autoreleasepool(|_| {
            Self(
                NSProcessInfo::processInfo().beginActivityWithOptions_reason(
                    NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
                    ns_string!("Yorling keyboard mouse movement"),
                ),
            )
        })
    }
}

impl Drop for InputActivity {
    fn drop(&mut self) {
        autoreleasepool(|_| {
            // SAFETY: this is the token returned by beginActivity above.
            unsafe { NSProcessInfo::processInfo().endActivity(&self.0) };
        });
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MouseMoveIntent {
    pub x: i8,
    pub y: i8,
    pub fast: bool,
}

impl MouseMoveIntent {
    fn is_idle(self) -> bool {
        self.x == 0 && self.y == 0
    }
}

#[derive(Debug, Default)]
struct SmoothMouseMotion {
    velocity_x: f64,
    velocity_y: f64,
}

impl SmoothMouseMotion {
    #[cfg(test)]
    fn step(&mut self, intent: MouseMoveIntent, dt: Duration) -> Option<(f64, f64)> {
        self.step_at_refresh(intent, dt, MOUSE_MOVE_TICK)
    }

    fn step_at_refresh(
        &mut self,
        intent: MouseMoveIntent,
        dt: Duration,
        period: Duration,
    ) -> Option<(f64, f64)> {
        if intent.is_idle() {
            *self = Self::default();
            return None;
        }
        let dt_secs = dt.min(MAX_FRAME_TIME.max(period)).as_secs_f64();
        if dt_secs <= f64::EPSILON {
            return None;
        }
        let (target_vx, target_vy) = mouse_move_target_velocity(intent);
        let blend = 1.0 - (-MOUSE_MOVE_RESPONSE * dt_secs).exp();
        self.velocity_x += (target_vx - self.velocity_x) * blend;
        self.velocity_y += (target_vy - self.velocity_y) * blend;
        Some((self.velocity_x * dt_secs, self.velocity_y * dt_secs))
    }
}

fn mouse_move_speed_per_second(fast: bool) -> f64 {
    if fast {
        MOUSE_MOVE_SPEED_FAST
    } else {
        MOUSE_MOVE_SPEED_SLOW
    }
}

fn mouse_move_target_velocity(intent: MouseMoveIntent) -> (f64, f64) {
    let x = f64::from(intent.x);
    let y = f64::from(intent.y);
    let magnitude = x.hypot(y);
    if magnitude <= f64::EPSILON {
        return (0.0, 0.0);
    }
    let speed = mouse_move_speed_per_second(intent.fast);
    (x / magnitude * speed, y / magnitude * speed)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DisplayBounds {
    pub id: u32,
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

fn constrain_to_displays(position: CGPoint, displays: &[DisplayBounds]) -> CGPoint {
    // Project into the nearest actual display, not the bounding rectangle of
    // all displays (which includes gaps in staggered/multi-monitor layouts).
    displays
        .iter()
        .map(|bounds| CGPoint {
            x: position.x.clamp(bounds.min_x, bounds.max_x),
            y: position.y.clamp(bounds.min_y, bounds.max_y),
        })
        .min_by(|a, b| {
            let distance = |p: &CGPoint| (p.x - position.x).hypot(p.y - position.y);
            distance(a).total_cmp(&distance(b))
        })
        .unwrap_or(position)
}

#[derive(Debug, Default)]
struct CursorMotionState {
    position: Option<CGPoint>,
}

impl CursorMotionState {
    fn next_position(
        &mut self,
        dx: f64,
        dy: f64,
        displays: &[DisplayBounds],
        current_position: impl FnOnce() -> CGPoint,
    ) -> CGPoint {
        let position = self.position.unwrap_or_else(current_position);
        let next = constrain_to_displays(
            CGPoint {
                x: position.x + dx,
                y: position.y + dy,
            },
            displays,
        );
        // Store the constrained position, so time spent pushing against an edge
        // never builds up invisible distance that must be undone on reversal.
        self.position = Some(next);
        next
    }
}

#[derive(Default)]
struct MotionRequest {
    intent: MouseMoveIntent,
    running: bool,
    frame: Option<DisplayFrame>,
    // Preserve a stop/restart even if both happen before the next worker tick.
    generation: u64,
}

impl MotionRequest {
    fn update(&mut self, intent: MouseMoveIntent) {
        if self.intent != intent && (self.intent.is_idle() || intent.is_idle()) {
            self.generation = self.generation.wrapping_add(1);
        }
        self.intent = intent;
    }
}

#[derive(Default)]
pub(super) struct MotionShared {
    request: Mutex<MotionRequest>,
    wake: Condvar,
}

impl MotionShared {
    pub(super) fn publish_frame(&self, frame: DisplayFrame) {
        // One replaceable slot, never a queue. No event posting or allocation
        // here; input releases use the same mutex/condition variable.
        if let Ok(mut request) = self.request.lock() {
            if request.running && !request.intent.is_idle() {
                request.frame = Some(frame);
                self.wake.notify_one();
            }
        }
    }
}

pub(crate) struct MouseMotionController {
    shared: Arc<MotionShared>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl MouseMotionController {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(MotionShared::default()),
            worker: Mutex::new(None),
        }
    }

    pub fn start(&self) {
        self.start_with_output(NativeMouseOutput::default());
    }

    fn start_with_output(&self, output: impl MouseOutput + Send + 'static) {
        // Serialize start/stop so a quick restart cannot leave two workers alive.
        let mut worker = self.worker.lock().unwrap();
        if worker.is_some() {
            return;
        }
        self.shared.request.lock().unwrap().running = true;
        let shared = Arc::clone(&self.shared);
        *worker = Some(thread::spawn(move || run_mouse_worker(shared, output)));
    }

    pub fn stop(&self) {
        let mut worker = self.worker.lock().unwrap();
        {
            let mut request = self.shared.request.lock().unwrap();
            request.running = false;
            request.update(MouseMoveIntent::default());
        }
        self.shared.wake.notify_one();
        if let Some(handle) = worker.take() {
            let _ = handle.join();
        }
    }

    pub fn update_intent(&self, intent: MouseMoveIntent) {
        self.shared.request.lock().unwrap().update(intent);
        self.shared.wake.notify_one();
    }

    pub fn stop_motion(&self) {
        self.update_intent(MouseMoveIntent::default());
    }
}

impl Drop for MouseMotionController {
    fn drop(&mut self) {
        self.stop();
    }
}

trait MouseOutput {
    fn reset(&mut self);
    fn move_by(&mut self, dx: f64, dy: f64);
    fn display_id(&self) -> Option<u32> {
        None
    }
}

#[derive(Default)]
struct NativeMouseOutput {
    cursor: CursorMotionState,
    displays: Vec<DisplayBounds>,
    displays_updated: Option<Instant>,
}

impl MouseOutput for NativeMouseOutput {
    fn reset(&mut self) {
        self.cursor = CursorMotionState {
            position: Some(get_cursor_position()),
        };
        self.displays = get_active_display_bounds();
        self.displays_updated = Some(Instant::now());
    }

    fn display_id(&self) -> Option<u32> {
        let position = self.cursor.position?;
        self.displays
            .iter()
            .find(|d| {
                position.x >= d.min_x
                    && position.x <= d.max_x
                    && position.y >= d.min_y
                    && position.y <= d.max_y
            })
            .map(|d| d.id)
    }

    fn move_by(&mut self, dx: f64, dy: f64) {
        // Rust-created threads have no AppKit-managed autorelease pool.
        // Drain transient framework objects every frame, not at thread exit.
        autoreleasepool(|_| {
            if self
                .displays_updated
                .is_none_or(|last| last.elapsed() >= DISPLAY_REFRESH_INTERVAL)
            {
                self.displays = get_active_display_bounds();
                self.displays_updated = Some(Instant::now());
            }
            if self.displays.is_empty() {
                // If the screen list is temporarily unavailable, reconcile with
                // the real cursor each frame instead of integrating off-screen.
                self.cursor = CursorMotionState::default();
            }
            let position = self
                .cursor
                .next_position(dx, dy, &self.displays, get_cursor_position);
            emit_mouse_move_to(position);
        });
    }
}

fn next_frame_deadline(previous: Instant, after_work: Instant) -> Instant {
    let next = previous + MOUSE_MOVE_TICK;
    if next > after_work {
        next
    } else {
        after_work + MOUSE_MOVE_TICK
    }
}

fn display_frame_elapsed(previous: Option<DisplayFrame>, frame: DisplayFrame) -> Duration {
    previous
        .map(|last| {
            frame
                .video_time
                .zip(last.video_time)
                .and_then(|(now, then)| now.checked_sub(then))
                .filter(|dt| !dt.is_zero())
                .unwrap_or_else(|| frame.at.saturating_duration_since(last.at))
        })
        .unwrap_or(frame.period)
}

fn run_mouse_worker(shared: Arc<MotionShared>, mut output: impl MouseOutput) {
    set_input_thread_priority();
    let mut activity = None;
    let mut clock: Option<DisplayLink> = None;
    let mut motion = SmoothMouseMotion::default();
    let mut generation = None;
    let mut last_tick = Instant::now();
    let mut next_tick = last_tick;
    let mut clock_progress = last_tick;
    let mut clock_retry = last_tick;
    let mut previous_frame = None;
    let mut last_stall_warning = None::<Instant>;

    loop {
        let mut request = shared.request.lock().unwrap();
        if !request.running {
            break;
        }
        if request.intent.is_idle() {
            // Native clock/activity teardown can wait for other threads. Never
            // hold the input mutex while invoking Foundation or CoreVideo.
            drop(request);
            clock.take();
            activity.take();
            request = shared.request.lock().unwrap();
            request.frame = None;
            request = shared
                .wake
                .wait_while(request, |r| r.running && r.intent.is_idle())
                .unwrap();
            if !request.running {
                break;
            }
        }

        let now = Instant::now();
        let restarted = generation != Some(request.generation);
        if restarted {
            generation = Some(request.generation);
            drop(request);
            clock.take();
            output.reset();
            motion = SmoothMouseMotion::default();
            previous_frame = None;
            last_tick = now - MOUSE_MOVE_TICK;
            next_tick = now;
            clock_retry = now;
            activity.get_or_insert_with(InputActivity::new);
            continue;
        }

        let display = output.display_id();
        let changed_display = clock
            .as_ref()
            .is_some_and(|link| Some(link.display()) != display);
        if changed_display || (clock.is_none() && display.is_some() && now >= clock_retry) {
            drop(request);
            clock.take();
            shared.request.lock().unwrap().frame = None;
            previous_frame = None;
            clock = display.and_then(|id| match DisplayLink::start(id, Arc::clone(&shared)) {
                Ok(link) => Some(link),
                Err(status) => {
                    log::debug!("Mouse display clock unavailable ({status}); using timer fallback");
                    None
                }
            });
            clock_progress = Instant::now();
            clock_retry = clock_progress + DISPLAY_REFRESH_INTERVAL;
            continue;
        }

        let (elapsed, period) = if clock.is_some() {
            if let Some(frame) = request.frame.take() {
                clock_progress = now;
                let elapsed = display_frame_elapsed(previous_frame, frame);
                previous_frame = Some(frame);
                (elapsed, frame.period)
            } else if now.saturating_duration_since(clock_progress) >= RESET_AFTER_STALL {
                drop(request);
                clock.take();
                previous_frame = None;
                clock_retry = now + DISPLAY_REFRESH_INTERVAL;
                next_tick = now;
                log::debug!("Mouse display clock paused; using timer fallback until it recovers");
                continue;
            } else {
                let timeout = RESET_AFTER_STALL - now.saturating_duration_since(clock_progress);
                let _ = shared.wake.wait_timeout(request, timeout).unwrap();
                continue;
            }
        } else {
            if now < next_tick {
                let _ = shared.wake.wait_timeout(request, next_tick - now).unwrap();
                continue;
            }
            (now.saturating_duration_since(last_tick), MOUSE_MOVE_TICK)
        };
        let intent = request.intent;
        drop(request);

        let mut elapsed = elapsed;
        if now.saturating_duration_since(last_tick) > RESET_AFTER_STALL {
            motion = SmoothMouseMotion::default();
            output.reset();
            elapsed = period;
            if last_stall_warning
                .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(10))
            {
                log::warn!(
                    "Mouse motion scheduling gap: {} ms; discarded stale movement",
                    now.duration_since(last_tick).as_millis()
                );
                last_stall_warning = Some(now);
            }
        }
        // Resetting screen state and starting native activities may take time.
        // Recheck after that work, so releases/restarts never replay old intent.
        {
            let latest = shared.request.lock().unwrap();
            if !latest.running || latest.intent != intent || Some(latest.generation) != generation {
                continue;
            }
        }
        if let Some((dx, dy)) = motion.step_at_refresh(intent, elapsed, period) {
            output.move_by(dx, dy);
        }
        last_tick = now;
        next_tick = next_frame_deadline(next_tick, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> DisplayBounds {
        DisplayBounds {
            id: 1,
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    #[test]
    fn pushing_against_an_edge_does_not_accumulate_hidden_distance() {
        let displays = [screen(0.0, 0.0, 1919.0, 1079.0)];
        let mut cursor = CursorMotionState {
            position: Some(CGPoint {
                x: 1900.0,
                y: 500.0,
            }),
        };
        for _ in 0..100_000 {
            let position = cursor.next_position(12.0, 0.0, &displays, || panic!("cached position"));
            assert!(position.x <= 1919.0);
        }
        let reversed = cursor.next_position(-3.0, 0.0, &displays, || panic!("cached position"));
        assert_eq!(reversed.x, 1916.0);
    }

    #[test]
    fn crossing_displays_supports_negative_coordinates_and_excludes_gaps() {
        let displays = [
            screen(-1920.0, 0.0, -1.0, 1079.0),
            screen(0.0, -300.0, 1511.0, 681.0),
        ];
        let mut cursor = CursorMotionState {
            position: Some(CGPoint { x: -3.0, y: 500.0 }),
        };
        let next = cursor.next_position(6.0, 0.0, &displays, || panic!("cached position"));
        assert_eq!((next.x, next.y), (3.0, 500.0));
        let gap = constrain_to_displays(CGPoint { x: 10.0, y: 900.0 }, &displays);
        assert_eq!((gap.x, gap.y), (-1.0, 900.0));
        let left_edge = constrain_to_displays(
            CGPoint {
                x: -9000.0,
                y: 500.0,
            },
            &displays,
        );
        assert_eq!(left_edge.x, -1920.0);
    }

    #[test]
    fn cursor_keeps_subpixel_motion_without_reading_stale_system_position() {
        let displays = [screen(0.0, 0.0, 1919.0, 1079.0)];
        let mut cursor = CursorMotionState::default();
        cursor.next_position(0.25, 0.0, &displays, || CGPoint { x: 100.0, y: 100.0 });
        for _ in 0..3 {
            cursor.next_position(0.25, 0.0, &displays, || {
                panic!("must not feed back a stale system position")
            });
        }
        assert_eq!(cursor.position.unwrap().x, 101.0);
    }

    #[test]
    fn display_removal_constrains_the_cached_position_to_the_remaining_screen() {
        let mut cursor = CursorMotionState {
            position: Some(CGPoint {
                x: -1500.0,
                y: 500.0,
            }),
        };
        let next = cursor.next_position(3.0, 0.0, &[screen(0.0, 0.0, 1511.0, 981.0)], || {
            panic!("cached position")
        });
        assert_eq!(next.x, 0.0);
    }

    #[test]
    fn stop_and_resume_between_ticks_still_resets_the_motion_session() {
        let right = MouseMoveIntent {
            x: 1,
            y: 0,
            fast: true,
        };
        let mut request = MotionRequest::default();
        request.update(right);
        let before = request.generation;
        request.update(MouseMoveIntent::default());
        request.update(right);
        assert_ne!(request.generation, before);
        assert_eq!(request.intent, right);
        let resumed = request.generation;
        request.update(MouseMoveIntent { x: -1, ..right });
        assert_eq!(
            request.generation, resumed,
            "direction changes preserve smooth acceleration"
        );
    }

    #[test]
    fn missed_deadlines_skip_frames_instead_of_creating_a_catchup_burst() {
        let now = Instant::now();
        assert_eq!(
            next_frame_deadline(now, now + Duration::from_millis(1)),
            now + MOUSE_MOVE_TICK
        );
        let after_stall = now + Duration::from_secs(2);
        assert_eq!(
            next_frame_deadline(now, after_stall),
            after_stall + MOUSE_MOVE_TICK
        );
    }

    #[test]
    fn continuous_motion_has_stable_speed_over_simulated_hours() {
        let intent = MouseMoveIntent {
            x: 1,
            y: 1,
            fast: true,
        };
        let mut motion = SmoothMouseMotion::default();
        for _ in 0..120 {
            motion.step(intent, MOUSE_MOVE_TICK);
        }
        let initial = motion.step(intent, MOUSE_MOVE_TICK).unwrap();
        for _ in 0..3_000_000 {
            motion.step(intent, MOUSE_MOVE_TICK);
        }
        let final_step = motion.step(intent, MOUSE_MOVE_TICK).unwrap();
        assert!((initial.0 - final_step.0).abs() < 1e-6);
        assert!((initial.1 - final_step.1).abs() < 1e-6);
    }

    #[test]
    fn display_timestamps_remove_worker_jitter_from_movement() {
        let period = Duration::from_secs_f64(1.0 / 75.0);
        let now = Instant::now();
        let previous = DisplayFrame {
            at: now,
            period,
            video_time: Some(Duration::from_secs(1)),
        };
        let late = DisplayFrame {
            at: now + period + Duration::from_millis(4),
            period,
            video_time: Some(Duration::from_secs(1) + period),
        };
        assert_eq!(display_frame_elapsed(Some(previous), late), period);
        let discontinuity = DisplayFrame {
            video_time: Some(Duration::ZERO),
            ..late
        };
        assert_eq!(
            display_frame_elapsed(Some(previous), discontinuity),
            period + Duration::from_millis(4)
        );
    }

    #[test]
    fn low_refresh_displays_keep_configured_speed_and_bound_missed_frames() {
        let intent = MouseMoveIntent {
            x: 1,
            y: 0,
            fast: true,
        };
        for rate in [30.0, 59.94, 60.0, 75.0, 120.0, 144.0, 240.0] {
            let period = Duration::from_secs_f64(1.0 / rate);
            let mut motion = SmoothMouseMotion::default();
            for _ in 0..1000 {
                motion.step_at_refresh(intent, period, period);
            }
            let (dx, _) = motion.step_at_refresh(intent, period, period).unwrap();
            assert!(
                (dx / period.as_secs_f64() - MOUSE_MOVE_SPEED_FAST).abs() < 0.001,
                "{rate} Hz changed speed"
            );
            let (late, _) = motion
                .step_at_refresh(intent, Duration::from_secs(2), period)
                .unwrap();
            assert!(late <= MOUSE_MOVE_SPEED_FAST * MAX_FRAME_TIME.max(period).as_secs_f64());
        }
    }

    #[test]
    fn display_frames_replace_pending_work_and_stop_publishing_when_idle() {
        let shared = MotionShared::default();
        {
            let mut r = shared.request.lock().unwrap();
            r.running = true;
            r.update(MouseMoveIntent {
                x: 1,
                y: 0,
                fast: false,
            });
        }
        let now = Instant::now();
        for i in 1..1000 {
            shared.publish_frame(DisplayFrame {
                at: now + MOUSE_MOVE_TICK * i,
                period: MOUSE_MOVE_TICK,
                video_time: Some(MOUSE_MOVE_TICK * i),
            });
        }
        let mut r = shared.request.lock().unwrap();
        assert_eq!(r.frame.take().unwrap().at, now + MOUSE_MOVE_TICK * 999);
        assert!(r.frame.is_none(), "old frames must never form a backlog");
        r.update(MouseMoveIntent::default());
        drop(r);
        shared.publish_frame(DisplayFrame {
            at: now,
            period: MOUSE_MOVE_TICK,
            video_time: None,
        });
        assert!(shared.request.lock().unwrap().frame.is_none());
    }

    #[test]
    fn matching_display_frames_avoids_uneven_visible_steps_at_75_hz() {
        // Sample steady cursor positions at each display refresh. This models
        // cadence only, not real WindowServer latency or dropped display frames.
        let visible_steps = |event_hz: f64| {
            (1..=75)
                .map(|frame| {
                    let sample = |n: i32| {
                        ((f64::from(n) / 75.0 * event_hz) + 1e-6).floor() / event_hz * 1400.0
                    };
                    sample(frame) - sample(frame - 1)
                })
                .collect::<Vec<_>>()
        };
        let spread = |steps: Vec<f64>| {
            steps.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                - steps.iter().cloned().fold(f64::INFINITY, f64::min)
        };
        assert!(spread(visible_steps(120.0)) > 11.0);
        assert!(spread(visible_steps(75.0)) < 1e-8);
    }

    #[test]
    #[ignore = "requires a logged-in macOS display; emits no desktop input"]
    fn native_display_clock_paces_output_and_releases_across_restarts() {
        struct Capture {
            tx: std::sync::mpsc::Sender<Instant>,
            display: Option<u32>,
        }
        impl MouseOutput for Capture {
            fn reset(&mut self) {}
            fn move_by(&mut self, _: f64, _: f64) {
                let _ = self.tx.send(Instant::now());
            }
            fn display_id(&self) -> Option<u32> {
                self.display
            }
        }
        let display = get_active_display_bounds()
            .first()
            .expect("active display")
            .id;
        let controller = MouseMotionController::new();
        let expected_period = DisplayLink::start(display, Arc::clone(&controller.shared))
            .expect("native display link")
            .nominal_period()
            .expect("known refresh rate");
        for (label, display) in [
            ("timer baseline", None),
            ("display clock", Some(display)),
            ("display clock restarted", Some(display)),
        ] {
            let (tx, rx) = std::sync::mpsc::channel();
            controller.start_with_output(Capture { tx, display });
            controller.update_intent(MouseMoveIntent {
                x: 1,
                y: 0,
                fast: true,
            });
            let mut instants = Vec::new();
            for _ in 0..150 {
                instants.push(
                    rx.recv_timeout(Duration::from_secs(2))
                        .expect("clock must produce output"),
                );
            }
            controller.stop_motion();
            // Wait until the worker acknowledges idle (including native teardown).
            thread::sleep(Duration::from_millis(100));
            while rx.try_recv().is_ok() {}
            assert!(
                rx.recv_timeout(Duration::from_millis(100)).is_err(),
                "idle output"
            );
            controller.stop();
            assert_eq!(
                Arc::strong_count(&controller.shared),
                1,
                "callback must release its shared state"
            );
            let mut intervals: Vec<_> = instants[20..]
                .windows(2)
                .map(|pair| pair[1].duration_since(pair[0]).as_secs_f64() * 1000.0)
                .collect();
            let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
            if display.is_some() {
                let expected = expected_period.as_secs_f64() * 1000.0;
                assert!(
                    (mean - expected).abs() < expected * 0.25,
                    "native output {mean:.3} ms does not track display {expected:.3} ms"
                );
            }
            intervals.sort_by(f64::total_cmp);
            eprintln!(
                "{label}: intervals={} median={:.3} ms p95={:.3} ms max={:.3} ms",
                intervals.len(),
                intervals[intervals.len() / 2],
                intervals[intervals.len() * 95 / 100],
                intervals.last().unwrap()
            );
        }
    }

    struct TestMouseOutput(std::sync::mpsc::Sender<Option<(f64, f64)>>);

    impl MouseOutput for TestMouseOutput {
        fn reset(&mut self) {
            let _ = self.0.send(None);
        }
        fn move_by(&mut self, dx: f64, dy: f64) {
            let _ = self.0.send(Some((dx, dy)));
        }
    }

    #[test]
    fn worker_sleeps_when_idle_and_stops_cleanly_across_restarts() {
        use std::sync::mpsc::{RecvTimeoutError, channel};
        let controller = MouseMotionController::new();
        for _ in 0..3 {
            let (tx, rx) = channel();
            controller.start_with_output(TestMouseOutput(tx));
            assert_eq!(
                rx.recv_timeout(Duration::from_millis(30)),
                Err(RecvTimeoutError::Timeout)
            );
            controller.update_intent(MouseMoveIntent {
                x: 1,
                y: 0,
                fast: true,
            });
            assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), None);
            assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap().unwrap().0 > 0.0);
            controller.stop();
            // join() guarantees no output can arrive from this worker afterwards.
            while rx.try_recv().is_ok() {}
            assert_eq!(
                rx.recv_timeout(Duration::from_millis(30)),
                Err(RecvTimeoutError::Disconnected)
            );
        }
    }

    #[test]
    fn only_the_latest_pending_intent_is_executed() {
        let controller = MouseMotionController::new();
        for _ in 0..1000 {
            controller.update_intent(MouseMoveIntent {
                x: 1,
                y: 0,
                fast: true,
            });
            controller.stop_motion();
        }
        controller.update_intent(MouseMoveIntent {
            x: -1,
            y: 0,
            fast: false,
        });
        let (tx, rx) = std::sync::mpsc::channel();
        controller.start_with_output(TestMouseOutput(tx));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), None);
        assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap().unwrap().0 < 0.0);
        controller.stop();
    }
    #[test]
    fn mouse_move_speed_profiles_are_faster_than_legacy_steps() {
        assert!(mouse_move_speed_per_second(false) > 300.0);
        assert!(mouse_move_speed_per_second(true) > 1250.0);
    }

    #[test]
    fn mouse_move_target_velocity_normalizes_diagonal_motion() {
        let (vx, vy) = mouse_move_target_velocity(MouseMoveIntent {
            x: 1,
            y: 1,
            fast: false,
        });

        assert!(vx > 0.0);
        assert!(vy > 0.0);
        assert!((vx - vy).abs() < 1e-6);

        let speed = (vx * vx + vy * vy).sqrt();
        assert!((speed - mouse_move_speed_per_second(false)).abs() < 1e-6);
    }

    #[test]
    fn smooth_mouse_motion_accelerates_and_stops_cleanly() {
        let mut motion = SmoothMouseMotion::default();
        let intent = MouseMoveIntent {
            x: 1,
            y: 0,
            fast: false,
        };

        let first = motion
            .step(intent, Duration::from_millis(8))
            .expect("initial movement");
        let second = motion
            .step(intent, Duration::from_millis(8))
            .expect("continued movement");

        assert!(second.0.abs() > first.0.abs());
        assert!(second.1.abs() <= first.1.abs());
        assert!(
            motion
                .step(MouseMoveIntent::default(), Duration::from_millis(8))
                .is_none()
        );
    }

    #[test]
    fn delayed_mouse_tick_does_not_replay_a_long_pause() {
        let mut motion = SmoothMouseMotion::default();
        let intent = MouseMoveIntent {
            x: 1,
            y: 0,
            fast: true,
        };
        for _ in 0..120 {
            motion.step(intent, MOUSE_MOVE_TICK);
        }
        let (dx, _) = motion.step(intent, Duration::from_secs(2)).unwrap();
        assert!(dx <= 1400.0 / 60.0, "one late tick jumped {dx} pixels");
    }
}
