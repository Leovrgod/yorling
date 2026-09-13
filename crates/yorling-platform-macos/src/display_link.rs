//! Display-paced input without depending on the WebView/main run loop.
//! CVDisplayLink supports the app's older macOS deployment target. Its newer
//! NSScreen replacement requires a main-run-loop integration and macOS 14+.
#![allow(deprecated)]

use std::{
    ffi::c_void,
    ptr::NonNull,
    sync::Arc,
    time::{Duration, Instant},
};

use objc2_core_foundation::CFRetained;
use objc2_core_video::{CVDisplayLink, CVOptionFlags, CVReturn, CVTimeStamp, CVTimeStampFlags};

use crate::mouse_motion::MotionShared;

#[derive(Clone, Copy, Debug)]
pub(super) struct DisplayFrame {
    pub at: Instant,
    pub period: Duration,
    // Video time is independent of callback/worker scheduling jitter.
    pub video_time: Option<Duration>,
}

pub(super) fn refresh_period(value: i64, scale: i32) -> Option<Duration> {
    if value <= 0 || scale <= 0 {
        return None;
    }
    let seconds = value as f64 / f64::from(scale);
    // Include low refresh and high refresh displays, reject invalid timestamps.
    (1.0 / 1000.0..=0.05)
        .contains(&seconds)
        .then(|| Duration::from_secs_f64(seconds))
}

struct CallbackContext(Arc<MotionShared>);

pub(super) struct DisplayLink {
    link: CFRetained<CVDisplayLink>,
    // Stable allocation; released after stopping and releasing the native link.
    _context: Box<CallbackContext>,
    display: u32,
}

impl DisplayLink {
    pub fn start(display: u32, shared: Arc<MotionShared>) -> Result<Self, i32> {
        let mut raw = std::ptr::null_mut();
        // SAFETY: valid out pointer; ownership is transferred to CFRetained.
        let status =
            unsafe { CVDisplayLink::create_with_active_cg_displays(NonNull::from(&mut raw)) };
        if status != 0 {
            return Err(status);
        }
        let raw = NonNull::new(raw).ok_or(-1)?;
        let mut this = Self {
            link: unsafe { CFRetained::from_raw(raw) },
            _context: Box::new(CallbackContext(shared)),
            display,
        };
        let status = this.link.set_current_cg_display(display);
        if status != 0 {
            return Err(status);
        }
        // SAFETY: context stays at a stable address until the link has stopped
        // and been released. The callback never posts events or invokes AppKit.
        let status = unsafe {
            this.link.set_output_callback(
                Some(on_frame),
                (&mut *this._context as *mut CallbackContext).cast(),
            )
        };
        if status != 0 {
            return Err(status);
        }
        let status = this.link.start();
        if status != 0 {
            return Err(status);
        }
        let period = this.link.nominal_output_video_refresh_period();
        if let Some(period) = refresh_period(period.timeValue, period.timeScale) {
            log::debug!(
                "Mouse frame clock: display {display}, {:.3} Hz",
                1.0 / period.as_secs_f64()
            );
        }
        Ok(this)
    }

    pub fn display(&self) -> u32 {
        self.display
    }

    #[cfg(test)]
    pub fn nominal_period(&self) -> Option<Duration> {
        let period = self.link.nominal_output_video_refresh_period();
        refresh_period(period.timeValue, period.timeScale)
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        // Called only on the worker, outside the input mutex. Stop waits for
        // outstanding callbacks; field order releases link before its context.
        self.link.stop();
    }
}

unsafe extern "C-unwind" fn on_frame(
    _link: NonNull<CVDisplayLink>,
    _now: NonNull<CVTimeStamp>,
    output: NonNull<CVTimeStamp>,
    _flags_in: CVOptionFlags,
    _flags_out: NonNull<CVOptionFlags>,
    context: *mut c_void,
) -> CVReturn {
    // SAFETY: CoreVideo owns the timestamps for this call; DisplayLink owns
    // the boxed context for the entire callback registration lifetime.
    let output = unsafe { output.as_ref() };
    let context = unsafe { &*context.cast::<CallbackContext>() };
    let flags = CVTimeStampFlags::from_bits_retain(output.flags);
    if flags.contains(CVTimeStampFlags::VideoRefreshPeriodValid) {
        if let Some(period) = refresh_period(output.videoRefreshPeriod, output.videoTimeScale) {
            let video_time = (flags.contains(CVTimeStampFlags::VideoTimeValid)
                && output.videoTime >= 0)
                .then(|| {
                    Duration::from_secs_f64(
                        output.videoTime as f64 / f64::from(output.videoTimeScale),
                    )
                });
            context.0.publish_frame(DisplayFrame {
                at: Instant::now(),
                period,
                video_time,
            });
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_period_accepts_fractional_and_low_rates_but_rejects_invalid_values() {
        assert_eq!(
            refresh_period(1001, 60000),
            Some(Duration::from_secs_f64(1001.0 / 60000.0))
        );
        assert_eq!(
            refresh_period(1, 30),
            Some(Duration::from_secs_f64(1.0 / 30.0))
        );
        for (value, scale) in [(0, 60), (1, 0), (-1, 60), (1, -60), (1, 1)] {
            assert_eq!(refresh_period(value, scale), None);
        }
    }
}
