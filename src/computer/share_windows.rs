//! Persistent, exact-window Windows Graphics Capture sharing.
//!
//! The capture item is created programmatically from the requested HWND after
//! validating its PID. Frames arrive on `windows-capture`'s free-threaded WGC
//! session and are copied into a capacity-one RGBA slot. There is deliberately
//! no monitor capture, title lookup, window fallback, global cursor capture, or
//! request to suppress Windows' normal capture border.

use std::error::Error;
use std::ffi::c_void;
use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use image::RgbaImage;
use windows_capture::capture::{
    CaptureControl, Context, GraphicsCaptureApiError, GraphicsCaptureApiHandler,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{
    Error as GraphicsCaptureError, InternalCaptureControl,
};
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use super::{ComputerError, WindowDescriptor};

const MIN_SHARE_FPS: u64 = 1;
const MAX_SHARE_FPS: u64 = 10;
const BYTES_PER_RGBA_PIXEL: u32 = 4;
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct NativeShareFrame {
    pub(crate) image: RgbaImage,
    pub(crate) source_sequence: u64,
    pub(crate) captured_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeShareMetadata {
    pub(crate) backend: &'static str,
    pub(crate) native_stream: bool,
    pub(crate) system_indicator: bool,
    pub(crate) selection_mode: &'static str,
}

#[derive(Debug, Default)]
struct FrameSlot {
    latest: Option<NativeShareFrame>,
    source_sequence: u64,
    terminal_error: Option<ComputerError>,
}

#[derive(Debug)]
struct SharedCaptureState {
    target_id: String,
    target_pid: u32,
    slot: Mutex<FrameSlot>,
    changed: Condvar,
    dropped_frames: AtomicU64,
}

impl SharedCaptureState {
    fn new(target_id: String, target_pid: u32) -> Self {
        Self {
            target_id,
            target_pid,
            slot: Mutex::new(FrameSlot::default()),
            changed: Condvar::new(),
            dropped_frames: AtomicU64::new(0),
        }
    }

    fn lock_for_handler(&self) -> Result<MutexGuard<'_, FrameSlot>, CaptureHandlerError> {
        self.slot.lock().map_err(|_| {
            CaptureHandlerError::new(format!(
                "native capture state for HWND {} (PID {}) was poisoned",
                self.target_id, self.target_pid
            ))
        })
    }

    fn lock_for_client(&self) -> Result<MutexGuard<'_, FrameSlot>, ComputerError> {
        self.slot.lock().map_err(|_| {
            capture_error(format!(
                "Native capture state for HWND {} (PID {}) was poisoned",
                self.target_id, self.target_pid
            ))
        })
    }

    fn publish(&self, image: RgbaImage, captured_at: Instant) -> Result<(), CaptureHandlerError> {
        let mut slot = self.lock_for_handler()?;
        if slot.terminal_error.is_some() {
            return Ok(());
        }

        let Some(source_sequence) = slot.source_sequence.checked_add(1) else {
            let message = "Native capture source sequence exhausted its numeric range";
            slot.terminal_error = Some(capture_error(message));
            slot.latest = None;
            drop(slot);
            self.changed.notify_all();
            return Err(CaptureHandlerError::new(message));
        };
        slot.source_sequence = source_sequence;
        let replaced = slot
            .latest
            .replace(NativeShareFrame {
                image,
                source_sequence,
                captured_at,
            })
            .is_some();
        drop(slot);
        self.changed.notify_all();

        if replaced {
            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    fn mark_closed(&self) -> Result<(), CaptureHandlerError> {
        let mut slot = self.lock_for_handler()?;
        if slot.terminal_error.is_none() {
            slot.terminal_error = Some(capture_error(format!(
                "The exact target HWND {} (PID {}) closed",
                self.target_id, self.target_pid
            )));
            slot.latest = None;
        }
        drop(slot);
        self.changed.notify_all();
        Ok(())
    }

    fn mark_failed(&self, message: impl Into<String>) {
        if let Ok(mut slot) = self.slot.lock()
            && slot.terminal_error.is_none()
        {
            slot.terminal_error = Some(capture_error(message));
            slot.latest = None;
        }
        self.changed.notify_all();
    }

    fn clear_latest(&self) -> Result<(), ComputerError> {
        let mut slot = self.lock_for_client()?;
        slot.latest = None;
        Ok(())
    }

    fn wait_for_first_frame(&self, timeout: Duration) -> Result<(), ComputerError> {
        let slot = self.lock_for_client()?;
        let (slot, wait_result) = self
            .changed
            .wait_timeout_while(slot, timeout, |slot| {
                slot.latest.is_none() && slot.terminal_error.is_none()
            })
            .map_err(|_| {
                capture_error(format!(
                    "Native capture state for HWND {} (PID {}) was poisoned while waiting for the first frame",
                    self.target_id, self.target_pid
                ))
            })?;
        if let Some(error) = slot.terminal_error.clone() {
            return Err(error);
        }
        if slot.latest.is_some() {
            return Ok(());
        }

        let detail = if wait_result.timed_out() {
            "timed out waiting for the first complete frame"
        } else {
            "ended before producing the first complete frame"
        };
        Err(capture_error(format!(
            "Windows Graphics Capture {detail} for HWND {} (PID {}); the window may contain protected content or no longer be shareable",
            self.target_id, self.target_pid
        )))
    }

    fn take_after(
        &self,
        last_source_sequence: u64,
    ) -> Result<Option<NativeShareFrame>, ComputerError> {
        let mut slot = self.lock_for_client()?;
        if let Some(error) = slot.terminal_error.clone() {
            return Err(error);
        }
        if let Some(frame) = slot.latest.take()
            && frame.source_sequence > last_source_sequence
        {
            return Ok(Some(frame));
        }
        Ok(None)
    }

    fn dropped_frames(&self) -> u64 {
        self.dropped_frames.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
struct CaptureFlags {
    state: Arc<SharedCaptureState>,
    target_id: String,
    target_pid: u32,
    minimum_interval: Duration,
}

#[derive(Debug)]
struct CaptureHandler {
    state: Arc<SharedCaptureState>,
    target_id: String,
    target_pid: u32,
    minimum_interval: Duration,
    last_published_at: Option<Instant>,
}

#[derive(Debug)]
struct CaptureHandlerError(String);

impl CaptureHandlerError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for CaptureHandlerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CaptureHandlerError {}

impl CaptureHandler {
    fn fail<T>(&self, message: impl Into<String>) -> Result<T, CaptureHandlerError> {
        let message = message.into();
        self.state.mark_failed(message.clone());
        Err(CaptureHandlerError::new(message))
    }
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = CaptureFlags;
    type Error = CaptureHandlerError;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let flags = context.flags;
        Ok(Self {
            state: flags.state,
            target_id: flags.target_id,
            target_pid: flags.target_pid,
            minimum_interval: flags.minimum_interval,
            last_published_at: None,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if let Err(error) = exact_window(&self.target_id, self.target_pid) {
            return self.fail(error.message);
        }

        let captured_at = Instant::now();
        if self
            .last_published_at
            .is_some_and(|last| captured_at.saturating_duration_since(last) < self.minimum_interval)
        {
            return Ok(());
        }

        let mut buffer = match frame.buffer() {
            Ok(buffer) => buffer,
            Err(error) => {
                return self.fail(format!("Failed to map the WGC frame: {error}"));
            }
        };
        let width = buffer.width();
        let height = buffer.height();
        let row_pitch = buffer.row_pitch();
        let image = match copy_rgba_rows(buffer.as_raw_buffer(), width, height, row_pitch) {
            Ok(image) => image,
            Err(error) => return self.fail(error),
        };

        self.state.publish(image, captured_at)?;
        self.last_published_at = Some(captured_at);
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.state.mark_closed()
    }
}

type NativeCaptureControl = CaptureControl<CaptureHandler, CaptureHandlerError>;

pub(crate) struct NativeShareCapture {
    control: Option<NativeCaptureControl>,
    state: Arc<SharedCaptureState>,
    window_id: String,
    fps: u64,
}

impl NativeShareCapture {
    pub(crate) fn start(target: &WindowDescriptor, fps: u64) -> Result<Self, ComputerError> {
        let minimum_interval = share_interval(fps)?;
        if target.minimized || target.width == 0 || target.height == 0 {
            return Err(no_window_error(
                "The requested exact window is minimized or has no capturable area",
            ));
        }
        let window = exact_window(&target.id, target.pid)?;
        let state = Arc::new(SharedCaptureState::new(target.id.clone(), target.pid));
        let control = match start_control(
            window,
            Arc::clone(&state),
            target,
            minimum_interval,
            MinimumUpdateIntervalSettings::Custom(minimum_interval),
        ) {
            Ok(control) => control,
            Err(GraphicsCaptureApiError::GraphicsCaptureApiError(
                GraphicsCaptureError::MinimumUpdateIntervalUnsupported,
            )) => start_control(
                // Older WGC builds lack MinUpdateInterval. Keep their native
                // cadence and enforce the requested maximum in the callback.
                exact_window(&target.id, target.pid)?,
                Arc::clone(&state),
                target,
                minimum_interval,
                MinimumUpdateIntervalSettings::Default,
            )
            .map_err(|error| start_error(target, error))?,
            Err(error) => return Err(start_error(target, error)),
        };

        if let Err(error) = exact_window(&target.id, target.pid) {
            let _ = control.stop();
            return Err(error);
        }
        if let Err(error) = state.wait_for_first_frame(FIRST_FRAME_TIMEOUT) {
            let _ = control.stop();
            return Err(error);
        }

        Ok(Self {
            control: Some(control),
            state,
            window_id: target.id.clone(),
            fps,
        })
    }

    pub(crate) fn latest_after(
        &self,
        last_source_sequence: u64,
    ) -> Result<Option<NativeShareFrame>, ComputerError> {
        let latest = self.state.take_after(last_source_sequence)?;
        if latest.is_none()
            && self
                .control
                .as_ref()
                .is_some_and(NativeCaptureControl::is_finished)
        {
            return Err(capture_error(format!(
                "The WGC capture thread for HWND {} (PID {}) ended unexpectedly",
                self.state.target_id, self.state.target_pid
            )));
        }
        Ok(latest)
    }

    pub(crate) fn stop(&mut self) -> Result<(), ComputerError> {
        let Some(control) = self.control.take() else {
            return Ok(());
        };

        if let Err(error) = control.stop() {
            let message = format!(
                "Failed to stop the WGC capture thread for HWND {} (PID {}): {error}",
                self.state.target_id, self.state.target_pid
            );
            self.state.mark_failed(message.clone());
            return Err(capture_error(message));
        }
        self.state.clear_latest()
    }

    pub(crate) const fn metadata(&self) -> NativeShareMetadata {
        NativeShareMetadata {
            backend: "windows-graphics-capture",
            native_stream: true,
            system_indicator: true,
            selection_mode: "programmatic-exact-window",
        }
    }

    pub(crate) fn dropped_frames(&self) -> u64 {
        self.state.dropped_frames()
    }
}

impl Drop for NativeShareCapture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl fmt::Debug for NativeShareCapture {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeShareCapture")
            .field("window_id", &self.window_id)
            .field("fps", &self.fps)
            .field("metadata", &self.metadata())
            .field("dropped_frames", &self.dropped_frames())
            .finish_non_exhaustive()
    }
}

fn start_control(
    window: Window,
    state: Arc<SharedCaptureState>,
    target: &WindowDescriptor,
    minimum_interval: Duration,
    native_update_interval: MinimumUpdateIntervalSettings,
) -> Result<NativeCaptureControl, GraphicsCaptureApiError<CaptureHandlerError>> {
    let flags = CaptureFlags {
        state,
        target_id: target.id.clone(),
        target_pid: target.pid,
        minimum_interval,
    };
    let settings = Settings::new(
        window,
        // Cursor suppression is mandatory so the bridge can composite its
        // app-scoped virtual pointer. The crate fails startup when this
        // property is unavailable instead of silently capturing the user cursor.
        CursorCaptureSettings::WithoutCursor,
        // Never request `WithoutBorder`: Windows owns the visible capture
        // indication and its consent semantics.
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        native_update_interval,
        DirtyRegionSettings::Default,
        ColorFormat::Rgba8,
        flags,
    );
    CaptureHandler::start_free_threaded(settings)
}

fn start_error(
    target: &WindowDescriptor,
    error: GraphicsCaptureApiError<CaptureHandlerError>,
) -> ComputerError {
    capture_error(format!(
        "Failed to start exact-window Windows Graphics Capture for HWND {} (PID {}): {error}",
        target.id, target.pid
    ))
}

fn exact_window(target_id: &str, target_pid: u32) -> Result<Window, ComputerError> {
    let hwnd = target_id.parse::<u32>().map_err(|_| {
        no_window_error(format!(
            "The exact target window ID {target_id:?} is not a valid HWND"
        ))
    })?;
    if hwnd == 0 {
        return Err(no_window_error("The exact target HWND is null"));
    }

    let window = Window::from_raw_hwnd(hwnd as usize as *mut c_void);
    let actual_pid = window.process_id().map_err(|error| {
        no_window_error(format!(
            "The exact target HWND {target_id} no longer exists: {error}"
        ))
    })?;
    if actual_pid != target_pid {
        return Err(no_window_error(format!(
            "The exact target HWND {target_id} changed owner from PID {target_pid} to PID {actual_pid}"
        )));
    }
    if !window.is_valid() {
        return Err(no_window_error(format!(
            "The exact target HWND {target_id} (PID {target_pid}) is not capturable"
        )));
    }
    Ok(window)
}

fn share_interval(fps: u64) -> Result<Duration, ComputerError> {
    if !(MIN_SHARE_FPS..=MAX_SHARE_FPS).contains(&fps) {
        return Err(capture_error(format!(
            "Native Windows sharing requires an FPS from {MIN_SHARE_FPS} through {MAX_SHARE_FPS}"
        )));
    }
    Ok(Duration::from_nanos(1_000_000_000 / fps))
}

fn copy_rgba_rows(
    raw: &[u8],
    width: u32,
    height: u32,
    row_pitch: u32,
) -> Result<RgbaImage, String> {
    if width == 0 || height == 0 {
        return Err("WGC returned an empty frame".to_string());
    }

    let packed_row_u32 = width
        .checked_mul(BYTES_PER_RGBA_PIXEL)
        .ok_or_else(|| "WGC frame row size overflowed".to_string())?;
    if row_pitch < packed_row_u32 {
        return Err(format!(
            "WGC frame row pitch {row_pitch} is smaller than packed RGBA width {packed_row_u32}"
        ));
    }

    let packed_row = usize::try_from(packed_row_u32)
        .map_err(|_| "WGC packed row size does not fit memory indexing".to_string())?;
    let stride = usize::try_from(row_pitch)
        .map_err(|_| "WGC row pitch does not fit memory indexing".to_string())?;
    let rows = usize::try_from(height)
        .map_err(|_| "WGC frame height does not fit memory indexing".to_string())?;
    let required_source_len = rows
        .checked_sub(1)
        .and_then(|last_row| last_row.checked_mul(stride))
        .and_then(|last_offset| last_offset.checked_add(packed_row))
        .ok_or_else(|| "WGC source buffer size overflowed".to_string())?;
    if raw.len() < required_source_len {
        return Err(format!(
            "WGC frame buffer is truncated: expected at least {required_source_len} bytes, got {}",
            raw.len()
        ));
    }

    let packed_len = packed_row
        .checked_mul(rows)
        .ok_or_else(|| "WGC packed frame size overflowed".to_string())?;
    let mut packed = vec![0_u8; packed_len];
    for row in 0..rows {
        let source_start = row * stride;
        let destination_start = row * packed_row;
        packed[destination_start..destination_start + packed_row]
            .copy_from_slice(&raw[source_start..source_start + packed_row]);
    }

    RgbaImage::from_raw(width, height, packed)
        .ok_or_else(|| "WGC packed RGBA dimensions were inconsistent".to_string())
}

fn no_window_error(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_NO_WINDOW", message)
}

fn capture_error(message: impl Into<String>) -> ComputerError {
    ComputerError::new("COMPUTER_CAPTURE_FAILED", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_rgba_rows_removes_stride_padding() {
        let raw = [
            1, 2, 3, 4, 5, 6, 7, 8, 90, 91, 92, 93, 9, 10, 11, 12, 13, 14, 15, 16, 94, 95, 96, 97,
        ];

        let image = copy_rgba_rows(&raw, 2, 2, 12).expect("padded frame should copy");

        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(
            image.as_raw(),
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn copy_rgba_rows_rejects_invalid_stride_and_truncation() {
        assert!(copy_rgba_rows(&[0; 8], 2, 1, 7).is_err());
        assert!(copy_rgba_rows(&[0; 15], 2, 2, 8).is_err());
        assert!(copy_rgba_rows(&[], 0, 1, 0).is_err());
    }

    #[test]
    fn capacity_one_state_replaces_old_frames_and_counts_drop() {
        let state = SharedCaptureState::new("42".to_string(), 7);
        let first_capture = Instant::now();
        state
            .publish(RgbaImage::new(1, 1), first_capture)
            .expect("first publish should succeed");
        state
            .publish(
                RgbaImage::new(2, 1),
                first_capture + Duration::from_millis(100),
            )
            .expect("second publish should succeed");

        let frame = state
            .take_after(0)
            .expect("state should be readable")
            .expect("latest frame should exist");
        assert_eq!(frame.source_sequence, 2);
        assert_eq!(frame.image.width(), 2);
        assert_eq!(state.dropped_frames(), 1);
        assert!(
            state
                .take_after(2)
                .expect("state should be readable")
                .is_none()
        );
    }

    #[test]
    fn close_discards_buffered_frame_and_fails_closed() {
        let state = SharedCaptureState::new("42".to_string(), 7);
        state
            .publish(RgbaImage::new(1, 1), Instant::now())
            .expect("publish should succeed");
        state.mark_closed().expect("close should succeed");

        let error = state
            .take_after(0)
            .expect_err("closed target should become terminal");
        assert_eq!(error.code, "COMPUTER_CAPTURE_FAILED");
        assert!(error.message.contains("HWND 42"));
        assert!(error.message.contains("PID 7"));
    }

    #[test]
    fn share_interval_enforces_contract_bounds() {
        assert!(share_interval(0).is_err());
        assert_eq!(
            share_interval(1).expect("1 FPS should work"),
            Duration::from_secs(1)
        );
        assert_eq!(
            share_interval(10).expect("10 FPS should work"),
            Duration::from_millis(100)
        );
        assert!(share_interval(11).is_err());
    }
}
