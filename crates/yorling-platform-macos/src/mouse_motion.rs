//! Keyboard-driven cursor motion. Input updates replace one shared intent;
//! they never enqueue frames for the worker to replay later.

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, ns_string};

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
    fn step(&mut self, intent: MouseMoveIntent, dt: Duration) -> Option<(f64, f64)> {
        if intent.is_idle() {
            *self = Self::default();
            return None;
        }
        let dt_secs = dt.min(MAX_FRAME_TIME).as_secs_f64();
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
struct MotionShared {
    request: Mutex<MotionRequest>,
    wake: Condvar,
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
}

#[derive(Default)]
struct NativeMouseOutput {
    cursor: CursorMotionState,
    displays: Vec<DisplayBounds>,
    displays_updated: Option<Instant>,
}

impl MouseOutput for NativeMouseOutput {
    fn reset(&mut self) {
        self.cursor = CursorMotionState::default();
        self.displays_updated = None;
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

fn run_mouse_worker(shared: Arc<MotionShared>, mut output: impl MouseOutput) {
    set_input_thread_priority();
    let mut activity = None;
    let mut motion = SmoothMouseMotion::default();
    let mut generation = None;
    let mut last_tick = Instant::now();
    let mut next_tick = last_tick;
    let mut last_stall_warning = None::<Instant>;

    loop {
        let mut request = shared.request.lock().unwrap();
        if !request.running {
            break;
        }
        if request.intent.is_idle() {
            // End the activity outside the input mutex: Foundation may do IPC.
            drop(request);
            activity.take();
            request = shared.request.lock().unwrap();
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
        let elapsed = now.saturating_duration_since(last_tick);
        let stalled = elapsed > RESET_AFTER_STALL;
        if restarted || stalled {
            motion = SmoothMouseMotion::default();
            output.reset();
            last_tick = now - MOUSE_MOVE_TICK;
            next_tick = now;
            generation = Some(request.generation);
            if stalled
                && !restarted
                && last_stall_warning
                    .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(10))
            {
                log::warn!(
                    "Mouse motion scheduling gap: {} ms; discarded stale movement",
                    elapsed.as_millis()
                );
                last_stall_warning = Some(now);
            }
        }
        if now < next_tick {
            // Releases the mutex while sleeping; a key release or shutdown wakes
            // us immediately. Direction changes retain the same frame deadline.
            let _ = shared.wake.wait_timeout(request, next_tick - now).unwrap();
            continue;
        }
        let intent = request.intent;
        drop(request);

        activity.get_or_insert_with(InputActivity::new);
        // Beginning a system activity can take time. Discard this frame if a
        // release, reversal or shutdown arrived while Foundation was working.
        {
            let latest = shared.request.lock().unwrap();
            if !latest.running || latest.intent != intent || Some(latest.generation) != generation {
                continue;
            }
        }
        if let Some((dx, dy)) = motion.step(intent, now.saturating_duration_since(last_tick)) {
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
